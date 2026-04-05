use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{State, Manager, AppHandle};
use std::collections::HashMap;
use boop_core::{IrohManager, address_book::AddressBook, iroh_boops::{BoopQueue, Boop, PendingBoopDto}};

pub struct AppState {
    pub iroh: IrohManager,
    pub address_book: Arc<Mutex<AddressBook>>,
    pub queues: Arc<Mutex<HashMap<String, Arc<Mutex<BoopQueue>>>>>,
}

#[tauri::command]
async fn get_my_endpoint(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    Ok(state.iroh.endpoint_id.to_string())
}

#[tauri::command]
async fn add_friend(state: State<'_, Arc<AppState>>, nickname: String, endpoint_id: String) -> Result<String, String> {
    log::info!("Adding friend {} via endpoint: {}", nickname, endpoint_id);
    let mut ab = state.address_book.lock().await;
    ab.add_friend(nickname, endpoint_id.clone());
    
    // The newly created friend ID from the local address book
    let friend_id = ab.friends.last().unwrap().id.clone();
    
    // Create a new empty document/queue for this pairing locally
    let queue = BoopQueue::new(None, state.iroh.clone()).await.map_err(|e| e.to_string())?;
    let doc_ticket = queue.ticket();
    
    ab.set_friend_doc(&endpoint_id, doc_ticket.clone());
    state.queues.lock().await.insert(friend_id.clone(), Arc::new(Mutex::new(queue)));
    
    // Try to eagerly dial the friend in the background to share the doc_ticket.
    // We swallow errors because they might be offline.
    let iroh = state.iroh.clone();
    let eid = endpoint_id.clone();
    let dt = doc_ticket.clone();
    tauri::async_runtime::spawn(async move {
        log::info!("Background: Dialing friend {} to share doc_ticket...", eid);
        match iroh.dial_friend(&eid, dt).await {
            Ok(_) => log::info!("Successfully sent handshake to {}!", eid),
            Err(e) => log::warn!("Failed to dial friend {} (might be offline): {}", eid, e),
        }
    });
    
    Ok(friend_id)
}

#[tauri::command]
async fn is_friend_online(state: State<'_, Arc<AppState>>, endpoint_id: String) -> Result<bool, String> {
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
async fn send_boop(state: State<'_, Arc<AppState>>, friend_id: String, audio_bytes: Vec<u8>) -> Result<(), String> {
    log::info!("Recording finished! Queueing a new boop for friend id {}", friend_id);
    let queues = state.queues.lock().await;
    if let Some(queue_mtx) = queues.get(&friend_id) {
        let mut queue = queue_mtx.lock().await;
        queue.send_boop(audio_bytes).await.map_err(|e| {
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
    let queues = state.queues.lock().await;
    if let Some(queue_mtx) = queues.get(&friend_id) {
        let queue = queue_mtx.lock().await;
        let boops = queue.get_pending_boops().await.map_err(|e| e.to_string())?;
        Ok(boops)
    } else {
        Err("Friend queue not initialized".into())
    }
}

#[tauri::command]
async fn get_audio_bytes(state: State<'_, Arc<AppState>>, friend_id: String, boop_id: String) -> Result<Vec<u8>, String> {
    let queues = state.queues.lock().await;
    if let Some(queue_mtx) = queues.get(&friend_id) {
        let queue = queue_mtx.lock().await;
        let bytes = queue.get_audio_bytes(&boop_id).await.map_err(|e| e.to_string())?;
        Ok(bytes)
    } else {
        Err("Friend queue not initialized".into())
    }
}

#[tauri::command]
async fn mark_listened(state: State<'_, Arc<AppState>>, friend_id: String, boop_id: String) -> Result<(), String> {
    let queues = state.queues.lock().await;
    if let Some(queue_mtx) = queues.get(&friend_id) {
        let queue = queue_mtx.lock().await;
        queue.mark_listened(&boop_id).await.map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Friend queue not initialized".into())
    }
}

#[tauri::command]
async fn download_boop(state: State<'_, Arc<AppState>>, friend_id: String, hash_str: String) -> Result<(), String> {
    log::info!("Eagerly downloading blob {} for friend {}", hash_str, friend_id);
    let mut friend_endpoint = String::new();
    {
        let ab = state.address_book.lock().await;
        if let Some(f) = ab.friends.iter().find(|x| x.id == friend_id) {
            friend_endpoint = f.endpoint_id.clone();
        }
    }
    
    if friend_endpoint.is_empty() {
        return Err("Friend not found".into());
    }
    
    state.iroh.fetch_blob(&hash_str, &friend_endpoint).await.map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Warn)
                        .level_for("app", log::LevelFilter::Debug)
                        .level_for("boop_core", log::LevelFilter::Debug)
                        .build(),
                )?;
            }
            
            log::info!("--- BOOP APP STARTED ---");
            
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let env_dir = std::env::var("BOOP_DATA_DIR").ok().map(std::path::PathBuf::from);
                let data_dir = env_dir.unwrap_or_else(|| app_handle.path().app_data_dir().unwrap_or_else(|_| std::path::PathBuf::from(".boop")));
                let (iroh, mut rx) = IrohManager::new(data_dir.join("iroh")).await.expect("failed to init iroh");
                
                let address_book = Arc::new(Mutex::new(AddressBook::new()));
                let queues = Arc::new(Mutex::new(HashMap::new()));
                
                let state = Arc::new(AppState {
                    iroh: iroh.clone(),
                    address_book: address_book.clone(),
                    queues: queues.clone(),
                });
                
                app_handle.manage(state);
                
                log::info!("Listening for background handshakes...");
                // Read Handshakes
                while let Some((sender_endpoint, doc_ticket)) = rx.recv().await {
                    log::info!(">>> Received Handshake from {}! Processing...", sender_endpoint);
                    let mut ab = address_book.lock().await;
                    
                    // If we don't naturally have this friend, create an implicit one
                    let is_existing = ab.friends.iter().any(|f| f.endpoint_id == sender_endpoint);
                    if !is_existing {
                        ab.add_friend(format!("Friend {}", &sender_endpoint[..5]), sender_endpoint.clone());
                    }
                    
                    ab.set_friend_doc(&sender_endpoint, doc_ticket.clone());
                    
                    let friend_id = ab.friends.iter().find(|f| f.endpoint_id == sender_endpoint).unwrap().id.clone();
                    log::info!("Local Handshake processed! Syncing doc for local friend id {}", friend_id);
                    
                    // Init queue
                    if let Ok(queue) = BoopQueue::new(Some(doc_ticket), iroh.clone()).await {
                        log::info!("Successfully joined queue from handshake.");
                        queues.lock().await.insert(friend_id, Arc::new(Mutex::new(queue)));
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
