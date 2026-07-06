#!/usr/bin/env bash
# Waits for the marker rebuild to finish (baseline_build.log "=== DONE"), then runs
# marker-probe STAGE=2 and captures the 0x607xx failed.len bisection markers.
cd /c/Users/felix/Development/azul
RES=/c/rb/probe_result5.log; : > "$RES"
echo "=== wait_and_probe5 start $(date +%H:%M:%S) ===" >> "$RES"
for i in $(seq 1 180); do
  if grep -q "=== DONE" /c/rb/baseline_build.log 2>/dev/null; then
    echo "=== build DONE detected $(date +%H:%M:%S) ===" >> "$RES"
    break
  fi
  a=$(powershell -NoProfile -Command "(Get-Process hello-world -EA SilentlyContinue|Measure-Object).Count" 2>/dev/null|tr -d '\r')
  [ "$a" = "0" ] && { echo "=== hello-world DIED before DONE $(date +%H:%M:%S) ===" >> "$RES"; tail -8 /c/rb/server_baseline.log >> "$RES" 2>&1; exit 1; }
  sleep 15
done
sleep 3
echo "=== marker-probe STAGE=2 $(date +%H:%M:%S) ===" >> "$RES"
AZ_PORT=8800 STAGE=2 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/marker-probe.mjs >> "$RES" 2>&1
echo "=== probe DONE $(date +%H:%M:%S) ===" >> "$RES"
