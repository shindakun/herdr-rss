//! `feeds.txt`: one feed per line as `Name | URL`, a `# Heading` line starts a
//! group, blank lines are ignored. The file is the source of truth; the store
//! mirrors it.

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
        if line.is_empty() {
            continue;
        }
        if let Some(heading) = line.strip_prefix('#') {
            group = heading.trim().to_string();
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
    fn ungrouped_feeds_have_no_heading() {
        let feeds = parse("A | https://a\n").unwrap();
        assert_eq!(feeds[0].group, "");
        assert_eq!(render(&feeds), "A | https://a\n");
    }
}
