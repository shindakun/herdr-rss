#!/usr/bin/env bash
# Open the reader zoomed over the focused pane. Same open / focus / close
# logic as open.sh; only the placement differs.
HERDR_RSS_PLACEMENT=zoomed exec bash "$(dirname "${BASH_SOURCE[0]:-$0}")/open.sh"
