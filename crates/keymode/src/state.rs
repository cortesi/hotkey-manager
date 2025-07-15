use crate::mode::{Action, Attrs, Mode};
use crate::shell::execute_shell;
use hotkey_manager::Key;

/// Result of handling a key press
#[derive(Debug, Default)]
pub struct Handled {
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
        // First try to find key in current mode
        let current_mode = if let Some(mode) = self.mode_stack.last() {
            mode
        } else {
            &self.root
        };

        if let Some((action, attrs)) = current_mode.get_with_attrs(key) {
            let action = action.clone();
            let attrs = attrs.clone();
            return self.execute_action(&action, &attrs);
        }

        // If not found, check global keys from parent modes (in reverse order, from root up)
        // Check root first
        if let Some((action, attrs)) = self.root.get_with_attrs(key) {
            if attrs.global && !self.mode_stack.is_empty() {
                let action = action.clone();
                let attrs = attrs.clone();
                return self.execute_action(&action, &attrs);
            }
        }

        // Check each mode in the stack (excluding the last one which was already checked)
        let stack_len = self.mode_stack.len();
        if stack_len > 1 {
            for i in 0..stack_len - 1 {
                if let Some((action, attrs)) = self.mode_stack[i].get_with_attrs(key) {
                    if attrs.global {
                        let action = action.clone();
                        let attrs = attrs.clone();
                        return self.execute_action(&action, &attrs);
                    }
                }
            }
        }

        // Key not found
        Ok(Handled::new())
    }

    /// Execute an action with the given attributes
    fn execute_action(&mut self, action: &Action, attrs: &Attrs) -> Result<Handled, String> {
        match action {
            Action::Mode(new_mode) => {
                self.mode_stack.push(new_mode.clone());
                Ok(Handled::new())
            }
            Action::Pop => {
                self.mode_stack.pop();
                Ok(Handled::new())
            }
            Action::Exit => {
                self.reset();
                Ok(Handled::new())
            }
            Action::Shell(cmd) => {
                execute_shell(cmd);
                if !attrs.noexit {
                    self.reset();
                }
                Ok(Handled::new())
            }
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

    /// Get all keys from the current mode as (Key, String, Attrs) tuples
    /// This includes global keys from parent modes
    pub fn keys(&self) -> Vec<(Key, String, Attrs)> {
        let mut keys = Vec::new();
        let mut seen_keys = std::collections::HashSet::new();

        // Get all keys from current mode first (they take precedence)
        let current_mode = self.mode_stack.last().unwrap_or(&self.root);
        for (k, desc, attrs) in current_mode.keys_with_attrs() {
            seen_keys.insert(k.to_string());
            keys.push((k, desc, attrs));
        }

        // Add global keys from each mode in the stack (in reverse order, excluding current)
        let stack_len = self.mode_stack.len();
        if stack_len > 0 {
            for i in (0..stack_len - 1).rev() {
                for (k, desc, attrs) in self.mode_stack[i].keys_with_attrs() {
                    if attrs.global && !seen_keys.contains(&k.to_string()) {
                        seen_keys.insert(k.to_string());
                        keys.push((k, desc, attrs));
                    }
                }
            }
        }

        // Add global keys from root (unless we're already at root)
        if !self.mode_stack.is_empty() {
            for (k, desc, attrs) in self.root.keys_with_attrs() {
                if attrs.global && !seen_keys.contains(&k.to_string()) {
                    keys.push((k, desc, attrs));
                }
            }
        }

        keys
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
        state.handle_key(&key("q")).unwrap(); // Exit resets to root
        assert_eq!(state.depth(), 0); // Should reset to root after action
        state.handle_key(&key("h")).unwrap(); // Shell command
        assert_eq!(state.depth(), 0); // Should reset to root after action

        // Enter mode2
        state.handle_key(&key("2")).unwrap(); // Mode transition
        assert_eq!(state.depth(), 1);

        // Test mode2 - actions should reset to root
        state.handle_key(&key("s")).unwrap(); // Shell command
        assert_eq!(state.depth(), 0); // Should reset to root after action

        // Go back to mode2 to test Exit
        state.handle_key(&key("2")).unwrap();
        state.handle_key(&key("e")).unwrap(); // Exit resets to root
        assert_eq!(state.depth(), 0); // Should reset to root after action

        // Test pop from nested mode
        state.handle_key(&key("2")).unwrap();
        state.handle_key(&key("p")).unwrap(); // Pop
        assert_eq!(state.depth(), 0);

        // Test we're back in root
        state.handle_key(&key("q")).unwrap(); // Exit resets to root

        // Test pop from root does nothing
        state.handle_key(&key("p")).unwrap(); // Pop from root does nothing
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
        state.handle_key(&key("n")).unwrap(); // Mode transition
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

        // Unknown key does nothing
        state.handle_key(&key("z")).unwrap();
        state.handle_key(&key("x")).unwrap();
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
        state.handle_key(&key("m")).unwrap(); // Mode transition
        assert_eq!(state.depth(), 1);

        // Normal action should reset to root
        state.handle_key(&key("n")).unwrap(); // Shell command
        assert_eq!(state.depth(), 0); // Should be at root after normal action

        // Go back to menu
        state.handle_key(&key("m")).unwrap(); // Mode transition
        assert_eq!(state.depth(), 1);

        // Sticky action should NOT reset
        state.handle_key(&key("s")).unwrap(); // Shell command
        assert_eq!(state.depth(), 1); // Should still be in menu

        // Go deeper
        state.handle_key(&key("d")).unwrap(); // Mode transition
        assert_eq!(state.depth(), 2);

        // Normal action in deep menu should reset to root
        state.handle_key(&key("x")).unwrap(); // Shell command
        assert_eq!(state.depth(), 0); // Should be back at root

        // Test sticky in deep menu
        state.handle_key(&key("m")).unwrap(); // Enter menu
        state.handle_key(&key("d")).unwrap(); // Enter deep
        assert_eq!(state.depth(), 2);

        // Sticky action in deep menu should NOT reset
        state.handle_key(&key("y")).unwrap(); // Shell command
        assert_eq!(state.depth(), 2); // Should still be in deep menu
    }

    #[test]
    fn test_global_keys_behavior() {
        // Create modes with global keys
        let ron_text = r#"[
            ("g", "Global from root", shell("echo global root"), (global: true)),
            ("r", "Regular root", shell("echo regular root")),
            ("m", "Menu", mode([
                ("a", "Action A", shell("echo action a")),
                ("h", "Global from menu", shell("echo global menu"), (global: true)),
                ("s", "Submenu", mode([
                    ("x", "Action X", shell("echo action x")),
                    ("p", "Pop", pop),
                ])),
                ("t", "Another submenu", mode([
                    ("y", "Action Y", shell("echo action y")),
                ])),
            ])),
            ("n", "Another menu", mode([
                ("z", "Action Z", shell("echo action z")),
            ])),
        ]"#;

        let root: Mode = ron::from_str(ron_text).unwrap();
        let mut state = State::new(root);

        // Test that root global key is available in nested modes
        state.handle_key(&key("m")).unwrap(); // Enter menu
        assert_eq!(state.depth(), 1);

        // Global key from root should work
        state.handle_key(&key("g")).unwrap();
        assert_eq!(state.depth(), 0); // Should reset after shell command

        // Regular root key should NOT work in nested mode
        state.handle_key(&key("m")).unwrap(); // Enter menu again
        state.handle_key(&key("r")).unwrap(); // Should do nothing
        assert_eq!(state.depth(), 1); // Still in menu

        // Test global key from menu is available in submenu
        state.handle_key(&key("s")).unwrap(); // Enter submenu
        assert_eq!(state.depth(), 2);

        // Global key from menu should work in submenu
        state.handle_key(&key("h")).unwrap();
        assert_eq!(state.depth(), 0); // Should reset after shell command

        // Test that global keys don't affect siblings
        state.handle_key(&key("n")).unwrap(); // Enter another menu
        assert_eq!(state.depth(), 1);

        // Global key from the other menu tree should NOT work here
        state.handle_key(&key("h")).unwrap(); // Should do nothing
        assert_eq!(state.depth(), 1); // Still in menu n

        // But root global should still work
        state.handle_key(&key("g")).unwrap();
        assert_eq!(state.depth(), 0); // Should reset after shell command
    }

    #[test]
    fn test_global_keys_in_keys_list() {
        // Create modes with global keys
        let ron_text = r#"[
            ("g", "Global from root", shell("echo global root"), (global: true)),
            ("r", "Regular root", shell("echo regular root")),
            ("m", "Menu", mode([
                ("a", "Action A", shell("echo action a")),
                ("h", "Global from menu", shell("echo global menu"), (global: true)),
                ("s", "Submenu", mode([
                    ("x", "Action X", shell("echo action x")),
                ])),
            ])),
        ]"#;

        let root: Mode = ron::from_str(ron_text).unwrap();
        let mut state = State::new(root);

        // Check keys in root mode
        let root_keys = state.keys();
        assert_eq!(root_keys.len(), 3); // g, r, m

        // Enter menu
        state.handle_key(&key("m")).unwrap();
        let menu_keys = state.keys();
        // Should have: g (global from root), a, h, s
        assert_eq!(menu_keys.len(), 4);
        assert!(menu_keys.iter().any(|(k, _, _)| k.to_string() == "g")); // Global from root
        assert!(menu_keys.iter().any(|(k, _, _)| k.to_string() == "a"));
        assert!(menu_keys.iter().any(|(k, _, _)| k.to_string() == "h"));
        assert!(menu_keys.iter().any(|(k, _, _)| k.to_string() == "s"));

        // Enter submenu
        state.handle_key(&key("s")).unwrap();
        let submenu_keys = state.keys();
        // Should have: g (global from root), h (global from menu), x
        assert_eq!(submenu_keys.len(), 3);
        assert!(submenu_keys.iter().any(|(k, _, _)| k.to_string() == "g")); // Global from root
        assert!(submenu_keys.iter().any(|(k, _, _)| k.to_string() == "h")); // Global from menu
        assert!(submenu_keys.iter().any(|(k, _, _)| k.to_string() == "x")); // Regular key
    }

    #[test]
    fn test_global_keys_with_noexit() {
        // Test that global keys respect their noexit attribute
        let ron_text = r#"[
            ("g", "Global sticky", shell("echo global"), (global: true, noexit: true)),
            ("m", "Menu", mode([
                ("a", "Action A", shell("echo action a")),
                ("s", "Submenu", mode([
                    ("x", "Action X", shell("echo action x")),
                ])),
            ])),
        ]"#;

        let root: Mode = ron::from_str(ron_text).unwrap();
        let mut state = State::new(root);

        // Enter submenu
        state.handle_key(&key("m")).unwrap();
        state.handle_key(&key("s")).unwrap();
        assert_eq!(state.depth(), 2);

        // Use global key with noexit
        state.handle_key(&key("g")).unwrap();
        assert_eq!(state.depth(), 2); // Should NOT reset due to noexit
    }

    #[test]
    fn test_hide_attribute() {
        // Create modes with hidden keys
        let ron_text = r#"[
            ("v", "Visible key", shell("echo visible")),
            ("h", "Hidden key", shell("echo hidden"), (hide: true)),
            ("g", "Global hidden", shell("echo global hidden"), (global: true, hide: true)),
            ("m", "Menu", mode([
                ("a", "Action A", shell("echo action a")),
                ("s", "Secret action", shell("echo secret"), (hide: true)),
            ])),
        ]"#;

        let root: Mode = ron::from_str(ron_text).unwrap();
        let mut state = State::new(root);

        // Test that hidden keys still work
        state.handle_key(&key("h")).unwrap();
        assert_eq!(state.depth(), 0); // Should reset after shell command

        // Check keys in root mode
        let root_keys = state.keys();
        // All keys should be present (including hidden ones)
        assert_eq!(root_keys.len(), 4); // v, h, g, m

        // Verify we can filter hidden keys
        let visible_keys: Vec<_> = root_keys
            .iter()
            .filter(|(_, _, attrs)| !attrs.hide)
            .collect();
        assert_eq!(visible_keys.len(), 2); // Only v and m should be visible
        assert!(visible_keys.iter().any(|(k, _, _)| k.to_string() == "v"));
        assert!(visible_keys.iter().any(|(k, _, _)| k.to_string() == "m"));

        // Enter menu and test hidden key there
        state.handle_key(&key("m")).unwrap();
        assert_eq!(state.depth(), 1);

        // Hidden key should still work
        state.handle_key(&key("s")).unwrap();
        assert_eq!(state.depth(), 0); // Should reset after shell command

        // Test global hidden key in menu
        state.handle_key(&key("m")).unwrap();
        state.handle_key(&key("g")).unwrap(); // Global hidden key should work
        assert_eq!(state.depth(), 0);
    }
}
