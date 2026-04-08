#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
DEFAULT_BRIDGE_BIN="$REPO_ROOT/target/debug/kwin-portal-bridge"
DEFAULT_TEACH_SOCKET="${XDG_RUNTIME_DIR:-/tmp}/kwin-portal-bridge/teach-overlay.sock"

run_bridge() {
	if [[ -n "${KWIN_PORTAL_BRIDGE_BIN:-}" ]]; then
		"$KWIN_PORTAL_BRIDGE_BIN" "$@"
		return
	fi

	if [[ -x "$DEFAULT_BRIDGE_BIN" ]]; then
		"$DEFAULT_BRIDGE_BIN" "$@"
		return
	fi

	(
		cd -- "$REPO_ROOT"
		cargo run --quiet -- "$@"
	)
}

teach_socket_path() {
	printf '%s\n' "$DEFAULT_TEACH_SOCKET"
}
