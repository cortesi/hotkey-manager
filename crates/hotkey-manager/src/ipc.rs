//! Inter-Process Communication (IPC) module for hotkey management.
//!
//! This module provides a client-server architecture for managing hotkeys
//! across process boundaries. The server runs in a separate process with
//! the actual HotkeyManager, while clients can connect to query state and
//! receive hotkey events.
//!
//! Key design decisions:
//! - Hotkeys must be pre-configured before starting the server (no dynamic binding)
//! - Communication uses Unix domain sockets with a simple length-prefixed protocol
//! - Single-client mode ensures automatic cleanup for one-to-one relationships
//! - Events are forwarded asynchronously to all connected clients
//!
//! The IPC system is designed to solve the problem of running hotkey managers
//! in separate processes, particularly useful for macOS applications where
//! hotkey handling in the main thread can cause issues.

use crate::HotkeyManager;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

/// Represents requests that can be sent from IPC clients to the server.
///
/// The IPC protocol is designed to be minimal and focused on querying
/// hotkey state rather than dynamic configuration. Hotkeys must be
/// configured when creating the HotkeyManager before starting the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IPCRequest {
    /// Request a list of all currently registered hotkeys.
    /// Returns hotkey ID, identifier, and description for each hotkey.
    ListHotkeys,
    /// Request the server to shut down gracefully.
    /// In single-client mode, the server will also shut down when
    /// the client disconnects without sending this command.
    Shutdown,
}

/// Represents responses sent from the IPC server to clients.
///
/// Responses can be either direct replies to requests or asynchronous
/// events like hotkey triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IPCResponse {
    /// Successful response to a request.
    /// Contains a human-readable message and optional JSON data.
    Success {
        message: String,
        data: Option<serde_json::Value>,
    },
    /// Error response indicating the request failed.
    Error { message: String },
    /// Asynchronous event sent when a hotkey is triggered.
    /// Contains the identifier that was provided when the hotkey was registered.
    HotkeyTriggered { identifier: String },
}

/// IPC server that manages hotkey operations and client connections.
///
/// The server runs in a separate process and communicates with clients
/// via Unix domain sockets. It maintains a pre-configured HotkeyManager
/// and forwards hotkey events to connected clients.
///
/// By default, the server operates in single-client mode where it
/// automatically shuts down when the client disconnects. This ensures
/// clean process management for one-to-one client-server relationships.
pub struct IPCServer {
    socket_path: PathBuf,
    manager: Arc<HotkeyManager>,
    event_senders: Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>>,
    single_client: bool,
}

impl IPCServer {
    /// Creates a new IPC server with a pre-configured HotkeyManager.
    ///
    /// The server will bind to the specified Unix domain socket path.
    /// Hotkeys must be configured on the HotkeyManager before creating
    /// the server, as dynamic binding is not supported through IPC.
    ///
    /// Defaults to single-client mode for automatic cleanup.
    pub fn new(socket_path: impl Into<PathBuf>, manager: HotkeyManager) -> Self {
        let socket_path = socket_path.into();
        let event_senders = Arc::new(Mutex::new(Vec::new()));

        Self {
            socket_path,
            manager: Arc::new(manager),
            event_senders,
            single_client: true, // Default to single-client mode
        }
    }

    /// Set whether the server should shut down when the last client disconnects.
    ///
    /// In single-client mode (default), the server will automatically exit when
    /// its client disconnects, ensuring no orphaned processes. This is ideal
    /// for applications with a one-to-one client-server relationship.
    ///
    /// In multi-client mode, the server continues running after clients disconnect
    /// and can accept new connections.
    pub fn set_single_client(mut self, single_client: bool) -> Self {
        self.single_client = single_client;
        self
    }

    /// Get a reference to the event senders for setting up hotkey callbacks.
    ///
    /// This is used with `create_event_forwarder` to create callbacks that
    /// forward hotkey events to all connected IPC clients. The event senders
    /// are managed internally and cleaned up when clients disconnect.
    pub fn event_senders(
        &self,
    ) -> Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>> {
        self.event_senders.clone()
    }

    /// Run the IPC server, accepting client connections and processing requests.
    ///
    /// This method will block until the server shuts down. In single-client mode,
    /// the server exits when the client disconnects. In multi-client mode, it
    /// runs until explicitly shut down.
    ///
    /// The server automatically removes any existing socket file at the path
    /// before binding to ensure a clean start.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // Remove socket file if it exists
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)?;

        if self.single_client {
            // In single-client mode, accept one connection and exit when it disconnects
            let (stream, _) = listener.accept().await?;
            let manager = self.manager.clone();
            let event_senders = self.event_senders.clone();

            println!("Single client connected, server will exit when client disconnects");
            handle_client(stream, manager, event_senders).await?;
            println!("Client disconnected, shutting down server");
        } else {
            // Multi-client mode
            loop {
                let (stream, _) = listener.accept().await?;
                let manager = self.manager.clone();
                let event_senders = self.event_senders.clone();

                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, manager, event_senders).await {
                        eprintln!("Client error: {e}");
                    }
                });
            }
        }

        Ok(())
    }
}

/// Handle a single client connection, processing requests and forwarding events.
///
/// This function manages the bidirectional communication with a client:
/// - Reads requests and sends responses
/// - Forwards hotkey events to the client
/// - Cleans up when the client disconnects
///
/// Uses a simple length-prefixed binary protocol for message framing.
async fn handle_client(
    stream: UnixStream,
    manager: Arc<HotkeyManager>,
    event_senders: Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    event_senders.lock().unwrap().push(event_tx.clone());

    let (reader, writer) = stream.into_split();
    let reader = Arc::new(tokio::sync::Mutex::new(reader));
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    // Spawn task to forward events to client
    let writer_clone = writer.clone();
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let data = match serde_json::to_vec(&event) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let len_bytes = (data.len() as u32).to_be_bytes();
            let mut writer = writer_clone.lock().await;
            let _ = writer.write_all(&len_bytes).await;
            let _ = writer.write_all(&data).await;
            let _ = writer.flush().await;
        }
    });

    loop {
        // Read message length
        let mut len_bytes = [0u8; 4];
        {
            let mut reader = reader.lock().await;
            match reader.read_exact(&mut len_bytes).await {
                Ok(_) => {}
                Err(_) => break,
            }
        }

        let len = u32::from_be_bytes(len_bytes) as usize;

        // Read message data
        let mut data = vec![0u8; len];
        {
            let mut reader = reader.lock().await;
            reader.read_exact(&mut data).await?;
        }

        let request: IPCRequest = serde_json::from_slice(&data)?;
        let is_shutdown = matches!(request, IPCRequest::Shutdown);
        let response = handle_request(&manager, request, &event_tx).await;

        // Send response
        let response_data = serde_json::to_vec(&response)?;
        let response_len = (response_data.len() as u32).to_be_bytes();
        {
            let mut writer = writer.lock().await;
            writer.write_all(&response_len).await?;
            writer.write_all(&response_data).await?;
            writer.flush().await?;
        }

        if is_shutdown {
            break;
        }
    }

    // Remove event sender
    event_senders.lock().unwrap().retain(|tx| !tx.is_closed());

    Ok(())
}

/// Process an individual IPC request and generate the appropriate response.
///
/// This function handles the business logic for each request type,
/// interfacing with the HotkeyManager to query state.
async fn handle_request(
    manager: &Arc<HotkeyManager>,
    request: IPCRequest,
    _event_tx: &tokio::sync::mpsc::UnboundedSender<IPCResponse>,
) -> IPCResponse {
    match request {
        IPCRequest::ListHotkeys => {
            let hotkeys = manager.list_hotkeys();
            IPCResponse::Success {
                message: format!("Found {} hotkeys", hotkeys.len()),
                data: Some(serde_json::json!(hotkeys)),
            }
        }

        IPCRequest::Shutdown => IPCResponse::Success {
            message: "Shutting down".to_string(),
            data: None,
        },
    }
}

/// IPC client for connecting to a hotkey manager server.
///
/// The client connects to a server via Unix domain socket and can
/// query hotkey state and receive hotkey events. It does not support
/// dynamic hotkey configuration - hotkeys must be pre-configured on
/// the server side.
pub struct IPCClient {
    socket_path: PathBuf,
}

impl IPCClient {
    /// Create a new IPC client that will connect to the specified socket path.
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    /// Connect to the IPC server and return a connection handle.
    ///
    /// The connection can be used to send requests and receive responses
    /// and events. The server must be running and listening on the socket
    /// path for this to succeed.
    pub async fn connect(&self) -> Result<IPCConnection, Box<dyn std::error::Error>> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        Ok(IPCConnection { stream })
    }
}

/// An active connection to an IPC server.
///
/// This struct provides methods to interact with the server, including
/// querying hotkey state and receiving events. All communication is
/// asynchronous and uses a length-prefixed binary protocol.
pub struct IPCConnection {
    stream: UnixStream,
}

impl IPCConnection {
    /// Send a request to the server using the length-prefixed protocol.
    ///
    /// Messages are encoded as JSON and prefixed with a 4-byte big-endian
    /// length header for proper framing over the stream connection.
    async fn send_request(
        &mut self,
        request: &IPCRequest,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data = serde_json::to_vec(request)?;
        let len_bytes = (data.len() as u32).to_be_bytes();
        self.stream.write_all(&len_bytes).await?;
        self.stream.write_all(&data).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Receive a response from the server using the length-prefixed protocol.
    ///
    /// Reads the 4-byte length header first, then reads exactly that many
    /// bytes and decodes the JSON response.
    async fn recv_response(&mut self) -> Result<IPCResponse, Box<dyn std::error::Error>> {
        let mut len_bytes = [0u8; 4];
        self.stream.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).await?;

        let response: IPCResponse = serde_json::from_slice(&data)?;
        Ok(response)
    }

    /// Request a list of all registered hotkeys from the server.
    ///
    /// Returns a vector of tuples containing:
    /// - Hotkey ID (unique identifier)
    /// - Identifier (user-provided string identifier)
    /// - Description (string representation of the hotkey combination)
    pub async fn list_hotkeys(
        &mut self,
    ) -> Result<Vec<(u32, String, String)>, Box<dyn std::error::Error>> {
        self.send_request(&IPCRequest::ListHotkeys).await?;

        match self.recv_response().await? {
            IPCResponse::Success { data, .. } => {
                if let Some(data) = data {
                    serde_json::from_value(data).map_err(|e| e.into())
                } else {
                    Ok(vec![])
                }
            }
            IPCResponse::Error { message } => Err(message.into()),
            _ => Err("Unexpected response".into()),
        }
    }

    /// Send a shutdown request to the server.
    ///
    /// This requests a graceful shutdown of the server. In single-client mode,
    /// the server will also shut down automatically when the client disconnects,
    /// but sending an explicit shutdown is recommended for clean termination.
    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.send_request(&IPCRequest::Shutdown).await?;
        Ok(())
    }

    /// Receive the next event or response from the server.
    ///
    /// This method blocks until a message is received. It can return:
    /// - Response to a previous request
    /// - HotkeyTriggered event when a hotkey is activated
    ///
    /// For typical request-response patterns, this is called internally
    /// by the request methods. Call this directly when waiting for
    /// asynchronous hotkey events.
    pub async fn recv_event(&mut self) -> Result<IPCResponse, Box<dyn std::error::Error>> {
        self.recv_response().await
    }
}

/// Creates a callback that forwards hotkey events to all connected IPC clients.
///
/// This function returns a closure that can be used as a hotkey callback.
/// When a hotkey is triggered, it sends a HotkeyTriggered event to all
/// connected IPC clients through their event channels.
///
/// Use this with the event_senders from an IPCServer to bridge hotkey
/// events to IPC clients. The callback is thread-safe and can be cloned
/// for multiple hotkeys.
pub fn create_event_forwarder(
    event_senders: Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>>,
) -> impl Fn(&str) + Send + Sync + 'static {
    move |identifier| {
        let senders = event_senders.lock().unwrap();
        for sender in senders.iter() {
            let _ = sender.send(IPCResponse::HotkeyTriggered {
                identifier: identifier.to_string(),
            });
        }
    }
}
