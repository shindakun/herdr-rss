//! Full-article extraction with `dom_smoothie`, a Rust port of Mozilla's
//! readability.js. Page HTML and URL in; title, byline, clean HTML out.

use dom_smoothie::{Config, Readability};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Article {
    pub title: String,
    pub byline: Option<String>,
    /// Cleaned article HTML, links made absolute against the page URL.
    pub html: String,
    /// Characters of text in the article.
    pub length: usize,
}

pub fn extract(page_html: &str, url: &str) -> Result<Article, String> {
    let cfg = Config {
        max_elements_to_parse: 50_000,
        ..Default::default()
    };
    let mut r = Readability::new(page_html, Some(url), Some(cfg)).map_err(|e| e.to_string())?;
    let a = r.parse().map_err(|e| e.to_string())?;
    if a.length == 0 {
        return Err("no article text found".into());
    }
    Ok(Article {
        title: a.title.trim().to_string(),
        byline: a
            .byline
            .map(|b| b.trim().to_string())
            .filter(|b| !b.is_empty()),
        html: a.content.to_string(),
        length: a.length,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_a_real_page() {
        let a = extract(
            include_str!("../tests/fixtures/article.html"),
            "https://daringfireball.net/linked/2026/09/19/stalman-iphone-18-pro",
        )
        .unwrap();
        assert!(a.title.contains("iPhone 18 Pro"), "{}", a.title);
        assert!(a.html.contains("Stalman"), "{}", a.html);
        assert!(!a.html.contains("<script"), "scripts stripped");
        assert!(a.length > 100);
    }

    #[test]
    fn empty_page_is_an_error() {
        assert!(extract("<html><body></body></html>", "https://x").is_err());
    }
}
