use crate::mode::{Action, Mode};

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

    /// Process a key press and return the resulting action if any
    pub fn key(&mut self, key: &str) -> Option<Action> {
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
                    // Push the new mode onto the stack
                    self.mode_stack.push(new_mode.clone());
                    None
                }
                Action::Pop => {
                    if self.mode_stack.is_empty() {
                        // Popping from root - return Exit
                        Some(Action::Exit)
                    } else {
                        // Pop the current mode
                        self.mode_stack.pop();
                        None
                    }
                }
                // All other actions - reset if noexit is false
                _ => {
                    let result = action.clone();
                    if !attrs.noexit {
                        self.reset();
                    }
                    Some(result)
                }
            }
        } else {
            // Key not found
            None
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Action;

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
        assert!(matches!(state.key("q"), Some(Action::Exit)));
        assert_eq!(state.depth(), 0); // Should reset to root after action
        assert!(matches!(state.key("h"), Some(Action::Shell(cmd)) if cmd == "echo hello"));
        assert_eq!(state.depth(), 0); // Should reset to root after action

        // Enter mode2
        assert!(state.key("2").is_none());
        assert_eq!(state.depth(), 1);

        // Test mode2 - actions should reset to root
        assert!(matches!(state.key("s"), Some(Action::Shell(cmd)) if cmd == "ls"));
        assert_eq!(state.depth(), 0); // Should reset to root after action

        // Go back to mode2 to test Exit
        state.key("2");
        assert!(matches!(state.key("e"), Some(Action::Exit)));
        assert_eq!(state.depth(), 0); // Should reset to root after action

        // Test pop from nested mode
        state.key("2");
        assert!(state.key("p").is_none());
        assert_eq!(state.depth(), 0);

        // Test we're back in root
        assert!(matches!(state.key("q"), Some(Action::Exit)));

        // Test pop from root returns Exit
        assert!(matches!(state.key("p"), Some(Action::Exit)));
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
        state.key("n");
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

        // Unknown key returns None
        assert!(state.key("z").is_none());
        assert!(state.key("unknown").is_none());
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
        assert!(state.key("m").is_none());
        assert_eq!(state.depth(), 1);

        // Normal action should reset to root
        assert!(matches!(state.key("n"), Some(Action::Shell(cmd)) if cmd == "echo normal"));
        assert_eq!(state.depth(), 0); // Should be at root after normal action

        // Go back to menu
        assert!(state.key("m").is_none());
        assert_eq!(state.depth(), 1);

        // Sticky action should NOT reset
        assert!(matches!(state.key("s"), Some(Action::Shell(cmd)) if cmd == "echo sticky"));
        assert_eq!(state.depth(), 1); // Should still be in menu

        // Go deeper
        assert!(state.key("d").is_none());
        assert_eq!(state.depth(), 2);

        // Normal action in deep menu should reset to root
        assert!(matches!(state.key("x"), Some(Action::Shell(cmd)) if cmd == "echo deep"));
        assert_eq!(state.depth(), 0); // Should be back at root

        // Test sticky in deep menu
        state.key("m"); // Enter menu
        state.key("d"); // Enter deep
        assert_eq!(state.depth(), 2);

        // Sticky action in deep menu should NOT reset
        assert!(matches!(state.key("y"), Some(Action::Shell(cmd)) if cmd == "echo sticky deep"));
        assert_eq!(state.depth(), 2); // Should still be in deep menu
    }
}
