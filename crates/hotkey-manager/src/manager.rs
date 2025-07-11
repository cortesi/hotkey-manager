use crate::Key;
use global_hotkey::{
    hotkey::HotKey,
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    manager: Arc<GlobalHotKeyManager>,
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
    pub fn new() -> Result<Self, String> {
        let manager = GlobalHotKeyManager::new()
            .map_err(|e| format!("Failed to create hotkey manager: {e}"))?;

        let hotkeys = Arc::new(Mutex::new(HashMap::<u32, HotkeyEntry>::new()));
        let hotkeys_clone = hotkeys.clone();

        // Spawn a thread to listen for hotkey events
        std::thread::spawn(move || loop {
            if let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
                if event.state == global_hotkey::HotKeyState::Pressed {
                    if let Ok(hotkeys) = hotkeys_clone.lock() {
                        if let Some(entry) = hotkeys.get(&event.id) {
                            (entry.callback)(&entry.identifier);
                        }
                    }
                }
            }
        });

        Ok(Self {
            manager: Arc::new(manager),
            hotkeys,
        })
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
    ) -> Result<u32, String>
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        let key = key.into();
        let hotkey = key.to_hotkey();

        // Register with the system
        self.manager
            .register(hotkey)
            .map_err(|e| format!("Failed to register hotkey: {e}"))?;

        // Store the hotkey entry
        let mut hotkeys = self.hotkeys.lock().unwrap();
        let id = hotkey.id();
        let entry = HotkeyEntry {
            hotkey,
            identifier: identifier.into(),
            callback: Arc::new(callback),
        };
        hotkeys.insert(id, entry);

        Ok(id)
    }

    /// Binds a new hotkey from a string representation.
    ///
    /// This is a convenience method that parses the key string before binding.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hotkey_manager::HotkeyManager;
    ///
    /// let manager = HotkeyManager::new().unwrap();
    /// manager.bind_from_str("copy", "ctrl+c", |_| println!("Copy!")).unwrap();
    /// ```
    pub fn bind_from_str<F>(
        &self,
        identifier: impl Into<String>,
        key_str: &str,
        callback: F,
    ) -> Result<u32, String>
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        let key = Key::parse(key_str)?;
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
    pub fn unbind(&self, id: u32) -> Result<(), String> {
        let mut hotkeys = self.hotkeys.lock().unwrap();

        if let Some(entry) = hotkeys.remove(&id) {
            self.manager
                .unregister(entry.hotkey)
                .map_err(|e| format!("Failed to unregister hotkey: {e}"))?;

            Ok(())
        } else {
            Err("Hotkey not found".to_string())
        }
    }

    /// Unbinds all registered hotkeys.
    ///
    /// # Errors
    ///
    /// Returns an error if any hotkey fails to unregister.
    pub fn unbind_all(&self) -> Result<(), String> {
        let mut hotkeys = self.hotkeys.lock().unwrap();

        for (_, entry) in hotkeys.drain() {
            self.manager
                .unregister(entry.hotkey)
                .map_err(|e| format!("Failed to unregister hotkey: {e}"))?;
        }

        Ok(())
    }

    /// Returns a list of all registered hotkeys.
    ///
    /// Each entry contains the hotkey ID, identifier, and a string representation of the hotkey.
    pub fn list_hotkeys(&self) -> Vec<(u32, String, String)> {
        let hotkeys = self.hotkeys.lock().unwrap();
        hotkeys
            .iter()
            .map(|(id, entry)| (*id, entry.identifier.clone(), format_hotkey(&entry.hotkey)))
            .collect()
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
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hotkey_manager::{HotkeyManager, Key};
    ///
    /// let manager = HotkeyManager::new().unwrap();
    /// let results = manager.bind_multiple(
    ///     &[
    ///         ("copy", Key::parse("ctrl+c").unwrap()),
    ///         ("paste", Key::parse("ctrl+v").unwrap()),
    ///     ],
    ///     |id| println!("Hotkey pressed: {}", id)
    /// );
    /// ```
    pub fn bind_multiple<F, K>(
        &self,
        hotkeys: &[(impl Into<String> + Clone, K)],
        callback: F,
    ) -> Vec<Result<u32, String>>
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
        // Clean up all hotkeys when the manager is dropped
        let _ = self.unbind_all();
    }
}

fn format_hotkey(hotkey: &HotKey) -> String {
    // This is a simplified formatter - could be expanded in the future
    format!("{hotkey:?}")
}