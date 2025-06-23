use sfifo::Sfifo;
use std::time::Duration;
use tokio::time::sleep;

/// Authenticated FIFO server example
///
/// Usage:
/// 1. Run in one terminal: cargo run --example auth_server
/// 2. Run in another terminal: cargo run --example auth_client
///
/// The server waits for client connections, performs authentication, and starts communication
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();

    let fifo_path = "/tmp/cross_process_auth_demo";
    let auth_token = "secure_cross_process_token_2024";

    println!("🖥️  Authenticated FIFO server starting...");
    println!("📁 FIFO path: {}", fifo_path);
    println!("🔑 Waiting for client connection...");

    // Clean up any existing FIFO files
    cleanup_fifos(fifo_path).await;

    // Configure server - create FIFO and wait for authentication
    let mut sfifo = Sfifo::new(fifo_path);
    sfifo.set_create(true);

    match sfifo.open_authenticated_receiver(auth_token).await {
        Ok(mut auth_fifo) => {
            let peer = auth_fifo.peer_info();
            println!("✅ Client authentication successful!");
            println!("   Client process ID: {}", peer.process_id);
            println!("   Client process name: {}", peer.process_name);
            println!("   Authentication timestamp: {}", peer.timestamp);

            // Start communication loop
            println!("\n💬 Starting message communication...");

            for i in 1..=5 {
                // Read client messages
                let mut buffer = vec![0u8; 1024];
                match auth_fifo.read(&mut buffer).await {
                    Ok(n) => {
                        buffer.truncate(n);
                        let message = String::from_utf8_lossy(&buffer);
                        println!("📨 Received message #{}: {}", i, message.trim());
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to read message: {}", e);
                        break;
                    }
                }

                // Brief delay
                sleep(Duration::from_millis(500)).await;
            }

            println!("\n✅ Server communication completed");
        }
        Err(e) => {
            eprintln!("❌ Server authentication failed: {}", e);
            return Err(e.into());
        }
    }

    // Cleanup
    cleanup_fifos(fifo_path).await;
    println!("🧹 Resource cleanup completed");

    Ok(())
}

/// Clean up FIFO files
async fn cleanup_fifos(base_path: &str) {
    let paths = [
        format!("{}.c2s", base_path),
        format!("{}.s2c", base_path),
        base_path.to_string(),
    ];

    for path in &paths {
        let _ = tokio::fs::remove_file(path).await;
    }
}
