use getset::{Getters, Setters};
use nix::{sys::stat::Mode, unistd::mkfifo};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
// Define a constant for the default timeout duration
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

// Define the Sfifo struct with getters and setters for its fields
#[derive(Default, Getters, Setters)]
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
            // read: true,
            ..Default::default()
        }
    }
    /// Opens a FIFO file with the specified options.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the opened file or an error.
    pub async fn open(&self) -> Result<tokio::fs::File, std::io::Error> {
        if self.create {
            create_fifo(&self.file_path).await?;
        }

        if self.blocking {
            let file = || async {
                tokio::fs::OpenOptions::new()
                    .read(self.read)
                    .write(self.write)
                    .open(&self.file_path)
                    .await
            };
            if self.notify {
                handle_file_with_notify(file, &self.file_path).await
            } else {
                handle_file_with_timeout(file, self.timeout).await
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
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<tokio::fs::File, std::io::Error>> + Send,
{
    match tokio::time::timeout(timeout, file_op()).await {
        Ok(res) => res,
        Err(_) => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "File operation timed out",
        )),
    }
}

// pub async fn handle_file_with_notify<F, Fut>(
//     file_op: F,
//     file_path: impl AsRef<Path>,
// ) -> Result<tokio::fs::File, std::io::Error>
// where
//     F: FnOnce() -> Fut,
//     Fut: std::future::Future<Output = Result<tokio::fs::File, std::io::Error>> + Send,
// {
//     let inotify = Inotify::init().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
//     println!("Creating watch for file");
//     inotify
//         .watches()
//         .add(file_path.as_ref(), WatchMask::DELETE)
//         .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

//     let mut buffer = [0; 1024];
//     let mut stream = inotify.into_event_stream(&mut buffer)?;
//     println!("Waiting for file deletion");

//     tokio::select! {
//         res = file_op() => {
//             println!("fail opertion");
//             res
//         },
//         _ = stream.next() => {
//             Err(std::io::Error::new(std::io::ErrorKind::Other, "File deleted"))
//         }
//     }
// }

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
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<tokio::fs::File, std::io::Error>> + Send,
{
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
        res = file_op() => {
            res
        },
        _ = t => {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "File deleted"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_handle_file_with_timeout() {
        let file_path = "test.txt";
        let file_op = || async { tokio::fs::File::create(file_path).await };
        let _ = handle_file_with_timeout(file_op, Duration::from_secs(2)).await;
        tokio::fs::remove_file(file_path).await.unwrap();
    }

    #[tokio::test]
    async fn test_handle_file_with_notify() {
        let file_path = "test.txt";
        let file_op = || async { tokio::fs::File::create(file_path).await };
        let _ = handle_file_with_notify(file_op, file_path).await;
        tokio::fs::remove_file(file_path).await.unwrap();
    }
}
