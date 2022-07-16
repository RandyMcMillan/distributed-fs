# Distributed Storage Network with libp2p

A decentralized network built in rust with libp2p to provide users with performant and secure data storage

# How it works

This peer-to-peer network of nodes is built using the [rust-libp2p](https://github.com/libp2p/rust-libp2p) package.

When you upload a file to a `api_node` it gets chunked and the chunks get send to different `storage_node`'s
The metadata of the entries (filenames, folder structure, ...) get stored in a kademlia store (key-value store)

Once you want to download a file the content identifiers get compared to check if the file is complete

The clients interact with the peers via a gRPC server hosted on `api_node`'s,
`api_node`'s interact with `storage_node`'s via a request_response protocol.

# Running a node

-   Clone this repository `git clone https://github.com/MatsDK/distributed-fs.git`
-   Run the program: `cargo run (api|storage) (url)`

```
$ cargo run api 127.0.0.1

$ cargo run storage 127.0.0.1
```
