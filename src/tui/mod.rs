//! The reader pane: terminal setup, the event loop, and a tick that picks up
//! finished refreshes. Layout and keys are in docs/PLAN.md.

mod app;
mod keys;
mod ui;

use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::config::Config;
use crate::herdr::PluginEnv;
use crate::store::Store;
use app::App;

const TICK: Duration = Duration::from_millis(250);

pub fn run() -> Result<(), String> {
    let env = PluginEnv::from_env()?;
    let config = Config::load(&env)?;
    let store = Store::open(&env.state_dir.join("rss.db"))?;
    let mut app = App::new(store, config, Some(env))?;
    if app.feeds.is_empty() {
        app.status = Some("no feeds: add lines to feeds.txt in the config dir, then R".into());
    }

    let mut terminal = ratatui::try_init().map_err(|e| format!("terminal: {e}"))?;
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<(), String> {
    while !app.quit {
        terminal
            .draw(|frame| ui::draw(frame, app))
            .map_err(|e| format!("draw: {e}"))?;
        if event::poll(TICK).map_err(|e| format!("poll: {e}"))? {
            match event::read().map_err(|e| format!("read: {e}"))? {
                Event::Key(key) if key.kind == KeyEventKind::Press => keys::handle(app, key)?,
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
        app.poll_refresh()?;
    }
    Ok(())
}
