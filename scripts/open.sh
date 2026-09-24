#!/usr/bin/env bash
# Open the reader beside the focused pane. Within the current tab: no reader
# pane, open one; one open but unfocused, focus it; the reader focused, close
# it. The binary makes the decision from `pane list` JSON. Any failure means
# open.
set -uo pipefail

herdr_bin="${HERDR_BIN_PATH:-herdr}"
root="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"
bin="$root/target/release/herdr-rss"
placement="${HERDR_RSS_PLACEMENT:-split}"

decision="OPEN"
if [ -x "$bin" ]; then
  panes="$("$herdr_bin" pane list 2>/dev/null || true)"
  if [ -n "$panes" ]; then
    decision="$(printf '%s' "$panes" | HERDR_PLUGIN_ROOT="$root" "$bin" --launch-decision 2>/dev/null || echo OPEN)"
  fi
fi

case "$decision" in
  "FOCUS "*)
    exec "$herdr_bin" plugin pane focus "${decision#FOCUS }"
    ;;
  "CLOSE "*)
    exec "$herdr_bin" plugin pane close "${decision#CLOSE }"
    ;;
esac

args=(plugin pane open --plugin shindakun.herdr-rss --entrypoint reader --placement "$placement" --focus)
if [ "$placement" = "split" ]; then
  direction="right"
  cfg="${HERDR_PLUGIN_CONFIG_DIR:-$("$herdr_bin" plugin config-dir shindakun.herdr-rss 2>/dev/null || true)}"
  if [ -x "$bin" ]; then
    case "$(HERDR_PLUGIN_CONFIG_DIR="$cfg" "$bin" --open-direction 2>/dev/null || true)" in
      down) direction="down" ;;
    esac
  fi
  args+=(--direction "$direction")
fi
exec "$herdr_bin" "${args[@]}"
