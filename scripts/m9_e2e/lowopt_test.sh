#!/usr/bin/env bash
# Restart the server (NO rebuild, warm cache) with AZ_LOWOPT_FNS=query_internal → lift
# query_internal at -O0. If the solver now PASSES → the bug is an over-aggressive opt fold
# in query_internal (a loop body deleted / pointer computation mis-folded). If it still
# TRAPS → the bug is a raw remill lift (pre-opt) → disasm-vs-IR.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/lowopt.log; : > "$LOG"
echo "=== kill server + restart with AZ_LOWOPT_FNS=query_internal  $(date +%H:%M:%S) ===" | tee -a "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 3
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:/c/Users/felix/tools/node:$PATH"
export AZ_BACKEND=web://127.0.0.1:8800
export AZ_LIFT_CACHE=1
export AZ_REMILL_KEEP_SCRATCH=1
export AZ_LOWOPT_FNS=request_fonts_fast,parse_font_faces,calculate_style_score,find_unicode_fallbacks,calculate_unicode_compatibility
: > /c/rb/server_lowopt.log
nohup ./examples/c/hello-world.exe > /c/rb/server_lowopt.log 2>&1 &
for i in $(seq 1 200); do
  grep -qE "Listening on" /c/rb/server_lowopt.log 2>/dev/null && { echo "READY ($i, $(date +%H:%M:%S))" | tee -a "$LOG"; break; }
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$a" = "0" ] && { echo "SERVER-DIED" | tee -a "$LOG"; tail -10 /c/rb/server_lowopt.log | tee -a "$LOG"; exit 1; }
  sleep 8
done
echo "=== solver probe (expect resolveChain=5E5E0001 + rects if -O0 FIXES it)  $(date +%H:%M:%S) ===" | tee -a "$LOG"
AZ_PORT=8800 AZ_HYDRATE=1 AZ_FONT=1 AZ_DUMP_DOM=1 AZ_DOM_SIZE=240 AZ_CHILD_OFF=152 \
  node scripts/m9_e2e/full-cycle.js 2>&1 | grep -iE "\[2c\]|\[2d|resolveChain|TRAPPED|wasm-function" | head -20 | tee -a "$LOG"
echo "=== func[262] size after -O0 sweep (was 146588; if CHANGED, one of the stems IS func[262]) ===" | tee -a "$LOG"
CUR=$(ls -dt /c/Users/felix/AppData/Local/Temp/azul-web-transpiler-* 2>/dev/null | head -1)
node /c/rb/funcrange.js "$CUR/azul-mini.wasm" 262 2>/dev/null | tee -a "$LOG"
echo "=== DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
