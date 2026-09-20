//! Subcommands over the store. Every one takes `--json`. Agents use these;
//! the skill in `skills/herdr-rss` documents them.

use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;

use crate::config::Config;
use crate::feeds::{self, Feed};
use crate::fetch::{self, Cache, Outcome};
use crate::herdr::PluginEnv;
use crate::store::{FetchRecord, ItemRow, ListQuery, Store};
use crate::time::{age, date, now};
use crate::{html, opml, parse};

/// Flags, `--key value` options, and positionals from an argv slice.
struct Args {
    flags: Vec<String>,
    values: Vec<(String, String)>,
    positional: Vec<String>,
}

const VALUE_OPTS: &[&str] = &["--feed", "--limit", "--name", "--group", "--width"];

impl Args {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut a = Args {
            flags: Vec::new(),
            values: Vec::new(),
            positional: Vec::new(),
        };
        let mut it = args.iter();
        while let Some(arg) = it.next() {
            if VALUE_OPTS.contains(&arg.as_str()) {
                let v = it.next().ok_or_else(|| format!("{arg} needs a value"))?;
                a.values.push((arg.clone(), v.clone()));
            } else if arg.starts_with("--") {
                a.flags.push(arg.clone());
            } else {
                a.positional.push(arg.clone());
            }
        }
        Ok(a)
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|f| f == name)
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn json(&self) -> bool {
        self.flag("--json")
    }
}

pub struct Ctx {
    pub env: PluginEnv,
    pub config: Config,
    pub store: Store,
}

impl Ctx {
    pub fn open() -> Result<Self, String> {
        let env = PluginEnv::from_env()?;
        let config = Config::load(&env)?;
        let store = Store::open(&env.state_dir.join("rss.db"))?;
        Ok(Self { env, config, store })
    }

    fn feeds_path(&self) -> std::path::PathBuf {
        self.env.config_dir.join("feeds.txt")
    }
}

fn print_json<T: Serialize>(v: &T) -> Result<(), String> {
    let s = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    println!("{s}");
    Ok(())
}

#[derive(Debug, Default, Serialize)]
pub struct RefreshReport {
    pub feeds: usize,
    pub fetched: usize,
    pub not_modified: usize,
    pub new_items: usize,
    pub pruned: usize,
    pub failed: Vec<FeedError>,
}

#[derive(Debug, Serialize)]
pub struct FeedError {
    pub url: String,
    pub error: String,
}

/// `refresh [--feed URL] [--detach] [--json]`: fetch and store.
pub fn refresh(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args)?;
    if a.flag("--detach") {
        return detach(args);
    }
    let mut ctx = Ctx::open()?;
    let report = refresh_into(&mut ctx, a.value("--feed"))?;
    if a.json() {
        return print_json(&report);
    }
    println!(
        "{} feeds: {} fetched, {} unchanged, {} failed; {} new items, {} pruned",
        report.feeds,
        report.fetched,
        report.not_modified,
        report.failed.len(),
        report.new_items,
        report.pruned
    );
    for f in &report.failed {
        println!("  ! {}: {}", f.url, f.error);
    }
    Ok(())
}

/// Syncs the feed list, fetches every feed (or one), stores what came back,
/// and prunes. Shared by the CLI and the reader's refresh worker.
pub fn refresh_into(ctx: &mut Ctx, only: Option<&str>) -> Result<RefreshReport, String> {
    let feeds = feeds::load(&ctx.feeds_path())?;
    ctx.store.sync_feeds(&feeds)?;
    let rows = ctx.store.feeds()?;
    let jobs: Vec<(String, Cache)> = rows
        .iter()
        .filter(|r| only.is_none_or(|u| u == r.url))
        .map(|r| {
            (
                r.url.clone(),
                Cache {
                    etag: r.etag.clone(),
                    last_modified: r.last_modified.clone(),
                },
            )
        })
        .collect();
    if let Some(u) = only {
        if jobs.is_empty() {
            return Err(format!("{u} is not in feeds.txt"));
        }
    }
    let mut report = RefreshReport {
        feeds: jobs.len(),
        ..Default::default()
    };
    let timeout = Duration::from_secs(ctx.config.fetch_timeout_secs.max(1));
    let cutoff = (ctx.config.keep_days > 0).then(|| now() - (ctx.config.keep_days as i64) * 86_400);
    for (url, outcome) in fetch::fetch_all(jobs, timeout) {
        let at = now();
        match outcome {
            Outcome::NotModified => {
                report.not_modified += 1;
                ctx.store
                    .record_fetch(&url, at, &FetchRecord::NotModified)?;
            }
            Outcome::Failed(err) => {
                ctx.store
                    .record_fetch(&url, at, &FetchRecord::Failed(&err))?;
                report.failed.push(FeedError { url, error: err });
            }
            Outcome::Fetched {
                body,
                etag,
                last_modified,
            } => match parse::parse(&url, &body) {
                Ok(items) => {
                    report.fetched += 1;
                    report.new_items += ctx.store.upsert_items(&items, at, cutoff)?;
                    ctx.store.record_fetch(
                        &url,
                        at,
                        &FetchRecord::Fetched {
                            etag: etag.as_deref(),
                            last_modified: last_modified.as_deref(),
                        },
                    )?;
                }
                Err(err) => {
                    let err = format!("parse: {err}");
                    ctx.store
                        .record_fetch(&url, at, &FetchRecord::Failed(&err))?;
                    report.failed.push(FeedError { url, error: err });
                }
            },
        }
    }
    if let Some(before) = cutoff {
        report.pruned = ctx.store.prune(before)?;
    }
    Ok(report)
}

/// Re-runs `refresh` without `--detach` as its own process group, output to
/// `refresh.log` in the state dir, and returns at once. The startup hook uses
/// this so Herdr's hook returns while the fetch runs.
fn detach(args: &[String]) -> Result<(), String> {
    let env = PluginEnv::from_env()?;
    std::fs::create_dir_all(&env.state_dir)
        .map_err(|e| format!("{}: {e}", env.state_dir.display()))?;
    let log_path = env.state_dir.join("refresh.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("{}: {e}", log_path.display()))?;
    let stderr = log.try_clone().map_err(|e| format!("clone log: {e}"))?;
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.arg("refresh")
        .args(args.iter().filter(|a| *a != "--detach"))
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(stderr));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let child = cmd.spawn().map_err(|e| format!("spawn refresh: {e}"))?;
    println!(
        "refresh: started pid {} (log: {})",
        child.id(),
        log_path.display()
    );
    Ok(())
}

/// `list [--unread] [--starred] [--feed URL] [--limit N] [--json]`.
pub fn list(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args)?;
    let ctx = Ctx::open()?;
    let q = ListQuery {
        unread_only: a.flag("--unread"),
        starred_only: a.flag("--starred"),
        feed_url: a.value("--feed").map(str::to_string),
        feed_urls: None,
        limit: match a.value("--limit") {
            Some(n) => Some(
                n.parse()
                    .map_err(|_| format!("--limit: not a number: {n}"))?,
            ),
            None => Some(50),
        },
    };
    let items = ctx.store.list(&q)?;
    if a.json() {
        return print_json(&items);
    }
    let t = now();
    for it in &items {
        println!("{}", list_line(it, t));
    }
    Ok(())
}

/// `id  ●  age  feed  title`; the dot marks unread, `*` a star.
pub fn list_line(it: &ItemRow, now: i64) -> String {
    format!(
        "{}  {}{}  {:>3}  {:<14}  {}",
        it.id,
        if it.read { ' ' } else { '●' },
        if it.starred { '*' } else { ' ' },
        age(now - it.published),
        truncate(&it.feed_name, 14),
        it.title
    )
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n - 1).collect();
        t.push('…');
        t
    }
}

/// `show ID [--width N] [--json]`: one item as text.
pub fn show(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args)?;
    let id = a.positional.first().ok_or("show: missing ID")?;
    let ctx = Ctx::open()?;
    let it = ctx.store.get(id)?.ok_or_else(|| format!("no item {id}"))?;
    if a.json() {
        return print_json(&it);
    }
    let width: usize = match a.value("--width") {
        Some(w) => w
            .parse()
            .map_err(|_| format!("--width: not a number: {w}"))?,
        None => 80,
    };
    println!("{}", it.title);
    let mut meta = vec![it.feed_name.clone()];
    if let Some(au) = &it.author {
        meta.push(au.clone());
    }
    meta.push(date(it.published));
    println!("{}", meta.join(" · "));
    if let Some(l) = &it.link {
        println!("{l}");
    }
    println!();
    let body = it
        .content_text
        .clone()
        .or_else(|| it.summary_html.as_deref().map(|h| html::to_text(h, width)))
        .unwrap_or_default();
    println!("{body}");
    Ok(())
}

/// `mark ID... [--unread]`: set read state.
pub fn mark(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args)?;
    if a.positional.is_empty() {
        return Err("mark: missing ID".into());
    }
    let ctx = Ctx::open()?;
    let n = ctx.store.set_read(&a.positional, !a.flag("--unread"))?;
    if a.json() {
        return print_json(&serde_json::json!({ "changed": n }));
    }
    println!(
        "{n} marked {}",
        if a.flag("--unread") { "unread" } else { "read" }
    );
    Ok(())
}

/// `star ID...`: toggle the star on each.
pub fn star(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args)?;
    if a.positional.is_empty() {
        return Err("star: missing ID".into());
    }
    let ctx = Ctx::open()?;
    let mut out = Vec::new();
    for id in &a.positional {
        let state = ctx
            .store
            .toggle_star(id)?
            .ok_or_else(|| format!("no item {id}"))?;
        out.push(serde_json::json!({ "id": id, "starred": state }));
        if !a.json() {
            println!("{id} {}", if state { "starred" } else { "unstarred" });
        }
    }
    if a.json() {
        return print_json(&out);
    }
    Ok(())
}

/// `add URL [--name N] [--group G]`: fetch the URL to prove it is a feed,
/// then append it to feeds.txt and store its items.
pub fn add(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args)?;
    let url = a.positional.first().ok_or("add: missing URL")?;
    let mut ctx = Ctx::open()?;
    let feed = add_checked(
        &mut ctx,
        url,
        a.value("--name"),
        a.value("--group").unwrap_or_default(),
    )?;
    if a.json() {
        return print_json(&serde_json::json!({
            "name": feed.name, "url": feed.url, "group": feed.group
        }));
    }
    println!("added {} ({})", feed.name, feed.url);
    Ok(())
}

/// Fetches and parses the feed once. A URL that is not a feed is an error
/// and nothing is written. Without a name, the feed's title is the name.
/// Then writes feeds.txt, syncs the store, and stores the items.
pub fn add_checked(
    ctx: &mut Ctx,
    url: &str,
    name: Option<&str>,
    group: &str,
) -> Result<Feed, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("{url}: url must start with http:// or https://"));
    }
    let path = ctx.feeds_path();
    if feeds::load(&path)?.iter().any(|f| f.url == url) {
        return Err(format!("{url} is already in feeds.txt"));
    }
    let timeout = Duration::from_secs(ctx.config.fetch_timeout_secs.max(1));
    let agent = fetch::agent(timeout);
    let (title, items) = match fetch::fetch_one(&agent, url, &Cache::default()) {
        Outcome::Fetched { body, .. } => {
            parse::parse_feed(url, &body).map_err(|e| format!("{url}: not a feed: {e}"))?
        }
        Outcome::NotModified => return Err(format!("{url}: unexpected 304")),
        Outcome::Failed(e) => return Err(format!("{url}: {e}")),
    };
    let feed = Feed {
        name: name
            .map(str::to_string)
            .filter(|n| !n.trim().is_empty())
            .or(title)
            .unwrap_or_else(|| url.to_string()),
        url: url.to_string(),
        group: group.trim().to_string(),
    };
    feeds::add(&path, feed.clone())?;
    ctx.store.sync_feeds(&feeds::load(&path)?)?;
    let at = now();
    let cutoff = (ctx.config.keep_days > 0).then(|| at - (ctx.config.keep_days as i64) * 86_400);
    ctx.store.upsert_items(&items, at, cutoff)?;
    ctx.store.record_fetch(
        url,
        at,
        &FetchRecord::Fetched {
            etag: None,
            last_modified: None,
        },
    )?;
    Ok(feed)
}

/// `remove URL`: drop a feed and its items.
pub fn remove(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args)?;
    let url = a.positional.first().ok_or("remove: missing URL")?;
    let mut ctx = Ctx::open()?;
    let feed = remove_feed(&mut ctx, url)?;
    if a.json() {
        return print_json(&serde_json::json!({ "name": feed.name, "url": feed.url }));
    }
    println!("removed {} ({})", feed.name, feed.url);
    Ok(())
}

pub fn remove_feed(ctx: &mut Ctx, url: &str) -> Result<Feed, String> {
    let path = ctx.feeds_path();
    let feed = feeds::remove(&path, url)?;
    ctx.store.sync_feeds(&feeds::load(&path)?)?;
    Ok(feed)
}

/// `import FILE [--replace]`: OPML into feeds.txt. Feeds already present
/// are skipped; `--replace` starts the file over. Nothing is fetched.
pub fn import(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args)?;
    let file = a.positional.first().ok_or("import: missing FILE")?;
    let xml = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let incoming = opml::parse(&xml)?;
    let mut ctx = Ctx::open()?;
    let path = ctx.feeds_path();
    let mut added = 0;
    let mut skipped = 0;
    if a.flag("--replace") {
        feeds::save(&path, &incoming)?;
        added = incoming.len();
    } else {
        let have = feeds::load(&path)?;
        for f in incoming {
            if have.iter().any(|x| x.url == f.url) {
                skipped += 1;
                continue;
            }
            feeds::add(&path, f)?;
            added += 1;
        }
    }
    let all = feeds::load(&path)?;
    ctx.store.sync_feeds(&all)?;
    if a.json() {
        return print_json(&serde_json::json!({
            "added": added, "skipped": skipped, "feeds": all.len()
        }));
    }
    println!(
        "{added} added, {skipped} already present; {} feeds in {}. Run `refresh` to fetch them.",
        all.len(),
        path.display()
    );
    Ok(())
}

/// `export`: feeds.txt as OPML on stdout.
pub fn export(_args: &[String]) -> Result<(), String> {
    let env = PluginEnv::from_env()?;
    let all = feeds::load(&env.config_dir.join("feeds.txt"))?;
    print!("{}", opml::render(&all));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::testserver::{ok, serve};

    fn ctx() -> (Ctx, std::path::PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("herdr-rss-cli-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&dir).unwrap();
        let env = PluginEnv {
            config_dir: dir.clone(),
            state_dir: dir.clone(),
            bin_path: "herdr".into(),
        };
        let ctx = Ctx {
            env,
            config: Config::default(),
            store: Store::open_in_memory().unwrap(),
        };
        (ctx, dir)
    }

    #[test]
    fn add_checked_fetches_names_and_stores_then_remove_drops() {
        let (mut ctx, dir) = ctx();
        let (url, h) = serve(ok(include_bytes!("../tests/fixtures/lobsters.rss")));
        let feed = add_checked(&mut ctx, &url, None, "Tech").unwrap();
        h.join().unwrap();
        assert_eq!(feed.name, "Lobsters");
        assert_eq!(feed.group, "Tech");
        let file = feeds::load(&dir.join("feeds.txt")).unwrap();
        assert_eq!(file, std::slice::from_ref(&feed));
        assert!(ctx.store.list(&ListQuery::default()).unwrap().len() >= 10);
        assert!(ctx.store.feeds().unwrap()[0].last_error.is_none());

        // Same URL again: refused before any fetch.
        assert!(add_checked(&mut ctx, &url, None, "")
            .unwrap_err()
            .contains("already"));

        // Not a feed: error, file untouched.
        let (bad, h) = serve(ok(b"<html><body>hello</body></html>"));
        let err = add_checked(&mut ctx, &bad, None, "").unwrap_err();
        h.join().unwrap();
        assert!(err.contains("not a feed"), "{err}");
        assert_eq!(feeds::load(&dir.join("feeds.txt")).unwrap().len(), 1);
        assert!(add_checked(&mut ctx, "ftp://x", None, "").is_err());

        // Explicit name wins over the title.
        let (url2, h) = serve(ok(include_bytes!("../tests/fixtures/daringfireball.atom")));
        let f2 = add_checked(&mut ctx, &url2, Some("DF"), "").unwrap();
        h.join().unwrap();
        assert_eq!(f2.name, "DF");

        let removed = remove_feed(&mut ctx, &url).unwrap();
        assert_eq!(removed.name, "Lobsters");
        assert_eq!(ctx.store.feeds().unwrap().len(), 1);
        assert!(remove_feed(&mut ctx, &url).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn args_split_flags_values_positionals() {
        let a = Args::parse(&[
            "x".into(),
            "--unread".into(),
            "--limit".into(),
            "5".into(),
            "y".into(),
            "--json".into(),
        ])
        .unwrap();
        assert_eq!(a.positional, ["x", "y"]);
        assert!(a.flag("--unread") && a.json());
        assert_eq!(a.value("--limit"), Some("5"));
        assert!(Args::parse(&["--limit".into()]).is_err());
    }

    #[test]
    fn list_line_shape() {
        let it = ItemRow {
            id: "0123456789abcdef".into(),
            feed_url: "u".into(),
            feed_name: "A Very Long Feed Name".into(),
            title: "T".into(),
            link: None,
            author: None,
            published: 0,
            summary_html: None,
            content_text: None,
            read: false,
            starred: true,
        };
        assert_eq!(
            list_line(&it, 3_600),
            "0123456789abcdef  ●*   1h  A Very Long F…  T"
        );
        assert_eq!(date(0), "1970-01-01 00:00 UTC");
    }
}
