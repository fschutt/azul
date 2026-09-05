#!/usr/bin/env bash
# RESET to a clean consistent working build: decomposition reverted, no keep-names.
# dll rebuild + azul.pdb copy + CLEAR lift cache (mv) + cold relift → should reproduce
# relift_merge's working 30MB monolithic mini (~1444 deps). Verify size before diagnostics.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/baseline_build.log; : > "$LOG"
export RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=abort" CARGO_BUILD_JOBS=6
echo "=== build dll $(date +%H:%M:%S) ===" | tee -a "$LOG"
cargo build -p azul-dll --release --no-default-features --features "build-dll web web-transpiler" \
  -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc 2>&1 | tail -4 | tee -a "$LOG"
grep -q "error\[" "$LOG" && { echo "COMPILE-ERROR"|tee -a "$LOG"; exit 1; }
[ -f target/x86_64-pc-windows-msvc/release/azul.dll ] || { echo "DLL-FAIL"|tee -a "$LOG"; exit 1; }
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
cp -f target/x86_64-pc-windows-msvc/release/azul.dll examples/c/azul.dll
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb examples/c/azul.pdb
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb ./azul.pdb
mv /c/Users/felix/AppData/Local/Temp/az-lift-cache "/c/Users/felix/AppData/Local/Temp/az-lift-cache.bak_reset_$(date +%s)" 2>/dev/null && echo "lift-cache cleared"
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1
echo "=== cold relift (reset) $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_baseline.log 2>&1 &
for i in $(seq 1 340); do
  grep -qE "Listening on" /c/rb/server_baseline.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)"|tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED"|tee -a "$LOG"; tail -8 /c/rb/server_baseline.log|tee -a "$LOG"; exit 1; }
  sleep 10
done
RES=/c/rb/relift_reset.log; : > "$RES"
echo "=== verify (mini size + solve reaches trap) $(date +%H:%M:%S) ===" | tee -a "$RES"
grep -iE "azul-mini.*exports|transitive lift complete" /c/rb/server_baseline.log 2>/dev/null | tail -2 | tee -a "$RES"
sleep 2
AZ_PORT=8800 AZ_HYDRATE=1 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 \
  | grep -vE "\[STUB-0\]" | grep -iE "\[1\]|\[2c\]|\[2d\] solveLayout|initLayoutCache|FAIL|unreachable|out of bounds" | head -10 | tee -a "$RES"
echo "=== reset DONE $(date +%H:%M:%S) ===" | tee -a "$RES"
