//! The article column laid out row by row, so every screen row is known and
//! the rows that carry a URL can be painted as OSC 8 hyperlinks after the
//! frame. That is what makes a URL wrapped across rows clickable: the
//! terminal keeps the whole URL on every cell.

use ratatui::style::{Color, Modifier, Style};

/// One screen row of the article.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArticleRow {
    pub text: String,
    pub style: RowStyle,
    /// Hyperlinked spans: start column, length in chars, URL.
    pub links: Vec<(u16, u16, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowStyle {
    Title,
    Meta,
    Link,
    Body,
}

impl RowStyle {
    pub fn style(self) -> Style {
        match self {
            RowStyle::Title => Style::default().add_modifier(Modifier::BOLD),
            RowStyle::Meta => Style::default().fg(Color::DarkGray),
            RowStyle::Link => Style::default().fg(Color::Blue),
            RowStyle::Body => Style::default(),
        }
    }
}

/// Lays out the header and the html2text body at `width`. `links` are the
/// footnote URLs in number order, from a wide render so none is split.
pub fn layout(
    title: &str,
    meta: &str,
    link: Option<&str>,
    body: &str,
    links: &[String],
    width: usize,
) -> Vec<ArticleRow> {
    let width = width.max(1);
    let mut rows = Vec::new();
    for t in wrap(title, width) {
        rows.push(ArticleRow {
            text: t,
            style: RowStyle::Title,
            links: vec![],
        });
    }
    for t in wrap(meta, width) {
        rows.push(ArticleRow {
            text: t,
            style: RowStyle::Meta,
            links: vec![],
        });
    }
    if let Some(url) = link {
        for t in hard_wrap(url, width) {
            let len = t.chars().count() as u16;
            rows.push(ArticleRow {
                text: t,
                style: RowStyle::Link,
                links: vec![(0, len, url.to_string())],
            });
        }
    }
    rows.push(ArticleRow {
        text: String::new(),
        style: RowStyle::Body,
        links: vec![],
    });

    // Footnotes: `[n]: url`, then continuation rows when html2text wrapped
    // the URL. Anything after the first footnote is footnotes.
    let mut footnote: Option<String> = None;
    for line in body.lines() {
        for text in hard_wrap(line, width) {
            let mut links_here = Vec::new();
            if let Some(n) = footnote_number(&text) {
                footnote = n.checked_sub(1).and_then(|i| links.get(i)).cloned();
                if let Some(url) = &footnote {
                    links_here.push((0, text.chars().count() as u16, url.clone()));
                }
            } else if let Some(url) = &footnote {
                if text.trim().is_empty() {
                    footnote = None;
                } else {
                    links_here.push((0, text.chars().count() as u16, url.clone()));
                }
            } else {
                links_here = ref_spans(&text, links);
            }
            rows.push(ArticleRow {
                text,
                style: RowStyle::Body,
                links: links_here,
            });
        }
    }
    rows
}

/// `[n]: ` at the start of a line.
fn footnote_number(line: &str) -> Option<usize> {
    let rest = line.strip_prefix('[')?;
    let (n, rest) = rest.split_once("]: ")?;
    (!rest.is_empty()).then(|| n.parse().ok()).flatten()
}

/// `[text][n]` references inside a body row, each hyperlinked to link n.
fn ref_spans(line: &str, links: &[String]) -> Vec<(u16, u16, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 2 < chars.len() {
        if chars[i] == ']' && chars[i + 1] == '[' {
            let mut j = i + 2;
            let mut n = 0usize;
            let mut digits = 0;
            while j < chars.len() && chars[j].is_ascii_digit() {
                n = n * 10 + chars[j].to_digit(10).unwrap_or(0) as usize;
                j += 1;
                digits += 1;
            }
            if digits > 0 && j < chars.len() && chars[j] == ']' {
                if let Some(start) = chars[..i].iter().rposition(|c| *c == '[') {
                    if let Some(url) = n.checked_sub(1).and_then(|k| links.get(k)) {
                        out.push((start as u16, (j + 1 - start) as u16, url.clone()));
                    }
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Word wrap; a word longer than the width is split.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut rows = Vec::new();
    let mut cur = String::new();
    let mut cur_len = 0;
    for word in text.split_whitespace() {
        let wl = word.chars().count();
        if cur_len > 0 && cur_len + 1 + wl > width {
            rows.push(std::mem::take(&mut cur));
            cur_len = 0;
        }
        if wl > width {
            for piece in hard_wrap(word, width) {
                if cur_len > 0 {
                    rows.push(std::mem::take(&mut cur));
                }
                cur_len = piece.chars().count();
                cur = piece;
            }
            continue;
        }
        if cur_len > 0 {
            cur.push(' ');
            cur_len += 1;
        }
        cur.push_str(word);
        cur_len += wl;
    }
    if !cur.is_empty() || rows.is_empty() {
        rows.push(cur);
    }
    rows
}

/// Splits every `width` chars, no word boundaries.
fn hard_wrap(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars
        .chunks(width.max(1))
        .map(|c| c.iter().collect())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_words_and_splits_long_ones() {
        assert_eq!(wrap("a bb ccc", 5), ["a bb", "ccc"]);
        assert_eq!(wrap("abcdefgh ij", 4), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap("", 4), [""]);
        assert_eq!(hard_wrap("abcde", 2), ["ab", "cd", "e"]);
    }

    #[test]
    fn footnotes_and_refs_carry_their_urls_across_wrapped_rows() {
        let links = vec![
            "https://example.com/a-very-long-path-that-wraps".to_string(),
            "https://example.com/b".to_string(),
        ];
        let body = "See [this][1] and [that][2].\n\n[1]: https://example.com/a-very-lo\nng-path-that-wraps\n[2]: https://example.com/b";
        let rows = layout(
            "T",
            "F · 2026",
            Some("https://example.com/item-x"),
            body,
            &links,
            34,
        );
        let texts: Vec<&str> = rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "T",
                "F · 2026",
                "https://example.com/item-x",
                "",
                "See [this][1] and [that][2].",
                "",
                "[1]: https://example.com/a-very-lo",
                "ng-path-that-wraps",
                "[2]: https://example.com/b",
            ]
        );
        assert_eq!(
            rows[2].links,
            [(0, 26, "https://example.com/item-x".to_string())]
        );
        assert_eq!(
            rows[4].links,
            [(4, 9, links[0].clone()), (18, 9, links[1].clone())]
        );
        assert_eq!(rows[6].links, [(0, 34, links[0].clone())]);
        assert_eq!(
            rows[7].links,
            [(0, 18, links[0].clone())],
            "continuation row keeps the full URL"
        );
        assert_eq!(rows[8].links, [(0, 26, links[1].clone())]);
        assert_eq!(rows[2].style, RowStyle::Link);
    }

    #[test]
    fn long_header_link_wraps_with_the_url_on_every_row() {
        let url = "https://example.com/abcdefghijklmnopqrstuvwxyz";
        let rows = layout("T", "M", Some(url), "", &[], 20);
        let link_rows: Vec<&ArticleRow> =
            rows.iter().filter(|r| r.style == RowStyle::Link).collect();
        assert_eq!(link_rows.len(), 3);
        assert!(link_rows.iter().all(|r| r.links[0].2 == url));
    }
}
