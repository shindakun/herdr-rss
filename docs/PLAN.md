# herdr-rss

A Herdr plugin that reads RSS, Atom, and JSON feeds in a terminal pane. Rust,
one binary, no other runtime. Opens beside your work as a split, or fills the
terminal as a zoomed pane. Feeds, items, and article text, all keyboard driven.

## What it does

1. You press a key. The launcher opens the reader in a split next to the
   focused pane, or zoomed over it. Press the key again and it focuses. Press
   it a third time and it closes.
2. The reader shows three columns: feeds, items in the selected feed, the
   selected item's text. A narrow split shows one column at a time.
3. `r` refreshes. Fetches run in parallel with conditional GETs. Read state,
   stars, and the item cache live in SQLite under the plugin state dir.
4. `o` opens the item in the browser. `y` copies the link. `Z` toggles zoom
   through `herdr pane zoom`.
5. The same binary is a CLI. An agent can list unread items as JSON, mark
   them read, or add a feed, without the pane open.

## Opening

Two actions, one launcher script each. Both are idempotent and scoped to the
current tab: open, else focus, else close.

| Action | Placement | Bind |
| --- | --- | --- |
| `open` | `split`, direction from config (`right` default, `down`) | `prefix+n` |
| `open-full` | `zoomed` | `prefix+shift+n` |

The launcher finds its own pane in `herdr pane list` by `label == "Feeds"`
and `cwd == HERDR_PLUGIN_ROOT`, filtered to the focused pane's `tab_id`.
Focus is `pane zoom <id> --on` then `--off`, since Herdr has no focus-by-id.
Close is `pane close <id>`. Any parse failure falls through to open.

```toml
# <config dir>/config.toml
open_direction = "right"   # or "down"
refresh_minutes = 30       # 0 disables auto refresh while the pane is open
fetch_timeout_secs = 15
keep_days = 30             # items older than this and unstarred are pruned
browser = "open"           # command that receives the URL; default per OS
```

## Layout

```text
┌ Feeds ────────┬ Items ─────────────────────────┬ Article ───────────────┐
│ ▸ Tech    (12)│ ● Ars: The M5 Mac mini review  │ The M5 Mac mini review │
│   Ars      4  │ ● HN: Show HN: a tiny TUI      │ Ars Technica · 2h ago  │
│   HN       8  │   Lobsters: Rust 1.96 released │                        │
│ ▸ Games    3  │                                │ Apple's smallest ...   │
└───────────────┴────────────────────────────────┴────────────────────────┘
 12 unread · refreshed 4m ago · r refresh  o open  s star  Z zoom  ? help
```

Widths: feeds 22 cells, items 40%, article the rest. Under 100 columns the
pane shows one column at a time. `Enter` goes right, `Esc` goes left. Under
100 columns is the normal split case, so this path gets tested first.

Feeds column groups by heading from the feed list. A group row shows its
unread total and folds with `Space`. The virtual feeds `All` and `Starred`
sit at the top.

Item rows: unread dot, feed short name, title, age. Sort is newest first.
`u` filters to unread. `/` searches titles in the current list.

Article: title, feed, author, date, then the item body as text. HTML from
the feed body goes through `html2text` at the column width. Links in the body
are numbered `[1]` and listed at the end; `1`..`9` opens that link. `f`
fetches the page and runs Readability extraction for feeds that ship only a
summary.

Extraction is `dom_smoothie`, a pure-Rust port of Mozilla's readability.js.
No Node, no JS runtime. The fetched HTML and the page URL go in; title,
byline, and cleaned article HTML come out; `html2text` renders that.

## Keys

| Key | Does |
| --- | --- |
| `j` `k` / arrows | Move in the focused column |
| `h` `l` / `Tab` | Move between columns |
| `Enter` / `Esc` | Open item / back (narrow mode) |
| `r` / `R` | Refresh selected feed / all feeds |
| `n` / `p` | Next / previous unread item |
| `m` / `M` | Toggle read on item / mark feed read |
| `s` | Toggle star |
| `u` | Show unread only |
| `/` | Search titles |
| `o` | Open item in browser |
| `y` | Copy item link |
| `f` | Fetch full article text |
| `a` | Add feed by URL |
| `d` | Delete feed |
| `Z` | Toggle zoom |
| `?` | Help |
| `q` | Quit pane |

## Feeds file

`<config dir>/feeds.txt`. One feed per line, `Name | URL`. A `# Heading`
line starts a group. Same format as the Picked source list in `feedthing`,
so that file drops in unchanged.

```text
# Tech
Ars Technica | https://feeds.arstechnica.com/arstechnica/index
Hacker News | https://hnrss.org/frontpage

# Games
Rock Paper Shotgun | https://www.rockpapershotgun.com/feed
```

`herdr-rss import feeds.opml` converts OPML to this file. `herdr-rss export`
writes OPML. `a` in the pane appends a line. The file is the source of truth;
SQLite mirrors it on every start and on `a` / `d`.

## Fetch and store

- HTTP: `ureq` with rustls. `If-None-Match` and `If-Modified-Since` from the
  last response. A 304 costs nothing. Redirects followed, 10 max. Per-feed
  timeout from config. Eight fetches at a time on a thread pool.
- Parse: `feed-rs`. Covers RSS 0.9x, 1.0, 2.0, Atom, JSON Feed.
- Item identity: feed URL plus `guid`, else `link`, else hash of title and
  date. A changed title on the same guid updates in place. An item the feed
  does not date keeps the time it was first seen, stepped back by its
  position so the feed's order holds.
- Store: `rusqlite` with the bundled feature, one file
  `HERDR_PLUGIN_STATE_DIR/rss.db`, WAL mode. Two tables:

```sql
feeds (url PRIMARY KEY, name, grp, etag, last_modified, last_fetch,
       last_error, position)
items (id PRIMARY KEY, feed_url, guid, title, link, author, published,
       summary_html, content_text, read, starred, fetched_at)
```

- Refresh runs on a worker thread. The UI never blocks. A feed that fails
  shows `!` in the feeds column and its error in the status line.
- Prune: after each refresh, delete unstarred items older than `keep_days`.
  Unknown items already older than that are not stored, so a refresh does
  not churn.
- Startup hook: `herdr-rss refresh --detach` warms the cache when Herdr
  starts, so the first open is not a wait.

## CLI

Every subcommand takes `--json`. Agents use these.

| Command | Does |
| --- | --- |
| `herdr-rss refresh [--feed URL] [--detach]` | Fetch and store |
| `herdr-rss list [--unread] [--starred] [--feed URL] [--limit N]` | Print items |
| `herdr-rss show <id> [--width N]` | Print one item's text |
| `herdr-rss mark <id>... [--unread]` | Set read state |
| `herdr-rss star <id>...` | Toggle star |
| `herdr-rss add <url> [--name N] [--group G]` | Append to feeds.txt |
| `herdr-rss import <file.opml>` / `export` | OPML in and out |
| `herdr-rss --launch-decision` | Reads `pane list` on stdin, prints `OPEN`, `FOCUS <id>`, or `CLOSE <id>` |

A `skills/herdr-rss/SKILL.md` teaches an agent the list and show commands.

## Code layout

```text
herdr-rss/
  herdr-plugin.toml
  Cargo.toml
  Makefile                 # check: fmt clippy test audit md-lint
  .markdownlint-cli2.jsonc
  .pre-commit-config.yaml
  scripts/open.sh          # split launcher
  scripts/open-full.sh     # zoomed launcher
  scripts/release.sh
  skills/herdr-rss/SKILL.md
  src/
    main.rs                # subcommand dispatch; no args = TUI
    cli.rs                 # list, show, mark, star, add, import, export
    config.rs              # config.toml
    feeds.rs               # feeds.txt parse and write, OPML
    fetch.rs               # ureq, conditional GET, thread pool
    parse.rs               # feed-rs to Item
    store.rs               # rusqlite schema, queries, prune
    html.rs                # html2text, link numbering
    readability.rs         # full-article extraction via dom_smoothie
    launch.rs              # launch decision from pane list JSON
    herdr.rs               # HERDR_BIN_PATH wrapper: pane zoom, pane close
    tui/
      mod.rs               # event loop, refresh worker channel
      app.rs               # state: selection, column, filter, mode
      feeds.rs             # feeds column
      items.rs             # items column
      article.rs           # article column
      help.rs
      keys.rs
  tests/fixtures/          # pane_list.json; feed captures: rss2, atom, jsonfeed, broken
  .github/workflows/ci.yml # fmt, clippy, test, release build on ubuntu and macos; audit; md lint
```

Unit tests live beside the code they test.

Dependencies: `ratatui`, `crossterm`, `feed-rs`, `ureq` (rustls), `rusqlite`
(bundled), `html2text`, `dom_smoothie`, `serde`, `serde_json`, `toml`,
`arboard` for the clipboard. All I/O is blocking on worker threads.

## Manifest

```toml
id = "shindakun.herdr-rss"
name = "Herdr RSS"
version = "0.1.0"
min_herdr_version = "0.9.0"
description = "Read RSS, Atom, and JSON feeds in a split or zoomed pane."
platforms = ["linux", "macos"]

[[build]]
command = ["cargo", "build", "--release"]

[[panes]]
id = "reader"
title = "Feeds"
placement = "split"
command = ["./target/release/herdr-rss"]

[[actions]]
id = "open"
title = "Open feeds"
command = ["bash", "scripts/open.sh"]

[[actions]]
id = "open-full"
title = "Open feeds full screen"
command = ["bash", "scripts/open-full.sh"]

[[actions]]
id = "refresh"
title = "Refresh feeds"
command = ["./target/release/herdr-rss", "refresh"]

[[startup]]
command = ["./target/release/herdr-rss", "refresh", "--detach"]
```

Keybinding in `~/.config/herdr/config.toml`:

```toml
[[keys.command]]
key = "prefix+n"
type = "plugin_action"
command = "shindakun.herdr-rss.open"
description = "open feeds"

[[keys.command]]
key = "prefix+shift+n"
type = "plugin_action"
command = "shindakun.herdr-rss.open-full"
description = "open feeds full screen"
```

## Milestones

1. Feeds file, fetch, parse, store. `herdr-rss refresh` and `list --json`
   work against the fixtures and against the real feeds.txt.
2. TUI with three columns, read state, stars, `o`, `y`. Wide and narrow
   layouts.
3. Launchers, manifest, `pane zoom`. Link the plugin, bind the keys, open it
   from a real Herdr session in both placements.
4. Background refresh worker, startup hook, prune, error display.
5. Search, unread filter, `a` / `d`, OPML import and export.
6. Full-article fetch, numbered links, skill file, release script.

## Not in v1

- Feed discovery from a page URL.
- Sync with a remote reader (Miniflux, FreshRSS).
- Images.
- Windows. The pane command is relative and Herdr cannot spawn it there.
