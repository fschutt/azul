#!/usr/bin/env bash
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/build_disp.log; : > "$LOG"
echo "=== build dll $(date +%H:%M:%S) ===" | tee -a "$LOG"
export RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort" CARGO_BUILD_JOBS=6
cargo build -p azul-dll --release --no-default-features --features "build-dll web web-transpiler" \
  -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc 2>&1 | tail -4 | tee -a "$LOG"
grep -q "error\[" "$LOG" && { echo "COMPILE-ERROR"|tee -a "$LOG"; exit 1; }
[ -f target/x86_64-pc-windows-msvc/release/azul.dll ] || { echo "DLL-FAIL"|tee -a "$LOG"; exit 1; }
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
cp -f target/x86_64-pc-windows-msvc/release/azul.dll examples/c/azul.dll
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb examples/c/azul.pdb; cp -f target/x86_64-pc-windows-msvc/release/azul.pdb ./azul.pdb
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8810 AZ_LIFT_CACHE=1 AZ_LIFT_CACHE_CLEAR=1
export AZ_REMILL_KEEP_SCRATCH=1 AZ_REG_TRACE='AzStartup_dispatchEvent' AZ_REG_TRACE_NOWRAP=1
echo "=== cold relift + dispatch trace $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_disp2.log 2>&1 &
for i in $(seq 1 260); do
  grep -qE "Listening on" /c/rb/server_disp2.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)"|tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED"|tee -a "$LOG"; tail -8 /c/rb/server_disp2.log|tee -a "$LOG"; exit 1; }
  sleep 10
done
AZ_DUMP_REGTRACE=1 AZ_DEBUG_PATCH=1 AZ_PORT=8810 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 | sed -n '/\[4\]\|regtrace\|RBP\|dbg\|FAIL\|PASS/p' | head -55 | tee -a "$LOG"
echo "=== DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
