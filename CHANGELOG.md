# Changelog

## Unreleased

- Fetch, parse, and store: conditional GETs, eight in flight, `feed-rs` for RSS, Atom, and JSON Feed, SQLite in the state dir. Dateless items keep the time they were first seen. Items older than `keep_days` are skipped on insert and pruned on refresh.
- CLI: `refresh` (with `--detach` for the startup hook), `list`, `show`, `mark`, `star`, all with `--json`.
- Scaffold: manifest, launchers for split and zoomed placement, `feeds.txt` parser, `config.toml`, `add` subcommand, house tooling and CI.
