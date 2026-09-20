---
name: herdr-rss
description: Read the user's RSS feeds from a Herdr session. Use when asked what is new, to summarize recent items, or to add a feed.
---

# herdr-rss

The plugin binary is a CLI over the same store the reader pane uses. Find it with `herdr plugin list` (the `plugin_root`, then `target/release/herdr-rss`). It needs two variables Herdr sets for plugin commands:

```sh
export HERDR_PLUGIN_CONFIG_DIR="$(herdr plugin config-dir shindakun.herdr-rss)"
export HERDR_PLUGIN_STATE_DIR="$HOME/.local/state/herdr/plugins/shindakun.herdr-rss"
```

Every subcommand takes `--json`. Items have a 16-character `id`.

| Command | Does |
| --- | --- |
| `herdr-rss refresh` | Fetch every feed; 209 feeds take about 25 seconds |
| `herdr-rss list --unread --limit 20 --json` | Unread items, newest first |
| `herdr-rss list --feed URL --json` | One feed's items |
| `herdr-rss show ID --json` | One item with its `summary_html`; without `--json`, as text |
| `herdr-rss mark ID...` | Mark read |
| `herdr-rss star ID...` | Toggle a star |
| `herdr-rss add URL --name N --group G` | Add a feed |

Do not mark items read unless the user asked you to.
