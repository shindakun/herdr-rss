//! `config.toml` in the plugin config dir. Every key is optional.

use std::fmt;

use serde::Deserialize;

use crate::herdr::PluginEnv;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Right,
    Down,
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Direction::Right => "right",
            Direction::Down => "down",
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Which way the split opens.
    pub open_direction: Direction,
    /// Auto refresh interval while the pane is open. 0 disables it.
    pub refresh_minutes: u64,
    /// Per-feed HTTP timeout.
    pub fetch_timeout_secs: u64,
    /// Unstarred items older than this are pruned on start.
    pub keep_days: u64,
    /// Command that receives the URL on `o`. Default per OS.
    pub browser: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            open_direction: Direction::Right,
            refresh_minutes: 30,
            fetch_timeout_secs: 15,
            keep_days: 30,
            browser: None,
        }
    }
}

impl Config {
    /// Loads `config.toml`; a missing file is the default config.
    pub fn load(env: &PluginEnv) -> Result<Self, String> {
        let path = env.config_dir.join("config.toml");
        match std::fs::read_to_string(&path) {
            Ok(text) => Self::parse(&text).map_err(|e| format!("{}: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_default() {
        let c = Config::parse("").unwrap();
        assert_eq!(c.open_direction, Direction::Right);
        assert_eq!(c.refresh_minutes, 30);
        assert_eq!(c.keep_days, 30);
        assert!(c.browser.is_none());
    }

    #[test]
    fn reads_direction_and_overrides() {
        let c = Config::parse(
            "open_direction = \"down\"\nrefresh_minutes = 0\nbrowser = \"firefox\"\n",
        )
        .unwrap();
        assert_eq!(c.open_direction, Direction::Down);
        assert_eq!(c.open_direction.to_string(), "down");
        assert_eq!(c.refresh_minutes, 0);
        assert_eq!(c.browser.as_deref(), Some("firefox"));
    }

    #[test]
    fn rejects_unknown_key() {
        assert!(Config::parse("colour = 1\n").is_err());
        assert!(Config::parse("open_direction = \"left\"\n").is_err());
    }
}
