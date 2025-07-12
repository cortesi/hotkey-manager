//! A general-purpose global hotkey manager.
//!
//! This crate provides a high-level interface for managing global hotkeys with callbacks.
//! It handles hotkey registration, event listening, and callback execution in a thread-safe manner.

/// Default socket path for IPC communication
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/hotkey-manager.sock";

mod client;
mod error;
mod ipc;
mod key;
mod manager;
mod process;
mod server;

// Re-export the main types from modules
pub use client::Client;
pub use error::{Error, Result};
pub use ipc::{IPCConnection, IPCResponse};
pub use key::Key;
pub use process::ServerProcess;
pub use server::Server;
