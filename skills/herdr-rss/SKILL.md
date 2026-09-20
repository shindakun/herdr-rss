---
name: herdr-rss
description: Read the user's RSS feeds from a Herdr session. Use when asked what is new, to summarize recent items, or to add a feed.
---

# herdr-rss

The plugin binary is a CLI over the same store the reader pane uses. Find it with `herdr plugin list` (the `plugin_root`, then `target/release/herdr-rss`). Every subcommand takes `--json`.

| Command | Does |
| --- | --- |
| `herdr-rss refresh` | Fetch every feed |
| `herdr-rss list --unread --limit 20 --json` | Unread items, newest first |
| `herdr-rss show ID` | One item's text |
| `herdr-rss mark ID...` | Mark read |
| `herdr-rss add URL --name N --group G` | Add a feed |

Do not mark items read unless the user asked you to.
