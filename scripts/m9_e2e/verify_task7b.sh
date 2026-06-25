#!/usr/bin/env bash
# Task #7 verification (v2 — WARM cache, no CLEAR). The v1 mistake: AZ_LIFT_CACHE_CLEAR=1
# forced a full cold re-lift (hundreds of remill subprocesses) → stalled. The lift cache
# (8400+ raw-IR entries) is warm; the trace injection runs DOWNSTREAM on opt.ll every
# relift, so a warm cache still exercises the refactored instrument_reg_stores.
#   Run A (cleanA, 8820, no trace)   → full 5-step pipeline PASS ⇒ production unaffected.
#   Run B (traceB, 8821, AZ_REG_TRACE) → ring populated ⇒ refactored tracer preserved.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/verify7b.log; : > "$LOG"
NODE="/c/Users/felix/tools/node/node.exe"
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
say(){ echo "$*" | tee -a "$LOG"; }

run(){  # $1=port $2=label  $3..=extra KEY=VAL env
  local port="$1" label="$2"; shift 2
  local slog="/c/rb/v7b_${label}.server.log"; : > "$slog"
  powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
  sleep 2
  say "=== [$label] warm relift on $port $(date +%H:%M:%S) ==="
  ( export AZ_BACKEND="web://127.0.0.1:$port" AZ_LIFT_CACHE=1 "$@"
    nohup ./examples/c/hello-world.exe > "$slog" 2>&1 & echo "  pid $!" )
  local ok=0
  for i in $(seq 1 150); do   # 150*3s = 450s cap
    if grep -qE "Listening on" "$slog" 2>/dev/null; then ok=1; say "  READY after ${i}x3s $(date +%H:%M:%S)"; break; fi
    sleep 3
  done
  [ "$ok" = 1 ] || { say "  [$label] NOT READY in 450s — tail:"; tail -4 "$slog" | tee -a "$LOG"; return 1; }
}

# ---- Run A: production path ----
if run 8820 cleanA; then
  say "--- [cleanA] full-cycle (expect PASS) ---"
  AZ_PORT=8820 "$NODE" scripts/m9_e2e/full-cycle.js 2>&1 | tee -a "$LOG"
fi

# ---- Run B: refactored tracer ----
if run 8821 traceB AZ_REG_TRACE='impl$6::from'; then
  say "--- [traceB] full-cycle + AZ_DUMP_REGTRACE (expect ring populated) ---"
  AZ_PORT=8821 AZ_DUMP_REGTRACE=1 "$NODE" scripts/m9_e2e/full-cycle.js 2>&1 | head -45 | tee -a "$LOG"
fi
say "=== verify7b DONE $(date +%H:%M:%S) ==="
