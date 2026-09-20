//! Reader state and every action a key can take. No terminal code here, so
//! the tests drive it against an in-memory store.

use std::collections::HashSet;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use crate::cli::{self, Ctx, RefreshReport};
use crate::config::Config;
use crate::herdr::PluginEnv;
use crate::html;
use crate::store::{FeedRow, ItemRow, ListQuery, Store};
use crate::time;
use ratatui::layout::Rect;
use ratatui::widgets::ListState;

/// Below this many columns the pane shows one column at a time.
pub const NARROW_BELOW: u16 = 100;
const ITEM_LIMIT: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    Feeds,
    Items,
    Article,
}

/// A row in the feeds column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    All,
    Starred,
    Group(String),
    Feed(usize),
}

pub struct App {
    pub store: Store,
    pub config: Config,
    pub env: Option<PluginEnv>,
    pub feeds: Vec<FeedRow>,
    pub rows: Vec<Row>,
    pub folded: HashSet<String>,
    pub feed_sel: usize,
    pub items: Vec<ItemRow>,
    pub item_sel: usize,
    pub column: Column,
    pub unread_only: bool,
    pub article_scroll: u16,
    /// Rendered body for (item id, width).
    article_cache: Option<(String, u16, String)>,
    /// One-line message shown until the next key.
    pub status: Option<String>,
    pub refreshing: bool,
    pub last_refresh: Option<i64>,
    pub show_help: bool,
    pub quit: bool,
    pub width: u16,
    pub height: u16,
    /// Where the last frame put each column, for mouse hits.
    pub areas: Areas,
    /// List widget state kept across frames so a click can map a row to
    /// an index through the scroll offset.
    pub feeds_state: ListState,
    pub items_state: ListState,
    refresh_rx: Option<Receiver<Result<RefreshReport, String>>>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Areas {
    pub feeds: Rect,
    pub items: Rect,
    pub article: Rect,
}

impl Areas {
    pub fn column_at(&self, x: u16, y: u16) -> Option<Column> {
        let hit = |r: Rect| r.width > 0 && r.contains((x, y).into());
        if hit(self.feeds) {
            Some(Column::Feeds)
        } else if hit(self.items) {
            Some(Column::Items)
        } else if hit(self.article) {
            Some(Column::Article)
        } else {
            None
        }
    }
}

impl App {
    pub fn new(store: Store, config: Config, env: Option<PluginEnv>) -> Result<Self, String> {
        let mut app = Self {
            store,
            config,
            env,
            feeds: Vec::new(),
            rows: Vec::new(),
            folded: HashSet::new(),
            feed_sel: 0,
            items: Vec::new(),
            item_sel: 0,
            column: Column::Items,
            unread_only: false,
            article_scroll: 0,
            article_cache: None,
            status: None,
            refreshing: false,
            last_refresh: None,
            show_help: false,
            quit: false,
            width: 120,
            height: 40,
            areas: Areas::default(),
            feeds_state: ListState::default(),
            items_state: ListState::default(),
            refresh_rx: None,
        };
        app.reload_feeds()?;
        app.reload_items()?;
        Ok(app)
    }

    pub fn narrow(&self) -> bool {
        self.width < NARROW_BELOW
    }

    // ----- loading -----------------------------------------------------

    /// Re-reads feeds and rebuilds the rows, keeping the selection on the
    /// same row where it still exists.
    pub fn reload_feeds(&mut self) -> Result<(), String> {
        let keep = self
            .rows
            .get(self.feed_sel)
            .cloned()
            .map(|r| self.row_key(&r));
        self.feeds = self.store.feeds()?;
        self.last_refresh = self.feeds.iter().filter_map(|f| f.last_fetch).max();
        let mut rows = vec![Row::All, Row::Starred];
        let mut seen: Vec<String> = Vec::new();
        for (i, f) in self.feeds.iter().enumerate() {
            if f.group.is_empty() {
                rows.push(Row::Feed(i));
                continue;
            }
            if !seen.contains(&f.group) {
                seen.push(f.group.clone());
                rows.push(Row::Group(f.group.clone()));
            }
            if !self.folded.contains(&f.group) {
                rows.push(Row::Feed(i));
            }
        }
        self.rows = rows;
        self.feed_sel = keep
            .and_then(|k| self.rows.iter().position(|r| self.row_key(r) == k))
            .unwrap_or(0)
            .min(self.rows.len().saturating_sub(1));
        Ok(())
    }

    fn row_key(&self, r: &Row) -> String {
        match r {
            Row::All => "all".into(),
            Row::Starred => "starred".into(),
            Row::Group(g) => format!("group:{g}"),
            Row::Feed(i) => format!("feed:{}", self.feeds[*i].url),
        }
    }

    pub fn reload_items(&mut self) -> Result<(), String> {
        let keep = self.items.get(self.item_sel).map(|i| i.id.clone());
        let mut q = ListQuery {
            unread_only: self.unread_only,
            limit: Some(ITEM_LIMIT),
            ..Default::default()
        };
        match self.rows.get(self.feed_sel) {
            Some(Row::Starred) => q.starred_only = true,
            Some(Row::Group(g)) => {
                q.feed_urls = Some(
                    self.feeds
                        .iter()
                        .filter(|f| &f.group == g)
                        .map(|f| f.url.clone())
                        .collect(),
                )
            }
            Some(Row::Feed(i)) => q.feed_url = Some(self.feeds[*i].url.clone()),
            Some(Row::All) | None => {}
        }
        self.items = self.store.list(&q)?;
        self.item_sel = keep
            .and_then(|id| self.items.iter().position(|i| i.id == id))
            .unwrap_or(0);
        self.article_scroll = 0;
        Ok(())
    }

    pub fn selected_item(&self) -> Option<&ItemRow> {
        self.items.get(self.item_sel)
    }

    pub fn unread_total(&self) -> i64 {
        self.feeds.iter().map(|f| f.unread).sum()
    }

    pub fn group_unread(&self, group: &str) -> i64 {
        self.feeds
            .iter()
            .filter(|f| f.group == group)
            .map(|f| f.unread)
            .sum()
    }

    /// Body text for the selected item at `width`, cached.
    pub fn article_text(&mut self, width: u16) -> String {
        let Some(it) = self.selected_item() else {
            return String::new();
        };
        if let Some((id, w, text)) = &self.article_cache {
            if id == &it.id && *w == width {
                return text.clone();
            }
        }
        let text = it
            .content_text
            .clone()
            .or_else(|| {
                it.summary_html
                    .as_deref()
                    .map(|h| html::to_text(h, width as usize))
            })
            .unwrap_or_default();
        self.article_cache = Some((it.id.clone(), width, text.clone()));
        text
    }

    // ----- movement ----------------------------------------------------

    pub fn down(&mut self) -> Result<(), String> {
        match self.column {
            Column::Feeds => {
                if self.feed_sel + 1 < self.rows.len() {
                    self.feed_sel += 1;
                    self.reload_items()?;
                }
            }
            Column::Items => {
                if self.item_sel + 1 < self.items.len() {
                    self.item_sel += 1;
                    self.article_scroll = 0;
                }
            }
            Column::Article => self.article_scroll = self.article_scroll.saturating_add(1),
        }
        Ok(())
    }

    pub fn up(&mut self) -> Result<(), String> {
        match self.column {
            Column::Feeds => {
                if self.feed_sel > 0 {
                    self.feed_sel -= 1;
                    self.reload_items()?;
                }
            }
            Column::Items => {
                if self.item_sel > 0 {
                    self.item_sel -= 1;
                    self.article_scroll = 0;
                }
            }
            Column::Article => self.article_scroll = self.article_scroll.saturating_sub(1),
        }
        Ok(())
    }

    pub fn page_down(&mut self) -> Result<(), String> {
        let step = self.height.saturating_sub(4).max(1) as usize;
        match self.column {
            Column::Article => {
                self.article_scroll = self.article_scroll.saturating_add(step as u16)
            }
            Column::Items => {
                self.item_sel = (self.item_sel + step).min(self.items.len().saturating_sub(1));
                self.article_scroll = 0;
            }
            Column::Feeds => {
                self.feed_sel = (self.feed_sel + step).min(self.rows.len().saturating_sub(1));
                self.reload_items()?;
            }
        }
        Ok(())
    }

    pub fn page_up(&mut self) -> Result<(), String> {
        let step = self.height.saturating_sub(4).max(1) as usize;
        match self.column {
            Column::Article => {
                self.article_scroll = self.article_scroll.saturating_sub(step as u16)
            }
            Column::Items => {
                self.item_sel = self.item_sel.saturating_sub(step);
                self.article_scroll = 0;
            }
            Column::Feeds => {
                self.feed_sel = self.feed_sel.saturating_sub(step);
                self.reload_items()?;
            }
        }
        Ok(())
    }

    pub fn top(&mut self) -> Result<(), String> {
        match self.column {
            Column::Article => self.article_scroll = 0,
            Column::Items => {
                self.item_sel = 0;
                self.article_scroll = 0;
            }
            Column::Feeds => {
                self.feed_sel = 0;
                self.reload_items()?;
            }
        }
        Ok(())
    }

    pub fn bottom(&mut self) -> Result<(), String> {
        match self.column {
            Column::Article => self.article_scroll = u16::MAX / 2,
            Column::Items => {
                self.item_sel = self.items.len().saturating_sub(1);
                self.article_scroll = 0;
            }
            Column::Feeds => {
                self.feed_sel = self.rows.len().saturating_sub(1);
                self.reload_items()?;
            }
        }
        Ok(())
    }

    pub fn left(&mut self) {
        self.column = match self.column {
            Column::Feeds => Column::Feeds,
            Column::Items => Column::Feeds,
            Column::Article => Column::Items,
        };
    }

    pub fn right(&mut self) -> Result<(), String> {
        match self.column {
            Column::Feeds => self.column = Column::Items,
            Column::Items => self.open_article()?,
            Column::Article => {}
        }
        Ok(())
    }

    /// Enter: into the next column; on an item it opens the article and
    /// marks it read; on a group row it folds.
    pub fn enter(&mut self) -> Result<(), String> {
        match self.column {
            Column::Feeds => match self.rows.get(self.feed_sel) {
                Some(Row::Group(_)) => self.toggle_fold()?,
                _ => self.column = Column::Items,
            },
            Column::Items => self.open_article()?,
            Column::Article => {}
        }
        Ok(())
    }

    /// Esc: back one column. In the feeds column it does nothing.
    pub fn back(&mut self) {
        self.left();
    }

    fn open_article(&mut self) -> Result<(), String> {
        if self.items.is_empty() {
            return Ok(());
        }
        self.column = Column::Article;
        self.article_scroll = 0;
        self.set_read(self.item_sel, true)
    }

    pub fn toggle_fold(&mut self) -> Result<(), String> {
        let group = match self.rows.get(self.feed_sel) {
            Some(Row::Group(g)) => g.clone(),
            Some(Row::Feed(i)) if !self.feeds[*i].group.is_empty() => self.feeds[*i].group.clone(),
            _ => return Ok(()),
        };
        let now_folded = !self.folded.remove(&group);
        if now_folded {
            self.folded.insert(group.clone());
        }
        self.reload_feeds()?;
        if now_folded {
            if let Some(i) = self
                .rows
                .iter()
                .position(|r| r == &Row::Group(group.clone()))
            {
                self.feed_sel = i;
            }
        }
        self.reload_items()
    }

    // ----- item state --------------------------------------------------

    fn set_read(&mut self, idx: usize, read: bool) -> Result<(), String> {
        let Some(it) = self.items.get_mut(idx) else {
            return Ok(());
        };
        if it.read == read {
            return Ok(());
        }
        it.read = read;
        let id = it.id.clone();
        self.store.set_read(&[id], read)?;
        self.reload_feeds()
    }

    pub fn toggle_read(&mut self) -> Result<(), String> {
        let Some(it) = self.selected_item() else {
            return Ok(());
        };
        let read = !it.read;
        self.set_read(self.item_sel, read)
    }

    /// Marks every item in the current list read.
    pub fn mark_all_read(&mut self) -> Result<(), String> {
        let ids: Vec<String> = self
            .items
            .iter()
            .filter(|i| !i.read)
            .map(|i| i.id.clone())
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        let n = self.store.set_read(&ids, true)?;
        for it in &mut self.items {
            it.read = true;
        }
        self.status = Some(format!("{n} marked read"));
        self.reload_feeds()
    }

    pub fn toggle_star(&mut self) -> Result<(), String> {
        let Some(it) = self.items.get_mut(self.item_sel) else {
            return Ok(());
        };
        if let Some(state) = self.store.toggle_star(&it.id)? {
            it.starred = state;
        }
        Ok(())
    }

    pub fn toggle_unread_filter(&mut self) -> Result<(), String> {
        self.unread_only = !self.unread_only;
        self.reload_items()
    }

    pub fn next_unread(&mut self) {
        match self
            .items
            .iter()
            .skip(self.item_sel + 1)
            .position(|i| !i.read)
        {
            Some(off) => {
                self.item_sel += off + 1;
                self.article_scroll = 0;
                if self.column == Column::Feeds {
                    self.column = Column::Items;
                }
            }
            None => self.status = Some("no more unread".into()),
        }
    }

    pub fn prev_unread(&mut self) {
        match self.items[..self.item_sel].iter().rposition(|i| !i.read) {
            Some(i) => {
                self.item_sel = i;
                self.article_scroll = 0;
            }
            None => self.status = Some("no earlier unread".into()),
        }
    }

    // ----- outside the pane --------------------------------------------

    pub fn open_in_browser(&mut self) {
        let Some(link) = self.selected_item().and_then(|i| i.link.clone()) else {
            self.status = Some("no link".into());
            return;
        };
        self.open_url(&link);
    }

    /// Opens the article's numbered link `n` (1-based, as printed in the
    /// body). Wrapped URLs cannot be clicked in the terminal, so this is the
    /// way to follow one.
    pub fn open_link(&mut self, n: usize) {
        let links = self.article_links();
        match n.checked_sub(1).and_then(|i| links.get(i)) {
            Some(url) => {
                let url = url.clone();
                self.open_url(&url);
            }
            None => self.status = Some(format!("no link [{n}]")),
        }
    }

    /// The `[n]: url` footnotes html2text appends to the body, in order.
    /// Rendered very wide so no footnote wraps; numbering does not depend on
    /// width, so it matches what the pane shows.
    pub fn article_links(&self) -> Vec<String> {
        let Some(it) = self.selected_item() else {
            return Vec::new();
        };
        let text = match (&it.content_text, &it.summary_html) {
            (Some(t), _) => t.clone(),
            (None, Some(h)) => html::to_text(h, 4000),
            (None, None) => return Vec::new(),
        };
        let mut links: Vec<(usize, String)> = text
            .lines()
            .filter_map(|l| {
                let rest = l.strip_prefix('[')?;
                let (n, rest) = rest.split_once("]: ")?;
                let n: usize = n.parse().ok()?;
                let url = rest.trim();
                (!url.is_empty()).then(|| (n, url.to_string()))
            })
            .collect();
        links.sort_by_key(|(n, _)| *n);
        links.into_iter().map(|(_, u)| u).collect()
    }

    fn open_url(&mut self, link: &str) {
        let cmd = self.config.browser.clone().unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "open".into()
            } else {
                "xdg-open".into()
            }
        });
        let mut parts = cmd.split_whitespace();
        let Some(program) = parts.next() else {
            self.status = Some("browser is empty".into());
            return;
        };
        let result = Command::new(program)
            .args(parts)
            .arg(link)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        self.status = Some(match result {
            Ok(_) => format!("opened {link}"),
            Err(e) => format!("{program}: {e}"),
        });
    }

    pub fn copy_link(&mut self) {
        let Some(link) = self.selected_item().and_then(|i| i.link.clone()) else {
            self.status = Some("no link".into());
            return;
        };
        let result = arboard::Clipboard::new().and_then(|mut c| c.set_text(link.clone()));
        self.status = Some(match result {
            Ok(()) => format!("copied {link}"),
            Err(e) => format!("clipboard: {e}"),
        });
    }

    /// `herdr pane zoom` on this pane.
    pub fn toggle_zoom(&mut self) {
        let Some(env) = &self.env else {
            self.status = Some("not running under herdr".into());
            return;
        };
        let pane = std::env::var("HERDR_PANE_ID").ok();
        let args: Vec<&str> = match &pane {
            Some(id) => vec!["pane", "zoom", id],
            None => vec!["pane", "zoom", "--current"],
        };
        if let Err(e) = env.run(&args) {
            self.status = Some(e);
        }
    }

    // ----- mouse -------------------------------------------------------

    /// Left click: focus the column under the pointer and select the row.
    /// A click on the selected item opens it; a click on the selected group
    /// row folds it.
    pub fn click(&mut self, x: u16, y: u16) -> Result<(), String> {
        let Some(col) = self.areas.column_at(x, y) else {
            return Ok(());
        };
        let was = self.column;
        self.column = col;
        match col {
            Column::Feeds => {
                let Some(idx) = row_at(self.areas.feeds, self.feeds_state.offset(), y) else {
                    return Ok(());
                };
                if idx >= self.rows.len() {
                    return Ok(());
                }
                if idx == self.feed_sel && was == Column::Feeds {
                    if let Row::Group(_) = self.rows[idx] {
                        return self.toggle_fold();
                    }
                }
                self.feed_sel = idx;
                self.reload_items()
            }
            Column::Items => {
                let Some(idx) = row_at(self.areas.items, self.items_state.offset(), y) else {
                    return Ok(());
                };
                if idx >= self.items.len() {
                    return Ok(());
                }
                if idx == self.item_sel && was != Column::Feeds {
                    return self.open_article();
                }
                self.item_sel = idx;
                self.article_scroll = 0;
                Ok(())
            }
            Column::Article => Ok(()),
        }
    }

    /// Wheel: move the selection under the pointer, or scroll the article.
    pub fn wheel(&mut self, x: u16, y: u16, down: bool) -> Result<(), String> {
        let Some(col) = self.areas.column_at(x, y) else {
            return Ok(());
        };
        let keep = self.column;
        self.column = col;
        let r = match (col, down) {
            (Column::Article, true) => {
                self.article_scroll = self.article_scroll.saturating_add(3);
                Ok(())
            }
            (Column::Article, false) => {
                self.article_scroll = self.article_scroll.saturating_sub(3);
                Ok(())
            }
            (_, true) => self.down(),
            (_, false) => self.up(),
        };
        self.column = keep;
        r
    }

    // ----- refresh -----------------------------------------------------

    /// Starts a refresh on a worker thread. `all` fetches every feed; else
    /// the selected feed, or the selected group's feeds one by one.
    pub fn refresh(&mut self, all: bool) {
        if self.refreshing {
            self.status = Some("already refreshing".into());
            return;
        }
        if self.env.is_none() {
            self.status = Some("not running under herdr".into());
            return;
        }
        let only: Option<Vec<String>> = if all {
            None
        } else {
            match self.rows.get(self.feed_sel) {
                Some(Row::Feed(i)) => Some(vec![self.feeds[*i].url.clone()]),
                Some(Row::Group(g)) => Some(
                    self.feeds
                        .iter()
                        .filter(|f| &f.group == g)
                        .map(|f| f.url.clone())
                        .collect(),
                ),
                _ => None,
            }
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = Ctx::open().and_then(|mut ctx| match only {
                None => cli::refresh_into(&mut ctx, None),
                Some(urls) => {
                    let mut total = RefreshReport::default();
                    for u in urls {
                        let r = cli::refresh_into(&mut ctx, Some(&u))?;
                        total.feeds += r.feeds;
                        total.fetched += r.fetched;
                        total.not_modified += r.not_modified;
                        total.new_items += r.new_items;
                        total.pruned += r.pruned;
                        total.failed.extend(r.failed);
                    }
                    Ok(total)
                }
            });
            let _ = tx.send(result);
        });
        self.refresh_rx = Some(rx);
        self.refreshing = true;
    }

    /// Picks up a finished refresh, if any, and reloads.
    pub fn poll_refresh(&mut self) -> Result<(), String> {
        let Some(rx) = &self.refresh_rx else {
            return Ok(());
        };
        let Ok(result) = rx.try_recv() else {
            return Ok(());
        };
        self.refresh_rx = None;
        self.refreshing = false;
        match result {
            Ok(r) => {
                self.status = Some(format!(
                    "{} new · {} fetched · {} unchanged · {} failed",
                    r.new_items,
                    r.fetched,
                    r.not_modified,
                    r.failed.len()
                ));
                self.reload_feeds()?;
                self.reload_items()
            }
            Err(e) => {
                self.status = Some(format!("refresh: {e}"));
                Ok(())
            }
        }
    }

    pub fn status_line(&self) -> String {
        if let Some(s) = &self.status {
            return s.clone();
        }
        let mut parts = vec![format!("{} unread", self.unread_total())];
        if self.refreshing {
            parts.push("refreshing…".into());
        } else if let Some(t) = self.last_refresh {
            parts.push(format!("refreshed {} ago", time::age(time::now() - t)));
        }
        if self.unread_only {
            parts.push("unread only".into());
        }
        parts.join(" · ")
    }
}

/// The list index at screen row `y` inside a bordered list `area`, given the
/// list's scroll offset.
fn row_at(area: Rect, offset: usize, y: u16) -> Option<usize> {
    let top = area.y + 1;
    let bottom = area.y + area.height.saturating_sub(1);
    if area.height < 3 || y < top || y >= bottom {
        return None;
    }
    Some(offset + (y - top) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feeds::Feed;
    use crate::parse::{item_id, Item};

    fn app() -> App {
        let mut store = Store::open_in_memory().unwrap();
        let feed = |name: &str, group: &str| Feed {
            name: name.into(),
            url: format!("https://{name}/feed"),
            group: group.into(),
        };
        store
            .sync_feeds(&[feed("a", "Tech"), feed("b", "Tech"), feed("c", "")])
            .unwrap();
        let item = |feed: &str, guid: &str, published: i64| Item {
            id: item_id(&format!("https://{feed}/feed"), guid),
            feed_url: format!("https://{feed}/feed"),
            guid: guid.into(),
            title: format!("{feed}-{guid}"),
            link: Some(format!("https://{feed}/{guid}")),
            author: None,
            published: Some(published),
            summary_html: Some("<p>body</p>".into()),
        };
        store
            .upsert_items(
                &[
                    item("a", "1", 300),
                    item("a", "2", 200),
                    item("b", "1", 250),
                    item("c", "1", 100),
                ],
                1000,
                None,
            )
            .unwrap();
        App::new(store, Config::default(), None).unwrap()
    }

    fn titles(app: &App) -> Vec<String> {
        app.items.iter().map(|i| i.title.clone()).collect()
    }

    #[test]
    fn rows_group_feeds_and_all_lists_everything() {
        let a = app();
        assert_eq!(
            a.rows,
            [
                Row::All,
                Row::Starred,
                Row::Group("Tech".into()),
                Row::Feed(0),
                Row::Feed(1),
                Row::Feed(2)
            ]
        );
        assert_eq!(titles(&a), ["a-1", "b-1", "a-2", "c-1"]);
        assert_eq!(a.unread_total(), 4);
        assert_eq!(a.group_unread("Tech"), 3);
    }

    #[test]
    fn feed_selection_drives_items() {
        let mut a = app();
        a.column = Column::Feeds;
        a.down().unwrap();
        a.down().unwrap();
        assert_eq!(a.rows[a.feed_sel], Row::Group("Tech".into()));
        assert_eq!(titles(&a), ["a-1", "b-1", "a-2"]);
        a.down().unwrap();
        assert_eq!(titles(&a), ["a-1", "a-2"]);
        a.bottom().unwrap();
        assert_eq!(titles(&a), ["c-1"]);
    }

    #[test]
    fn fold_hides_the_groups_feeds() {
        let mut a = app();
        a.column = Column::Feeds;
        a.feed_sel = 2;
        a.toggle_fold().unwrap();
        assert_eq!(
            a.rows,
            [
                Row::All,
                Row::Starred,
                Row::Group("Tech".into()),
                Row::Feed(2)
            ]
        );
        assert_eq!(a.feed_sel, 2);
        a.toggle_fold().unwrap();
        assert_eq!(a.rows.len(), 6);
    }

    #[test]
    fn enter_opens_and_marks_read_selection_does_not() {
        let mut a = app();
        a.down().unwrap();
        assert!(!a.items[1].read);
        a.enter().unwrap();
        assert_eq!(a.column, Column::Article);
        assert!(a.items[1].read);
        assert_eq!(a.unread_total(), 3);
        a.back();
        assert_eq!(a.column, Column::Items);
        a.toggle_read().unwrap();
        assert!(!a.items[1].read);
        assert_eq!(a.unread_total(), 4);
    }

    #[test]
    fn unread_filter_and_next_unread() {
        let mut a = app();
        a.enter().unwrap();
        a.back();
        a.next_unread();
        assert_eq!(a.item_sel, 1);
        a.next_unread();
        a.next_unread();
        assert_eq!(a.item_sel, 3);
        a.next_unread();
        assert_eq!(a.status.as_deref(), Some("no more unread"));
        a.prev_unread();
        assert_eq!(a.item_sel, 2);
        a.toggle_unread_filter().unwrap();
        assert_eq!(titles(&a), ["b-1", "a-2", "c-1"]);
        a.mark_all_read().unwrap();
        assert_eq!(a.unread_total(), 0);
        a.toggle_unread_filter().unwrap();
        assert_eq!(a.items.len(), 4);
    }

    #[test]
    fn stars_and_the_starred_row() {
        let mut a = app();
        a.toggle_star().unwrap();
        assert!(a.items[0].starred);
        a.column = Column::Feeds;
        a.down().unwrap();
        assert_eq!(a.rows[a.feed_sel], Row::Starred);
        assert_eq!(titles(&a), ["a-1"]);
    }

    #[test]
    fn article_text_renders_and_caches() {
        let mut a = app();
        assert_eq!(a.article_text(40), "body");
        assert_eq!(a.article_text(40), "body");
        a.item_sel = 3;
        assert_eq!(a.article_text(40), "body");
    }

    #[test]
    fn clicks_select_open_and_fold() {
        let mut a = app();
        a.areas = Areas {
            feeds: Rect::new(0, 0, 24, 20),
            items: Rect::new(24, 0, 40, 20),
            article: Rect::new(64, 0, 40, 20),
        };
        assert_eq!(row_at(a.areas.feeds, 0, 0), None, "border");
        assert_eq!(row_at(a.areas.feeds, 0, 1), Some(0));
        assert_eq!(row_at(a.areas.feeds, 5, 3), Some(7));
        assert_eq!(row_at(a.areas.feeds, 0, 19), None, "bottom border");

        // Click the Tech group row: select it. Click again: fold it.
        a.click(2, 3).unwrap();
        assert_eq!(a.column, Column::Feeds);
        assert_eq!(a.rows[a.feed_sel], Row::Group("Tech".into()));
        assert_eq!(titles(&a), ["a-1", "b-1", "a-2"]);
        a.click(2, 3).unwrap();
        assert!(a.folded.contains("Tech"));

        // Click the second item: select. Click it again: open, marks read.
        a.click(30, 2).unwrap();
        assert_eq!(a.column, Column::Items);
        assert_eq!(a.item_sel, 1);
        assert!(!a.items[1].read);
        a.click(30, 2).unwrap();
        assert_eq!(a.column, Column::Article);
        assert!(a.items[1].read);

        // Wheel over the items column moves that selection and keeps focus.
        a.wheel(30, 5, true).unwrap();
        assert_eq!(a.item_sel, 2);
        assert_eq!(a.column, Column::Article);
        a.wheel(70, 5, true).unwrap();
        assert_eq!(a.article_scroll, 3);
        a.click(200, 200).unwrap();
        assert_eq!(a.column, Column::Article, "a miss changes nothing");
    }

    #[test]
    fn numbered_links_come_from_the_footnotes() {
        let mut a = app();
        a.items[0].summary_html = Some(
            "<p>See <a href=\"https://x.example/one\">one</a> and <a href=\"https://x.example/two\">two</a>.</p>".into(),
        );
        let links = a.article_links();
        assert_eq!(links, ["https://x.example/one", "https://x.example/two"]);
        assert!(
            a.article_text(20).lines().count() > 4,
            "narrow render wraps"
        );
        a.config.browser = Some("true".into());
        a.open_link(2);
        assert_eq!(a.status.as_deref(), Some("opened https://x.example/two"));
        a.open_link(3);
        assert_eq!(a.status.as_deref(), Some("no link [3]"));
    }

    #[test]
    fn narrow_and_status() {
        let mut a = app();
        assert!(!a.narrow());
        a.width = 80;
        assert!(a.narrow());
        assert_eq!(a.status_line(), "4 unread");
        a.status = Some("hi".into());
        assert_eq!(a.status_line(), "hi");
    }
}
