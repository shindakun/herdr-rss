//! Drawing: three columns wide, one column narrow, a status line, and the
//! help overlay.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use super::app::{App, Areas, Column, Hyperlink, Row};
use super::article;
use super::keys::HELP;
use crate::time;

const FEEDS_WIDTH: u16 = 24;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.width = area.width;
    app.height = area.height;
    app.hyperlinks.clear();
    let [main, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

    if app.narrow() {
        app.areas = Areas::default();
        match app.column {
            Column::Feeds => {
                app.areas.feeds = main;
                draw_feeds(frame, app, main);
            }
            Column::Items => {
                app.areas.items = main;
                draw_items(frame, app, main);
            }
            Column::Article => {
                app.areas.article = main;
                draw_article(frame, app, main);
            }
        }
    } else {
        let [feeds, items, article] = Layout::horizontal([
            Constraint::Length(FEEDS_WIDTH),
            Constraint::Percentage(40),
            Constraint::Fill(1),
        ])
        .areas(main);
        app.areas = Areas {
            feeds,
            items,
            article,
        };
        draw_feeds(frame, app, feeds);
        draw_items(frame, app, items);
        draw_article(frame, app, article);
    }

    let hint = if app.narrow() {
        "? help"
    } else {
        "r refresh  o open  s star  m read  Z zoom  ? help"
    };
    let line = match app.prompt_line() {
        Some(p) => {
            // Put the terminal cursor at the end of the input.
            let x = status.x + (p.chars().count() as u16).min(status.width.saturating_sub(1));
            frame.set_cursor_position((x, status.y));
            Line::from(Span::styled(p, Style::default().fg(Color::Yellow)))
        }
        None => {
            let left = app.status_line();
            let pad = (status.width as usize).saturating_sub(left.chars().count() + hint.len() + 1);
            Line::from(vec![
                Span::raw(left),
                Span::raw(" ".repeat(pad.max(1))),
                Span::styled(hint, Style::default().fg(Color::DarkGray)),
            ])
        }
    };
    frame.render_widget(Paragraph::new(line), status);

    if app.show_help {
        // The overlay covers the article; hyperlinks are repainted after the
        // frame and would punch through it.
        app.hyperlinks.clear();
        draw_help(frame, area);
    }
}

fn block(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(style)
        .title(Span::styled(
            format!(" {title} "),
            Style::default().add_modifier(Modifier::BOLD),
        ))
}

fn highlight() -> Style {
    Style::default().bg(Color::Blue).fg(Color::White)
}

fn draw_feeds(frame: &mut Frame, app: &mut App, area: Rect) {
    let inner = area.width.saturating_sub(2) as usize;
    let lines: Vec<ListItem> = app
        .rows
        .iter()
        .map(|row| {
            let (label, count, marker, indent) = match row {
                Row::All => ("All".to_string(), app.unread_total(), "", 0),
                Row::Starred => ("Starred".to_string(), -1, "", 0),
                Row::Group(g) => {
                    let arrow = if app.folded.contains(g) {
                        "▸ "
                    } else {
                        "▾ "
                    };
                    (format!("{arrow}{g}"), app.group_unread(g), "", 0)
                }
                Row::Feed(i) => {
                    let f = &app.feeds[*i];
                    let marker = if f.last_error.is_some() { "!" } else { "" };
                    let indent = if f.group.is_empty() { 0 } else { 2 };
                    (f.name.clone(), f.unread, marker, indent)
                }
            };
            let count_s = if count < 0 {
                String::new()
            } else {
                count.to_string()
            };
            let right = format!("{marker}{count_s}");
            let avail = inner.saturating_sub(indent + right.len() + 1);
            let name = truncate(&label, avail);
            let pad = avail.saturating_sub(name.chars().count());
            let text = format!("{}{name}{} {right}", " ".repeat(indent), " ".repeat(pad));
            let mut style = Style::default();
            if count > 0 || matches!(row, Row::Group(_)) {
                style = style.add_modifier(Modifier::BOLD);
            }
            if marker == "!" {
                style = style.fg(Color::Red);
            }
            ListItem::new(text).style(style)
        })
        .collect();
    app.feeds_state.select(Some(app.feed_sel));
    frame.render_stateful_widget(
        List::new(lines)
            .block(block("Feeds", app.column == Column::Feeds))
            .highlight_style(highlight()),
        area,
        &mut app.feeds_state,
    );
}

fn draw_items(frame: &mut Frame, app: &mut App, area: Rect) {
    let inner = area.width.saturating_sub(2) as usize;
    let now = time::now();
    let show_feed = !matches!(app.rows.get(app.feed_sel), Some(Row::Feed(_)));
    let lines: Vec<ListItem> = app
        .items
        .iter()
        .map(|it| {
            let dot = if it.read { "  " } else { "● " };
            let star = if it.starred { "* " } else { "" };
            let age = time::age(now - it.published);
            let feed = if show_feed {
                format!("{}: ", truncate(&it.feed_name, 14))
            } else {
                String::new()
            };
            let head = format!("{dot}{star}{feed}");
            let avail = inner.saturating_sub(head.chars().count() + age.len() + 1);
            let title = truncate(&it.title, avail);
            let pad = avail.saturating_sub(title.chars().count());
            let mut style = Style::default();
            if !it.read {
                style = style.add_modifier(Modifier::BOLD);
            } else {
                style = style.fg(Color::Gray);
            }
            ListItem::new(Line::from(vec![
                Span::styled(head, style),
                Span::styled(title, style),
                Span::raw(" ".repeat(pad + 1)),
                Span::styled(age, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    let title = match app.rows.get(app.feed_sel) {
        Some(Row::All) | None => "Items".to_string(),
        Some(Row::Starred) => "Starred".to_string(),
        Some(Row::Group(g)) => g.clone(),
        Some(Row::Feed(i)) => app.feeds[*i].name.clone(),
    };
    app.items_state.select(if app.items.is_empty() {
        None
    } else {
        Some(app.item_sel)
    });
    frame.render_stateful_widget(
        List::new(lines)
            .block(block(&title, app.column == Column::Items))
            .highlight_style(highlight()),
        area,
        &mut app.items_state,
    );
}

fn draw_article(frame: &mut Frame, app: &mut App, area: Rect) {
    let width = area.width.saturating_sub(2);
    let focused = app.column == Column::Article;
    let Some(it) = app.selected_item().cloned() else {
        frame.render_widget(
            Paragraph::new("no items").block(block("Article", focused)),
            area,
        );
        return;
    };
    let body = app.article_text(width.max(20));
    let links = app.article_links();
    let mut meta = vec![it.feed_name.clone()];
    if let Some(a) = &it.author {
        meta.push(a.clone());
    }
    meta.push(time::date(it.published));
    if app.showing_full_text() {
        meta.push("full text".into());
    }
    let rows = article::layout(
        &it.title,
        &meta.join(" · "),
        it.link.as_deref(),
        &body,
        &links,
        width.max(20) as usize,
    );

    let visible = area.height.saturating_sub(2);
    let max_scroll = (rows.len() as u16).saturating_sub(visible);
    if app.article_scroll > max_scroll {
        app.article_scroll = max_scroll;
    }
    let scroll = app.article_scroll as usize;
    let lines: Vec<Line> = rows
        .iter()
        .map(|r| Line::styled(r.text.clone(), r.style.style()))
        .collect();
    for (i, r) in rows.iter().enumerate().skip(scroll).take(visible as usize) {
        let y = area.y + 1 + (i - scroll) as u16;
        for (x, len, url) in &r.links {
            let text: String = r
                .text
                .chars()
                .skip(*x as usize)
                .take(*len as usize)
                .collect();
            app.hyperlinks.push(Hyperlink {
                x: area.x + 1 + x,
                y,
                text,
                url: url.clone(),
                style: r.style,
            });
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .block(block("Article", focused))
            .scroll((app.article_scroll, 0)),
        area,
    );
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let key_w = HELP
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, what)| {
            Line::from(vec![
                Span::styled(
                    format!("{k:>key_w$}  "),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(*what),
            ])
        })
        .collect();
    let h = (lines.len() as u16 + 2).min(area.height);
    let w = (key_w as u16 + 40).min(area.width);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    };
    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(block("Keys", true)), popup);
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else if n == 0 {
        String::new()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('…');
        t
    }
}
