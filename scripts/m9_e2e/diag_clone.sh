#!/usr/bin/env bash
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/diag_clone.log; : > "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 2
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8806 AZ_LIFT_CACHE=1 AZ_LIFT_CACHE_CLEAR=1
export AZ_REMILL_KEEP_SCRATCH=1 AZ_REG_TRACE='vec::impl$11::clone' AZ_REG_TRACE_NOWRAP=1
echo "=== relift, regtrace Vec::clone $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_clone.log 2>&1 &
for i in $(seq 1 260); do
  grep -qE "Listening on" /c/rb/server_clone.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)"|tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED"|tee -a "$LOG"; tail -8 /c/rb/server_clone.log|tee -a "$LOG"; exit 1; }
  sleep 10
done
AZ_PORT=8806 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 | head -10 | tee -a "$LOG"
echo "=== DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
