//! Inter-Process Communication (IPC) module for hotkey management.
//!
//! This module provides a client-server architecture for managing hotkeys
//! across process boundaries. The server runs in a separate process with
//! the actual HotkeyManager, while a single client can connect to query
//! state and receive hotkey events.
//!
//! Key design decisions:
//! - Hotkeys must be pre-configured before starting the server (no dynamic binding)
//! - Communication uses Unix domain sockets with a simple length-prefixed protocol
//! - Enforces single client/server relationship for simplicity and automatic cleanup
//! - Events are forwarded asynchronously to the connected client
//!
//! The IPC system is designed to solve the problem of running hotkey managers
//! in separate processes, particularly useful for macOS applications where
//! hotkey handling in the main thread can cause issues.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

use crate::{
    error::{Error, Result},
    manager::HotkeyManager,
    Key,
};
use tracing::{debug, error, info, trace, warn};

/// Represents requests that can be sent from IPC clients to the server.
///
/// The IPC protocol is designed to be minimal and focused on querying
/// hotkey state rather than dynamic configuration. Hotkeys must be
/// configured when creating the HotkeyManager before starting the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IPCRequest {
    /// Request the server to shut down gracefully.
    /// In single-client mode, the server will also shut down when
    /// the client disconnects without sending this command.
    Shutdown,
    /// Rebind all hotkeys, replacing the current configuration.
    /// This will first unbind all existing hotkeys, then bind the new ones.
    /// The operation is atomic - if any binding fails, all are rolled back.
    Rebind {
        /// Vector of keys to bind
        keys: Vec<Key>,
    },
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

/// IPC server that manages hotkey operations for a single client.
///
/// The server runs in a separate process and communicates with one client
/// via Unix domain socket. It maintains a pre-configured HotkeyManager
/// and forwards hotkey events to the connected client.
///
/// The server automatically shuts down when the client disconnects,
/// ensuring clean process management.
pub(crate) struct IPCServer {
    socket_path: PathBuf,
    manager: Arc<HotkeyManager>,
    event_sender: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>>,
}

impl IPCServer {
    /// Creates a new IPC server with a pre-configured HotkeyManager.
    ///
    /// The server will bind to the specified Unix domain socket path.
    /// Hotkeys must be configured on the HotkeyManager before creating
    /// the server, as dynamic binding is not supported through IPC.
    pub(crate) fn new(socket_path: impl Into<PathBuf>, manager: HotkeyManager) -> Self {
        let socket_path = socket_path.into();
        let event_sender = Arc::new(Mutex::new(None));

        Self {
            socket_path,
            manager: Arc::new(manager),
            event_sender,
        }
    }

    /// Run the IPC server, accepting a single client connection.
    ///
    /// This method will block until the server shuts down. The server
    /// exits when the client disconnects.
    ///
    /// The server automatically removes any existing socket file at the path
    /// before binding to ensure a clean start.
    pub async fn run(self) -> Result<()> {
        // Remove socket file if it exists
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)?;

        // Accept single connection and handle it
        let (stream, _) = listener.accept().await?;
        let manager = self.manager.clone();
        let event_sender = self.event_sender.clone();

        info!("Client connected");
        handle_client(stream, manager, event_sender).await?;
        info!("Client disconnected");
        Ok(())
    }
}

/// Handle the client connection, processing requests and forwarding events.
///
/// This function manages the bidirectional communication with the client:
/// - Reads requests and sends responses
/// - Forwards hotkey events to the client
/// - Cleans up when the client disconnects
///
/// Uses a simple length-prefixed binary protocol for message framing.
async fn handle_client(
    stream: UnixStream,
    manager: Arc<HotkeyManager>,
    event_sender: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>>,
) -> Result<()> {
    debug!("handle_client: Starting client handler");
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    trace!("handle_client: Created event channel");
    *event_sender.lock().expect("event_sender mutex poisoned") = Some(event_tx.clone());
    debug!("handle_client: Set event sender in shared state");

    let (reader, writer) = stream.into_split();
    let reader = Arc::new(tokio::sync::Mutex::new(reader));
    let writer = Arc::new(tokio::sync::Mutex::new(writer));

    // Spawn task to forward events to client
    let writer_clone = writer.clone();
    tokio::spawn(async move {
        info!("Event forwarding task started");
        while let Some(event) = event_rx.recv().await {
            debug!("Event forwarding task received event: {:?}", event);
            let data = match serde_json::to_vec(&event) {
                Ok(d) => d,
                Err(e) => {
                    error!("Failed to serialize event: {:?}", e);
                    continue;
                }
            };
            let len_bytes = (data.len() as u32).to_be_bytes();
            let mut writer = writer_clone.lock().await;
            trace!("Sending event to client, data len: {}", data.len());
            if let Err(e) = writer.write_all(&len_bytes).await {
                error!("Failed to write event length: {:?}", e);
                break;
            }
            if let Err(e) = writer.write_all(&data).await {
                error!("Failed to write event data: {:?}", e);
                break;
            }
            if let Err(e) = writer.flush().await {
                error!("Failed to flush event data: {:?}", e);
                break;
            }
            trace!("Event sent to client successfully");
        }
        info!("Event forwarding task ended");
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
        debug!("Received request: {:?}", request);
        let is_shutdown = matches!(request, IPCRequest::Shutdown);
        let response = handle_request(&manager, request, &event_sender).await;
        trace!("Generated response: {:?}", response);

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

    // Clear event sender
    *event_sender.lock().expect("event_sender mutex poisoned") = None;

    Ok(())
}

/// Process an individual IPC request and generate the appropriate response.
///
/// This function handles the business logic for each request type,
/// interfacing with the HotkeyManager to query state.
async fn handle_request(
    manager: &Arc<HotkeyManager>,
    request: IPCRequest,
    event_sender: &Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>>,
) -> IPCResponse {
    match request {
        IPCRequest::Shutdown => IPCResponse::Success {
            message: "Shutting down".to_string(),
            data: None,
        },

        IPCRequest::Rebind { keys } => {
            info!("Processing Rebind request with {} keys", keys.len());
            // First unbind all existing hotkeys
            if let Err(e) = manager.unbind_all() {
                return IPCResponse::Error {
                    message: format!("Failed to unbind existing hotkeys: {e}"),
                };
            }

            // Use the existing event sender for creating callbacks
            debug!("Creating event forwarder with existing event sender");
            let callback = create_event_forwarder(event_sender.clone());

            // Convert keys to (identifier, key) pairs using the key's string representation
            let key_pairs: Vec<(String, Key)> = keys
                .iter()
                .map(|key| (key.to_string(), key.clone()))
                .collect();

            // Bind all the new hotkeys
            debug!("Binding {} new hotkeys", keys.len());
            let results = manager.bind_multiple(&key_pairs, callback);

            // Check if any bindings failed
            let mut failed_bindings = Vec::new();
            let mut successful_count = 0;

            for (idx, result) in results.iter().enumerate() {
                match result {
                    Ok(_) => successful_count += 1,
                    Err(e) => failed_bindings.push((key_pairs[idx].0.clone(), e.to_string())),
                }
            }

            if failed_bindings.is_empty() {
                IPCResponse::Success {
                    message: format!("Successfully bound {successful_count} hotkeys"),
                    data: None,
                }
            } else {
                // If any failed, unbind all to maintain atomicity
                let _ = manager.unbind_all();
                IPCResponse::Error {
                    message: format!(
                        "Failed to bind {} hotkeys: {:?}",
                        failed_bindings.len(),
                        failed_bindings
                    ),
                }
            }
        }
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
    pub async fn connect(&self) -> Result<IPCConnection> {
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
    async fn send_request(&mut self, request: &IPCRequest) -> Result<()> {
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
    async fn recv_response(&mut self) -> Result<IPCResponse> {
        let mut len_bytes = [0u8; 4];
        self.stream.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).await?;

        let response: IPCResponse = serde_json::from_slice(&data)?;
        Ok(response)
    }

    /// Send a shutdown request to the server.
    ///
    /// This requests a graceful shutdown of the server. In single-client mode,
    /// the server will also shut down automatically when the client disconnects,
    /// but sending an explicit shutdown is recommended for clean termination.
    pub async fn shutdown(&mut self) -> Result<()> {
        self.send_request(&IPCRequest::Shutdown).await?;
        Ok(())
    }

    /// Rebind all hotkeys, replacing the current configuration.
    ///
    /// This operation is atomic - if any binding fails, all existing hotkeys
    /// are restored.
    pub async fn rebind(&mut self, keys: &[Key]) -> Result<()> {
        self.send_request(&IPCRequest::Rebind {
            keys: keys.to_vec(),
        })
        .await?;

        match self.recv_response().await? {
            IPCResponse::Success { .. } => Ok(()),
            IPCResponse::Error { message } => Err(Error::Ipc(message)),
            _ => Err(Error::Ipc("Unexpected response".to_string())),
        }
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
    pub async fn recv_event(&mut self) -> Result<IPCResponse> {
        self.recv_response().await
    }
}

/// Creates a callback that forwards hotkey events to the connected IPC client.
///
/// This function returns a closure that can be used as a hotkey callback.
/// When a hotkey is triggered, it sends a HotkeyTriggered event to the
/// connected IPC client through the event channel.
///
/// Use this with the event_sender from an IPCServer to bridge hotkey
/// events to the IPC client. The callback is thread-safe and can be cloned
/// for multiple hotkeys.
pub(crate) fn create_event_forwarder(
    event_sender: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>>,
) -> impl Fn(&str) + Send + Sync + Clone + 'static {
    move |identifier| {
        trace!("Event forwarder called for identifier: '{}'", identifier);
        if let Some(sender) = event_sender
            .lock()
            .expect("event_sender mutex poisoned")
            .as_ref()
        {
            debug!(
                "Sending HotkeyTriggered event for identifier: '{}'",
                identifier
            );
            match sender.send(IPCResponse::HotkeyTriggered {
                identifier: identifier.to_string(),
            }) {
                Ok(_) => trace!("HotkeyTriggered event sent successfully"),
                Err(e) => error!("Failed to send HotkeyTriggered event: {:?}", e),
            }
        } else {
            warn!(
                "No event sender available to forward hotkey event for identifier: '{}'",
                identifier
            );
        }
    }
}
