//! Item body HTML to terminal text with `html2text`, which numbers links
//! `[text][n]` and lists them at the end.

pub fn to_text(html: &str, width: usize) -> String {
    html2text::from_read(html.as_bytes(), width.max(20))
        .unwrap_or_else(|_| strip_tags(html))
        .trim_end()
        .to_string()
}

/// Fallback when the HTML will not parse: drop tags, keep text.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_paragraphs_and_wraps() {
        let t = to_text("<p>Hello <b>world</b>.</p><p>Second paragraph that is long enough to wrap around the width.</p>", 30);
        assert!(t.starts_with("Hello **world**."), "{t}");
        assert!(t.lines().all(|l| l.chars().count() <= 30), "{t}");
        assert!(t.contains("\n\n"), "{t}");
    }

    #[test]
    fn strip_tags_keeps_text() {
        assert_eq!(strip_tags("a <i>b</i> c"), "a b c");
    }
}
