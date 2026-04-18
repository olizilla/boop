use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{State, Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder};
use std::collections::HashMap;
use boop_core::{IrohManager, address_book::AddressBook, iroh_boops::{BoopQueue, PendingBoopDto}};
use std::str::FromStr;

pub struct AppState {
	pub iroh: IrohManager,
	pub address_book: Arc<Mutex<AddressBook>>,
	pub queues: Arc<Mutex<HashMap<uuid::Uuid, Arc<Mutex<BoopQueue>>>>>,
	pub address_book_path: std::path::PathBuf,
}

fn save_address_book(path: &std::path::Path, ab: &AddressBook) -> Result<(), String> {
	let json = serde_json::to_string_pretty(ab).map_err(|e| e.to_string())?;
	std::fs::write(path, json).map_err(|e| e.to_string())?;
	Ok(())
}

#[tauri::command]
async fn get_my_endpoint(state: State<'_, Arc<AppState>>) -> Result<String, String> {
	Ok(state.iroh.endpoint_id.to_string())
}

#[tauri::command]
async fn add_friend(state: State<'_, Arc<AppState>>, nickname: String, endpoint_id: String) -> Result<uuid::Uuid, String> {
	log::info!("Adding friend {} via endpoint: {}", nickname, endpoint_id);
	let friend_ep = endpoint_id.parse::<boop_core::iroh::PublicKey>().map_err(|e| e.to_string())?;
	let mut ab = state.address_book.lock().await;
	let friend_id = ab.add_friend(nickname, friend_ep);
	
	// Create a new empty document/queue for this pairing locally
	let queue = BoopQueue::new(None, state.iroh.clone()).await.map_err(|e| e.to_string())?;
	let doc_ticket = queue.ticket();
	
	ab.set_friend_doc(friend_ep, doc_ticket.clone());
	save_address_book(&state.address_book_path, &ab)?;
	
	state.queues.lock().await.insert(friend_id, Arc::new(Mutex::new(queue)));
	
	// Try to eagerly dial the friend in the background to share the doc_ticket.
	// We swallow errors because they might be offline.
	let iroh = state.iroh.clone();
	let dt = doc_ticket.clone();
	tauri::async_runtime::spawn(async move {
		log::info!("Background: Dialing friend {} to share doc_ticket...", friend_ep);
		match iroh.dial_friend(friend_ep, dt).await {
			Ok(_) => log::info!("Successfully sent handshake to {}!", friend_ep),
			Err(e) => log::warn!("Failed to dial friend {} (might be offline): {}", friend_ep, e),
		}
	});
	
	Ok(friend_id)
}

#[tauri::command]
async fn is_friend_online(_state: State<'_, Arc<AppState>>, _endpoint_id: String) -> Result<bool, String> {
	// For MVP, we pretend they are always dialed and let QUIC handle timeouts.
	// In production, we'd add an endpoint connection check here.
	Ok(true)
}

#[tauri::command]
async fn get_friends(state: State<'_, Arc<AppState>>) -> Result<Vec<boop_core::address_book::Friend>, String> {
	let ab = state.address_book.lock().await;
	Ok(ab.friends.clone())
}

#[tauri::command]
async fn send_boop(state: State<'_, Arc<AppState>>, friend_id: String, audio_bytes: Vec<u8>, mime_type: String) -> Result<(), String> {
	log::info!("Recording finished! Queueing a new boop for friend id {} ({})", friend_id, mime_type);
	let f_id = friend_id.parse::<uuid::Uuid>().map_err(|e| e.to_string())?;
	let queues = state.queues.lock().await;
	if let Some(queue_mtx) = queues.get(&f_id) {
		let mut queue = queue_mtx.lock().await;
		queue.send_boop(audio_bytes, mime_type).await.map_err(|e| {
			log::error!("Failed to enqueue boop: {}", e);
			e.to_string()
		})?;
		log::info!("Boop successfully committed to local iroh-doc!");
		Ok(())
	} else {
		Err("Friend queue not initialized".into())
	}
}

#[tauri::command]
async fn get_pending_boops(state: State<'_, Arc<AppState>>, friend_id: String) -> Result<Vec<PendingBoopDto>, String> {
	let f_id = friend_id.parse::<uuid::Uuid>().map_err(|e| e.to_string())?;
	let queues = state.queues.lock().await;
	if let Some(queue_mtx) = queues.get(&f_id) {
		let queue = queue_mtx.lock().await;
		let boops = queue.get_pending_boops().await.map_err(|e| e.to_string())?;
		Ok(boops)
	} else {
		Err("Friend queue not initialized".into())
	}
}

#[tauri::command]
async fn get_audio_bytes(state: State<'_, Arc<AppState>>, friend_id: String, boop_id: String) -> Result<Vec<u8>, String> {
	let f_id = friend_id.parse::<uuid::Uuid>().map_err(|e| e.to_string())?;
	let hash = boop_core::iroh_blobs::Hash::from_str(&boop_id).map_err(|e| e.to_string())?;
	let queues = state.queues.lock().await;
	if let Some(queue_mtx) = queues.get(&f_id) {
		let queue = queue_mtx.lock().await;
		let bytes = queue.get_audio_bytes(hash).await.map_err(|e| e.to_string())?;
		Ok(bytes)
	} else {
		Err("Friend queue not initialized".into())
	}
}

#[tauri::command]
async fn mark_listened(state: State<'_, Arc<AppState>>, friend_id: String, boop_id: String) -> Result<(), String> {
	let f_id = friend_id.parse::<uuid::Uuid>().map_err(|e| e.to_string())?;
	let b_id = boop_id.parse::<uuid::Uuid>().map_err(|e| e.to_string())?;
	let queues = state.queues.lock().await;
	if let Some(queue_mtx) = queues.get(&f_id) {
		let queue = queue_mtx.lock().await;
		queue.mark_listened(b_id).await.map_err(|e| e.to_string())?;
		Ok(())
	} else {
		Err("Friend queue not initialized".into())
	}
}

#[tauri::command]
async fn download_boop(state: State<'_, Arc<AppState>>, friend_id: String, hash_str: String) -> Result<(), String> {
	let f_id = friend_id.parse::<uuid::Uuid>().map_err(|e| e.to_string())?;
	let mut friend_endpoint = None;
	{
		let ab = state.address_book.lock().await;
		if let Some(f) = ab.friends.iter().find(|x| x.id == f_id) {
			friend_endpoint = Some(f.endpoint_id);
		}
	}
	
	let friend_endpoint = friend_endpoint.ok_or_else(|| "Friend not found".to_string())?;
	
	state.iroh.fetch_blob(&hash_str, &friend_endpoint.to_string()).await.map_err(|e| e.to_string())?;
	Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
	tauri::Builder::default()
		.setup(|app| {
			if cfg!(debug_assertions) {
				app.handle().plugin(
					tauri_plugin_log::Builder::new()
						.targets([
							tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
							tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Webview),
						])
						.level(log::LevelFilter::Warn) // Default level for external crates
						.level_for("app_lib", log::LevelFilter::Debug) // Show our app logs
						.level_for("boop_core", log::LevelFilter::Debug) // Show our core logs
						.level_for("iroh::net_report", log::LevelFilter::Error)
						.level_for("tracing::span", log::LevelFilter::Error)
						.build(),
				)?;
			}

			log::info!("--- BOOP APP STARTED ---");
			let win_builder = WebviewWindowBuilder::new(app, "main", WebviewUrl::default())
				.title("BOOP")
				.inner_size(450.0, 450.0)
				.resizable(false)
				.fullscreen(false);

			// set transparent title bar only when building for macOS
			#[cfg(target_os = "macos")]
			let win_builder = win_builder.title_bar_style(TitleBarStyle::Transparent);

			let window = win_builder.build().unwrap();

			// set background color only when building for macOS
			#[cfg(target_os = "macos")]
			{
				use objc2_app_kit::{NSColor, NSWindow};
				
				unsafe {
					let ns_window = window.ns_window().unwrap() as *mut NSWindow;
					let ns_window: &NSWindow = &*ns_window;

					let bg_color = NSColor::colorWithRed_green_blue_alpha(
						0.0,
						0.0,
						0.5 / 255.0,
						1.0,
					);
					ns_window.setBackgroundColor(Some(&bg_color));
				}
			}

			let app_handle = app.handle().clone();
			let (state, mut rx) = tauri::async_runtime::block_on(async move {
				let env_dir = std::env::var("BOOP_DATA_DIR").ok().map(std::path::PathBuf::from);
				let data_dir = env_dir.unwrap_or_else(|| app_handle.path().app_data_dir().unwrap_or_else(|_| std::path::PathBuf::from(".boop")));
				let iroh_dir = data_dir.join("iroh");
				let address_book_path = data_dir.join("friends.json");
				
				let (iroh, rx) = IrohManager::new(iroh_dir, false).await.expect("failed to init iroh");
				
				let address_book = if address_book_path.exists() {
					let json = std::fs::read_to_string(&address_book_path).expect("failed to read address book");
					serde_json::from_str(&json).expect("failed to parse address book")
				} else {
					AddressBook::new()
				};
				
				let address_book = Arc::new(Mutex::new(address_book));
				let queues = Arc::new(Mutex::new(HashMap::new()));
				
				// Pre-warm queues for existing friends
				{
					let ab = address_book.lock().await;
					for friend in &ab.friends {
						if let Some(ref ticket) = friend.doc_ticket {
							if let Ok(queue) = BoopQueue::new(Some(ticket.clone()), iroh.clone()).await {
								queues.lock().await.insert(friend.id, Arc::new(Mutex::new(queue)));
							}
						}
					}
				}

				let state = Arc::new(AppState {
					iroh: iroh.clone(),
					address_book: address_book.clone(),
					queues: queues.clone(),
					address_book_path: address_book_path.clone(),
				});
				
				(state, rx)
			});
			
			app.manage(state.clone());
			
			let state_for_handshake = state.clone();
			tauri::async_runtime::spawn(async move {
				log::info!("Listening for background handshakes...");
				// Read Handshakes
				while let Some((sender_endpoint, doc_ticket)) = rx.recv().await {
					log::info!(">>> Received Handshake from {}! Processing...", sender_endpoint);
					let mut ab = state_for_handshake.address_book.lock().await;
					
					// If we don't naturally have this friend, create an implicit one
					let is_existing = ab.friends.iter().any(|f| f.endpoint_id == sender_endpoint);
					if !is_existing {
						ab.add_friend(format!("Friend {}", &sender_endpoint.to_string()[..5]), sender_endpoint);
					}
					
					ab.set_friend_doc(sender_endpoint, doc_ticket.clone());
					save_address_book(&state_for_handshake.address_book_path, &ab).ok();
					
					let friend_id = ab.friends.iter().find(|f| f.endpoint_id == sender_endpoint).unwrap().id;
					log::info!("Local Handshake processed! Syncing doc for local friend id {}", friend_id);
					
					// Init queue
					if let Ok(queue) = BoopQueue::new(Some(doc_ticket), state_for_handshake.iroh.clone()).await {
						log::info!("Successfully joined queue from handshake.");
						state_for_handshake.queues.lock().await.insert(friend_id, Arc::new(Mutex::new(queue)));
					} else {
						log::error!("Failed to initialize queue from incoming handshake.");
					}
				}
			});
			
			Ok(())
		})
		.invoke_handler(tauri::generate_handler![
			get_my_endpoint,
			add_friend,
			get_friends,
			is_friend_online,
			send_boop,
			get_pending_boops,
			get_audio_bytes,
			mark_listened,
			download_boop
		])
		.run(tauri::generate_context!())
		.expect("error while running tauri application");
}
