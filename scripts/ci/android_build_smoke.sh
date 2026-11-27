#!/usr/bin/env bash
# android_build_smoke.sh: build the NAMED phone apps for Android x86_64 and boot-test them on the emulator, in ONE job on ONE Linux runner:
# the library variant each app names (mobile, mobile-iroh), the apps against them, then scripts/ci/android_smoke.sh with the buttons and
# texts of the manifest ([app.smoke]: tap, expect, after_tap, after_tap_gone, probe_tap, probe_after).
#
#   bash scripts/ci/android_build_smoke.sh <App> [<App> ...]            e.g. AzCompanion, or AzWidgets AzMaps
#   AZ_SMOKE_BOTH_WAYS=1 bash scripts/ci/android_build_smoke.sh <App>   also a second boot test without the probe (smoke-noprobe/)
#
# Everything lands in demo-out/: apps/*.apk, results/*.json (build), libs/*/libazul.info.json, smoke/<App>-android-x86_64/ (baseline.png,
# screenshot*.png, logcat*.txt, ui-*.xml, tap.json, result.json: the one pass line and its reason) and smoke/summary.json.
#
# On a runner, from any ref that has this file:
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_build_smoke.sh AzCompanion' --codegen --artifacts 'demo-out/apps/*
#   demo-out/results/*
#   demo-out/libs/*/libazul.info.json
#   demo-out/smoke/**
#   demo-out/plan.txt'
# A cold library is 6 to 11 minutes, an app a minute, the boot test about a minute per app after the emulator's 5 to 8 minutes.
set -euxo pipefail
[ $# -gt 0 ] || { echo "usage: android_build_smoke.sh <App> [<App> ...]" >&2; exit 2; }
apps="$*"
target=x86_64-linux-android
col=android-x86_64

# a runner is disposable; nothing is deleted anywhere else
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
  python3 -m pip install --quiet --user tomli || true   # the runner's Python 3.10 has no tomllib
fi

export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
rustup target add "$target"

mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan | tee demo-out/plan.txt

# the libraries the named apps link, each once
variants=""
for app in $apps; do
  variant="$(python3 scripts/demo_apps.py mobile-lib "$app")"
  case " $variants " in *" $variant "*) ;; *) variants="$variants $variant" ;; esac
done
for variant in $variants; do
  features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
  echo "== $variant: $features"
  start=$(date +%s)
  bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant"
  echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"
done

start=$(date +%s)
python3 scripts/demo_build.py android --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant ""
echo "APPS SECONDS: $(( $(date +%s) - start ))"
ls -la demo-out/apps
for f in demo-out/results/*.json; do
  python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["column"], r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:300])' "$f"
done

# the boot test with what the manifest says each app must show and do
AZ_SMOKE_TAPS="$(grep '^smoke_taps=' demo-out/plan.txt | cut -d= -f2-)"
AZ_SMOKE_EXPECTS="$(grep '^smoke_expects=' demo-out/plan.txt | cut -d= -f2-)"
AZ_SMOKE_AFTER="$(grep '^smoke_after=' demo-out/plan.txt | cut -d= -f2-)"
export AZ_SMOKE_TAPS AZ_SMOKE_EXPECTS AZ_SMOKE_AFTER
bash scripts/ci/android_smoke.sh demo-out/smoke 20 demo-out/apps/*.apk || echo "SMOKE: exit $?"
for f in demo-out/smoke/*/result.json; do
  python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], "PASS" if r["pass"] else "FAIL", "-", r["reason"])' "$f"
done
# AZ_SMOKE_BOTH_WAYS=1: the same test once more in a fresh emulator session WITHOUT the first press with a short effect (the probe), so
# the main key is pressed first in a fresh process: a key that works then and fails after the probe means the toast layer eats a press
if [ "${AZ_SMOKE_BOTH_WAYS:-}" = "1" ]; then
  AZ_SMOKE_NO_PROBE=1 bash scripts/ci/android_smoke.sh demo-out/smoke-noprobe 20 demo-out/apps/*.apk || echo "SMOKE (no probe): exit $?"
  for f in demo-out/smoke-noprobe/*/result.json; do
    python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print("NO PROBE:", r["app"], "PASS" if r["pass"] else "FAIL", "-", r["reason"])' "$f"
  done
fi
