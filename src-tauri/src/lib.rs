use std::sync::Arc;
use tauri::{State, Manager, WebviewUrl, WebviewWindowBuilder, Emitter};
#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;
use boop_core::{IrohManager, BoopEngine};

pub struct AppState {
    pub engine: Arc<BoopEngine>,
}

#[tauri::command]
async fn get_my_endpoint(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    Ok(state.engine.get_my_endpoint())
}

#[tauri::command]
async fn add_friend(state: State<'_, Arc<AppState>>, nickname: String, endpoint_id: String) -> Result<uuid::Uuid, String> {
    log::info!("Adding friend {} via endpoint: {}", nickname, endpoint_id);
    let ep = endpoint_id.parse::<boop_core::iroh::PublicKey>().map_err(|e| e.to_string())?;
    state.engine.add_friend(nickname, ep).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn send_boop(state: State<'_, Arc<AppState>>, friend_id: String, audio_bytes: Vec<u8>, mime_type: String) -> Result<(), String> {
    log::info!("Recording finished! Queueing a new boop for friend id {} ({})", friend_id, mime_type);
    let f_id = friend_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    state.engine.send_boop(f_id, audio_bytes, mime_type).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_audio_bytes(state: State<'_, Arc<AppState>>, friend_id: String, boop_id: String) -> Result<Vec<u8>, String> {
    let f_id = friend_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    state.engine.get_audio_bytes(f_id, &boop_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn mark_listened(state: State<'_, Arc<AppState>>, friend_id: String, boop_id: String) -> Result<(), String> {
    let f_id = friend_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    let b_id = boop_id.parse().map_err(|e: uuid::Error| e.to_string())?;
    state.engine.mark_listened(f_id, b_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn frontend_ready(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    log::info!("Frontend Ready! Emitting snapshot.");
    state.engine.emit_snapshot().await;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // fix ui glitch on linux on arm. see: https://github.com/olizilla/boop/issues/1
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");

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
                .fullscreen(false)
                // fix ui glitch on linux on arm. see: https://github.com/olizilla/boop/issues/1
                .visible(false);

            #[cfg(target_os = "macos")]
            let win_builder = win_builder.title_bar_style(TitleBarStyle::Transparent);

            let window = win_builder.build().unwrap();

            #[cfg(target_os = "macos")]
            {
                use objc2_app_kit::{NSColor, NSWindow};
                unsafe {
                    let ns_window = window.ns_window().unwrap() as *mut NSWindow;
                    let ns_window: &NSWindow = &*ns_window;
                    let bg_color = NSColor::colorWithRed_green_blue_alpha(0.0, 0.0, 0.5 / 255.0, 1.0);
                    ns_window.setBackgroundColor(Some(&bg_color));
                }
            }

            // fix mic permissions on linux. see: https://github.com/olizilla/boop/issues/2
            #[cfg(target_os = "linux")]
            {
                use webkit2gtk::{PermissionRequestExt, SettingsExt, WebViewExt};
                window.with_webview(|webview| {
                    let inner = webview.inner();
                    let settings = inner.settings().unwrap();
                    settings.set_enable_media_stream(true);
                    settings.set_enable_webrtc(true);
                    settings.set_enable_media_capabilities(true);
                    settings.set_enable_mediasource(true);
                    inner.connect_permission_request(|_view, request| {
                        request.allow();
                        true
                    });
                }).unwrap();
            }

            let app_handle = app.handle().clone();
            
            let (engine, mut event_rx) = tauri::async_runtime::block_on(async move {
                let env_dir = std::env::var("BOOP_DATA_DIR").ok().map(std::path::PathBuf::from);
                let data_dir = env_dir.unwrap_or_else(|| app_handle.path().app_data_dir().unwrap_or_else(|_| std::path::PathBuf::from(".boop")));
                let iroh_dir = data_dir.join("iroh");
                let address_book_path = data_dir.join("friends.json");
                
                let (iroh, rx) = IrohManager::new(iroh_dir, false).await.expect("failed to init iroh");
                
                let engine = BoopEngine::new(iroh, address_book_path, rx).await.expect("Failed to create engine");
                let event_rx = engine.event_tx.subscribe();
                (engine, event_rx)
            });
            
            let app_state = Arc::new(AppState { engine: Arc::new(engine) });
            app.manage(app_state);

            let emit_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Ok(event) = event_rx.recv().await {
                    if let Err(e) = emit_handle.emit("core-event", event) {
                        log::error!("Failed to emit core event to frontend: {}", e);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_my_endpoint,
            add_friend,
            send_boop,
            get_audio_bytes,
            mark_listened,
            frontend_ready
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
