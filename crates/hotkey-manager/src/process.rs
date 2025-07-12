use crate::{Error, Result};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// Default delay to wait for server startup
const DEFAULT_STARTUP_DELAY: Duration = Duration::from_millis(500);

/// Configuration for launching a hotkey server process
#[derive(Debug, Clone)]
pub struct ProcessConfig {
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

    /// Add an argument to pass to the server
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Add multiple arguments
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(|s| s.into()));
        self
    }

    /// Set an environment variable
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Set the startup delay
    pub fn startup_delay(mut self, delay: Duration) -> Self {
        self.startup_delay = delay;
        self
    }

    /// Set whether to inherit the parent's environment
    pub fn inherit_env(mut self, inherit: bool) -> Self {
        self.inherit_env = inherit;
        self
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
    pub fn new(config: ProcessConfig) -> Self {
        Self {
            child: None,
            config,
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Start the server process
    pub async fn start(&mut self) -> Result<()> {
        if self.is_running() {
            return Err(Error::HotkeyOperation("Server is already running".to_string()));
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
        let child = command
            .spawn()
            .map_err(Error::Io)?;

        let pid = child.id();
        info!("Server process spawned with PID: {}", pid);

        self.child = Some(child);
        self.is_running.store(true, Ordering::SeqCst);

        // Wait for startup
        debug!("Waiting {:?} for server startup", self.config.startup_delay);
        sleep(self.config.startup_delay).await;

        // Check if process is still running
        if !self.is_running() {
            return Err(Error::HotkeyOperation("Server process died during startup".to_string()));
        }

        Ok(())
    }

    /// Stop the server process
    pub async fn stop(&mut self) -> Result<()> {
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

    /// Restart the server process
    pub async fn restart(&mut self) -> Result<()> {
        info!("Restarting server process");
        self.stop().await?;
        self.start().await?;
        Ok(())
    }

    /// Check if the server process is running
    pub fn is_running(&self) -> bool {
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

    /// Get a reference to the process configuration
    pub fn config(&self) -> &ProcessConfig {
        &self.config
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        if self.is_running() {
            warn!("ServerProcess dropped while still running, attempting to stop");
            // Block on stopping the process
            let runtime = tokio::runtime::Handle::try_current();
            if let Ok(handle) = runtime {
                let _ = handle.block_on(self.stop());
            } else {
                // Fallback to synchronous kill if no runtime
                if let Some(mut child) = self.child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }
}

/// Builder for creating a ServerProcess with fluent API
pub struct ProcessBuilder {
    config: ProcessConfig,
}

impl ProcessBuilder {
    /// Create a new process builder with the given executable
    pub fn new(executable: impl Into<PathBuf>) -> Self {
        Self {
            config: ProcessConfig::new(executable),
        }
    }

    /// Add an argument to pass to the server
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.config = self.config.arg(arg);
        self
    }

    /// Add multiple arguments
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.config = self.config.args(args);
        self
    }

    /// Set an environment variable
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config = self.config.env(key, value);
        self
    }

    /// Set the startup delay
    pub fn startup_delay(mut self, delay: Duration) -> Self {
        self.config = self.config.startup_delay(delay);
        self
    }

    /// Set whether to inherit the parent's environment
    pub fn inherit_env(mut self, inherit: bool) -> Self {
        self.config = self.config.inherit_env(inherit);
        self
    }

    /// Build the ServerProcess
    pub fn build(self) -> ServerProcess {
        ServerProcess::new(self.config)
    }

    /// Build and start the ServerProcess
    pub async fn start(self) -> Result<ServerProcess> {
        let mut process = self.build();
        process.start().await?;
        Ok(process)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_config() {
        let config = ProcessConfig::new("/usr/bin/test")
            .arg("--verbose")
            .args(vec!["--port", "8080"])
            .env("RUST_LOG", "debug")
            .startup_delay(Duration::from_secs(1))
            .inherit_env(false);

        assert_eq!(config.executable, PathBuf::from("/usr/bin/test"));
        assert_eq!(config.args, vec!["--server", "--verbose", "--port", "8080"]);
        assert_eq!(config.env, vec![("RUST_LOG".to_string(), "debug".to_string())]);
        assert_eq!(config.startup_delay, Duration::from_secs(1));
        assert!(!config.inherit_env);
    }

    #[test]
    fn test_process_builder() {
        let process = ProcessBuilder::new("/usr/bin/test")
            .arg("--verbose")
            .env("TEST", "value")
            .build();

        assert_eq!(process.config().executable, PathBuf::from("/usr/bin/test"));
        assert_eq!(process.config().args, vec!["--server", "--verbose"]);
    }
}