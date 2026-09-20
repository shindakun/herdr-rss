//! Subcommands over the store. Every one takes `--json`. Agents use these;
//! the skill in `skills/herdr-rss` documents them.

use std::path::Path;

use crate::feeds::{self, Feed};
use crate::herdr::PluginEnv;

const PENDING: &str = "not built yet; see docs/PLAN.md milestones";

/// `refresh [--feed URL] [--detach]`: fetch and store.
pub fn refresh(_args: &[String]) -> Result<(), String> {
    Err(format!("refresh: {PENDING}"))
}

/// `list [--unread] [--feed URL] [--limit N]`: print items.
pub fn list(_args: &[String]) -> Result<(), String> {
    Err(format!("list: {PENDING}"))
}

/// `show ID`: print one item's text.
pub fn show(_args: &[String]) -> Result<(), String> {
    Err(format!("show: {PENDING}"))
}

/// `mark ID... [--unread]`: set read state.
pub fn mark(_args: &[String]) -> Result<(), String> {
    Err(format!("mark: {PENDING}"))
}

/// `star ID...`: toggle star.
pub fn star(_args: &[String]) -> Result<(), String> {
    Err(format!("star: {PENDING}"))
}

/// `add URL [--name N] [--group G]`: append a line to feeds.txt.
pub fn add(args: &[String]) -> Result<(), String> {
    let url = args
        .first()
        .filter(|a| !a.starts_with("--"))
        .ok_or("add: missing URL")?;
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let feed = Feed {
        name: flag("--name").unwrap_or_else(|| url.clone()),
        url: url.clone(),
        group: flag("--group").unwrap_or_default(),
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
}
