use boop_core::{IrohManager, iroh_boops::BoopQueue};
use std::time::Duration;
use serial_test::serial;

async fn robust_dial(from: &IrohManager, to: &IrohManager, ticket: String) -> anyhow::Result<()> {
	let addr = to.endpoint().addr();
	let mut last_err = None;
	
	for i in 0..5 {
		log::info!("Robust Dial: Attempt {} to connect to {}", i, to.endpoint_id);
		match from.dial_friend(addr.clone(), ticket.clone()).await {
			Ok(_) => return Ok(()),
			Err(e) => {
				log::warn!("Robust Dial: Attempt {} failed: {}", i, e);
				last_err = Some(e);
				tokio::time::sleep(Duration::from_millis(500)).await;
			}
		}
	}
	
	Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Dial failed after all retries")))
}

#[tokio::test]
#[serial]
async fn test_handshake_and_boop_sync() {
	let dir_a = tempfile::tempdir().unwrap();
	let dir_b = tempfile::tempdir().unwrap();

	let (iroh_a, _rx_a) = IrohManager::new(dir_a.path().to_path_buf(), true).await.unwrap();
	let (iroh_b, mut rx_b) = IrohManager::new(dir_b.path().to_path_buf(), true).await.unwrap();

	// Alice creates a queue
	let queue_a = BoopQueue::new(None, iroh_a.clone()).await.unwrap();
	let ticket = queue_a.ticket();

	// Alice dials Bob with retries
	robust_dial(&iroh_a, &iroh_b, ticket.clone()).await.expect("Failed to dial Bob after retries");

	// Bob receives it
	let result = tokio::time::timeout(Duration::from_secs(10), async {
		if let Some((sender_id, doc_ticket)) = rx_b.recv().await {
			assert_eq!(sender_id, iroh_a.endpoint_id);
			assert_eq!(doc_ticket, ticket);
			true
		} else {
			false
		}
	}).await.expect("Handshake receive timed out");
	
	assert!(result, "Handshake failed");
}

#[tokio::test]
#[serial]
async fn test_full_boop_lifecycle() {
	let dir_a = tempfile::tempdir().unwrap();
	let dir_b = tempfile::tempdir().unwrap();

	let (iroh_a, _rx_a) = IrohManager::new(dir_a.path().to_path_buf(), true).await.unwrap();
	let (iroh_b, mut rx_b) = IrohManager::new(dir_b.path().to_path_buf(), true).await.unwrap();

	// Alice creates a queue
	let mut queue_a = BoopQueue::new(None, iroh_a.clone()).await.unwrap();
	let ticket = queue_a.ticket();

	// Alice dials Bob with retries
	robust_dial(&iroh_a, &iroh_b, ticket.clone()).await.expect("Failed to dial Bob after retries");

	// Bob receives it and joins the queue
	let (_, ticket_b) = tokio::time::timeout(Duration::from_secs(10), rx_b.recv())
		.await.unwrap().expect("Bob should receive a handshake");
		
	// Bob might need a moment for the doc import to succeed if networking is busy
	let mut queue_b = None;
	for _ in 0..5 {
		match BoopQueue::new(Some(ticket_b.clone()), iroh_b.clone()).await {
			Ok(q) => {
				queue_b = Some(q);
				break;
			}
			Err(_) => tokio::time::sleep(Duration::from_millis(500)).await,
		}
	}
	let queue_b = queue_b.expect("Failed to join queue after retries");

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
	iroh_b.fetch_blob(&boop.blob_hash.to_string(), &iroh_a.endpoint_id.to_string()).await.unwrap();

	// Verify Bob now has it ready
	boops = queue_b.get_pending_boops().await.unwrap();
	assert!(boops[0].is_ready, "Blob should be local now");
	
	// Verify content
	let downloaded_bytes = queue_b.get_audio_bytes(boops[0].blob_hash).await.unwrap();
	assert_eq!(downloaded_bytes, dummy_audio);

	// Bob marks as listened (tombstone)
	queue_b.mark_listened(boops[0].id).await.unwrap();

	// Alice should eventually see the tombstone and GC the boop
	let mut alice_sees_empty = false;
	for _ in 0..20 {
		// get_pending_boops on Alice's side should trigger GC once tombstone is synced
		let alice_boops = queue_a.get_pending_boops().await.unwrap();
		if alice_boops.is_empty() {
			alice_sees_empty = true;
			break;
		}
		tokio::time::sleep(Duration::from_millis(500)).await;
	}
	assert!(alice_sees_empty, "Alice should have garbage collected the boop");
}
