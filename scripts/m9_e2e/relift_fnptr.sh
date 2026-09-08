#!/usr/bin/env bash
# Relift with the fn-ptr discovery fix + RAISED depth cap. The fix pulls in the whole
# previously-invisible indirect-dispatch closure (vtable/fmt targets), so the walk grows
# past the default 4096 cap (which ABORTS the lift). AZ_MINI_MAX_DEPTH raises it.
# dll is already built with the fix; lift cache is WARM (same dll bytes) → fast restart.
set -o pipefail
cd /c/Users/felix/Development/azul
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1
export AZ_MINI_MAX_DEPTH=16384
echo "=== relift (fn-ptr fix, depth cap 16384) $(date +%H:%M:%S) ==="
nohup ./examples/c/hello-world.exe > /c/rb/server_baseline.log 2>&1 &
for i in $(seq 1 400); do
  grep -qE "Listening on" /c/rb/server_baseline.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED $(date +%H:%M:%S)"; tail -10 /c/rb/server_baseline.log; exit 1; }
  sleep 10
done
sleep 3
RES=/c/rb/relift_fnptr.log; : > "$RES"
echo "=== SOLVE VERDICT $(date +%H:%M:%S) ===" | tee -a "$RES"
grep -oE "transitive lift complete: [0-9]+ functions|azul-mini.*exports" /c/rb/server_baseline.log 2>/dev/null | tail -2 | tee -a "$RES"
AZ_PORT=8800 AZ_HYDRATE=1 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 \
  | grep -vE "\[STUB-0\]" | grep -iE "\[1\]|\[2c\]|\[2d\] solveLayout|\[2d-p0\]|initLayoutCache|FAIL|PASS|unreachable|out of bounds|rects" | head -14 | tee -a "$RES"
echo "=== fnptr DONE $(date +%H:%M:%S) ===" | tee -a "$RES"
