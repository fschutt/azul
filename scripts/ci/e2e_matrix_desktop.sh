#!/usr/bin/env bash
# e2e_matrix_desktop.sh: the desktop column of the e2e matrix (scripts/ci/e2e_matrix.py): AzWidgets built WITH the e2e feature (the engine's scenario
# executor compiled into the app, as the manifest of the crate says) and run headless with AZ_E2E=<file> for every file that AzWidgets hosts.
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/e2e_matrix_desktop.sh' --codegen --artifacts 'demo-out/e2e-matrix-desktop/**'
set -uo pipefail
cargo build -p AzWidgets --features e2e || { echo "AzWidgets with the e2e feature did not build"; exit 3; }
bin=target/debug/AzWidgets
python3 scripts/ci/e2e_matrix.py desktop --repo . --out demo-out/e2e-matrix-desktop --bin "AzWidgets=$bin"
