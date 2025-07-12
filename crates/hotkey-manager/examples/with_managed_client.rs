//! Example showing the ManagedClient abstraction that handles server lifecycle automatically

use hotkey_manager::{Key, ManagedClientBuilder};
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{error, info};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

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

    // Client mode with ManagedClient - much simpler!
    info!("Starting client with ManagedClient");

    // Create a managed client that will automatically spawn the server if needed
    let mut client = ManagedClientBuilder::new(DEFAULT_SOCKET_PATH)
        .with_server_executable(env::current_exe()?)
        .server_startup_timeout(Duration::from_millis(1000))
        .max_connection_attempts(5)
        .connection_retry_delay(Duration::from_millis(200))
        .connect()
        .await?;

    info!("Connected to server (PID: {:?})", client.server_pid());

    // Set up Ctrl+C handler
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_ctrlc = shutdown.clone();
    tokio::spawn(async move {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        println!("\nReceived Ctrl+C, shutting down...");
        shutdown_ctrlc.store(true, Ordering::SeqCst);
    });

    // Get the connection and use it
    let connection = client.connection()?;

    // Bind hotkeys
    let keys = vec![
        ("quit".to_string(), Key::parse("q").unwrap()),
        ("test".to_string(), Key::parse("t").unwrap()),
    ];
    connection.rebind(&keys).await?;
    println!("Hotkeys bound:");
    println!("  'q' - quit");
    println!("  't' - test message");
    println!("  Ctrl+C - graceful shutdown");

    // Listen for events
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

        tokio::select! {
            result = connection.recv_event() => {
                match result {
                    Ok(hotkey_manager::ipc::IPCResponse::HotkeyTriggered { identifier }) => {
                        println!("Hotkey triggered: {identifier}");
                        match identifier.as_str() {
                            "quit" => {
                                println!("Quit hotkey pressed - shutting down...");
                                break;
                            }
                            "test" => {
                                println!("Test hotkey pressed!");
                            }
                            _ => {}
                        }
                    }
                    Ok(resp) => println!("Received: {resp:?}"),
                    Err(e) => {
                        error!("Error receiving event: {}", e);
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(100)) => {
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }
            }
        }
    }

    // Disconnect and stop the server if we spawned it
    println!("Disconnecting...");
    client.disconnect(true).await?;

    println!("Done!");
    Ok(())
}

async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    use hotkey_manager::{ipc::IPCServer, HotkeyManager};
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
            Event::NewEvents(_) | Event::MainEventsCleared => {}
            _ => {}
        }
    });
}
