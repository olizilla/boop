use std::path::PathBuf;
use anyhow::{Result, Context};
use iroh::{Endpoint, SecretKey, protocol::Router, protocol::ProtocolHandler, endpoint::Connection, endpoint::presets, protocol::AcceptError};
use iroh_blobs::{ALPN as BLOBS_ALPN, BlobsProtocol, api::blobs::Blobs, store::fs::FsStore};
use iroh_docs::{ALPN as DOCS_ALPN, protocol::Docs};
use iroh_gossip::{ALPN as GOSSIP_ALPN, net::Gossip};
use tokio::io::AsyncWriteExt;
use iroh_docs::AuthorId;
use std::future::Future; 
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use n0_future::StreamExt;

pub const HANDSHAKE_ALPN: &[u8] = b"boop/handshk";

#[derive(Serialize, Deserialize, Debug)]
pub struct HandshakePayload {
	pub sender_endpoint_id: String,
	pub doc_ticket: String,
}

#[derive(Clone, Debug)]
pub struct BoopHandshakeHandler {
	pub tx: mpsc::UnboundedSender<(String, String)>, // (endpoint_id, doc_ticket)
}

impl ProtocolHandler for BoopHandshakeHandler {
	fn accept(&self, connection: Connection) -> impl Future<Output = Result<(), AcceptError>> + Send {
		let tx = self.tx.clone();
		async move {
			if let Ok(mut recv) = connection.accept_uni().await {
				if let Ok(buf) = recv.read_to_end(2048).await {
					if let Ok(payload) = serde_json::from_slice::<HandshakePayload>(&buf) {
						// We got the payload, emit it to the app!
						let _ = tx.send((payload.sender_endpoint_id, payload.doc_ticket));
					}
				}
			}
			Ok(())
		}
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
}

impl IrohManager {
	pub async fn new(path: PathBuf, local_only: bool) -> Result<(Self, mpsc::UnboundedReceiver<(String, String)>)> {
		tokio::fs::create_dir_all(&path).await?;
		let key = Self::load_secret_key(path.clone().join("keypair")).await?;

		// 1. Create Endpoint
		let endpoint_id = key.public();
		let endpoint = if local_only {
			use iroh::{address_lookup::MdnsAddressLookup, RelayMode};
			
			let builder = Endpoint::empty_builder()
				.relay_mode(RelayMode::Disabled);
				
			let mdns = MdnsAddressLookup::builder().build(endpoint_id).unwrap();
			builder.address_lookup(mdns).secret_key(key.clone()).bind().await?
		} else {
			Endpoint::builder(presets::N0).secret_key(key.clone()).bind().await?
		};

		// 2. Load Blobs FsStore
		let blobs = FsStore::load(&path).await?;
		
		let gossip = Gossip::builder().spawn(endpoint.clone());

		let docs = Docs::persistent(path)
			.spawn(endpoint.clone(), (*blobs).clone(), gossip.clone())
			.await?;

		let (tx, rx) = mpsc::unbounded_channel();
		let handshake_handler = BoopHandshakeHandler { tx };

		let builder = Router::builder(endpoint.clone());
		let router = builder
			.accept(BLOBS_ALPN, BlobsProtocol::new(&blobs, None))
			.accept(DOCS_ALPN, docs.clone())
			.accept(GOSSIP_ALPN, gossip)
			.accept(HANDSHAKE_ALPN, std::sync::Arc::new(handshake_handler))
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
		};

		Ok((manager, rx))
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
	
	pub async fn dial_friend(&self, addr: impl Into<iroh::EndpointAddr>, doc_ticket: String) -> Result<()> {
		let addr = addr.into();
		let connection = self.endpoint.connect(addr, HANDSHAKE_ALPN).await?;
		let mut send = connection.open_uni().await?;
		
		let my_id = self.endpoint_id.to_string();
		
		let payload = HandshakePayload {
			sender_endpoint_id: my_id,
			doc_ticket,
		};
		
		let bytes = serde_json::to_vec(&payload)?;
		send.write_all(&bytes).await?;
		send.finish()?;
		
		// Give Bob a moment to process before dropping the connection
		tokio::time::sleep(std::time::Duration::from_millis(500)).await;
		
		Ok(())
	}

	async fn load_secret_key(key_path: PathBuf) -> Result<SecretKey> {
		if key_path.exists() {
			let key_bytes = tokio::fs::read(key_path).await?;
			let secret_key = SecretKey::try_from(&key_bytes[0..32])?;
			Ok(secret_key)
		} else {
			let secret_key = SecretKey::generate(&mut rand::rng());
			
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
}
