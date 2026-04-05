# Boop Blob Sync Design Document

This design utilizes `iroh-docs` for metadata CRDT sync and `iroh-blobs` for heavy-duty audio payload routing. 

## Storage & Document Structure
Each conversation (friendship) shares exactly **1 Iroh Document**. Both users (A and B) subscribe and write to this single doc. Iroh's CRDT engine resolves concurrent edits natively using built-in timestamping and AuthorIDs.

Documents store structured JSON Metadata in keys, whereas the actual `.webm` files are placed exclusively in the `iroh-blobs` content-addressed storage (CAS) engine.

## Lifecycle of a Boop

### 1. Recording (User A)
- User A records a new voice note. The raw audio `.webm` buffer is sent to Rust.
- **Blob Storage:** Rust directly inserts the audio into the local `iroh-blobs` backend using standard storage (`store.add_bytes()`). This produces a BLAKE3 `BlobHash`.
- **Metadata Notification:** Rust crafts a JSON payload referencing this new boop: 
  ```json
  { "id": "uuid-123", "blob_hash": "bafkr..." }
  ```
- **KV Insert:** The JSON payload is inserted into the shared Iroh Doc using a sortable composite key: `boops/<timestamp>-<uuid>`. 
  - (Because keys are lexicographically ordered, ordering by timestamp makes fetching "next-to-play" naturally trivial for UI).

### 2. Discovering & Fetching (User B)
- Because `iroh-docs` is replicating the document via `gossip` continuously, User B detects the new `boops/timestamp...` key almost instantaneously.
- User B parses the metadata to find the `blob_hash`.
- **Eager Fetching:** Standard `iroh-docs` sync does **not** download referenced blobs automatically. User B's node spins up an instance of `iroh_blobs::api::downloader::Downloader` in the background. 
- Using this `Downloader`, User B iterates through the 3 most recently created offline boops and `.download()`s the BlobHashes from User A eagerly.
- While downloading, User B's UI reflects "Downloading..."; once it finishes, it switches to "Tap to Play" (1 in, 1 out buffer).

### 3. Listening & Garbage Collection (Tombstoning)
Garbage collection is necessary to prevent audio files from infinitely expanding users' disks over time. 
Because of CRDT semantics, User B cannot simply delete an entry authored by User A without causing sync headaches. Instead, we use tombstones!

1. User B listens to the Boop.
2. User B writes a receipt key to the shared document: `listened/<uuid>`.
3. User B deletes the `.webm` payload from their local `iroh-blobs` storage.
4. User A's node observes the `listened/<uuid>` key pop up! 
5. User A realizes their message was read, so User A deletes the original `.webm` payload from their local `iroh-blobs` storage.
6. User A deletes the original `boops/*` entry, and deletes Bob's `listened/*` entry, scrubbing the history completely clean!
