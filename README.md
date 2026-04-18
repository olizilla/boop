# Boop
A cosy peer-to-peer voice note booper.

No cloud, no accounts. Boop lets share voice messages with a friend, from your computer to theirs.

We once had copper wires between every home, and if we were bored at home, we'd boop a friend's house to see if they were bored at home too. But spam calls and smartphones killed that. Boop is bringing booping back.

> Warning: this is for fun. it is early days. it may work. no promises.

## Cosy stack
Friends that made this possible:
- **[Iroh](https://iroh.computer/)**: Fancy peer-to-peer networkinging magic.
- **[Tauri](https://tauri.app/)**: The app wrap that lets us use web platform to boop some audio.

## How it Works:
1. You record a voice message. The audio is stored locally as a .webm file.
2. The application creates a JSON record for the message with the hash
  of the audio file.
3. This record is synced with your friends's device using `iroh-docs`, a
  a CRDT-based document synchronization system.
4. Your friends's booper detects the new record and fetches the audio from you using `iroh-blobs`.
5. Once the message is listened to, a "listened" receipt is sent back, and both boopers candelete the audio file to keep things neat.

More detail in the [Boop Blob Sync doc](docs/boop_blob_sync_design.md)

## Development
To get started with Boop, ensure you have Node.js and Rust installed, then run the setup script to install system dependencies and project packages:

```bash
npm run setup
```

To run the application locally, you can use `npm run tauri dev`. You can even isolate local instances using environment variables to test P2P connections on the same machine!

```bash
# Run Instance A
npm run tauri dev

# Run Instance B alongside it natively on custom local paths
BOOP_DATA_DIR=/tmp/boop_b npm run tauri dev
```
