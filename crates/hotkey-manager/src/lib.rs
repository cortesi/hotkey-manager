//! A general-purpose global hotkey manager.
//!
//! This crate provides a high-level interface for managing global hotkeys with callbacks.
//! It handles hotkey registration, event listening, and callback execution in a thread-safe manner.

// Re-export commonly used types
pub use global_hotkey::hotkey::{Code, Modifiers};

/// Default socket path for IPC communication
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/hotkey-manager.sock";

pub mod client;
pub mod error;
pub mod ipc;
pub mod key;
pub mod manager;
pub mod process;
pub mod server;

// Re-export the main types from modules
pub use client::{Client, ManagedClientConfig};
pub use error::{Error, Result};
pub use key::Key;
pub use manager::{HotkeyCallback, HotkeyManager};
pub use process::{ProcessBuilder, ProcessConfig, ServerProcess};
pub use server::Server;
