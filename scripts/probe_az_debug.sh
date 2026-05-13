#!/usr/bin/env bash
# probe_az_debug.sh <port> <expected_before> <expected_after>
#
# Drives the AZ_DEBUG counter probe against a running hello-world.
# Prints the BEFORE / AFTER values and exits non-zero if they don't
# match the expected ones (default: 5 / 8).
#
# Usage:
#   ./probe_az_debug.sh 8080            # asserts counter goes 5 → 8
#   ./probe_az_debug.sh 8080 5 8        # explicit form

set -uo pipefail

PORT="${1:?usage: $0 <port> [expected_before=5] [expected_after=8]}"
EXPECTED_BEFORE="${2:-5}"
EXPECTED_AFTER="${3:-8}"
HOST="http://localhost:${PORT}/"

# Wait up to ~10s for the AZ_DEBUG server to come up.
attempts=0
until curl -s --max-time 1 -X POST -d '{"op":"get_html_string"}' "$HOST" -o /tmp/probe_before.json 2>/dev/null && [ -s /tmp/probe_before.json ]; do
    attempts=$((attempts + 1))
    if [ "$attempts" -gt 20 ]; then
        echo "[probe] AZ_DEBUG server on :$PORT did not respond within 10s" >&2
        exit 2
    fi
    sleep 0.5
done

# Parse counter from HTML (font-size: 32px div).
parse_counter() {
    python3 -c "
import json, re, sys
try:
    d = json.loads(open(sys.argv[1]).read(), strict=False)
    html = d['data']['value']['html']
    m = re.search(r'font-size: 32px[^>]*>\\s*<[^>]*>\\s*([0-9]+)', html)
    print(m.group(1) if m else 'NF')
except Exception as e:
    print(f'ERR: {e}', file=sys.stderr)
    print('NF')
" "$1"
}

BEFORE=$(parse_counter /tmp/probe_before.json)
echo "[probe] BEFORE counter: $BEFORE"

for _ in 1 2 3; do
    curl -s --max-time 2 -X POST -d '{"op":"click","selector":".__azul-native-button"}' "$HOST" > /dev/null
    sleep 0.3
done

curl -s --max-time 2 -X POST -d '{"op":"get_html_string"}' "$HOST" -o /tmp/probe_after.json
AFTER=$(parse_counter /tmp/probe_after.json)
echo "[probe] AFTER  counter: $AFTER"

if [ "$BEFORE" = "$EXPECTED_BEFORE" ] && [ "$AFTER" = "$EXPECTED_AFTER" ]; then
    echo "[probe] PASS"
    exit 0
fi
echo "[probe] FAIL: expected $EXPECTED_BEFORE → $EXPECTED_AFTER, got $BEFORE → $AFTER" >&2
exit 1
