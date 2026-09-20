//! `feeds.txt`: one feed per line as `Name | URL`, a `# Heading` line starts a
//! group, `//` starts a comment, blank lines are ignored. A `#` line with a
//! `|` in it is a commented-out feed, not a heading. The file is the source
//! of truth; the store mirrors it.

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    pub name: String,
    pub url: String,
    /// Group heading, empty when the feed sits above the first heading.
    pub group: String,
}

/// Parses the file text. A line without ` | ` is an error with its line number.
pub fn parse(text: &str) -> Result<Vec<Feed>, String> {
    let mut feeds = Vec::new();
    let mut group = String::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(heading) = line.strip_prefix('#') {
            if !heading.contains('|') {
                group = heading.trim().to_string();
            }
            continue;
        }
        let (name, url) = line
            .split_once('|')
            .ok_or_else(|| format!("line {}: expected `Name | URL`, got `{line}`", i + 1))?;
        let (name, url) = (name.trim(), url.trim());
        if name.is_empty() || url.is_empty() {
            return Err(format!("line {}: empty name or url", i + 1));
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(format!(
                "line {}: url must start with http:// or https://",
                i + 1
            ));
        }
        feeds.push(Feed {
            name: name.to_string(),
            url: url.to_string(),
            group: group.clone(),
        });
    }
    Ok(feeds)
}

pub fn load(path: &Path) -> Result<Vec<Feed>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Renders feeds back to file text, grouped in first-seen order.
pub fn render(feeds: &[Feed]) -> String {
    let mut out = String::new();
    let mut current: Option<&str> = None;
    for f in feeds {
        if current != Some(f.group.as_str()) {
            if !out.is_empty() {
                out.push('\n');
            }
            if !f.group.is_empty() {
                out.push_str("# ");
                out.push_str(&f.group);
                out.push('\n');
            }
            current = Some(&f.group);
        }
        out.push_str(&f.name);
        out.push_str(" | ");
        out.push_str(&f.url);
        out.push('\n');
    }
    out
}

/// Adds a feed line after the last feed of its group, or under a new
/// heading at the end, editing the file in place so comments and layout
/// survive. Rejects a duplicate URL.
pub fn add(path: &Path, feed: Feed) -> Result<(), String> {
    let text = read(path)?;
    if parse(&text)?.iter().any(|f| f.url == feed.url) {
        return Err(format!("{} is already in {}", feed.url, path.display()));
    }
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let entry = format!("{} | {}", feed.name, feed.url);
    // Index just past the last feed line in the wanted group.
    let mut group = String::new();
    let mut group_seen = feed.group.is_empty();
    let mut insert_at: Option<usize> = None;
    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(h) = line.strip_prefix('#') {
            if !h.contains('|') {
                group = h.trim().to_string();
                if group == feed.group {
                    group_seen = true;
                    insert_at = Some(i + 1);
                }
            }
            continue;
        }
        if group == feed.group {
            insert_at = Some(i + 1);
        }
    }
    match insert_at {
        Some(i) if group_seen => lines.insert(i, entry),
        _ => {
            if lines.last().is_some_and(|l| !l.trim().is_empty()) {
                lines.push(String::new());
            }
            if !feed.group.is_empty() {
                lines.push(format!("# {}", feed.group));
            }
            lines.push(entry);
        }
    }
    write(path, &lines)
}

/// Removes the line carrying this URL, leaving everything else as it was.
/// Returns the removed feed, or an error when it was not there.
pub fn remove(path: &Path, url: &str) -> Result<Feed, String> {
    let text = read(path)?;
    let feed = parse(&text)?
        .into_iter()
        .find(|f| f.url == url)
        .ok_or_else(|| format!("{url} is not in {}", path.display()))?;
    let lines: Vec<String> = text
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with('#')
                || t.starts_with("//")
                || t.split_once('|').is_none_or(|(_, u)| u.trim() != url)
        })
        .map(str::to_string)
        .collect();
    write(path, &lines)?;
    Ok(feed)
}

fn read(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write(path: &Path, lines: &[String]) -> Result<(), String> {
    let mut text = lines.join("\n");
    text.push('\n');
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes the whole file from scratch; comments do not survive this.
pub fn save(path: &Path, feeds: &[Feed]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, render(feeds)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Picked sources. One per line: Display Name | feed URL\n\n# Tech\nArs Technica | https://feeds.arstechnica.com/arstechnica/index\nHacker News | https://hnrss.org/frontpage\n\n# Games\nRock Paper Shotgun | https://www.rockpapershotgun.com/feed\n";

    #[test]
    fn parses_groups() {
        let feeds = parse(SAMPLE).unwrap();
        assert_eq!(feeds.len(), 3);
        assert_eq!(feeds[0].name, "Ars Technica");
        assert_eq!(feeds[0].group, "Tech");
        assert_eq!(feeds[2].group, "Games");
        assert_eq!(feeds[2].url, "https://www.rockpapershotgun.com/feed");
    }

    #[test]
    fn comments_and_commented_out_feeds_are_not_headings() {
        let text = "// notes up top\n# Tech\n#   Old | https://old.example/feed\nA | https://a\n// B | https://b\n";
        let feeds = parse(text).unwrap();
        assert_eq!(feeds.len(), 1);
        assert_eq!(feeds[0].group, "Tech");
    }

    #[test]
    fn reports_bad_line() {
        let err = parse("# Tech\nno pipe here\n").unwrap_err();
        assert!(err.starts_with("line 2:"), "{err}");
        assert!(parse("A | ftp://x\n").is_err());
        assert!(parse("| https://x\n").is_err());
    }

    #[test]
    fn render_round_trips() {
        let feeds = parse(SAMPLE).unwrap();
        let text = render(&feeds);
        assert_eq!(parse(&text).unwrap(), feeds);
        assert!(text.starts_with("# Tech\n"));
    }

    #[test]
    fn add_and_remove_edit_in_place() {
        let dir = std::env::temp_dir().join(format!("herdr-rss-feeds-{}", std::process::id()));
        let path = dir.join("feeds.txt");
        let feed = |name: &str, group: &str| Feed {
            name: name.into(),
            url: format!("https://{name}.example/feed"),
            group: group.into(),
        };
        add(&path, feed("a", "Tech")).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Tech\na | https://a.example/feed\n"
        );
        add(&path, feed("b", "Games")).unwrap();
        add(&path, feed("c", "Tech")).unwrap();
        add(&path, feed("d", "")).unwrap();
        let names: Vec<String> = load(&path).unwrap().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["a", "c", "b", "d"]);
        assert!(add(&path, feed("c", "Tech")).is_err(), "duplicate url");

        // Comments and a commented-out feed stay where they are.
        let mut text = std::fs::read_to_string(&path).unwrap();
        text = text.replace(
            "# Tech\n",
            "// kept\n# Tech\n#   Old | https://old.example/feed\n",
        );
        std::fs::write(&path, &text).unwrap();
        add(&path, feed("e", "Tech")).unwrap();
        assert_eq!(remove(&path, "https://c.example/feed").unwrap().name, "c");
        assert!(remove(&path, "https://c.example/feed").is_err());
        let got = std::fs::read_to_string(&path).unwrap();
        assert_eq!(
            got,
            "// kept\n# Tech\n#   Old | https://old.example/feed\na | https://a.example/feed\ne | https://e.example/feed\n\n# Games\nb | https://b.example/feed\n\nd | https://d.example/feed\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ungrouped_feeds_have_no_heading() {
        let feeds = parse("A | https://a\n").unwrap();
        assert_eq!(feeds[0].group, "");
        assert_eq!(render(&feeds), "A | https://a\n");
    }
}
