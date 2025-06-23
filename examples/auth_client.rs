use sfifo::Sfifo;
use std::time::Duration;
use tokio::time::sleep;

/// Authenticated FIFO client example
///
/// Usage:
/// 1. First run the server: cargo run --example auth_server
/// 2. Then run the client: cargo run --example auth_client
///
/// The client connects to the server, performs authentication, and sends messages
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::init();

    let fifo_path = "/tmp/cross_process_auth_demo";
    let auth_token = "secure_cross_process_token_2024";

    println!("📱 Authenticated FIFO client starting...");
    println!("📁 FIFO path: {}", fifo_path);
    println!("🔗 Connecting to server...");

    // Wait a moment to ensure server is started
    sleep(Duration::from_millis(1000)).await;

    // Configure client - connect to server and perform authentication
    let client_config = Sfifo::new(fifo_path);

    match client_config.open_authenticated_sender(auth_token).await {
        Ok(mut auth_fifo) => {
            let peer = auth_fifo.peer_info();
            println!("✅ Server authentication successful!");
            println!("   Server process ID: {}", peer.process_id);
            println!("   Server process name: {}", peer.process_name);
            println!("   Authentication timestamp: {}", peer.timestamp);

            // Start sending messages
            println!("\n💬 Starting to send messages...");

            let messages = [
                "Hello from client process! 👋",
                "This is an authenticated FIFO communication 🔐",
                "Cross-process messaging works perfectly! 🚀",
                "Secure authentication established ✅",
                "Goodbye from client! 👋",
            ];

            for (i, message) in messages.iter().enumerate() {
                match auth_fifo.write_line(message).await {
                    Ok(_) => {
                        println!("📤 Sent message #{}: {}", i + 1, message);
                    }
                    Err(e) => {
                        eprintln!("❌ Failed to send message: {}", e);
                        break;
                    }
                }

                // Message interval
                sleep(Duration::from_millis(800)).await;
            }

            println!("\n✅ Client communication completed");
        }
        Err(e) => {
            eprintln!("❌ Client authentication failed: {}", e);
            eprintln!("💡 Tip: Please ensure the server is already running");
            return Err(e.into());
        }
    }

    Ok(())
}
