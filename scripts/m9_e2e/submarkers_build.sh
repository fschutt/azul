#!/usr/bin/env bash
# Rebuild azul-dll with resolve-block sub-markers, warm-relift on :8800, run the
# solver probe. The resolveChain marker (0x40690) reads back as 0xAAAA000N = which
# of the 4 resolve calls trapped (1=resolve_font_chain_with_scripts, 2=chain iter,
# 3=resolve_char, 4=len, 5=list), or 0x5E5E0001 = all completed.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/submarkers.log; : > "$LOG"
echo "=== [1/4] build azul-dll  $(date +%H:%M:%S) ===" | tee -a "$LOG"
export RUSTC_BOOTSTRAP=1
export RUSTFLAGS="-Zunstable-options -Cpanic=immediate-abort"
export CARGO_BUILD_JOBS=6
cargo build -p azul-dll --release --no-default-features \
  --features "build-dll web web-transpiler" \
  -Z build-std=std,panic_abort --target x86_64-pc-windows-msvc 2>&1 | tail -5 | tee -a "$LOG"
RC=${PIPESTATUS[0]}
[ "$RC" != "0" ] && { echo "BUILD-FAILED rc=$RC" | tee -a "$LOG"; grep -E "error\[|error:" "$LOG" | head | tee -a "$LOG"; exit 1; }
ls -la --time-style=+%H:%M target/x86_64-pc-windows-msvc/release/azul.dll | tee -a "$LOG"
echo "=== [2/4] kill server + stage  $(date +%H:%M:%S) ===" | tee -a "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 3
cp -f target/x86_64-pc-windows-msvc/release/azul.dll examples/c/azul.dll
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb examples/c/azul.pdb
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb ./azul.pdb
echo "=== [3/4] restart warm (NO cache-clear)  $(date +%H:%M:%S) ===" | tee -a "$LOG"
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:/c/Users/felix/tools/node:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800
export AZ_LIFT_CACHE=1
export AZ_REMILL_KEEP_SCRATCH=1
: > /c/rb/server_sm.log
nohup ./examples/c/hello-world.exe > /c/rb/server_sm.log 2>&1 &
for i in $(seq 1 170); do
  grep -qE "Listening on" /c/rb/server_sm.log 2>/dev/null && { echo "READY ($i) $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$a" = "0" ] && { echo "SERVER-DIED" | tee -a "$LOG"; tail -10 /c/rb/server_sm.log | tee -a "$LOG"; exit 1; }
  sleep 8
done
echo "=== [4/4] solver probe  $(date +%H:%M:%S) ===" | tee -a "$LOG"
AZ_PORT=8800 AZ_HYDRATE=1 AZ_FONT=1 AZ_DUMP_DOM=1 AZ_DOM_SIZE=240 AZ_CHILD_OFF=152 \
  timeout 70 node scripts/m9_e2e/full-cycle.js 2>&1 | grep -iE "\[2c\]|\[2d|css\(|fontParse|resolveChain|TRAPPED|wasm-function" | head -30 | tee -a "$LOG"
echo "=== DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
