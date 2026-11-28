#!/usr/bin/env bash
# e2e_matrix_desktop.sh: the desktop column of the e2e matrix (scripts/ci/e2e_matrix.py): the files that mount their own DOM run in process,
# headless, in `azul-doc e2e <file> --keep-going` (the executor of an app without an app); the others are marked with the reason they have no
# desktop host here. AzWidgets with the executor compiled in (`cargo build -p AzWidgets --features e2e`) is tried too and used when it links.
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/e2e_matrix_desktop.sh' --codegen --artifacts 'demo-out/e2e-matrix-desktop/**'
set -uo pipefail
doc=target/release/azul-doc
[ -x "$doc" ] || cargo build -r -p azul-doc || { echo "azul-doc did not build"; exit 3; }
bins=()
if cargo build -p AzWidgets --features e2e; then bins+=(--bin "AzWidgets=target/debug/AzWidgets"); else echo "AzWidgets with the e2e feature did not build: its files stay 'no desktop host'"; fi
python3 scripts/ci/e2e_matrix.py desktop --repo . --out demo-out/e2e-matrix-desktop --azul-doc "$doc" ${bins[@]+"${bins[@]}"}
