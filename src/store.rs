//! SQLite at `HERDR_PLUGIN_STATE_DIR/rss.db` through `rusqlite`, WAL mode.
//! `feeds` mirrors feeds.txt; `items` keeps read and star state across
//! refreshes.

use std::path::Path;
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::feeds::Feed;
use crate::parse::Item;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS feeds (
    url TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    grp TEXT NOT NULL DEFAULT '',
    position INTEGER NOT NULL DEFAULT 0,
    etag TEXT,
    last_modified TEXT,
    last_fetch INTEGER,
    last_error TEXT
);
CREATE TABLE IF NOT EXISTS items (
    id TEXT PRIMARY KEY,
    feed_url TEXT NOT NULL REFERENCES feeds(url) ON DELETE CASCADE,
    guid TEXT NOT NULL,
    title TEXT NOT NULL,
    link TEXT,
    author TEXT,
    published INTEGER NOT NULL,
    summary_html TEXT,
    content_html TEXT,
    read INTEGER NOT NULL DEFAULT 0,
    starred INTEGER NOT NULL DEFAULT 0,
    fetched_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS items_feed_published ON items(feed_url, published DESC);
CREATE INDEX IF NOT EXISTS items_published ON items(published DESC);
";

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedRow {
    pub url: String,
    pub name: String,
    pub group: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub last_fetch: Option<i64>,
    pub last_error: Option<String>,
    pub unread: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemRow {
    pub id: String,
    pub feed_url: String,
    pub feed_name: String,
    pub title: String,
    pub link: Option<String>,
    pub author: Option<String>,
    pub published: i64,
    pub summary_html: Option<String>,
    /// Extracted full article, when `f` or `show --full` fetched it.
    pub content_html: Option<String>,
    pub read: bool,
    pub starred: bool,
}

/// What a fetch produced, as the store records it.
#[derive(Debug)]
pub enum FetchRecord<'a> {
    Fetched {
        etag: Option<&'a str>,
        last_modified: Option<&'a str>,
    },
    NotModified,
    Failed(&'a str),
}

#[derive(Debug, Default, Clone)]
pub struct ListQuery {
    pub unread_only: bool,
    pub starred_only: bool,
    pub feed_url: Option<String>,
    /// Restrict to these feeds; the reader uses it for a group row.
    pub feed_urls: Option<Vec<String>>,
    pub limit: Option<usize>,
}

fn sql_err(e: rusqlite::Error) -> String {
    format!("sqlite: {e}")
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let conn = Connection::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        conn.busy_timeout(Duration::from_secs(5)).map_err(sql_err)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(sql_err)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, String> {
        Self::init(Connection::open_in_memory().map_err(sql_err)?)
    }

    fn init(conn: Connection) -> Result<Self, String> {
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(sql_err)?;
        conn.execute_batch(SCHEMA).map_err(sql_err)?;
        // Databases from before the column was renamed.
        let old = conn
            .prepare("PRAGMA table_info(items)")
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(1))?
                    .collect::<Result<Vec<_>, _>>()
            })
            .map_err(sql_err)?
            .iter()
            .any(|c| c == "content_text");
        if old {
            conn.execute_batch("ALTER TABLE items RENAME COLUMN content_text TO content_html")
                .map_err(sql_err)?;
        }
        Ok(Self { conn })
    }

    /// Makes the feeds table match feeds.txt. Feeds no longer in the file go,
    /// and their items with them.
    pub fn sync_feeds(&mut self, feeds: &[Feed]) -> Result<(), String> {
        let tx = self.conn.transaction().map_err(sql_err)?;
        for (i, f) in feeds.iter().enumerate() {
            tx.execute(
                "INSERT INTO feeds (url, name, grp, position) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(url) DO UPDATE SET name = excluded.name, grp = excluded.grp,
                 position = excluded.position",
                params![f.url, f.name, f.group, i as i64],
            )
            .map_err(sql_err)?;
        }
        let keep: Vec<&str> = feeds.iter().map(|f| f.url.as_str()).collect();
        let placeholders = std::iter::repeat_n("?", keep.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = if keep.is_empty() {
            "DELETE FROM feeds".to_string()
        } else {
            format!("DELETE FROM feeds WHERE url NOT IN ({placeholders})")
        };
        tx.execute(&sql, rusqlite::params_from_iter(keep.iter()))
            .map_err(sql_err)?;
        tx.commit().map_err(sql_err)
    }

    pub fn feeds(&self) -> Result<Vec<FeedRow>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT f.url, f.name, f.grp, f.etag, f.last_modified, f.last_fetch, f.last_error,
                        (SELECT COUNT(*) FROM items i WHERE i.feed_url = f.url AND i.read = 0)
                 FROM feeds f ORDER BY f.position",
            )
            .map_err(sql_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(FeedRow {
                    url: r.get(0)?,
                    name: r.get(1)?,
                    group: r.get(2)?,
                    etag: r.get(3)?,
                    last_modified: r.get(4)?,
                    last_fetch: r.get(5)?,
                    last_error: r.get(6)?,
                    unread: r.get(7)?,
                })
            })
            .map_err(sql_err)?;
        rows.collect::<Result<_, _>>().map_err(sql_err)
    }

    pub fn record_fetch(&self, url: &str, at: i64, rec: &FetchRecord) -> Result<(), String> {
        match rec {
            FetchRecord::Fetched {
                etag,
                last_modified,
            } => self.conn.execute(
                "UPDATE feeds SET etag = ?2, last_modified = ?3, last_fetch = ?4, last_error = NULL
                 WHERE url = ?1",
                params![url, etag, last_modified, at],
            ),
            FetchRecord::NotModified => self.conn.execute(
                "UPDATE feeds SET last_fetch = ?2, last_error = NULL WHERE url = ?1",
                params![url, at],
            ),
            FetchRecord::Failed(err) => self.conn.execute(
                "UPDATE feeds SET last_fetch = ?2, last_error = ?3 WHERE url = ?1",
                params![url, at, err],
            ),
        }
        .map(drop)
        .map_err(sql_err)
    }

    /// Inserts new items and refreshes title, link, author, date, and body on
    /// known ones. Read and star state survive. An item the feed does not date
    /// keeps the time it was first seen, stepped back by its position so the
    /// feed's own order holds. Unknown items older than `min_published` are
    /// skipped so a prune does not churn. Returns how many were new.
    pub fn upsert_items(
        &mut self,
        items: &[Item],
        fetched_at: i64,
        min_published: Option<i64>,
    ) -> Result<usize, String> {
        let tx = self.conn.transaction().map_err(sql_err)?;
        let mut new = 0;
        {
            let mut exists = tx
                .prepare("SELECT 1 FROM items WHERE id = ?1")
                .map_err(sql_err)?;
            let mut upsert = tx
                .prepare(
                    "INSERT INTO items (id, feed_url, guid, title, link, author, published,
                                        summary_html, fetched_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, COALESCE(?7, ?10), ?8, ?9)
                     ON CONFLICT(id) DO UPDATE SET title = excluded.title, link = excluded.link,
                        author = excluded.author, published = COALESCE(?7, items.published),
                        summary_html = excluded.summary_html",
                )
                .map_err(sql_err)?;
            for (pos, it) in items.iter().enumerate() {
                let known = exists.exists(params![it.id]).map_err(sql_err)?;
                let first_seen = fetched_at - pos as i64;
                if !known && min_published.is_some_and(|m| it.published.unwrap_or(first_seen) < m) {
                    continue;
                }
                upsert
                    .execute(params![
                        it.id,
                        it.feed_url,
                        it.guid,
                        it.title,
                        it.link,
                        it.author,
                        it.published,
                        it.summary_html,
                        fetched_at,
                        first_seen
                    ])
                    .map_err(sql_err)?;
                if !known {
                    new += 1;
                }
            }
        }
        tx.commit().map_err(sql_err)?;
        Ok(new)
    }

    pub fn list(&self, q: &ListQuery) -> Result<Vec<ItemRow>, String> {
        let mut sql = String::from(
            "SELECT i.id, i.feed_url, f.name, i.title, i.link, i.author, i.published,
                    i.summary_html, i.content_html, i.read, i.starred
             FROM items i JOIN feeds f ON f.url = i.feed_url WHERE 1 = 1",
        );
        let mut args: Vec<rusqlite::types::Value> = Vec::new();
        if q.unread_only {
            sql.push_str(" AND i.read = 0");
        }
        if q.starred_only {
            sql.push_str(" AND i.starred = 1");
        }
        if let Some(url) = &q.feed_url {
            sql.push_str(" AND i.feed_url = ?");
            args.push(url.clone().into());
        }
        if let Some(urls) = &q.feed_urls {
            let marks = std::iter::repeat_n("?", urls.len())
                .collect::<Vec<_>>()
                .join(",");
            sql.push_str(&format!(" AND i.feed_url IN ({marks})"));
            args.extend(urls.iter().map(|u| u.clone().into()));
        }
        sql.push_str(" ORDER BY i.published DESC, i.id");
        if let Some(n) = q.limit {
            sql.push_str(" LIMIT ?");
            args.push((n as i64).into());
        }
        let mut stmt = self.conn.prepare(&sql).map_err(sql_err)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(args), row_to_item)
            .map_err(sql_err)?;
        rows.collect::<Result<_, _>>().map_err(sql_err)
    }

    pub fn get(&self, id: &str) -> Result<Option<ItemRow>, String> {
        self.conn
            .query_row(
                "SELECT i.id, i.feed_url, f.name, i.title, i.link, i.author, i.published,
                        i.summary_html, i.content_html, i.read, i.starred
                 FROM items i JOIN feeds f ON f.url = i.feed_url WHERE i.id = ?1",
                params![id],
                row_to_item,
            )
            .optional()
            .map_err(sql_err)
    }

    /// Returns how many rows changed.
    pub fn set_read(&self, ids: &[String], read: bool) -> Result<usize, String> {
        let mut n = 0;
        for id in ids {
            n += self
                .conn
                .execute(
                    "UPDATE items SET read = ?2 WHERE id = ?1",
                    params![id, read as i64],
                )
                .map_err(sql_err)?;
        }
        Ok(n)
    }

    /// Flips the star and returns the new state, or None for an unknown id.
    pub fn toggle_star(&self, id: &str) -> Result<Option<bool>, String> {
        let n = self
            .conn
            .execute(
                "UPDATE items SET starred = 1 - starred WHERE id = ?1",
                params![id],
            )
            .map_err(sql_err)?;
        if n == 0 {
            return Ok(None);
        }
        self.conn
            .query_row(
                "SELECT starred FROM items WHERE id = ?1",
                params![id],
                |r| r.get::<_, i64>(0),
            )
            .map(|s| Some(s == 1))
            .map_err(sql_err)
    }

    /// Stores the extracted article for an item.
    pub fn set_content(&self, id: &str, html: &str) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "UPDATE items SET content_html = ?2 WHERE id = ?1",
                params![id, html],
            )
            .map_err(sql_err)?;
        if n == 0 {
            return Err(format!("no item {id}"));
        }
        Ok(())
    }

    /// Deletes unstarred items published before `before`.
    pub fn prune(&self, before: i64) -> Result<usize, String> {
        self.conn
            .execute(
                "DELETE FROM items WHERE starred = 0 AND published < ?1",
                params![before],
            )
            .map_err(sql_err)
    }
}

fn row_to_item(r: &rusqlite::Row) -> rusqlite::Result<ItemRow> {
    Ok(ItemRow {
        id: r.get(0)?,
        feed_url: r.get(1)?,
        feed_name: r.get(2)?,
        title: r.get(3)?,
        link: r.get(4)?,
        author: r.get(5)?,
        published: r.get(6)?,
        summary_html: r.get(7)?,
        content_html: r.get(8)?,
        read: r.get::<_, i64>(9)? == 1,
        starred: r.get::<_, i64>(10)? == 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::item_id;

    fn feed(name: &str) -> Feed {
        Feed {
            name: name.into(),
            url: format!("https://{name}/feed"),
            group: "G".into(),
        }
    }

    fn item(feed: &str, guid: &str, published: i64) -> Item {
        let url = format!("https://{feed}/feed");
        Item {
            id: item_id(&url, guid),
            feed_url: url,
            guid: guid.into(),
            title: format!("{guid} title"),
            link: Some(format!("https://{feed}/{guid}")),
            author: None,
            published: Some(published),
            summary_html: Some("<p>hi</p>".into()),
        }
    }

    #[test]
    fn sync_upsert_list_and_state() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_feeds(&[feed("a"), feed("b")]).unwrap();
        assert_eq!(s.feeds().unwrap().len(), 2);

        let new = s
            .upsert_items(
                &[
                    item("a", "1", 100),
                    item("a", "2", 200),
                    item("b", "1", 150),
                ],
                1000,
                None,
            )
            .unwrap();
        assert_eq!(new, 3);
        // Same guid again: not new, title refreshed, read state kept.
        let id = item_id("https://a/feed", "1");
        assert_eq!(s.set_read(std::slice::from_ref(&id), true).unwrap(), 1);
        let mut again = item("a", "1", 100);
        again.title = "renamed".into();
        assert_eq!(s.upsert_items(&[again], 1001, None).unwrap(), 0);
        let got = s.get(&id).unwrap().unwrap();
        assert_eq!(got.title, "renamed");
        assert!(got.read);

        let all = s.list(&ListQuery::default()).unwrap();
        let titles: Vec<&str> = all.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["2 title", "1 title", "renamed"]);
        assert_eq!(all[0].feed_name, "a");

        let unread = s
            .list(&ListQuery {
                unread_only: true,
                limit: Some(1),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(unread.len(), 1);
        assert_eq!(unread[0].title, "2 title");

        let only_b = s
            .list(&ListQuery {
                feed_url: Some("https://b/feed".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(only_b.len(), 1);
        let both = s
            .list(&ListQuery {
                feed_urls: Some(vec!["https://a/feed".into(), "https://b/feed".into()]),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(both.len(), 3);
        let none = s
            .list(&ListQuery {
                feed_urls: Some(vec![]),
                ..Default::default()
            })
            .unwrap();
        assert!(none.is_empty());
        assert_eq!(s.feeds().unwrap()[0].unread, 1);

        assert_eq!(s.toggle_star(&id).unwrap(), Some(true));
        assert_eq!(s.toggle_star(&id).unwrap(), Some(false));
        assert_eq!(s.toggle_star("nope").unwrap(), None);
    }

    #[test]
    fn removed_feed_takes_its_items() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_feeds(&[feed("a"), feed("b")]).unwrap();
        s.upsert_items(&[item("a", "1", 100), item("b", "1", 100)], 1, None)
            .unwrap();
        s.sync_feeds(&[feed("b")]).unwrap();
        assert_eq!(s.list(&ListQuery::default()).unwrap().len(), 1);
        s.sync_feeds(&[]).unwrap();
        assert!(s.feeds().unwrap().is_empty());
        assert!(s.list(&ListQuery::default()).unwrap().is_empty());
    }

    #[test]
    fn prune_keeps_stars() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_feeds(&[feed("a")]).unwrap();
        s.upsert_items(
            &[
                item("a", "old", 10),
                item("a", "kept", 20),
                item("a", "new", 500),
            ],
            1,
            None,
        )
        .unwrap();
        s.toggle_star(&item_id("https://a/feed", "kept")).unwrap();
        assert_eq!(s.prune(100).unwrap(), 1);
        assert_eq!(s.list(&ListQuery::default()).unwrap().len(), 2);
    }

    #[test]
    fn old_unknown_items_are_skipped_but_known_ones_update() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_feeds(&[feed("a")]).unwrap();
        assert_eq!(s.upsert_items(&[item("a", "old", 10)], 1, None).unwrap(), 1);
        let mut renamed = item("a", "old", 10);
        renamed.title = "renamed".into();
        let n = s
            .upsert_items(
                &[renamed, item("a", "older", 5), item("a", "new", 500)],
                2,
                Some(100),
            )
            .unwrap();
        assert_eq!(n, 1);
        let all = s.list(&ListQuery::default()).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[1].title, "renamed");
    }

    #[test]
    fn dateless_items_keep_first_seen_in_feed_order() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_feeds(&[feed("a")]).unwrap();
        let mut first = item("a", "1", 0);
        first.published = None;
        let mut second = item("a", "2", 0);
        second.published = None;
        s.upsert_items(&[first.clone(), second.clone()], 1000, Some(900))
            .unwrap();
        let all = s.list(&ListQuery::default()).unwrap();
        assert_eq!(all[0].title, "1 title");
        assert_eq!((all[0].published, all[1].published), (1000, 999));
        s.upsert_items(&[first, second], 5000, None).unwrap();
        let all = s.list(&ListQuery::default()).unwrap();
        assert_eq!(
            (all[0].published, all[1].published),
            (1000, 999),
            "a refetch does not move them"
        );
    }

    #[test]
    fn content_is_stored_and_the_old_column_name_migrates() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_feeds(&[feed("a")]).unwrap();
        s.upsert_items(&[item("a", "1", 100)], 1, None).unwrap();
        let id = item_id("https://a/feed", "1");
        s.set_content(&id, "<p>full</p>").unwrap();
        assert_eq!(
            s.get(&id).unwrap().unwrap().content_html.as_deref(),
            Some("<p>full</p>")
        );
        assert!(s.set_content("nope", "x").is_err());

        let old = Connection::open_in_memory().unwrap();
        old.execute_batch(
            "CREATE TABLE feeds (url TEXT PRIMARY KEY, name TEXT NOT NULL, grp TEXT NOT NULL DEFAULT '',
                position INTEGER NOT NULL DEFAULT 0, etag TEXT, last_modified TEXT, last_fetch INTEGER, last_error TEXT);
             CREATE TABLE items (id TEXT PRIMARY KEY, feed_url TEXT NOT NULL REFERENCES feeds(url) ON DELETE CASCADE,
                guid TEXT NOT NULL, title TEXT NOT NULL, link TEXT, author TEXT, published INTEGER NOT NULL,
                summary_html TEXT, content_text TEXT, read INTEGER NOT NULL DEFAULT 0,
                starred INTEGER NOT NULL DEFAULT 0, fetched_at INTEGER NOT NULL);
             INSERT INTO feeds (url, name) VALUES ('https://a/feed', 'a');
             INSERT INTO items (id, feed_url, guid, title, published, content_text, fetched_at)
                VALUES ('x', 'https://a/feed', 'g', 't', 1, '<p>old</p>', 1);",
        )
        .unwrap();
        let s = Store::init(old).unwrap();
        assert_eq!(
            s.get("x").unwrap().unwrap().content_html.as_deref(),
            Some("<p>old</p>")
        );
    }

    #[test]
    fn record_fetch_round_trips() {
        let mut s = Store::open_in_memory().unwrap();
        s.sync_feeds(&[feed("a")]).unwrap();
        let url = "https://a/feed";
        s.record_fetch(
            url,
            5,
            &FetchRecord::Fetched {
                etag: Some("\"x\""),
                last_modified: None,
            },
        )
        .unwrap();
        let f = &s.feeds().unwrap()[0];
        assert_eq!(f.etag.as_deref(), Some("\"x\""));
        assert_eq!(f.last_fetch, Some(5));
        s.record_fetch(url, 6, &FetchRecord::Failed("boom"))
            .unwrap();
        let f = &s.feeds().unwrap()[0];
        assert_eq!(f.last_error.as_deref(), Some("boom"));
        assert_eq!(
            f.etag.as_deref(),
            Some("\"x\""),
            "a failure keeps the cache keys"
        );
        s.record_fetch(url, 7, &FetchRecord::NotModified).unwrap();
        assert!(s.feeds().unwrap()[0].last_error.is_none());
    }
}
