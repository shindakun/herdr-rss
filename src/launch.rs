//! The launcher's open / focus / close decision, computed from `herdr pane
//! list` JSON. A plugin pane shows up with `label` equal to the manifest pane
//! title and `cwd` equal to the plugin root; the decision is scoped to the
//! focused pane's tab.

use std::path::Path;

use serde::Deserialize;

/// The `[[panes]] title` in herdr-plugin.toml.
pub const PANE_TITLE: &str = "Feeds";

#[derive(Debug, Deserialize)]
struct Listing {
    result: PaneList,
}

#[derive(Debug, Deserialize)]
struct PaneList {
    panes: Vec<Pane>,
}

#[derive(Debug, Deserialize)]
struct Pane {
    pane_id: String,
    tab_id: String,
    #[serde(default)]
    focused: bool,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Open,
    Focus(String),
    Close(String),
}

impl std::fmt::Display for Decision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Decision::Open => f.write_str("OPEN"),
            Decision::Focus(id) => write!(f, "FOCUS {id}"),
            Decision::Close(id) => write!(f, "CLOSE {id}"),
        }
    }
}

pub fn decide(json: &str, plugin_root: &Path) -> Result<Decision, String> {
    let listing: Listing = serde_json::from_str(json).map_err(|e| format!("pane list: {e}"))?;
    let panes = listing.result.panes;
    let focused = panes.iter().find(|p| p.focused).ok_or("no focused pane")?;
    let root = canonical(plugin_root);
    let ours = panes.iter().find(|p| {
        p.tab_id == focused.tab_id
            && p.label.as_deref() == Some(PANE_TITLE)
            && p.cwd.as_deref().map(|c| canonical(Path::new(c)) == root) == Some(true)
    });
    let Some(pane) = ours else {
        return Ok(Decision::Open);
    };
    if !safe_pane_id(&pane.pane_id) {
        return Err(format!("unexpected pane id `{}`", pane.pane_id));
    }
    Ok(if pane.focused {
        Decision::Close(pane.pane_id.clone())
    } else {
        Decision::Focus(pane.pane_id.clone())
    })
}

fn canonical(p: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The digits Herdr encodes workspace, tab, and pane numbers in: base 32
/// over digits and uppercase letters, less I, L, O, and U
/// (`herdr/src/workspace.rs`, `PUBLIC_ID_ALPHABET`).
const ID_DIGITS: &[u8] = b"123456789ABCDEFGHJKMNPQRSTVWXYZ0";

/// A pane id is `w<n>:p<n>` in those digits, as in `wK:p4` or `wA:p1B`.
/// Anything else never reaches an argv.
fn safe_pane_id(id: &str) -> bool {
    let Some((w, p)) = id.split_once(':') else {
        return false;
    };
    let number = |s: &str, prefix: char| {
        s.strip_prefix(prefix)
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| ID_DIGITS.contains(&b)))
    };
    number(w, 'w') && number(p, 'p')
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/pane_list.json");
    const ROOT: &str = "/Users/steve/Code/herdr-rss";

    fn with_focus(id: &str) -> String {
        // Move `focused: true` to the given pane so one fixture covers every case.
        let mut v: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
        for p in v["result"]["panes"].as_array_mut().unwrap() {
            p["focused"] = serde_json::Value::Bool(p["pane_id"] == id);
        }
        v.to_string()
    }

    #[test]
    fn opens_when_tab_has_no_reader() {
        assert_eq!(
            decide(&with_focus("w5:p1"), Path::new(ROOT)).unwrap(),
            Decision::Open
        );
    }

    #[test]
    fn focuses_the_reader_in_this_tab() {
        assert_eq!(
            decide(&with_focus("w3:p1"), Path::new(ROOT)).unwrap(),
            Decision::Focus("w3:p19".into())
        );
    }

    #[test]
    fn closes_when_the_reader_is_focused() {
        assert_eq!(
            decide(&with_focus("w3:p19"), Path::new(ROOT)).unwrap(),
            Decision::Close("w3:p19".into())
        );
    }

    #[test]
    fn ignores_other_plugins_panes_with_the_same_label() {
        assert_eq!(
            decide(&with_focus("w3:p1"), Path::new("/somewhere/else")).unwrap(),
            Decision::Open
        );
    }

    #[test]
    fn no_focused_pane_is_an_error() {
        assert!(decide(&with_focus("w9:p9"), Path::new(ROOT)).is_err());
    }

    #[test]
    fn pane_id_guard() {
        assert!(safe_pane_id("w3:p19"));
        assert!(
            safe_pane_id("wK:p4"),
            "past the ninth workspace ids use letters"
        );
        assert!(
            safe_pane_id("wA:p1B"),
            "past the 32nd pane they get two digits"
        );
        assert!(!safe_pane_id("w3:p"));
        assert!(!safe_pane_id("wI:p1"), "I, L, O, and U are not id digits");
        assert!(!safe_pane_id("wa:p1"), "digits are uppercase");
        assert!(!safe_pane_id("--rm"));
        assert!(!safe_pane_id("w3:p19 --on"));
    }

    #[test]
    fn display() {
        assert_eq!(Decision::Open.to_string(), "OPEN");
        assert_eq!(Decision::Focus("w1:p2".into()).to_string(), "FOCUS w1:p2");
    }
}
