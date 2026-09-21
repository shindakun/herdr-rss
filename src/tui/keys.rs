//! Key to action. One place to read to learn the keys; the help overlay is
//! generated from `HELP`.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

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
    ("/", "search titles in this list; empty clears"),
    ("a", "add a feed by URL under the selected group"),
    ("d", "delete the selected feed"),
    ("o", "open item in browser"),
    ("1 … 9", "open that numbered link from the article"),
    ("y", "copy item link"),
    ("f", "fetch the full article from its page"),
    ("Z", "toggle zoom"),
    ("?", "this help"),
    ("q", "quit"),
    ("click", "focus and select; again on an item opens it"),
    (
        "wheel",
        "focus that column; move the list or scroll the article",
    ),
];

pub fn handle_mouse(app: &mut App, m: MouseEvent) -> Result<(), String> {
    if app.show_help {
        if matches!(m.kind, MouseEventKind::Down(_)) {
            app.show_help = false;
        }
        return Ok(());
    }
    match m.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            app.status = None;
            app.click(m.column, m.row)
        }
        MouseEventKind::ScrollDown => app.wheel(m.column, m.row, true),
        MouseEventKind::ScrollUp => app.wheel(m.column, m.row, false),
        _ => Ok(()),
    }
}

pub fn handle(app: &mut App, key: KeyEvent) -> Result<(), String> {
    if app.show_help {
        app.show_help = false;
        return Ok(());
    }
    if app.prompt.is_some() {
        return match key.code {
            KeyCode::Esc => {
                app.prompt_cancel();
                Ok(())
            }
            KeyCode::Enter => app.prompt_enter(),
            KeyCode::Backspace => {
                app.prompt_backspace();
                Ok(())
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.prompt_cancel();
                Ok(())
            }
            KeyCode::Char(c) => app.prompt_char(c),
            _ => Ok(()),
        };
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
        KeyCode::Char('/') => app.start_search(),
        KeyCode::Char('a') => app.start_add(),
        KeyCode::Char('d') => app.start_delete(),
        KeyCode::Char('o') => app.open_in_browser(),
        KeyCode::Char(c @ '1'..='9') => app.open_link(c as usize - '0' as usize),
        KeyCode::Char('y') => app.copy_link(),
        KeyCode::Char('f') => app.fetch_article(),
        KeyCode::Char('Z') => app.toggle_zoom(),
        KeyCode::Char('?') => app.show_help = true,
        _ => {}
    }
    Ok(())
}
