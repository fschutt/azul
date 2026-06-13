#!/bin/bash
# Windows (git-bash) port of scripts/web_relift.sh — relift launcher +
# port waiter for the azul web (lifted) backend on x86_64-pc-windows-msvc.
#
# Usage:  scripts/web_relift_win.sh <example-exe> <server-log>
#   e.g.  scripts/web_relift_win.sh examples/c/hello-world.exe C:/rb/server_hw.log
#
# Launches the example under AZ_BACKEND=web, waits (up to ~50 min) for the
# lift to finish and the port to answer, then exits 0 (READY) or 1
# (TIMEOUT/DEAD). Run it with run_in_background so the harness re-invokes
# you when the port is up.
set -u
BIN="${1:?need example exe path}"
LOG="${2:?need server log path}"
WSROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WSROOT"

TARGETDIR="$WSROOT/target/x86_64-pc-windows-msvc/release"
RL_DEFAULT="$WSROOT/third_party/remill-install/bin/remill-lift-17.exe"
RL="${REMILL_LIFT_BIN:-$RL_DEFAULT}"
LLVMBIN="$WSROOT/third_party/remill/dependencies/install/bin"

# azul.dll lives in the cargo target dir — put it on PATH for the exe's
# loader, plus the LLVM tools for the subprocess pipeline.
export PATH="$TARGETDIR:$LLVMBIN:$PATH"

# 1. Kill orphans (handoff gotcha ③): stale example exes, remill-lift
#    workers, and whatever owns port 8800. PowerShell equivalents of the
#    macOS ps/lsof dance. Never matches this launcher (different names).
powershell -NoProfile -Command "
  Get-Process -Name 'remill-lift-17' -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue;
  Get-Process | Where-Object { \$_.Path -and \$_.Path -like '*examples\\c\\*.exe' } | Stop-Process -Force -ErrorAction SilentlyContinue;
  Get-NetTCPConnection -LocalPort 8800 -State Listen -ErrorAction SilentlyContinue |
    Select-Object -ExpandProperty OwningProcess -Unique |
    ForEach-Object { Stop-Process -Id \$_ -Force -ErrorAction SilentlyContinue }
" >/dev/null 2>&1
sleep 2

# 2. Launch the server (it lifts the dll → wasm at startup; first run
#    can take a while, AZ_LIFT_CACHE=1 makes reruns fast).
echo "LAUNCH $(date) bin=$BIN"
REMILL_LIFT_BIN="$RL" \
  AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_PREFLIGHT=1 \
  nohup "$BIN" > "$LOG" 2>&1 &

# 3. Wait for the port (poll; never probe the launch pid). ~50 min cap.
for i in $(seq 1 300); do
  sleep 10
  if curl -s -o /dev/null 127.0.0.1:8800 2>/dev/null; then
    echo "READY after $((i*10))s $(date)"
    grep -E 'Listening on|transitive\[' "$LOG" | tail -3
    exit 0
  fi
  if grep -qE "thread '.*' panicked|fatal runtime|STATUS_ACCESS_VIOLATION|0xc0000005|stack overflow" "$LOG" 2>/dev/null; then
    echo "DEAD: fatal in log after $((i*10))s"; tail -25 "$LOG"; exit 1
  fi
  # bail early if the process died silently (no port, no process)
  if ! powershell -NoProfile -Command "Get-Process | Where-Object { \$_.Path -eq '$(cygpath -w "$WSROOT/$BIN" 2>/dev/null | sed 's/\\\\/\\\\\\\\/g')' } | Select-Object -First 1" 2>/dev/null | grep -q .; then
    if [ "$i" -gt 3 ] && ! grep -q "Listening" "$LOG" 2>/dev/null; then
      echo "DEAD: process gone after $((i*10))s"; tail -25 "$LOG"; exit 1
    fi
  fi
done
echo "TIMEOUT waiting for port $(date)"; tail -25 "$LOG"; exit 1
