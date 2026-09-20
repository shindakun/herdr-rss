# Changelog

## Unreleased

- `/` searches titles in the current list. `a` adds a feed by URL under the selected group; `d` deletes the selected feed after `y`.
- CLI: `add` fetches the URL first and refuses one that is not a feed, naming it from the feed title; `remove`; `import FILE.opml [--replace]`; `export`.
- feeds.txt: `//` comments; a `#` line containing `|` is a commented-out feed. Adds and removes edit the file in place.
- Auto refresh every `refresh_minutes` while the pane is open. A failed feed's error shows in the status line while selected. Wheel or click into the article marks the item read.
- Mouse: click focuses and selects, a second click opens an item or folds a group, the wheel focuses the column under the pointer and moves its list or scrolls the article.
- Article links are OSC 8 terminal hyperlinks: the item link, `[text][n]` references, and footnotes, including a footnote wrapped across rows. Relative footnote URLs resolve against the item link. `1`..`9` open link `n` from the keyboard.
- Reader pane: feeds, items, and article columns, one at a time under 100 columns. Groups fold. Open marks read; `m`, `M`, `s`, `u`, `n`, `p`, `o`, `y`, `Z`, `r`, `R`, `?`. Refresh runs on a worker thread.
- Fetch, parse, and store: conditional GETs, eight in flight, `feed-rs` for RSS, Atom, and JSON Feed, SQLite in the state dir. Dateless items keep the time they were first seen. Items older than `keep_days` are skipped on insert and pruned on refresh.
- CLI: `refresh` (with `--detach` for the startup hook), `list`, `show`, `mark`, `star`, all with `--json`.
- Scaffold: manifest, launchers for split and zoomed placement, `feeds.txt` parser, `config.toml`, `add` subcommand, house tooling and CI.
