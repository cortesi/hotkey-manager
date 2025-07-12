use crate::ipc::{IPCClient, IPCConnection};
use crate::{Error, ProcessConfig, Result, ServerProcess, DEFAULT_SOCKET_PATH};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

/// Configuration for a managed client
#[derive(Debug, Clone)]
pub struct ManagedClientConfig {
    /// Socket path for IPC communication
    pub socket_path: String,
    /// Optional server configuration (if None, won't spawn server)
    pub server_config: Option<ProcessConfig>,
    /// How long to wait for server to be ready after spawning
    pub server_startup_timeout: Duration,
    /// How long to wait for initial connection
    pub connection_timeout: Duration,
    /// Number of connection attempts before giving up
    pub max_connection_attempts: u32,
    /// Delay between connection attempts
    pub connection_retry_delay: Duration,
}

impl Default for ManagedClientConfig {
    fn default() -> Self {
        Self {
            socket_path: DEFAULT_SOCKET_PATH.to_string(),
            server_config: None,
            server_startup_timeout: Duration::from_millis(1000),
            connection_timeout: Duration::from_secs(5),
            max_connection_attempts: 5,
            connection_retry_delay: Duration::from_millis(200),
        }
    }
}

impl ManagedClientConfig {
    /// Create a new configuration with the given socket path
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            ..Default::default()
        }
    }

    /// Set the server configuration for automatic spawning
    pub fn with_server(mut self, config: ProcessConfig) -> Self {
        self.server_config = Some(config);
        self
    }

    /// Set the server executable for automatic spawning (convenience method)
    pub fn with_server_executable(mut self, executable: impl Into<PathBuf>) -> Self {
        self.server_config = Some(ProcessConfig::new(executable));
        self
    }

    /// Set the server startup timeout
    pub fn server_startup_timeout(mut self, timeout: Duration) -> Self {
        self.server_startup_timeout = timeout;
        self
    }

    /// Set the connection timeout
    pub fn connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Set the maximum number of connection attempts
    pub fn max_connection_attempts(mut self, attempts: u32) -> Self {
        self.max_connection_attempts = attempts;
        self
    }

    /// Set the delay between connection retry attempts
    pub fn connection_retry_delay(mut self, delay: Duration) -> Self {
        self.connection_retry_delay = delay;
        self
    }
}

/// A managed client that can automatically spawn and connect to a server
pub struct Client {
    config: ManagedClientConfig,
    server: Option<ServerProcess>,
    connection: Option<IPCConnection>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Create a new managed client with default configuration
    pub fn new() -> Self {
        Self {
            config: ManagedClientConfig::default(),
            server: None,
            connection: None,
        }
    }

    /// Create a new managed client with the given socket path
    pub fn new_with_socket(socket_path: impl Into<String>) -> Self {
        Self {
            config: ManagedClientConfig::new(socket_path),
            server: None,
            connection: None,
        }
    }

    /// Set the socket path
    pub fn with_socket_path(mut self, socket_path: impl Into<String>) -> Self {
        self.config.socket_path = socket_path.into();
        self
    }

    /// Set the server configuration for automatic spawning
    pub fn with_server(mut self, config: ProcessConfig) -> Self {
        self.config.server_config = Some(config);
        self
    }

    /// Set the server executable for automatic spawning (convenience method)
    pub fn with_server_executable(mut self, executable: impl Into<PathBuf>) -> Self {
        self.config.server_config = Some(ProcessConfig::new(executable));
        self
    }

    /// Set the server startup timeout
    pub fn with_server_startup_timeout(mut self, timeout: Duration) -> Self {
        self.config.server_startup_timeout = timeout;
        self
    }

    /// Set the connection timeout
    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.config.connection_timeout = timeout;
        self
    }

    /// Set the maximum number of connection attempts
    pub fn with_max_connection_attempts(mut self, attempts: u32) -> Self {
        self.config.max_connection_attempts = attempts;
        self
    }

    /// Set the delay between connection retry attempts
    pub fn with_connection_retry_delay(mut self, delay: Duration) -> Self {
        self.config.connection_retry_delay = delay;
        self
    }

    /// Connect to the server, optionally spawning it first
    pub async fn connect(mut self) -> Result<Self> {
        // Check if we're already connected
        if self.connection.is_some() {
            debug!("Already connected to server");
            return Ok(self);
        }

        // Try to connect to existing server first
        info!(
            "Attempting to connect to existing server at {}",
            self.config.socket_path
        );
        match self.try_connect().await {
            Ok(connection) => {
                info!("Connected to existing server");
                self.connection = Some(connection);
                return Ok(self);
            }
            Err(e) => {
                debug!("Failed to connect to existing server: {}", e);
            }
        }

        // If we have server config, spawn the server
        if let Some(server_config) = &self.config.server_config {
            info!("No existing server found, spawning new server");

            let mut server = ServerProcess::new(server_config.clone());
            server.start().await?;

            // Try to connect with retries, polling for server readiness
            debug!(
                "Polling for server readiness (timeout: {:?})",
                self.config.server_startup_timeout
            );

            let start_time = tokio::time::Instant::now();
            let mut poll_interval = Duration::from_millis(10); // Start with fast polling
            let connection = loop {
                match self.try_connect().await {
                    Ok(conn) => {
                        let elapsed = start_time.elapsed();
                        info!("Successfully connected to spawned server in {:?}", elapsed);
                        break Some(conn);
                    }
                    Err(_) => {
                        // Check if we've exceeded the startup timeout
                        if start_time.elapsed() >= self.config.server_startup_timeout {
                            debug!("Server startup timeout reached, trying with retries");
                            break None;
                        }
                        // Server might not be ready yet, wait a bit and try again
                        sleep(poll_interval).await;

                        // Gradually increase polling interval to reduce CPU usage
                        // but keep it reasonably fast for quick startup
                        if poll_interval < Duration::from_millis(100) {
                            poll_interval = poll_interval.saturating_add(Duration::from_millis(10));
                        }
                    }
                }
            };

            match connection {
                Some(conn) => {
                    self.connection = Some(conn);
                    self.server = Some(server);
                    Ok(self)
                }
                None => {
                    // If we couldn't connect during startup timeout, try with normal retries
                    match self.try_connect_with_retries().await {
                        Ok(conn) => {
                            info!("Successfully connected to spawned server");
                            self.connection = Some(conn);
                            self.server = Some(server);
                            Ok(self)
                        }
                        Err(e) => {
                            error!("Failed to connect to spawned server: {}", e);
                            // Stop the server since we can't connect
                            server.stop().await?;
                            Err(e)
                        }
                    }
                }
            }
        } else {
            // No server config, so we can't spawn a server
            Err(Error::Ipc(
                "No server running and no server configuration provided".to_string(),
            ))
        }
    }

    /// Try to connect to the server once
    async fn try_connect(&self) -> Result<IPCConnection> {
        let client = IPCClient::new(&self.config.socket_path);

        match timeout(self.config.connection_timeout, client.connect()).await {
            Ok(Ok(connection)) => Ok(connection),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::Ipc(format!(
                "Connection timeout after {:?}",
                self.config.connection_timeout
            ))),
        }
    }

    /// Try to connect with retries
    async fn try_connect_with_retries(&self) -> Result<IPCConnection> {
        let mut last_error = None;

        for attempt in 1..=self.config.max_connection_attempts {
            debug!(
                "Connection attempt {}/{}",
                attempt, self.config.max_connection_attempts
            );

            match self.try_connect().await {
                Ok(connection) => return Ok(connection),
                Err(e) => {
                    warn!("Connection attempt {} failed: {}", attempt, e);
                    last_error = Some(e);

                    if attempt < self.config.max_connection_attempts {
                        sleep(self.config.connection_retry_delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            Error::Ipc("Failed to connect after all retry attempts".to_string())
        }))
    }

    /// Get a reference to the connection
    pub fn connection(&mut self) -> Result<&mut IPCConnection> {
        self.connection
            .as_mut()
            .ok_or_else(|| Error::Ipc("Not connected to server".to_string()))
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    /// Disconnect from the server and optionally stop it
    pub async fn disconnect(&mut self, stop_server: bool) -> Result<()> {
        // Shutdown the connection
        if let Some(mut connection) = self.connection.take() {
            info!("Shutting down connection");
            connection.shutdown().await?;
        }

        // Stop the server if requested and we spawned it
        if stop_server {
            if let Some(mut server) = self.server.take() {
                info!("Stopping managed server");
                server.stop().await?;
            }
        }

        Ok(())
    }

    /// Get the server process if we spawned one
    pub fn server(&self) -> Option<&ServerProcess> {
        self.server.as_ref()
    }

    /// Get the server PID if we spawned a server
    pub fn server_pid(&self) -> Option<u32> {
        self.server.as_ref().and_then(|s| s.pid())
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Clean disconnect on drop
        if self.is_connected() {
            warn!("ManagedClient dropped while still connected");
            // Can't do async in drop, so connection will close when dropped
        }

        // ServerProcess has its own drop implementation
        if self.server.is_some() {
            warn!("ManagedClient dropped with running server");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = ManagedClientConfig::new("/custom/socket.sock")
            .server_startup_timeout(Duration::from_secs(2))
            .connection_timeout(Duration::from_secs(10))
            .max_connection_attempts(3)
            .connection_retry_delay(Duration::from_millis(500));

        assert_eq!(config.socket_path, "/custom/socket.sock");
        assert_eq!(config.server_startup_timeout, Duration::from_secs(2));
        assert_eq!(config.connection_timeout, Duration::from_secs(10));
        assert_eq!(config.max_connection_attempts, 3);
        assert_eq!(config.connection_retry_delay, Duration::from_millis(500));
    }

    #[test]
    fn test_client_builder() {
        let client = Client::new_with_socket("/test/socket.sock").with_max_connection_attempts(10);

        assert_eq!(client.config.socket_path, "/test/socket.sock");
        assert_eq!(client.config.max_connection_attempts, 10);
    }

    #[test]
    fn test_client_default_socket_path() {
        let client = Client::new();
        assert_eq!(client.config.socket_path, DEFAULT_SOCKET_PATH);
    }
}
