#!/bin/bash
echo "Starting hotki - press Ctrl+C to test shutdown..."
echo "The program will wait for 5 seconds after listing hotkeys"
echo ""

# Create a simple test program that waits
cat > /tmp/test_hotki.rs << 'EOF'
use hotkey_manager::{
    HotkeyManager,
    ipc::{IPCClient, IPCServer, IPCConnection},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use tokio::signal;

const DEFAULT_SOCKET_PATH: &str = "/tmp/hotkey-manager-test.sock";
const SERVER_STARTUP_DELAY_MS: u64 = 100;
const SERVER_SHUTDOWN_DELAY_MS: u64 = 100;

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
            eprintln!("\nServerGuard cleanup: Ensuring shutdown on drop");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = DEFAULT_SOCKET_PATH;
    let shutdown_sent = Arc::new(AtomicBool::new(false));

    let manager =
        HotkeyManager::new().map_err(|e| format!("Failed to create hotkey manager: {e}"))?;

    let server = IPCServer::new(socket_path, manager);
    
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

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

    sleep(Duration::from_millis(SERVER_STARTUP_DELAY_MS)).await;

    let client = IPCClient::new(socket_path);
    let connection = client.connect().await?;
    let mut guard = ServerGuard::new(connection, shutdown_sent.clone());
    
    let shutdown_sent_ctrlc = shutdown_sent.clone();
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        println!("\n*** Received Ctrl+C, shutting down gracefully...");
        shutdown_sent_ctrlc.store(true, Ordering::SeqCst);
    });

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
    
    println!("\n>>> Waiting 5 seconds - press Ctrl+C to test graceful shutdown...");
    
    for i in 1..=5 {
        if shutdown_sent.load(Ordering::SeqCst) {
            println!(">>> Shutdown requested!");
            break;
        }
        println!(">>> {}...", i);
        sleep(Duration::from_secs(1)).await;
    }
    
    if !shutdown_sent.load(Ordering::SeqCst) {
        println!("\n>>> Normal shutdown");
        guard.shutdown().await?;
    } else {
        guard.shutdown().await?;
    }
    
    let _ = shutdown_tx.send(());
    
    tokio::time::timeout(
        Duration::from_millis(SERVER_SHUTDOWN_DELAY_MS * 2),
        server_handle
    ).await
    .map_err(|_| "Server shutdown timeout")??;

    println!(">>> Clean shutdown complete!");
    Ok(())
}
EOF

cd /Users/cortesi/git/public/hotkey-manager
rustc --edition 2021 -L target/debug/deps /tmp/test_hotki.rs -o /tmp/test_hotki \
    --extern hotkey_manager=target/debug/libhotkey_manager.rlib \
    --extern tokio=target/debug/deps/libtokio-*.rlib \
    -L target/debug \
    $(find target/debug/deps -name "*.rlib" | sed 's/^/--extern /' | tr '\n' ' ')

if [ $? -eq 0 ]; then
    /tmp/test_hotki
else
    echo "Compilation failed, running cargo build first..."
    cargo build --bin hotki
    cargo run --bin hotki
fi