use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use tokio::{signal, time::sleep};
use tracing::{debug, error, info};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use hotkey_manager::{Client, IPCConnection, IPCResponse, Key, Server};
use keymode::{Mode, State};

#[derive(Debug, Clone, ValueEnum)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Parser, Debug)]
#[command(name = "hotki-cli")]
#[command(about = "Hotkey manager client and server", long_about = None)]
struct Args {
    /// Path to RON mode definition file
    #[arg(required_unless_present = "server")]
    config: Option<std::path::PathBuf>,

    /// Run in server mode
    #[arg(long)]
    server: bool,

    /// Set the log level
    #[arg(short, long, value_enum)]
    log_level: Option<LogLevel>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Only initialize tracing if RUST_LOG is set or log level is explicitly provided
    if std::env::var("RUST_LOG").is_ok() || args.log_level.is_some() {
        let log_level = match args.log_level.unwrap_or(LogLevel::Info) {
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
                    .add_directive(format!("hotki_cli={log_level}").parse()?),
            )
            .init();
    }

    if args.server {
        info!("Starting hotki-cli server");
        Server::new().run()?;
        Ok(())
    } else {
        info!("Starting hotki-cli client");
        let runtime = tokio::runtime::Runtime::new().context("Failed to create Tokio runtime")?;
        runtime.block_on(client_main(args.config))
    }
}

/// Process hotkey events in a loop
async fn process_hotkey_events(connection: &mut IPCConnection, state: &mut State) -> Result<bool> {
    // Rebind keys for current mode
    let keys = state.keys();
    let key_refs: Vec<Key> = keys.iter().map(|(k, _, _)| k.clone()).collect();
    connection
        .rebind(&key_refs)
        .await
        .context("Failed to rebind hotkeys")?;

    // Print available keys before each event (excluding hidden ones)
    println!("\n\nAvailable keys:");
    for (key, desc, attrs) in &keys {
        if !attrs.hide {
            println!("  {key} - {desc}");
        }
    }

    match connection.recv_event().await {
        Ok(IPCResponse::HotkeyTriggered(key)) => {
            debug!("Received hotkey event: {}", key);
            match state.handle_key(&key) {
                Ok(handled) => {
                    // Display user message if present
                    if !handled.user.is_empty() {
                        println!("{}", handled.user);
                    }
                    // Display warning if present
                    if !handled.warn.is_empty() {
                        eprintln!("Warning: {}", handled.warn);
                    }
                    // Check if we should exit
                    if handled.exit {
                        info!("Exit action - shutting down...");
                        return Ok(true); // Signal to exit
                    }
                }
                Err(e) => {
                    error!("Error handling key: {}", e);
                    return Err(anyhow::anyhow!("Error handling key: {}", e));
                }
            }
        }
        Ok(response) => {
            info!("Received unexpected response: {:?}", response);
        }
        Err(e) => {
            error!("Error receiving event: {}", e);
            return Err(e.into());
        }
    }

    Ok(false) // Continue processing
}

async fn client_main(config_path: Option<std::path::PathBuf>) -> Result<()> {
    // Load and parse RON mode definition
    let path = config_path.expect("Config path is required for client mode");
    info!("Loading mode configuration from: {:?}", path);
    let ron_content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {path:?}"))?;

    let mode = match Mode::from_ron(&ron_content) {
        Ok(mode) => {
            info!("Successfully parsed mode configuration");
            mode
        }
        Err(e) => {
            error!("Failed to parse RON mode definition: {}", e);
            return Err(anyhow::anyhow!("Invalid mode configuration: {}", e));
        }
    };

    // Create keymode state
    let mut state = State::new(mode);

    let shutdown_sent = Arc::new(AtomicBool::new(false));
    let mut client = Client::new()
        .with_auto_spawn_server()
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
        // Bind keys from the current mode
        debug!("Starting event listener loop");

        tokio::select! {
            result = async {
                loop {
                    match process_hotkey_events(connection, &mut state).await {
                        Ok(should_exit) => {
                            if should_exit {
                                break Ok(());
                            }
                        }
                        Err(e) => {
                            error!("Error processing hotkey event: {}", e);
                            break Err(e);
                        }
                    }
                }
            } => {
                info!("Event loop ended");
                result
            }
            _ = async {
                while !shutdown_sent.load(Ordering::SeqCst) {
                    sleep(Duration::from_millis(100)).await;
                }
            } => {
                info!("Shutdown requested via Ctrl+C");
                Ok(())
            }
        }
    }
    .await;

    info!("\nShutting down...");
    // Try to disconnect gracefully, but don't fail if the connection is already broken
    if let Err(e) = client.disconnect(true).await {
        debug!(
            "Error during disconnect (this is expected if server was killed): {}",
            e
        );
    }
    result
}
