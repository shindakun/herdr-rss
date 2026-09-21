---
name: herdr-rss
description: Read the user's RSS feeds from a Herdr session. Use when asked what is new, to summarize recent items, or to add a feed.
---

# herdr-rss

The plugin binary is a CLI over the store the reader pane uses. It is `target/release/herdr-rss` under the `plugin_root` that `herdr plugin list` prints. Set the two variables Herdr gives plugin commands:

```sh
export HERDR_PLUGIN_CONFIG_DIR="$(herdr plugin config-dir shindakun.herdr-rss)"
export HERDR_PLUGIN_STATE_DIR="$HOME/.local/state/herdr/plugins/shindakun.herdr-rss"
```

Every subcommand takes `--json`. Items have a 16-character `id`.

| Command | Does |
| --- | --- |
| `herdr-rss refresh` | Fetch every feed; slow with hundreds of feeds |
| `herdr-rss list --unread --limit 20 --json` | Unread items, newest first |
| `herdr-rss list --feed URL --json` | One feed's items |
| `herdr-rss show ID --json` | One item with its `summary_html`; without `--json`, as text |
| `herdr-rss show ID --full` | Fetch the item's page and show the extracted article |
| `herdr-rss mark ID...` | Mark read |
| `herdr-rss star ID...` | Toggle a star |
| `herdr-rss add URL --name N --group G` | Fetch the URL and add it; fails if it is not a feed |
| `herdr-rss remove URL` | Drop a feed |
| `herdr-rss export` | All feeds as OPML |

Do not mark items read unless the user asked you to.
