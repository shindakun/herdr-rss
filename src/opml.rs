//! OPML in and out. Nested `outline` elements are groups; an `outline` with
//! `xmlUrl` is a feed. One level of grouping survives; deeper levels flatten
//! to their innermost name.

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::{Reader, Writer};

use crate::feeds::Feed;

pub fn parse(xml: &str) -> Result<Vec<Feed>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut feeds = Vec::new();
    let mut groups: Vec<String> = Vec::new();
    loop {
        match reader.read_event().map_err(|e| format!("opml: {e}"))? {
            Event::Start(e) if e.local_name().as_ref() == b"outline" => match outline(&e)? {
                Some(f) => {
                    feeds.push(with_group(f, &groups));
                    groups.push(String::new());
                }
                None => groups.push(text(&e)?),
            },
            Event::Empty(e) if e.local_name().as_ref() == b"outline" => {
                if let Some(f) = outline(&e)? {
                    feeds.push(with_group(f, &groups));
                }
            }
            Event::End(e) if e.local_name().as_ref() == b"outline" => {
                groups.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(feeds)
}

fn with_group(mut f: Feed, groups: &[String]) -> Feed {
    f.group = groups
        .iter()
        .rev()
        .find(|g| !g.is_empty())
        .cloned()
        .unwrap_or_default();
    f
}

fn attr(e: &BytesStart, name: &str) -> Result<Option<String>, String> {
    for a in e.attributes() {
        let a = a.map_err(|e| format!("opml: {e}"))?;
        if a.key.as_ref() == name.as_bytes() {
            let v = a
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|e| format!("opml: {e}"))?;
            return Ok(Some(v.into_owned()));
        }
    }
    Ok(None)
}

fn text(e: &BytesStart) -> Result<String, String> {
    Ok(attr(e, "text")?
        .or(attr(e, "title")?)
        .unwrap_or_default()
        .trim()
        .to_string())
}

/// A feed when the outline has an `xmlUrl`, else None (a group).
fn outline(e: &BytesStart) -> Result<Option<Feed>, String> {
    let Some(url) = attr(e, "xmlUrl")? else {
        return Ok(None);
    };
    let url = url.trim().to_string();
    if url.is_empty() {
        return Ok(None);
    }
    let name = text(e)?;
    Ok(Some(Feed {
        name: if name.is_empty() { url.clone() } else { name },
        url,
        group: String::new(),
    }))
}

pub fn render(feeds: &[Feed]) -> String {
    let mut w = Writer::new_with_indent(Vec::new(), b' ', 2);
    let open = |w: &mut Writer<Vec<u8>>, name: &str, attrs: &[(&str, &str)]| {
        let mut e = BytesStart::new(name);
        for (k, v) in attrs {
            e.push_attribute((*k, *v));
        }
        w.write_event(Event::Start(e))
    };
    let _ = w.write_event(Event::Decl(quick_xml::events::BytesDecl::new(
        "1.0",
        Some("UTF-8"),
        None,
    )));
    let _ = open(&mut w, "opml", &[("version", "2.0")]);
    let _ = open(&mut w, "head", &[]);
    let _ = open(&mut w, "title", &[]);
    let _ = w.write_event(Event::Text(quick_xml::events::BytesText::new(
        "herdr-rss feeds",
    )));
    let _ = w.write_event(Event::End(BytesEnd::new("title")));
    let _ = w.write_event(Event::End(BytesEnd::new("head")));
    let _ = open(&mut w, "body", &[]);
    let mut current: Option<&str> = None;
    for f in feeds {
        if current != Some(f.group.as_str()) {
            if current.is_some_and(|g| !g.is_empty()) {
                let _ = w.write_event(Event::End(BytesEnd::new("outline")));
            }
            if !f.group.is_empty() {
                let _ = open(&mut w, "outline", &[("text", &f.group)]);
            }
            current = Some(&f.group);
        }
        let mut e = BytesStart::new("outline");
        e.push_attribute(("type", "rss"));
        e.push_attribute(("text", f.name.as_str()));
        e.push_attribute(("xmlUrl", f.url.as_str()));
        let _ = w.write_event(Event::Empty(e));
    }
    if current.is_some_and(|g| !g.is_empty()) {
        let _ = w.write_event(Event::End(BytesEnd::new("outline")));
    }
    let _ = w.write_event(Event::End(BytesEnd::new("body")));
    let _ = w.write_event(Event::End(BytesEnd::new("opml")));
    let mut out = String::from_utf8(w.into_inner()).unwrap_or_default();
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<opml version="1.0">
  <head><title>Subs</title></head>
  <body>
    <outline text="Tech">
      <outline type="rss" text="Ars &amp; Co" xmlUrl="https://feeds.arstechnica.com/arstechnica/index" htmlUrl="https://arstechnica.com"/>
      <outline title="Untitled group">
        <outline type="rss" text="Deep" xmlUrl="https://deep.example/feed"/>
      </outline>
    </outline>
    <outline type="rss" xmlUrl="https://noname.example/feed"/>
    <outline type="rss" text="Lobsters" xmlUrl="https://lobste.rs/rss"></outline>
  </body>
</opml>"#;

    #[test]
    fn parses_groups_names_and_nesting() {
        let f = parse(SAMPLE).unwrap();
        let got: Vec<(&str, &str, &str)> = f
            .iter()
            .map(|f| (f.name.as_str(), f.url.as_str(), f.group.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                (
                    "Ars & Co",
                    "https://feeds.arstechnica.com/arstechnica/index",
                    "Tech"
                ),
                ("Deep", "https://deep.example/feed", "Untitled group"),
                (
                    "https://noname.example/feed",
                    "https://noname.example/feed",
                    ""
                ),
                ("Lobsters", "https://lobste.rs/rss", ""),
            ]
        );
    }

    #[test]
    fn render_round_trips_and_escapes() {
        let feeds = vec![
            Feed {
                name: "A & B".into(),
                url: "https://a.example/feed?x=1&y=2".into(),
                group: "Tech".into(),
            },
            Feed {
                name: "C".into(),
                url: "https://c.example/feed".into(),
                group: "".into(),
            },
        ];
        let xml = render(&feeds);
        assert!(xml.starts_with("<?xml"), "{xml}");
        assert!(xml.contains(r#"<outline text="Tech">"#), "{xml}");
        assert!(xml.contains("A &amp; B"), "{xml}");
        assert_eq!(parse(&xml).unwrap(), feeds);
    }

    #[test]
    fn bad_xml_is_an_error() {
        assert!(
            parse("<opml><body><outline").is_err()
                || parse("<opml><body><outline").unwrap().is_empty()
        );
        assert!(parse("<opml><body><outline text=\"x\"></body></opml>").is_err());
    }
}
