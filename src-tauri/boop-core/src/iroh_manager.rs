use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use anyhow::{Result, Context};
use iroh::{Endpoint, SecretKey, protocol::Router, protocol::ProtocolHandler, endpoint::Connection, endpoint::presets, protocol::AcceptError};
use iroh_blobs::{ALPN as BLOBS_ALPN, BlobsProtocol, api::blobs::Blobs, store::fs::FsStore};
use iroh_docs::{ALPN as DOCS_ALPN, protocol::Docs};
use iroh_gossip::{ALPN as GOSSIP_ALPN, net::Gossip};
use tokio::io::AsyncWriteExt;
use iroh_docs::AuthorId;
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use n0_future::StreamExt;
use iroh_tickets::endpoint::EndpointTicket;
use hex;
use std::collections::HashMap;

pub const HANDSHAKE_ALPN: &[u8] = b"boop/handshk";
pub const PRESENCE_ALPN: &[u8] = b"boop/prsnc";
pub const WELCOME_ALPN: &[u8] = b"boop/wlcm";

pub struct IrohEvents {
	pub handshake_rx: mpsc::UnboundedReceiver<(iroh::PublicKey, String)>,
	pub presence_rx: mpsc::UnboundedReceiver<(iroh::PublicKey, bool)>,
	pub welcome_rx: mpsc::UnboundedReceiver<uuid::Uuid>, // Emits the new friend's ID
}

#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakePayload {
	pub sender_endpoint_id: iroh::PublicKey,
	pub doc_ticket: String,
}

#[derive(Clone, Debug)]
pub struct BoopHandshakeHandler {
	pub tx: mpsc::UnboundedSender<(iroh::PublicKey, String)>, // (endpoint_id, doc_ticket)
	pub address_book: Arc<Mutex<crate::address_book::AddressBook>>,
	pub connections: Arc<Mutex<HashMap<(iroh::PublicKey, Vec<u8>), Connection>>>,
}

impl ProtocolHandler for BoopHandshakeHandler {
	async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
		let remote_id = connection.remote_id();
		{
			let ab = self.address_book.lock().await;
			if !ab.friends.contains_key(&remote_id) {
				return Err(AcceptError::from_err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Unknown peer")));
			}
		}
		
		let tx = self.tx.clone();
		{
			let mut conns = self.connections.lock().await;
			conns.insert((remote_id, HANDSHAKE_ALPN.to_vec()), connection.clone());
		}

		while let Ok((mut send, mut recv)) = connection.accept_bi().await {
			let tx = tx.clone();
			tokio::spawn(async move {
				let Ok(buf) = recv.read_to_end(2048).await else { return; };
				let Ok(payload) = serde_json::from_slice::<HandshakePayload>(&buf) else { return; };
				
				// We got the payload, emit it to the app!
				let _ = tx.send((payload.sender_endpoint_id, payload.doc_ticket));
				
				// Acknowledge receipt
				let _ = send.write_all(&[1]).await;
				let _ = send.finish();
			});
		}
		
		Ok(())
	}
}

#[derive(Clone, Debug)]
pub struct BoopPresenceHandler {
	pub tx: mpsc::UnboundedSender<(iroh::PublicKey, bool)>,
	pub address_book: Arc<Mutex<crate::address_book::AddressBook>>,
	pub connections: Arc<Mutex<HashMap<(iroh::PublicKey, Vec<u8>), Connection>>>,
}

impl ProtocolHandler for BoopPresenceHandler {
	async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
		let remote_id = connection.remote_id();
		{
			let ab = self.address_book.lock().await;
			if !ab.friends.contains_key(&remote_id) {
				return Err(AcceptError::from_err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Unknown peer")));
			}
		}

		let tx = self.tx.clone();
		{
			let mut conns = self.connections.lock().await;
			conns.insert((remote_id, PRESENCE_ALPN.to_vec()), connection.clone());
		}

		while let Ok((mut send, mut recv)) = connection.accept_bi().await {
			let tx = tx.clone();
			tokio::spawn(async move {
				let mut buf = [0u8; 1];
				let Ok(_) = recv.read_exact(&mut buf).await else { return; };
				
				let is_active = buf[0] != 0;
				// Emit event
				let _ = tx.send((remote_id, is_active));
				
				// Acknowledge receipt
				let _ = send.write_all(&[1]).await;
				let _ = send.finish();
			});
		}
		
		Ok(())
	}
}

#[derive(Clone, Debug)]
pub struct BoopWelcomeHandler {
	pub tx: mpsc::UnboundedSender<uuid::Uuid>,
	pub address_book: Arc<Mutex<crate::address_book::AddressBook>>,
	pub connections: Arc<Mutex<HashMap<(iroh::PublicKey, Vec<u8>), Connection>>>,
}

impl ProtocolHandler for BoopWelcomeHandler {
	async fn accept(&self, connection: Connection) -> Result<(), AcceptError> {
		let remote_id = connection.remote_id();
		// We do NOT cache Welcome connections as they are ephemeral onboarding events
		let tx = self.tx.clone();
		let address_book = self.address_book.clone();

		while let Ok((mut send, mut recv)) = connection.accept_bi().await {
			let tx = tx.clone();
			let address_book = address_book.clone();
			tokio::spawn(async move {
				let mut token = [0u8; 32];
				let Ok(_) = recv.read_exact(&mut token).await else { return; };
				
				let token_hex = hex::encode(token);
				
				let friend_id = {
					let mut ab = address_book.lock().await;
					if let Some(invite) = ab.pending_invites.remove(&token_hex) {
						let pet_name = invite.pet_name;
						ab.add_friend(pet_name, remote_id)
					} else {
						log::error!("Invalid invite token received: {}", token_hex);
						return;
					}
				};
				
				// Send ACK
				let _ = send.write_all(&[1]).await;
				let _ = send.finish();
				
				let _ = tx.send(friend_id);
			});
		}
		
		Ok(())
	}
}

#[derive(Clone, Debug)]
pub struct IrohManager {
	#[allow(dead_code)]
	router: Router,
	store: FsStore,
	docs: Docs,
	author: AuthorId,
	pub endpoint: Endpoint,
	pub endpoint_id: iroh::PublicKey,
	connections: Arc<Mutex<HashMap<(iroh::PublicKey, Vec<u8>), Connection>>>,
}

impl IrohManager {
	pub async fn new(path: PathBuf, local_only: bool, address_book: Arc<Mutex<crate::address_book::AddressBook>>) -> Result<(Self, IrohEvents)> {
		tokio::fs::create_dir_all(&path).await?;
		let key = Self::load_secret_key(path.clone().join("keypair")).await?;

		// 1. Create Endpoint
		let endpoint_id = key.public();
		let endpoint = if local_only {
			use iroh::RelayMode;
			
			Endpoint::builder(presets::N0)
				.relay_mode(RelayMode::Disabled)
				// .bind_addr("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap()).unwrap()
				.secret_key(key.clone())
				.bind()
				.await?
		} else {
			Endpoint::builder(presets::N0).secret_key(key.clone()).bind().await?
		};

		// 2. Load Blobs FsStore
		let blobs = FsStore::load(&path).await?;
		
		let gossip = Gossip::builder().spawn(endpoint.clone());

		let docs = Docs::persistent(path)
			.spawn(endpoint.clone(), (*blobs).clone(), gossip.clone())
			.await?;

		let connections: Arc<Mutex<HashMap<(iroh::PublicKey, Vec<u8>), Connection>>> = Arc::new(Mutex::new(HashMap::new()));

		let (tx, handshake_rx) = mpsc::unbounded_channel();
		let handshake_handler = BoopHandshakeHandler { tx, address_book: address_book.clone(), connections: connections.clone() };

		let (presence_tx, presence_rx) = mpsc::unbounded_channel();
		let presence_handler = BoopPresenceHandler { tx: presence_tx, address_book: address_book.clone(), connections: connections.clone() };

		let (welcome_tx, welcome_rx) = mpsc::unbounded_channel();
		let welcome_handler = BoopWelcomeHandler { tx: welcome_tx, address_book: address_book.clone(), connections: connections.clone() };

		let builder = Router::builder(endpoint.clone());
		let router = builder
			.accept(BLOBS_ALPN, BlobsProtocol::new(&blobs, None))
			.accept(DOCS_ALPN, docs.clone())
			.accept(GOSSIP_ALPN, gossip)
			.accept(HANDSHAKE_ALPN, std::sync::Arc::new(handshake_handler))
			.accept(PRESENCE_ALPN, std::sync::Arc::new(presence_handler))
			.accept(WELCOME_ALPN, std::sync::Arc::new(welcome_handler))
			.spawn();

		let authors: Vec<Result<AuthorId>> = docs.author_list().await?.collect().await;
		let author = if let Some(Ok(a)) = authors.into_iter().next() {
			a
		} else {
			docs.author_create().await?
		};
		log::info!("Using AuthorId: {}", author);

		let manager = Self {
			router,
			docs,
			store: blobs,
			author,
			endpoint,
			endpoint_id,
			connections,
		};

		Ok((manager, IrohEvents { handshake_rx, presence_rx, welcome_rx }))
	}

	pub async fn get_or_connect(&self, addr: impl Into<iroh::EndpointAddr>, alpn: &[u8]) -> Result<Connection> {
		let addr = addr.into();
		let node_id = addr.id;
		let key = (node_id, alpn.to_vec());
		
		let mut conns = self.connections.lock().await;
		if let Some(conn) = conns.get(&key) {
			return Ok(conn.clone());
		}
		
		let conn = self.endpoint.connect(addr, alpn).await?;
		conns.insert(key.clone(), conn.clone());
		
		// Spawn a background task to clean up when the connection is closed
		let conns_clone = self.connections.clone();
		let key_clone = key.clone();
		let conn_for_closed = conn.clone();
		tokio::spawn(async move {
			let _ = conn_for_closed.closed().await;
			let mut conns = conns_clone.lock().await;
			if let Some(existing) = conns.get(&key_clone) {
				// Only remove if it's the exact same connection (comparing handles)
				if existing.stable_id() == conn_for_closed.stable_id() {
					conns.remove(&key_clone);
				}
			}
		});
		
		Ok(conn)
	}

	pub fn blobs(&self) -> &Blobs {
		self.store.blobs()
	}

	pub fn docs(&self) -> &Docs {
		&self.docs
	}

	pub fn author(&self) -> AuthorId {
		self.author
	}
	
	pub fn endpoint(&self) -> &Endpoint {
		&self.endpoint
	}
	
	pub fn endpoint_ticket(&self) -> Result<EndpointTicket> {
		Ok(EndpointTicket::new(self.endpoint.addr()))
	}

	pub async fn connect_to_endpoint_ticket(&self, ticket: &EndpointTicket) -> Result<()> {
		self.get_or_connect(ticket.endpoint_addr().clone(), PRESENCE_ALPN).await?;
		Ok(())
	}

	pub async fn dial_welcome(&self, addr: impl Into<iroh::EndpointAddr>, token: [u8; 32]) -> Result<()> {
		let addr = addr.into();
		// Welcome is ephemeral, don't use the connection pool
		let connection = self.endpoint.connect(addr, WELCOME_ALPN).await?;
		let (mut send, mut recv) = connection.open_bi().await?;
		
		send.write_all(&token).await?;
		send.finish()?;
		
		let mut ack = [0u8; 1];
		recv.read_exact(&mut ack).await?;
		if ack[0] != 1 {
			connection.close(1u32.into(), b"Welcome failed");
			anyhow::bail!("Welcome handshake failed");
		}
		
		connection.close(0u32.into(), b"Welcome complete");
		Ok(())
	}
	
	pub async fn dial_friend(&self, addr: impl Into<iroh::EndpointAddr>, doc_ticket: String) -> Result<()> {
		let addr = addr.into();
		let connection = self.get_or_connect(addr, HANDSHAKE_ALPN).await?;
		let (mut send, mut recv) = connection.open_bi().await?;
		
		let my_id = self.endpoint_id;
		
		let payload = HandshakePayload {
			sender_endpoint_id: my_id,
			doc_ticket,
		};
		
		let bytes = serde_json::to_vec(&payload)?;
		send.write_all(&bytes).await?;
		send.finish()?;
		
		// Wait for ACK
		let mut ack = [0u8; 1];
		let _ = recv.read_exact(&mut ack).await;
		
		Ok(())
	}

	async fn load_secret_key(key_path: PathBuf) -> Result<SecretKey> {
		if key_path.exists() {
			let key_bytes = tokio::fs::read(key_path).await?;
			let secret_key = SecretKey::try_from(&key_bytes[0..32])?;
			Ok(secret_key)
		} else {
			let secret_key = SecretKey::generate();
			
			let key_path_parent = key_path.parent().expect("must have parent");
			tokio::fs::create_dir_all(&key_path_parent).await?;
			let (file, temp_file_path) = tempfile::NamedTempFile::new_in(key_path_parent)?
				.into_parts();
			let mut file = tokio::fs::File::from_std(file);
			file.write_all(&secret_key.to_bytes()).await?;
			file.flush().await?;
			drop(file);
			tokio::fs::rename(temp_file_path, key_path).await?;
			
			Ok(secret_key)
		}
	}

	pub async fn fetch_blob(&self, hash_str: &str, endpoint_id_str: &str) -> anyhow::Result<()> {
		use iroh_blobs::Hash;
		use std::str::FromStr;
		
		log::info!("Starting Explicit Downloader for blob: {}", hash_str);
		
		let hash = Hash::from_str(hash_str)?;
		let endpoint_id: iroh::PublicKey = endpoint_id_str.parse().context("Invalid EndpointId")?;
		
		let downloader = iroh_blobs::api::downloader::Downloader::new(&self.store, &self.endpoint);
		downloader.download(hash, Some(endpoint_id)).await.context("Blob download failed")?;
		
		log::info!("Explicit Downloader completed blob fetch!");
		
		Ok(())
	}

	pub async fn send_presence(&self, addr: impl Into<iroh::EndpointAddr>, is_active: bool) -> Result<()> {
		let addr = addr.into();
		let connection = self.get_or_connect(addr, PRESENCE_ALPN).await?;
		let (mut send, mut recv) = connection.open_bi().await?;
		
		let val = if is_active { 1u8 } else { 0u8 };
		send.write_all(&[val]).await?;
		send.finish()?;
		
		let mut ack = [0u8; 1];
		recv.read_exact(&mut ack).await?;
		Ok(())
	}
}
