The `chat` directory contains two distinct chat applications: a basic TCP-based chat server (`chat.rs`) and a peer-to-peer chat application built with `libp2p` (`chat_p2p.rs`).

### `chat.rs` - Basic TCP Chat Server

This is a simple multi-client chat server implemented using Tokio's TCP functionalities. It allows multiple clients to connect and exchange messages.

**Key Features:**
-   **TCP Listener**: Binds to a specified IP address and port (e.g., `192.168.0.248:8080`) to accept incoming client connections.
-   **Broadcast Channel**: Uses a `tokio::sync::broadcast` channel to distribute messages from one client to all other connected clients.
-   **Asynchronous Handling**: Each client connection is handled in a separate Tokio task, allowing concurrent communication.
-   **Username Support**: Clients send their username upon connection, which is then prepended to their messages.

**How to Run:**

1.  **Build the project**:
    ```bash
    cargo build
    ```
2.  **Run the server**:
    ```bash
    cargo run --bin chat
    ```
    The server will start listening on the configured address and port.

3.  **Connect with a client (e.g., using `netcat` or a custom client)**:
    ```bash
    nc 192.168.0.248 8080
    ```
    Upon connecting, you'll need to send your username followed by a newline. For example, type `Alice` and press Enter. Then you can start sending messages. Open multiple `nc` instances to chat between clients.

### `chat_p2p.rs` - Peer-to-Peer Chat Application

This is a more advanced P2P chat application built using the `libp2p` framework. It leverages `libp2p`'s capabilities for peer discovery and message broadcasting in a decentralized manner.

**Key Features:**
-   **Peer Discovery (mDNS)**: Automatically discovers other chat peers on the local network using mDNS.
-   **Message Broadcasting (Floodsub)**: Uses `libp2p`'s Floodsub protocol to broadcast messages to all connected peers on a specific topic ("chat").
-   **Authenticated Transport**: Employs `noise` for authenticated encryption and `mplex` for multiplexing substreams over a TCP transport, ensuring secure and efficient communication.
-   **Command Line Interface**: Allows users to type messages into stdin, which are then broadcast to the chat network. It also displays incoming messages from other peers.

**How to Run:**

1.  **Build the project**:
    ```bash
    cargo build
    ```
2.  **Run the chat application**:
    -   **First instance (listening)**:
        ```bash
        cargo run --bin chat_p2p
        ```
        This will start a chat peer listening on a random port. It will print its local peer ID and the address it's listening on.
    -   **Second instance (dialing another peer)**:
        ```bash
        cargo run --bin chat_p2p /ip4/127.0.0.1/tcp/<port_of_first_instance>
        ```
        Replace `<port_of_first_instance>` with the port number printed by the first instance. This will connect the second peer to the first one.

Once connected, you can type messages in either terminal, and they will be broadcast and displayed in the other.