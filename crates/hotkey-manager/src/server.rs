use crate::ipc::IPCServer;
use crate::manager::HotkeyManager;
use crate::{Error, Result, DEFAULT_SOCKET_PATH};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use tao::event::Event;
use tao::event_loop::{ControlFlow, EventLoop};
#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tracing::{debug, error, info, trace};

/// A hotkey server that manages the event loop and IPC communication
pub struct Server {
    socket_path: String,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    /// Create a new hotkey server with default configuration
    pub fn new() -> Self {
        Self {
            socket_path: DEFAULT_SOCKET_PATH.to_string(),
        }
    }

    /// Set the socket path for IPC communication
    pub fn with_socket_path(mut self, path: impl Into<String>) -> Self {
        self.socket_path = path.into();
        self
    }

    /// Run the server
    ///
    /// This will:
    /// 1. Create a tao event loop on the current thread (must be main thread on macOS)
    /// 2. Create a HotkeyManager
    /// 3. Start an IPC server in a background thread
    /// 4. Run the event loop until shutdown is requested
    ///
    /// The server will automatically shut down when:
    /// - The IPC client disconnects
    /// - An error occurs in the IPC server
    /// - The event loop is explicitly terminated
    pub fn run(self) -> Result<()> {
        info!("Starting hotkey server on socket: {}", self.socket_path);

        // Create the tao event loop (must be on main thread for macOS)
        let mut event_loop = EventLoop::new();
        
        // Set activation policy to Accessory on macOS to prevent dock icon
        #[cfg(target_os = "macos")]
        {
            event_loop.set_activation_policy(ActivationPolicy::Accessory);
        }

        // Create the hotkey manager
        debug!("Creating HotkeyManager");
        let manager = HotkeyManager::new()
            .map_err(|e| Error::HotkeyOperation(format!("Failed to create HotkeyManager: {e}")))?;
        info!("HotkeyManager created successfully");

        // Create the IPC server
        let ipc_server = IPCServer::new(&self.socket_path, manager);

        // Create shutdown coordination
        let shutdown_requested = Arc::new(AtomicBool::new(false));
        let shutdown_requested_clone = shutdown_requested.clone();

        // Spawn IPC server in background thread
        let _server_thread = thread::spawn(move || {
            // Create a tokio runtime for the IPC server
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("Failed to create tokio runtime: {}", e);
                    shutdown_requested_clone.store(true, Ordering::SeqCst);
                    return;
                }
            };

            info!("IPC server thread started, waiting for client connection...");

            // Run the IPC server
            runtime.block_on(async {
                if let Err(e) = ipc_server.run().await {
                    error!("IPC server error: {}", e);
                }
            });

            info!("IPC server thread ending, signaling shutdown");
            shutdown_requested_clone.store(true, Ordering::SeqCst);
        });

        // Run the event loop on the main thread
        info!("Starting tao event loop...");
        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;

            // Check for shutdown
            if shutdown_requested.load(Ordering::SeqCst) {
                info!("Shutdown requested, exiting event loop");
                *control_flow = ControlFlow::Exit;
                return;
            }

            // Process events (most are handled internally by tao/global-hotkey)
            match event {
                Event::NewEvents(_) | Event::MainEventsCleared | Event::RedrawEventsCleared => {
                    // These events fire frequently, ignore them
                }
                Event::LoopDestroyed => {
                    info!("Event loop destroyed");
                }
                _ => {
                    // Log other events at trace level for debugging
                    trace!("Event loop received: {:?}", event);
                }
            }
        });

        // The event loop runs forever and only exits when control flow is set to Exit
        // This Ok(()) is technically unreachable but required by the function signature
        #[allow(unreachable_code)]
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_with_methods() {
        // Test with_socket_path
        let server = Server::new().with_socket_path("/custom/path.sock");
        assert_eq!(server.socket_path, "/custom/path.sock");

        // Test chaining from new
        let server = Server::new()
            .with_socket_path("/initial/path.sock")
            .with_socket_path("/another/path.sock");
        assert_eq!(server.socket_path, "/another/path.sock");
    }

    #[test]
    fn test_server_default() {
        let server = Server::default();
        assert_eq!(server.socket_path, DEFAULT_SOCKET_PATH);
    }
}
