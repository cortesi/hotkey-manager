use crate::error::{Error, Result};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A unified key definition that can be parsed, serialized, and converted to HotKey
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modifiers: Option<Modifiers>,
    pub code: Code,
}

impl Key {
    /// Create a new Key with the given code and optional modifiers
    pub fn new(code: Code, modifiers: Option<Modifiers>) -> Self {
        Key { code, modifiers }
    }

    /// Parse a key from a string representation
    ///
    /// Supports formats like:
    /// - "a" (just a key)
    /// - "ctrl+a" (with modifiers)
    /// - "cmd+shift+a" (multiple modifiers)
    /// - "control+alt+delete" (alternative names)
    pub fn parse(s: &str) -> Result<Self> {
        // Split by '+' to separate modifiers and key
        let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();

        if parts.is_empty() {
            return Err(Error::InvalidKey("Empty key string".to_string()));
        }

        // The last part should be the key code
        // SAFETY: unwrap is safe here because we checked parts.is_empty() above
        let key_part = parts.last().unwrap();
        let modifier_parts = &parts[..parts.len() - 1];

        // Parse the key code
        let code = parse_code(key_part)?;

        // Parse modifiers
        let modifiers = if modifier_parts.is_empty() {
            None
        } else {
            let mut mods = Modifiers::empty();
            for part in modifier_parts {
                match part.to_lowercase().as_str() {
                    "ctrl" | "control" => mods |= Modifiers::CONTROL,
                    "alt" | "option" => mods |= Modifiers::ALT,
                    "shift" => mods |= Modifiers::SHIFT,
                    "cmd" | "command" | "super" | "win" | "windows" | "meta" => {
                        mods |= Modifiers::SUPER
                    }
                    _ => return Err(Error::InvalidKey(format!("Unknown modifier: {part}"))),
                }
            }
            Some(mods)
        };

        Ok(Key { code, modifiers })
    }

    /// Convert this Key to a global_hotkey HotKey
    pub fn to_hotkey(&self) -> HotKey {
        HotKey::new(self.modifiers, self.code)
    }
}

impl From<Key> for HotKey {
    fn from(key: Key) -> Self {
        key.to_hotkey()
    }
}

impl From<&Key> for HotKey {
    fn from(key: &Key) -> Self {
        key.to_hotkey()
    }
}

impl From<HotKey> for Key {
    fn from(hotkey: HotKey) -> Self {
        Key {
            modifiers: if hotkey.mods.is_empty() {
                None
            } else {
                Some(hotkey.mods)
            },
            code: hotkey.key,
        }
    }
}

impl From<&HotKey> for Key {
    fn from(hotkey: &HotKey) -> Self {
        Key {
            modifiers: if hotkey.mods.is_empty() {
                None
            } else {
                Some(hotkey.mods)
            },
            code: hotkey.key,
        }
    }
}

impl TryFrom<&str> for Key {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self> {
        Key::parse(s)
    }
}

impl TryFrom<String> for Key {
    type Error = Error;

    fn try_from(s: String) -> Result<Self> {
        Key::parse(&s)
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();

        if let Some(mods) = self.modifiers {
            if mods.contains(Modifiers::CONTROL) {
                parts.push("ctrl");
            }
            if mods.contains(Modifiers::ALT) {
                parts.push("alt");
            }
            if mods.contains(Modifiers::SHIFT) {
                parts.push("shift");
            }
            if mods.contains(Modifiers::SUPER) {
                parts.push("cmd");
            }
        }

        parts.push(format_code(&self.code));
        write!(f, "{}", parts.join("+"))
    }
}

impl FromStr for Key {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Key::parse(s)
    }
}

/// Parse a key code from a string
fn parse_code(s: &str) -> Result<Code> {
    match s.to_lowercase().as_str() {
        // Letters
        "a" => Ok(Code::KeyA),
        "b" => Ok(Code::KeyB),
        "c" => Ok(Code::KeyC),
        "d" => Ok(Code::KeyD),
        "e" => Ok(Code::KeyE),
        "f" => Ok(Code::KeyF),
        "g" => Ok(Code::KeyG),
        "h" => Ok(Code::KeyH),
        "i" => Ok(Code::KeyI),
        "j" => Ok(Code::KeyJ),
        "k" => Ok(Code::KeyK),
        "l" => Ok(Code::KeyL),
        "m" => Ok(Code::KeyM),
        "n" => Ok(Code::KeyN),
        "o" => Ok(Code::KeyO),
        "p" => Ok(Code::KeyP),
        "q" => Ok(Code::KeyQ),
        "r" => Ok(Code::KeyR),
        "s" => Ok(Code::KeyS),
        "t" => Ok(Code::KeyT),
        "u" => Ok(Code::KeyU),
        "v" => Ok(Code::KeyV),
        "w" => Ok(Code::KeyW),
        "x" => Ok(Code::KeyX),
        "y" => Ok(Code::KeyY),
        "z" => Ok(Code::KeyZ),

        // Numbers
        "0" | "digit0" => Ok(Code::Digit0),
        "1" | "digit1" => Ok(Code::Digit1),
        "2" | "digit2" => Ok(Code::Digit2),
        "3" | "digit3" => Ok(Code::Digit3),
        "4" | "digit4" => Ok(Code::Digit4),
        "5" | "digit5" => Ok(Code::Digit5),
        "6" | "digit6" => Ok(Code::Digit6),
        "7" | "digit7" => Ok(Code::Digit7),
        "8" | "digit8" => Ok(Code::Digit8),
        "9" | "digit9" => Ok(Code::Digit9),

        // Function keys
        "f1" => Ok(Code::F1),
        "f2" => Ok(Code::F2),
        "f3" => Ok(Code::F3),
        "f4" => Ok(Code::F4),
        "f5" => Ok(Code::F5),
        "f6" => Ok(Code::F6),
        "f7" => Ok(Code::F7),
        "f8" => Ok(Code::F8),
        "f9" => Ok(Code::F9),
        "f10" => Ok(Code::F10),
        "f11" => Ok(Code::F11),
        "f12" => Ok(Code::F12),

        // Special keys
        "escape" | "esc" => Ok(Code::Escape),
        "space" | " " => Ok(Code::Space),
        "enter" | "return" => Ok(Code::Enter),
        "tab" => Ok(Code::Tab),
        "backspace" => Ok(Code::Backspace),
        "delete" | "del" => Ok(Code::Delete),
        "insert" | "ins" => Ok(Code::Insert),
        "home" => Ok(Code::Home),
        "end" => Ok(Code::End),
        "pageup" | "page_up" | "pgup" => Ok(Code::PageUp),
        "pagedown" | "page_down" | "pgdn" => Ok(Code::PageDown),

        // Arrow keys
        "left" | "arrowleft" => Ok(Code::ArrowLeft),
        "right" | "arrowright" => Ok(Code::ArrowRight),
        "up" | "arrowup" => Ok(Code::ArrowUp),
        "down" | "arrowdown" => Ok(Code::ArrowDown),

        // Punctuation and symbols
        "minus" | "-" => Ok(Code::Minus),
        "equal" | "equals" | "=" => Ok(Code::Equal),
        "bracket_left" | "bracketleft" | "[" => Ok(Code::BracketLeft),
        "bracket_right" | "bracketright" | "]" => Ok(Code::BracketRight),
        "backslash" | "\\" => Ok(Code::Backslash),
        "semicolon" | ";" => Ok(Code::Semicolon),
        "quote" | "'" => Ok(Code::Quote),
        "comma" | "," => Ok(Code::Comma),
        "period" | "." => Ok(Code::Period),
        "slash" | "/" => Ok(Code::Slash),
        "backquote" | "grave" | "`" => Ok(Code::Backquote),

        _ => Err(Error::InvalidKey(format!("Unknown key code: {s}"))),
    }
}

/// Format a Code enum value into a user-friendly string
fn format_code(code: &Code) -> &'static str {
    match code {
        // Letters
        Code::KeyA => "a",
        Code::KeyB => "b",
        Code::KeyC => "c",
        Code::KeyD => "d",
        Code::KeyE => "e",
        Code::KeyF => "f",
        Code::KeyG => "g",
        Code::KeyH => "h",
        Code::KeyI => "i",
        Code::KeyJ => "j",
        Code::KeyK => "k",
        Code::KeyL => "l",
        Code::KeyM => "m",
        Code::KeyN => "n",
        Code::KeyO => "o",
        Code::KeyP => "p",
        Code::KeyQ => "q",
        Code::KeyR => "r",
        Code::KeyS => "s",
        Code::KeyT => "t",
        Code::KeyU => "u",
        Code::KeyV => "v",
        Code::KeyW => "w",
        Code::KeyX => "x",
        Code::KeyY => "y",
        Code::KeyZ => "z",

        // Numbers
        Code::Digit0 => "0",
        Code::Digit1 => "1",
        Code::Digit2 => "2",
        Code::Digit3 => "3",
        Code::Digit4 => "4",
        Code::Digit5 => "5",
        Code::Digit6 => "6",
        Code::Digit7 => "7",
        Code::Digit8 => "8",
        Code::Digit9 => "9",

        // Function keys
        Code::F1 => "f1",
        Code::F2 => "f2",
        Code::F3 => "f3",
        Code::F4 => "f4",
        Code::F5 => "f5",
        Code::F6 => "f6",
        Code::F7 => "f7",
        Code::F8 => "f8",
        Code::F9 => "f9",
        Code::F10 => "f10",
        Code::F11 => "f11",
        Code::F12 => "f12",

        // Special keys
        Code::Escape => "escape",
        Code::Space => "space",
        Code::Enter => "enter",
        Code::Tab => "tab",
        Code::Backspace => "backspace",
        Code::Delete => "delete",
        Code::Insert => "insert",
        Code::Home => "home",
        Code::End => "end",
        Code::PageUp => "pageup",
        Code::PageDown => "pagedown",

        // Arrow keys
        Code::ArrowLeft => "left",
        Code::ArrowRight => "right",
        Code::ArrowUp => "up",
        Code::ArrowDown => "down",

        // Punctuation and symbols
        Code::Minus => "minus",
        Code::Equal => "equal",
        Code::BracketLeft => "bracketleft",
        Code::BracketRight => "bracketright",
        Code::Backslash => "backslash",
        Code::Semicolon => "semicolon",
        Code::Quote => "quote",
        Code::Comma => "comma",
        Code::Period => "period",
        Code::Slash => "slash",
        Code::Backquote => "backquote",

        // Fallback for any unhandled codes
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_keys() {
        let key = Key::parse("a").unwrap();
        assert_eq!(key.code, Code::KeyA);
        assert_eq!(key.modifiers, None);

        let key = Key::parse("f1").unwrap();
        assert_eq!(key.code, Code::F1);
        assert_eq!(key.modifiers, None);

        let key = Key::parse("space").unwrap();
        assert_eq!(key.code, Code::Space);
        assert_eq!(key.modifiers, None);
    }

    #[test]
    fn test_parse_with_modifiers() {
        let key = Key::parse("ctrl+a").unwrap();
        assert_eq!(key.code, Code::KeyA);
        assert_eq!(key.modifiers, Some(Modifiers::CONTROL));

        let key = Key::parse("cmd+shift+n").unwrap();
        assert_eq!(key.code, Code::KeyN);
        assert_eq!(key.modifiers, Some(Modifiers::SUPER | Modifiers::SHIFT));

        let key = Key::parse("ctrl+alt+delete").unwrap();
        assert_eq!(key.code, Code::Delete);
        assert_eq!(key.modifiers, Some(Modifiers::CONTROL | Modifiers::ALT));
    }

    #[test]
    fn test_parse_alternative_names() {
        let key1 = Key::parse("control+a").unwrap();
        let key2 = Key::parse("ctrl+a").unwrap();
        assert_eq!(key1, key2);

        let key1 = Key::parse("cmd+a").unwrap();
        let key2 = Key::parse("super+a").unwrap();
        let key3 = Key::parse("win+a").unwrap();
        assert_eq!(key1, key2);
        assert_eq!(key2, key3);

        let key1 = Key::parse("option+a").unwrap();
        let key2 = Key::parse("alt+a").unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_display() {
        let key = Key::parse("ctrl+a").unwrap();
        assert_eq!(key.to_string(), "ctrl+a");

        let key = Key::parse("cmd+shift+n").unwrap();
        assert_eq!(key.to_string(), "shift+cmd+n");

        let key = Key::parse("f1").unwrap();
        assert_eq!(key.to_string(), "f1");

        let key = Key::parse("ctrl+1").unwrap();
        assert_eq!(key.to_string(), "ctrl+1");

        let key = Key::parse("alt+tab").unwrap();
        assert_eq!(key.to_string(), "alt+tab");

        let key = Key::parse("cmd+space").unwrap();
        assert_eq!(key.to_string(), "cmd+space");
    }

    #[test]
    fn test_to_hotkey() {
        let key = Key::parse("ctrl+a").unwrap();
        let hotkey = key.to_hotkey();
        assert_eq!(hotkey.mods, Modifiers::CONTROL);
        assert_eq!(hotkey.key, Code::KeyA);
    }

    #[test]
    fn test_from_hotkey() {
        let hotkey = HotKey::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyN);
        let key: Key = hotkey.into();
        assert_eq!(key.modifiers, Some(Modifiers::CONTROL | Modifiers::SHIFT));
        assert_eq!(key.code, Code::KeyN);

        // Test with no modifiers
        let hotkey = HotKey::new(None, Code::Space);
        let key: Key = hotkey.into();
        assert_eq!(key.modifiers, None);
        assert_eq!(key.code, Code::Space);

        // Test from reference
        let hotkey = HotKey::new(Some(Modifiers::ALT), Code::Tab);
        let key: Key = (&hotkey).into();
        assert_eq!(key.modifiers, Some(Modifiers::ALT));
        assert_eq!(key.code, Code::Tab);
    }

    #[test]
    fn test_serialization() {
        let key = Key::parse("ctrl+a").unwrap();
        let json = serde_json::to_string(&key).unwrap();
        let deserialized: Key = serde_json::from_str(&json).unwrap();
        assert_eq!(key, deserialized);
    }

    #[test]
    fn test_from_str() {
        let key: Key = "ctrl+a".parse().unwrap();
        assert_eq!(key.code, Code::KeyA);
        assert_eq!(key.modifiers, Some(Modifiers::CONTROL));
    }

    #[test]
    fn test_parse_errors() {
        assert!(Key::parse("").is_err());
        assert!(Key::parse("ctrl+").is_err());
        assert!(Key::parse("unknown+a").is_err());
        assert!(Key::parse("ctrl+unknown").is_err());
    }
}
