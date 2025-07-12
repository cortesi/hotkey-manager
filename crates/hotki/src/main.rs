use std::{
    env,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use tokio::{signal, time::sleep};
use tracing::{debug, error, info, trace};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use hotkey_manager::{Client, Key, Server, ipc::IPCResponse};

/// Delay to wait for server startup
const SERVER_STARTUP_DELAY_MS: u64 = 500;

#[derive(Debug, Clone, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Parser, Debug)]
#[command(name = "hotki")]
#[command(about = "Hotkey manager client and server", long_about = None)]
struct Args {
    /// Run in server mode
    #[arg(long)]
    server: bool,

    /// Set the log level
    #[arg(short, long, value_enum, default_value = "info")]
    log_level: LogLevel,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Configure log level based on CLI argument
    let log_level = match args.log_level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
    };

    // Initialize tracing with custom format (no timestamps)
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .without_time()
                .with_target(false)
                .with_thread_ids(false)
                .with_thread_names(false),
        )
        .with(
            EnvFilter::from_default_env()
                .add_directive(format!("hotkey_manager={log_level}").parse()?)
                .add_directive(format!("hotki={log_level}").parse()?),
        )
        .init();

    if args.server {
        info!("Starting hotki in server mode");
        Server::new().run()?;
        Ok(())
    } else {
        info!("Starting hotki in client mode");
        let runtime = tokio::runtime::Runtime::new().context("Failed to create Tokio runtime")?;
        runtime.block_on(client_main())
    }
}

async fn client_main() -> Result<()> {
    let shutdown_sent = Arc::new(AtomicBool::new(false));
    info!("Starting client");
    let mut client = Client::new()
        .with_server_executable(
            env::current_exe().context("Failed to get current executable path")?,
        )
        .with_server_startup_timeout(Duration::from_millis(SERVER_STARTUP_DELAY_MS))
        .connect()
        .await
        .context("Failed to connect to hotkey server")?;

    info!("Connected to server (PID: {:?})", client.server_pid());

    // Set up Ctrl+C handler
    let shutdown_sent_ctrlc = shutdown_sent.clone();
    tokio::spawn(async move {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        info!("Received Ctrl+C, shutting down...");
        shutdown_sent_ctrlc.store(true, Ordering::SeqCst);
    });

    // Get the connection
    let connection = client
        .connection()
        .context("Failed to get client connection")?;

    // Run main logic
    let result = async {
        let keys = vec![("quit".to_string(), Key::parse("q").unwrap())];

        connection
            .rebind(&keys)
            .await
            .context("Failed to rebind hotkeys")?;

        info!("Successfully bound hotkeys via IPC");
        println!("Successfully bound hotkeys");

        // Wait for quit event
        println!("\nPress 'q' to quit, or Ctrl+C to test graceful shutdown...");
        println!("Waiting for events...");

        // Listen for events from the server
        debug!("Starting event listener loop");
        tokio::select! {
            _ = async {
                loop {
                    trace!("Waiting for IPC event...");
                    match connection.recv_event().await {
                        Ok(IPCResponse::HotkeyTriggered { identifier }) => {
                            info!("Received hotkey event: {identifier}");
                            debug!("Received hotkey event: {identifier}");
                            if identifier == "quit" {
                                info!("Quit hotkey pressed - shutting down...");
                                break;
                            }
                        }
                        Ok(response) => {
                            debug!("Received response: {response:?}");
                            debug!("Received unexpected response: {response:?}");
                        }
                        Err(e) => {
                            error!("Error receiving event: {e}");
                            break;
                        }
                    }
                }
            } => {
                info!("Event loop ended");
            }
            _ = async {
                while !shutdown_sent.load(Ordering::SeqCst) {
                    sleep(Duration::from_millis(100)).await;
                }
            } => {
                info!("Shutdown requested via Ctrl+C");
            }
        }

        Ok(())
    }
    .await;

    println!("\nShutting down...");
    client
        .disconnect(true)
        .await
        .context("Failed to disconnect client")?;
    result
}
