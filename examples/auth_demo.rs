use sfifo::Sfifo;
use std::time::Duration;

/// Example demonstrating authenticated FIFO communication
/// Run this example with:
/// cargo run --example auth_demo
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fifo_path = "/tmp/auth_demo_fifo";
    let auth_token = "secure_shared_token_123";

    let level = log::LevelFilter::Debug;
    let _ = env_logger::builder().filter_level(level).try_init();

    println!("Starting authenticated FIFO demo...");

    // Clean up any existing FIFO
    let _ = std::fs::remove_file(fifo_path);

    // Spawn server task
    let server_task = tokio::spawn(async move {
        println!("Server: Initializing...");

        let server_config = Sfifo::new(fifo_path)
            .set_create(true) // Server creates the FIFO
            .set_read(true)
            .clone();

        match server_config.open_as_server(auth_token).await {
            Ok(mut auth_fifo) => {
                let peer = auth_fifo.peer_info();
                println!("Server: Successfully authenticated with client!");
                println!("Server: Client PID: {}", peer.process_id);
                println!("Server: Client name: {}", peer.process_name);

                // Server can now use auth_fifo.file() for communication
                let mut buf = [0; 1024];
                let n = auth_fifo.read(&mut buf).await.unwrap();
                println!("Server: Read: {:?}", String::from_utf8_lossy(&buf[..n]));
                Ok(())
            }
            Err(e) => {
                println!("Server: Authentication failed: {}", e);
                Err(e)
            }
        }
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Spawn client task
    let client_task = tokio::spawn(async move {
        println!("Client: Connecting...");

        let client_config = Sfifo::new(fifo_path).set_write(true).clone();

        match client_config.open_as_client(auth_token).await {
            Ok(mut auth_fifo) => {
                let peer = auth_fifo.peer_info();
                println!("Client: Successfully authenticated with server!");
                println!("Client: Server PID: {}", peer.process_id);
                println!("Client: Server name: {}", peer.process_name);

                // Client can now use auth_fifo.file() for communication

                let n = auth_fifo.write(b"Hello, server!").await.unwrap();
                println!("Client: Write: {:?}", n);

                Ok(())
            }
            Err(e) => {
                println!("Client: Authentication failed: {}", e);
                Err(e)
            }
        }
    });

    // Wait for both tasks to complete
    let (server_result, client_result) = tokio::join!(server_task, client_task);

    match (server_result?, client_result?) {
        (Ok(_), Ok(_)) => {
            println!("Demo completed successfully!");
            println!("Both processes have verified each other's identity.");
        }
        (Err(e), _) | (_, Err(e)) => {
            println!("Demo failed: {}", e);
        }
    }

    // Clean up
    let _ = std::fs::remove_file(fifo_path);

    Ok(())
}
