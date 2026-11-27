#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): one DESKTOP leg of the demos pipeline on one runner, with the scripts the workflow runs, to find
# what breaks BEFORE the full run: the shipped libazul once (scripts/demo_lib.sh), then a few apps against it (scripts/demo_build.py).
#   bash scripts/ci/exp_desktop_leg.sh linux          Ubuntu: also the Python tests of the lint job and the start check under xvfb
#   bash scripts/ci/exp_desktop_leg.sh windows        Windows (Git Bash): the MSVC library azul.lib and the apps
#   bash scripts/ci/exp_desktop_leg.sh macos-intel    macOS: the x86_64 library and apps built on an Apple Silicon runner
# Everything lands in demo-out/ (results/*.json, libs, logs/, plan.txt, tests.txt). The apps are DEMO_EXP_APPS (default six that differ:
# the plain one, the one that may compile azul-core, the one that needs the debug server, the camera and audio one, the terminal, the painter).
set -uxo pipefail
leg="${1:?linux | windows | macos-intel}"
apps="${DEMO_EXP_APPS:-AzWidgets AzMail AzBuilder AzMeet AzTerm AzPaint}"
mkdir -p demo-out/lib demo-out/apps demo-out/results
status=0

case "$leg" in
  linux)
    sudo apt-get update -qq || true
    sudo apt-get install -y clang libgl1-mesa-dev libx11-dev libv4l-dev xvfb libgl1-mesa-dri fonts-dejavu-core
    lib_file=libazul.a; target=""; deploy=""
    ;;
  windows)
    if ! command -v gcc >/dev/null 2>&1; then
      if [ -x /c/msys64/mingw64/bin/gcc.exe ]; then export PATH="/c/msys64/mingw64/bin:$PATH"; else choco install mingw --version 16.1.0 -y --no-progress || true; fi
    fi
    lib_file=azul.lib; target=""; deploy=""
    ;;
  macos-intel)
    brew install coreutils || true
    rustup target add x86_64-apple-darwin
    export MACOSX_DEPLOYMENT_TARGET=10.12
    lib_file=libazul.a; target="x86_64-apple-darwin"; deploy="10.12"
    ;;
  *) echo "unknown leg $leg" >&2; exit 2 ;;
esac

python3 -m pip install --quiet --user tomli || true   # a runner's Python 3.10 has no tomllib
python3 scripts/demo_apps.py plan | tee demo-out/plan.txt

if [ "$leg" = linux ]; then
  # the tests the lint job of rust.yml runs, here on Linux (the checks of the manifest, the scripts, the gates)
  {
    for t in "scripts/demo_apps.py check" scripts/test_demo_apps.py scripts/test_prune_dead_release_links.py scripts/test_preflight_contracts.py \
             scripts/test_android_elf_check.py scripts/test_ios_symbol_gate.py; do
      python3 $t && echo "TEST OK: $t" || { echo "TEST FAILED: $t"; status=1; }
    done
  } 2>&1 | tee demo-out/tests.txt
fi

start=$(date +%s)
bash scripts/demo_lib.sh desktop "$target" dist "demo-out/lib" || { echo "LIB FAILED"; exit 1; }
echo "LIB SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
args=(desktop --leg "$leg" --apps "$apps" --lib "demo-out/lib/$lib_file" --out demo-out/apps --results demo-out/results)
[ -n "$target" ] && args+=(--target "$target")
[ -n "$deploy" ] && args+=(--deployment-target "$deploy")
[ "$leg" = linux ] && args+=(--run-check)
python3 scripts/demo_build.py "${args[@]}"
echo "APPS SECONDS: $(( $(date +%s) - start ))"
ls -la demo-out/apps demo-out/lib || true
for f in demo-out/results/*.json; do
  python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r.get("app"), r.get("column"), r.get("status"), r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:300])' "$f" || true
done
exit "$status"
