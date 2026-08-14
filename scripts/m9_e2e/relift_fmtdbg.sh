#!/usr/bin/env bash
# 2026-08-13: dll rebuild (PROBE0b format! decomposition + AZ_MINI_KEEP_NAMES flag fix:
# omit --strip-all, no invalid --keep-section=name) + CLEAR lift cache (bytes changed) +
# cold relift with names + full-cycle AZ_HYDRATE=1 → [2d-fmt] shows which write returns Err
# (isolates the fmt Result Ok→Err mis-lift) + NAMED solve trap stack.
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
mv /c/Users/felix/AppData/Local/Temp/az-lift-cache "/c/Users/felix/AppData/Local/Temp/az-lift-cache.bak_fmtdbg_$(date +%s)" 2>/dev/null && echo "lift-cache cleared"
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1 AZ_MINI_KEEP_NAMES=1
echo "=== cold relift (names + fmt decomposition) $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_baseline.log 2>&1 &
for i in $(seq 1 320); do
  grep -qE "Listening on" /c/rb/server_baseline.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)"|tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED $(date +%H:%M:%S)"|tee -a "$LOG"; tail -8 /c/rb/server_baseline.log|tee -a "$LOG"; exit 1; }
  sleep 10
done
sleep 3
RES=/c/rb/relift_fmtdbg.log; : > "$RES"
echo "=== fmt decomposition + NAMED trap $(date +%H:%M:%S) ===" | tee -a "$RES"
AZ_PORT=8800 AZ_HYDRATE=1 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 \
  | grep -iE "\[2c\]|\[2d-fmt\]|\[2d\] solveLayout|unreachable|out of bounds|at [A-Za-z_]|sub_[0-9a-f]|write_str|format|fmt|panic" | head -30 | tee -a "$RES"
echo "=== fmtdbg DONE $(date +%H:%M:%S) ===" | tee -a "$RES"
