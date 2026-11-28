#!/usr/bin/env bash
# audit_job.sh android <App>: ONE runner job: the QA variant of <App> (the `mobile-e2e` phone library and the app against it, built from the tree the job runs in: a SHA or a branch) and
# scripts/ci/android_qa.sh's `audit` session on it: the phone-fit audit with REAL touches (scripts/ci/mobile_e2e.py --audit-only): the first picture, portrait and landscape layout audits, the dark mode,
# the keyboard (a real touch on the app's first text field), and the app's own script when the APK's name has one (AzPaint paint, AzWriter writer, AzMaps maps, AzMeet meet).
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/audit_job.sh android AzMaps' --codegen --artifacts 'demo-out/qa-audit/**
#   demo-out/results/*'
# Result: demo-out/qa-audit/swiftshader_indirect/audit/<apk name>/ (layout-audit.md and .json, the dumps, the pictures per orientation, the app's script's pictures and verdicts).
# The iPhone twin of an app's pictures is scripts/ci/text_size_job.sh ios <App> <scenario part> with AZ_QA_MATRIX_RECORD=1 (the matrix session and the Simulator's screen recording).
set -uxo pipefail
platform="${1:?android}"; app="${2:?an app name of the manifest, e.g. AzMaps}"
[ "$platform" = android ] || { echo "audit_job.sh: android only (the iPhone twin: text_size_job.sh ios)"; exit 2; }
target=x86_64-linux-android; col=android-x86_64
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
  python3 -m pip install --quiet --user tomli || true
fi
export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
rustup target add "$target"
features="$(python3 scripts/demo_apps.py features "$col" mobile-e2e)"
echo "FEATURES mobile-e2e: $features"
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-mobile-e2e" || { echo "LIB mobile-e2e FAILED"; exit 3; }
python3 scripts/demo_build.py android --target "$target" --apps "$app" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant mobile-e2e || echo "demo_build.py returned non-zero"
ls -la demo-out/apps
apk="$(ls demo-out/apps/$app-e2e-$col.apk 2>/dev/null | head -n 1)"
[ -n "$apk" ] || { echo "no e2e APK of $app"; exit 3; }
timeout 3300 bash scripts/ci/android_qa.sh demo-out/qa-audit "$apk" swiftshader_indirect "$app audit" audit
echo "SESSION audit $app: exit $?"
exit 0
