use crate::ipc::{IPCClient, IPCConnection};
use crate::process::ProcessConfig;
use crate::{Error, Result, ServerProcess, DEFAULT_SOCKET_PATH};
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use tracing::{debug, error, info, warn};

/// A client for connecting to a hotkey server.
///
/// The client will attempt to connect to an existing server at the configured socket path.
/// If no server is running and auto-spawn is configured, it will spawn a new server process.
///
/// # Server Spawning
///
/// By default, the client will only connect to existing servers. To enable automatic
/// server spawning, use one of these methods:
///
/// - [`with_auto_spawn_server()`](Self::with_auto_spawn_server) - Uses the current executable with `--server` flag
/// - [`with_server_command()`](Self::with_server_command) - Uses a custom command
pub struct Client {
    /// Socket path for IPC communication
    socket_path: String,
    /// Optional server configuration (if None, won't spawn server)
    server_config: Option<ProcessConfig>,
    /// How long to wait for server to be ready after spawning
    server_startup_timeout: Duration,
    /// How long to wait for initial connection
    connection_timeout: Duration,
    /// Number of connection attempts before giving up
    max_connection_attempts: u32,
    /// Delay between connection attempts
    connection_retry_delay: Duration,
    /// The spawned server process (if any)
    server: Option<ServerProcess>,
    /// The active IPC connection (if connected)
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
            socket_path: DEFAULT_SOCKET_PATH.to_string(),
            server_config: None,
            server_startup_timeout: Duration::from_millis(1000),
            connection_timeout: Duration::from_secs(5),
            max_connection_attempts: 5,
            connection_retry_delay: Duration::from_millis(200),
            server: None,
            connection: None,
        }
    }

    /// Create a new managed client with the given socket path
    pub fn new_with_socket(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
            server_config: None,
            server_startup_timeout: Duration::from_millis(1000),
            connection_timeout: Duration::from_secs(5),
            max_connection_attempts: 5,
            connection_retry_delay: Duration::from_millis(200),
            server: None,
            connection: None,
        }
    }

    /// Set the socket path
    pub fn with_socket_path(mut self, socket_path: impl Into<String>) -> Self {
        self.socket_path = socket_path.into();
        self
    }

    /// Enable automatic server spawning using the default command.
    ///
    /// The default command is the current executable with the "--server" argument.
    /// This is equivalent to calling `with_server_command(current_exe, ["--server"])`.
    pub fn with_auto_spawn_server(mut self) -> Self {
        if let Ok(current_exe) = std::env::current_exe() {
            self.server_config = Some(ProcessConfig::new(current_exe));
        }
        self
    }

    /// Set a custom server command for automatic spawning.
    ///
    /// Use this when you want to spawn a server with a specific command
    /// instead of the default (current executable with "--server" flag).
    ///
    /// # Arguments
    ///
    /// * `command` - The command to run (e.g., "/path/to/server")
    /// * `args` - Additional arguments to pass to the server command
    pub fn with_server_command<S, I, A>(mut self, command: S, args: I) -> Self
    where
        S: Into<PathBuf>,
        I: IntoIterator<Item = A>,
        A: AsRef<str>,
    {
        let mut config = ProcessConfig::new(command);
        config.args = args.into_iter().map(|s| s.as_ref().to_string()).collect();
        self.server_config = Some(config);
        self
    }

    /// Set the server startup timeout
    pub fn with_server_startup_timeout(mut self, timeout: Duration) -> Self {
        self.server_startup_timeout = timeout;
        self
    }

    /// Set the connection timeout
    pub fn with_connection_timeout(mut self, timeout: Duration) -> Self {
        self.connection_timeout = timeout;
        self
    }

    /// Set the maximum number of connection attempts
    pub fn with_max_connection_attempts(mut self, attempts: u32) -> Self {
        self.max_connection_attempts = attempts;
        self
    }

    /// Set the delay between connection retry attempts
    pub fn with_connection_retry_delay(mut self, delay: Duration) -> Self {
        self.connection_retry_delay = delay;
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
            self.socket_path
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
        if let Some(server_config) = &self.server_config {
            info!("No existing server found, spawning new server");

            let mut server = ServerProcess::new(server_config.clone());
            server.start().await?;

            // Try to connect with retries, polling for server readiness
            debug!(
                "Polling for server readiness (timeout: {:?})",
                self.server_startup_timeout
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
                        if start_time.elapsed() >= self.server_startup_timeout {
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
        let client = IPCClient::new(&self.socket_path);

        match timeout(self.connection_timeout, client.connect()).await {
            Ok(Ok(connection)) => Ok(connection),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(Error::Ipc(format!(
                "Connection timeout after {:?}",
                self.connection_timeout
            ))),
        }
    }

    /// Try to connect with retries
    async fn try_connect_with_retries(&self) -> Result<IPCConnection> {
        let mut last_error = None;

        for attempt in 1..=self.max_connection_attempts {
            debug!(
                "Connection attempt {}/{}",
                attempt, self.max_connection_attempts
            );

            match self.try_connect().await {
                Ok(connection) => return Ok(connection),
                Err(e) => {
                    warn!("Connection attempt {} failed: {}", attempt, e);
                    last_error = Some(e);

                    if attempt < self.max_connection_attempts {
                        sleep(self.connection_retry_delay).await;
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

    /// Get the PID of the spawned server process, if any.
    ///
    /// Returns `None` if no server was spawned (e.g., connected to an existing server)
    /// or if the server process has terminated.
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
    fn test_client_builder() {
        let client = Client::new_with_socket("/test/socket.sock")
            .with_max_connection_attempts(10)
            .with_server_startup_timeout(Duration::from_secs(2))
            .with_connection_timeout(Duration::from_secs(10))
            .with_connection_retry_delay(Duration::from_millis(500));

        assert_eq!(client.socket_path, "/test/socket.sock");
        assert_eq!(client.max_connection_attempts, 10);
        assert_eq!(client.server_startup_timeout, Duration::from_secs(2));
        assert_eq!(client.connection_timeout, Duration::from_secs(10));
        assert_eq!(client.connection_retry_delay, Duration::from_millis(500));
    }

    #[test]
    fn test_client_default_socket_path() {
        let client = Client::new();
        assert_eq!(client.socket_path, DEFAULT_SOCKET_PATH);
    }
}
