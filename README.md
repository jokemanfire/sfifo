# sfifo

`sfifo` is a Rust library designed to manage FIFO (named pipe) files with advanced features including process authentication and deadlock prevention. It provides functionalities to create, open, and delete FIFO files, along with support for timeouts and file deletion notifications.Use blocking mode also will not cause deadlock.

## Features

- Create and manage FIFO files with process authentication
- Secure inter-process communication using token-based handshake mechanism
- Open FIFO files with configurable options (read, write, blocking, non-blocking)
- Support for fifo operation timeouts and file deletion notifications
- Prevention of user mode deadlock through authenticated connections
- Three-way handshake protocol (Request → Response → Acknowledgment)
- Timestamp-based replay attack protection
- Process identification and authentication


## Problem
How to rehandle the fifo one side is closed?
remove this fifo file, and rehandle the fifo?

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
sfifo = { path = "../path/to/sfifo" }  # Replace with the actual path or version

```
## Usage

### Basic Example
Here's a basic example of how to create and open a FIFO file using sfifo.
```rust
use sfifo::Sfifo;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sfifo = Sfifo::new("example.fifo");
    sfifo.set_read(true);
    sfifo.set_blocking(true);
    sfifo.set_timeout(Duration::from_secs(5));
    // sfifo.set_notify(true);  also support
    let file = sfifo.open().await?;
    // Use the file for reading/writing operations

    Ok(())
}
```

### Authenticated FIFO Communication

For secure inter-process communication with authentication:

#### Server Side
```rust
use sfifo::Sfifo;
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let server_sfifo = Sfifo::new("/tmp/secure_pipe")
        .set_create(true)
        .set_read(true)
        .set_write(true)
        .clone();
    
    let token = "secure_shared_token_12345";
    let mut authenticated_fifo = server_sfifo.open_as_server(token).await?;
    
    println!("Connected to client: PID {}", 
             authenticated_fifo.peer_info().process_id);
    
    // Now safely use the authenticated connection
    let mut buf = [0; 1024];
    let n = authenticated_fifo.read(&mut buf).await?;
    println!("Received: {}", String::from_utf8_lossy(&buf[..n]));

    Ok(())
}
```

#### Client Side
```rust
use sfifo::Sfifo;
use tokio::io::AsyncReadExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client_sfifo = Sfifo::new("/tmp/secure_pipe")
        .set_read(true)
        .set_write(true)
        .clone();
    
    let token = "secure_shared_token_12345";
    let mut authenticated_fifo = client_sfifo.open_as_client(token).await?;
    
    println!("Connected to server: PID {}", 
             authenticated_fifo.peer_info().process_id);
    
    let n = authenticated_fifo.write(b"Hello, server!").await?;
    println!("Sent: {}", n);

    Ok(())
}
```

### Security Features

- **Token-based Authentication**: Both processes must share the same secret token
- **Timestamp Validation**: Messages older than 30 seconds are rejected to prevent replay attacks
- **Process Identification**: Each handshake includes process ID and name for logging
- **Three-way Handshake**: Request → Response → Acknowledgment ensures both sides are authenticated


## License
This project is licensed under the MIT License. See the LICENSE file for details.


## Contributing
Contributions are welcome! Please open an issue or submit a pull request if you have any improvements or bug fixes.

For more detailed information, please refer to the source code and documentation.