use boop_core::{IrohManager, iroh_boops::BoopQueue};
use std::time::Duration;

#[tokio::test]
async fn test_handshake_and_boop_sync() {
    // We create isolated temp directories for node A and node B
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let (iroh_a, _rx_a) = IrohManager::new(dir_a.path().to_path_buf()).await.unwrap();
    let (iroh_b, mut rx_b) = IrohManager::new(dir_b.path().to_path_buf()).await.unwrap();

    // Alice creates a queue
    let queue_a = BoopQueue::new(None, iroh_a.clone()).await.unwrap();
    let ticket = queue_a.ticket();

    // Alice dials Bob to share the ticket
    let bob_id = iroh_b.endpoint_id.to_string();
    iroh_a.dial_friend(&bob_id, ticket.clone()).await.expect("Failed to dial Bob");

    // Bob receives it
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        if let Some((sender_id, doc_ticket)) = rx_b.recv().await {
            assert_eq!(sender_id, iroh_a.endpoint_id.to_string());
            assert_eq!(doc_ticket, ticket);
            true
        } else {
            false
        }
    }).await.expect("Handshake timed out - discovery or QUIC failed");
    
    assert!(result, "Handshake failed");
}
