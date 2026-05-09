# Boop Networking Protocols

Boop uses custom protocols over QUIC (via Iroh) to manage peer-to-peer communication. These protocols are identified by their ALPN (Application-Layer Protocol Negotiation) strings.

## Overview

| Protocol | ALPN | Pattern | Lifecycle | Access Control |
| :--- | :--- | :--- | :--- | :--- |
| **Welcome** | `boop/wlcm` | One-shot | Ephemeral | Open (Token required) |
| **Handshake** | `boop/handshk` | Multiplexed | Persistent | Friends Only |
| **Presence** | `boop/prsnc` | Multiplexed | Persistent | Friends Only |

---

## 1. Welcome Protocol (`boop/wlcm`)

The Welcome protocol is used for initial onboarding between two peers via an Invite Ticket.

- **Pattern**: Single Request/Response.
- **Lifecycle**: Ephemeral. The connection is opened, the token is exchanged for a friend entry, and the connection is closed immediately.
- **Access Control**: **Open**. Any peer can initiate a connection to this ALPN. However, the responder will only accept the connection if the initiator provides a valid, pending `Invite Token`.
- **Flow**:
  1. Client dials `boop/wlcm`.
  2. Client sends 32-byte `Invite Token`.
  3. Server validates token against `pending_invites`.
  4. Server adds Client to `AddressBook`, sends ACK (`0x01`).
  5. Client receives ACK, adds Server to `AddressBook`.
  6. Client closes connection.

## 2. Handshake Protocol (`boop/handshk`)

The Handshake protocol is used to exchange Iroh Doc tickets between established friends.

- **Pattern**: Request/Response over a multiplexed connection.
- **Lifecycle**: **Persistent**. Connections are cached in the `IrohManager` connection pool and reused for subsequent handshakes (e.g., on app focus or startup).
- **Access Control**: **Friends Only**. The `ProtocolHandler` will reject any incoming connection if the remote `NodeId` is not already in the `AddressBook`.
- **Flow**:
  1. Client dials `boop/handshk` (or reuses cached connection).
  2. Client opens a new bidirectional stream.
  3. Client sends `HandshakePayload` (JSON with `PublicKey` and `DocTicket`).
  4. Server receives payload, joins the friend's Doc, sends ACK.
  5. Stream is closed, but the underlying QUIC connection remains in the pool.

## 3. Presence Protocol (`boop/prsnc`)

The Presence protocol manages real-time "active" status and provides immediate offline detection.

- **Pattern**: Multiple Request/Response over a multiplexed connection.
- **Lifecycle**: **Persistent**. This is the primary long-lived tunnel between friends.
- **Access Control**: **Friends Only**. Like the Handshake protocol, only known friends can connect.
- **Flow**:
  1. Client dials `boop/prsnc` (or reuses cached connection).
  2. When the app is focused/blurred, Client opens a new stream.
  3. Client sends 1 byte (`0x01` for Active, `0x00` for Backgrounded).
  4. Server updates UI and sends ACK.
- **Offline Detection**: The `IrohManager` monitors the `connection.closed()` future. Because this connection is persistent, if the peer's app crashes or their network drops, Boop detects the closure immediately and updates the friend's status to offline.

---

## Connection Management

All persistent connections are managed by the `IrohManager`'s connection pool. They are keyed by `(NodeId, ALPN)`. 

### Cleanup
If a connection is closed by either side (due to idle timeout, crash, or network loss), a background task automatically removes it from the pool using the connection's `stable_id()` to ensure only the stale instance is evicted.
