The `src/api` module in the distributed-fs project is responsible for handling the gRPC API interactions for the distributed storage network. It defines the `MyApi` struct, which implements the `Service` trait for handling `Put` and `Get` requests from clients. This module acts as the interface between the gRPC client and the underlying Distributed Hash Table (DHT) for storing and retrieving file metadata and content.

Here's a breakdown of its components:

### `MyApi` Struct

The `MyApi` struct holds the necessary communication channels to interact with the DHT:
- `api_req_sender`: An `mpsc::Sender` to send `DhtRequestType` messages (GetRecord or PutRecord) to the DHT.
- `api_res_receiver`: An `Arc<Mutex<broadcast::Receiver<DhtResponseType>>>` to receive responses from the DHT.

### `put` RPC

The `put` RPC handles file and metadata uploads from clients. It's a streaming endpoint, meaning clients can send metadata and file chunks in a single request stream.

**Key functionalities:**
- **Public Key Extraction**: Extracts the client's public key from the request metadata for authentication and authorization.
- **Metadata Validation**: Verifies the signature of the metadata using `secp256k1` to ensure the integrity and authenticity of the upload request. It also validates the `ApiEntry` to ensure that small files are stored inline.
- **File Chunk Processing**: For each file chunk received, it calculates the CID (Content Identifier) using SHA256 and verifies it against the provided CID. If valid, the chunk is stored in a local cache (`./cache/`).
- **DHT Interaction**: After processing all metadata and file chunks, it sends a `DhtPutRecordRequest` to the DHT via `api_req_sender` to store the file's metadata. It then awaits a `DhtPutRecordResponse` to confirm the operation.

### `get` RPC

The `get` RPC handles file and metadata downloads. It's also a streaming endpoint, allowing the server to stream file content or metadata back to the client.

**Key functionalities:**
- **Public Key Extraction**: Similar to `put`, it extracts the client's public key.
- **Signature Verification**: Verifies the signature provided in the `GetRequest` against the requested `location` and `public_key` to ensure the client has permission to access the data.
- **DHT Interaction**: Sends a `DhtGetRecordRequest` to the DHT to retrieve the `Entry` metadata associated with the requested `location`.
- **Access Control**: Checks if the requesting `public_key` has access to the `Entry` using the `user_has_access` function.
- **Download Logic**:
    - If `request.download` is true, it spawns a new task to:
        - Resolve the CIDs required for the download using `resolve_cid`.
        - Identify which CIDs are not locally cached and need to be downloaded from other peers using `download_data`.
        - Stream the file content back to the client using `download_file`.
    - If `request.download` is false, it returns the metadata (children of the requested location) to the client.

### Helper Functions (in `mod.rs` and `utils.rs`)

- **`download_data(location: String, entry: &Entry) -> Vec<Vec<String>>` (mod.rs)**:
    - Resolves the CIDs for the given location and entry.
    - Filters out CIDs that are already present in the local cache.
    - Splits the remaining CIDs into requests based on `MAX_REQUEST_SIZE`.
    - Returns a vector of vectors of CIDs to be downloaded from other peers.

- **`user_has_access(entry: Entry, public_key: PublicKey) -> (bool, Entry)` (mod.rs)**:
    - Checks if the given `public_key` has access to the `entry`.
    - Returns `true` if the entry is public or if the `public_key` is in the `read_users` list.

- **`validate_metadata_entry(entry: &ApiEntry) -> Result<(), String>` (mod.rs)**:
    - Validates the `ApiEntry` to ensure that files smaller than `MAX_DHT_STORED_CHUNKS` have their data stored inline.

- **`get_location_key(input_location: String) -> Result<(Key, String, String), String>` (utils.rs)**:
    - Parses an input location string to extract the `Key`, the relative `location` within the entry, and the `signature`. The key is identified by a part starting with "e_".

- **`resolve_cid(location: String, metadata: Vec<Children>) -> Result<Vec<Children>, String>` (utils.rs)**:
    - Given a `location` and a list of `Children` metadata, it returns the `Children` that correspond to the requested location. This is used to determine which file chunks (CIDs) are part of the requested file or directory.

- **`download_file(location: String, entry: Entry, tx: mpsc::Sender<Result<GetResponse, Status>>)` (utils.rs)**:
    - Streams the content of the requested file(s) to the client.
    - It first checks if the file data is stored inline within the metadata.
    - If not inline, it reads the file chunks from the local cache (`./cache/`) and sends them to the client via the provided `mpsc::Sender`.

- **`get_cids_with_sizes(items: Vec<Children>) -> Vec<(String, i32)>` (utils.rs)**:
    - Extracts CIDs and their corresponding sizes from a list of `Children` items.
    - Filters out CIDs that are smaller than `MAX_DHT_STORED_CHUNKS` (as these are stored inline).

- **`split_get_file_request(mut cids: Vec<(String, i32)>) -> Vec<Vec<String>>` (utils.rs)**:
    - Takes a list of CIDs with their sizes and splits them into smaller requests, ensuring that the total size of CIDs in each request does not exceed `MAX_REQUEST_SIZE`. This is to optimize requests to other peers for file chunks.