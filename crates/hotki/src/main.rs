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

use hotkey_manager::{Client, IPCResponse, Key, Server};
use keymode::{Action, Mode, State};

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
    /// Path to RON mode definition file
    #[arg(required_unless_present = "server")]
    config: Option<std::path::PathBuf>,

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
        info!("Starting hotki server");
        Server::new().run()?;
        Ok(())
    } else {
        info!("Starting hotki client");
        let runtime = tokio::runtime::Runtime::new().context("Failed to create Tokio runtime")?;
        runtime.block_on(client_main(args.config))
    }
}

async fn client_main(config_path: Option<std::path::PathBuf>) -> Result<()> {
    // Load and parse RON mode definition
    let path = config_path.expect("Config path is required for client mode");
    info!("Loading mode configuration from: {:?}", path);
    let ron_content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {:?}", path))?;

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
        let current_mode = state.mode();
        let keys: Vec<(String, Key)> = current_mode
            .keys()
            .filter_map(|(key_str, desc)| match Key::parse(key_str) {
                Ok(key) => {
                    info!("Binding key '{}': {}", key_str, desc);
                    Some((key_str.to_string(), key))
                }
                Err(e) => {
                    error!("Failed to parse key '{}': {}", key_str, e);
                    None
                }
            })
            .collect();

        connection
            .rebind(&keys)
            .await
            .context("Failed to rebind hotkeys")?;

        info!("Successfully bound {} hotkeys", keys.len());
        debug!("Starting event listener loop");

        tokio::select! {
            _ = async {
                loop {
                    match connection.recv_event().await {
                        Ok(IPCResponse::HotkeyTriggered { identifier }) => {
                            debug!("Received hotkey event: {}", identifier);

                            // Process the key through the state
                            if let Some(action) = state.key(&identifier) {
                                info!("Action triggered: {:?}", action);

                                match action {
                                    Action::Exit => {
                                        info!("Exit action - shutting down...");
                                        break;
                                    }
                                    Action::Shell(cmd) => {
                                        println!("Shell command: {}", cmd);
                                    }
                                    Action::Mode(_) => {
                                        println!("Mode change (would rebind keys)");
                                    }
                                    Action::Pop => {
                                        println!("Pop (would rebind to previous mode)");
                                    }
                                }
                            } else {
                                debug!("Key '{}' not found in current mode", identifier);
                            }
                        }
                        Ok(response) => {
                            info!("Received unexpected response: {:?}", response);
                        }
                        Err(e) => {
                            error!("Error receiving event: {}", e);
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

    info!("\nShutting down...");
    client
        .disconnect(true)
        .await
        .context("Failed to disconnect client")?;
    result
}
