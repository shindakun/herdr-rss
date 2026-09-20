//! `feed-rs` output to the store's `Item`. Identity is the feed URL plus the
//! entry id, else its link, else its title and date, hashed to 16 hex chars.

use feed_rs::model::Entry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub feed_url: String,
    pub guid: String,
    pub title: String,
    pub link: Option<String>,
    pub author: Option<String>,
    /// Unix seconds: `published`, else `updated`. None when the feed gives
    /// no date; the store then keeps the time the item was first seen.
    pub published: Option<i64>,
    pub summary_html: Option<String>,
}

/// Parses RSS, Atom, or JSON Feed bytes into items.
pub fn parse(feed_url: &str, bytes: &[u8]) -> Result<Vec<Item>, String> {
    parse_feed(feed_url, bytes).map(|(_, items)| items)
}

/// Parses a feed into its title and items. The title is what `add` uses
/// when no name is given.
pub fn parse_feed(feed_url: &str, bytes: &[u8]) -> Result<(Option<String>, Vec<Item>), String> {
    let feed = feed_rs::parser::parse(bytes).map_err(|e| e.to_string())?;
    let title = feed
        .title
        .map(|t| t.content.trim().to_string())
        .filter(|t| !t.is_empty());
    let items = feed
        .entries
        .iter()
        .map(|e| item_from(feed_url, e))
        .collect();
    Ok((title, items))
}

fn item_from(feed_url: &str, e: &Entry) -> Item {
    let link = e
        .links
        .iter()
        .find(|l| l.rel.as_deref().is_none_or(|r| r == "alternate"))
        .or(e.links.first())
        .map(|l| l.href.clone());
    let title = e
        .title
        .as_ref()
        .map(|t| t.content.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "(untitled)".to_string());
    let published = e.published.or(e.updated).map(|d| d.timestamp());
    let guid = if !e.id.trim().is_empty() {
        e.id.trim().to_string()
    } else if let Some(l) = &link {
        l.clone()
    } else {
        format!("{title}\n{}", published.unwrap_or(0))
    };
    let summary_html = e
        .content
        .as_ref()
        .and_then(|c| c.body.clone())
        .filter(|b| !b.trim().is_empty())
        .or_else(|| e.summary.as_ref().map(text_as_html))
        .filter(|b| !b.trim().is_empty());
    Item {
        id: item_id(feed_url, &guid),
        feed_url: feed_url.to_string(),
        guid,
        title,
        link,
        author: e
            .authors
            .first()
            .map(|p| p.name.clone())
            .filter(|n| !n.is_empty()),
        published,
        summary_html,
    }
}

fn text_as_html(t: &feed_rs::model::Text) -> String {
    if t.content_type.essence().to_string() == "text/plain" {
        format!(
            "<p>{}</p>",
            t.content
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        )
    } else {
        t.content.clone()
    }
}

/// FNV-1a over `feed_url \0 guid`, 16 hex chars. Stable across builds, which
/// `DefaultHasher` does not promise.
pub fn item_id(feed_url: &str, guid: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in feed_url.bytes().chain([0u8]).chain(guid.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss2() {
        let (title, p) = parse_feed(
            "https://lobste.rs/rss",
            include_bytes!("../tests/fixtures/lobsters.rss"),
        )
        .unwrap();
        assert_eq!(title.as_deref(), Some("Lobsters"));
        assert!(p.len() >= 10, "{}", p.len());
        let i = &p[0];
        assert_eq!(i.id.len(), 16);
        assert!(i.link.as_deref().unwrap().starts_with("https://"));
        assert!(i.published.unwrap() > 1_700_000_000);
        assert!(i.summary_html.is_some());
        assert_ne!(p[0].id, p[1].id);
    }

    #[test]
    fn atom() {
        let p = parse(
            "https://daringfireball.net/feeds/main",
            include_bytes!("../tests/fixtures/daringfireball.atom"),
        )
        .unwrap();
        assert_eq!(p.len(), 5);
        let i = &p[0];
        assert!(i.guid.starts_with("tag:daringfireball.net"), "{}", i.guid);
        assert_eq!(i.author.as_deref(), Some("John Gruber"));
        assert!(i.link.as_deref().unwrap().starts_with("https://"));
        assert!(i.summary_html.as_deref().unwrap().contains("<p>"));
    }

    #[test]
    fn jsonfeed() {
        let p = parse(
            "https://daringfireball.net/feeds/json",
            include_bytes!("../tests/fixtures/daringfireball.json"),
        )
        .unwrap();
        assert_eq!(p.len(), 5);
        assert!(p.iter().all(|i| i.summary_html.is_some()));
        assert!(p.iter().all(|i| i.published.is_some()));
    }

    #[test]
    fn dateless_entry_has_no_published() {
        let rss = b"<rss version=\"2.0\"><channel><title>t</title><item><title>a</title><link>https://x/a</link></item></channel></rss>";
        let p = parse("https://x/feed", rss).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].published, None);
        assert_eq!(p[0].link.as_deref(), Some("https://x/a"));
    }

    #[test]
    fn broken_is_an_error() {
        assert!(parse("https://x", include_bytes!("../tests/fixtures/broken.rss")).is_err());
    }

    #[test]
    fn ids_are_stable_and_scoped_to_the_feed() {
        assert_eq!(item_id("https://a", "g"), item_id("https://a", "g"));
        assert_ne!(item_id("https://a", "g"), item_id("https://b", "g"));
        assert_eq!(item_id("https://a", "g").len(), 16);
    }
}
