#!/usr/bin/env bash
# TEST: rebuild dll (with env-driven llc codegen flags) + cold relift with
# AZ_LLC_NOSTOREMERGE=1 (disables DAG store-merging + load-op-store-width reduction)
# → does the class-B i128/fat-pointer solveLayoutReal OOB disappear?
set -o pipefail
cd /c/Users/felix/Development/azul
export AZ_LLC_NOSTOREMERGE=1
bash scripts/m9_e2e/baseline_build.sh
RES=/c/rb/relift_nostore.log
echo "=== baseline_build done; probing solveLayoutReal $(date +%H:%M:%S) ===" >> "$RES"
sleep 3
AZ_PORT=8800 STAGE=2 "/c/Users/felix/tools/node/node.exe" scripts/m9_e2e/marker-probe.mjs 2>&1 \
  | grep -iE "VERDICT|ERROR: RuntimeError|wasm-function\[|no hang|solve returned|resolveChain" | head -10 >> "$RES" 2>&1
echo "=== nostore probe DONE $(date +%H:%M:%S) ===" >> "$RES"
