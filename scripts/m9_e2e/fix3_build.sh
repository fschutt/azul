#!/usr/bin/env bash
# Rebuild azul.dll (helper ret-pop + enforce_sp SP-delta), stage, cold relift,
# verify. codegen already current. Expect [2] rc=0 AND [4] counter 5->6.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/fix3_build.log
: > "$LOG"
echo "=== [1/3] build azul-dll  $(date +%H:%M:%S) ===" | tee -a "$LOG"
export RUSTC_BOOTSTRAP=1
export RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort"
export CARGO_BUILD_JOBS=6
cargo build -p azul-dll --release --no-default-features \
  --features "build-dll web web-transpiler" \
  -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc 2>&1 | tail -6 | tee -a "$LOG"
ls -la --time-style=+%H:%M target/x86_64-pc-windows-msvc/release/azul.dll 2>/dev/null \
  | tee -a "$LOG" || { echo "DLL-BUILD-FAILED" | tee -a "$LOG"; exit 1; }
grep -q "error\[" "$LOG" && { echo "COMPILE-ERROR" | tee -a "$LOG"; exit 1; }

echo "=== [2/3] stage + kill old  $(date +%H:%M:%S) ===" | tee -a "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 2
cp -f target/x86_64-pc-windows-msvc/release/azul.dll examples/c/azul.dll
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb examples/c/azul.pdb
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb ./azul.pdb

export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800
export AZ_LIFT_CACHE=1
export AZ_LIFT_CACHE_CLEAR=1
export AZ_REMILL_KEEP_SCRATCH=1
echo "=== [3/3] cold relift  $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_fix5.log 2>&1 &
for i in $(seq 1 260); do
  grep -qE "Listening on" /c/rb/server_fix5.log 2>/dev/null && { echo "READY ($(wc -l < /c/rb/server_fix5.log)L) $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  alive=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$alive" = "0" ] && { echo "SERVER-DIED" | tee -a "$LOG"; tail -8 /c/rb/server_fix5.log | tee -a "$LOG"; exit 1; }
  sleep 10
done
echo "=== THE TEST (expect [2] rc=0 AND [4] counter 5->6)  $(date +%H:%M:%S) ===" | tee -a "$LOG"
AZ_PORT=8800 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 | head -24 | tee -a "$LOG"
echo "=== DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
