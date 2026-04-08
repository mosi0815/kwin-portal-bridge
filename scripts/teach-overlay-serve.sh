#!/usr/bin/env bash
set -euo pipefail

source "$(cd -- "$(dirname -- "$0")" && pwd)/_bridge_common.sh"

socket="$(teach_socket_path)"
printf 'Serving standalone teach overlay on %s\n' "$socket"
printf 'Leave this running in its own terminal while using the other scripts.\n'

run_bridge serve-teach-overlay --socket "$socket"
