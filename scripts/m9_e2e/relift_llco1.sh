#!/usr/bin/env bash
# BACKEND-OPT-LEVEL TEST: rebuild dll + cold relift with AZ_LLC_O1=1 → per-fn wasm
# backend (llc) at -O1 (RegStackify ON so locals fit, but the -O2-specific backend
# opts OFF). Splits an -O2 backend miscompile from a deeper cause:
#   solve RETURNS at backend -O1 ⇒ an -O2-only wasm-backend opt miscompiles it → bisect that pass.
#   solve still OOB at -O1       ⇒ bug is in -O1-level opt / instruction-selection / a semantic.
# (-O0 was inconclusive: RegStackify off → "local count too large", module won't instantiate.)
set -o pipefail
cd /c/Users/felix/Development/azul
export AZ_LLC_O1=1
mv /c/Users/felix/AppData/Local/Temp/az-lift-cache "/c/Users/felix/AppData/Local/Temp/az-lift-cache.bak_llco1_$(date +%s)" 2>/dev/null && echo "lift-cache cleared"
bash scripts/m9_e2e/baseline_build.sh
RES=/c/rb/relift_llco1.log
echo "=== probing solveLayoutReal (wasm BACKEND -O1) $(date +%H:%M:%S) ===" >> "$RES"
sleep 3
AZ_PORT=8800 STAGE=2 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/marker-probe.mjs 2>&1 \
  | grep -iE "VERDICT|ERROR: RuntimeError|wasm-function\[|no hang|solve returned|resolveChain|diag-at-trap|RIP=|local count" | head -14 | tee -a "$RES"
echo "=== llco1 probe DONE $(date +%H:%M:%S) ===" >> "$RES"
