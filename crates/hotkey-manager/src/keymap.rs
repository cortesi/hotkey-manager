use global_hotkey::hotkey::HotKey;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Actions that can be triggered by hotkeys
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Execute a shell command
    Shell(String),
    /// Enter a new mode
    Mode(Mode),
    /// Return to the previous mode
    Pop,
    /// Exit the hotkey manager
    Exit,
}

impl Action {
    /// Create a Shell action
    pub fn shell(cmd: impl Into<String>) -> Self {
        Action::Shell(cmd.into())
    }
}

/// A collection of key bindings with their associated actions and descriptions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct Mode {
    keys: Vec<(String, String, Action)>,
}

impl Mode {
    /// Create a new Mode from an array of (key, name, action) tuples
    pub fn from_bindings<'a>(
        bindings: impl IntoIterator<Item = (&'a str, &'a str, Action)>,
    ) -> Self {
        Mode {
            keys: bindings
                .into_iter()
                .map(|(key, name, action)| (key.to_string(), name.to_string(), action))
                .collect(),
        }
    }

    /// Get the action associated with a key
    pub fn get(&self, key: &str) -> Option<&Action> {
        self.keys
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, _, action)| action)
    }

    /// Get both the name and action associated with a key
    pub fn get_with_name(&self, key: &str) -> Option<(&str, &Action)> {
        self.keys
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, name, action)| (name.as_str(), action))
    }

    /// Validate all key bindings in this mode and nested modes
    pub fn validate(&self) -> Result<(), String> {
        for (key, name, action) in &self.keys {
            // Try to parse the key with global_hotkey
            if let Err(e) = HotKey::from_str(key) {
                return Err(format!("Invalid key '{key}' ({name}): {e}"));
            }

            // Recursively validate nested modes
            if let Action::Mode(nested_mode) = action {
                nested_mode.validate()?;
            }
        }
        Ok(())
    }
}

/// Manages a stack of modes for hierarchical key binding navigation
#[derive(Debug)]
pub struct KeyMapState {
    mode_stack: Vec<Mode>,
}

impl Default for KeyMapState {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyMapState {
    /// Create a new empty keymap state
    pub fn new() -> Self {
        Self {
            mode_stack: Vec::new(),
        }
    }

    /// Push a new mode onto the stack
    pub fn push_mode(&mut self, mode: Mode) {
        self.mode_stack.push(mode);
    }

    /// Pop the current mode from the stack
    pub fn pop_mode(&mut self) -> Option<Mode> {
        self.mode_stack.pop()
    }

    /// Get a reference to the current mode
    pub fn current_mode(&self) -> Option<&Mode> {
        self.mode_stack.last()
    }

    /// Handle a key press in the current mode
    pub fn handle_key(&self, key: &str) -> Option<&Action> {
        self.current_mode().and_then(|mode| mode.get(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode() {
        let mode = Mode::from_bindings([
            ("q", "Exit", Action::Exit),
            ("s", "Shell", Action::shell("echo hello")),
        ]);

        assert!(matches!(mode.get("q"), Some(Action::Exit)));
        assert!(matches!(mode.get("s"), Some(Action::Shell(cmd)) if cmd == "echo hello"));
        assert_eq!(mode.get("x"), None);
    }

    #[test]
    fn test_nested_modes() {
        let submenu = Mode::from_bindings([
            ("x", "Exit", Action::shell("exit")),
            ("p", "Back", Action::Pop),
        ]);

        let main_mode = Mode::from_bindings([
            ("q", "Exit", Action::Exit),
            ("m", "Submenu", Action::Mode(submenu)),
        ]);

        assert!(matches!(main_mode.get("q"), Some(Action::Exit)));

        if let Some(Action::Mode(nested)) = main_mode.get("m") {
            assert!(matches!(nested.get("x"), Some(Action::Shell(cmd)) if cmd == "exit"));
        } else {
            panic!("Expected nested mode");
        }
    }

    #[test]
    fn test_keymap_manager() {
        let mut manager = KeyMapState::new();

        let mode2 = Mode::from_bindings([
            ("p", "Back", Action::Pop),
            ("s", "Shell", Action::shell("ls")),
        ]);

        let mode1 = Mode::from_bindings([
            ("q", "Exit", Action::Exit),
            ("2", "Mode 2", Action::Mode(mode2.clone())),
        ]);

        manager.push_mode(mode1);
        assert!(matches!(manager.handle_key("q"), Some(Action::Exit)));

        if let Some(Action::Mode(next_mode)) = manager.handle_key("2") {
            manager.push_mode(next_mode.clone());
        }

        assert!(matches!(manager.handle_key("p"), Some(Action::Pop)));
        assert!(matches!(manager.handle_key("s"), Some(Action::Shell(cmd)) if cmd == "ls"));

        manager.pop_mode();
        assert!(matches!(manager.handle_key("q"), Some(Action::Exit)));
    }

    #[test]
    fn test_mode_serialization() {
        let nested_mode = Mode::from_bindings([("x", "Exit", Action::shell("exit"))]);

        let mode = Mode::from_bindings([
            ("q", "Exit", Action::Exit),
            ("s", "Shell", Action::shell("echo hello")),
            ("p", "Back", Action::Pop),
            ("n", "Nested", Action::Mode(nested_mode)),
        ]);

        // Serialize to RON
        let ron_string = ron::to_string(&mode).unwrap();

        // Deserialize from RON
        let deserialized: Mode = ron::from_str(&ron_string).unwrap();

        // Verify they are equal
        assert_eq!(mode, deserialized);
        assert!(matches!(deserialized.get("q"), Some(Action::Exit)));
        assert!(matches!(deserialized.get("s"), Some(Action::Shell(cmd)) if cmd == "echo hello"));
    }

    #[test]
    fn test_ron_deserialization() {
        // RON text definition of nested modes
        let ron_text = r#"[
            ("q", "Exit", exit),
            ("h", "Hello", shell("echo 'Hello World'")),
            ("g", "Git", mode([
                ("s", "Status", shell("git status")),
                ("p", "Pull", shell("git pull")),
                ("c", "Commit", mode([
                    ("m", "Message", shell("git commit -m 'Quick commit'")),
                    ("a", "Amend", shell("git commit --amend")),
                    ("p", "Back", pop),
                ])),
                ("q", "Back", pop),
            ])),
            ("f", "Files", mode([
                ("l", "List", shell("ls -la")),
                ("t", "Tree", shell("tree")),
                ("q", "Back", pop),
            ])),
        ]"#;

        // Construct the expected mode structure
        let commit_mode = Mode::from_bindings([
            (
                "m",
                "Message",
                Action::shell("git commit -m 'Quick commit'"),
            ),
            ("a", "Amend", Action::shell("git commit --amend")),
            ("p", "Back", Action::Pop),
        ]);

        let git_mode = Mode::from_bindings([
            ("s", "Status", Action::shell("git status")),
            ("p", "Pull", Action::shell("git pull")),
            ("c", "Commit", Action::Mode(commit_mode)),
            ("q", "Back", Action::Pop),
        ]);

        let files_mode = Mode::from_bindings([
            ("l", "List", Action::shell("ls -la")),
            ("t", "Tree", Action::shell("tree")),
            ("q", "Back", Action::Pop),
        ]);

        let expected = Mode::from_bindings([
            ("q", "Exit", Action::Exit),
            ("h", "Hello", Action::shell("echo 'Hello World'")),
            ("g", "Git", Action::Mode(git_mode)),
            ("f", "Files", Action::Mode(files_mode)),
        ]);

        // Deserialize from RON text
        let deserialized: Mode = ron::from_str(ron_text).unwrap();

        // Compare the structures
        assert_eq!(deserialized, expected);
    }

    #[test]
    fn test_get_with_name() {
        // Test the get_with_name method
        let mode = Mode::from_bindings([
            ("q", "Exit", Action::Exit),
            ("s", "Shell", Action::shell("ls")),
        ]);

        // Test get_with_name
        if let Some((name, action)) = mode.get_with_name("q") {
            assert_eq!(name, "Exit");
            assert!(matches!(action, Action::Exit));
        } else {
            panic!("Expected to find 'q' key");
        }

        if let Some((name, action)) = mode.get_with_name("s") {
            assert_eq!(name, "Shell");
            assert!(matches!(action, Action::Shell(cmd) if cmd == "ls"));
        } else {
            panic!("Expected to find 's' key");
        }

        // Test with non-existent key
        assert!(mode.get_with_name("x").is_none());

        // Test RON text with tuples
        let ron_text = r#"[
            ("a", "Anonymous", shell("echo anonymous")),
            ("m", "Mode", mode([
                ("x", "Exit", pop),
            ])),
        ]"#;

        let mode: Mode = ron::from_str(ron_text).unwrap();
        assert!(matches!(mode.get("a"), Some(Action::Shell(cmd)) if cmd == "echo anonymous"));

        if let Some(Action::Mode(nested)) = mode.get("m") {
            assert!(matches!(nested.get("x"), Some(Action::Pop)));
        } else {
            panic!("Expected nested mode");
        }
    }

    #[test]
    fn test_validate_valid_keys() {
        // Simple valid keys
        let mode = Mode::from_bindings([
            ("ctrl+a", "Select All", Action::shell("select all")),
            ("cmd+c", "Copy", Action::shell("copy")),
            ("shift+f1", "Help", Action::shell("help")),
        ]);
        assert!(mode.validate().is_ok());

        // Nested modes with valid keys
        let nested = Mode::from_bindings([
            ("ctrl+s", "Save", Action::shell("save")),
            ("ctrl+shift+s", "Save As", Action::shell("save as")),
        ]);

        let main_mode = Mode::from_bindings([
            ("cmd+f", "File", Action::Mode(nested)),
            ("escape", "Exit", Action::Exit),
        ]);
        assert!(main_mode.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_keys() {
        // Invalid key at root level
        let mode = Mode::from_bindings([
            ("ctrl+a", "Valid", Action::shell("valid")),
            ("invalid key", "Invalid", Action::shell("invalid")),
        ]);
        let err = mode.validate().unwrap_err();
        assert!(err.contains("Invalid key 'invalid key' (Invalid)"));
        assert!(err.contains("Couldn't recognize"));

        // Invalid key in nested mode
        let nested = Mode::from_bindings([
            ("ctrl+s", "Save", Action::shell("save")),
            ("bad+key", "Bad", Action::shell("bad")),
        ]);

        let main_mode = Mode::from_bindings([
            ("cmd+f", "File", Action::Mode(nested)),
            ("escape", "Exit", Action::Exit),
        ]);
        let err = main_mode.validate().unwrap_err();
        assert!(err.contains("Invalid key 'bad+key' (Bad)"));
    }

    #[test]
    fn test_validate_deeply_nested() {
        // Create a deeply nested mode structure
        let level3 = Mode::from_bindings([
            ("ctrl+3", "Level 3", Action::shell("level3")),
            ("invalid", "Invalid", Action::shell("invalid")),
        ]);

        let level2 = Mode::from_bindings([("ctrl+2", "Level 2", Action::Mode(level3))]);

        let level1 = Mode::from_bindings([("ctrl+1", "Level 1", Action::Mode(level2))]);

        let err = level1.validate().unwrap_err();
        assert!(err.contains("Invalid key 'invalid' (Invalid)"));
    }
}
