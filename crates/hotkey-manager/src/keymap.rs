use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Actions that can be triggered by hotkeys
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

/// A collection of key bindings with their associated actions and descriptions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Mode {
    keys: HashMap<String, (String, Action)>,
}

impl Mode {
    /// Get the action associated with a key
    pub fn get(&self, key: &str) -> Option<&Action> {
        self.keys.get(key).map(|(_, action)| action)
    }

    /// Get both the name and action associated with a key
    pub fn get_with_name(&self, key: &str) -> Option<(&str, &Action)> {
        self.keys
            .get(key)
            .map(|(name, action)| (name.as_str(), action))
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
        let mode = Mode {
            keys: HashMap::from([
                ("q".to_string(), ("Exit".to_string(), Action::Exit)),
                (
                    "s".to_string(),
                    ("Shell".to_string(), Action::Shell("echo hello".to_string())),
                ),
            ]),
        };

        assert!(matches!(mode.get("q"), Some(Action::Exit)));
        assert!(matches!(mode.get("s"), Some(Action::Shell(cmd)) if cmd == "echo hello"));
        assert_eq!(mode.get("x"), None);
    }

    #[test]
    fn test_nested_modes() {
        let submenu = Mode {
            keys: HashMap::from([
                (
                    "x".to_string(),
                    ("Exit".to_string(), Action::Shell("exit".to_string())),
                ),
                ("p".to_string(), ("Back".to_string(), Action::Pop)),
            ]),
        };

        let main_mode = Mode {
            keys: HashMap::from([
                ("q".to_string(), ("Exit".to_string(), Action::Exit)),
                (
                    "m".to_string(),
                    ("Submenu".to_string(), Action::Mode(submenu)),
                ),
            ]),
        };

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

        let mode2 = Mode {
            keys: HashMap::from([
                ("p".to_string(), ("Back".to_string(), Action::Pop)),
                (
                    "s".to_string(),
                    ("Shell".to_string(), Action::Shell("ls".to_string())),
                ),
            ]),
        };

        let mode1 = Mode {
            keys: HashMap::from([
                ("q".to_string(), ("Exit".to_string(), Action::Exit)),
                (
                    "2".to_string(),
                    ("Mode 2".to_string(), Action::Mode(mode2.clone())),
                ),
            ]),
        };

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
        let nested_mode = Mode {
            keys: HashMap::from([(
                "x".to_string(),
                ("Exit".to_string(), Action::Shell("exit".to_string())),
            )]),
        };

        let mode = Mode {
            keys: HashMap::from([
                ("q".to_string(), ("Exit".to_string(), Action::Exit)),
                (
                    "s".to_string(),
                    ("Shell".to_string(), Action::Shell("echo hello".to_string())),
                ),
                ("p".to_string(), ("Back".to_string(), Action::Pop)),
                (
                    "n".to_string(),
                    ("Nested".to_string(), Action::Mode(nested_mode)),
                ),
            ]),
        };

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
        let ron_text = r#"(
            keys: {
                "q": ("Exit", Exit),
                "h": ("Hello", Shell("echo 'Hello World'")),
                "g": ("Git", Mode((
                    keys: {
                        "s": ("Status", Shell("git status")),
                        "p": ("Pull", Shell("git pull")),
                        "c": ("Commit", Mode((
                            keys: {
                                "m": ("Message", Shell("git commit -m 'Quick commit'")),
                                "a": ("Amend", Shell("git commit --amend")),
                                "p": ("Back", Pop),
                            }
                        ))),
                        "q": ("Back", Pop),
                    }
                ))),
                "f": ("Files", Mode((
                    keys: {
                        "l": ("List", Shell("ls -la")),
                        "t": ("Tree", Shell("tree")),
                        "q": ("Back", Pop),
                    }
                ))),
            }
        )"#;

        // Construct the expected mode structure
        let commit_mode = Mode {
            keys: HashMap::from([
                (
                    "m".to_string(),
                    (
                        "Message".to_string(),
                        Action::Shell("git commit -m 'Quick commit'".to_string()),
                    ),
                ),
                (
                    "a".to_string(),
                    (
                        "Amend".to_string(),
                        Action::Shell("git commit --amend".to_string()),
                    ),
                ),
                ("p".to_string(), ("Back".to_string(), Action::Pop)),
            ]),
        };

        let git_mode = Mode {
            keys: HashMap::from([
                (
                    "s".to_string(),
                    (
                        "Status".to_string(),
                        Action::Shell("git status".to_string()),
                    ),
                ),
                (
                    "p".to_string(),
                    ("Pull".to_string(), Action::Shell("git pull".to_string())),
                ),
                (
                    "c".to_string(),
                    ("Commit".to_string(), Action::Mode(commit_mode)),
                ),
                ("q".to_string(), ("Back".to_string(), Action::Pop)),
            ]),
        };

        let files_mode = Mode {
            keys: HashMap::from([
                (
                    "l".to_string(),
                    ("List".to_string(), Action::Shell("ls -la".to_string())),
                ),
                (
                    "t".to_string(),
                    ("Tree".to_string(), Action::Shell("tree".to_string())),
                ),
                ("q".to_string(), ("Back".to_string(), Action::Pop)),
            ]),
        };

        let expected = Mode {
            keys: HashMap::from([
                ("q".to_string(), ("Exit".to_string(), Action::Exit)),
                (
                    "h".to_string(),
                    (
                        "Hello".to_string(),
                        Action::Shell("echo 'Hello World'".to_string()),
                    ),
                ),
                ("g".to_string(), ("Git".to_string(), Action::Mode(git_mode))),
                (
                    "f".to_string(),
                    ("Files".to_string(), Action::Mode(files_mode)),
                ),
            ]),
        };

        // Deserialize from RON text
        let deserialized: Mode = ron::from_str(ron_text).unwrap();

        // Compare the structures
        assert_eq!(deserialized, expected);
    }

    #[test]
    fn test_get_with_name() {
        // Test the get_with_name method
        let mode = Mode {
            keys: HashMap::from([
                ("q".to_string(), ("Exit".to_string(), Action::Exit)),
                (
                    "s".to_string(),
                    ("Shell".to_string(), Action::Shell("ls".to_string())),
                ),
            ]),
        };

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
        let ron_text = r#"(
            keys: {
                "a": ("Anonymous", Shell("echo anonymous")),
                "m": ("Mode", Mode((
                    keys: {
                        "x": ("Exit", Pop),
                    }
                ))),
            }
        )"#;

        let mode: Mode = ron::from_str(ron_text).unwrap();
        assert!(matches!(mode.get("a"), Some(Action::Shell(cmd)) if cmd == "echo anonymous"));

        if let Some(Action::Mode(nested)) = mode.get("m") {
            assert!(matches!(nested.get("x"), Some(Action::Pop)));
        } else {
            panic!("Expected nested mode");
        }
    }
}
