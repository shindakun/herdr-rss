# Fixtures

`pane_list.json` is the shape of `herdr pane list` on Herdr 0.9.1, trimmed to three panes: an agent pane, a plugin pane in the same tab (`label` is the manifest pane title, `cwd` is the plugin root), and a focused pane in another workspace. Tests move `focused` around to cover open, focus, and close.
