# Copilot instructions for distributed-fs

## Commands

- Rust build: `cargo build`
- Rust test: `cargo test`
- Single Rust test: `cargo test test_signatures -- --exact`
- Rust formatter: `cargo fmt` / `cargo fmt --check` (uses `rustfmt.toml`, 4-space tabs)
- Rust node launcher: `cargo run --bin tcp-chat -- api 127.0.0.1`, `cargo run --bin tcp-chat -- storage 127.0.0.1`, `cargo run --bin tcp-chat -- gen-keypair`
- P2P chat demo: `cargo run --bin chat-p2p`
- Client setup: `cd client && npm install`
- Client dev: `cd client && npm run dev`
- Client build: `cd client && npm run build`
- Client start: `cd client && npm run start`

## Architecture

- The Rust crate is the backend for the distributed storage network; `build.rs` bootstraps protobuf generation with `tonic-build`, and `src/main.rs` launches either an `ApiNode` or `StorageNode`.
- `src/node/mod.rs` is the node entrypoint layer: `Node` wraps `ApiNode`/`StorageNode`, creates them, and dispatches `run()`.
- `src/swarm.rs`, `src/behaviour.rs`, and `src/event_loop.rs` wire the libp2p swarm: Kademlia + mDNS + request-response with a custom JSON codec over the `/file-exchange/1` protocol.
- `src/api/mod.rs` implements the tonic `Service` gRPC API. `Put` and `Get` verify secp256k1 signatures, talk to the DHT through channels, and stream chunk data from the local cache.
- `src/entry.rs` holds the serialized entry model shared across the DHT, gRPC layer, and file-selection logic.
- `build.rs` compiles `proto/api.proto` into Rust types and adds serde derives to generated messages.
- The TypeScript client in `client/src/client.ts` loads the same API proto, builds directory metadata, hashes file chunks into CIDs, and streams uploads/downloads to the API node.

## Conventions

- Keep `proto/api.proto` and `client/proto/api.proto` in sync; both sides depend on the same service and message shapes.
- Chunking thresholds are centralized in `src/constants.rs`: 256 KiB chunks, 512 KiB request batches, and a 1 KiB inline/DHT threshold.
- Small file content is stored inline in metadata; larger content is chunked and cached on disk under `./cache/`.
- gRPC requests expect a `public_key` metadata header, and signatures are validated before upload/download access is processed.
- File integrity is based on SHA-256 CIDs computed from chunk contents.
- The client uses TypeScript strict mode and nodemon watches both `src` and `proto`.
