#!/usr/bin/env bash
# desktop_e2e_build.sh: the QA builds of the desktop apps in ONE job on ONE runner: the artifact-only library [lib.desktop-e2e] of
# scripts/demo_apps.toml (the shipped dist features plus the engine's debug server, E2E scripting and test hooks) and the apps that say
# `e2e = true`, linked against it as <Name>-e2e-<platform> (AzWidgets-e2e-linux), for the E2E runs of the desktop column.
#
#   bash scripts/ci/desktop_e2e_build.sh [linux | macos-arm64] [<App> ...]        default: linux, every app the plan names for it
#
# Why a library of its own: an app's `azul = link-dynamic` leaves the Az* symbols to the library, and a feature that compiles the engine
# into the app (azul/e2e-scripting) neither links nor passes the engine gate (scripts/demo_gate.py): `cargo build -p AzWidgets --features
# e2e` is not a recipe that works. The library carries the marker AZLIN-BUILD-MARKER:test-hooks and scripts/demo_lib.sh checks it both
# ways (the shipped desktop library must not hold it, this one must). Nothing built here is ever published.
#
# Everything lands in demo-out/: apps/<App>-e2e-<platform>, results/*.json (with "variant": "desktop-e2e"), libs/*/libazul.info.json
# (seconds and bytes), plan.txt. About 25 minutes for the library on a runner (Linux), a minute for each app.
#
# On a runner, from any ref that has this file:
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/desktop_e2e_build.sh linux' --codegen --artifacts 'demo-out/apps/*
#   demo-out/results/*
#   demo-out/libs/*/libazul.info.json
#   demo-out/plan.txt'
set -euxo pipefail
leg="${1:-linux}"
[ $# -gt 0 ] && shift
case "$leg" in
  linux | macos-arm64) ;;
  *) echo "usage: desktop_e2e_build.sh [linux | macos-arm64] [<App> ...]" >&2; exit 2 ;;
esac
variant=desktop-e2e

# a runner is disposable; nothing is deleted anywhere else
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  case "$leg" in
    linux)
      sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL /usr/local/lib/android 2>/dev/null || true
      sudo apt-get update -qq || true
      sudo apt-get install -y clang libgl1-mesa-dev libx11-dev libv4l-dev xvfb libgl1-mesa-dri fonts-dejavu-core
      ;;
    macos-arm64) brew install coreutils || true ;;
  esac
  python3 -m pip install --quiet --user tomli || true   # a runner's Python 3.10 has no tomllib
fi

mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt

# what the plan says for this leg: lib file name, target, deployment target, the apps of the QA build ("|" separates: a field may be empty)
IFS='|' read -r lib_file target deploy plan_apps < <(python3 - "$leg" "$variant" <<'PY'
import json, sys
leg, variant = sys.argv[1:]
plan = {}
for line in open("demo-out/plan.txt"):
    key, _, value = line.rstrip("\n").partition("=")
    plan[key] = value
for key in ("apps_linux", "apps_macos", "apps_windows"):
    for e in json.loads(plan[key])["include"]:
        if e["leg"] == leg and e["variant"] == variant:
            print("|".join([e["lib_file"], e["target"], e["deployment_target"], e["apps"]]))
            sys.exit(0)
sys.exit("the plan has no QA build for " + leg)
PY
)
apps="${*:-$plan_apps}"
[ -n "$deploy" ] && export MACOSX_DEPLOYMENT_TARGET="$deploy"

features="$(python3 scripts/demo_apps.py features "$leg" "$variant")"
echo "== $variant ($leg): $features"
start=$(date +%s)
bash scripts/demo_lib.sh desktop "$target" "$features" "demo-out/libs/demolib-$leg-$variant"
echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"

start=$(date +%s)
args=(desktop --leg "$leg" --apps "$apps" --lib "demo-out/libs/demolib-$leg-$variant/$lib_file" --out demo-out/apps
      --results demo-out/results --variant "$variant")
[ -n "$target" ] && args+=(--target "$target")
[ -n "$deploy" ] && args+=(--deployment-target "$deploy")
python3 scripts/demo_build.py "${args[@]}"
echo "APPS SECONDS: $(( $(date +%s) - start ))"
ls -la demo-out/apps demo-out/libs/*
for f in demo-out/results/*.json; do
  python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["column"], r.get("variant", ""), r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:300])' "$f"
done
