#!/usr/bin/env bash
# Validate the B1-SSE remill fix (x86 jump-table devirt in Lift.cpp + the
# TraceLifter convergence guard) in the FULL azul pipeline:
#   (A) hello-world full 5-step pipeline still PASSES (the convergence fix
#       touches every lift, so this is the regression gate), AND
#   (B) the 2 B1-SSE functions now LIFT (no "FAILED to lift" in the server log).
# Cold relift (remill rebuilt → lift cache invalidated) — generous wait.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/b1sse_validate.log; : > "$LOG"
SLOG=/c/rb/b1sse_server.log; : > "$SLOG"
NODE="/c/Users/felix/tools/node/node.exe"
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND="web://127.0.0.1:8830" AZ_LIFT_CACHE=1
say(){ echo "$*" | tee -a "$LOG"; }

powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 2
say "=== cold relift on 8830 (new remill: x86 devirt + convergence fix) $(date +%H:%M:%S) ==="
nohup ./examples/c/hello-world.exe > "$SLOG" 2>&1 & echo "  server pid $!" | tee -a "$LOG"
ok=0
for i in $(seq 1 420); do   # 420*3s = 21min cap
  if grep -qE "Listening on" "$SLOG" 2>/dev/null; then ok=1; say "  READY after ${i}x3s $(date +%H:%M:%S)"; break; fi
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$a" = "0" ] && { say "  SERVER-DIED at $(grep -oE 'transitive\[[0-9]+\]' "$SLOG" | tail -1)"; tail -5 "$SLOG" | tee -a "$LOG"; exit 1; }
  sleep 3
done
[ "$ok" = 1 ] || { say "  NOT READY in 21min — tail:"; tail -5 "$SLOG" | tee -a "$LOG"; exit 1; }

say "--- (A) full-cycle (expect PASS) ---"
AZ_PORT=8830 "$NODE" scripts/m9_e2e/full-cycle.js 2>&1 | tee -a "$LOG"

say "--- (B) did the 2 B1-SSE fns lift? (expect NO 'FAILED to lift') ---"
for fn in resolve_font_size_slow "UnresolvedBoxProps::resolve"; do
  n=$(grep -c "$fn FAILED to lift" "$SLOG" 2>/dev/null)
  say "  $fn: FAILED-to-lift count = $n  $([ "$n" = 0 ] && echo '✓ LIFTS NOW' || echo '✗ still failing')"
done
say "  (any remaining lift FAILUREs:)"
grep -oE "⚠ transitive\[[0-9]+\]: [^ ]+ FAILED to lift" "$SLOG" 2>/dev/null | tee -a "$LOG" || say "    (none)"
say "=== validate DONE $(date +%H:%M:%S) ==="
