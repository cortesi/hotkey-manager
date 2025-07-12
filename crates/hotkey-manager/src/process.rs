use crate::{Error, Result};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Default delay to wait for server startup
pub(crate) const DEFAULT_STARTUP_DELAY: Duration = Duration::from_millis(500);

/// Configuration for launching a hotkey server process
#[derive(Debug, Clone)]
pub(crate) struct ProcessConfig {
    /// Path to the executable
    pub executable: PathBuf,
    /// Arguments to pass to the server
    pub args: Vec<String>,
    /// Environment variables to set
    pub env: Vec<(String, String)>,
    /// How long to wait after spawning before considering it "started"
    pub startup_delay: Duration,
    /// Whether to inherit the parent's environment
    pub inherit_env: bool,
}

impl ProcessConfig {
    /// Create a new process configuration with the given executable
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            args: vec!["--server".to_string()],
            env: Vec::new(),
            startup_delay: DEFAULT_STARTUP_DELAY,
            inherit_env: true,
        }
    }
}

/// A managed server process for hotkey handling
pub struct ServerProcess {
    child: Option<Child>,
    config: ProcessConfig,
    is_running: Arc<AtomicBool>,
}

impl ServerProcess {
    /// Create a new server process with the given configuration
    pub(crate) fn new(config: ProcessConfig) -> Self {
        Self {
            child: None,
            config,
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the server process
    pub(crate) async fn start(&mut self) -> Result<()> {
        if self.is_running() {
            return Err(Error::HotkeyOperation(
                "Server is already running".to_string(),
            ));
        }

        info!("Starting server process: {:?}", self.config.executable);
        debug!("Server args: {:?}", self.config.args);

        let mut command = Command::new(&self.config.executable);

        // Add arguments
        for arg in &self.config.args {
            command.arg(arg);
        }

        // Configure environment
        if !self.config.inherit_env {
            command.env_clear();
        }

        for (key, value) in &self.config.env {
            command.env(key, value);
        }

        // Spawn the process
        let child = command.spawn().map_err(Error::Io)?;

        let pid = child.id();
        info!("Server process spawned with PID: {}", pid);

        self.child = Some(child);
        self.is_running.store(true, Ordering::SeqCst);

        // Wait for startup
        debug!("Waiting {:?} for server startup", self.config.startup_delay);
        sleep(self.config.startup_delay).await;

        // Check if process is still running
        if !self.is_running() {
            return Err(Error::HotkeyOperation(
                "Server process died during startup".to_string(),
            ));
        }

        Ok(())
    }

    /// Stop the server process
    pub(crate) async fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            info!("Stopping server process");

            // Try graceful termination first
            if let Err(e) = child.kill() {
                error!("Failed to kill server process: {}", e);
                return Err(Error::Io(e));
            }

            // Wait for the process to exit
            match child.wait() {
                Ok(status) => {
                    info!("Server process exited with status: {:?}", status);
                }
                Err(e) => {
                    warn!("Failed to wait for server process: {}", e);
                }
            }

            self.is_running.store(false, Ordering::SeqCst);
        }

        Ok(())
    }

    /// Check if the server process is running
    pub(crate) fn is_running(&self) -> bool {
        if let Some(child) = self.child.as_ref() {
            // Try to get the process status without waiting
            match std::process::Command::new("kill")
                .args(["-0", &child.id().to_string()])
                .output()
            {
                Ok(output) => {
                    let is_running = output.status.success();
                    self.is_running.store(is_running, Ordering::SeqCst);
                    is_running
                }
                Err(_) => {
                    // If we can't check, assume it's not running
                    self.is_running.store(false, Ordering::SeqCst);
                    false
                }
            }
        } else {
            false
        }
    }

    /// Get the process ID if running
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(|c| c.id())
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        if self.is_running() {
            warn!("ServerProcess dropped while still running, attempting to stop");
            // Always use synchronous kill to avoid runtime issues
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
                self.is_running.store(false, Ordering::SeqCst);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_config() {
        let config = ProcessConfig::new("/usr/bin/test");

        assert_eq!(config.executable, PathBuf::from("/usr/bin/test"));
        assert_eq!(config.args, vec!["--server"]);
        assert_eq!(config.env, Vec::<(String, String)>::new());
        assert_eq!(config.startup_delay, DEFAULT_STARTUP_DELAY);
        assert!(config.inherit_env);
    }
}
