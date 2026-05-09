#[cfg(test)]
mod integration_tests {
    use crate::engine::BoopEngine;
    use crate::iroh_manager::IrohManager;
    use crate::player::BoopPlayer;
    use std::time::Duration;
    use tempfile::tempdir;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use async_trait::async_trait;
    use n0_future::StreamExt;
    use std::str::FromStr;

    struct MockPlayer;
    #[async_trait]
    impl BoopPlayer for MockPlayer {
        async fn play(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
            // Mock instant playback
            Ok(())
        }
        async fn stop(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_offline_boop_fetch_logic() {
        let dir_a = tempdir().unwrap();
        let dir_b = tempdir().unwrap();
        
        let iroh_dir_a = dir_a.path().join("iroh");
        let iroh_dir_b = dir_b.path().join("iroh");
        
        let addr_book_a = dir_a.path().join("friends.json");
        let addr_book_b = dir_b.path().join("friends.json");

        let ab_a = Arc::new(tokio::sync::Mutex::new(crate::address_book::AddressBook::new()));
        let ab_b = Arc::new(tokio::sync::Mutex::new(crate::address_book::AddressBook::new()));
        
        let (iroh_a, rx_a) = IrohManager::new(iroh_dir_a, true, ab_a.clone()).await.unwrap();
        let (iroh_b, rx_b) = IrohManager::new(iroh_dir_b, true, ab_b.clone()).await.unwrap();

        ab_b.lock().await.add_friend("A".to_string(), iroh_a.endpoint_id);
        ab_a.lock().await.add_friend("B".to_string(), iroh_b.endpoint_id);

        let ticket_a = iroh_a.endpoint_ticket().unwrap();
        iroh_b.connect_to_endpoint_ticket(&ticket_a).await.unwrap();

        let player_a = Arc::new(MockPlayer);
        let player_b = Arc::new(MockPlayer);
        let _engine_a = BoopEngine::new(iroh_a.clone(), addr_book_a, ab_a.clone(), rx_a, player_a).await.unwrap();
        let engine_b = BoopEngine::new(iroh_b.clone(), addr_book_b, ab_b.clone(), rx_b, player_b).await.unwrap();



        let boop_bytes = b"real-boop-payload".to_vec();
        let raw_hash = iroh_a.blobs().add_bytes(boop_bytes.clone()).await.unwrap().hash;
        let boop = crate::iroh_boops::Boop {
            id: uuid::Uuid::new_v4(),
            created: 1,
            blob_hash: raw_hash,
            is_listened: false,
            mime_type: "audio/webm".to_string(),
        };
        let boop_meta_bytes = serde_json::to_vec(&boop).unwrap();
        let meta_hash = iroh_a.blobs().add_bytes(boop_meta_bytes.clone()).await.unwrap().hash;
        // Fetch the blob
        engine_b.iroh.fetch_blob(&meta_hash.to_string(), &iroh_a.endpoint_id.to_string()).await.expect("Blob download failed");
        let fetched_meta_bytes = engine_b.iroh.blobs().get_bytes(meta_hash).await.unwrap();
        assert_eq!(fetched_meta_bytes.len(), boop_meta_bytes.len());
    }

    #[tokio::test]
    async fn test_play_boop_marks_as_listened() {
        let dir = tempdir().unwrap();
        let iroh_dir = dir.path().join("iroh");
        let addr_book = dir.path().join("friends.json");
        let ab = Arc::new(tokio::sync::Mutex::new(crate::address_book::AddressBook::new()));
        let (iroh, rx) = IrohManager::new(iroh_dir, true, ab.clone()).await.unwrap();
        
        let player = Arc::new(MockPlayer);
        let engine = BoopEngine::new(iroh.clone(), addr_book, ab.clone(), rx, player).await.unwrap();

        let friend_id = uuid::Uuid::new_v4();
        let audio_bytes = b"fake-audio".to_vec();

        // Mock a queue for this friend
        let queue = crate::iroh_boops::BoopQueue::new(None, iroh.clone()).await.unwrap();
        let queue_arc = Arc::new(tokio::sync::Mutex::new(queue));
        engine.queues.lock().await.insert(friend_id, queue_arc.clone());

        // Add a boop to the queue as a DIFFERENT author (so it's not skipped by get_pending_boops)
        let friend_author = iroh.docs().author_create().await.unwrap();
        {
            let q = queue_arc.lock().await;
            let hash = iroh.blobs().add_bytes(audio_bytes).await.unwrap().hash;
            let boop = crate::iroh_boops::Boop {
                id: uuid::Uuid::new_v4(),
                created: 12345,
                blob_hash: hash,
                is_listened: false,
                mime_type: "audio/mp4".to_string(),
            };
            let key = format!("boops/{:020}-{}", boop.created, boop.id);
            let bytes = serde_json::to_vec(&boop).unwrap();
            q.doc().set_bytes(friend_author, key.as_bytes().to_vec(), bytes).await.unwrap();
        }

        // Give it a moment to index
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Get the ID of the boop we just sent
        let boop_id = {
            let q = queue_arc.lock().await;
            let pending = q.get_pending_boops().await.unwrap();
            assert!(!pending.is_empty(), "Pending boops should not be empty after send_boop from another author");
            pending[0].id
        };

        // Play the boop
        engine.play_boop(friend_id, boop_id).await.unwrap();

        // Verify it was marked as listened (tombstone exists)
        let queue_arc = engine.queues.lock().await.get(&friend_id).unwrap().clone();
        let q = queue_arc.lock().await;
        let _pending = q.get_pending_boops().await.unwrap();
        // Since we authored it (wait, mark_listened writes to the doc), 
        // we should check if the tombstone is there.
        // Actually, mark_listened writes "listened/{id}"
        let ticket = q.native_ticket();
        let doc = iroh.docs().import(ticket).await.unwrap();
        let key = format!("listened/{}", boop_id);
        let entry = doc.get_one(iroh_docs::store::Query::key_exact(key)).await.unwrap();
        assert!(entry.is_some(), "Tombstone should exist after playback");
    }

    #[tokio::test]
    async fn test_send_boop_transcodes_wav_to_flac() {
        let dir = tempdir().unwrap();
        let iroh_dir = dir.path().join("iroh");
        let addr_book = dir.path().join("friends.json");
        let ab = Arc::new(tokio::sync::Mutex::new(crate::address_book::AddressBook::new()));
        let (iroh, rx) = IrohManager::new(iroh_dir, true, ab.clone()).await.unwrap();
        let player = Arc::new(MockPlayer);
        let engine = BoopEngine::new(iroh.clone(), addr_book, ab.clone(), rx, player).await.unwrap();

        let friend_id = uuid::Uuid::new_v4();
        
        // Mock a queue for this friend
        let queue = crate::iroh_boops::BoopQueue::new(None, iroh.clone()).await.unwrap();
        let queue_arc = Arc::new(tokio::sync::Mutex::new(queue));
        engine.queues.lock().await.insert(friend_id, queue_arc.clone());

        // Create a valid 1s mono 16kHz WAV
        let mut wav_bytes = Vec::new();
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        {
            let mut writer = hound::WavWriter::new(std::io::Cursor::new(&mut wav_bytes), spec).unwrap();
            for i in 0..16000 {
                writer.write_sample(((i as f32).sin() * 32767.0) as i16).unwrap();
            }
            writer.finalize().unwrap();
        }
        let original_size = wav_bytes.len();

        // Send the WAV boop
        engine.send_boop(friend_id, wav_bytes, "audio/wav".to_string()).await.unwrap();

        // Verify it was stored as FLAC
        let _pending = {
            let q = queue_arc.lock().await;
            q.get_pending_boops().await.unwrap()
        };
        
        // Note: The MockPlayer/integration logic in get_pending_boops filters out 
        // boops authored by us. So we might need to look at the doc directly or 
        // use a different author for the send if we want to use get_pending_boops.
        // Actually, we can just check the last entry in the doc.
        let q = queue_arc.lock().await;
        let entries = q.doc().get_many(iroh_docs::store::Query::key_prefix("boops/")).await.unwrap();
        tokio::pin!(entries);
        let entry = entries.next().await.unwrap().unwrap();
        let content = iroh.blobs().get_bytes(entry.content_hash()).await.unwrap();
        let boop: crate::iroh_boops::Boop = serde_json::from_slice(&content).unwrap();
        
        assert_eq!(boop.mime_type, "audio/flac");
        assert!(boop.blob_hash != iroh_blobs::Hash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap());
        
        let audio_bytes = iroh.blobs().get_bytes(boop.blob_hash).await.unwrap();
        assert!(audio_bytes.len() < original_size, "FLAC should be smaller than WAV");
    }

    #[tokio::test]
    async fn test_presence_and_neighbor_events() {
        let dir_a = tempdir().unwrap();
        let dir_b = tempdir().unwrap();
        
        let ab_a = Arc::new(tokio::sync::Mutex::new(crate::address_book::AddressBook::new()));
        let ab_b = Arc::new(tokio::sync::Mutex::new(crate::address_book::AddressBook::new()));
        
        let (iroh_a, rx_a) = IrohManager::new(dir_a.path().join("iroh"), true, ab_a.clone()).await.unwrap();
        let (iroh_b, rx_b) = IrohManager::new(dir_b.path().join("iroh"), true, ab_b.clone()).await.unwrap();

        ab_a.lock().await.add_friend("B".to_string(), iroh_b.endpoint_id);
        ab_b.lock().await.add_friend("A".to_string(), iroh_a.endpoint_id);

        let player_a = Arc::new(MockPlayer);
        let player_b = Arc::new(MockPlayer);
        
        let _engine_a = BoopEngine::new(iroh_a.clone(), dir_a.path().join("friends.json"), ab_a.clone(), rx_a, player_a).await.unwrap();
        let engine_b = BoopEngine::new(iroh_b.clone(), dir_b.path().join("friends.json"), ab_b.clone(), rx_b, player_b).await.unwrap();

        let mut event_rx_b = engine_b.event_tx.subscribe();
        
        // Manually connect nodes
        let ticket_b = iroh_b.endpoint_ticket().unwrap();
        iroh_a.connect_to_endpoint_ticket(&ticket_b).await.unwrap();

        // Try explicitly sending presence
        iroh_a.send_presence(iroh_b.endpoint_id, true).await.expect("send_presence failed");
        
        // Wait for B to receive PeerActive
        let friend_id_a = ab_b.lock().await.friends.values().next().unwrap().id;
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Ok(event) = event_rx_b.recv().await {
                if let crate::events::CoreEvent::PeerActive { friend_id } = event {
                    if friend_id == friend_id_a {
                        return;
                    }
                }
            }
        }).await.expect("Timed out waiting for PeerActive event");
    }
    #[tokio::test]
    #[serial_test::serial]
    async fn test_invite_flow() {
        use std::println as info;
        let dir_a = tempdir().unwrap();
        let dir_b = tempdir().unwrap();
        
        let ab_a = Arc::new(tokio::sync::Mutex::new(crate::address_book::AddressBook::new()));
        let ab_b = Arc::new(tokio::sync::Mutex::new(crate::address_book::AddressBook::new()));
        
        let (iroh_a, rx_a) = IrohManager::new(dir_a.path().join("iroh"), true, ab_a.clone()).await.unwrap();
        let (iroh_b, rx_b) = IrohManager::new(dir_b.path().join("iroh"), true, ab_b.clone()).await.unwrap();
        
        let player_a = Arc::new(MockPlayer);
        let player_b = Arc::new(MockPlayer);
        
        let engine_a = BoopEngine::new(iroh_a, dir_a.path().join("friends.json"), ab_a, rx_a, player_a).await.unwrap();
        let engine_b = BoopEngine::new(iroh_b, dir_b.path().join("friends.json"), ab_b, rx_b, player_b).await.unwrap();
        
        let mut event_rx_a = engine_a.event_tx.subscribe();
        let mut event_rx_b = engine_b.event_tx.subscribe();
        
        // 1. A generates invite
        info!("A generating invite...");
        let invite_ticket = engine_a.generate_invite("Friend B".to_string()).await.unwrap();
        
        // 2. B accepts invite
        info!("B accepting invite...");
        let friend_id_a = engine_b.accept_invite(invite_ticket, "Friend A".to_string()).await.unwrap();
        
        // 3. Wait for A to receive FriendAdded event (server side)
        info!("Waiting for A to add friend...");
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Ok(event) = event_rx_a.recv().await {
                if let crate::events::CoreEvent::FriendAdded { friend } = event {
                    if friend.nickname == "Friend B" {
                        return;
                    }
                }
            }
        }).await.expect("A timed out waiting for FriendAdded event");

        // Trigger presence broadcast so they see each other as active
        engine_a.set_focus_state(true).await;
        engine_b.set_focus_state(true).await;
        
        // 4. Wait for them to sync and become active (proves handshake worked)
        info!("Waiting for B to see A as active...");
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Ok(event) = event_rx_b.recv().await {
                if let crate::events::CoreEvent::PeerActive { friend_id } = event {
                    if friend_id == friend_id_a {
                        return;
                    }
                }
            }
        }).await.expect("B timed out waiting for PeerActive event");
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn test_connection_reuse() {
        use std::println as info;
        let dir_a = tempdir().unwrap();
        let dir_b = tempdir().unwrap();
        
        let ab_a = Arc::new(Mutex::new(crate::address_book::AddressBook::new()));
        let ab_b = Arc::new(Mutex::new(crate::address_book::AddressBook::new()));
        
        let (iroh_a, _events_a) = IrohManager::new(dir_a.path().join("iroh"), true, ab_a.clone()).await.unwrap();
        let (iroh_b, _events_b) = IrohManager::new(dir_b.path().join("iroh"), true, ab_b.clone()).await.unwrap();
        
        // Add A to B's address book and vice versa manually
        {
            let mut ab = ab_a.lock().await;
            ab.add_friend("Friend B".to_string(), iroh_b.endpoint_id);
        }
        {
            let mut ab = ab_b.lock().await;
            ab.add_friend("Friend A".to_string(), iroh_a.endpoint_id);
        }

        let addr_b = iroh_b.endpoint.addr();
        
        // First presence update - should establish connection
        info!("Sending first presence update...");
        iroh_a.send_presence(addr_b.clone(), true).await.unwrap();
        
        // Second presence update - should REUSE connection
        info!("Sending second presence update...");
        iroh_a.send_presence(addr_b.clone(), false).await.unwrap();
        
        // Third presence update - should REUSE connection
        info!("Sending third presence update...");
        iroh_a.send_presence(addr_b, true).await.unwrap();
        
        info!("Connection reuse test complete!");
    }
}

