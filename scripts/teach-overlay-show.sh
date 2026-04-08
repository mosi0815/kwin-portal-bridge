#!/usr/bin/env bash
set -euo pipefail

source "$(cd -- "$(dirname -- "$0")" && pwd)/_bridge_common.sh"

usage() {
	cat <<'EOF'
Usage:
  teach-overlay-show.sh [--display DISPLAY_ID] [--centered]
  teach-overlay-show.sh --display DISPLAY_ID --anchor X Y

Options:
  --display DISPLAY_ID   Bridge/KWin display id from teach-overlay-screens.sh
  --centered            Show a centered bubble with no anchor
  --anchor X Y          Global logical anchor point for an anchored bubble
  --explanation TEXT    Step explanation text
  --next-preview TEXT   Next-preview text
EOF
}

display=''
mode='centered'
anchor_x=''
anchor_y=''
explanation='Manual teach-overlay test. Click Next or Exit to verify the bridge-backed bubble.'
next_preview='Use teach-overlay-working.sh after Next if you want to test the spinner state.'

while [[ $# -gt 0 ]]; do
	case "$1" in
	--display)
		display="${2:?missing value for --display}"
		shift 2
		;;
	--centered)
		mode='centered'
		shift
		;;
	--anchor)
		mode='anchor'
		anchor_x="${2:?missing X for --anchor}"
		anchor_y="${3:?missing Y for --anchor}"
		shift 3
		;;
	--explanation)
		explanation="${2:?missing value for --explanation}"
		shift 2
		;;
	--next-preview)
		next_preview="${2:?missing value for --next-preview}"
		shift 2
		;;
	--help | -h)
		usage
		exit 0
		;;
	*)
		printf 'Unknown argument: %s\n\n' "$1" >&2
		usage >&2
		exit 1
		;;
	esac
done

if [[ "$mode" == 'anchor' && -z "$display" ]]; then
	printf '%s\n' 'Anchored tests require --display so the bridge can localize the anchor.' >&2
	exit 1
fi

payload="$({
	python3 - "$mode" "$explanation" "$next_preview" "$anchor_x" "$anchor_y" <<'PY'
import json
import sys

mode, explanation, next_preview, anchor_x, anchor_y = sys.argv[1:6]
payload = {
    "explanation": explanation,
    "nextPreview": next_preview,
}
if mode == "anchor":
    payload["anchorLogical"] = {"x": int(anchor_x), "y": int(anchor_y)}
print(json.dumps(payload, separators=(",", ":")))
PY
})"

args=(teach-step --payload "$payload")
if [[ -n "$display" ]]; then
	args+=(--display "$display")
fi

run_bridge "${args[@]}"
