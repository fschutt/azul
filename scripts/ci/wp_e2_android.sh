#!/usr/bin/env bash
# wp_e2_android.sh: ONE Linux runner job: apps of WP-E2 (default AzCode) on the Android emulator. It builds the phone libraries of the plan for
# android-x86_64 (`mobile-e2e`, and `mobile` with WP_E2_ANDROID_PLAIN=1: the shipped variant), the apps against them (scripts/demo_build.py: the
# 16 KiB page check of every library, the engine gate), then runs on the emulator
#   1. the phone-fit audit of the QA build (scripts/ci/qa_android_audit.sh: portrait and landscape, boxes over the edges, touch targets under 48 dp,
#      the engine's frame and the device's frame, the system dark mode, the German texts) and
#   2. the scenario files named in WP_E2_SCENARIOS (space separated repo paths: examples/azul-code/e2e/*.json) through scripts/ci/android_qa.sh's
#      `e2e` session (the debug server of the QA build driven from the runner, the engine's frames kept: AZ_E2E_SHOT_DIR).
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/wp_e2_android.sh' --codegen --artifacts 'demo-out/wp-e2-android/**
#   demo-out/results/*
#   demo-out/libs/*/libazul.info.json
#   demo-out/elf-check-*.txt
#   demo-out/plan.txt'
#
# WP_E2_ANDROID_APPS: the apps (space separated, default AzCode). WP_E2_ANDROID_SKIP_AUDIT=1: no audit session. WP_E2_ANDROID_PICK=<run id>:
# the APKs of an earlier run of this script (artifact remote-Linux, demo-out/apps/*) instead of building (a fix of the scenario files or of the
# scripts needs no new library). Sessions end at a time limit; the last lines are `SESSION <name>: exit <n> after <s> s`. Exit status 0 when the
# build stage passed.
set -euxo pipefail
target=x86_64-linux-android
col=android-x86_64
apps="${WP_E2_ANDROID_APPS:-AzCode}"
here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p demo-out/libs demo-out/apps demo-out/results

if [ -n "${WP_E2_ANDROID_PICK:-}" ]; then
  gh run download "$WP_E2_ANDROID_PICK" -n remote-Linux -D pick
  cp pick/demo-out/apps/*-e2e-android-x86_64.apk demo-out/apps/
else
  # a runner is disposable; nothing is deleted anywhere else
  if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
    sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
    python3 -m pip install --quiet --user tomli || true   # the runner's Python 3.10 has no tomllib
  fi
  df -h / | tail -n 1
  export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
  SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
  yes | "$SDKM" --licenses >/dev/null 2>&1 || true
  "$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
  export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
  export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
  rustup target add "$target"
  python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt

  variants="mobile-e2e"
  [ "${WP_E2_ANDROID_PLAIN:-}" = 1 ] && variants="mobile-e2e mobile"
  for variant in $variants; do
    features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
    echo "== $variant: $features"
    start=$(date +%s)
    bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant"
    echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"
  done
  start=$(date +%s)
  python3 scripts/demo_build.py android --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps \
    --results demo-out/results --variant mobile-e2e
  if [ "${WP_E2_ANDROID_PLAIN:-}" = 1 ]; then
    python3 scripts/demo_build.py android --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps \
      --results demo-out/results --variant ""
  fi
  echo "APPS SECONDS: $(( $(date +%s) - start ))"
  ls -la demo-out/apps
  for f in demo-out/results/*.json; do
    python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["column"], r.get("variant", ""), r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:300])' "$f"
  done
  python3 scripts/android_elf_check.py demo-out/libs/*/libazul.so | tee demo-out/elf-check-libs.txt
  python3 scripts/android_elf_check.py demo-out/apps/*.apk | tee demo-out/elf-check-apks.txt
fi

# ---- the emulator sessions: what ran is kept even when a later one is cut ----
set +e
session() {   # session <name> <seconds> <command...>
  local name="$1" limit="$2"
  shift 2
  echo "=== session $name (limit $limit s) $(date '+%H:%M:%S')"
  local start rc
  start=$(date +%s)
  timeout "$limit" "$@"
  rc=$?
  echo "SESSION $name: exit $rc after $(( $(date +%s) - start )) s"
  adb emu kill >/dev/null 2>&1 || true      # an emulator a cut session left behind would hold the next one's port
  sleep 5
}
mkdir -p demo-out/qa-apks
for a in $apps; do
  cp "demo-out/apps/$a-e2e-android-x86_64.apk" demo-out/qa-apks/ 2>/dev/null || echo "no QA APK of $a"
done
if [ "${WP_E2_ANDROID_SKIP_AUDIT:-}" != 1 ] && ls demo-out/qa-apks/*.apk >/dev/null 2>&1; then
  AZ_QA_OUT=demo-out/wp-e2-android/audit session audit 1500 bash "$here/qa_android_audit.sh" demo-out/qa-apks
fi
if [ -n "${WP_E2_SCENARIOS:-}" ]; then
  for a in $apps; do
    apk="demo-out/qa-apks/$a-e2e-android-x86_64.apk"
    [ -f "$apk" ] || continue
    AZ_QA_SCENARIOS="$WP_E2_SCENARIOS" session "e2e $a" 2400 bash "$here/android_qa.sh" "demo-out/wp-e2-android/e2e-$a" "$apk" swiftshader_indirect "$a" e2e
  done
fi
exit 0
