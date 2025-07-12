use crate::error::{Error, Result};
use crate::Key;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyEvent, GlobalHotKeyManager};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, trace, warn};

/// Type alias for hotkey callbacks that receive the identifier
pub type HotkeyCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Represents a registered hotkey with its metadata
struct HotkeyEntry {
    /// The actual hotkey combination
    hotkey: HotKey,
    /// User-provided identifier for this hotkey
    identifier: String,
    /// Callback function to execute when the hotkey is pressed
    callback: HotkeyCallback,
}

/// A manager for global hotkeys that handles registration and callback execution.
pub struct HotkeyManager {
    manager: GlobalHotKeyManager,
    hotkeys: Arc<Mutex<HashMap<u32, HotkeyEntry>>>,
}

impl HotkeyManager {
    /// Creates a new `HotkeyManager` instance.
    ///
    /// This will spawn a background thread to listen for hotkey events.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying global hotkey manager fails to initialize.
    pub fn new() -> Result<Self> {
        trace!("Creating new HotkeyManager");
        let manager = GlobalHotKeyManager::new()?;
        debug!("GlobalHotKeyManager created successfully");

        let hotkeys = Arc::new(Mutex::new(HashMap::<u32, HotkeyEntry>::new()));
        let hotkeys_clone = hotkeys.clone();

        // Spawn a thread to listen for hotkey events
        std::thread::spawn(move || {
            info!("Hotkey event listener thread started");
            trace!("Thread ID: {:?}", std::thread::current().id());

            loop {
                trace!("Waiting for hotkey event...");
                match GlobalHotKeyEvent::receiver().recv() {
                    Ok(event) => {
                        info!(
                            "*** HOTKEY EVENT RECEIVED: id={}, state={:?}",
                            event.id, event.state
                        );
                        trace!(
                            "Received hotkey event: id={}, state={:?}",
                            event.id,
                            event.state
                        );

                        if event.state == global_hotkey::HotKeyState::Pressed {
                            debug!("Hotkey pressed event detected for id={}", event.id);

                            match hotkeys_clone.lock() {
                                Ok(hotkeys) => {
                                    trace!(
                                        "Successfully acquired hotkeys lock, checking {} entries",
                                        hotkeys.len()
                                    );

                                    if let Some(entry) = hotkeys.get(&event.id) {
                                        info!(
                                            "Triggering callback for identifier: '{}'",
                                            entry.identifier
                                        );
                                        trace!("About to call callback for '{}'", entry.identifier);
                                        (entry.callback)(&entry.identifier);
                                        trace!("Callback completed for '{}'", entry.identifier);
                                    } else {
                                        warn!("No hotkey entry found for id: {} (available IDs: {:?})", 
                                              event.id,
                                              hotkeys.keys().collect::<Vec<_>>());
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to acquire hotkeys lock: {:?}", e);
                                }
                            }
                        } else {
                            trace!("Ignoring hotkey event with state: {:?}", event.state);
                        }
                    }
                    Err(e) => {
                        error!("Error receiving hotkey event: {:?}", e);
                        trace!("Receiver error details: {:?}", e);
                    }
                }
            }
        });

        let result = Self { manager, hotkeys };
        info!("HotkeyManager initialized successfully");
        Ok(result)
    }

    /// Binds a new hotkey with a callback function.
    ///
    /// # Arguments
    ///
    /// * `identifier` - A string identifier for this hotkey
    /// * `key` - The key combination to bind
    /// * `callback` - The function to call when the hotkey is pressed (receives the identifier)
    ///
    /// # Returns
    ///
    /// Returns the unique ID of the registered hotkey on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the hotkey registration fails.
    pub fn bind<F>(
        &self,
        identifier: impl Into<String>,
        key: impl Into<Key>,
        callback: F,
    ) -> Result<u32>
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        let key = key.into();
        let hotkey = key.to_hotkey();
        let identifier = identifier.into();
        debug!(
            "Binding hotkey '{}': {:?} with id {}",
            identifier,
            key,
            hotkey.id()
        );
        trace!("Key details: {:?}", key);

        // Register with the system
        trace!("Registering hotkey with system...");
        self.manager.register(hotkey)?;
        info!(
            "Successfully registered hotkey '{}' with system",
            identifier
        );

        // Store the hotkey entry
        trace!("Acquiring hotkeys lock...");
        let mut hotkeys = self.hotkeys.lock().unwrap();
        let id = hotkey.id();
        trace!("Hotkey ID from hotkey.id(): {}", id);
        let entry = HotkeyEntry {
            hotkey,
            identifier: identifier.clone(),
            callback: Arc::new(callback),
        };
        hotkeys.insert(id, entry);
        debug!("Stored hotkey entry for '{}' with id {}", identifier, id);
        trace!("Total hotkeys registered: {}", hotkeys.len());
        trace!(
            "All registered hotkey IDs: {:?}",
            hotkeys.keys().collect::<Vec<_>>()
        );

        Ok(id)
    }

    /// Binds a new hotkey from a string representation.
    ///
    /// This is a convenience method that parses the key string before binding.
    pub fn bind_from_str<F>(
        &self,
        identifier: impl Into<String>,
        key_str: &str,
        callback: F,
    ) -> Result<u32>
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        trace!("Parsing key string: '{}'...", key_str);
        let key = Key::parse(key_str)?;
        debug!("Parsed key string '{}' successfully", key_str);
        self.bind(identifier, key, callback)
    }

    /// Unbinds a previously registered hotkey.
    ///
    /// # Arguments
    ///
    /// * `id` - The ID of the hotkey to unbind
    ///
    /// # Errors
    ///
    /// Returns an error if the hotkey is not found or unregistration fails.
    pub fn unbind(&self, id: u32) -> Result<()> {
        debug!("Unbinding hotkey with id {}", id);
        let mut hotkeys = self.hotkeys.lock().unwrap();

        if let Some(entry) = hotkeys.remove(&id) {
            info!("Unregistering hotkey '{}' (id: {})", entry.identifier, id);
            self.manager.unregister(entry.hotkey)?;
            trace!("Hotkey unregistered successfully");
            Ok(())
        } else {
            warn!("Attempted to unbind non-existent hotkey with id {}", id);
            Err(Error::HotkeyOperation("Hotkey not found".to_string()))
        }
    }

    /// Unbinds all registered hotkeys.
    ///
    /// # Errors
    ///
    /// Returns an error if any hotkey fails to unregister.
    pub fn unbind_all(&self) -> Result<()> {
        debug!("Unbinding all hotkeys");
        let mut hotkeys = self.hotkeys.lock().unwrap();
        let count = hotkeys.len();
        trace!("Found {} hotkeys to unbind", count);

        for (id, entry) in hotkeys.drain() {
            trace!("Unregistering hotkey '{}' (id: {})", entry.identifier, id);
            self.manager.unregister(entry.hotkey)?;
        }

        info!("Successfully unbound all {} hotkeys", count);
        Ok(())
    }

    /// Convenience method to bind multiple hotkeys with a single callback that receives the identifier.
    ///
    /// # Arguments
    ///
    /// * `hotkeys` - A slice of tuples containing (identifier, key)
    /// * `callback` - The function to call when any hotkey is pressed (receives the identifier)
    ///
    /// # Returns
    ///
    /// Returns a vector of results, one for each hotkey binding attempt.
    pub fn bind_multiple<F, K>(
        &self,
        hotkeys: &[(impl Into<String> + Clone, K)],
        callback: F,
    ) -> Vec<Result<u32>>
    where
        F: Fn(&str) + Send + Sync + 'static + Clone,
        K: Into<Key> + Clone,
    {
        hotkeys
            .iter()
            .map(|(id, key)| self.bind(id.clone(), key.clone(), callback.clone()))
            .collect()
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        debug!("Dropping HotkeyManager, cleaning up all hotkeys");
        // Clean up all hotkeys when the manager is dropped
        if let Err(e) = self.unbind_all() {
            error!("Failed to unbind all hotkeys during drop: {:?}", e);
        }
    }
}
