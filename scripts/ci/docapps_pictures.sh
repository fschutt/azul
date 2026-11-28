#!/usr/bin/env bash
# docapps_pictures.sh: the host pictures of the three document apps (AzWriter, AzSheets, AzShow) in one job: the desktop QA build of the apps
# (the shipped libazul plus the debug server and the e2e executor, scripts/ci/desktop_e2e_build.sh) over the bindings, and every scenario file
# of examples/azul-{writer,sheets,show}/e2e run headless through the e2e matrix, each take_screenshot (a CPU render of the window at rest)
# copied to its own folder.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/docapps_pictures.sh' --codegen --artifacts 'demo-out/wpc-shots/**
#   demo-out/e2e-wpc/**
#   demo-out/results/*.json'
#
# DOCAPPS_FILES names the scenario files to run (stems, space separated; none named = every file of the three apps); DOCAPPS_NOBUILD=1 skips
# the build (the apps of an earlier step are in demo-out/apps). Steps do not stop each other; the last lines are one `STEP <name> <exit>`
# each, the exit code is 0 only when every step passed and every scenario verdict is PASS or SKIP.
set -uo pipefail
names=()
codes=()
step() {
  local name="$1"
  shift
  echo "=== STEP $name: $* ==="
  "$@"
  local code=$?
  names+=("$name")
  codes+=("$code")
  echo "=== STEP $name: exit $code ==="
  return 0
}

if [ "${DOCAPPS_NOBUILD:-0}" != "1" ]; then
  step build bash scripts/ci/desktop_e2e_build.sh linux AzWriter AzSheets AzShow
fi

mkdir -p demo-out/wpc-shots demo-out/e2e-wpc
# <folder> <package>: the host app of every scenario file of the folder
for pair in azul-writer:AzWriter azul-sheets:AzSheets azul-show:AzShow; do
  folder="${pair%%:*}"
  app="${pair##*:}"
  for file in examples/"$folder"/e2e/*.json; do
    [ -e "$file" ] || continue
    stem="$(basename "$file" .json)"
    if [ -n "${DOCAPPS_FILES:-}" ] && ! echo " ${DOCAPPS_FILES} " | grep -q " ${stem} "; then
      continue
    fi
    mkdir -p "demo-out/wpc-shots/$stem"
    step "e2e_$stem" env AZ_E2E_SHOT_DIR="$PWD/demo-out/wpc-shots/$stem" \
      python3 scripts/ci/e2e_matrix.py desktop --repo . --bin "$app=demo-out/apps/$app-e2e-linux" --only "$stem" --out "demo-out/e2e-wpc/$stem"
  done
done
# the matrix exits 0 on a FAIL verdict: read them
step verdicts python3 scripts/ci/tabs_verdicts.py demo-out/e2e-wpc

echo
bad=0
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
find demo-out/wpc-shots -name '*.png' | sort | head -80
exit "$bad"
