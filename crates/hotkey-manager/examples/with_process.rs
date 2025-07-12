//! Example showing simplified process management using the new ServerProcess abstraction

use hotkey_manager::{ProcessBuilder, ipc::IPCClient, Key};
use std::env;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

const DEFAULT_SOCKET_PATH: &str = "/tmp/hotkey-manager.sock";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("hotkey_manager=info".parse()?))
        .init();

    // Check if we should run as server
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "--server" {
        // Run server code (unchanged from before)
        return run_server().await;
    }

    // Client mode with new ServerProcess abstraction
    info!("Starting client with managed server process");

    // Create and start the server process
    let mut server = ProcessBuilder::new(env::current_exe()?)
        .env("RUST_LOG", env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .start()
        .await?;

    info!("Server started with PID: {:?}", server.pid());

    // Run client logic
    let result = run_client().await;

    // ServerProcess will automatically stop when dropped
    info!("Stopping server...");
    server.stop().await?;

    result
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    use hotkey_manager::{HotkeyManager, ipc::IPCServer};
    use tao::event::Event;
    use tao::event_loop::{ControlFlow, EventLoop};

    info!("Running in server mode");

    // Create event loop and hotkey manager
    let event_loop = EventLoop::new();
    let manager = HotkeyManager::new()?;
    let server = IPCServer::new(DEFAULT_SOCKET_PATH, manager);

    let shutdown_requested = Arc::new(AtomicBool::new(false));
    let shutdown_requested_clone = shutdown_requested.clone();

    // Run IPC server in background
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            if let Err(e) = server.run().await {
                error!("Server error: {e}");
            }
        });
        shutdown_requested_clone.store(true, Ordering::SeqCst);
    });

    // Run event loop
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        if shutdown_requested.load(Ordering::SeqCst) {
            *control_flow = ControlFlow::Exit;
            return;
        }

        match event {
            Event::NewEvents(_) | Event::MainEventsCleared => {},
            _ => {}
        }
    });
}

async fn run_client() -> Result<(), Box<dyn std::error::Error>> {
    use hotkey_manager::ipc::IPCResponse;

    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_ctrlc = shutdown.clone();

    // Set up Ctrl+C handler
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        shutdown_ctrlc.store(true, Ordering::SeqCst);
    });

    // Connect to server
    let client = IPCClient::new(DEFAULT_SOCKET_PATH);
    let mut connection = client.connect().await?;
    info!("Connected to server");

    // Bind hotkey
    let keys = vec![("quit".to_string(), Key::parse("q").unwrap())];
    connection.rebind(&keys).await?;
    println!("Bound 'q' key - press to quit, or Ctrl+C to exit");

    // Listen for events
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

        tokio::select! {
            result = connection.recv_event() => {
                match result {
                    Ok(IPCResponse::HotkeyTriggered { identifier }) => {
                        println!("Hotkey triggered: {identifier}");
                        if identifier == "quit" {
                            break;
                        }
                    }
                    Ok(resp) => println!("Received: {resp:?}"),
                    Err(e) => {
                        error!("Error receiving event: {}", e);
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(tokio::time::Duration::from_millis(100)) => {
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }

    connection.shutdown().await?;
    Ok(())
}