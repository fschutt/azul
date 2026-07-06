#!/usr/bin/env bash
# TEST: rebuild dll + cold relift with AZ_LLC_NOSTOREMERGE=1 (now = store-merge +
# machine-cse + machine-sink all OFF) to isolate the class-B multi-word regalloc/
# coalescing miscompile (the fat-ptr LEN half gets garbage). MOVxPS <2 x i64> fix
# still active in amd64.bc. Manual cache-clear (AZ_LIFT_CACHE_CLEAR doesn't clear it).
set -o pipefail
cd /c/Users/felix/Development/azul
export AZ_LLC_NOSTOREMERGE=1
mv /c/Users/felix/AppData/Local/Temp/az-lift-cache "/c/Users/felix/AppData/Local/Temp/az-lift-cache.bak3_$(date +%s)" 2>/dev/null && echo "cache cleared"
bash scripts/m9_e2e/baseline_build.sh
RES=/c/rb/relift_simd.log
echo "=== probing solveLayoutReal (+simd128 (native v128, no i64-pair split)) $(date +%H:%M:%S) ===" >> "$RES"
sleep 3
AZ_PORT=8800 STAGE=2 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/marker-probe.mjs 2>&1 \
  | grep -iE "VERDICT|ERROR: RuntimeError|wasm-function\[|no hang|solve returned|resolveChain|diag-at-trap" | head -12 | tee -a "$RES"
echo "=== mcse probe DONE $(date +%H:%M:%S) ===" >> "$RES"
