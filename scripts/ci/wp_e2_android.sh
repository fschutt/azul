#!/usr/bin/env bash
# wp_e2_android.sh: the QA variant of the apps of WP-E2 for the Android emulator in ONE Linux job: the `mobile-e2e` phone library (the debug
# server, the scenario executor, the test hooks) and the apps against it (scripts/demo_build.py android --variant mobile-e2e), then per app
#   1. the phone-fit audit (scripts/ci/qa_android_audit.sh: portrait and landscape, boxes over the edges, touch targets under 48 dp, the
#      system dark mode, the app's own script) in demo-out/wp-e2-android/audit-<App>/,
#   2. the scenario files (scripts/ci/android_qa.sh ... e2e, WP_E2_SCENARIOS: the files in repo paths, space separated) in
#      demo-out/wp-e2-android/e2e-<App>/: the debug server runs each file on the emulator and the pictures of its take_screenshot steps are kept.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/wp_e2_android.sh' --codegen --artifacts 'demo-out/wp-e2-android/**
#   demo-out/results/*'
#
# WP_E2_ANDROID_APPS: the apps (default AzMaps). WP_E2_AUDIT=0 skips the audit, WP_E2_SCENARIOS names the files (none: no scenario session).
# WP_E2_APKS=<dir>: the *-e2e-android-x86_64.apk files of an earlier run (a directory) instead of building them again.
set -uxo pipefail
apps="${WP_E2_ANDROID_APPS:-AzMaps}"
target=x86_64-linux-android
col=android-x86_64
out=demo-out/wp-e2-android
mkdir -p "$out" demo-out/libs demo-out/apps demo-out/results

if [ -z "${WP_E2_APKS:-}" ]; then
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
  python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt
  # the QA library of the first app (the apps of WP-E2 link the plain one: mobile-e2e)
  variant="$(python3 scripts/demo_apps.py e2e-lib "$(echo "$apps" | cut -d' ' -f1)")"
  features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
  echo "== $variant: $features"
  bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant" || { echo "LIB FAILED"; exit 3; }
  python3 scripts/demo_build.py android --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps \
    --results demo-out/results --variant "$variant" || echo "demo_build.py returned non-zero"
  ls -la demo-out/apps
  apk_dir=demo-out/apps
else
  apk_dir="$WP_E2_APKS"
fi

for app in $apps; do
  apk="$apk_dir/$app-e2e-$col.apk"
  [ -f "$apk" ] || { echo "no $apk"; continue; }
  if [ "${WP_E2_AUDIT:-1}" != 0 ]; then
    mkdir -p "$out/apk-$app" && cp "$apk" "$out/apk-$app/"
    AZ_QA_OUT="$out/audit-$app" timeout 1500 bash scripts/ci/qa_android_audit.sh "$out/apk-$app"
    echo "SESSION audit $app: exit $?"
    adb emu kill >/dev/null 2>&1 || true
    sleep 5
  fi
  if [ -n "${WP_E2_SCENARIOS:-}" ]; then
    AZ_QA_SCENARIOS="$WP_E2_SCENARIOS" timeout 2400 bash scripts/ci/android_qa.sh "$out/e2e-$app" "$apk" swiftshader_indirect "$app e2e" e2e
    echo "SESSION e2e $app: exit $?"
    adb emu kill >/dev/null 2>&1 || true
  fi
done
exit 0
