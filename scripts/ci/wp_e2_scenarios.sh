#!/usr/bin/env bash
# wp_e2_scenarios.sh: the scenario files of AzMaps and AzCode (examples/azul-maps/e2e, examples/azul-code/e2e) on the desktop, in ONE job on ONE
# Linux runner: the desktop QA build of the apps (the shipped library with the debug server and the e2e executor, scripts/ci/desktop_e2e_build.sh),
# then each file once through scripts/ci/e2e_matrix.py desktop (the app mounts its window headless, runs the file, prints the report and exits),
# its pictures (the take_screenshot steps, numbered) kept in demo-out/wp-e2/scenarios/<file stem>/ and the verdicts in <file stem>-result/.
#
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/wp_e2_scenarios.sh' --codegen --artifacts 'demo-out/wp-e2/scenarios/**'
#
# WP_E2_APPS: the apps (default AzMaps AzCode); WP_E2_SKIP_BUILD=<dir>: the QA builds (*-e2e-linux) of an earlier run instead of building again;
# WP_E2_FILES: the files, space separated, instead of every file of the apps' e2e folders; WP_E2_LEGACY=1: afterwards AzMaps' three older
# debug-server drivers against the same QA build (scripts/azmaps_e2e.py: the map, the panel, the place, Route, the fold, the settings, the kept
# view, timed typing; azmaps_nav_e2e.py: public transport, a guided drive, an NMEA replay, the cache offline, a Takeout import;
# azmaps_routing_e2e.py: road routing on a Monaco extract, a plain file server and routing-tiles built here), their pictures and logs in
# demo-out/wp-e2/legacy/<driver>/; they need the network (Transitous, FOSSGIS' Valhalla, Geofabrik): a red one is read before it is believed.
# The exit code is 0 only when every file (and every driver asked for) passed.
set -uo pipefail
apps="${WP_E2_APPS:-AzMaps AzCode}"
out="$PWD/demo-out/wp-e2/scenarios"
mkdir -p "$out"

if [ -n "${WP_E2_SKIP_BUILD:-}" ]; then
  bins="$WP_E2_SKIP_BUILD"
else
  # shellcheck disable=SC2086
  bash scripts/ci/desktop_e2e_build.sh linux $apps || { echo "BUILD FAILED"; exit 3; }
  bins=demo-out/apps
fi
for f in "$bins"/*-e2e-linux; do [ -f "$f" ] && chmod +x "$f"; done

# the apps' own e2e folders: App name -> folder
folder_of() {
  case "$1" in
    AzMaps) echo examples/azul-maps/e2e ;;
    AzCode) echo examples/azul-code/e2e ;;
    *) echo "" ;;
  esac
}

bad=0
for app in $apps; do
  dir="$(folder_of "$app")"
  [ -n "$dir" ] && [ -d "$dir" ] || { echo "no scenario folder for $app"; continue; }
  bin="$bins/$app-e2e-linux"
  [ -f "$bin" ] || { echo "no $bin"; bad=1; continue; }
  files="${WP_E2_FILES:-$(ls "$dir"/*.json)}"
  for file in $files; do
    case "$file" in "$dir"/*) ;; *) continue ;; esac
    stem="$(basename "$file" .json)"
    mkdir -p "$out/$stem"
    echo "=== $app $stem ==="
    AZ_E2E_SHOT_DIR="$out/$stem" python3 scripts/ci/e2e_matrix.py desktop --only "$stem" --bin "$app=$bin" --out "$out/$stem-result"
    code=$?
    # the matrix always exits 0: the verdicts are in its JSON
    python3 - "$out/$stem-result/e2e-desktop.json" <<'PY' || code=1
import json
import sys

try:
    records = json.load(open(sys.argv[1]))
except (OSError, ValueError) as why:
    print(f"no verdicts: {why}")
    sys.exit(1)
for r in records:
    print(f"{r['verdict']:<11} {r['test'][:90]}  {r.get('detail', '')[:200]}")
sys.exit(1 if not records or any(r["verdict"] not in ("PASS", "SKIP", "XFAIL", "PENDING") for r in records) else 0)
PY
    echo "=== $app $stem: exit $code ==="
    [ "$code" -eq 0 ] || bad=1
    ls "$out/$stem" | head -40
  done
done

if [ -n "${WP_E2_LEGACY:-}" ]; then
  bin="$bins/AzMaps-e2e-linux"
  ldir="$PWD/demo-out/wp-e2/legacy"
  mkdir -p "$ldir"
  legacy() {   # legacy <name> <seconds> <command...>: one driver, its tail printed, a non-zero exit counted
    local name="$1" seconds="$2"
    shift 2
    echo "=== legacy $name ==="
    mkdir -p "$ldir/$name"
    timeout "$seconds" "$@" --out "$ldir/$name" > "$ldir/$name/driver.log" 2>&1
    local code=$?
    tail -25 "$ldir/$name/driver.log"
    echo "=== legacy $name: exit $code ==="
    [ "$code" -eq 0 ] || bad=1
  }
  if [ -f "$bin" ]; then
    chmod +x "$bin"
    legacy azmaps_e2e 900 python3 scripts/azmaps_e2e.py --bin "$bin"
    legacy azmaps_nav_e2e 900 python3 scripts/azmaps_nav_e2e.py --bin "$bin"
    if cargo build --release -p azul-routing --bin routing-tiles > "$ldir/routing-tiles-build.log" 2>&1; then
      legacy azmaps_routing_e2e 900 python3 scripts/azmaps_routing_e2e.py --bin "$bin" --routing-tiles-bin target/release/routing-tiles --download
    else
      echo "routing-tiles did not build:"
      tail -20 "$ldir/routing-tiles-build.log"
      bad=1
    fi
  else
    echo "no $bin for the legacy drivers"
    bad=1
  fi
fi
exit "$bad"
