use boop_core::{IrohManager, iroh_boops::BoopQueue};
use std::time::Duration;

#[tokio::test]
async fn test_handshake_and_boop_sync() {
	// We create isolated temp directories for node A and node B
	let dir_a = tempfile::tempdir().unwrap();
	let dir_b = tempfile::tempdir().unwrap();

	let (iroh_a, _rx_a) = IrohManager::new(dir_a.path().to_path_buf(), true).await.unwrap();
	let (iroh_b, mut rx_b) = IrohManager::new(dir_b.path().to_path_buf(), true).await.unwrap();

	// Alice creates a queue
	let queue_a = BoopQueue::new(None, iroh_a.clone()).await.unwrap();
	let ticket = queue_a.ticket();

	// Sleep to let local addresses populate
	// tokio::time::sleep(Duration::from_secs(2)).await;

	// Alice dials Bob to share the ticket using his full direct address info
	let bob_addr = iroh_b.endpoint().addr();
	iroh_a.dial_friend(bob_addr, ticket.clone()).await.expect("Failed to dial Bob");

	// Bob receives it
	let result = tokio::time::timeout(Duration::from_secs(5), async {
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

#[tokio::test]
async fn test_full_boop_lifecycle() {
	let dir_a = tempfile::tempdir().unwrap();
	let dir_b = tempfile::tempdir().unwrap();

	let (iroh_a, _rx_a) = IrohManager::new(dir_a.path().to_path_buf(), true).await.unwrap();
	let (iroh_b, mut rx_b) = IrohManager::new(dir_b.path().to_path_buf(), true).await.unwrap();

	// Alice creates a queue
	let mut queue_a = BoopQueue::new(None, iroh_a.clone()).await.unwrap();
	let ticket = queue_a.ticket();

	// Alice dials Bob
	let bob_addr = iroh_b.endpoint().addr();
	iroh_a.dial_friend(bob_addr, ticket.clone()).await.unwrap();

	// Bob receives it and joins the queue
	let (_, ticket_b) = tokio::time::timeout(Duration::from_secs(5), rx_b.recv())
		.await.unwrap().unwrap();
	let queue_b = BoopQueue::new(Some(ticket_b), iroh_b.clone()).await.unwrap();

	// Alice sends a boop to Bob
	let dummy_audio = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
	queue_a.send_boop(dummy_audio.clone(), "audio/webm".to_string()).await.unwrap();
	
	// Bob should see the metadata eventually
	let mut boops = Vec::new();
	for _ in 0..20 {
		boops = queue_b.get_pending_boops().await.unwrap();
		if !boops.is_empty() { break; }
		tokio::time::sleep(Duration::from_millis(500)).await;
	}
	assert_eq!(boops.len(), 1, "Bob should have received 1 boop metadata");
	let boop = &boops[0];
	assert_eq!(boop.is_ready, false, "Blob shouldn't be local yet");

	// Bob downloads the blob
	iroh_b.fetch_blob(&boop.blob_hash, &iroh_a.endpoint_id.to_string()).await.unwrap();

	// Verify Bob now has it ready
	boops = queue_b.get_pending_boops().await.unwrap();
	assert!(boops[0].is_ready, "Blob should be local now");
	
	// Verify content
	let downloaded_bytes = queue_b.get_audio_bytes(&boops[0].blob_hash).await.unwrap();
	assert_eq!(downloaded_bytes, dummy_audio);

	// Bob marks as listened (tombstone)
	queue_b.mark_listened(&boops[0].id).await.unwrap();

	// Alice should eventually see the tombstone and GC the boop
	let mut alice_sees_empty = false;
	for _ in 0..20 {
		// get_pending_boops on Alice's side should trigger GC once tombstone is synced
		let alice_boops = queue_a.get_pending_boops().await.unwrap();
		if alice_boops.is_empty() {
			// Check if it was actually deleted or just not listed.
			// The doc entries count should have decreased.
			alice_sees_empty = true;
			break;
		}
		tokio::time::sleep(Duration::from_millis(500)).await;
	}
	assert!(alice_sees_empty, "Alice should have garbage collected the boop");
}
