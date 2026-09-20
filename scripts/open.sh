#!/usr/bin/env bash
# Open the reader in a split beside the focused pane. Idempotent inside the
# current tab: no reader pane -> open; one exists unfocused -> focus it; the
# reader is focused -> close it. Herdr has no focus-by-id, so focus is a
# `pane zoom --on` then `--off`.
#
# The decision comes from the binary (`--launch-decision`, `pane list` JSON on
# stdin) so it is unit tested. Any failure falls through to OPEN.
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
    pid="${decision#FOCUS }"
    "$herdr_bin" pane zoom "$pid" --on >/dev/null 2>&1 || true
    exec "$herdr_bin" pane zoom "$pid" --off
    ;;
  "CLOSE "*)
    exec "$herdr_bin" pane close "${decision#CLOSE }"
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
