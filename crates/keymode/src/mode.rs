use hotkey_manager::Key;
use serde::{Deserialize, Serialize};

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
    keys: Vec<(Key, String, Action, Attrs)>,
}

// Manual Serialize implementation that respects transparent
impl Serialize for Mode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.keys.len()))?;
        for (key, desc, action, attrs) in &self.keys {
            // Serialize as a tuple with key converted to string
            if attrs == &Attrs::default() {
                seq.serialize_element(&(key.to_string(), desc, action))?;
            } else {
                seq.serialize_element(&(key.to_string(), desc, action, attrs))?;
            }
        }
        seq.end()
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
        let mut keys = Vec::new();

        for entry in entries {
            match entry {
                Entry::Simple(k, n, a) => match Key::parse(&k) {
                    Ok(key) => keys.push((key, n, a, Attrs::default())),
                    Err(e) => {
                        return Err(serde::de::Error::custom(format!("Invalid key '{k}': {e}")));
                    }
                },
                Entry::WithAttrs(k, n, a, attrs) => match Key::parse(&k) {
                    Ok(key) => keys.push((key, n, a, attrs)),
                    Err(e) => {
                        return Err(serde::de::Error::custom(format!("Invalid key '{k}': {e}")));
                    }
                },
            }
        }

        Ok(Mode { keys })
    }
}

impl Mode {
    /// Create a Mode from a RON string
    pub fn from_ron(ron_str: &str) -> Result<Self, String> {
        ron::from_str(ron_str).map_err(|e| format!("Failed to parse RON: {e}"))
    }

    /// Get the action and attributes associated with a key
    pub fn get_with_attrs(&self, key: &Key) -> Option<(&Action, &Attrs)> {
        self.keys
            .iter()
            .find(|(k, _, _, _)| k == key)
            .map(|(_, _, action, attrs)| (action, attrs))
    }

    /// Get all keys in this mode
    ///
    /// Returns an iterator over tuples of (key_string, description)
    pub fn keys(&self) -> impl Iterator<Item = (String, &str)> + '_ {
        self.keys
            .iter()
            .map(|(k, desc, _, _)| (k.to_string(), desc.as_str()))
    }

    /// Get all Key objects in this mode
    pub fn key_objects(&self) -> impl Iterator<Item = &Key> + '_ {
        self.keys.iter().map(|(k, _, _, _)| k)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper function to create a Key from a string for tests
    fn key(s: &str) -> Key {
        Key::parse(s).unwrap()
    }

    #[test]
    fn test_mode() {
        let mode = Mode::from_ron(
            r#"[
            ("q", "Exit", exit),
            ("s", "Shell", shell("echo hello")),
        ]"#,
        )
        .unwrap();

        assert!(matches!(
            mode.get_with_attrs(&key("q")),
            Some((Action::Exit, _))
        ));
        assert!(
            matches!(mode.get_with_attrs(&key("s")), Some((Action::Shell(cmd), _)) if cmd == "echo hello")
        );
        assert_eq!(mode.get_with_attrs(&key("x")), None);
    }

    #[test]
    fn test_from_ron() {
        let ron_str = r#"[
            ("q", "Exit", exit),
            ("s", "Shell", shell("echo hello")),
            ("m", "Submenu", mode([
                ("x", "Exit submenu", pop),
            ])),
        ]"#;

        let mode = Mode::from_ron(ron_str).unwrap();

        assert!(matches!(
            mode.get_with_attrs(&key("q")),
            Some((Action::Exit, _))
        ));
        assert!(
            matches!(mode.get_with_attrs(&key("s")), Some((Action::Shell(cmd), _)) if cmd == "echo hello")
        );

        // Test nested mode
        if let Some((Action::Mode(nested), _)) = mode.get_with_attrs(&key("m")) {
            assert!(matches!(
                nested.get_with_attrs(&key("x")),
                Some((Action::Pop, _))
            ));
        } else {
            panic!("Expected nested mode");
        }
    }

    #[test]
    fn test_from_ron_error() {
        let invalid_ron = r#"[
            ("q", "Exit", invalid_action),
        ]"#;

        let result = Mode::from_ron(invalid_ron);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to parse RON"));
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

        let main_mode = Mode::from_ron(ron_text).unwrap();

        assert!(matches!(
            main_mode.get_with_attrs(&key("q")),
            Some((Action::Exit, _))
        ));

        if let Some((Action::Mode(nested), _)) = main_mode.get_with_attrs(&key("m")) {
            assert!(
                matches!(nested.get_with_attrs(&key("x")), Some((Action::Shell(cmd), _)) if cmd == "exit")
            );
        } else {
            panic!("Expected nested mode");
        }
    }

    #[test]
    fn test_mode_serialization() {
        let mode = Mode::from_ron(
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
        let deserialized = Mode::from_ron(&ron_string).unwrap();

        // Verify they are equal
        assert_eq!(mode, deserialized);
        assert!(matches!(
            deserialized.get_with_attrs(&key("q")),
            Some((Action::Exit, _))
        ));
        assert!(
            matches!(deserialized.get_with_attrs(&key("s")), Some((Action::Shell(cmd), _)) if cmd == "echo hello")
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
                    key("m"),
                    "Message".to_string(),
                    Action::shell("git commit -m 'Quick commit'"),
                    Attrs::default(),
                ),
                (
                    key("a"),
                    "Amend".to_string(),
                    Action::shell("git commit --amend"),
                    Attrs::default(),
                ),
                (key("p"), "Back".to_string(), Action::Pop, Attrs::default()),
            ],
        };

        let git_mode = Mode {
            keys: vec![
                (
                    key("s"),
                    "Status".to_string(),
                    Action::shell("git status"),
                    Attrs::default(),
                ),
                (
                    key("l"),
                    "Log".to_string(),
                    Action::shell("git log"),
                    Attrs { noexit: true },
                ),
                (
                    key("p"),
                    "Pull".to_string(),
                    Action::shell("git pull"),
                    Attrs::default(),
                ),
                (
                    key("c"),
                    "Commit".to_string(),
                    Action::Mode(commit_mode),
                    Attrs::default(),
                ),
                (key("q"), "Back".to_string(), Action::Pop, Attrs::default()),
            ],
        };

        let files_mode = Mode {
            keys: vec![
                (
                    key("l"),
                    "List".to_string(),
                    Action::shell("ls -la"),
                    Attrs::default(),
                ),
                (
                    key("t"),
                    "Tree".to_string(),
                    Action::shell("tree"),
                    Attrs { noexit: true },
                ),
                (key("q"), "Back".to_string(), Action::Pop, Attrs::default()),
            ],
        };

        let expected = Mode {
            keys: vec![
                (key("q"), "Exit".to_string(), Action::Exit, Attrs::default()),
                (
                    key("h"),
                    "Hello".to_string(),
                    Action::shell("echo 'Hello World'"),
                    Attrs::default(),
                ),
                (
                    key("g"),
                    "Git".to_string(),
                    Action::Mode(git_mode),
                    Attrs::default(),
                ),
                (
                    key("f"),
                    "Files".to_string(),
                    Action::Mode(files_mode),
                    Attrs::default(),
                ),
            ],
        };

        // Deserialize from RON text
        let deserialized = Mode::from_ron(ron_text).unwrap();

        // Compare the structures
        assert_eq!(deserialized, expected);
    }

    #[test]
    fn test_attrs_deserialization() {
        // Test that attrs deserialize correctly
        let ron_text = r#"[
            ("a", "Action A", shell("echo a")),
            ("b", "Action B", shell("echo b"), (noexit: true)),
            ("c", "Action C", shell("echo c"), (noexit: false)),
        ]"#;

        let mode = Mode::from_ron(ron_text).unwrap();

        // Check action a has default attrs
        let (action_a, attrs_a) = mode.get_with_attrs(&key("a")).unwrap();
        assert!(matches!(action_a, Action::Shell(cmd) if cmd == "echo a"));
        assert!(!attrs_a.noexit);

        // Check action b has noexit: true
        let (action_b, attrs_b) = mode.get_with_attrs(&key("b")).unwrap();
        assert!(matches!(action_b, Action::Shell(cmd) if cmd == "echo b"));
        assert!(attrs_b.noexit);

        // Check action c has noexit: false
        let (action_c, attrs_c) = mode.get_with_attrs(&key("c")).unwrap();
        assert!(matches!(action_c, Action::Shell(cmd) if cmd == "echo c"));
        assert!(!attrs_c.noexit);
    }
}
