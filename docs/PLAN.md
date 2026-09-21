# herdr-rss

A Herdr plugin that reads RSS, Atom, and JSON feeds in a terminal pane. Rust,
one binary, no other runtime. The README covers install, keys, the feeds
file, config, and the CLI. This file covers how it works and what is left.

## Opening

Two actions, one launcher script each. Both are idempotent within the
current tab: open, else focus, else close.

| Action | Placement |
| --- | --- |
| `open` | `split`, direction from `open_direction` |
| `open-full` | `zoomed` |

The launcher finds its own pane in `herdr pane list` by `label == "Feeds"`
and `cwd == HERDR_PLUGIN_ROOT`, within the focused pane's `tab_id`. The
decision comes from `herdr-rss --launch-decision`, which reads the pane list
on stdin, so it is unit tested. Focus is `pane zoom <id> --on` then `--off`;
Herdr has no focus-by-id. Close is `pane close <id>`. Any failure falls
through to open.

## Layout

```text
┌ Feeds ────────┬ Items ─────────────────────────┬ Article ───────────────┐
│ ▸ Tech    (12)│ ● Ars: The M5 Mac mini review  │ The M5 Mac mini review │
│   Ars      4  │ ● HN: Show HN: a tiny TUI      │ Ars Technica · 2h ago  │
│   HN       8  │   Lobsters: Rust 1.96 released │                        │
│ ▸ Games    3  │                                │ Apple's smallest ...   │
└───────────────┴────────────────────────────────┴────────────────────────┘
 12 unread · refreshed 4m ago          r refresh  o open  s star  Z zoom  ? help
```

Feeds 24 cells, items 40%, article the rest. Under 100 columns the pane
shows one column at a time; `Enter` goes right, `Esc` goes left. A split
beside a terminal is under 100 most of the time.

Feeds column: `All`, `Starred`, then the groups from feeds.txt. A group row
shows its unread total and folds with `Space`. A feed that failed its last
fetch is marked `!` and, while selected, puts its error in the status line.

Items: unread dot, star, feed name when the list spans feeds, title, age.
Newest first. `u` keeps unread only. `/` filters titles; the filter follows
you between feeds until cleared. `a` prompts for a URL and adds it under the
selected group on the worker thread. `d` asks `y/n`, then drops the feed and
its items.

Article: title, feed, author, date, item link, then the body through
`html2text` at the column width. Links in the body become `[text][n]` with a
`[n]: url` list at the end. The article is laid out row by row, and after
each frame every row that carries a URL is rewritten wrapped in OSC 8: the
item link, each `[text][n]`, each footnote and its wrapped continuation.
The terminal then holds the whole URL on every cell, so Ctrl+click works on
a URL that spans rows. ratatui has no hyperlink attribute; the rewrite
repeats the text it drew, so its buffer stays right. Relative footnote URLs
resolve against the item link. `1`..`9` opens link `n`.

`f` fetches the page on the worker thread and extracts the article with
`dom_smoothie`, a Rust port of Mozilla's readability.js: page HTML and URL
in, clean HTML out. The HTML is stored in `items.content_html` and takes the
place of the summary for rendering and for the numbered links. The header
gains `full text`. `show --full` does the same on the CLI.

Opening an item marks it read: `Enter`, click, or wheel into the article.
Moving the selection does not.

## Feeds file

`Name | URL` per line, `# Heading` for a group, `//` for a comment, and a
`#` line containing `|` is a commented-out feed. `add` and `remove` change
one line and leave the rest alone. `import` merges OPML the same way and
fetches nothing. `export` writes OPML with one level of groups. The file is
the source of truth; the store syncs to it on every refresh, add, remove,
and import.

`add` fetches the URL first. Not a feed: refused, nothing written. No name:
the feed's title.

## Fetch and store

- `ureq` with rustls. `If-None-Match` and `If-Modified-Since` from the last
  response; a 304 costs one small request. Ten redirects. Per-feed timeout
  from config. Eight fetches at a time on plain threads.
- `feed-rs` parses RSS 0.9x, 1.0, 2.0, Atom, JSON Feed.
- Item id: FNV-1a over the feed URL and the entry id, else the link, else
  title and date. Sixteen hex chars, stable across builds. A changed title
  on the same id updates in place. An item without a date keeps the time it
  was first seen, stepped back by its position so the feed's order holds.
- `rusqlite`, bundled, one file `HERDR_PLUGIN_STATE_DIR/rss.db`, WAL:

```sql
feeds (url PRIMARY KEY, name, grp, position, etag, last_modified,
       last_fetch, last_error)
items (id PRIMARY KEY, feed_url, guid, title, link, author, published,
       summary_html, content_text, read, starred, fetched_at)
```

- Refresh runs on a worker thread. While the pane is open it repeats every
  `refresh_minutes`, measured from the newest fetch in the store, so a pane
  opened long after the startup hook refreshes at once.
- After each refresh, unstarred items older than `keep_days` are deleted.
  Unknown items already older than that are never stored.
- The startup hook runs `refresh --detach`, which re-runs itself in its own
  process group with output in `refresh.log`, so Herdr's hook returns while
  the fetch runs.

## Code

```text
herdr-plugin.toml          # pane, three actions, startup hook
scripts/open.sh            # split launcher; open-full.sh wraps it zoomed
scripts/release.sh         # bump, check, tag, push, GitHub release
skills/herdr-rss/SKILL.md  # the CLI for agents
src/
  main.rs                  # dispatch; no args = reader
  cli.rs                   # refresh, list, show, mark, star, add, remove, import, export
  config.rs                # config.toml
  feeds.rs                 # feeds.txt parse, in-place add and remove
  opml.rs                  # OPML parse and render
  fetch.rs                 # ureq, conditional GET, thread pool
  parse.rs                 # feed-rs to Item
  store.rs                 # rusqlite
  html.rs                  # html2text
  readability.rs           # dom_smoothie
  launch.rs                # open / focus / close from pane list JSON
  herdr.rs                 # plugin env, HERDR_BIN_PATH calls
  time.rs                  # now, age, date
  tui/mod.rs               # terminal, event loop, tick, hyperlink repaint
  tui/app.rs               # state and actions, tested against an in-memory store
  tui/ui.rs                # columns, status line, help
  tui/article.rs           # article rows and their URL spans
  tui/keys.rs              # key to action, HELP table
tests/fixtures/            # pane_list.json; feed captures: rss2, atom, jsonfeed, broken
```

Dependencies: `ratatui`, `feed-rs`, `ureq`, `rusqlite`, `html2text`,
`dom_smoothie`, `quick-xml`, `url`, `chrono`, `arboard`, `serde`,
`serde_json`, `toml`. All I/O is blocking, on worker threads where the pane
would otherwise wait.

`make check` runs fmt, clippy, tests, `cargo audit`, and markdownlint; CI
runs the same on Ubuntu and macOS.

## Milestones

1. Done. Feeds file, fetch, parse, store, `refresh` and `list --json`.
2. Done. Reader pane, read state, stars, mouse, hyperlinks, numbered links.
3. Done. Manifest, launchers, startup hook, skill, release script.
4. Done. Auto refresh, failed-feed error in the status line.
5. Done. Search, add and delete from the pane, `remove`, OPML, comments in
   feeds.txt.
6. Done. `f` full-article fetch, `show --full`. Release 0.1.0.
7. Done. Launcher open, focus, and close paths run through the action in a
   live session.

## Not in v1

- Feed discovery from a page URL.
- Sync with a remote reader (Miniflux, FreshRSS).
- Images.
- Windows. The pane command is relative and Herdr cannot spawn it there.
