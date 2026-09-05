#!/usr/bin/env bash
# 2026-08-14 v2: dll rebuild with BOTH fixes then cold relift + verify.
#  (1) corrected TIGHTENED vtable scan (contiguous run, stop after 6 consecutive
#      non-fn slots) → smaller closure than the loose 512B sweep.
#  (2) per-cb LiftOpts cap 256 → 8192 (AZ_CB_MAX_DEPTH): the loose run's on_click +
#      layout cb lifts EXCEEDED 256 and fell back to NO-OP STUBS → initLayoutCache
#      trapped "null function or function signature mismatch".
# Mini cap stays high via AZ_MINI_MAX_DEPTH.
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
mv /c/Users/felix/AppData/Local/Temp/az-lift-cache "/c/Users/felix/AppData/Local/Temp/az-lift-cache.bak_v2_$(date +%s)" 2>/dev/null && echo "lift-cache cleared"
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1
export AZ_MINI_MAX_DEPTH=16384 AZ_CB_MAX_DEPTH=8192
echo "=== cold relift v2 $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_baseline.log 2>&1 &
for i in $(seq 1 500); do
  grep -qE "Listening on" /c/rb/server_baseline.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)"|tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED $(date +%H:%M:%S)"|tee -a "$LOG"; tail -8 /c/rb/server_baseline.log|tee -a "$LOG"; exit 1; }
  sleep 10
done
sleep 3
RES=/c/rb/relift_v2.log; : > "$RES"
echo "=== v2 VERDICT $(date +%H:%M:%S) ===" | tee -a "$RES"
grep -oE "transitive lift complete: [0-9]+ functions|azul-mini: lifted \+ linked [0-9]+ bytes|lift failed for [a-z_]+" /c/rb/server_baseline.log 2>/dev/null | tail -5 | tee -a "$RES"
AZ_PORT=8800 AZ_HYDRATE=1 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 \
  | grep -vE "\[STUB-0\]" | grep -iE "\[1\]|\[2\]|\[2c\]|\[2d\] solveLayout|initLayoutCache|FAIL|PASS|unreachable|out of bounds|null function|rects" | head -14 | tee -a "$RES"
echo "=== v2 DONE $(date +%H:%M:%S) ===" | tee -a "$RES"
