#!/usr/bin/env bash
# Run AzWriter (examples/azul-writer → bin `azul-doc-demo`) through the web-lift
# backend and capture a CDP screenshot. This is a MUCH larger surface than
# hello-world (document layout, text rendering, an Export-to-PDF button), so its
# console/exception log is the point: each `RuntimeError` names a lifting bug.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/azwriter_web.log; : > "$LOG"

# 1. wait for the (already running) cargo build to finish
for i in $(seq 1 240); do
  [ -f target/release/azul-doc-demo.exe ] && break
  a=$(ps -W 2>/dev/null | grep -icE 'cargo|rustc')
  [ "$a" = "0" ] && break
  sleep 15
done
if [ ! -f target/release/azul-doc-demo.exe ]; then
  echo "BUILD-MISSING: azul-doc-demo.exe not produced" | tee -a "$LOG"
  tail -25 /c/rb/azwriter_build.log 2>/dev/null | tee -a "$LOG"
  exit 1
fi
echo "=== binary ready $(date +%H:%M:%S) ===" | tee -a "$LOG"

powershell -NoProfile -Command "Get-Process azul-doc-demo -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8801 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1
export AZ_MINI_MAX_DEPTH=16384 AZ_CB_MAX_DEPTH=8192
echo "=== lifting AzWriter (first run is cold; can take a while) $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./target/release/azul-doc-demo.exe > /c/rb/azwriter_server.log 2>&1 &

for i in $(seq 1 500); do
  grep -qE "Listening on" /c/rb/azwriter_server.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  a=$(ps -W 2>/dev/null | grep -icE 'azul-doc-demo')
  [ "$a" = "0" ] && { echo "DIED $(date +%H:%M:%S)" | tee -a "$LOG"; tail -25 /c/rb/azwriter_server.log | tee -a "$LOG"; exit 1; }
  sleep 10
done
echo "8byte-stub/link-fails: $(grep -icE '0xc0000142|falling back to 8-byte' /c/rb/azwriter_server.log)" | tee -a "$LOG"
grep -oE "azul-mini: lifted \+ linked [0-9]+ bytes \([0-9]+ exports|transitive lift complete: [0-9]+ functions" /c/rb/azwriter_server.log | tail -3 | tee -a "$LOG"
sleep 3
echo "=== CDP screenshot $(date +%H:%M:%S) ===" | tee -a "$LOG"
"/c/Users/felix/tools/node/node.exe" --experimental-websocket \
  scripts/m9_e2e/cdp_screenshot.js "http://127.0.0.1:8801/" /c/rb/azwriter.png 20000 2>&1 | tee -a "$LOG"
echo "=== AZWRITER DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
