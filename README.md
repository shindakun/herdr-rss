# herdr-rss

A [Herdr](https://herdr.dev) plugin that reads RSS, Atom, and JSON feeds in a terminal pane. Rust, one binary, no other runtime. Opens beside your work as a split or fills the terminal as a zoomed pane.

Status: the reader, fetcher, store, and CLI work. Search, OPML, and full-article fetch are next. The design is in [docs/PLAN.md](docs/PLAN.md).

## Install

```sh
herdr plugin install shindakun/herdr-rss
```

Needs `cargo`; the install step builds the binary. Linux and macOS.

For local development, link the checkout instead:

```sh
cargo build --release
herdr plugin link /path/to/herdr-rss
```

## Feeds

`$(herdr plugin config-dir shindakun.herdr-rss)/feeds.txt`. One feed per line, `Name | URL`. A `# Heading` line starts a group.

```text
# Tech
Ars Technica | https://feeds.arstechnica.com/arstechnica/index
Hacker News | https://hnrss.org/frontpage

# Games
Rock Paper Shotgun | https://www.rockpapershotgun.com/feed
```

`herdr-rss add URL --name N --group G` appends a line. `import FILE.opml` and `export` convert to and from OPML.

## CLI

The plugin binary is also a CLI over the same store, for scripts and agents. Find it at `<plugin root>/target/release/herdr-rss` (`herdr plugin list` prints the root). It needs `HERDR_PLUGIN_CONFIG_DIR` and `HERDR_PLUGIN_STATE_DIR` set the way Herdr sets them for plugin commands; `herdr plugin config-dir shindakun.herdr-rss` prints the first, and the state dir is `~/.local/state/herdr/plugins/shindakun.herdr-rss`. Every subcommand takes `--json`.

| Command | Does |
| --- | --- |
| `refresh [--feed URL] [--detach]` | Fetch and store; `--detach` returns at once and logs to `refresh.log` in the state dir |
| `list [--unread] [--starred] [--feed URL] [--limit N]` | Items, newest first; 50 by default |
| `show ID [--width N]` | One item as text |
| `mark ID... [--unread]` | Set read state |
| `star ID...` | Toggle the star |
| `add URL [--name N] [--group G]` | Append to feeds.txt |

A refresh sends `If-None-Match` and `If-Modified-Since`, so a feed that has not changed costs one small request. Eight fetches run at a time. Items older than `keep_days` are not stored, and stored items age out unless starred.

## Open it

Bind the two actions in `~/.config/herdr/config.toml`, then `herdr server reload-config`:

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

`open` splits beside the focused pane; `open-full` zooms over it. Either key pressed again focuses the reader, and a third press closes it.

## Keys

Three columns: feeds, items, article. Under 100 columns the pane shows one at a time; `Enter` goes right, `Esc` goes left.

| Key | Does |
| --- | --- |
| `j` `k` / arrows | Move in the focused column |
| `h` `l` / `Tab` | Move between columns |
| `Enter` / `Esc` | Open item (marks it read) / back |
| `Space` | Fold a group in the feeds column; page down elsewhere |
| `g` `G` / `PgUp` `PgDn` | Top, bottom, page |
| `r` / `R` | Refresh selected feed / all feeds |
| `n` / `p` | Next / previous unread item |
| `m` / `M` | Toggle read on item / mark list read |
| `s` | Toggle star |
| `u` | Show unread only |
| `o` | Open item in browser |
| `1`..`9` | Open that numbered link from the article |
| `y` | Copy item link |
| `Z` | Toggle zoom |
| `?` | Help |
| `q` | Quit |
| click | Focus and select; again on an item opens it, on a group folds it |
| wheel | Focus the column under the pointer; move its list or scroll the article |

Links in the article (the item link, `[text][n]` references, and the `[n]: url` footnotes) are real terminal hyperlinks, so Ctrl+click opens them in Herdr even when a URL wraps across rows.

## Configure

`$(herdr plugin config-dir shindakun.herdr-rss)/config.toml`, every key optional:

```toml
open_direction = "right"   # or "down"
refresh_minutes = 30       # 0 disables auto refresh while the pane is open
fetch_timeout_secs = 15
keep_days = 30             # unstarred items older than this are pruned
browser = "open"           # command that receives the URL; default per OS
```

## Development

```sh
make check   # fmt, clippy, test, audit, markdown lint; same as CI
make hooks   # install pre-commit
```

## License

MIT.
