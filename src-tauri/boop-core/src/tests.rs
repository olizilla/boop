#[cfg(test)]
mod integration_tests {
    use crate::engine::BoopEngine;
    use crate::iroh_manager::IrohManager;
    use crate::player::BoopPlayer;
    use std::time::Duration;
    use tempfile::tempdir;
    use std::sync::Arc;
    use async_trait::async_trait;

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

        let (iroh_a, rx_a) = IrohManager::new(iroh_dir_a, true).await.unwrap();
        let (iroh_b, rx_b) = IrohManager::new(iroh_dir_b, true).await.unwrap();

        // This should fail to compile because BoopEngine::new doesn't take player yet
        let player_a = Arc::new(MockPlayer);
        let player_b = Arc::new(MockPlayer);
        let _engine_a = BoopEngine::new(iroh_a.clone(), addr_book_a, rx_a, player_a).await.unwrap();
        let engine_b = BoopEngine::new(iroh_b.clone(), addr_book_b, rx_b, player_b).await.unwrap();

        // ... existing test logic ...
        tokio::time::sleep(Duration::from_millis(500)).await;
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
        engine_b.iroh.fetch_blob(&meta_hash.to_string(), &iroh_a.endpoint_id.to_string()).await.unwrap();
        let fetched_meta_bytes = engine_b.iroh.blobs().get_bytes(meta_hash).await.unwrap();
        assert_eq!(fetched_meta_bytes.len(), boop_meta_bytes.len());
    }

    #[tokio::test]
    async fn test_play_boop_marks_as_listened() {
        let dir = tempdir().unwrap();
        let iroh_dir = dir.path().join("iroh");
        let addr_book = dir.path().join("friends.json");
        let (iroh, rx) = IrohManager::new(iroh_dir, true).await.unwrap();
        
        let player = Arc::new(MockPlayer);
        let engine = BoopEngine::new(iroh.clone(), addr_book, rx, player).await.unwrap();

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
}
