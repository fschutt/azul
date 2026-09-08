#!/usr/bin/env bash
# 2026-08-13: rebuild dll (AZ_MINI_KEEP_NAMES gate) + relift with names kept in the
# OPTIMIZED mini wasm, so the solveLayoutReal trap stack shows sub_<addr> names instead
# of wasm-function[N]. Identifies the .unwrap()(→core::result::unwrap_failed) trap fn
# WITHOUT the lost index→name tooling. Lift cache kept WARM (per-fn lifts are cache hits;
# only the mini LINK re-runs with names) → faster than a cold relift.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/baseline_build.log; : > "$LOG"
export RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=abort" CARGO_BUILD_JOBS=6
echo "=== build dll (AZ_MINI_KEEP_NAMES) $(date +%H:%M:%S) ===" | tee -a "$LOG"
cargo build -p azul-dll --release --no-default-features --features "build-dll web web-transpiler" \
  -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc 2>&1 | tail -5 | tee -a "$LOG"
grep -q "error\[" "$LOG" && { echo "COMPILE-ERROR"|tee -a "$LOG"; exit 1; }
[ -f target/x86_64-pc-windows-msvc/release/azul.dll ] || { echo "DLL-FAIL"|tee -a "$LOG"; exit 1; }
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
cp -f target/x86_64-pc-windows-msvc/release/azul.dll examples/c/azul.dll
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb examples/c/azul.pdb 2>/dev/null
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
# NOTE: lift cache kept warm (no AZ_LIFT_CACHE_CLEAR, no mv of az-lift-cache) → only the
# mini link re-runs (with names). AZ_REMILL_KEEP_SCRATCH so we can inspect artifacts.
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1 AZ_MINI_KEEP_NAMES=1
echo "=== relift (names kept, warm cache) $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_baseline.log 2>&1 &
for i in $(seq 1 200); do
  grep -qE "Listening on" /c/rb/server_baseline.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)"|tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED"|tee -a "$LOG"; tail -8 /c/rb/server_baseline.log|tee -a "$LOG"; exit 1; }
  sleep 10
done
sleep 3
RES=/c/rb/relift_names.log
echo "=== solve trap with NAMED functions $(date +%H:%M:%S) ===" | tee "$RES"
AZ_PORT=8800 AZ_HYDRATE=1 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 \
  | grep -iE "\[2c\]|\[2d\] solveLayout|unreachable|wasm-function|sub_[0-9a-f]|hashbrown|unwrap|format|result|panic|at [a-z_]+" | head -30 | tee -a "$RES"
echo "=== names probe DONE $(date +%H:%M:%S) ===" | tee -a "$RES"
