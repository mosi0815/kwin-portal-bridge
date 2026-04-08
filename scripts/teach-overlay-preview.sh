#!/usr/bin/env bash
set -euo pipefail

source "$(cd -- "$(dirname -- "$0")" && pwd)/_bridge_common.sh"

usage() {
	cat <<'EOF'
Usage:
  teach-overlay-preview.sh [--display DISPLAY_ID] [--centered] [--auto-exit-ms 8000]
  teach-overlay-preview.sh --display DISPLAY_ID --anchor X Y [--working]

This runs the teach bubble directly on the main thread for manual visual testing.
EOF
}

display=''
mode='centered'
anchor_x=''
anchor_y=''
working='false'
auto_exit_ms='8000'
explanation='Direct teach-overlay preview. This bypasses the controller socket so we can manually test rendering.'
next_preview='If this appears, the layer-shell bubble itself is rendering. Use --working to preview the spinner state.'

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
	--working)
		working='true'
		shift
		;;
	--auto-exit-ms)
		auto_exit_ms="${2:?missing value for --auto-exit-ms}"
		shift 2
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
	printf '%s\n' 'Anchored previews require --display so the bridge can localize the anchor.' >&2
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

args=(teach-overlay-preview --payload "$payload")
if [[ -n "$display" ]]; then
	args+=(--display "$display")
fi
if [[ "$working" == 'true' ]]; then
	args+=(--working)
fi
if [[ -n "$auto_exit_ms" ]]; then
	args+=(--auto-exit-ms "$auto_exit_ms")
fi

run_bridge "${args[@]}"
