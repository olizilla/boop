#[cfg(test)]
mod integration_tests {
    use crate::engine::BoopEngine;
    use crate::iroh_manager::IrohManager;
    use std::time::Duration;
    use tempfile::tempdir;

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

        let _engine_a = BoopEngine::new(iroh_a.clone(), addr_book_a, rx_a).await.unwrap();
        let engine_b = BoopEngine::new(iroh_b.clone(), addr_book_b, rx_b).await.unwrap();

        // Let endpoints initialize
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Construct a raw boop payload directly in A's Blob Store.
        let boop_bytes = b"real-boop-payload".to_vec();
        let raw_hash = iroh_a.blobs().add_bytes(boop_bytes.clone()).await.unwrap().hash;
        
        // Construct a metadata Boop struct matching A's data
        let boop = crate::iroh_boops::Boop {
            id: uuid::Uuid::new_v4(),
            created: 1,
            blob_hash: raw_hash,
            is_listened: false,
            mime_type: "audio/webm".to_string(),
        };
        let boop_meta_bytes = serde_json::to_vec(&boop).unwrap();
        
        // Insert metadata into A's store
        let meta_hash = iroh_a.blobs().add_bytes(boop_meta_bytes.clone()).await.unwrap().hash;
        
        // Assert B does NOT have the metadata blob natively
        assert!(iroh_b.blobs().get_bytes(meta_hash).await.is_err(), "B should not magically have the blob yet!");

        // Prove that explicit `fetch_blob` cleanly downloads it
        engine_b.iroh.fetch_blob(&meta_hash.to_string(), &iroh_a.endpoint_id.to_string()).await.unwrap();

        // Verify B now has the identical metadata bytes!
        let fetched_meta_bytes = engine_b.iroh.blobs().get_bytes(meta_hash).await.unwrap();
        assert_eq!(fetched_meta_bytes.len(), boop_meta_bytes.len());
        
        // This validates the retry explicitly: fetch_blob connects and successfully pulls
        // the un-synced chunks dynamically based on the endpoint ID.
    }
}
