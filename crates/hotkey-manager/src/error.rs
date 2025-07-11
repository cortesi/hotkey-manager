use thiserror::Error;

/// The main error type for hotkey-manager operations
#[derive(Error, Debug)]
pub enum Error {
    /// Error parsing or validating a key combination
    #[error("Invalid key: {0}")]
    InvalidKey(String),

    /// Error registering or managing hotkeys
    #[error("Hotkey error: {0}")]
    HotkeyOperation(String),

    /// Error in IPC communication
    #[error("IPC error: {0}")]
    Ipc(String),

    /// IO-related errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization/deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Convenience type alias for Results using our Error type
pub type Result<T> = std::result::Result<T, Error>;

// Implement conversions for common error types we encounter
impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Serialization(err.to_string())
    }
}

impl From<global_hotkey::Error> for Error {
    fn from(err: global_hotkey::Error) -> Self {
        Error::HotkeyOperation(err.to_string())
    }
}

