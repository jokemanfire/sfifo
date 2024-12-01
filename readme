# sfifo

`sfifo` is a Rust library designed to manage FIFO (named pipe) files. It provides functionalities to create, open, and delete FIFO files, along with support for timeouts and file deletion notifications.Provide Fifo with user mode protection and prevent user mode deadlock.
 
## Features

- Create and manage FIFO files.
- Open FIFO files with configurable options (read, write, blocking, non-blocking).
- Support for fifo operation timeouts.
- Support for fifo operation notification.

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
sfifo = { path = "../path/to/sfifo" }  # Replace with the actual path or version

```
## Usage
Basic Example
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
    // sfifo.set_notify(true);  aslo support
    let file = sfifo.open().await?;
    // Use the file for reading/writing operations

    Ok(())
}
```


## License
This project is licensed under the MIT License. See the LICENSE file for details.


## Contributing
Contributions are welcome! Please open an issue or submit a pull request if you have any improvements or bug fixes.

For more detailed information, please refer to the source code and documentation.