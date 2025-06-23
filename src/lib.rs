use getset::{Getters, Setters};
use log::{debug, error, info};
use nix::{sys::stat::Mode, unistd::mkfifo};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::net::unix::pipe::{Receiver, Sender};
// Define a constant for the default timeout duration
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);
// Define a constant for handshake timeout
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

// Handshake message structure for process authentication
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HandshakeMessage {
    pub process_id: u32,
    pub process_name: String,
    pub token: String,
    pub timestamp: u64,
    pub message_type: HandshakeType,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum HandshakeType {
    Request,
    Response,
    Ack,
}

// Authenticated FIFO wrapper that ensures both ends are verified
#[derive(Debug)]
pub enum AuthenticatedFifo {
    Sender {
        inner: Sender,
        peer_info: HandshakeMessage,
        is_server: bool,
    },
    Receiver {
        inner: Receiver,
        peer_info: HandshakeMessage,
        is_server: bool,
    },
}

impl AuthenticatedFifo {
    /// Get peer process information
    pub fn peer_info(&self) -> &HandshakeMessage {
        match self {
            AuthenticatedFifo::Sender { peer_info, .. } => peer_info,
            AuthenticatedFifo::Receiver { peer_info, .. } => peer_info,
        }
    }

    /// Check if this is the server side of the connection
    pub fn is_server(&self) -> bool {
        match self {
            AuthenticatedFifo::Sender { is_server, .. } => *is_server,
            AuthenticatedFifo::Receiver { is_server, .. } => *is_server,
        }
    }

    /// Create a new sender-based AuthenticatedFifo
    pub fn new_sender(sender: Sender, peer_info: HandshakeMessage, is_server: bool) -> Self {
        AuthenticatedFifo::Sender {
            inner: sender,
            peer_info,
            is_server,
        }
    }

    /// Create a new receiver-based AuthenticatedFifo
    pub fn new_receiver(receiver: Receiver, peer_info: HandshakeMessage, is_server: bool) -> Self {
        AuthenticatedFifo::Receiver {
            inner: receiver,
            peer_info,
            is_server,
        }
    }

    /// Check if this is a sender
    pub fn is_sender(&self) -> bool {
        matches!(self, AuthenticatedFifo::Sender { .. })
    }

    /// Check if this is a receiver
    pub fn is_receiver(&self) -> bool {
        matches!(self, AuthenticatedFifo::Receiver { .. })
    }

    /// Try to read data (non-blocking) - only works for Receiver
    pub fn try_read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            AuthenticatedFifo::Receiver { inner, .. } => inner.try_read(buf),
            AuthenticatedFifo::Sender { .. } => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot read from sender FIFO",
            )),
        }
    }

    /// Try to write data (non-blocking) - only works for Sender
    pub fn try_write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            AuthenticatedFifo::Sender { inner, .. } => inner.try_write(buf),
            AuthenticatedFifo::Receiver { .. } => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot write to receiver FIFO",
            )),
        }
    }

    /// Wait for readiness - only works for appropriate variant
    pub async fn readable(&self) -> std::io::Result<()> {
        match self {
            AuthenticatedFifo::Receiver { inner, .. } => inner.readable().await,
            AuthenticatedFifo::Sender { .. } => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot wait for readable on sender FIFO",
            )),
        }
    }

    /// Wait for writable - only works for Sender
    pub async fn writable(&self) -> std::io::Result<()> {
        match self {
            AuthenticatedFifo::Sender { inner, .. } => inner.writable().await,
            AuthenticatedFifo::Receiver { .. } => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot wait for writable on receiver FIFO",
            )),
        }
    }

    /// Read some bytes from the FIFO (async) - only works for Receiver
    pub async fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            AuthenticatedFifo::Receiver { .. } => loop {
                self.readable().await?;
                match self.try_read(buf) {
                    Ok(n) => return Ok(n),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(e) => return Err(e),
                }
            },
            AuthenticatedFifo::Sender { .. } => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot read from sender FIFO",
            )),
        }
    }

    /// Read exact number of bytes (async) - only works for Receiver
    pub async fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let mut bytes_read = 0;
        while bytes_read < buf.len() {
            match self.read(&mut buf[bytes_read..]).await? {
                0 => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "Failed to read exact number of bytes",
                    ))
                }
                n => bytes_read += n,
            }
        }
        Ok(())
    }

    /// Write some bytes to the FIFO (async) - only works for Sender
    pub async fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            AuthenticatedFifo::Sender { .. } => loop {
                self.writable().await?;
                match self.try_write(buf) {
                    Ok(n) => return Ok(n),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(e) => return Err(e),
                }
            },
            AuthenticatedFifo::Receiver { .. } => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Cannot write to receiver FIFO",
            )),
        }
    }

    /// Write all bytes to the FIFO (async) - only works for Sender
    pub async fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        let mut bytes_written = 0;
        while bytes_written < buf.len() {
            let n = self.write(&buf[bytes_written..]).await?;
            bytes_written += n;
        }
        Ok(())
    }

    /// Write a string - only works for Sender
    pub async fn write_str(&mut self, s: &str) -> std::io::Result<()> {
        self.write_all(s.as_bytes()).await
    }

    /// Write a line (with newline) - only works for Sender
    pub async fn write_line(&mut self, s: &str) -> std::io::Result<()> {
        self.write_all(s.as_bytes()).await?;
        self.write_all(b"\n").await
    }
}

impl HandshakeMessage {
    /// Create a new handshake message
    pub fn new(token: String, message_type: HandshakeType) -> std::io::Result<Self> {
        let process_id = std::process::id();
        let process_name = get_process_name()?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
            .as_secs();

        Ok(HandshakeMessage {
            process_id,
            process_name,
            token,
            timestamp,
            message_type,
        })
    }

    /// Serialize the handshake message to bytes
    pub fn to_bytes(&self) -> std::io::Result<Vec<u8>> {
        bincode::serialize(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Deserialize bytes to handshake message
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        bincode::deserialize(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Validate the handshake message
    pub fn validate(&self, expected_token: &str, max_age_secs: u64) -> std::io::Result<()> {
        // Validate token
        if self.token != expected_token {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Invalid authentication token",
            ));
        }

        // Validate timestamp to prevent replay attacks
        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?
            .as_secs();

        if current_time.saturating_sub(self.timestamp) > max_age_secs {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Handshake message too old",
            ));
        }

        Ok(())
    }
}

// Define the Sfifo struct with getters and setters for its fields
#[derive(Default, Clone, Getters, Setters)]
pub struct Sfifo {
    #[getset(get = "pub")]
    pub file_path: PathBuf,
    #[getset(get = "pub", set = "pub")]
    pub timeout: Duration,
    #[getset(get = "pub", set = "pub")]
    pub notify: bool,
    #[getset(get = "pub", set = "pub")]
    pub create: bool,
    #[getset(get = "pub", set = "pub")]
    pub write: bool,
    #[getset(get = "pub", set = "pub")]
    pub read: bool,
    #[getset(get = "pub", set = "pub")]
    pub blocking: bool,
}

impl Sfifo {
    /// Creates a new instance of `Sfifo`.
    ///
    /// # Parameters
    ///
    /// * `file_path`: A parameter that specifies the file path. It can be any type that implements the `AsRef<Path>` trait.
    ///
    /// # Returns
    ///
    /// Returns an instance of the `Sfifo` struct initialized with the given file path.
    /// Additionally, it sets the default timeout, blocking mode, and other default values.
    pub fn new(file_path: impl AsRef<Path>) -> Self {
        Sfifo {
            file_path: file_path.as_ref().to_path_buf(),
            timeout: DEFAULT_TIMEOUT,
            blocking: true,
            ..Default::default()
        }
    }

    pub async fn open_sender(&self) -> Result<Sender, std::io::Error> {
        let file_path = self.file_path.clone();
        let file_op = move |tokio_cancel: tokio_util::sync::CancellationToken| async move {
            loop {
                if tokio_cancel.is_cancelled() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "File deleted",
                    ));
                }
                let r = tokio::net::unix::pipe::OpenOptions::new().open_sender(&file_path);
                if let Ok(r) = r {
                    return Ok(r);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        };
        if self.notify {
            handle_file_with_notify_sender(file_op, &self.file_path).await
        } else {
            let tokio_cancel = tokio_util::sync::CancellationToken::new();
            let cancel_clone = tokio_cancel.clone();
            let timeout = self.timeout;
            let t = tokio::task::spawn(async move {
                tokio::time::sleep(timeout).await;
                cancel_clone.cancel();
            });
            let res = tokio::select! {
                res = file_op(tokio_cancel) => {
                    res
                },
                _ = t => {
                    Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "File deleted"))
                }
            };
            res
        }
    }

    pub async fn open_receiver(&self) -> Result<Receiver, std::io::Error> {
        if self.create {
            create_fifo(&self.file_path).await?;
        }
        let file_path = self.file_path.clone();
        tokio::net::unix::pipe::OpenOptions::new().open_receiver(&file_path)
    }
    /// Opens a FIFO file with the specified options.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the opened file or an error.
    /// Open FIFO as tokio::fs::File (legacy method)
    /// Deprecated: Use open_sender() or open_receiver() instead
    pub async fn open(&self) -> Result<tokio::fs::File, std::io::Error> {
        if self.create {
            create_fifo(&self.file_path).await?;
        }

        if self.read && self.write {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "For safety, read and write cannot be true at the same time",
            ));
        }

        let read = self.read;
        if self.blocking {
            if read {
                let owned_fd = self.open_receiver().await?.into_blocking_fd()?;
                let std_file = std::fs::File::from(owned_fd);
                Ok(tokio::fs::File::from_std(std_file))
            } else {
                let owned_fd = self.open_sender().await?.into_blocking_fd()?;
                let std_file = std::fs::File::from(owned_fd);
                Ok(tokio::fs::File::from_std(std_file))
            }
        } else {
            tokio::fs::OpenOptions::new()
                .custom_flags(libc::O_NONBLOCK)
                .read(self.read)
                .write(self.write)
                .open(&self.file_path)
                .await
        }
    }

    /// Create a new authenticated sender FIFO
    pub async fn open_authenticated_sender(
        &self,
        token: &str,
    ) -> Result<AuthenticatedFifo, std::io::Error> {
        let mut config = self.clone();
        config.set_write(true);
        config.set_read(false);
        config.open_as_client(token).await
    }

    /// Create a new authenticated receiver FIFO
    pub async fn open_authenticated_receiver(
        &self,
        token: &str,
    ) -> Result<AuthenticatedFifo, std::io::Error> {
        let mut config = self.clone();
        config.set_read(true);
        config.set_write(false);
        config.open_as_server(token).await
    }

    /// Opens a FIFO with authentication as server side
    /// Server side waits for client to initiate handshake
    ///
    /// # Parameters
    ///
    /// * `token`: Authentication token that both sides must share
    ///
    /// # Returns
    ///
    /// Returns an `AuthenticatedFifo` after successful handshake
    pub async fn open_as_server(&self, token: &str) -> Result<AuthenticatedFifo, std::io::Error> {
        let tokio_cancel = tokio_util::sync::CancellationToken::new();
        let cancel_clone = tokio_cancel.clone();

        // Spawn a task that will cancel the operation after HANDSHAKE_TIMEOUT
        let cancel_handle = tokio::spawn(async move {
            tokio::time::sleep(HANDSHAKE_TIMEOUT).await;
            cancel_clone.cancel();
        });

        let peer_info = self.perform_server_handshake(token, &tokio_cancel).await;
        // Cancel the timeout task since handshake completed
        cancel_handle.abort();

        match peer_info {
            Ok(peer_info) => {
                info!(
                    "Handshake completed with client PID {}",
                    peer_info.process_id
                );
                // reopen
                let file = self.open_receiver().await?;
                Ok(AuthenticatedFifo::new_receiver(file, peer_info, true))
            }
            Err(e) => {
                error!("Server: Handshake error: {:?}", e);
                Err(e)
            }
        }
    }

    /// Opens a FIFO with authentication as client side
    /// Client side initiates handshake with server
    ///
    /// # Parameters
    ///
    /// * `token`: Authentication token that both sides must share
    ///
    /// # Returns
    ///
    /// Returns an `AuthenticatedFifo` after successful handshake
    pub async fn open_as_client(&self, token: &str) -> Result<AuthenticatedFifo, std::io::Error> {
        let tokio_cancel = tokio_util::sync::CancellationToken::new();
        let cancel_clone = tokio_cancel.clone();

        // Spawn a task that will cancel the operation after HANDSHAKE_TIMEOUT
        let cancel_handle = tokio::spawn(async move {
            tokio::time::sleep(HANDSHAKE_TIMEOUT).await;
            cancel_clone.cancel();
        });

        tokio::select! {
            peer_info = self.perform_client_handshake(token,&tokio_cancel) => {
                // Cancel the timeout task since handshake completed
                cancel_handle.abort();
                match peer_info {
                    Ok(peer_info) => {
                        // reopen
                        let file = self.open_sender().await?;
                        Ok(AuthenticatedFifo::new_sender(file, peer_info, false))
                    }
                    Err(e) => {
                        Err(e)
                    }
                }
            }
            _ = tokio_cancel.cancelled() => {
                Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "Handshake timeout",
                ))
            }
        }
    }

    /// Perform handshake as server (waits for client to initiate)
    async fn perform_server_handshake(
        &self,
        token: &str,
        cancel_token: &tokio_util::sync::CancellationToken,
    ) -> Result<HandshakeMessage, std::io::Error> {
        // Step 1: Wait for client handshake request (client->server FIFO)
        let mut client_to_server_path = self.file_path.clone();
        client_to_server_path.set_extension("c2s");

        let mut read_sfifo = Sfifo::new(&client_to_server_path);
        read_sfifo.set_create(true);
        let mut read_file = read_sfifo.open_receiver().await?;
        debug!(
            "Server: Waiting for client handshake request on {:?}",
            client_to_server_path
        );
        let client_request = read_handshake_message(&mut read_file, cancel_token).await?;
        debug!(
            "Server: Received client handshake request {:?}",
            client_request
        );
        drop(read_file);
        // Validate client request
        if client_request.message_type != HandshakeType::Request {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Expected handshake request",
            ));
        }

        client_request.validate(token, 30)?;

        // Step 2: Send handshake response (server->client FIFO)
        debug!("Server: Sending handshake response");
        let mut server_to_client_path = self.file_path.clone();
        server_to_client_path.set_extension("s2c");

        let mut write_sfifo = Sfifo::new(&server_to_client_path);
        write_sfifo.set_create(true);
        let mut write_file = write_sfifo.open_sender().await?;
        let server_response = HandshakeMessage::new(token.to_string(), HandshakeType::Response)?;
        write_handshake_message(&mut write_file, &server_response).await?;
        drop(write_file);

        // Step 3: Wait for client acknowledgment (client->server FIFO)
        debug!("Server: Waiting for client acknowledgment");
        let read_sfifo = Sfifo::new(&client_to_server_path);
        let mut read_file = read_sfifo.open_receiver().await?;
        let client_ack = read_handshake_message(&mut read_file, cancel_token).await?;
        drop(read_file);

        debug!("Server: Received client acknowledgment {:?}", client_ack);
        if client_ack.message_type != HandshakeType::Ack {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Expected handshake acknowledgment",
            ));
        }

        client_ack.validate(token, 30)?;

        debug!(
            "Server: Handshake completed with client PID {}",
            client_request.process_id
        );
        Ok(client_request)
    }

    /// Perform handshake as client (initiates handshake)
    async fn perform_client_handshake(
        &self,
        token: &str,
        cancel_token: &tokio_util::sync::CancellationToken,
    ) -> Result<HandshakeMessage, std::io::Error> {
        // Step 1: Send handshake request (client->server FIFO)
        debug!("client: Sending handshake request");
        let mut client_to_server_path = self.file_path.clone();
        client_to_server_path.set_extension("c2s");

        let mut write_sfifo = Sfifo::new(&client_to_server_path);
        write_sfifo.set_create(true);
        let mut write_file = write_sfifo.open_sender().await?;
        let client_request = HandshakeMessage::new(token.to_string(), HandshakeType::Request)?;
        write_handshake_message(&mut write_file, &client_request).await?;
        drop(write_file);

        // Step 2: Wait for server response (server->client FIFO)
        debug!("client: Waiting for server response");
        let mut server_to_client_path = self.file_path.clone();
        server_to_client_path.set_extension("s2c");

        let mut read_sfifo = Sfifo::new(&server_to_client_path);
        read_sfifo.set_create(true);
        let mut read_file = read_sfifo.open_receiver().await?;
        let server_response = read_handshake_message(&mut read_file, cancel_token).await?;
        drop(read_file);
        debug!("client: Received server response {:?}", server_response);
        if server_response.message_type != HandshakeType::Response {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Expected handshake response",
            ));
        }

        server_response.validate(token, 30)?;

        // Step 3: Send acknowledgment (client->server FIFO)
        debug!("client: Sending acknowledgment");
        let write_sfifo = Sfifo::new(&client_to_server_path);
        let mut write_file = write_sfifo.open_sender().await?;
        let client_ack = HandshakeMessage::new(token.to_string(), HandshakeType::Ack)?;
        write_handshake_message(&mut write_file, &client_ack).await?;
        drop(write_file);

        debug!(
            "Client: Handshake completed with server PID {}",
            server_response.process_id
        );
        Ok(server_response)
    }
}

/// Creates a FIFO file at the specified path.
///
/// # Parameters
///
/// * `file_path`: The path where the FIFO file should be created.
///
/// # Returns
///
/// Returns a `Result` indicating success or an I/O error.
pub async fn create_fifo(file_path: impl AsRef<Path>) -> Result<(), std::io::Error> {
    if !Path::new(file_path.as_ref()).exists() {
        mkfifo(file_path.as_ref(), Mode::S_IRWXU)?;
    }
    Ok(())
}
/// Deletes a FIFO file at the specified path.
///
/// # Parameters
///
/// * `file_path`: The path of the FIFO file to be deleted.
///
/// # Returns
///
/// Returns a `Result` indicating success or an I/O error.
pub async fn delete_fifo(file_path: impl AsRef<Path>) -> Result<(), std::io::Error> {
    tokio::fs::remove_file(file_path).await?;
    Ok(())
}
/// Deprecated, There's a tread leak
/// Handles a file operation with a timeout.
///
/// # Parameters
///
/// * `file_op`: A closure that performs the file operation.
/// * `timeout`: The maximum duration to wait for the file operation to complete.
///
/// # Returns
///
/// Returns a `Result` containing the file or an error if the operation times out.
///
pub async fn handle_file_with_timeout<F, Fut>(
    file_op: F,
    timeout: Duration,
) -> Result<tokio::fs::File, std::io::Error>
where
    F: FnOnce(tokio_util::sync::CancellationToken) -> Fut,
    Fut: std::future::Future<Output = Result<tokio::fs::File, std::io::Error>> + Send,
{
    let tokio_cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = tokio_cancel.clone();
    match tokio::time::timeout(timeout, file_op(tokio_cancel)).await {
        Ok(res) => res,
        Err(_) => {
            cancel_clone.cancel();
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "File operation timed out",
            ))
        }
    }
}
/// Deprecated, There's a tread leak
/// Handles a file operation with notification on file deletion.
///
/// # Parameters
///
/// * `file_op`: A closure that performs the file operation.
/// * `file_path`: The path of the file to monitor for deletion.
///
/// # Returns
///
/// Returns a `Result` containing the file or an error if the file is deleted.
pub async fn handle_file_with_notify<F, Fut>(
    file_op: F,
    file_path: impl AsRef<Path>,
) -> Result<tokio::fs::File, std::io::Error>
where
    F: FnOnce(tokio_util::sync::CancellationToken) -> Fut,
    Fut: std::future::Future<Output = Result<tokio::fs::File, std::io::Error>> + Send,
{
    let tokio_cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = tokio_cancel.clone();
    let filepath_clone = file_path.as_ref().to_path_buf();
    let t = tokio::task::spawn(async move {
        loop {
            if tokio::fs::metadata(&filepath_clone).await.is_err() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    });

    tokio::select! {
        res = file_op(tokio_cancel) => {
            res
        },
        _ = t => {
            cancel_clone.cancel();
            Err(std::io::Error::new(std::io::ErrorKind::Other, "File deleted"))
        }
    }
}
/// There's no thread leak
/// Handles a file operation with notification on file deletion.
///
/// # Parameters
///
/// * `file_op`: A closure that performs the file operation.
/// * `file_path`: The path of the file to monitor for deletion.
///
pub async fn handle_file_with_notify_sender<F, Fut>(
    file_op: F,
    file_path: impl AsRef<Path>,
) -> Result<tokio::net::unix::pipe::Sender, std::io::Error>
where
    F: FnOnce(tokio_util::sync::CancellationToken) -> Fut,
    Fut:
        std::future::Future<Output = Result<tokio::net::unix::pipe::Sender, std::io::Error>> + Send,
{
    let tokio_cancel = tokio_util::sync::CancellationToken::new();
    let cancel_clone = tokio_cancel.clone();
    let filepath_clone = file_path.as_ref().to_path_buf();
    let t = tokio::task::spawn(async move {
        loop {
            if tokio::fs::metadata(&filepath_clone).await.is_err() {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    });
    tokio::select! {
        res = file_op(tokio_cancel) => {
            res
        },
        _ = t => {
            cancel_clone.cancel();
            Err(std::io::Error::new(std::io::ErrorKind::Other, "File deleted"))
        }
    }
}

/// Read a handshake message from the file
async fn read_handshake_message(
    file: &mut tokio::net::unix::pipe::Receiver,
    cancel_token: &tokio_util::sync::CancellationToken,
) -> Result<HandshakeMessage, std::io::Error> {
    // Read message length first (4 bytes)
    let mut len_buf = [0u8; 4];
    let mut bytes_read = 0;

    // Retry reading until we get all 4 bytes or timeout/cancel
    while bytes_read < 4 {
        tokio::select! {
            res = file.readable() => {
                match res {
                    Ok(_) => {
                        match file.try_read(&mut len_buf[bytes_read..]) {
                            Ok(n) => {
                                bytes_read += n;
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                continue;
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
            _ = cancel_token.cancelled() => {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Handshake timeout"));
            }
        }
    }
    let message_len = u32::from_le_bytes(len_buf) as usize;
    // Validate message length to prevent DoS
    if message_len > 4096 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Handshake message too large",
        ));
    }

    // Read the actual message
    let mut message_buf = vec![0u8; message_len];
    let mut bytes_read = 0;

    // Retry reading until we get all message bytes or timeout/cancel
    while bytes_read < message_len {
        tokio::select! {
            res = file.readable() => {
                match res {
                    Ok(_) => {
                        match file.try_read(&mut message_buf[bytes_read..]) {
                            Ok(n) => {
                                bytes_read += n;
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                continue;
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    Err(e) => return Err(e),
                }
            }
            _ = cancel_token.cancelled() => {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "Handshake timeout"));
            }
        }
    }

    HandshakeMessage::from_bytes(&message_buf)
}

/// Write a handshake message to the file
async fn write_handshake_message(
    file: &mut tokio::net::unix::pipe::Sender,
    message: &HandshakeMessage,
) -> Result<(), std::io::Error> {
    let message_bytes = message.to_bytes()?;
    let message_len = message_bytes.len() as u32;
    // Write message length first (4 bytes)
    let len_bytes = message_len.to_le_bytes();
    loop {
        file.writable().await?;
        match file.try_write(&len_bytes) {
            Ok(n) if n == len_bytes.len() => break,
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(e),
        }
    }

    // Write the actual message
    loop {
        file.writable().await?;
        match file.try_write(&message_bytes) {
            Ok(n) if n == message_bytes.len() => break,
            Ok(_) => continue,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => return Err(e),
        }
    }

    Ok(())
}

/// Get the current process name
fn get_process_name() -> std::io::Result<String> {
    let pid = std::process::id();
    let comm_path = format!("/proc/{}/comm", pid);

    match std::fs::read_to_string(&comm_path) {
        Ok(name) => Ok(name.trim().to_string()),
        Err(_) => {
            // Fallback: try to get from command line args
            std::env::args()
                .next()
                .map(|arg| {
                    std::path::Path::new(&arg)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                })
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::Other, "Cannot determine process name")
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_file_with_timeout() {
        let file_path = "test.txt";
        let file_op = |_| async { tokio::fs::File::create(file_path).await };
        let _ = handle_file_with_timeout(file_op, Duration::from_secs(2)).await;
        tokio::fs::remove_file(file_path).await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_file_with_notify() {
        let file_path = "test.txt";
        let file_op = |_| async { tokio::fs::File::create(file_path).await };
        let _ = handle_file_with_notify(file_op, file_path).await;
        tokio::fs::remove_file(file_path).await.unwrap();
    }

    #[tokio::test]
    async fn test_handshake_message_creation() {
        let token = "test_token_123".to_string();
        let msg = HandshakeMessage::new(token.clone(), HandshakeType::Request).unwrap();

        assert_eq!(msg.token, token);
        assert_eq!(msg.message_type, HandshakeType::Request);
        assert_eq!(msg.process_id, std::process::id());
        assert!(!msg.process_name.is_empty());
    }

    #[tokio::test]
    async fn test_handshake_message_serialization() {
        let token = "test_token_456".to_string();
        let msg = HandshakeMessage::new(token.clone(), HandshakeType::Response).unwrap();

        // Test serialization
        let bytes = msg.to_bytes().unwrap();
        assert!(!bytes.is_empty());

        // Test deserialization
        let deserialized = HandshakeMessage::from_bytes(&bytes).unwrap();
        assert_eq!(deserialized.token, msg.token);
        assert_eq!(deserialized.message_type, msg.message_type);
        assert_eq!(deserialized.process_id, msg.process_id);
        assert_eq!(deserialized.process_name, msg.process_name);
    }

    #[tokio::test]
    async fn test_handshake_message_validation() {
        let token = "valid_token".to_string();
        let msg = HandshakeMessage::new(token.clone(), HandshakeType::Request).unwrap();

        // Test valid token
        assert!(msg.validate(&token, 60).is_ok());

        // Test invalid token
        assert!(msg.validate("wrong_token", 60).is_err());

        // Test timestamp validation - create an old message
        let mut old_msg = msg.clone();
        old_msg.timestamp = 0;
        assert!(old_msg.validate(&token, 60).is_err());
    }

    #[tokio::test]
    async fn test_authenticated_fifo_integration() {
        let fifo_path = "/tmp/test_auth_fifo";
        let token = "integration_test_token";

        // Clean up any existing fifo
        let _ = tokio::fs::remove_file(fifo_path).await;

        // Create server and client configurations
        let server_config = Sfifo::new(fifo_path)
            .set_create(true)
            .set_read(true)
            .clone();

        let client_config = Sfifo::new(fifo_path).set_write(true).clone();

        // Test server-client handshake
        let server_handle = tokio::spawn(async move { server_config.open_as_server(token).await });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        let client_handle = tokio::spawn(async move { client_config.open_as_client(token).await });

        // Wait for both to complete
        let (server_result, client_result) = tokio::join!(server_handle, client_handle);

        // Both should succeed
        let server_fifo = server_result.unwrap().unwrap();
        let client_fifo = client_result.unwrap().unwrap();

        // Check peer information
        assert!(server_fifo.is_server());
        assert!(!client_fifo.is_server());
        assert_eq!(server_fifo.peer_info().token, token);
        assert_eq!(client_fifo.peer_info().token, token);

        // Clean up
        let _ = tokio::fs::remove_file(fifo_path).await;
    }

    #[tokio::test]
    async fn test_handshake_timeout() {
        let fifo_path = "/tmp/test_timeout_fifo";
        let token = "timeout_test_token";

        // Clean up any existing fifo
        let _ = tokio::fs::remove_file(fifo_path).await;

        let server_config = Sfifo::new(fifo_path)
            .set_create(true)
            .set_read(true)
            .clone();

        // Server should timeout if no client connects
        let result =
            tokio::time::timeout(Duration::from_secs(1), server_config.open_as_server(token)).await;
        println!("result: {:?}", result);
        // Should timeout
        assert!(result.is_err());

        // Clean up
        let _ = tokio::fs::remove_file(fifo_path).await;
        println!("Handshake timeout test passed");
    }

    #[tokio::test]
    async fn test_handshake_token_mismatch() {
        let fifo_path = "/tmp/test_mismatch_fifo";
        let server_token = "server_token";
        let client_token = "client_token";

        // Clean up any existing fifo
        let _ = tokio::fs::remove_file(fifo_path).await;

        let server_config = Sfifo::new(fifo_path)
            .set_create(true)
            .set_read(true)
            .clone();

        let client_config = Sfifo::new(fifo_path).set_write(true).clone();

        // Start server with one token
        let server_handle =
            tokio::spawn(async move { server_config.open_as_server(server_token).await });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Start client with different token
        let client_handle =
            tokio::spawn(async move { client_config.open_as_client(client_token).await });

        // Both should fail due to token mismatch
        let (server_result, client_result) = tokio::join!(server_handle, client_handle);

        println!("server_result: {:?}", server_result);
        println!("client_result: {:?}", client_result);
        assert!(server_result.unwrap().is_err());
        assert!(client_result.unwrap().is_err());

        // Clean up
        let _ = tokio::fs::remove_file(fifo_path).await;
    }

    #[tokio::test]
    async fn test_fifo_read_write() {
        let fifo_path = "/tmp/test_fifo";
        let server_config = Sfifo::new(fifo_path)
            .set_create(true)
            .set_read(true)
            .set_write(true)
            .clone();

        let r = server_config.open().await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn test_authenticated_fifo_usage() {
        let fifo_path = "/tmp/test_auth_usage";
        let token = "test_token_usage";

        // Clean up any existing fifo
        let _ = tokio::fs::remove_file(format!("{}.c2s", fifo_path)).await;
        let _ = tokio::fs::remove_file(format!("{}.s2c", fifo_path)).await;

        // Create server and client configurations using new convenience methods
        let mut server_config = Sfifo::new(fifo_path);
        server_config.set_create(true);
        let client_config = Sfifo::new(fifo_path);

        // Test server-client communication
        let server_handle = tokio::spawn(async move {
            let mut server_fifo = server_config.open_authenticated_receiver(token).await?;

            // Read message using async method
            let mut buf = vec![0u8; 256];
            let n = server_fifo.read(&mut buf).await?;
            buf.truncate(n);

            Ok::<Vec<u8>, std::io::Error>(buf)
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        let client_handle = tokio::spawn(async move {
            let mut client_fifo = client_config.open_authenticated_sender(token).await?;

            // Write message using async method
            let message = b"Hello from authenticated client!";
            client_fifo.write_all(message).await?;

            Ok::<(), std::io::Error>(())
        });

        // Wait for both to complete
        let (server_result, client_result) = tokio::join!(server_handle, client_handle);

        // Both should succeed
        let received_data = server_result.unwrap().unwrap();
        client_result.unwrap().unwrap();

        assert_eq!(received_data, b"Hello from authenticated client!");

        // Clean up
        let _ = tokio::fs::remove_file(format!("{}.c2s", fifo_path)).await;
        let _ = tokio::fs::remove_file(format!("{}.s2c", fifo_path)).await;
    }

    #[tokio::test]
    async fn test_authenticated_fifo_string_operations() {
        let fifo_path = "/tmp/test_auth_string";
        let token = "string_test_token";

        // Clean up
        let _ = tokio::fs::remove_file(format!("{}.c2s", fifo_path)).await;
        let _ = tokio::fs::remove_file(format!("{}.s2c", fifo_path)).await;

        let mut server_config = Sfifo::new(fifo_path);
        server_config.set_create(true);
        let client_config = Sfifo::new(fifo_path);

        let server_handle = tokio::spawn(async move {
            let mut server_fifo = server_config.open_authenticated_receiver(token).await?;
            let mut buf = vec![0u8; 1024];
            let n = server_fifo.read(&mut buf).await?;
            buf.truncate(n);
            Ok::<String, std::io::Error>(String::from_utf8_lossy(&buf).to_string())
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let client_handle = tokio::spawn(async move {
            let mut client_fifo = client_config.open_authenticated_sender(token).await?;
            client_fifo.write_line("Hello, World!").await?;
            client_fifo.write_str("Another message").await?;
            Ok::<(), std::io::Error>(())
        });

        let (server_result, client_result) = tokio::join!(server_handle, client_handle);
        let received_text = server_result.unwrap().unwrap();
        client_result.unwrap().unwrap();

        assert!(received_text.contains("Hello, World!"));
        assert!(received_text.contains("Another message"));

        // Clean up
        let _ = tokio::fs::remove_file(format!("{}.c2s", fifo_path)).await;
        let _ = tokio::fs::remove_file(format!("{}.s2c", fifo_path)).await;
    }
}
