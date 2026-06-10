#!/bin/bash
# Reusable relift launcher + port waiter for the azul web (lifted) backend.
# Usage:  scripts/web_relift.sh <example-bin> <server-log>
#   e.g.  scripts/web_relift.sh examples/c/web-text-min.bin /tmp/server_wtm.log
# Launches the example under AZ_BACKEND=web, waits (up to ~50 min) for the lift to
# finish and the port to answer, then exits 0 (READY) or 1 (TIMEOUT/DEAD).
# Run it with run_in_background so the harness re-invokes you when the port is up.
set -u
BIN="${1:?need example bin path}"
LOG="${2:?need server log path}"
cd /Users/fschutt/Development/azul-mobile
DYLDIR=target/aarch64-apple-darwin/release
RL=/Users/fschutt/Development/azul/third_party/remill-install/build/remill/bin/lift/remill-lift-17

# 1. Kill orphans (handoff gotcha ③). Exclude THIS script (its argv contains the bin
#    name too — matching it would kill the launcher itself) via web_relift + self-PID guard.
SELF=$$
ps -axo pid,command \
  | grep -E 'examples/c/[A-Za-z0-9_-]+\.bin|remill-lift' \
  | grep -vE 'grep|web_relift' \
  | awk -v self="$SELF" '$1 != self {print $1}' \
  | xargs kill -9 2>/dev/null
lsof -ti tcp:8800 2>/dev/null | xargs kill -9 2>/dev/null
sleep 2

# 2. Launch the server (forks+exits; it relifts the dylib → wasm, ~15-30 min).
echo "LAUNCH $(date) bin=$BIN"
DYLD_LIBRARY_PATH=$DYLDIR REMILL_LIFT_BIN=$RL \
  AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_PREFLIGHT=1 \
  nohup "$BIN" > "$LOG" 2>&1 &

# 3. Wait for the port (poll, never kill -0 the launch pid). ~50 min cap (300*10s).
for i in $(seq 1 300); do
  sleep 10
  if curl -s -o /dev/null 127.0.0.1:8800 2>/dev/null; then
    echo "READY after $((i*10))s $(date)"
    # show the last transitive/listening lines for context
    grep -E 'Listening on|transitive\[' "$LOG" | tail -3
    exit 0
  fi
  # liveness: if no remill workers AND the log shows a fatal panic, bail early
  if grep -qE "thread '.*' panicked|fatal runtime|Segmentation fault|SIGABRT|SIGBUS|SIGSEGV" "$LOG" 2>/dev/null; then
    echo "DEAD: fatal in log after $((i*10))s"; tail -25 "$LOG"; exit 1
  fi
done
echo "TIMEOUT waiting for port $(date)"; tail -25 "$LOG"; exit 1
