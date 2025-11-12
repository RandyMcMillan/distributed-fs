The `client` directory contains a TypeScript-based gRPC client designed to interact with the `api_node` of the distributed storage network. This client facilitates uploading directories and downloading files to and from the decentralized storage.

### Technologies Used

-   **TypeScript**: The primary language for client-side logic.
-   **gRPC**: Used for communication with the `api_node`, defined by `api.proto`.
-   **Node.js**: The runtime environment for the client.
-   **`@grpc/grpc-js` & `@grpc/proto-loader`**: Libraries for gRPC client implementation in Node.js.
-   **`crypto` (Node.js built-in)**: For cryptographic operations like SHA256 hashing.
-   **`secp256k1`**: For handling public-key cryptography, including signing and verification, essential for secure interactions with the distributed network.

### File Structure

-   **`src/client.ts`**: The main client application logic. It contains functions for:
    -   Loading the gRPC service definition.
    -   Establishing an insecure connection to the `api_node`.
    -   `getMetaDataForDir`: Recursively scans a local directory to build metadata (file names, types, sizes, inline data).
    -   `addCidsToChildren`: Calculates SHA256 CIDs for file chunks.
    -   `uploadDirectory`: Handles the streaming upload of directory metadata and file chunks to the `api_node`.
    -   `downloadFile`: Initiates a file download from the `api_node` and reconstructs the file locally.
-   **`proto/api.proto`**: The Protocol Buffer definition file that specifies the gRPC services (`Put` and `Get`) and message structures used for communication between the client and the `api_node`.
-   **`package.json`**: Defines project metadata, scripts (e.g., `dev`, `build`, `start`), and lists all Node.js dependencies and devDependencies.
-   **`tsconfig.json`**: TypeScript compiler configuration, specifying target JavaScript version, module system, output directory, and strictness rules.
-   **`nodemon.json`**: Configuration for `nodemon`, used during development to automatically restart the client when source files change.

### Setup

1.  **Navigate to the `client` directory**:
    ```bash
    cd client
    ```
2.  **Install dependencies**:
    ```bash
    npm install
    # or if you use pnpm
    pnpm install
    ```

### Usage

The client can be used to upload local directories and download files from the distributed storage network.

#### Running the Client

-   **Development Mode (with `nodemon`)**:
    ```bash
    npm run dev
    ```
    This will watch for changes in `src` and `proto` directories and automatically re-run `client.ts`.

-   **Production Mode (after building)**:
    ```bash
    npm run build
    npm run start
    ```
    First, compile the TypeScript code to JavaScript, then run the compiled JavaScript client.

#### Uploading a Directory

To upload a directory, you need to modify the `uploadDirectory` function call in `client/src/client.ts` to point to the local directory you wish to upload.

**Example**: To upload a directory named `my_files` located in the project root (one level up from the `client` directory):

```typescript
// client/src/client.ts
// ...
uploadDirectory("../my_files"); // Change this line to your desired directory
// ...
```

#### Downloading a File

To download a file, you need to uncomment and potentially modify the `downloadFile` function call in `client/src/client.ts`. Ensure the `location` and `sig` (signature) within the `downloadFile` function are correctly set to identify the file you want to retrieve from the network. Downloaded files will be saved in a `./download` directory relative to the client's execution path.

**Example**:

```typescript
// client/src/client.ts
// ...
// uploadDirectory("../test/Hello")
downloadFile(); // Uncomment this line and ensure location/sig are set inside the function
// ...
```

#### Note on Server URL

The client connects to a hardcoded `SERVER_URL` (e.g., `"192.168.0.248:50051"`) defined in `client/src/client.ts`. You **must** ensure this URL matches the address where your `api_node` is running. If your `api_node` is on a different IP address or port, you will need to update this constant in `client/src/client.ts`.