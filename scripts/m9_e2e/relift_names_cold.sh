#!/usr/bin/env bash
# Cold relift with names (dll already built with AZ_MINI_KEEP_NAMES gate). CLEAR the lift
# cache — the earlier relift_names HANG was a stale warm cache (relift_merge cleared it +
# worked). Full pipeline (incl. relocate_stack + inject_data_segments) → SERVED named wasm.
set -o pipefail
cd /c/Users/felix/Development/azul
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
mv /c/Users/felix/AppData/Local/Temp/az-lift-cache "/c/Users/felix/AppData/Local/Temp/az-lift-cache.bak_names_$(date +%s)" 2>/dev/null && echo "lift-cache cleared"
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1 AZ_MINI_KEEP_NAMES=1
echo "=== cold relift (names) $(date +%H:%M:%S) ==="
nohup ./examples/c/hello-world.exe > /c/rb/server_baseline.log 2>&1 &
for i in $(seq 1 300); do
  grep -qE "Listening on" /c/rb/server_baseline.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED $(date +%H:%M:%S)"; tail -8 /c/rb/server_baseline.log; exit 1; }
  sleep 10
done
sleep 3
RES=/c/rb/relift_names.log; : > "$RES"
echo "=== NAMED solve trap $(date +%H:%M:%S) ===" | tee -a "$RES"
AZ_PORT=8800 AZ_HYDRATE=1 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 \
  | grep -iE "\[2c\]|\[2d\] solveLayout|unreachable|out of bounds|at [A-Za-z_]|sub_[0-9a-f]|hashbrown|unwrap|format|panic|wasm-function" | head -30 | tee -a "$RES"
echo "=== names probe DONE $(date +%H:%M:%S) ===" | tee -a "$RES"
