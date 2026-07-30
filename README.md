# Distributed Storage Network with libp2p

A decentralized Rust storage network built on `libp2p`.

## Build

```bash
cargo build
```

## Run

### Node entrypoint

The main binary accepts a node role and host/IP:

```bash
cargo run -- api 127.0.0.1
cargo run -- storage 127.0.0.1
```

### P2P client

```bash
cargo run --bin client -- upload ./my_files
cargo run --bin client -- download <location> <signature>
```

### Chat demo

```bash
cargo run --bin chat_p2p
cargo run --bin chat_p2p /ip4/127.0.0.1/tcp/<port>
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
