# Changelog

## Unreleased

- Reader pane: feeds, items, and article columns; one at a time under 100 columns. Groups fold. Read state, stars, unread filter, title search, next and previous unread. Add and delete feeds from the pane.
- Article links are terminal hyperlinks, wrapped or not. `1`..`9` open numbered links. `o` opens the item, `y` copies its link.
- Mouse: click to focus and select, click again to open or fold, wheel to move or scroll.
- Refresh on a worker thread, on `r` / `R`, and every `refresh_minutes`. A failed feed is marked and its error shown while selected.
- Fetch with conditional GETs, eight at a time; RSS, Atom, and JSON Feed through `feed-rs`; SQLite in the state dir. Items past `keep_days` are dropped.
- feeds.txt with groups, `//` comments, and commented-out feeds. Adds and removes edit one line.
- CLI: `refresh`, `list`, `show`, `mark`, `star`, `add`, `remove`, `import`, `export`, all with `--json`.
- Split and zoomed launchers that open, focus, or close on repeat. Startup hook warms the cache.
