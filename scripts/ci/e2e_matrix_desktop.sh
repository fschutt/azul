#!/usr/bin/env bash
# e2e_matrix_desktop.sh [<dir of the desktop QA build's apps>]: the desktop column of the e2e matrix (scripts/ci/e2e_matrix.py).
#
#   - the files that mount their own DOM run in process, headless, in `azul-doc e2e <file> --keep-going` (the executor of an app without an app);
#   - the files that assert an app's DOM run in that app's binary of the desktop QA build (scripts/ci/desktop_e2e_build.sh: the shipped
#     libazul plus the debug server and the e2e executor, the apps AzWidgets-e2e-linux and AzWriter-e2e-linux), AZ_E2E=<file> on the headless
#     backend, when the directory is given;
#   - the rest is marked with the reason it has no desktop host.
# The scenarios take Chrome's numbers for 16px Arial, which azul draws in Liberation Sans on Linux: the fonts of the CI's layout tests are
# installed first, or a font-dependent line count is a verdict about the runner's fonts.
#
#   scripts/remote.sh run <ref> ubuntu-22.04 'gh run download <desktop qa build run id> -n remote-Linux -D desktop-qa; bash scripts/ci/e2e_matrix_desktop.sh desktop-qa/demo-out/apps' \
#       --codegen --artifacts 'demo-out/e2e-matrix-desktop/**'
set -uo pipefail
apps_dir="${1:-}"
if command -v apt-get >/dev/null 2>&1; then
  { sudo apt-get update -qq && sudo apt-get install -y -qq fonts-liberation fonts-dejavu-core; } >/dev/null 2>&1 \
    && echo "fonts of the CI's layout tests installed" || echo "the fonts of the CI's layout tests could not be installed"
fi
doc=target/release/azul-doc
[ -x "$doc" ] || cargo build -r -p azul-doc || { echo "azul-doc did not build"; exit 3; }
bins=()
if [ -n "$apps_dir" ] && [ -d "$apps_dir" ]; then
  for app in "$apps_dir"/*-e2e-linux; do
    [ -f "$app" ] || continue
    chmod +x "$app"
    bins+=(--bin "$(basename "$app" | cut -d- -f1)=$app")
    echo "desktop host: $app"
  done
else
  echo "no desktop QA build given: the files that assert an app's DOM stay 'no desktop host'"
fi
python3 scripts/ci/e2e_matrix.py desktop --repo . --out demo-out/e2e-matrix-desktop --azul-doc "$doc" ${bins[@]+"${bins[@]}"}
