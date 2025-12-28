# nerve-core

NERVE Core Daemon — Local, ultra-low-latency protocol runtime

## Overview

nerve-core is the reference daemon implementation for the NERVE protocol.

It provides a local-first, ultra-low-latency runtime that accepts framed protocol messages over Unix Domain Sockets, dispatches them deterministically, and manages request lifecycles with robust cancellation and error handling.

This repository intentionally contains no business logic (search relevance, AI reasoning, agents). It exists solely to provide a stable, testable execution substrate for higher-level components.

## Design Goals
- Protocol correctness first
- Deterministic behavior
- Safe ownership and lifecycle management
- Real IPC, not mocks
- Minimal surface area
- Testability over cleverness

## Non-Goals (v0.1.0)
- ❌ Multiple concurrent clients
- ❌ Async runtime
- ❌ Network sockets (TCP)
- ❌ Search relevance logic
- ❌ AI / agent execution
- ❌ UI integration

These are intentionally handled above this layer.

## Architecture
```
Client
  │
  │  (Unix Domain Socket)
  ▼
┌─────────────────────┐
│     nerve-core      │
│                     │
│  ┌───────────────┐  │
│  │ FrameReader   │  │  ← framed IPC
│  └───────────────┘  │
│           │         │
│           ▼         │
│  ┌───────────────┐  │
│  │ Dispatch      │  │  ← pure logic
│  └───────────────┘  │
│           │         │
│           ▼         │
│  ┌───────────────┐  │
│  │ RequestTable  │  │  ← lifecycle & cancel
│  └───────────────┘  │
│                     │
└─────────────────────┘
```
All protocol framing, encoding, decoding, and lifecycle rules are defined in the separate nerve-protocol crate.

## Repository Structure

```
nerve-core/
├── src/
│   ├── main.rs        # process bootstrap only
│   ├── server.rs      # socket setup and accept loop
│   ├── connection.rs  # per-connection runtime loop
│   ├── dispatch.rs    # pure message dispatch logic
│   └── lib.rs         # exports for tests
│
├── tests/
│   ├── ping.rs
│   ├── cancel.rs
│   ├── lifecycle.rs
│   └── error_handling.rs
│
└── Cargo.toml
```

## Supported Message Types (v0.1.0)

| Message Type   | Behavior                 |
|----------------|--------------------------|
| PING           | Echo reply               |
| SEARCH_QUERY   | Stub result (placeholder)|
| CANCEL         | Cancels in-flight request|
| Unknown types  | Ignored safely           |

Payload semantics are intentionally opaque at this layer.

## Testing Strategy

nerve-core uses real Unix Domain Sockets for integration testing.

Test coverage includes:
- Unit tests for dispatch behavior
- End-to-end IPC tests
- Request lifecycle validation
- Cancellation semantics
- Malformed input handling
- Partial frame handling
- Connection teardown safety
- Rapid connect/disconnect scenarios

No mocks. No fake transports.

## Running the Daemon

```
cargo run
```

The daemon listens on `/tmp/nerve.sock`.

(v0.1.0 supports a single client connection.)

## Versioning

This repository follows semantic versioning.

Current version: **v0.1.0**

This version guarantees:
- frozen protocol usage
- stable dispatch semantics
- tested IPC behavior

Breaking changes will increment the minor or major version.

## Relationship to Other Repositories

- nerve-protocol: Defines wire format, codec, framing, lifecycle rules.
- nerve-search-adapter (future): Implements real search integration.
- Agent runtimes / browser integrations (future): Built on top of nerve-core.

## Philosophy

This daemon is intentionally boring.

Reliability, correctness, and testability matter more than features at this layer. All complexity belongs above this boundary.

## License


## Status

Stable foundation. Actively building higher layers.
