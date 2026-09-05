#!/usr/bin/env bash
# Clean COLD relift, NO AZ_MINI_KEEP_NAMES (that broke the wasm: sharded 320KB, missing
# deps). Matches the working relift_merge config → monolithic 30MB wasm. dll already built
# with the PROBE0b format! decomposition. → full-cycle AZ_HYDRATE=1 [2d-fmt] shows which
# write returns Err (the fmt Result Ok→Err mis-lift).
set -o pipefail
cd /c/Users/felix/Development/azul
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
mv /c/Users/felix/AppData/Local/Temp/az-lift-cache "/c/Users/felix/AppData/Local/Temp/az-lift-cache.bak_fd2_$(date +%s)" 2>/dev/null && echo "lift-cache cleared"
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1
echo "=== cold relift (normal, fmt decomposition) $(date +%H:%M:%S) ==="
nohup ./examples/c/hello-world.exe > /c/rb/server_baseline.log 2>&1 &
for i in $(seq 1 320); do
  grep -qE "Listening on" /c/rb/server_baseline.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED $(date +%H:%M:%S)"; tail -8 /c/rb/server_baseline.log; exit 1; }
  sleep 10
done
sleep 3
RES=/c/rb/relift_fmtdbg.log; : > "$RES"
echo "=== fmt decomposition $(date +%H:%M:%S) ===" | tee -a "$RES"
AZ_PORT=8800 AZ_HYDRATE=1 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 \
  | grep -vE "\[STUB-0\]" | grep -iE "\[1\]|\[2c\]|\[2d-fmt\]|\[2d-p0\]|\[2d\] solveLayout|initLayoutCache|unreachable|out of bounds|FAIL|write_str" | head -20 | tee -a "$RES"
echo "=== fmtdbg2 DONE $(date +%H:%M:%S) ===" | tee -a "$RES"
