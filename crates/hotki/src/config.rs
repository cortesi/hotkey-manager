use keymode::Mode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Pos {
    N,
    #[default]
    NE,
    E,
    SE,
    S,
    SW,
    W,
    NW,
    Center,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub keys: Mode,
    #[serde(default)]
    pub pos: Pos,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_deserialization() {
        // Test with proper Config struct format
        let config_text = r#"(
            keys: [
                ("a", "Say hello", shell("echo 'Hello'")),
                ("b", "Say world", shell("echo 'World'")),
                ("m", "Submenu", mode([
                    ("x", "Exit submenu", pop),
                ])),
            ],
            pos: n,
        )"#;

        let config: Config = ron::from_str(config_text).unwrap();

        // Verify we have the expected keys
        let keys = config.keys.keys();
        let key_vec: Vec<_> = keys.collect();
        assert_eq!(key_vec.len(), 3);

        // Check that the keys contain our expected values
        let key_strings: Vec<String> = key_vec.iter().map(|(k, _)| k.clone()).collect();
        assert!(key_strings.contains(&"a".to_string()));
        assert!(key_strings.contains(&"b".to_string()));
        assert!(key_strings.contains(&"m".to_string()));
    }
}
