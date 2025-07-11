use hotkey_manager::{
    HotkeyManager,
    ipc::{IPCClient, IPCServer},
};
use std::time::Duration;
use tokio::time::sleep;

/// Default socket path for IPC communication
const DEFAULT_SOCKET_PATH: &str = "/tmp/hotkey-manager.sock";

/// Delay to wait for server startup
const SERVER_STARTUP_DELAY_MS: u64 = 100;

/// Delay to wait for server shutdown
const SERVER_SHUTDOWN_DELAY_MS: u64 = 100;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = DEFAULT_SOCKET_PATH;

    // Create a hotkey manager and configure some example hotkeys
    let manager =
        HotkeyManager::new().map_err(|e| format!("Failed to create hotkey manager: {e}"))?;

    // Create the IPC server with the manager
    let server = IPCServer::new(socket_path, manager);

    // Note: In a real application, you would bind hotkeys before creating the server
    // and use create_event_forwarder to forward events to IPC clients

    // Spawn the server in a background task
    let _server_handle = tokio::spawn(async move {
        if let Err(e) = server.run().await {
            eprintln!("Server error: {e}");
        }
    });

    // Give the server a moment to start
    sleep(Duration::from_millis(SERVER_STARTUP_DELAY_MS)).await;

    // Create a client and connect
    let client = IPCClient::new(socket_path);
    let mut connection = client.connect().await?;

    // List hotkeys
    println!("Listing hotkeys...");
    match connection.list_hotkeys().await {
        Ok(hotkeys) => {
            if hotkeys.is_empty() {
                println!("No hotkeys registered.");
            } else {
                println!("Registered hotkeys:");
                for (id, identifier, description) in hotkeys {
                    println!("  ID: {id}, Identifier: {identifier}, Description: {description}");
                }
            }
        }
        Err(e) => eprintln!("Failed to list hotkeys: {e}"),
    }

    // Send shutdown command
    println!("\nSending shutdown command...");
    connection.shutdown().await?;

    // Wait for server to shut down
    sleep(Duration::from_millis(SERVER_SHUTDOWN_DELAY_MS)).await;

    println!("Done!");
    Ok(())
}
