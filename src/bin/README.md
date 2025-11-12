The `src/bin` directory contains executable binaries for the distributed-fs project. Currently, it includes `chat_p2p.rs`, which is a peer-to-peer chat application built using `libp2p`.

### `chat_p2p.rs`

This is a simple P2P chat application demonstrating the use of `libp2p` for peer discovery (mDNS) and message broadcasting (Floodsub).

**Key Features:**
-   **Peer Discovery**: Uses mDNS to discover other peers on the local network.
-   **Message Broadcasting**: Utilizes Floodsub to broadcast messages to all connected peers on a specific topic ("chat").
-   **Authenticated Transport**: Employs `noise` for authenticated encryption and `mplex` for multiplexing substreams over a TCP transport.
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