# Distributed Storage Network with libp2p

A decentralized Rust storage network built on `libp2p`.

## Build

```bash
cargo build
```

## Run

The Rust binaries are the supported path; `./client` is legacy TypeScript/gRPC code kept for reference only.

### Node entrypoint

The main binary is `gnostr-p2p` and accepts a node role plus host/IP:

```bash
cargo run --bin gnostr-p2p -- --role api --addr 127.0.0.1
cargo run --bin gnostr-p2p -- --role storage --addr 127.0.0.1
```

### P2P client

```bash
cargo run --bin client -- --path ./my_files
cargo run --bin client -- --download <location> <signature>
```

### Chat demo

```bash
cargo run --bin chat_p2p
cargo run --bin chat_p2p /ip4/127.0.0.1/tcp/<port>
```

### Full demo

Run the end-to-end demo script to build the workspace, start nodes, upload sample data, and download it back:

```bash
bash scripts/run.sh
```

## API docs

Generate the Rust API docs from the inline `rustdoc` comments:

```bash
cargo doc --no-deps --open
```

Use `cargo doc --document-private-items` if you want the private/internal modules included too.

## Inline code docs

Add inline documentation with `///` on items and `//!` at module roots. These comments are what `cargo doc` uses to build the rendered docs, so keep them close to the code they describe.

## Project structure

- `src/` contains the Rust implementation
- `src/api/` contains the gRPC API layer
- `src/node/` contains node orchestration
- `src/bin/` contains runnable binaries
- `proto/` contains protobuf definitions
