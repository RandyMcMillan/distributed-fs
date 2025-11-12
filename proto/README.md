The `proto` directory contains the Protocol Buffer definition files for the distributed-fs project. These files define the gRPC services and message structures used for communication between different components of the distributed storage network, particularly between the client and the `api_node`.

### `api.proto`

This file defines the core API for interacting with the distributed file system. It specifies two main gRPC services: `Put` for uploading data and `Get` for retrieving data.

#### Service: `Service`

-   **`rpc Put(stream PutRequest) returns(PutResponse)`**:
    -   A streaming RPC for uploading files and their associated metadata.
    -   Clients send a stream of `PutRequest` messages, which can contain either `PutRequestMetadata` (for file/directory information) or `UploadFile` (for file chunks).
    -   Returns a `PutResponse` indicating the success or failure of the upload.

-   **`rpc Get(GetRequest) returns(stream GetResponse)`**:
    -   A streaming RPC for downloading files or retrieving metadata.
    -   Clients send a single `GetRequest` specifying the `location` and `signature` of the desired data, and whether to `download` the actual file content.
    -   Returns a stream of `GetResponse` messages, which can contain either `GetResponseMetadata` (for directory listings or file metadata) or `DownloadFile` (for file chunks).

#### Message Definitions

-   **`PutRequestMetadata`**:
    -   `signature`: A string representing the cryptographic signature of the entry.
    -   `entry`: An `ApiEntry` message containing the metadata of the file or directory being uploaded.

-   **`ApiChildren`**:
    -   `name`: The name of the child file or directory.
    -   `type`: The type of the child ("file" or "directory").
    -   `size`: The size of the file in bytes.
    -   `cids`: A repeated string field containing Content Identifiers (CIDs) of the file's chunks.
    -   `data`: Optional bytes field for small files whose content is stored inline.

-   **`ApiEntry`**:
    -   `owner`: The public key of the owner of the entry.
    -   `public`: A boolean indicating if the entry is publicly accessible.
    -   `read_users`: A repeated string field containing public keys of users who have read access (if not public).
    -   `name`: The name of the entry (file or directory).
    -   `children`: A repeated `ApiChildren` message, representing the contents of a directory or the chunks of a file.

-   **`GetRequest`**:
    -   `location`: The path or identifier of the file/directory to retrieve.
    -   `sig`: The signature associated with the request, used for access control.
    -   `download`: A boolean indicating whether to download the file content (`true`) or just its metadata (`false`).

-   **`GetResponseMetadata`**:
    -   `entry`: Optional `ApiEntry` containing the metadata of the requested item.
    -   `children`: A repeated `ApiChildren` message, typically used for directory listings.
    -   `success`: A boolean indicating if the request was successful.
    -   `error`: Optional string containing an error message if the request failed.

-   **`DownloadFile`**:
    -   `content`: Bytes representing a chunk of the file content.
    -   `cid`: The Content Identifier of the file chunk.
    -   `name`: The name of the file.

-   **`UploadFile`**:
    -   `content`: Bytes representing a chunk of the file content.
    -   `cid`: The Content Identifier of the file chunk.

-   **`PutResponse`**:
    -   `key`: The key (signature) of the uploaded entry.
    -   `success`: A boolean indicating if the put operation was successful.
    -   `error`: Optional string containing an error message if the operation failed.

-   **`PutRequest`**:
    -   A `oneof` field that can either be `metadata` (for `PutRequestMetadata`) or `file` (for `UploadFile`). This allows sending both metadata and file chunks within the same stream.

-   **`GetResponse`**:
    -   A `oneof` field that can either be `metadata` (for `GetResponseMetadata`), `file` (for `DownloadFile`), or `error` (for a string error message). This allows the server to stream back either metadata, file chunks, or error messages.