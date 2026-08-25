#!/usr/bin/env bash
# Task #7 verification: prove the instrumentation refactor (emit_ring_record
# consolidation + AZ_SP_TRACE aarch64 gate) is (A) production-inert — a clean
# cold relift still passes the full 5-step pipeline — and (B) behavior-preserving
# for the refactored tracer — an AZ_REG_TRACE relift still populates the ring.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/verify_task7.log; : > "$LOG"
DLL=target/x86_64-pc-windows-msvc/release/azul.dll
NODE="/c/Users/felix/tools/node/node.exe"
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"

[ -f "$DLL" ] || { echo "DLL MISSING — build first" | tee -a "$LOG"; exit 1; }
echo "=== stage built dll/pdb $(date +%H:%M:%S) ===" | tee -a "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 2
cp -f "$DLL" examples/c/azul.dll
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb examples/c/azul.pdb
cp -f target/x86_64-pc-windows-msvc/release/azul.pdb ./azul.pdb

run_pipeline() {  # $1=port  $2=label  $3..=extra env (KEY=VAL)
  local port="$1" label="$2"; shift 2
  local slog="/c/rb/verify_${label}.server.log"
  echo "=== [$label] cold relift on $port $(date +%H:%M:%S) ===" | tee -a "$LOG"
  powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
  sleep 2
  ( export AZ_BACKEND="web://127.0.0.1:$port" AZ_LIFT_CACHE=1 AZ_LIFT_CACHE_CLEAR=1 "$@"
    nohup ./examples/c/hello-world.exe > "$slog" 2>&1 & echo "server pid $!" | tee -a "$LOG" )
  for i in $(seq 1 260); do
    grep -qE "Listening on" "$slog" 2>/dev/null && { echo "READY $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
    a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
    [ "$a" = "0" ] && { echo "[$label] SERVER-DIED" | tee -a "$LOG"; tail -8 "$slog" | tee -a "$LOG"; return 1; }
    sleep 5
  done
}

# ---- Run A: clean (no trace) — the production path ----
run_pipeline 8820 cleanA
echo "--- [cleanA] full-cycle (expect PASS) ---" | tee -a "$LOG"
AZ_PORT=8820 "$NODE" scripts/m9_e2e/full-cycle.js 2>&1 | tee -a "$LOG"

# ---- Run B: AZ_REG_TRACE — exercises the refactored instrument_reg_stores ----
run_pipeline 8821 traceB AZ_REG_TRACE='impl$6::from'
echo "--- [traceB] full-cycle + AZ_DUMP_REGTRACE (expect ring populated) ---" | tee -a "$LOG"
AZ_PORT=8821 AZ_DUMP_REGTRACE=1 "$NODE" scripts/m9_e2e/full-cycle.js 2>&1 | head -40 | tee -a "$LOG"

echo "=== verify_task7 done $(date +%H:%M:%S) ===" | tee -a "$LOG"
