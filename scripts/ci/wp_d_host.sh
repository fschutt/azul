#!/usr/bin/env bash
# wp_d_host.sh: AzERP on the HOST in one runner job (WP-D of the UI refactor): the desktop QA build of AzERP
# (scripts/ci/desktop_e2e_build.sh: the shipped libazul plus the engine's debug server and e2e executor, the app linked
# against it as AzERP-e2e-linux) and what runs on it, every step with its REAL exit code.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'WPD_ONLY=build,audit bash scripts/ci/wp_d_host.sh' --codegen --artifacts 'demo-out/wp-d/**
#   demo-out/results/*'
#
# Steps (WPD_ONLY names them, comma separated; none named = all of them; a step needs the build before it):
#   build      desktop_e2e_build.sh linux AzERP (about 25 minutes cold for the library)
#   audit      scripts/wp_d_audit.py: the named pictures of the OLD window (BEFORE): a first launch on an empty folder, the sample
#              register, every page, at 1280x800, Pixel 8, iPhone 16 Pro, 360x640, landscape; light and dark; the phone-fit audits
#   e2e_old    scripts/azerp_e2e.py: the old driver (new asset, check-out, maintenance, delete, export, reports, run, restart + import)
#   scenarios  every examples/azul-erp/e2e/*.json in the app's binary: AZ_E2E=<file>, the headless backend, the engine's frames
#              of each file in demo-out/wp-d/scenarios/<file stem>/ (AZ_E2E_SHOT_DIR)
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the
# script is 0 only when every step that ran passed.
set -uo pipefail
only="${WPD_ONLY:-}"
names=()
codes=()
out=demo-out/wp-d
app=demo-out/apps/AzERP-e2e-linux
mkdir -p "$out"

run() {
  local name="$1"
  shift
  case ",$only," in
    ",,") ;;
    *",$name,"*) ;;
    *) return 0 ;;
  esac
  echo "=== STEP $name: $*"
  "$@"
  local code=$?
  names+=("$name")
  codes+=("$code")
  echo "=== STEP $name: exit $code"
}

build() {
  bash scripts/ci/desktop_e2e_build.sh linux AzERP 2>&1 | tee "$out/build.txt"
  local code="${PIPESTATUS[0]}"
  [ -x "$app" ] || { echo "no $app"; return 3; }
  return "$code"
}
run build build

audit() {
  [ -x "$app" ] || { echo "no $app: the build step has not run"; return 3; }
  timeout 2400 python3 scripts/wp_d_audit.py --bin "$app" --out "$out/audit" 2>&1 | tee "$out/audit.txt"
  return "${PIPESTATUS[0]}"
}
run audit audit

e2e_old() {
  [ -x "$app" ] || { echo "no $app: the build step has not run"; return 3; }
  mkdir -p "$out/e2e-old"
  timeout 1500 python3 scripts/azerp_e2e.py --bin "$app" --out "$out/e2e-old" --keep 2>&1 | tee "$out/e2e-old.txt"
  return "${PIPESTATUS[0]}"
}
run e2e_old e2e_old

scenarios() {
  [ -x "$app" ] || { echo "no $app: the build step has not run"; return 3; }
  local code=0 f stem
  for f in examples/azul-erp/e2e/*.json; do
    [ -f "$f" ] || continue
    stem="$(basename "$f" .json)"
    mkdir -p "$out/scenarios/$stem"
    # one fresh data folder per file: a scenario that wants the sample loads it with the app's own action
    AZ_E2E="$PWD/$f" AZ_BACKEND=headless AZ_LOG=off AZ_E2E_SHOT_DIR="$PWD/$out/scenarios/$stem" \
      AZERP_TODAY=2026-10-03 timeout 600 "$app" --data-dir "$PWD/$out/scenarios/$stem/data" --language en \
      > "$out/scenarios/$stem.txt" 2>&1
    local one=$?
    echo "scenario $stem: exit $one"
    grep -E "^test |test result" "$out/scenarios/$stem.txt" | head -40
    [ "$one" -eq 0 ] || code=1
  done
  return "$code"
}
run scenarios scenarios

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: WPD_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"
