#!/bin/sh
# smoke-test.sh — verify a built xvn binary before release.
# Usage: scripts/smoke-test.sh path/to/xvn
#
# Checks: --version, doctor, dashboard serve (start + curl + stop).

set -eu

BINARY="${1:-./target/release/xvn}"
PORT="${SMOKE_PORT:-19999}"
TIMEOUT=15
XVN_HOME=$(mktemp -d)
trap 'rm -rf "$XVN_HOME"' EXIT

export XVN_HOME

echo "=== Smoke test: $BINARY ==="

# 1. version
echo "--- version ---"
"$BINARY" --version

# 2. doctor
echo "--- doctor ---"
"$BINARY" doctor

# 3. dashboard serve (start, curl, stop)
echo "--- dashboard serve ---"
"$BINARY" dashboard serve --bind "127.0.0.1:$PORT" --home "$XVN_HOME" &
PID=$!
echo "  pid=$PID"

# Wait for server to start
ELAPSED=0
while [ $ELAPSED -lt $TIMEOUT ]; do
    if curl -s "http://127.0.0.1:$PORT/" > /dev/null 2>&1; then
        echo "  server responding after ${ELAPSED}s"
        break
    fi
    sleep 1
    ELAPSED=$((ELAPSED + 1))
    # Check if process died
    if ! kill -0 "$PID" 2>/dev/null; then
        echo "  server died before responding" >&2
        exit 1
    fi
done

if [ $ELAPSED -ge $TIMEOUT ]; then
    echo "  server did not respond within ${TIMEOUT}s" >&2
    kill "$PID" 2>/dev/null || true
    exit 1
fi

# Verify response is HTML (embedded SPA)
RESP=$(curl -s "http://127.0.0.1:$PORT/")
if echo "$RESP" | grep -q '<!DOCTYPE html>'; then
    echo "  SPA HTML served"
else
    echo "  response does not look like SPA HTML" >&2
    kill "$PID" 2>/dev/null || true
    exit 1
fi

# Stop server
kill "$PID" 2>/dev/null || true
wait "$PID" 2>/dev/null || true
echo "  server stopped"

# 4. verify DB auto-bootstrapped
if [ -f "$XVN_HOME/xvn.db" ]; then
    echo "--- DB auto-bootstrapped ---"
else
    echo "DB not auto-bootstrapped" >&2
    exit 1
fi

echo ""
echo "All smoke tests passed."
