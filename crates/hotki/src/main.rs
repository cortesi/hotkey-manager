use hotkey_manager::{
    HotkeyManager, Key,
    ipc::{IPCClient, IPCConnection, IPCResponse, IPCServer},
};
use std::env;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoop};
use tokio::signal;
use tokio::time::sleep;
use tracing::{debug, error, info, trace};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Default socket path for IPC communication
const DEFAULT_SOCKET_PATH: &str = "/tmp/hotkey-manager.sock";

/// Delay to wait for server startup
const SERVER_STARTUP_DELAY_MS: u64 = 500;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("hotkey_manager=trace".parse()?))
        .init();

    // Check command line arguments
    let args: Vec<String> = env::args().collect();

    if args.len() > 1 && args[1] == "--server" {
        info!("Starting hotki in server mode");
        run_server()
    } else {
        info!("Starting hotki in client mode");
        run_client()
    }
}

/// Run the hotkey manager server
fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = DEFAULT_SOCKET_PATH;

    // Create the tao event loop on the main thread
    let event_loop = EventLoop::new();

    // Create a hotkey manager on the main thread
    debug!("Creating HotkeyManager on main thread");
    let manager =
        HotkeyManager::new().map_err(|e| format!("Failed to create hotkey manager: {e}"))?;
    info!("HotkeyManager created successfully");

    // Create the IPC server with the manager
    let server = IPCServer::new(socket_path, manager);

    // Create a channel to signal shutdown
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();
    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let shutdown_requested_clone = shutdown_requested.clone();

    // Spawn the IPC server in a separate thread
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        info!("Server starting, waiting for client connection...");
        runtime.block_on(async {
            if let Err(e) = server.run().await {
                error!("Server error: {e}");
            }
        });
        info!("IPC server thread ending, sending shutdown signal");
        shutdown_requested_clone.store(true, Ordering::SeqCst);
        let _ = shutdown_tx.send(());
    });

    // Monitor for shutdown in another thread
    std::thread::spawn(move || {
        let _ = shutdown_rx.recv();
        info!("Shutdown signal received");
    });

    info!("Starting tao event loop on main thread...");

    // Run the tao event loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        // Check for shutdown
        if shutdown_requested.load(Ordering::SeqCst) {
            info!("Shutting down event loop");
            *control_flow = ControlFlow::Exit;
            return;
        }

        // Log significant events for debugging
        match event {
            Event::NewEvents(_) => {
                // This fires frequently, don't log
            }
            Event::MainEventsCleared => {
                // This fires frequently, don't log
            }
            _ => {
                trace!("Event loop received: {:?}", event);
            }
        }
    });
}

/// Run the client and spawn the server process
fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    // Spawn the server process
    let exe_path = env::current_exe()?;
    debug!("Spawning server process: {:?} --server", exe_path);

    let mut server_process = Command::new(&exe_path)
        .arg("--server")
        .env(
            "RUST_LOG",
            env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        )
        .spawn()
        .map_err(|e| format!("Failed to spawn server process: {e}"))?;

    info!("Server process spawned with PID: {:?}", server_process.id());

    // Run the async client
    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime.block_on(client_main());

    // Ensure server process is terminated
    info!("Terminating server process...");
    let _ = server_process.kill();
    let _ = server_process.wait();

    result
}

async fn client_main() -> Result<(), Box<dyn std::error::Error>> {
    let socket_path = DEFAULT_SOCKET_PATH;
    let shutdown_sent = Arc::new(AtomicBool::new(false));

    // Give the server a moment to start
    debug!("Waiting {}ms for server to start", SERVER_STARTUP_DELAY_MS);
    sleep(Duration::from_millis(SERVER_STARTUP_DELAY_MS)).await;

    // Create a client and connect
    debug!("Creating IPC client");
    let client = IPCClient::new(socket_path);
    debug!("Connecting to IPC server");
    let connection = client.connect().await?;
    info!("Connected to IPC server");
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

    // Run main logic
    let result = async {
        // List hotkeys before binding
        println!("Listing hotkeys before rebind...");
        match guard.connection().await.list_hotkeys().await {
            Ok(hotkeys) => {
                if hotkeys.is_empty() {
                    println!("No hotkeys registered.");
                } else {
                    println!("Registered hotkeys:");
                    for (id, identifier, description) in hotkeys {
                        println!(
                            "  ID: {id}, Identifier: {identifier}, Description: {description}"
                        );
                    }
                }
            }
            Err(e) => eprintln!("Failed to list hotkeys: {e}"),
        }

        // Use the Rebind operation to bind the "q" key
        println!("\nBinding 'q' key using Rebind operation...");
        let keys = vec![("quit".to_string(), Key::parse("q").unwrap())];
        debug!("Sending rebind request with keys: {:?}", keys);

        match guard.connection().await.rebind(&keys).await {
            Ok(()) => {
                info!("Successfully bound hotkeys via IPC");
                println!("Successfully bound hotkeys");
            }
            Err(e) => {
                error!("Failed to rebind hotkeys: {e}");
                eprintln!("Failed to rebind hotkeys: {e}");
                return Err(e.into());
            }
        }

        // List hotkeys after binding
        println!("\nListing hotkeys after rebind...");
        match guard.connection().await.list_hotkeys().await {
            Ok(hotkeys) => {
                if hotkeys.is_empty() {
                    println!("No hotkeys registered.");
                } else {
                    println!("Registered hotkeys:");
                    for (id, identifier, description) in hotkeys {
                        println!(
                            "  ID: {id}, Identifier: {identifier}, Description: {description}"
                        );
                    }
                }
            }
            Err(e) => eprintln!("Failed to list hotkeys: {e}"),
        }

        // Wait for quit event
        println!("\nPress 'q' to quit, or Ctrl+C to test graceful shutdown...");
        println!("Waiting for events...");

        // Listen for events from the server
        debug!("Starting event listener loop");
        tokio::select! {
            _ = async {
                loop {
                    trace!("Waiting for IPC event...");
                    match guard.connection().await.recv_event().await {
                        Ok(IPCResponse::HotkeyTriggered { identifier }) => {
                            info!("Received hotkey event: {identifier}");
                            println!("Received hotkey event: {identifier}");
                            if identifier == "quit" {
                                info!("Quit hotkey pressed - shutting down...");
                                println!("Quit hotkey pressed - shutting down...");
                                break;
                            }
                        }
                        Ok(response) => {
                            debug!("Received response: {response:?}");
                            println!("Received response: {response:?}");
                        }
                        Err(e) => {
                            error!("Error receiving event: {e}");
                            eprintln!("Error receiving event: {e}");
                            break;
                        }
                    }
                }
            } => {
                info!("Event loop ended");
                println!("Event loop ended");
            }
            _ = async {
                while !shutdown_sent.load(Ordering::SeqCst) {
                    sleep(Duration::from_millis(100)).await;
                }
            } => {
                info!("Shutdown requested via Ctrl+C");
                println!("Shutdown requested via Ctrl+C");
            }
        }

        // Normal shutdown
        guard.shutdown().await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;

    // Ensure shutdown is sent
    if !shutdown_sent.load(Ordering::SeqCst) {
        guard.shutdown().await?;
    }

    println!("Done!");
    result
}
