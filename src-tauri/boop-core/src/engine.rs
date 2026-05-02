use crate::address_book::{AddressBook, Friend};
use crate::events::CoreEvent;
use crate::iroh_boops::{BoopQueue, PendingBoopDto};
use crate::iroh_manager::IrohManager;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use crate::player::BoopPlayer;
use iroh_docs::engine::LiveEvent;
use n0_future::StreamExt;
use std::io::Cursor;
use hound::WavReader;
use flacenc::config;
use flacenc::source::MemSource;
use flacenc::error::Verify;
use flacenc::component::BitRepr;

#[derive(Clone)]
pub struct BoopEngine {
    pub iroh: IrohManager,
    pub address_book: Arc<Mutex<AddressBook>>,
    pub queues: Arc<Mutex<HashMap<uuid::Uuid, Arc<Mutex<BoopQueue>>>>>,
    pub address_book_path: PathBuf,
    pub event_tx: broadcast::Sender<CoreEvent>,
    pub player: Arc<dyn BoopPlayer>,
}

impl BoopEngine {
    pub async fn new(
        iroh: IrohManager,
        address_book_path: PathBuf,
        mut rx_handshake: tokio::sync::mpsc::UnboundedReceiver<(iroh::PublicKey, String)>,
        player: Arc<dyn BoopPlayer>,
    ) -> Result<Self> {
        let address_book = if address_book_path.exists() {
            let json = tokio::fs::read_to_string(&address_book_path).await?;
            serde_json::from_str(&json).unwrap_or_else(|e| {
                log::warn!("Failed to parse address book: {}", e);
                AddressBook::new()
            })
        } else {
            AddressBook::new()
        };

        let address_book = Arc::new(Mutex::new(address_book));
        let queues = Arc::new(Mutex::new(HashMap::new()));
        let (event_tx, _) = broadcast::channel(100);

        let engine = Self {
            iroh: iroh.clone(),
            address_book: address_book.clone(),
            queues: queues.clone(),
            address_book_path: address_book_path.clone(),
            event_tx: event_tx.clone(),
            player,
        };

        // Pre-warm queues for existing friends
        {
            let ab = address_book.lock().await;
            for friend in ab.friends.values() {
                if let Some(ref ticket) = friend.doc_ticket {
                    if let Ok(queue) = BoopQueue::new(Some(ticket.clone()), iroh.clone()).await {
                        let queue_arc = Arc::new(Mutex::new(queue));
                        queues.lock().await.insert(friend.id, queue_arc.clone());
                        engine.spawn_queue_listener(friend.id, friend.endpoint_id, queue_arc).await;
                    }
                }
            }
        }

        // Spawn Handshake Listener
        let engine_for_handshake = engine.clone();
        tokio::spawn(async move {
            while let Some((sender_endpoint, doc_ticket)) = rx_handshake.recv().await {
                log::info!(">>> Received Handshake from {}", sender_endpoint);
                engine_for_handshake.handle_handshake(sender_endpoint, doc_ticket).await;
            }
        });

        Ok(engine)
    }

    pub async fn handle_handshake(&self, sender_endpoint: iroh::PublicKey, doc_ticket: String) {
        let mut ab = self.address_book.lock().await;
        
        let is_existing = ab.friends.contains_key(&sender_endpoint);
        if !is_existing {
            let nickname = format!("Friend {}", &sender_endpoint.to_string()[..5]);
            let _id = ab.add_friend(nickname, sender_endpoint);
            // Notify frontend
            if let Some(friend) = ab.friends.get(&sender_endpoint) {
                let _ = self.event_tx.send(CoreEvent::FriendAdded { friend: friend.clone() });
            }
        }
        
        ab.set_friend_doc(sender_endpoint, doc_ticket.clone());
        self.save_address_book(&ab).await.ok();
        
        let friend = ab.friends.get(&sender_endpoint).cloned().unwrap();
        
        if let Ok(queue) = BoopQueue::new(Some(doc_ticket), self.iroh.clone()).await {
            log::info!("Successfully joined queue from handshake.");
            let queue_arc = Arc::new(Mutex::new(queue));
            self.queues.lock().await.insert(friend.id, queue_arc.clone());
            self.spawn_queue_listener(friend.id, friend.endpoint_id, queue_arc).await;
        }
    }

    pub async fn spawn_queue_listener(
        &self,
        friend_id: uuid::Uuid,
        friend_endpoint: iroh::PublicKey,
        queue: Arc<Mutex<BoopQueue>>,
    ) {
        let engine = self.clone();
        tokio::spawn(async move {
            let mut stream = {
                let q = queue.lock().await;
                match q.doc_subscribe().await {
                    Ok(s) => s,
                    Err(e) => {
                        log::error!("Failed to subscribe to doc: {}", e);
                        return;
                    }
                }
            };

            while let Some(Ok(event)) = stream.next().await {
                if let LiveEvent::InsertRemote { from: _, entry, .. } = event {
                    let key = entry.key().to_vec();
                    if let Ok(key_str) = String::from_utf8(key) {
                        log::debug!("InsertRemote: {}", key_str);
                        if key_str.starts_with("boops/") {
                            engine.handle_remote_boop(friend_id, friend_endpoint, entry).await;
                        } else if key_str.starts_with("listened/") {
                            let boop_id_str = key_str.replace("listened/", "");
                            if let Ok(boop_id) = boop_id_str.parse::<uuid::Uuid>() {
                                engine.handle_remote_listened(friend_id, boop_id, entry).await;
                            }
                        }
                    }
                }
            }
        });
    }

    async fn handle_remote_boop(
        &self, 
        friend_id: uuid::Uuid, 
        friend_endpoint: iroh::PublicKey, 
        entry: iroh_docs::Entry,
    ) {
        let hash = entry.content_hash();
        let mut fetched_bytes = None;
        
        for attempt in 0..5 {
            if let Ok(bytes) = self.iroh.blobs().get_bytes(hash).await {
                fetched_bytes = Some(bytes);
                break;
            }
            log::warn!("Metadata blob {} missing, explicitly fetching...", hash);
            let _ = self.iroh.fetch_blob(&hash.to_string(), &friend_endpoint.to_string()).await;
            tokio::time::sleep(tokio::time::Duration::from_millis(500 * (attempt + 1))).await;
        }

        if let Some(b) = fetched_bytes {
            if let Ok(boop) = crate::iroh_boops::Boop::from_bytes(b) {
                if !boop.is_listened {
                    let is_ready = self.iroh.blobs().has(boop.blob_hash).await.unwrap_or(false);
                    let dto = PendingBoopDto {
                        id: boop.id,
                        created: boop.created,
                        blob_hash: boop.blob_hash,
                        is_ready,
                        mime_type: boop.mime_type,
                    };
                    
                    let _ = self.event_tx.send(CoreEvent::BoopReceived { friend_id, boop: dto });

                    if !is_ready {
                        let iroh = self.iroh.clone();
                        let event_tx = self.event_tx.clone();
                        let hash_str = boop.blob_hash.to_string();
                        let ep_str = friend_endpoint.to_string();
                        
                        tokio::spawn(async move {
                            // Retry loop for robustness
                            for attempt in 1..=5 {
                                log::info!("Attempt {} to fetch audio blob {}", attempt, hash_str);
                                if iroh.fetch_blob(&hash_str, &ep_str).await.is_ok() {
                                    log::info!("Audio blob {} explicitly fetched!", hash_str);
                                    let _ = event_tx.send(CoreEvent::BoopReady { friend_id, boop_id: boop.id });
                                    break;
                                }
                                tokio::time::sleep(tokio::time::Duration::from_secs(5 * attempt)).await;
                            }
                        });
                    } else {
                        let _ = self.event_tx.send(CoreEvent::BoopReady { friend_id, boop_id: boop.id });
                    }
                }
            }
        } else {
            log::error!("Failed to fetch metadata blob for remote boop after retries");
        }
    }

    async fn handle_remote_listened(
        &self,
        friend_id: uuid::Uuid,
        boop_id: uuid::Uuid,
        _entry: iroh_docs::Entry,
    ) {
        // Collect garbage
        if let Some(queue_arc) = self.queues.lock().await.get(&friend_id) {
            let queue = queue_arc.lock().await;
            
            // Delete the metadata
            // let boop_key = format!("boops/{:020}-{boop_id}", 0); // We'd need the created timestamp... or we can just list prefix
            
            // Wait, we need the exact key to delete it. Let's just use queue.garbage_collect_tombstones()
            queue.garbage_collect_tombstones().await.ok();
        }

        let _ = self.event_tx.send(CoreEvent::BoopListenedRemote { friend_id, boop_id });
    }

    pub async fn save_address_book(&self, ab: &AddressBook) -> Result<()> {
        let json = serde_json::to_string_pretty(ab)?;
        tokio::fs::write(&self.address_book_path, json).await?;
        Ok(())
    }

    pub async fn emit_snapshot(&self) {
        let friends: Vec<Friend> = {
            let ab = self.address_book.lock().await;
            ab.friends.values().cloned().collect()
        };

        let mut pending_boops = HashMap::new();
        let queues = self.queues.lock().await;
        for (f_id, queue_arc) in queues.iter() {
            let queue = queue_arc.lock().await;
            if let Ok(boops) = queue.get_pending_boops().await {
                // Background start explicit fetch for unready ones
                for b in &boops {
                    if !b.is_ready {
                        let iroh = self.iroh.clone();
                        let event_tx = self.event_tx.clone();
                        let friend_id = *f_id;
                        let boop_id = b.id;
                        let hash_str = b.blob_hash.to_string();
                        let ep_str = friends.iter().find(|x| x.id == friend_id).unwrap().endpoint_id.to_string();
                        
                        tokio::spawn(async move {
                            if iroh.fetch_blob(&hash_str, &ep_str).await.is_ok() {
                                let _ = event_tx.send(CoreEvent::BoopReady { friend_id, boop_id });
                            }
                        });
                    }
                }
                pending_boops.insert(*f_id, boops);
            }
        }

        let _ = self.event_tx.send(CoreEvent::StateSnapshot { friends, pending_boops });
    }

    pub fn get_my_endpoint(&self) -> String {
        self.iroh.endpoint_id.to_string()
    }

    pub async fn add_friend(&self, nickname: String, endpoint_id: iroh::PublicKey) -> Result<uuid::Uuid> {
        let mut ab = self.address_book.lock().await;
        let friend_id = ab.add_friend(nickname, endpoint_id);
        
        let queue = BoopQueue::new(None, self.iroh.clone()).await?;
        let doc_ticket = queue.ticket();
        
        ab.set_friend_doc(endpoint_id, doc_ticket.clone());
        self.save_address_book(&ab).await?;
        
        let queue_arc = Arc::new(Mutex::new(queue));
        self.queues.lock().await.insert(friend_id, queue_arc.clone());
        self.spawn_queue_listener(friend_id, endpoint_id, queue_arc).await;
        
        let dt = doc_ticket.clone();
        let iroh = self.iroh.clone();
        tokio::spawn(async move {
            let _ = iroh.dial_friend(endpoint_id, dt).await;
        });

        if let Some(friend) = ab.friends.get(&endpoint_id) {
            let _ = self.event_tx.send(CoreEvent::FriendAdded { friend: friend.clone() });
        }
        
        Ok(friend_id)
    }

    pub async fn send_boop(&self, friend_id: uuid::Uuid, mut audio_bytes: Vec<u8>, mut mime_type: String) -> Result<()> {
        if mime_type == "audio/wav" {
            let start_size = audio_bytes.len();
            let start_time = std::time::Instant::now();
            
            match encode_flac(&audio_bytes) {
                Ok(flac_bytes) => {
                    let end_size = flac_bytes.len();
                    let duration = start_time.elapsed();
                    let ratio = (end_size as f32 / start_size as f32) * 100.0;
                    
                    log::info!(
                        "[Engine] Transcoded WAV to FLAC: {} bytes -> {} bytes ({:.1}% ratio) in {:?}",
                        start_size, end_size, ratio, duration
                    );
                    
                    audio_bytes = flac_bytes;
                    mime_type = "audio/flac".to_string();
                }
                Err(e) => {
                    log::error!("[Engine] FLAC transcoding failed, sending original WAV: {}", e);
                }
            }
        }

        let queues = self.queues.lock().await;
        if let Some(queue_mtx) = queues.get(&friend_id) {
            let mut queue = queue_mtx.lock().await;
            queue.send_boop(audio_bytes, mime_type).await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Friend queue not initialized"))
        }
    }

    pub async fn get_audio_bytes(&self, friend_id: uuid::Uuid, boop_id: &str) -> Result<Vec<u8>> {
        use std::str::FromStr;
        let hash = iroh_blobs::Hash::from_str(boop_id)?;
        let queues = self.queues.lock().await;
        if let Some(queue_mtx) = queues.get(&friend_id) {
            let queue = queue_mtx.lock().await;
            queue.get_audio_bytes(hash).await
        } else {
            Err(anyhow::anyhow!("Friend queue not initialized"))
        }
    }

    pub async fn mark_listened(&self, friend_id: uuid::Uuid, boop_id: uuid::Uuid) -> Result<()> {
        let queues = self.queues.lock().await;
        if let Some(queue_mtx) = queues.get(&friend_id) {
            let queue = queue_mtx.lock().await;
            queue.mark_listened(boop_id).await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Friend queue not initialized"))
        }
    }

    pub async fn play_boop(&self, friend_id: uuid::Uuid, boop_id: uuid::Uuid) -> Result<()> {
        let audio_bytes = {
            let queues = self.queues.lock().await;
            if let Some(queue_mtx) = queues.get(&friend_id) {
                let queue = queue_mtx.lock().await;
                // Find the boop to get its hash
                let pending = queue.get_pending_boops().await?;
                let boop = pending.iter().find(|b| b.id == boop_id)
                    .ok_or_else(|| anyhow::anyhow!("Boop not found in pending queue"))?;
                
                queue.get_audio_bytes(boop.blob_hash).await?
            } else {
                return Err(anyhow::anyhow!("Friend queue not initialized"));
            }
        };

        let _ = self.event_tx.send(CoreEvent::PlaybackStarted { friend_id, boop_id });
        
        // Play the audio. This blocks until playback finishes.
        self.player.play(audio_bytes).await?;

        // Automatically mark as listened
        self.mark_listened(friend_id, boop_id).await?;

        let _ = self.event_tx.send(CoreEvent::PlaybackFinished { friend_id, boop_id });
        
        Ok(())
    }
}

fn encode_flac(wav_bytes: &[u8]) -> Result<Vec<u8>> {
    let mut reader = WavReader::new(Cursor::new(wav_bytes))?;
    let spec = reader.spec();
    
    let samples: Vec<i32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            reader.samples::<i32>().map(|s| s.unwrap()).collect()
        },
        hound::SampleFormat::Float => {
            reader.samples::<f32>().map(|s| (s.unwrap() * 2147483647.0) as i32).collect()
        },
    };

    let config = config::Encoder::default().into_verified().map_err(|e| anyhow::anyhow!("FLAC config error: {:?}", e))?;
    let source = MemSource::from_samples(&samples, spec.channels as usize, spec.bits_per_sample as usize, spec.sample_rate as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size).map_err(|e| anyhow::anyhow!("FLAC encode error: {:?}", e))?;
    
    let mut sink = flacenc::bitsink::ByteSink::new();
    let _ = stream.write(&mut sink);
    
    Ok(sink.into_inner())
}
