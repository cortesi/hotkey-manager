use global_hotkey::hotkey::HotKey;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Attributes for key bindings
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Attrs {
    #[serde(default)]
    pub noexit: bool,
}

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
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mode {
    keys: Vec<(String, String, Action, Attrs)>,
}

// Manual Serialize implementation that respects transparent
impl Serialize for Mode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.keys.serialize(serializer)
    }
}

// Custom deserializer that accepts both 3-tuples and 4-tuples
impl<'de> Deserialize<'de> for Mode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Entry {
            Simple(String, String, Action),
            WithAttrs(String, String, Action, Attrs),
        }

        let entries = Vec::<Entry>::deserialize(deserializer)?;
        let keys = entries
            .into_iter()
            .map(|entry| match entry {
                Entry::Simple(k, n, a) => (k, n, a, Attrs::default()),
                Entry::WithAttrs(k, n, a, attrs) => (k, n, a, attrs),
            })
            .collect();

        Ok(Mode { keys })
    }
}

impl Mode {
    /// Get the action and attributes associated with a key
    pub fn get_with_attrs(&self, key: &str) -> Option<(&Action, &Attrs)> {
        self.keys
            .iter()
            .find(|(k, _, _, _)| k == key)
            .map(|(_, _, action, attrs)| (action, attrs))
    }

    /// Validate all key bindings in this mode and nested modes
    pub fn validate(&self) -> Result<(), String> {
        for (key, name, action, _) in &self.keys {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode() {
        let mode: Mode = ron::from_str(
            r#"[
            ("q", "Exit", exit),
            ("s", "Shell", shell("echo hello")),
        ]"#,
        )
        .unwrap();

        assert!(matches!(mode.get_with_attrs("q"), Some((Action::Exit, _))));
        assert!(
            matches!(mode.get_with_attrs("s"), Some((Action::Shell(cmd), _)) if cmd == "echo hello")
        );
        assert_eq!(mode.get_with_attrs("x"), None);
    }

    #[test]
    fn test_nested_modes() {
        let ron_text = r#"[
            ("q", "Exit", exit),
            ("m", "Submenu", mode([
                ("x", "Exit", shell("exit")),
                ("p", "Back", pop),
            ])),
        ]"#;

        let main_mode: Mode = ron::from_str(ron_text).unwrap();

        assert!(matches!(
            main_mode.get_with_attrs("q"),
            Some((Action::Exit, _))
        ));

        if let Some((Action::Mode(nested), _)) = main_mode.get_with_attrs("m") {
            assert!(
                matches!(nested.get_with_attrs("x"), Some((Action::Shell(cmd), _)) if cmd == "exit")
            );
        } else {
            panic!("Expected nested mode");
        }
    }

    #[test]
    fn test_mode_serialization() {
        let mode: Mode = ron::from_str(
            r#"[
            ("q", "Exit", exit),
            ("s", "Shell", shell("echo hello")),
            ("p", "Back", pop),
            ("n", "Nested", mode([
                ("x", "Exit", shell("exit")),
            ])),
        ]"#,
        )
        .unwrap();

        // Serialize to RON
        let ron_string = ron::to_string(&mode).unwrap();

        // Deserialize from RON
        let deserialized: Mode = ron::from_str(&ron_string).unwrap();

        // Verify they are equal
        assert_eq!(mode, deserialized);
        assert!(matches!(
            deserialized.get_with_attrs("q"),
            Some((Action::Exit, _))
        ));
        assert!(
            matches!(deserialized.get_with_attrs("s"), Some((Action::Shell(cmd), _)) if cmd == "echo hello")
        );
    }

    #[test]
    fn test_ron_deserialization() {
        // RON text definition of nested modes - testing both with and without attrs
        let ron_text = r#"[
            ("q", "Exit", exit),
            ("h", "Hello", shell("echo 'Hello World'")),
            ("g", "Git", mode([
                ("s", "Status", shell("git status")),
                ("l", "Log", shell("git log"), (noexit: true)),
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
                ("t", "Tree", shell("tree"), (noexit: true)),
                ("q", "Back", pop),
            ])),
        ]"#;

        // Manually construct the expected mode structure
        let commit_mode = Mode {
            keys: vec![
                (
                    "m".to_string(),
                    "Message".to_string(),
                    Action::shell("git commit -m 'Quick commit'"),
                    Attrs::default(),
                ),
                (
                    "a".to_string(),
                    "Amend".to_string(),
                    Action::shell("git commit --amend"),
                    Attrs::default(),
                ),
                (
                    "p".to_string(),
                    "Back".to_string(),
                    Action::Pop,
                    Attrs::default(),
                ),
            ],
        };

        let git_mode = Mode {
            keys: vec![
                (
                    "s".to_string(),
                    "Status".to_string(),
                    Action::shell("git status"),
                    Attrs::default(),
                ),
                (
                    "l".to_string(),
                    "Log".to_string(),
                    Action::shell("git log"),
                    Attrs { noexit: true },
                ),
                (
                    "p".to_string(),
                    "Pull".to_string(),
                    Action::shell("git pull"),
                    Attrs::default(),
                ),
                (
                    "c".to_string(),
                    "Commit".to_string(),
                    Action::Mode(commit_mode),
                    Attrs::default(),
                ),
                (
                    "q".to_string(),
                    "Back".to_string(),
                    Action::Pop,
                    Attrs::default(),
                ),
            ],
        };

        let files_mode = Mode {
            keys: vec![
                (
                    "l".to_string(),
                    "List".to_string(),
                    Action::shell("ls -la"),
                    Attrs::default(),
                ),
                (
                    "t".to_string(),
                    "Tree".to_string(),
                    Action::shell("tree"),
                    Attrs { noexit: true },
                ),
                (
                    "q".to_string(),
                    "Back".to_string(),
                    Action::Pop,
                    Attrs::default(),
                ),
            ],
        };

        let expected = Mode {
            keys: vec![
                (
                    "q".to_string(),
                    "Exit".to_string(),
                    Action::Exit,
                    Attrs::default(),
                ),
                (
                    "h".to_string(),
                    "Hello".to_string(),
                    Action::shell("echo 'Hello World'"),
                    Attrs::default(),
                ),
                (
                    "g".to_string(),
                    "Git".to_string(),
                    Action::Mode(git_mode),
                    Attrs::default(),
                ),
                (
                    "f".to_string(),
                    "Files".to_string(),
                    Action::Mode(files_mode),
                    Attrs::default(),
                ),
            ],
        };

        // Deserialize from RON text
        let deserialized: Mode = ron::from_str(ron_text).unwrap();

        // Compare the structures
        assert_eq!(deserialized, expected);
    }

    #[test]
    fn test_validate_valid_keys() {
        // Simple valid keys
        let mode: Mode = ron::from_str(
            r#"[
            ("ctrl+a", "Select All", shell("select all")),
            ("cmd+c", "Copy", shell("copy")),
            ("shift+f1", "Help", shell("help")),
        ]"#,
        )
        .unwrap();
        assert!(mode.validate().is_ok());

        // Nested modes with valid keys
        let main_mode: Mode = ron::from_str(
            r#"[
            ("cmd+f", "File", mode([
                ("ctrl+s", "Save", shell("save")),
                ("ctrl+shift+s", "Save As", shell("save as")),
            ])),
            ("escape", "Exit", exit),
        ]"#,
        )
        .unwrap();
        assert!(main_mode.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_keys() {
        // Invalid key at root level
        let mode: Mode = ron::from_str(
            r#"[
            ("ctrl+a", "Valid", shell("valid")),
            ("invalid key", "Invalid", shell("invalid")),
        ]"#,
        )
        .unwrap();
        let err = mode.validate().unwrap_err();
        assert!(err.contains("Invalid key 'invalid key' (Invalid)"));
        assert!(err.contains("Couldn't recognize"));

        // Invalid key in nested mode
        let main_mode: Mode = ron::from_str(
            r#"[
            ("cmd+f", "File", mode([
                ("ctrl+s", "Save", shell("save")),
                ("bad+key", "Bad", shell("bad")),
            ])),
            ("escape", "Exit", exit),
        ]"#,
        )
        .unwrap();
        let err = main_mode.validate().unwrap_err();
        assert!(err.contains("Invalid key 'bad+key' (Bad)"));
    }

    #[test]
    fn test_validate_deeply_nested() {
        // Create a deeply nested mode structure
        let level1: Mode = ron::from_str(
            r#"[
            ("ctrl+1", "Level 1", mode([
                ("ctrl+2", "Level 2", mode([
                    ("ctrl+3", "Level 3", shell("level3")),
                    ("invalid", "Invalid", shell("invalid")),
                ])),
            ])),
        ]"#,
        )
        .unwrap();

        let err = level1.validate().unwrap_err();
        assert!(err.contains("Invalid key 'invalid' (Invalid)"));
    }

    #[test]
    fn test_attrs_deserialization() {
        // Test that attrs deserialize correctly
        let ron_text = r#"[
            ("a", "Action A", shell("echo a")),
            ("b", "Action B", shell("echo b"), (noexit: true)),
            ("c", "Action C", shell("echo c"), (noexit: false)),
        ]"#;

        let mode: Mode = ron::from_str(ron_text).unwrap();

        // Check action a has default attrs
        let (action_a, attrs_a) = mode.get_with_attrs("a").unwrap();
        assert!(matches!(action_a, Action::Shell(cmd) if cmd == "echo a"));
        assert!(!attrs_a.noexit);

        // Check action b has noexit: true
        let (action_b, attrs_b) = mode.get_with_attrs("b").unwrap();
        assert!(matches!(action_b, Action::Shell(cmd) if cmd == "echo b"));
        assert!(attrs_b.noexit);

        // Check action c has noexit: false
        let (action_c, attrs_c) = mode.get_with_attrs("c").unwrap();
        assert!(matches!(action_c, Action::Shell(cmd) if cmd == "echo c"));
        assert!(!attrs_c.noexit);
    }
}
