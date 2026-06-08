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

1. create an invite code and send it to a friend to connect.
2. record a voice note in the UI, a boop, encoded as a WAV.
3. the boop is passed to the Rust backend and transcoded to FLAC to take up less space.
4. a hash of the boop is shared with your friend via [`iroh-docs`](https://docs.iroh.computer/protocols/documents#documents).
5. your friend fetches the audio driectly from you using the hash and [`iroh-blobs`](https://docs.iroh.computer/protocols/blobs).
6. they send a "listened" receipt, and both boopers delete the audio to keep things neat.
7. if they chillin, they might boop you back.

More detail in the [Boop Sync doc](docs/boop_sync-design.md)

## Running on mac

_macOS complains the app is "damaged" and should be moved to the trash._

This app isn't signed yet. An Apple developer license costs $99/yr. So for now it's gonna throw an error on mac that we need to work around. You can right click on the app and choose open to side step the error:

1. Drag **Boop** to your **Applications** folder.
2. **right-click** (or Control-click) `Boop.app`, and choose **Open**.
3. Click **Open** on the confirmation dialog to whitelist the app.

To tell macOS to take Boop.app off the naughty list run this in Terminal.app:
```bash
xattr -cr /Applications/Boop.app
```

## Development

With Node.js and Rust installed, use `npm` to install the dependencies.

```bash
npm install

# on linux, run setup to fetch needed OS deps like `libwebkit2gtk-4.1-dev` etc.
npm run setup
```

Use `npm run tauri dev` to run the application locally. Isolate multiple local instances using BOOP_DATA_DIR env var.

```bash
# Run Instance A
npm run tauri dev

# Run Instance B alongside it natively on custom local paths
BOOP_DATA_DIR=/tmp/boop_b npm run tauri dev
```

## Deployment

Create a semver version tag and push it to github.
A draft release with installers for linux, mac and windows is created automatically.

```bash
npm version patch
git push origin main --tags
```

👉☎️👈
