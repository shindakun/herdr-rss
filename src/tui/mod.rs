//! The reader pane: ratatui event loop, three columns, a refresh worker on a
//! channel. Milestone 2. Layout and keys are in docs/PLAN.md.

use crate::config::Config;
use crate::herdr::PluginEnv;
use crate::store::{ListQuery, Store};

pub fn run() -> Result<(), String> {
    let env = PluginEnv::from_env()?;
    let config = Config::load(&env)?;
    let store = Store::open(&env.state_dir.join("rss.db"))?;
    let feeds = store.feeds()?;
    let unread = store
        .list(&ListQuery {
            unread_only: true,
            ..Default::default()
        })?
        .len();
    Err(format!(
        "reader not built yet; {} feeds, {unread} unread, refresh every {} min. Use `herdr-rss list`. See docs/PLAN.md.",
        feeds.len(),
        config.refresh_minutes
    ))
}
