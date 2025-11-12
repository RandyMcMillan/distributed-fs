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

# Client Usage

The client interacts with the `api_node` via a gRPC interface. You can use the provided TypeScript client to upload directories and download files.

## Setup

1.  Navigate to the `client` directory: `cd client`
2.  Install dependencies: `npm install`

## Uploading a Directory

To upload a directory, modify the `uploadDirectory` function call in `client/src/client.ts` with the path to the directory you wish to upload.

Example (uploading a directory named `my_files` located in the project root):

```typescript
// client/src/client.ts
uploadDirectory("../my_files");
```

Then, run the client:

```bash
npm run client
```

## Downloading a File

To download a file, modify the `downloadFile` function call in `client/src/client.ts` with the appropriate `location` and `sig` (signature) of the file you wish to download. The downloaded file will be saved in the `./download` directory relative to the client.

Example:

```typescript
// client/src/client.ts
downloadFile(); // Ensure the location and sig are correctly set within the function
```

Then, run the client:

```bash
npm run client
```

**Note on Server URL:** The client currently connects to a hardcoded `SERVER_URL` (e.g., `192.168.0.248:50051`) defined in `client/src/client.ts`. Ensure this matches the address of your running `api_node`. You may need to modify this value in `client/src/client.ts` to point to your `api_node`.
