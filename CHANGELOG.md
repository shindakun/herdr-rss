# Changelog

## Unreleased

- The launcher's pane id check accepted hex; Herdr numbers ids in base 32 over `123456789ABCDEFGHJKMNPQRSTVWXYZ0`, so a pane in workspace `wK` was rejected and every keypress opened a new pane. 0.1.1 fixed this for `wA` through `wF` only.
- Focus and close go through `herdr plugin pane focus` and `plugin pane close` instead of a `pane zoom --on` / `--off` cycle.

## 0.1.1 (2026-09-20)

- Launcher: pane ids are hex (`wA:p3`); the id check rejected them, so every press opened a new pane instead of focusing or closing.

## 0.1.0 (2026-09-20)

First release.

- Reader pane: feeds, items, and article columns; one at a time under 100 columns. Groups fold. Read state, stars, unread filter, title search, next and previous unread. Add and delete feeds from the pane.
- Article links are terminal hyperlinks, wrapped or not. `1`..`9` open numbered links. `o` opens the item, `y` copies its link. `f` fetches the full article from its page.
- Mouse: click to focus and select, click again to open or fold, wheel to move or scroll.
- Refresh on a worker thread, on `r` / `R`, and every `refresh_minutes`. A failed feed is marked and its error shown while selected.
- Fetch with conditional GETs, eight at a time; RSS, Atom, and JSON Feed through `feed-rs`; SQLite in the state dir. Items past `keep_days` are dropped.
- feeds.txt with groups, `//` comments, and commented-out feeds. Adds and removes edit one line.
- CLI: `refresh`, `list`, `show` (with `--full`), `mark`, `star`, `add`, `remove`, `import`, `export`, all with `--json`.
- Split and zoomed launchers that open, focus, or close on repeat. Startup hook warms the cache.
