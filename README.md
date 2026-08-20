# nerve-core

Local AI daemon core for the Anvesha browser intelligence system.

## Overview

nerve-core is the local process that sits between the browser extension and the AI daemon. It accepts NERVE-framed messages over two transports — a Unix Domain Socket (UDS) for local tool integrations and a WebSocket server for the browser extension — dispatches them to the correct handler, manages request lifecycles, and enforces cancellation semantics.

It has no knowledge of search relevance, AI reasoning, or crawlers. That belongs to the layers above.

## Architecture

```
Browser Extension
      │
      │  NERVE frames over WebSocket (ws://127.0.0.1:9001)
      │  Sec-WebSocket-Protocol: anvesha-v1.<token>
      │  Origin: chrome-extension://<extension_id>
      ▼
┌─────────────────────────────────────────┐
│               nerve-core                │
│                                         │
│  ws_server          server              │
│  (WebSocket)        (UDS)               │
│       │               │                 │
│       └───────┬───────┘                 │
│               ▼                         │
│          dispatch_frame()               │  ← transport-agnostic
│               │                         │  ← Ping handled inline
│               │                         │  ← Cancel marks request
│               ▼                         │  ← SearchQuery → AI boundary
│         RequestTable                    │  ← per-connection, isolated
│                                         │
└─────────────────────────────────────────┘
      │
      │  (future)
      ▼
  AI Daemon
  ├── query understanding
  ├── search client (HTTPS → hosted API)
  ├── context builder
  └── inference / token streaming
```

Each accepted connection (UDS or WebSocket) runs in its own OS thread with an independent `RequestTable`. A misbehaving client cannot affect other connections.

## Design Goals

- Protocol correctness first
- Deterministic, non-blocking dispatch
- Connection isolation: one bad client ≠ daemon crash
- Real IPC in tests, no mocks
- Clean boundary between transport and AI daemon
- Minimal surface area
- Browser-reachable over WebSocket without any web server or third-party relay

## Non-Goals

- AI reasoning, search ranking, or crawler logic
- Async runtime (not needed at current scale)
- TLS (localhost-only; TLS would be for a hosted endpoint)
- Browser-specific code

## Security

nerve-core is only accessible to the browser extension and local tools:

| Control | Detail |
|---|---|
| **Bind address** | Always `127.0.0.1`, never `0.0.0.0` |
| **Origin check** | `Origin: chrome-extension://<ANVESHA_EXTENSION_ID>` |
| **Token auth** | Per-install secret at `~/.anvesha/token` (mode `0600`); passed via `Sec-WebSocket-Protocol: anvesha-v1.<token>` — never in the URL |
| **Payload limit** | `payload_length` validated before allocation; oversized frames rejected before any bytes are read |
| **Error responses** | 403 on auth failure — never reveals which check (origin vs. token) failed |

The token is generated with 32 bytes of OS randomness (`getrandom`) on first run, stored as 64-character hex, and reloaded on subsequent starts.

## Repository Structure

```
nerve-core/
├── src/
│   ├── main.rs           # process entry point — starts WS + UDS servers
│   ├── lib.rs
│   ├── config.rs         # Config struct; bind address, ports, token path
│   ├── auth.rs           # per-install token generation and persistence
│   ├── server.rs         # UDS accept loop, thread-per-connection, read_frame
│   ├── ws_server.rs      # WebSocket accept loop + auth handshake
│   ├── dispatch.rs       # pure transport-agnostic dispatch, AI daemon boundary
│   └── request_table.rs  # per-connection request lifecycle and cancellation
│
├── tests/
│   ├── helpers/
│   │   └── mod.rs                  # shared WebSocket test helpers
│   │
│   │  — WebSocket transport —
│   ├── ws_basic.rs                 # server startup, auth, ping roundtrip
│   ├── ws_security.rs              # auth rejection, malformed frames, bind addr
│   ├── ws_connections.rs           # concurrent clients, isolation, reconnect
│   ├── ws_protocol.rs              # NERVE semantics over WebSocket
│   │
│   │  — Unix Domain Socket —
│   ├── ping.rs                     # Ping round-trip
│   ├── ping_roundtrip.rs           # Ping over real UDS
│   ├── cancel.rs                   # Cancel semantics
│   ├── cancel_edge_cases.rs        # Cancel unknown / duplicate cancel
│   ├── cancel_marks_requests.rs    # RequestTable cancel behaviour
│   ├── concurrent_connections.rs   # Isolation: 2+ clients, malformed client, cross-cancel
│   ├── lifecycle.rs                # Connection and request lifecycle
│   ├── search_roundtrip.rs         # SearchQuery dispatch (no inline response)
│   ├── search_streaming.rs         # SearchQuery + Cancel connection stability
│   ├── search_cancel_mid_stream.rs # Cancel of pending SearchQuery
│   ├── search_worker_routing.rs    # Connection isolation proofs
│   ├── agent_task_lifecycle.rs     # AgentTask message acceptance
│   ├── error_handling.rs           # Malformed data, partial frames, rapid disconnect
│   ├── request_table.rs            # RequestTable unit tests
│   └── socket_read_frame.rs        # read_frame unit test
│
└── Cargo.toml
```

## Message Dispatch

`dispatch_frame()` is transport-agnostic — the same function handles both UDS and WebSocket connections. It never performs I/O; it returns a `DispatchAction` that the caller's connection loop translates into a write.

| Message          | Behaviour                                                                        |
|------------------|----------------------------------------------------------------------------------|
| `Ping`           | Echo reply with `FINAL` flag, same `request_id`                                  |
| `SearchQuery`    | Registered in `RequestTable` → `ForwardToAiDaemon` (AI daemon not yet built)    |
| `Cancel`         | Marks the request cancelled in `RequestTable`                                    |
| `AgentTaskStart` | Registers request                                                                |
| `AgentTaskEvent` | No-op (future: forwarded to AI daemon)                                           |
| `AgentTaskDone`  | Removes request from table                                                       |
| Unknown types    | Ignored safely                                                                   |

`SearchQuery` and `AgentTask*` carry their business logic in the AI daemon layer. nerve-core registers the request for cancellation tracking but does not generate a response itself.

## Request Lifecycle

```
SearchQuery received (UDS or WebSocket)
      ↓
insert(req_id) into RequestTable
      ↓
ForwardToAiDaemon(req_id)  ← AI daemon picks this up (future)
      ↓
[AI daemon streams SearchResult / AiToken frames back to client]
      ↓
remove(req_id) on completion

--- or ---

Cancel received
      ↓
cancel(req_id) in RequestTable
      ↓
AI daemon checks is_cancelled() before each emission
      ↓
remove(req_id)
```

Cancelling request A never affects request B, even on the same connection or on a different connection with the same request_id.

## WebSocket Protocol

nerve-core is purely a transport layer. NERVE frames travel unchanged inside WebSocket binary messages:

```
WebSocket binary message body
        │
        ▼
  NERVE frame (20-byte header + payload)
        │
        ▼
  dispatch_frame()
```

One WebSocket binary message = one NERVE frame. The JSON structure of payloads, message types, flags, and request IDs are defined by the `nerve` protocol crate and are not modified by nerve-core.

The browser extension must present:
- `Origin: chrome-extension://<ANVESHA_EXTENSION_ID>` — checked against the configured extension ID
- `Sec-WebSocket-Protocol: anvesha-v1.<token>` — checked against `~/.anvesha/token`

Both checks happen at the WebSocket handshake. No frames are processed on a rejected connection.

## Running

```sh
cargo run
```

On first run:
- Generates a random token and writes it to `~/.anvesha/token` (mode `0600`)
- Starts the WebSocket server on `127.0.0.1:9001`
- Starts the UDS server on `/tmp/nerve.sock`

On subsequent runs the existing token is reloaded and validated.

## Testing

```sh
cargo test
```

62 integration tests. No mocks, no fake transports — every test exercises the real server code over real sockets.

### WebSocket tests (25 tests)

| File | Coverage |
|---|---|
| `ws_basic.rs` | Server startup, valid token auth, Origin auth, Ping roundtrip, sequential pings |
| `ws_security.rs` | Wrong token → 403, missing token → 403, wrong origin → 403, missing origin → 403, correct origin wrong token → 403, oversized payload header rejected, bad magic rejected, truncated header rejected, bind addr is 127.0.0.1 |
| `ws_connections.rs` | 2 concurrent clients, 3 concurrent clients, bad frame closes one connection only, Cancel isolation between connections, disconnect/reconnect |
| `ws_protocol.rs` | request_id echo, FINAL/STREAM flags on Ping reply, SearchQuery connection stays open, Cancel connection stays open, AgentTask lifecycle, multiple pings with distinct IDs |

### UDS tests (37 tests)

- Ping round-trip (single and multiple)
- Concurrent connections (2 and 3 simultaneous clients)
- Connection isolation (malformed client A does not affect client B)
- Cancel semantics (unknown, duplicate, cross-connection)
- SearchQuery dispatch to AI boundary
- Interleaved SearchQuery and Ping on same connection
- Cancel of pending SearchQuery
- Request lifecycle and cleanup
- Malformed frames, partial frames, rapid connect/disconnect

## Dependencies

```toml
nerve-protocol = { path = "../nerve", package = "nerve" }
tracing = "0.1"
tracing-subscriber = "0.3"
tungstenite = "0.24"
getrandom = "0.2"
```

The `nerve` crate provides all wire format, codec, framing, and protocol validation. nerve-core does not redefine any protocol types.

`tungstenite` is the WebSocket implementation (blocking, thread-per-connection, no Tokio dependency).

`getrandom` is used for cryptographically secure token generation only.

## Relationship to Other Repositories

| Repository | Role |
|---|---|
| `nerve` | Wire format, codec, framing, message types |
| `nerve-core` | Local daemon core — this repository |
| AI daemon (future) | Query understanding, search client, inference |
| Browser extension (future) | Sends NERVE frames over WebSocket |
| Hosted search API (future) | Retrieval layer, crawler, Tantivy |

## Status

**v0.1.0 — Stage B: Browser WebSocket Transport complete.**

- ✅ Multiple simultaneous connections (UDS + WebSocket)
- ✅ Connection isolation (per-connection `RequestTable`)
- ✅ Request lifecycle and cancellation
- ✅ Clean AI daemon boundary (`ForwardToAiDaemon`)
- ✅ Transport-agnostic dispatch (`dispatch_frame` shared by UDS and WebSocket)
- ✅ WebSocket server on `127.0.0.1:9001`
- ✅ Origin + token authentication at the WebSocket handshake
- ✅ Per-install token at `~/.anvesha/token` (mode `0600`, `getrandom`)
- ✅ Oversized payload rejected before allocation
- ✅ Zero warnings under `clippy -D warnings`
- ⏳ AI daemon integration
- ⏳ Browser extension
