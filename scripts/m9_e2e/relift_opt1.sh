#!/usr/bin/env bash
# TEST: relift the CURRENT dll with AZ_OPT_LEVEL=1 (conservative LTO codegen).
# If the solve OOB disappears, the LTO-stage codegen is the class-B miscompile culprit.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/relift_opt1.log; : > "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_LIFT_CACHE_CLEAR=1 AZ_REMILL_KEEP_SCRATCH=1
export AZ_OPT_LEVEL=1
echo "=== relift OPT=1 $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_opt1.log 2>&1 &
for i in $(seq 1 300); do
  grep -qE "Listening on" /c/rb/server_opt1.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED $(date +%H:%M:%S)" | tee -a "$LOG"; tail -10 /c/rb/server_opt1.log | tee -a "$LOG"; exit 1; }
  sleep 10
done
echo "=== relift DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
sleep 3
echo "=== marker-probe STAGE=2 (does the solve RETURN now?) ===" | tee -a "$LOG"
AZ_PORT=8800 STAGE=2 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/marker-probe.mjs >> "$LOG" 2>&1
echo "=== probe DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
