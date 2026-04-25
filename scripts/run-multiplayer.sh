#!/usr/bin/env bash
# Build, then launch the server and two clients with distinct ids.
# Ctrl+C tears everything down.

set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="${PROFILE:-debug}"
case "$PROFILE" in
    debug) CARGO_FLAGS=() ;;
    release) CARGO_FLAGS=(--release) ;;
    *) echo "PROFILE must be debug or release" >&2; exit 1 ;;
esac

echo "==> building"
cargo build "${CARGO_FLAGS[@]}" -p vaern-server -p vaern-client

cleanup() {
    echo
    echo "==> shutting down"
    # Kill all children of this shell
    pkill -P $$ 2>/dev/null || true
}
trap cleanup EXIT INT TERM

BIN_DIR="target/$PROFILE"

# Local dev defaults: bind + connect on loopback. Override either by
# exporting before invoking. Debug builds fall back to the all-zero dev
# key when VAERN_NETCODE_KEY is unset; release builds reject it.
export VAERN_BIND="${VAERN_BIND:-127.0.0.1:27015}"
export VAERN_SERVER="${VAERN_SERVER:-127.0.0.1:27015}"

echo "==> starting server (bind $VAERN_BIND)"
"$BIN_DIR/vaern-server" &
SERVER_PID=$!

# Small delay so the UDP socket is bound before clients try to connect.
sleep 0.4

# Each client picks its class via VAERN_CLASS (0..=14 or internal label
# e.g. fighter, wizard, rogue, cleric). If unset, the server falls back to
# `client_id % 15`.
echo "==> launching client 1000 (Fighter)"
VAERN_CLIENT_ID=1000 VAERN_CLASS=fighter "$BIN_DIR/vaern-client" &

echo "==> launching client 1001 (Wizard)"
VAERN_CLIENT_ID=1001 VAERN_CLASS=wizard "$BIN_DIR/vaern-client" &

echo "==> running. Ctrl+C to stop."
wait
