# herdr-rss

A [Herdr](https://herdr.dev) plugin that reads RSS, Atom, and JSON feeds in a terminal pane. Rust, one binary, no other runtime. Opens beside your work as a split or over it, zoomed.

## Install

```sh
herdr plugin install shindakun/herdr-rss
```

Needs `cargo`; the install step builds the binary. Linux and macOS.

To work on it, link a checkout instead:

```sh
cargo build --release
herdr plugin link /path/to/herdr-rss
```

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

`open` splits beside the focused pane; `open-full` zooms over it. The same key again focuses the reader; a third press closes it.

## Keys

Three columns: feeds, items, article. Under 100 columns the pane shows one at a time; `Enter` goes right, `Esc` goes left.

| Key | Does |
| --- | --- |
| `j` `k` / arrows | Move in the focused column |
| `h` `l` / `Tab` | Move between columns |
| `Enter` / `Esc` | Open item / back |
| `Space` | Fold a group in the feeds column; page down elsewhere |
| `g` `G` / `PgUp` `PgDn` | Top, bottom, page |
| `r` / `R` | Refresh selected feed / all feeds |
| `n` / `p` | Next / previous unread item |
| `m` / `M` | Toggle read on item / mark list read |
| `s` | Toggle star |
| `u` | Show unread only |
| `/` | Search titles in this list; `Enter` on an empty line clears |
| `a` | Add a feed by URL under the selected group |
| `d` | Delete the selected feed, after `y` |
| `o` | Open item in browser |
| `1`..`9` | Open that numbered link from the article |
| `y` | Copy item link |
| `f` | Fetch the full article from its page |
| `Z` | Toggle zoom |
| `?` | Help |
| `q` | Quit |
| click | Focus and select; again on an item opens it, on a group folds it |
| wheel | Focus the column under the pointer; move its list or scroll the article |

Opening an item marks it read, by `Enter`, click, or wheel. A feed marked `!` failed its last fetch; select it and the status line shows why. The pane refreshes every `refresh_minutes` on its own.

`f` fetches the item's page and swaps in the article text, extracted the way Firefox's reader view does it. The header then says `full text`. A site that blocks non-browser clients shows its HTTP status instead.

Links in the article are terminal hyperlinks: the item link, `[text][n]` references, and the `[n]: url` footnotes. Ctrl+click opens them, wrapped or not.

## Feeds

`$(herdr plugin config-dir shindakun.herdr-rss)/feeds.txt`. One feed per line, `Name | URL`. `# Heading` starts a group. `//` starts a comment. A `#` line with a `|` in it is a commented-out feed.

```text
# Tech
Ars Technica | https://feeds.arstechnica.com/arstechnica/index
Hacker News | https://hnrss.org/frontpage
// Cloudflare blocks this one:
#   Ausretrogamer | https://ausretrogamer.com/feed/

# Games
Rock Paper Shotgun | https://www.rockpapershotgun.com/feed
```

Edit it by hand, press `a` in the pane, or use the CLI. Adds and removes change one line and leave the rest alone.

## Configure

`$(herdr plugin config-dir shindakun.herdr-rss)/config.toml`, every key optional:

```toml
open_direction = "right"   # or "down"
refresh_minutes = 30       # 0 turns auto refresh off
fetch_timeout_secs = 15
keep_days = 30             # unstarred items older than this are dropped
browser = "open"           # command that receives the URL; default per OS
```

## CLI

The same binary is a CLI over the same store, for scripts and agents. It lives at `<plugin root>/target/release/herdr-rss`; `herdr plugin list` prints the root. It needs the two variables Herdr gives plugin commands:

```sh
export HERDR_PLUGIN_CONFIG_DIR="$(herdr plugin config-dir shindakun.herdr-rss)"
export HERDR_PLUGIN_STATE_DIR="$HOME/.local/state/herdr/plugins/shindakun.herdr-rss"
```

Every subcommand takes `--json`.

| Command | Does |
| --- | --- |
| `refresh [--feed URL] [--detach]` | Fetch and store; `--detach` returns at once and logs to `refresh.log` in the state dir |
| `list [--unread] [--starred] [--feed URL] [--limit N]` | Items, newest first; 50 by default |
| `show ID [--full] [--width N]` | One item as text; `--full` fetches and extracts the article first |
| `mark ID... [--unread]` | Set read state |
| `star ID...` | Toggle the star |
| `add URL [--name N] [--group G]` | Fetch the URL, then add it; a URL that is not a feed is refused |
| `remove URL` | Drop a feed and its items |
| `import FILE.opml [--replace]` | Merge an OPML file, skipping feeds already present; `--replace` starts over |
| `export` | feeds.txt as OPML on stdout |

A refresh sends `If-None-Match` and `If-Modified-Since`; an unchanged feed costs one small request. Eight fetches run at a time. Items older than `keep_days` are not stored, and stored items age out unless starred.

## Development

```sh
make check   # fmt, clippy, test, audit, markdown lint; same as CI
make hooks   # install pre-commit
```

Design and what is left: [docs/PLAN.md](docs/PLAN.md).

## License

MIT.
