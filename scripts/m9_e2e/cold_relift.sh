#!/usr/bin/env bash
# COLD relift (AZ_LIFT_CACHE_CLEAR=1) to pick up the remill DoCPUID inline-emulation fix.
# No dll rebuild — the current examples/c/azul.dll (PROBE0 + chain_cache disabled) is fine; we
# just re-lift every fn with the new amd64.bc semantics so the HashMap/SwissTable CPUID gap is gone.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/cold_relift.log; : > "$LOG"
echo "=== kill server $(date +%H:%M:%S) ===" | tee -a "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 3
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:/c/Users/felix/tools/node:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800
export AZ_LIFT_CACHE=1
export AZ_LIFT_CACHE_CLEAR=1
export AZ_REMILL_KEEP_SCRATCH=1
echo "=== COLD relift start $(date +%H:%M:%S) ===" | tee -a "$LOG"
: > /c/rb/server_cold.log
nohup ./examples/c/hello-world.exe > /c/rb/server_cold.log 2>&1 &
for i in $(seq 1 320); do
  grep -qE "Listening on" /c/rb/server_cold.log 2>/dev/null && { echo "READY ($i) $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$a" = "0" ] && { echo "SERVER-DIED $(date +%H:%M:%S)" | tee -a "$LOG"; tail -20 /c/rb/server_cold.log | tee -a "$LOG"; exit 1; }
  sleep 8
done
echo "=== probe $(date +%H:%M:%S) ===" | tee -a "$LOG"
timeout 80 env AZ_PORT=8800 AZ_HYDRATE=1 AZ_FONT=1 AZ_DUMP_DOM=1 AZ_DOM_SIZE=240 AZ_CHILD_OFF=152 \
  node scripts/m9_e2e/full-cycle.js 2>&1 | grep -iE "\[2c\]|\[2d|css\(|fontParse|TRAPPED|wasm-function|solved|rects" | head -40 | tee -a "$LOG"
echo "=== DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
