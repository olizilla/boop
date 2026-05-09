# Boop
A cosy peer-to-peer voice note booper.

No cloud, no accounts. Boop lets share voice messages with a friend, from your computer to theirs.

We once had copper wires between every home, and if we were bored at home, we'd boop a friend's house to see if they were bored at home too. But spam calls and smartphones killed that. Boop is bringing booping back.

> Warning: this is for fun. it is early days. it may work. no promises.

## Cosy stack

Friends that made this possible:
- **[Iroh](https://iroh.computer/)**: Fancy peer-to-peer networkinging.
- **[Tauri](https://tauri.app/)**: The app wrap that lets us use the web platform for our UI.
- **[Rodio](https://github.com/RustAudio/rodio)**: rust audio beebop.

## How it Works:

1. you record a voice note in the UI, a boop, encoded as a WAV.
2. the boop is passed to the Rust backend and transcoded to FLAC to take up less space.
3. a hash of the boop is shared with your friend via `iroh-docs`.
4. your friend fetches the audio from you using the hash and `iroh-blobs`.
5. they send a "listened" receipt, and both boopers delete the audio to keep things neat.
6. if they chillin too, they might boop you back.

More detail in the [Boop Sync doc](docs/boop_blob_sync_design.md)

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
