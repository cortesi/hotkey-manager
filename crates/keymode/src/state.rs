use crate::mode::{Action, Mode};
use crate::shell::execute_shell;
use hotkey_manager::Key;

/// Result of handling a key press
#[derive(Debug, Default)]
pub struct Handled {
    /// Whether the exit action was triggered
    pub exit: bool,
    /// Message to display to the user
    pub user: String,
    /// Warning message
    pub warn: String,
}

impl Handled {
    /// Create a new Handled with default values
    fn new() -> Self {
        Self::default()
    }

    /// Create a Handled that signals exit
    fn exit() -> Self {
        Self {
            exit: true,
            ..Default::default()
        }
    }
}

/// Manages a stack of modes for hierarchical key binding navigation
#[derive(Debug)]
pub struct State {
    root: Mode,
    mode_stack: Vec<Mode>,
}

impl State {
    /// Create a new state with the given root mode
    pub fn new(root: Mode) -> Self {
        Self {
            root,
            mode_stack: Vec::new(),
        }
    }

    /// Process a key press and handle the action internally
    /// Returns a Result containing information about the handled action
    pub fn handle_key(&mut self, key: &Key) -> Result<Handled, String> {
        // Get the current mode (from stack or root)
        let current_mode = if let Some(mode) = self.mode_stack.last() {
            mode
        } else {
            &self.root
        };

        // Look up the action and attrs for this key
        if let Some((action, attrs)) = current_mode.get_with_attrs(key) {
            match action {
                Action::Mode(new_mode) => {
                    self.mode_stack.push(new_mode.clone());
                    Ok(Handled::new())
                }
                Action::Pop => {
                    if self.mode_stack.is_empty() {
                        Ok(Handled::exit())
                    } else {
                        self.mode_stack.pop();
                        Ok(Handled::new())
                    }
                }
                Action::Exit => {
                    if !attrs.noexit {
                        self.reset();
                    }
                    Ok(Handled::exit())
                }
                Action::Shell(cmd) => {
                    execute_shell(cmd);
                    if !attrs.noexit {
                        self.reset();
                    }
                    Ok(Handled::new())
                }
            }
        } else {
            // Key not found
            Ok(Handled::new())
        }
    }

    /// Reset to the root mode
    pub fn reset(&mut self) {
        self.mode_stack.clear();
    }

    /// Get the current mode depth (0 = root)
    pub fn depth(&self) -> usize {
        self.mode_stack.len()
    }

    /// Get a reference to the current mode
    pub fn mode(&self) -> &Mode {
        self.mode_stack.last().unwrap_or(&self.root)
    }

    /// Get all keys from the current mode as Key objects
    pub fn keys(&self) -> Vec<Key> {
        self.mode().key_objects().cloned().collect()
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
    fn test_state_navigation() {
        let root: Mode = ron::from_str(
            r#"[
            ("q", "Exit", exit),
            ("2", "Mode 2", mode([
                ("p", "Back", pop),
                ("s", "Shell", shell("ls")),
                ("e", "Exit", exit),
            ])),
            ("h", "Hello", shell("echo hello")),
            ("p", "Pop", pop),
        ]"#,
        )
        .unwrap();

        let mut state = State::new(root);

        // Test root mode
        assert_eq!(state.depth(), 0);
        assert!(state.handle_key(&key("q")).unwrap().exit); // Exit returns true
        assert_eq!(state.depth(), 0); // Should reset to root after action
        assert!(!state.handle_key(&key("h")).unwrap().exit); // Shell command returns false
        assert_eq!(state.depth(), 0); // Should reset to root after action

        // Enter mode2
        assert!(!state.handle_key(&key("2")).unwrap().exit); // Mode transition returns false
        assert_eq!(state.depth(), 1);

        // Test mode2 - actions should reset to root
        assert!(!state.handle_key(&key("s")).unwrap().exit); // Shell command returns false
        assert_eq!(state.depth(), 0); // Should reset to root after action

        // Go back to mode2 to test Exit
        state.handle_key(&key("2")).unwrap();
        assert!(state.handle_key(&key("e")).unwrap().exit); // Exit returns true
        assert_eq!(state.depth(), 0); // Should reset to root after action

        // Test pop from nested mode
        state.handle_key(&key("2")).unwrap();
        assert!(!state.handle_key(&key("p")).unwrap().exit); // Pop returns false
        assert_eq!(state.depth(), 0);

        // Test we're back in root
        assert!(state.handle_key(&key("q")).unwrap().exit); // Exit returns true

        // Test pop from root returns Exit
        assert!(state.handle_key(&key("p")).unwrap().exit); // Pop from root returns true (exit)
    }

    #[test]
    fn test_state_reset() {
        let root: Mode = ron::from_str(
            r#"[
            ("n", "Nested", mode([
                ("x", "Exit", exit),
            ])),
        ]"#,
        )
        .unwrap();

        let mut state = State::new(root);

        // Go into nested mode
        assert!(!state.handle_key(&key("n")).unwrap().exit); // Mode transition returns false
        assert_eq!(state.depth(), 1);

        // Reset
        state.reset();
        assert_eq!(state.depth(), 0);
    }

    #[test]
    fn test_unknown_keys() {
        let root: Mode = ron::from_str(
            r#"[
            ("a", "Action", shell("test")),
        ]"#,
        )
        .unwrap();

        let mut state = State::new(root);

        // Unknown key returns false (no exit)
        assert!(!state.handle_key(&key("z")).unwrap().exit);
        assert!(!state.handle_key(&key("x")).unwrap().exit);
    }

    #[test]
    fn test_noexit_behavior() {
        // Create modes with noexit actions
        let ron_text = r#"[
            ("m", "Menu", mode([
                ("n", "Normal", shell("echo normal")),
                ("s", "Sticky", shell("echo sticky"), (noexit: true)),
                ("d", "Deep", mode([
                    ("x", "Execute", shell("echo deep")),
                    ("y", "Sticky Deep", shell("echo sticky deep"), (noexit: true)),
                ])),
            ])),
        ]"#;

        let root: Mode = ron::from_str(ron_text).unwrap();
        let mut state = State::new(root);

        // Enter menu
        assert!(!state.handle_key(&key("m")).unwrap().exit); // Mode transition returns false
        assert_eq!(state.depth(), 1);

        // Normal action should reset to root
        assert!(!state.handle_key(&key("n")).unwrap().exit); // Shell command returns false
        assert_eq!(state.depth(), 0); // Should be at root after normal action

        // Go back to menu
        assert!(!state.handle_key(&key("m")).unwrap().exit); // Mode transition returns false
        assert_eq!(state.depth(), 1);

        // Sticky action should NOT reset
        assert!(!state.handle_key(&key("s")).unwrap().exit); // Shell command returns false
        assert_eq!(state.depth(), 1); // Should still be in menu

        // Go deeper
        assert!(!state.handle_key(&key("d")).unwrap().exit); // Mode transition returns false
        assert_eq!(state.depth(), 2);

        // Normal action in deep menu should reset to root
        assert!(!state.handle_key(&key("x")).unwrap().exit); // Shell command returns false
        assert_eq!(state.depth(), 0); // Should be back at root

        // Test sticky in deep menu
        state.handle_key(&key("m")).unwrap(); // Enter menu
        state.handle_key(&key("d")).unwrap(); // Enter deep
        assert_eq!(state.depth(), 2);

        // Sticky action in deep menu should NOT reset
        assert!(!state.handle_key(&key("y")).unwrap().exit); // Shell command returns false
        assert_eq!(state.depth(), 2); // Should still be in deep menu
    }
}
