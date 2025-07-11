use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Shell(String),
    Mode(Mode),
    Pop,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mode {
    name: String,
    keys: HashMap<String, Action>,
}

impl Mode {
    pub fn new(name: String) -> Self {
        Self {
            name,
            keys: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: String, action: Action) {
        self.keys.insert(key, action);
    }

    pub fn get(&self, key: &str) -> Option<&Action> {
        self.keys.get(key)
    }

    pub fn name(&self) -> Option<&str> {
        if self.name.is_empty() {
            None
        } else {
            Some(&self.name)
        }
    }
}

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
    pub fn new() -> Self {
        Self {
            mode_stack: Vec::new(),
        }
    }

    pub fn push_mode(&mut self, mode: Mode) {
        self.mode_stack.push(mode);
    }

    pub fn pop_mode(&mut self) -> Option<Mode> {
        self.mode_stack.pop()
    }

    pub fn current_mode(&self) -> Option<&Mode> {
        self.mode_stack.last()
    }

    pub fn handle_key(&self, key: &str) -> Option<&Action> {
        self.current_mode().and_then(|mode| mode.get(key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode() {
        let mut mode = Mode::new("test".to_string());
        mode.insert("q".to_string(), Action::Quit);
        mode.insert("s".to_string(), Action::Shell("echo hello".to_string()));

        assert!(matches!(mode.get("q"), Some(Action::Quit)));
        assert!(matches!(mode.get("s"), Some(Action::Shell(cmd)) if cmd == "echo hello"));
        assert_eq!(mode.get("x"), None);
    }

    #[test]
    fn test_nested_modes() {
        let mut submenu = Mode::new("submenu".to_string());
        submenu.insert("x".to_string(), Action::Shell("exit".to_string()));
        submenu.insert("p".to_string(), Action::Pop);

        let mut main_mode = Mode::new("main".to_string());
        main_mode.insert("q".to_string(), Action::Quit);
        main_mode.insert("m".to_string(), Action::Mode(submenu));

        assert!(matches!(main_mode.get("q"), Some(Action::Quit)));

        if let Some(Action::Mode(nested)) = main_mode.get("m") {
            assert_eq!(nested.name(), Some("submenu"));
            assert!(matches!(nested.get("x"), Some(Action::Shell(cmd)) if cmd == "exit"));
        } else {
            panic!("Expected nested mode");
        }
    }

    #[test]
    fn test_keymap_manager() {
        let mut manager = KeyMapState::new();

        let mut mode1 = Mode::new("mode1".to_string());
        mode1.insert("q".to_string(), Action::Quit);

        let mut mode2 = Mode::new("mode2".to_string());
        mode2.insert("p".to_string(), Action::Pop);
        mode2.insert("s".to_string(), Action::Shell("ls".to_string()));

        mode1.insert("2".to_string(), Action::Mode(mode2.clone()));

        manager.push_mode(mode1);
        assert!(matches!(manager.handle_key("q"), Some(Action::Quit)));

        if let Some(Action::Mode(next_mode)) = manager.handle_key("2") {
            manager.push_mode(next_mode.clone());
        }

        assert!(matches!(manager.handle_key("p"), Some(Action::Pop)));
        assert!(matches!(manager.handle_key("s"), Some(Action::Shell(cmd)) if cmd == "ls"));

        manager.pop_mode();
        assert!(matches!(manager.handle_key("q"), Some(Action::Quit)));
    }

    #[test]
    fn test_mode_serialization() {
        let mut mode = Mode::new("test_mode".to_string());
        mode.insert("q".to_string(), Action::Quit);
        mode.insert("s".to_string(), Action::Shell("echo hello".to_string()));
        mode.insert("p".to_string(), Action::Pop);

        // Create a nested mode
        let mut nested_mode = Mode::new("nested".to_string());
        nested_mode.insert("x".to_string(), Action::Shell("exit".to_string()));
        mode.insert("n".to_string(), Action::Mode(nested_mode));

        // Serialize to RON
        let ron_string = ron::to_string(&mode).unwrap();

        // Deserialize from RON
        let deserialized: Mode = ron::from_str(&ron_string).unwrap();

        // Verify they are equal
        assert_eq!(mode, deserialized);
        assert_eq!(deserialized.name(), Some("test_mode"));
        assert!(matches!(deserialized.get("q"), Some(Action::Quit)));
        assert!(matches!(deserialized.get("s"), Some(Action::Shell(cmd)) if cmd == "echo hello"));
    }

    #[test]
    fn test_mode_deserialization_from_ron_text() {
        // RON text definition of nested modes
        let ron_text = r#"(
            name: "main",
            keys: {
                "q": Quit,
                "h": Shell("echo 'Hello World'"),
                "g": Mode((
                    name: "git",
                    keys: {
                        "s": Shell("git status"),
                        "p": Shell("git pull"),
                        "c": Mode((
                            name: "commit",
                            keys: {
                                "m": Shell("git commit -m 'Quick commit'"),
                                "a": Shell("git commit --amend"),
                                "p": Pop,
                            }
                        )),
                        "q": Pop,
                    }
                )),
                "f": Mode((
                    name: "files",
                    keys: {
                        "l": Shell("ls -la"),
                        "t": Shell("tree"),
                        "q": Pop,
                    }
                )),
            }
        )"#;

        // Deserialize from RON text
        let mode: Mode = ron::from_str(ron_text).unwrap();

        // Verify the structure
        assert_eq!(mode.name(), Some("main"));
        assert!(matches!(mode.get("q"), Some(Action::Quit)));
        assert!(matches!(mode.get("h"), Some(Action::Shell(cmd)) if cmd == "echo 'Hello World'"));

        // Check git submenu
        if let Some(Action::Mode(git_mode)) = mode.get("g") {
            assert_eq!(git_mode.name(), Some("git"));
            assert!(matches!(git_mode.get("s"), Some(Action::Shell(cmd)) if cmd == "git status"));
            assert!(matches!(git_mode.get("p"), Some(Action::Shell(cmd)) if cmd == "git pull"));
            assert!(matches!(git_mode.get("q"), Some(Action::Pop)));

            // Check nested commit submenu
            if let Some(Action::Mode(commit_mode)) = git_mode.get("c") {
                assert_eq!(commit_mode.name(), Some("commit"));
                assert!(
                    matches!(commit_mode.get("m"), Some(Action::Shell(cmd)) if cmd == "git commit -m 'Quick commit'")
                );
                assert!(
                    matches!(commit_mode.get("a"), Some(Action::Shell(cmd)) if cmd == "git commit --amend")
                );
                assert!(matches!(commit_mode.get("p"), Some(Action::Pop)));
            } else {
                panic!("Expected commit submenu");
            }
        } else {
            panic!("Expected git submenu");
        }

        // Check files submenu
        if let Some(Action::Mode(files_mode)) = mode.get("f") {
            assert_eq!(files_mode.name(), Some("files"));
            assert!(matches!(files_mode.get("l"), Some(Action::Shell(cmd)) if cmd == "ls -la"));
            assert!(matches!(files_mode.get("t"), Some(Action::Shell(cmd)) if cmd == "tree"));
            assert!(matches!(files_mode.get("q"), Some(Action::Pop)));
        } else {
            panic!("Expected files submenu");
        }
    }

    #[test]
    fn test_mode_with_empty_name() {
        // Test creating a mode with an empty name
        let mut mode = Mode::new("".to_string());
        mode.insert("q".to_string(), Action::Quit);
        mode.insert("s".to_string(), Action::Shell("ls".to_string()));

        // Verify name returns None for empty string
        assert_eq!(mode.name(), None);

        // Test serialization/deserialization with empty name
        let ron_string = ron::to_string(&mode).unwrap();
        let deserialized: Mode = ron::from_str(&ron_string).unwrap();
        assert_eq!(deserialized.name(), None);
        assert!(matches!(deserialized.get("q"), Some(Action::Quit)));

        // Test RON text with empty name
        let ron_text = r#"(
            name: "",
            keys: {
                "a": Shell("echo anonymous"),
                "m": Mode((
                    name: "",
                    keys: {
                        "x": Pop,
                    }
                )),
            }
        )"#;

        let mode: Mode = ron::from_str(ron_text).unwrap();
        assert_eq!(mode.name(), None);
        assert!(matches!(mode.get("a"), Some(Action::Shell(cmd)) if cmd == "echo anonymous"));

        if let Some(Action::Mode(nested)) = mode.get("m") {
            assert_eq!(nested.name(), None);
            assert!(matches!(nested.get("x"), Some(Action::Pop)));
        } else {
            panic!("Expected nested mode");
        }
    }
}
