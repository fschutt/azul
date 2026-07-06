#!/usr/bin/env bash
# DECISIVE TEST: rebuild dll + cold relift with AZ_LLC_O0=1 → the per-fn wasm BACKEND
# (llc) runs at -O0 (no regalloc/scheduling/peephole opt) while the IR is still opt -O2.
# Splits the class-B solveLayoutReal OOB:
#   trap DISAPPEARS at backend -O0  ⇒ LLVM-17 wasm-BACKEND codegen miscompile
#                                     (all SSE semantics verified correct; +simd128 didn't fix it)
#   trap PERSISTS at backend -O0    ⇒ non-SSE remill semantic / instruction-selection bug
# NOTE: manual lift-cache clear (AZ_LIFT_CACHE_CLEAR only wipes scratch, not the lift cache).
set -o pipefail
cd /c/Users/felix/Development/azul
export AZ_LLC_O0=1
mv /c/Users/felix/AppData/Local/Temp/az-lift-cache "/c/Users/felix/AppData/Local/Temp/az-lift-cache.bak_llco0_$(date +%s)" 2>/dev/null && echo "lift-cache cleared"
bash scripts/m9_e2e/baseline_build.sh
RES=/c/rb/relift_llco0.log
echo "=== probing solveLayoutReal (wasm BACKEND -O0) $(date +%H:%M:%S) ===" >> "$RES"
sleep 3
AZ_PORT=8800 STAGE=2 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/marker-probe.mjs 2>&1 \
  | grep -iE "VERDICT|ERROR: RuntimeError|wasm-function\[|no hang|solve returned|resolveChain|diag-at-trap|RIP=" | head -14 | tee -a "$RES"
echo "=== llco0 probe DONE $(date +%H:%M:%S) ===" >> "$RES"
