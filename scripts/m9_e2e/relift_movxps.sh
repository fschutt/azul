#!/usr/bin/env bash
# TEST: relift with the new amd64.bc (MOVxPS = bit-preserving <2 x i64> copy, not <4 x float>).
# Does the class-B fat-pointer solveLayoutReal OOB disappear? (dll unchanged; only the
# remill runtime semantics .bc changed → relift picks it up.)
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/relift_movxps.log; : > "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_LIFT_CACHE_CLEAR=1 AZ_REMILL_KEEP_SCRATCH=1
# NOTE: AZ_LLC_NOSTOREMERGE deliberately UNSET (that flag didn't help; normal codegen).
echo "=== relift MOVxPS-fix $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_movxps.log 2>&1 &
for i in $(seq 1 320); do
  grep -qE "Listening on" /c/rb/server_movxps.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED $(date +%H:%M:%S)" | tee -a "$LOG"; tail -12 /c/rb/server_movxps.log | tee -a "$LOG"; exit 1; }
  sleep 10
done
sleep 3
echo "=== marker-probe STAGE=2 (does solveLayoutReal RETURN now?) ===" | tee -a "$LOG"
AZ_PORT=8800 STAGE=2 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/marker-probe.mjs 2>&1 \
  | grep -iE "VERDICT|ERROR: RuntimeError|wasm-function\[|no hang|solve returned|resolveChain|diag-at-trap" | head -12 | tee -a "$LOG"
echo "=== movxps probe DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
