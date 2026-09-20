//! Subcommands over the store. Every one takes `--json`. Agents use these;
//! the skill in `skills/herdr-rss` documents them.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;

use crate::config::Config;
use crate::feeds::{self, Feed};
use crate::fetch::{self, Cache, Outcome};
use crate::herdr::PluginEnv;
use crate::store::{FetchRecord, ItemRow, ListQuery, Store};
use crate::time::{age, date, now};
use crate::{html, parse};

const PENDING: &str = "not built yet; see docs/PLAN.md milestones";

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

/// `add URL [--name N] [--group G]`: append a line to feeds.txt.
pub fn add(args: &[String]) -> Result<(), String> {
    let a = Args::parse(args)?;
    let url = a.positional.first().ok_or("add: missing URL")?;
    let feed = Feed {
        name: a.value("--name").unwrap_or(url).to_string(),
        url: url.clone(),
        group: a.value("--group").unwrap_or_default().to_string(),
    };
    let env = PluginEnv::from_env()?;
    add_feed(&env.config_dir.join("feeds.txt"), feed)
}

/// Adds a feed under its group, keeping the file's group order. Rejects a
/// duplicate URL.
pub fn add_feed(path: &Path, feed: Feed) -> Result<(), String> {
    let mut all = feeds::load(path)?;
    if all.iter().any(|f| f.url == feed.url) {
        return Err(format!("{} is already in {}", feed.url, path.display()));
    }
    let at = all
        .iter()
        .rposition(|f| f.group == feed.group)
        .map_or(all.len(), |i| i + 1);
    all.insert(at, feed);
    std::fs::write(path, feeds::render(&all)).map_err(|e| format!("{}: {e}", path.display()))
}

/// `import FILE`: OPML to feeds.txt.
pub fn import(_args: &[String]) -> Result<(), String> {
    Err(format!("import: {PENDING}"))
}

/// `export`: feeds.txt to OPML on stdout.
pub fn export(_args: &[String]) -> Result<(), String> {
    Err(format!("export: {PENDING}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(name: &str, group: &str) -> Feed {
        Feed {
            name: name.into(),
            url: format!("https://{name}.example/feed"),
            group: group.into(),
        }
    }

    #[test]
    fn add_inserts_at_end_of_its_group() {
        let dir = std::env::temp_dir().join(format!("herdr-rss-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("feeds.txt");
        std::fs::write(
            &path,
            "# Tech\nA | https://a.example/feed\n\n# Games\nB | https://b.example/feed\n",
        )
        .unwrap();

        add_feed(&path, feed("c", "Tech")).unwrap();
        add_feed(&path, feed("d", "")).unwrap();
        let got = feeds::load(&path).unwrap();
        let names: Vec<&str> = got.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["A", "c", "B", "d"]);
        assert!(add_feed(&path, feed("c", "Tech")).is_err());
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
