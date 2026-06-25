#!/usr/bin/env bash
# Diagnostic relift (dll already built with the ret-pop fix): keep scratch so we
# can read Css::from's opt.ll and confirm the pop is present + not DCE'd.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/diag_relift.log
: > "$LOG"
powershell -NoProfile -Command "Get-Process hello-world -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 2
export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8802
export AZ_LIFT_CACHE=1
export AZ_LIFT_CACHE_CLEAR=1
export AZ_REMILL_KEEP_SCRATCH=1
export AZ_REG_TRACE='impl$6::from'
echo "=== diag cold relift (scratch kept, regtrace)  $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup ./examples/c/hello-world.exe > /c/rb/server_diag.log 2>&1 &
echo "server pid $!" | tee -a "$LOG"
for i in $(seq 1 260); do
  grep -qE "Listening on" /c/rb/server_diag.log 2>/dev/null && { echo "READY ($(wc -l < /c/rb/server_diag.log)L) $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  alive=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null | tr -d '\r')
  [ "$alive" = "0" ] && { echo "SERVER-DIED" | tee -a "$LOG"; tail -8 /c/rb/server_diag.log | tee -a "$LOG"; exit 1; }
  sleep 10
done
echo "=== run full-cycle on 8802  $(date +%H:%M:%S) ===" | tee -a "$LOG"
AZ_PORT=8802 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/full-cycle.js 2>&1 | head -20 | tee -a "$LOG"
echo "=== locate scratch dir ===" | tee -a "$LOG"
SP=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue | Select -First 1).Id" 2>/dev/null | tr -d '\r')
echo "server pid=$SP" | tee -a "$LOG"
ls -dt "$TEMP"/azul-web-transpiler-* 2>/dev/null | head -3 | tee -a "$LOG"
echo "=== DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
