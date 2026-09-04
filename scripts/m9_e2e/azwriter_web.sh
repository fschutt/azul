#!/usr/bin/env bash
# Run AzWriter (examples/azul-writer → bin `AzWriter`) through the web-lift
# backend and capture a CDP screenshot. This is a MUCH larger surface than
# hello-world (document layout, text rendering, an Export-to-PDF button), so its
# console/exception log is the point: each `RuntimeError` names a lifting bug.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/azwriter_web.log; : > "$LOG"

# 1. wait for the (already running) cargo build to finish.
# The binary now builds with the dll's lift-friendly recipe
# (build-std + -Cpanic=abort + explicit target triple), so it lands under the
# target-triple dir, not target/release.
BIN=target/x86_64-pc-windows-msvc/release/AzWriter.exe
for i in $(seq 1 240); do
  [ -f "$BIN" ] && break
  a=$(ps -W 2>/dev/null | grep -icE 'cargo|rustc')
  [ "$a" = "0" ] && break
  sleep 15
done
if [ ! -f "$BIN" ]; then
  echo "BUILD-MISSING: $BIN not produced" | tee -a "$LOG"
  tail -25 /c/rb/azwriter_build2.log 2>/dev/null | tee -a "$LOG"
  exit 1
fi
echo "=== binary ready $(date +%H:%M:%S) ===" | tee -a "$LOG"

powershell -NoProfile -Command "Get-Process AzWriter -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8801 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1
export AZ_MINI_MAX_DEPTH=16384 AZ_CB_MAX_DEPTH=8192
echo "=== lifting AzWriter (first run is cold; can take a while) $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup "./$BIN" > /c/rb/azwriter_server.log 2>&1 &

# AzWriter's cold lift (mini + layout cb + on_export cb, each ~3000 fns) can
# exceed 2h when a rebuild shifted every address (per-fn cache keys on exact
# bytes). Budget 3h — and on expiry ABORT, never fall through: run 2 timed
# out at 83 min mid-pre-render and screenshotted ERR_CONNECTION_REFUSED.
for i in $(seq 1 1080); do
  grep -qE "Listening on" /c/rb/azwriter_server.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  a=$(ps -W 2>/dev/null | grep -icE 'AzWriter')
  [ "$a" = "0" ] && {
    echo "DIED $(date +%H:%M:%S) — last lift line + any crash text below" | tee -a "$LOG"
    grep -vE "^\[azul-web\]   (intercept|    dep)" /c/rb/azwriter_server.log | tail -12 | tee -a "$LOG"
    exit 1; }
  sleep 10
done
grep -qE "Listening on" /c/rb/azwriter_server.log 2>/dev/null || {
  echo "TIMEOUT: server never printed 'Listening on' — aborting (server left running)" | tee -a "$LOG"; exit 1; }
echo "8byte-stub/link-fails: $(grep -icE '0xc0000142|falling back to 8-byte|falling back to stub' /c/rb/azwriter_server.log)" | tee -a "$LOG"
grep -oE "azul-mini: lifted \+ linked [0-9]+ bytes \([0-9]+ exports|transitive lift complete: [0-9]+ functions" /c/rb/azwriter_server.log | tail -3 | tee -a "$LOG"
# The mini link line is the load-bearing sanity check: run 1 shipped an 8-byte
# stub mini (dlsym on a static exe) that the old greps never caught.
grep -q "azul-mini: lifted + linked" /c/rb/azwriter_server.log || \
  echo "WARNING: no 'azul-mini: lifted + linked' line — mini is a stub, bootstrap WILL fail" | tee -a "$LOG"
sleep 3
echo "=== CDP screenshot $(date +%H:%M:%S) ===" | tee -a "$LOG"
"/c/Users/felix/tools/node/node.exe" --experimental-websocket \
  scripts/m9_e2e/cdp_screenshot.js "http://127.0.0.1:8801/" /c/rb/azwriter.png 20000 2>&1 | tee -a "$LOG"
echo "=== AZWRITER DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
