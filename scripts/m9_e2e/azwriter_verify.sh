#!/usr/bin/env bash
# AzWriter web-lift run with the relocation cache VERIFIED.
#
# AZ_RELOC_VERIFY=1 fresh-lifts every reloc-cache hit and byte-diffs it against
# the translated template. A divergence is a wrong address spliced into lifted
# code: it links, runs, and jumps somewhere plausible but wrong. Verify mode
# reports each one, discards the bad template and uses the fresh lift, so this
# run both names the broken entries and boots correctly.
#
# It roughly doubles remill time for the run. That is the documented price of a
# debugging run - see doc/web-reloc-cache-identity-collision.md, "Operational
# rule". Do not switch it off to save time; that is why the last occurrence of
# this bug class survived for days.
set -o pipefail
cd /c/Users/felix/Development/azul
LOG=/c/rb/azwriter_verify.log; : > "$LOG"

BIN=target/x86_64-pc-windows-msvc/release/AzWriter.exe
if [ ! -f "$BIN" ]; then
  echo "BUILD-MISSING: $BIN" | tee -a "$LOG"; exit 1
fi

powershell -NoProfile -Command "Get-Process AzWriter -EA SilentlyContinue | Stop-Process -Force" 2>/dev/null
sleep 2

export REMILL_LIFT_BIN=/c/rb/remill/bin/lift/remill-lift-17.exe
export PATH="$PWD/third_party/remill/dependencies/install/bin:$PATH"
export AZ_BACKEND=web://127.0.0.1:8801 AZ_LIFT_CACHE=1 AZ_REMILL_KEEP_SCRATCH=1
export AZ_MINI_MAX_DEPTH=16384 AZ_CB_MAX_DEPTH=8192
export AZ_RELOC_VERIFY=1

# Rotate, never truncate: synth addresses are assigned per build, so the log of
# the run being diagnosed must survive the next run.
if [ -s /c/rb/azwriter_server.log ]; then
  PREV_TS=$(date -r /c/rb/azwriter_server.log +%Y%m%d-%H%M%S 2>/dev/null || date +%Y%m%d-%H%M%S)
  cp /c/rb/azwriter_server.log "/c/rb/azwriter_server.$PREV_TS.log" 2>/dev/null || true
fi

echo "=== lifting AzWriter with AZ_RELOC_VERIFY=1 $(date +%H:%M:%S) ===" | tee -a "$LOG"
nohup "./$BIN" > /c/rb/azwriter_server.log 2>&1 &

for i in $(seq 1 1080); do
  grep -qE "Listening on" /c/rb/azwriter_server.log 2>/dev/null && {
    echo "READY $(date +%H:%M:%S)" | tee -a "$LOG"; break; }
  a=$(ps -W 2>/dev/null | grep -icE 'AzWriter')
  [ "$a" = "0" ] && {
    echo "DIED $(date +%H:%M:%S)" | tee -a "$LOG"
    grep -vE "^\[azul-web\]   (intercept|    dep)" /c/rb/azwriter_server.log | tail -12 | tee -a "$LOG"
    exit 1; }
  sleep 10
done
grep -qE "Listening on" /c/rb/azwriter_server.log 2>/dev/null || {
  echo "TIMEOUT: no 'Listening on'" | tee -a "$LOG"; exit 1; }

echo "=== reloc verify summary ===" | tee -a "$LOG"
echo "divergences: $(grep -c 'DIVERGENCE' /c/rb/azwriter_server.log)" | tee -a "$LOG"
grep 'DIVERGENCE in' /c/rb/azwriter_server.log | head -40 | tee -a "$LOG"

sleep 3
echo "=== CDP boot $(date +%H:%M:%S) ===" | tee -a "$LOG"
"/c/Users/felix/tools/node/node.exe" --experimental-websocket \
  scripts/m9_e2e/cdp-console.js "http://127.0.0.1:8801/" 25000 2>&1 | tee -a "$LOG"
echo "=== VERIFY RUN DONE $(date +%H:%M:%S) ===" | tee -a "$LOG"
