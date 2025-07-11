use crate::HotkeyManager;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IPCRequest {
    ListHotkeys,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IPCResponse {
    Success {
        message: String,
        data: Option<serde_json::Value>,
    },
    Error {
        message: String,
    },
    HotkeyTriggered {
        identifier: String,
    },
}

pub struct IPCServer {
    socket_path: PathBuf,
    manager: Arc<HotkeyManager>,
    event_senders: Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>>,
}

impl IPCServer {
    /// Creates a new IPC server with a pre-configured HotkeyManager
    pub fn new(socket_path: impl Into<PathBuf>, manager: HotkeyManager) -> Self {
        let socket_path = socket_path.into();
        let event_senders = Arc::new(Mutex::new(Vec::new()));

        Self {
            socket_path,
            manager: Arc::new(manager),
            event_senders,
        }
    }

    /// Get a reference to the event senders for setting up hotkey callbacks
    pub fn event_senders(
        &self,
    ) -> Arc<Mutex<Vec<tokio::sync::mpsc::UnboundedSender<IPCResponse>>>> {
        self.event_senders.clone()
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        // Remove socket file if it exists
        let _ = std::fs::remove_file(&self.socket_path);

        let listener = UnixListener::bind(&self.socket_path)?;

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
}

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

pub struct IPCClient {
    socket_path: PathBuf,
}

impl IPCClient {
    pub fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub async fn connect(&self) -> Result<IPCConnection, Box<dyn std::error::Error>> {
        let stream = UnixStream::connect(&self.socket_path).await?;
        Ok(IPCConnection { stream })
    }
}

pub struct IPCConnection {
    stream: UnixStream,
}

impl IPCConnection {
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

    async fn recv_response(&mut self) -> Result<IPCResponse, Box<dyn std::error::Error>> {
        let mut len_bytes = [0u8; 4];
        self.stream.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data).await?;

        let response: IPCResponse = serde_json::from_slice(&data)?;
        Ok(response)
    }

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

    pub async fn shutdown(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.send_request(&IPCRequest::Shutdown).await?;
        Ok(())
    }

    pub async fn recv_event(&mut self) -> Result<IPCResponse, Box<dyn std::error::Error>> {
        self.recv_response().await
    }
}

/// Creates a callback that forwards hotkey events to all connected IPC clients
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
