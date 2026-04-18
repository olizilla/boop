# Boop Event Architecture

This document describes the reactive, event-driven architecture of the Boop application. The system eliminates manual polling in favor of a robust streaming pipeline spanning from the P2P networking layer (Iroh) all the way up to the frontend UI (SolidJS).

## System Flow: Iroh -> Core -> Tauri -> Solid

The architecture is divided into strictly decoupled layers. Each layer is responsible for translating events into the semantic domain of the layer above it, ensuring unidirectional data flow and highly predictable UI state updates.

### 1. `iroh` (Networking & Database Layer)
The foundation of Boop is built on `iroh-docs` and `iroh-blobs`.
* **Docs (Metadata)**: Small JSON structs representing Boops are synchronized automatically across peers via CRDT-based documents.
* **Blobs (Audio)**: Large raw audio blobs (`WebM`/`Opus`) are fetched independently by explicitly exchanging Blake3 Hashes.
* **Handshakes**: Core endpoints locate friends securely using `ALPN` handshake side-channels over QUIC streams.

### 2. `boop-core` (State Management & Event Engine)
The core Rust library, centered around `BoopEngine`, abstracts Iroh's complex async P2P mechanics into a clean, semantic internal event bus.
* `IrohManager` acts as the low-level wrapper around Iroh endpoints and document sync.
* Background workers inside `BoopQueue` listen to `iroh_docs::engine::LiveEvent`. 
* When remote document entries are inserted (`InsertRemote`), `BoopEngine` receives them. It guarantees robust resolution of the associated chunk blobs and broadcasts semantic `CoreEvent` enumerations across a `tokio::sync::broadcast` channel.

### 3. `src-tauri` (IPC Bindings)
The Tauri bridge acts strictly as a dumb proxy.
* A background Tokio task subscribes to the `BoopEngine`'s broadcast channel.
* When a `CoreEvent` fires, Tauri securely serializes it into an IPC JSON envelope (`app.emit("core-event", event)`).
* Exposes simple asynchronous commands like `frontend_ready` to prompt the engine to flush current network state.

### 4. `SolidJS` (Reactive UI)
The frontend UI is a deterministic representation of the events received from the IPC bus.
* Uses lightweight global stores (`pendingBoops` handled by Solid's deep `createStore` Proxy).
* Events are handled monotonically. For instance, receiving new events mutates precise node locations in the store using `produce(draft => ...)` triggering localized, highly efficient DOM updates without large reflows.

---

## Expected Happy Path Event Sequence

Below is the execution flow, structured chronologically from starting the UI, establishing friend connections, and naturally dispatching Boops.

### 1. App UI Initialization
   - The UI finishes mounting and executing `App.jsx` and establishes the IPC event listener `await listen('core-event', ...)`.
   - The frontend calls `invoke('frontend_ready')` to explicitly notify the backend that the webview is ready.
   - `BoopEngine` intercepts this and locally compiles the entire node state natively (collating iterating `address_book` entries alongside gathering deep pending document sizes traversing the `BoopQueue` paths).
   - `BoopEngine` streams `CoreEvent::StateSnapshot` down to the UI bridging the gap. By updating the native `friends` array and hydrating the deep nested SolidJS `pendingBoops` store iteratively using `produce()`, `App.jsx` instantaneously shifts to `MODE_FRIEND` seamlessly with all offline activity restored.

### 2. Handshaking & Adding a Friend
   - A user opens `MODE_ADD_FRIEND` and inputs a remote peer's base32 encoded `EndpointId` (which executes on Iroh's stack fundamentally mapping to a 32-byte cryptographic `PublicKey`).
   - Frontend invokes `invoke('add_friend')`. `BoopEngine` leverages `friends.json` natively resolving and creating a distinct empty CRDT document tailored purely via `BoopQueue` for this duo.
   - Node A leverages Iroh ALPN networking (`ALPN_BOOP_HANDSHAKE`) natively executing `dial_friend`. It establishes a secure background QUIC stream directly firing a JSON envelope carrying Node A's `EndpointId` and its generated `DocTicket`.
   - Sitting idle, Node B's nested `BoopHandshakeHandler` Protocol recognizes the incoming QUIC bi-direction request securely. It dynamically reads the ticket, parsing it gracefully and inserting the reciprocal `Friend` identity synchronously without polling UI interaction!
   - Both engines trigger internal streams cascading a `CoreEvent::FriendAdded` mapping securely down the pipeline into the local UI tracking loops updating lists flawlessly.

### 3. Record & Send (Node A)
   - Node A clicks "Boop" and records audio.
   - Frontend calls `invoke('send_boop', { audio_bytes })`.
   - `BoopEngine` adds audio to `iroh-blobs` natively.
   - `BoopEngine` creates a `PendingBoopDto` metadata struct with the blob hash and natively `set_bytes` into the shared Iroh `Doc`.

### 4. Sync & Fetch (Node B)
   - Node B's `BoopQueue` background listener natively receives `LiveEvent::InsertRemote`.
   - `BoopEngine` attempts to retrieve the raw audio chunk.
   - Since the audio chunk may lag behind the lightweight metadata sync, `BoopEngine` enters a rapid backoff `fetch_blob` loop specifically requesting the raw bytes from Node A's endpoint.

### 5. Event Up-streaming (Node B)
   - Once metadata safely resolves, `BoopEngine` emits `CoreEvent::BoopReceived` to the UI with `is_ready: false`.
   - **UI Action**: Renders `<span style="color: yellow">fetching boop...</span>`.
   - Once the audio chunk successfully completes fetching, `BoopEngine` emits `CoreEvent::BoopReady`.
   - **UI Action**: Mutates the proxy state `is_ready: true`. Renders "tap to play" and adds the visual `.glow-effect`.

### 6. Playback & Cleanup (Node B)
   - Node B clicks to play.
   - Frontend invokes `invoke('get_audio_bytes')`. It streams natively and plays locally over `Audio`.
   - On completion, frontend invokes `invoke('mark_listened')`.
   - Node B's engine places a CRDT `tombstone` in the `listened/` path of the shared Doc, signaling Node A to garbage collect the entry.

---

## Error States & Handling

* **Missing / Dropped Metadata Chunks**: If `LiveEvent::InsertRemote` fires but the raw chunk blob hasn't arrived immediately locally, `BoopEngine` intercepts the miss and iteratively triggers `fetch_blob` backoffs up to 5 times rather than silently skipping the boop.
* **Component Remounts / HMR Race Conditions**: During development hot-reloads, SolidJS might unmount and replace `App.jsx` dynamically, causing the fast `frontend_ready` `invoke()` to miss the rapidly incoming `StateSnapshot`. **Solution**: A hard refresh (Cmd+R) resets the window execution environment fully, assuring listener registrations preceding snapshot dispatches.
* **Serde JSON Mismatches**: Given the external tagging structure of Enums combined with `#[serde(rename_all = "camelCase")]`, variants are camel-cased but individual properties inside stay snake-case. UI parsers safely expect `payload.friend_id` over standard JS camel convention `payload.friendId` to avoid `undefined` silent mutation bugs.

---

## Writing Tests

The application boasts decoupled components intentionally scoped to allow robust testing across the entire pipeline without spinning up UI simulators.

### 1. `boop-core` (Rust Integration Tests)
Tests in `boop-core/src/tests.rs` execute fully local headless validation.
* Utilize `tempfile` to sandbox `iroh-blobs` per-test.
* Focus purely on the structural stability of the offline fetch pipeline. Tests can explicitly fabricate metadata blocks locally and assert how `fetch_blob` natively rectifies the networking gap safely.
* Run via: `cargo test -p boop-core`

### 2. `Tauri` (IPC Handlers)
For broader system integration beyond `core`, typical mock suites reside internally within standard `cargo test` runs invoking Tauri builders and `plugin` pipelines. Because Tauri layers here act as simple pass-throughs, they require almost zero complex business-logic mocking!

### 3. `SolidJS` (UI Reactivity Vitest)
Frontend UI tests are powered by `vitest` and `@solidjs/testing-library`.
Instead of utilizing heavy end-to-end browser suites, we hook entirely into the `tauri-bridge.js` file:
* Calling `mockEmit(payload)` mimics Rust `StateSnapshot` natively routing exact JSON properties into Solid's reactive model.
* Validates complex DOM node updates (such as asserting proxy reactivity handles un-hydrated arrays) rendering `fallback` loaders seamlessly.
* Run via: `npm run test` or `npx vitest run src/App.test.jsx`.
