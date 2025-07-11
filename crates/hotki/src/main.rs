use hotkey_manager::{
    HotkeyManager,
    ipc::{IPCClient, IPCConnection, IPCServer},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::signal;
use tokio::time::sleep;

/// Default socket path for IPC communication
const DEFAULT_SOCKET_PATH: &str = "/tmp/hotkey-manager.sock";

/// Delay to wait for server startup
const SERVER_STARTUP_DELAY_MS: u64 = 100;

/// Delay to wait for server shutdown
const SERVER_SHUTDOWN_DELAY_MS: u64 = 100;

/// Wrapper to ensure server shutdown on drop
struct ServerGuard {
    connection: Option<IPCConnection>,
    shutdown_sent: Arc<AtomicBool>,
}

impl ServerGuard {
    fn new(connection: IPCConnection, shutdown_sent: Arc<AtomicBool>) -> Self {
        Self {
            connection: Some(connection),
            shutdown_sent,
        }
    }

    async fn connection(&mut self) -> &mut IPCConnection {
        self.connection.as_mut().unwrap()
    }

    async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(mut conn) = self.connection.take() {
            if !self.shutdown_sent.load(Ordering::SeqCst) {
                println!("\nSending shutdown command...");
                conn.shutdown().await?;
                self.shutdown_sent.store(true, Ordering::SeqCst);
            }
        }
        Ok(())
    }
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        if self.connection.is_some() && !self.shutdown_sent.load(Ordering::SeqCst) {
            eprintln!("\nWarning: ServerGuard dropped without sending shutdown command");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = DEFAULT_SOCKET_PATH;
    let shutdown_sent = Arc::new(AtomicBool::new(false));

    // Create a hotkey manager and configure some example hotkeys
    let manager =
        HotkeyManager::new().map_err(|e| format!("Failed to create hotkey manager: {e}"))?;

    // Create the IPC server with the manager
    let server = IPCServer::new(socket_path, manager);

    // Create a channel to signal server shutdown
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn the server in a background task
    let server_handle = tokio::spawn(async move {
        tokio::select! {
            result = server.run() => {
                if let Err(e) = result {
                    eprintln!("Server error: {e}");
                }
            }
            _ = shutdown_rx => {
                println!("Server received shutdown signal");
            }
        }
    });

    // Give the server a moment to start
    sleep(Duration::from_millis(SERVER_STARTUP_DELAY_MS)).await;

    // Create a client and connect
    let client = IPCClient::new(socket_path);
    let connection = client.connect().await?;
    let mut guard = ServerGuard::new(connection, shutdown_sent.clone());

    // Set up Ctrl+C handler
    let shutdown_sent_ctrlc = shutdown_sent.clone();
    tokio::spawn(async move {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        println!("\nReceived Ctrl+C, shutting down...");
        shutdown_sent_ctrlc.store(true, Ordering::SeqCst);
    });

    // Run main logic in a select! to handle Ctrl+C
    let result = tokio::select! {
        result = async {
            // List hotkeys
            println!("Listing hotkeys...");
            match guard.connection().await.list_hotkeys().await {
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

            // Add a small delay to allow testing Ctrl+C
            println!("\nPress Ctrl+C to test graceful shutdown, or wait 2 seconds...");
            tokio::select! {
                _ = sleep(Duration::from_secs(2)) => {
                    println!("Proceeding with normal shutdown");
                }
                _ = async {
                    while !shutdown_sent.load(Ordering::SeqCst) {
                        sleep(Duration::from_millis(100)).await;
                    }
                } => {
                    println!("Shutdown requested via Ctrl+C");
                }
            }

            // Normal shutdown
            guard.shutdown().await?;
            Ok::<(), Box<dyn std::error::Error>>(())
        } => result,

        _ = tokio::time::sleep(Duration::from_secs(1)) => {
            if shutdown_sent.load(Ordering::SeqCst) {
                guard.shutdown().await?;
                Ok(())
            } else {
                Ok(())
            }
        }
    };

    // Ensure shutdown is sent
    if !shutdown_sent.load(Ordering::SeqCst) {
        guard.shutdown().await?;
    }

    // Signal server to stop
    let _ = shutdown_tx.send(());

    // Wait for server to shut down
    tokio::time::timeout(
        Duration::from_millis(SERVER_SHUTDOWN_DELAY_MS * 2),
        server_handle,
    )
    .await
    .map_err(|_| "Server shutdown timeout")??;

    println!("Done!");
    result
}
