//! Key to action. One place to read to learn the keys; the help overlay is
//! generated from `HELP`.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::{App, Column};

pub const HELP: &[(&str, &str)] = &[
    ("j k ↓ ↑", "move in the focused column"),
    ("h l ← → Tab", "move between columns"),
    ("Enter / Esc", "open item / back"),
    ("Space", "fold a group; page down elsewhere"),
    ("g G PgUp PgDn", "top, bottom, page"),
    ("r / R", "refresh selected feed / all feeds"),
    ("n / p", "next / previous unread item"),
    ("m / M", "toggle read on item / mark list read"),
    ("s", "toggle star"),
    ("u", "show unread only"),
    ("o", "open item in browser"),
    ("y", "copy item link"),
    ("Z", "toggle zoom"),
    ("?", "this help"),
    ("q", "quit"),
];

pub fn handle(app: &mut App, key: KeyEvent) -> Result<(), String> {
    if app.show_help {
        app.show_help = false;
        return Ok(());
    }
    app.status = None;
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.quit = true;
        return Ok(());
    }
    match key.code {
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Char('j') | KeyCode::Down => app.down()?,
        KeyCode::Char('k') | KeyCode::Up => app.up()?,
        KeyCode::Char('h') | KeyCode::Left | KeyCode::BackTab => app.left(),
        KeyCode::Char('l') | KeyCode::Right | KeyCode::Tab => app.right()?,
        KeyCode::Enter => app.enter()?,
        KeyCode::Esc => app.back(),
        KeyCode::Char(' ') => {
            if app.column == Column::Feeds {
                app.toggle_fold()?
            } else {
                app.page_down()?
            }
        }
        KeyCode::Char('g') | KeyCode::Home => app.top()?,
        KeyCode::Char('G') | KeyCode::End => app.bottom()?,
        KeyCode::PageDown => app.page_down()?,
        KeyCode::PageUp => app.page_up()?,
        KeyCode::Char('r') => app.refresh(false),
        KeyCode::Char('R') => app.refresh(true),
        KeyCode::Char('n') => app.next_unread(),
        KeyCode::Char('p') => app.prev_unread(),
        KeyCode::Char('m') => app.toggle_read()?,
        KeyCode::Char('M') => app.mark_all_read()?,
        KeyCode::Char('s') => app.toggle_star()?,
        KeyCode::Char('u') => app.toggle_unread_filter()?,
        KeyCode::Char('o') => app.open_in_browser(),
        KeyCode::Char('y') => app.copy_link(),
        KeyCode::Char('Z') => app.toggle_zoom(),
        KeyCode::Char('?') => app.show_help = true,
        _ => {}
    }
    Ok(())
}
