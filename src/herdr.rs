//! The environment Herdr injects into plugin commands and calls back into
//! Herdr through `HERDR_BIN_PATH`. Names follow herdr 0.9.1.

use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone)]
#[allow(dead_code)] // state_dir and run() land with the store and the TUI
pub struct PluginEnv {
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub bin_path: PathBuf,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

impl PluginEnv {
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            config_dir: var("HERDR_PLUGIN_CONFIG_DIR")
                .map(PathBuf::from)
                .ok_or("HERDR_PLUGIN_CONFIG_DIR is not set; run under herdr")?,
            state_dir: var("HERDR_PLUGIN_STATE_DIR")
                .map(PathBuf::from)
                .ok_or("HERDR_PLUGIN_STATE_DIR is not set; run under herdr")?,
            bin_path: var("HERDR_BIN_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("herdr")),
        })
    }

    /// The plugin directory. Herdr sets it for pane and action processes; the
    /// launcher script sets it for the probes it runs itself.
    pub fn plugin_root() -> Result<PathBuf, String> {
        var("HERDR_PLUGIN_ROOT")
            .map(PathBuf::from)
            .ok_or_else(|| "HERDR_PLUGIN_ROOT is not set; run under herdr".to_string())
    }

    /// Runs `herdr <args>` and returns stdout. Used for `pane zoom --current`
    /// and friends from inside the TUI.
    #[allow(dead_code)] // the TUI's `Z` (pane zoom --current) uses this
    pub fn run(&self, args: &[&str]) -> Result<String, String> {
        let out = Command::new(&self.bin_path)
            .args(args)
            .output()
            .map_err(|e| format!("spawn {}: {e}", self.bin_path.display()))?;
        if !out.status.success() {
            return Err(format!(
                "herdr {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}
