#!/usr/bin/env bash
# PRIMARY-GOAL TEST (2026-08-13): does the trailofbits upstream merge (MOVD/MOVZX/MOVSX
# zero/sign-extension fixes + 73 other x86 commits) resolve the solveLayoutReal wild-
# garbage OOB? Rebuild dll on master (picks up az_mark/eventloop/AZ_LLC_O1 build fixes)
# + COLD relift with the freshly-rebuilt amd64.bc + full-cycle.js + STAGE=2 solve probe.
# NOTE: AZ_LLC_O1 deliberately UNSET (test the merge at normal -O2). MUST manually clear
# the lift cache — it keys on native-bytes+lifter-mtime, NOT amd64.bc, so the .bc change
# alone would be masked by a cache hit.
set -o pipefail
cd /c/Users/felix/Development/azul
mv /c/Users/felix/AppData/Local/Temp/az-lift-cache "/c/Users/felix/AppData/Local/Temp/az-lift-cache.bak_merge_$(date +%s)" 2>/dev/null && echo "lift-cache cleared"
bash scripts/m9_e2e/baseline_build.sh
RES=/c/rb/relift_merge.log
echo "=== probing solveLayoutReal (upstream-merge amd64.bc) $(date +%H:%M:%S) ===" >> "$RES"
sleep 3
AZ_PORT=8800 STAGE=2 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/marker-probe.mjs 2>&1 \
  | grep -iE "VERDICT|ERROR: RuntimeError|wasm-function\[|no hang|solve returned|resolveChain|diag-at-trap|RIP=|unsupported|remill_error" | head -16 | tee -a "$RES"
echo "=== merge probe DONE $(date +%H:%M:%S) ===" >> "$RES"
