//! The reader pane: ratatui event loop, three columns, a refresh worker on a
//! channel. Milestone 2. Layout and keys are in docs/PLAN.md.

use crate::config::Config;
use crate::herdr::PluginEnv;

pub fn run() -> Result<(), String> {
    let env = PluginEnv::from_env()?;
    let config = Config::load(&env)?;
    let feeds = crate::feeds::load(&env.config_dir.join("feeds.txt"))?;
    Err(format!(
        "reader not built yet; {} feeds in feeds.txt, refresh every {} min. See docs/PLAN.md.",
        feeds.len(),
        config.refresh_minutes
    ))
}
