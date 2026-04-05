# Boop
A cosy peer-to-peer voice note booper. 

No cloud, no accounts. Boop lets share voice messages with a friend, from your computer to theirs.

## The Idea
You shouldn't need a facebook or a google to chat to your friends. 

We once had copper wires between every home, and if we were bored at home, we'd boop a friend's house to see if they were bored at home too. But spam calls and smartphones killed that. Boop is bringing booping back.

## Cosy stack
Our friends that made this possible:
- **[Iroh](https://iroh.computer/)**: Fancy peer-to-peer networkinging magic.
- **[Tauri](https://tauri.app/)**: The app wrap that lets us use web platform magic to boop some audio.

### The Protocol
- `iroh-blobs`: Explicitly downloads and stages heavy WebM audio payloads via content-addressed blob hashing. 
- `iroh-docs`: Leverages CRDT (Conflict-free Replicated Data Type) document synchronization to instantly gossip lightweight JSON metadata about messages over the network.


More detail in the [Boop Blob Sync Design](docs/boop_blob_sync_design.md)

## Development
To run Boop locally, ensure you have the Rust toolchain setup. You can isolate local instances using environment variables to test P2P connections on the same machine!

```bash
# Run Instance A
cargo tauri dev

# Run Instance B alongside it natively on custom local paths
BOOP_DATA_DIR=/tmp/boop_b cargo tauri dev
```
