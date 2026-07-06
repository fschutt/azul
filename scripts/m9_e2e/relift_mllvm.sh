#!/usr/bin/env bash
# Fast LTO-relink test: relift with AZ_WASM_LD_MLLVM="$1" (forwards -mllvm to the LTO backend).
# Per-fn .o are cached so only the wasm-ld LTO link re-runs (~1 min). Probes if the solve returns.
set -o pipefail
cd /c/Users/felix/Development/azul
FLAGS="$1"
LOG=/c/rb/relift_mllvm.log; : > "$LOG"
echo "=== AZ_WASM_LD_MLLVM=$FLAGS ===" | tee -a "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null; sleep 2
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800 AZ_LIFT_CACHE=1 AZ_LIFT_CACHE_CLEAR=1 AZ_REMILL_KEEP_SCRATCH=1
export AZ_WASM_LD_MLLVM="$FLAGS"
nohup ./examples/c/hello-world.exe > /c/rb/server_mllvm.log 2>&1 &
for i in $(seq 1 300); do
  grep -qE "Listening on" /c/rb/server_mllvm.log 2>/dev/null && { echo "READY $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$a" = "0" ] && { echo "DIED $(date +%H:%M:%S)" | tee -a "$LOG"; tail -12 /c/rb/server_mllvm.log | tee -a "$LOG"; exit 1; }
  sleep 5
done
sleep 3
AZ_PORT=8800 STAGE=2 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/marker-probe.mjs 2>&1 | grep -iE "VERDICT|ERROR: RuntimeError|wasm-function\[|no hang|solve returned" | head -8 | tee -a "$LOG"
echo "=== done $(date +%H:%M:%S) ===" | tee -a "$LOG"
