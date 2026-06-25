#!/usr/bin/env bash
# Full pipeline for the x86 helper ret-pop fix: regenerate codegen (rebase
# changed core/src/video.rs + api.json), rebuild azul.dll, stage, cold relift,
# run full-cycle. Expect [2] rc=0 (full hello-world, no OOB) AND [4] 5->6.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/fix_build.log
: > "$LOG"
echo "=== [1/5] build azul-doc  $(date +%H:%M:%S) ===" | tee -a "$LOG"
cargo build --release -p azul-doc 2>&1 | tail -6 | tee -a "$LOG"
[ -f target/release/azul-doc.exe ] || { echo "AZUL-DOC-BUILD-FAILED" | tee -a "$LOG"; exit 1; }

echo "=== [2/5] codegen all     $(date +%H:%M:%S) ===" | tee -a "$LOG"
./target/release/azul-doc.exe codegen all 2>&1 | tail -8 | tee -a "$LOG"

echo "=== [3/5] build azul-dll  $(date +%H:%M:%S) ===" | tee -a "$LOG"
export RUSTC_BOOTSTRAP=1
export RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort"
export CARGO_BUILD_JOBS=6
cargo build -p azul-dll --release --no-default-features \
  --features "build-dll web web-transpiler" \
  -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc 2>&1 | tail -8 | tee -a "$LOG"
ls -la --time-style=+%H:%M target/x86_64-pc-windows-msvc/release/azul.dll 2>/dev/null \
  | tee -a "$LOG" || { echo "DLL-BUILD-FAILED" | tee -a "$LOG"; exit 1; }

echo "=== [4/5] stage + kill old servers  $(date +%H:%M:%S) ===" | tee -a "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 2
cp -f target/x86_64-pc-windows-msvc/release/azul.dll examples/c/azul.dll
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb examples/c/azul.pdb
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb ./azul.pdb

export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800
export AZ_LIFT_CACHE=1
export AZ_LIFT_CACHE_CLEAR=1   # cold relift — bust the wasm cache so the fix applies
echo "=== [5/5] cold relift (server)  $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_fix3.log 2>&1 &
for i in $(seq 1 240); do
  grep -qE "Listening on" /c/rb/server_fix3.log 2>/dev/null && { echo "READY ($(wc -l < /c/rb/server_fix3.log)L) $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  alive=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$alive" = "0" ] && { echo "SERVER-DIED" | tee -a "$LOG"; tail -8 /c/rb/server_fix3.log | tee -a "$LOG"; exit 1; }
  sleep 10
done
echo "=== THE TEST (expect [2] rc=0 AND [4] counter 5->6)  $(date +%H:%M:%S) ===" | tee -a "$LOG"
AZ_PORT=8800 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 | head -20 | tee -a "$LOG"
echo "=== DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
