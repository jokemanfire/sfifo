# Authenticated FIFO Examples

This directory contains examples demonstrating secure inter-process communication using authenticated FIFOs.

## Examples

### 1. Single Process Demo (`auth_demo.rs`)
A simple demonstration of authenticated FIFO communication within a single process.

```bash
cargo run --example auth_demo
```

### 2. Cross-Process Authentication (`auth_server.rs` + `auth_client.rs`)
Demonstrates secure communication between two separate processes using authentication tokens.

#### Running the Cross-Process Example

**Terminal 1 - Start the server:**
```bash
cargo run --example auth_server
```

**Terminal 2 - Start the client:**
```bash
cargo run --example auth_client
```

#### What happens:
1. The server creates authenticated FIFO endpoints and waits for connections
2. The client connects to the server using a shared authentication token
3. Both processes verify each other's identity (PID, process name, timestamp)
4. Secure message exchange begins after authentication succeeds
5. The client sends 5 messages to the server
6. The server receives and displays each message
7. Both processes clean up and exit

## Features Demonstrated

- **Mutual Authentication**: Both processes verify each other's identity
- **Secure Token Exchange**: Uses shared secrets for authentication
- **Process Information**: Exchanges PID, process name, and timestamps
- **Bidirectional Setup**: Uses separate FIFOs for client→server and server→client communication
- **Error Handling**: Proper error handling for authentication failures
- **Resource Cleanup**: Automatic cleanup of FIFO files

## Security Features

- **Token Validation**: Shared authentication tokens must match
- **Timestamp Verification**: Prevents replay attacks using message timestamps
- **Process Verification**: Validates process identity information
- **Timeout Protection**: Authentication attempts timeout after 5 seconds

## Authentication Flow

1. **Server Initialization**: Creates FIFO files and waits for client
2. **Client Connection**: Client connects and sends authentication request
3. **Server Verification**: Server validates client token and responds
4. **Client Confirmation**: Client validates server response and sends acknowledgment
5. **Communication Start**: Both sides begin secure data exchange

## File Structure

- `*.c2s` - Client-to-Server FIFO (for client requests and acknowledgments)
- `*.s2c` - Server-to-Client FIFO (for server responses)

This separation prevents message conflicts and ensures proper bidirectional communication. 