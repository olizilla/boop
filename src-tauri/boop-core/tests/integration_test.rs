use boop_core::{IrohManager, iroh_boops::BoopQueue, address_book::AddressBook};
use std::time::Duration;
use serial_test::serial;


#[tokio::test]
#[serial]
async fn test_handshake_and_boop_sync() {
	let dir_a = tempfile::tempdir().unwrap();
	let dir_b = tempfile::tempdir().unwrap();

	let ab_a = std::sync::Arc::new(tokio::sync::Mutex::new(AddressBook::new()));
	let ab_b = std::sync::Arc::new(tokio::sync::Mutex::new(AddressBook::new()));

	let (iroh_a, _rx_a) = IrohManager::new(dir_a.path().to_path_buf(), true, ab_a.clone()).await.unwrap();
	let (iroh_b, mut rx_b) = IrohManager::new(dir_b.path().to_path_buf(), true, ab_b.clone()).await.unwrap();

	// Bob adds Alice as a friend so he accepts her handshake
	ab_b.lock().await.add_friend("Alice".to_string(), iroh_a.endpoint_id);

	// Manually connect nodes using EndpointTicket
	let ticket_b = iroh_b.endpoint_ticket().unwrap();
	iroh_a.connect_to_endpoint_ticket(&ticket_b).await.unwrap();

	// Alice creates a queue
	let queue_a = BoopQueue::new(None, iroh_a.clone()).await.unwrap();
	let ticket = queue_a.ticket();

	// Alice dials Bob
	iroh_a.dial_friend(iroh_b.endpoint().addr(), ticket.clone()).await.expect("Failed to dial Bob");

	// Bob receives it
	tokio::time::timeout(Duration::from_secs(5), async {
		let (sender_id, doc_ticket) = rx_b.handshake_rx.recv().await.expect("Channel closed");
		assert_eq!(sender_id, iroh_a.endpoint_id);
		assert_eq!(doc_ticket, ticket);
	}).await.expect("Handshake receive timed out");
}

#[tokio::test]
#[serial]
async fn test_full_boop_lifecycle() {
	let dir_a = tempfile::tempdir().unwrap();
	let dir_b = tempfile::tempdir().unwrap();

	let ab_a = std::sync::Arc::new(tokio::sync::Mutex::new(AddressBook::new()));
	let ab_b = std::sync::Arc::new(tokio::sync::Mutex::new(AddressBook::new()));

	let (iroh_a, _rx_a) = IrohManager::new(dir_a.path().to_path_buf(), true, ab_a.clone()).await.unwrap();
	let (iroh_b, mut rx_b) = IrohManager::new(dir_b.path().to_path_buf(), true, ab_b.clone()).await.unwrap();

	// Bob adds Alice as a friend so he accepts her handshake
	ab_b.lock().await.add_friend("Alice".to_string(), iroh_a.endpoint_id);

	// Manually connect nodes using EndpointTicket
	let ticket_b = iroh_b.endpoint_ticket().unwrap();
	iroh_a.connect_to_endpoint_ticket(&ticket_b).await.unwrap();

	// Alice creates a queue
	let mut queue_a = BoopQueue::new(None, iroh_a.clone()).await.unwrap();
	let ticket = queue_a.ticket();

	// Alice dials Bob
	iroh_a.dial_friend(iroh_b.endpoint().addr(), ticket.clone()).await.expect("Failed to dial Bob");

	// Bob receives it and joins the queue
	let (_, ticket_b) = tokio::time::timeout(Duration::from_secs(5), rx_b.handshake_rx.recv())
		.await.unwrap().expect("Bob should receive a handshake");
		
	let queue_b = BoopQueue::new(Some(ticket_b), iroh_b.clone()).await.expect("Failed to join queue");

	// Alice sends a boop to Bob
	let dummy_audio = vec![0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
	queue_a.send_boop(dummy_audio.clone(), "audio/webm".to_string()).await.unwrap();
	
	// Bob should see the metadata eventually (gossip sync)
	let mut boops = Vec::new();
	for _ in 0..20 {
		boops = queue_b.get_pending_boops().await.unwrap();
		if !boops.is_empty() { break; }
		tokio::time::sleep(Duration::from_millis(200)).await;
	}
	assert_eq!(boops.len(), 1, "Bob should have received 1 boop metadata");
	assert!(!boops[0].is_ready, "Blob shouldn't be local yet");

	// Bob downloads the blob
	iroh_b.fetch_blob(&boops[0].blob_hash.to_string(), &iroh_a.endpoint_id.to_string()).await.unwrap();

	// Verify Bob now has it ready
	boops = queue_b.get_pending_boops().await.unwrap();
	assert!(boops[0].is_ready, "Blob should be local now");
	
	// Verify content
	let downloaded_bytes = queue_b.get_audio_bytes(boops[0].blob_hash).await.unwrap();
	assert_eq!(downloaded_bytes, dummy_audio);

	// Bob marks as listened (tombstone)
	queue_b.mark_listened(boops[0].id).await.unwrap();

	// Alice should eventually see the tombstone and GC the boop (sync)
	let mut alice_sees_empty = false;
	for _ in 0..20 {
		if queue_a.get_pending_boops().await.unwrap().is_empty() {
			alice_sees_empty = true;
			break;
		}
		tokio::time::sleep(Duration::from_millis(200)).await;
	}
	assert!(alice_sees_empty, "Alice should have garbage collected the boop");
}
