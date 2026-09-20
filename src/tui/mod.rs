//! The reader pane: terminal setup, the event loop, and a tick that picks up
//! finished refreshes. Layout and keys are in docs/PLAN.md.

mod app;
mod keys;
mod ui;

use std::time::Duration;

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;

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
    let mouse = execute!(std::io::stdout(), EnableMouseCapture).is_ok();
    let result = event_loop(&mut terminal, &mut app);
    if mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
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
                Event::Mouse(m) => keys::handle_mouse(app, m)?,
                _ => {}
            }
        }
        app.poll_refresh()?;
    }
    Ok(())
}
