# Changelog

## Unreleased

- Mouse: click focuses and selects, a second click opens an item or folds a group, the wheel moves the list or scrolls the article.
- `1`..`9` open the article's numbered links. A URL wrapped across rows cannot be clicked in the terminal; this is the way to follow it.
- Reader pane: feeds, items, and article columns, one at a time under 100 columns. Groups fold. Open marks read; `m`, `M`, `s`, `u`, `n`, `p`, `o`, `y`, `Z`, `r`, `R`, `?`. Refresh runs on a worker thread.
- Fetch, parse, and store: conditional GETs, eight in flight, `feed-rs` for RSS, Atom, and JSON Feed, SQLite in the state dir. Dateless items keep the time they were first seen. Items older than `keep_days` are skipped on insert and pruned on refresh.
- CLI: `refresh` (with `--detach` for the startup hook), `list`, `show`, `mark`, `star`, all with `--json`.
- Scaffold: manifest, launchers for split and zoomed placement, `feeds.txt` parser, `config.toml`, `add` subcommand, house tooling and CI.
