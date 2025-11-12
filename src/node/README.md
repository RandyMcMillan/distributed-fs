The `src/node` module defines the core components for the distributed storage network's nodes: `ApiNode` and `StorageNode`. It orchestrates their creation, initialization, and execution, acting as the central point for managing different node types.

### `Node` Enum and Implementations (`mod.rs`)

The `Node` enum serves as a wrapper for the two distinct node implementations: `ApiNode` and `StorageNode`. This allows for a unified interface to create and run different types of nodes.

-   **`NodeType` Enum**: Defines the two possible types of nodes: `ApiNode` and `StorageNode`.
-   **`NodeImp` Enum**: An internal enum that holds either an `ApiNode` or a `StorageNode` instance.
-   **`Node` Struct**: A newtype wrapper around `NodeImp`, providing `From` implementations for easy conversion from `ApiNode` and `StorageNode`.
-   **`new_api_node`**: Asynchronously creates and initializes an `ApiNode`.
-   **`new_storage_node`**: Asynchronously creates and initializes a `StorageNode`.
-   **`run`**: Dispatches to the appropriate `run_api` or `run_storage_node` method based on the underlying node type.

### `ApiNode` (`api_node.rs`)

The `ApiNode` is responsible for handling client-facing gRPC requests and interacting with the DHT and other nodes in the network.

**Key Components:**
-   **`api_req_receiver_stream`**: Receives gRPC requests from the `MyApi` service.
-   **`api_res_sender`**: Sends responses back to the `MyApi` service.
-   **`requests_receiver`**: Receives inbound `FileRequest` messages from other peers in the network.
-   **`dht_event_sender`**: Sends `DhtEvent` messages to the `EventLoop` to interact with the Kademlia DHT and Request-Response protocols.

**Core Functionalities:**
-   **`new(swarm_addr: &str, api_addr: SocketAddr)`**: Constructor that sets up the communication channels, initializes a `ManagedSwarm`, and spawns the `EventLoop` and the gRPC server.
-   **`run_api()`**: The main event loop for the `ApiNode`, continuously selecting between incoming gRPC requests (`api_req_receiver_stream`) and inbound requests from other peers (`requests_receiver`).
-   **`handle_request_response(req: FileRequest, channel: ResponseChannel<FileResponse>, peer: PeerId)`**: Handles inbound `FileRequest` messages from other peers.
    -   **`GetNodeTypeRequest`**: Responds with `NodeType::ApiNode`.
    -   **`ProvideRequest`**: Responds with an error as `ApiNode`s do not store chunks.
    -   **`GetFileRequest`**: Retrieves file chunks from its local cache (`./cache/`) based on provided CIDs and sends them back to the requesting peer.
-   **`handle_api_event(data: DhtRequestType)`**: Handles `DhtRequestType` messages originating from the gRPC `MyApi` service.
    -   **`GetRecord`**: Retrieves an `Entry` from the DHT using `dht_event_sender` and sends the result back to the `MyApi` service.
    -   **`PutRecord`**: Stores an `Entry` in the DHT. It first discovers `StorageNode`s, creates the `Entry` with associated storage peers, puts the record in the DHT, and then sends `ProvideRequest` messages to the discovered `StorageNode`s to instruct them to store the file chunks.

### `StorageNode` (`storage_node.rs`)

The `StorageNode` is responsible for storing file chunks and responding to requests for these chunks from other nodes.

**Key Components:**
-   **`requests_receiver`**: Receives inbound `FileRequest` messages from other peers.
-   **`dht_event_sender`**: Sends `DhtEvent` messages to the `EventLoop` to interact with the Kademlia DHT and Request-Response protocols.

**Core Functionalities:**
-   **`new(swarm_addr: &str)`**: Constructor that sets up communication channels, initializes a `ManagedSwarm`, and spawns the `EventLoop`.
-   **`run_storage_node()`**: The main event loop for the `StorageNode`, continuously listening for inbound requests from other peers (`requests_receiver`).
-   **`handle_request_response(req: FileRequest, channel: ResponseChannel<FileResponse>, peer: PeerId)`**: Handles inbound `FileRequest` messages from other peers.
    -   **`GetNodeTypeRequest`**: Responds with `NodeType::StorageNode`.
    -   **`ProvideRequest`**: This is a crucial part of the storage node's functionality. When an `ApiNode` sends a `ProvideRequest` with a list of CIDs, the `StorageNode` acknowledges it, then sends `GetFileRequest` messages back to the `ApiNode` (or the peer that sent the `ProvideRequest`) to download the actual file chunks. Once received, these chunks are stored in its local cache (`./cache/`).
    -   **`GetFileRequest`**: Retrieves file chunks from its local cache (`./cache/`) based on provided CIDs and sends them back to the requesting peer.