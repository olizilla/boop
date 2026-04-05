use iroh::client::blobs::Client;
async fn test(client: Client) {
    let _ = client.delete_blob(iroh_blobs::Hash::from([0; 32])).await;
}
fn main() {}
