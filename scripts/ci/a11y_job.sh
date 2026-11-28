#!/usr/bin/env bash
# a11y_job.sh <android|ios> [<App>]: ONE runner job of WP-A11Y: the QA variant of <App> (default AzWidgets: the `mobile-e2e` phone library and the app
# against it, built from the tree the job runs in: a SHA or a branch), then (1) the `a11y` session of the QA script (scripts/ci/android_qa.sh /
# ios_qa.sh): the "Display settings" scenario (examples/azul-widgets/e2e/a11y_display_settings_system.json) once per REAL system setting (Android:
# adb settings put; iOS: simctl ui) with the engine's frames, and a live sequence on the running app with the device's own pictures; (2) the
# override scenario (a11y_display_settings.json: set_system_accessibility says each option, the ones no switch of the emulator or the Simulator can
# set: bold text, reduced transparency, differentiate without colour) in the matrix session.
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/a11y_job.sh android' --codegen --artifacts 'demo-out/qa-a11y/**
#   demo-out/results/*'
#   scripts/remote.sh run <full sha> macos-15 'bash scripts/ci/a11y_job.sh ios' --codegen --artifacts 'demo-out/qa-a11y/**
#   demo-out/results/*'
# Result: demo-out/qa-a11y/ (Android: swiftshader_indirect/a11y-<set>/shots, a11y-live/NN-<set>.png, matrix/shots for the override run).
set -uxo pipefail
platform="${1:?android or ios}"; app="${2:-AzWidgets}"
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
case "$platform" in
  android)
    target=x86_64-linux-android; col=android-x86_64
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
    timeout 3000 bash scripts/ci/android_qa.sh demo-out/qa-a11y "$apk" swiftshader_indirect "$app a11y" a11y
    AZ_QA_MATRIX_ONLY="a11y_display_settings.json" timeout 1800 bash scripts/ci/android_qa.sh demo-out/qa-a11y-override "$apk" swiftshader_indirect "$app a11y override" matrix ;;
  ios)
    target=aarch64-apple-ios-sim
    rustup target add "$target"
    features="$(python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios mobile-e2e ios-sim)" || exit 3
    echo "FEATURES mobile-e2e: $features"
    bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-ios-sim-mobile-e2e" || { echo "LIB mobile-e2e FAILED"; exit 3; }
    python3 scripts/demo_build.py ios --target "$target" --apps "$app" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant mobile-e2e || echo "demo_build.py returned non-zero"
    ls -la demo-out/apps
    zip="$(ls demo-out/apps/$app-e2e-ios-sim*.zip 2>/dev/null | head -n 1)"
    [ -n "$zip" ] || { echo "no e2e Simulator app of $app"; exit 3; }
    AZ_QA_MATRIX_APPS="$zip" bash scripts/ci/ios_qa.sh demo-out/qa-a11y "$zip" "$app a11y" a11y
    AZ_QA_MATRIX_ONLY="a11y_display_settings.json" AZ_QA_MATRIX_APPS="$zip" bash scripts/ci/ios_qa.sh demo-out/qa-a11y-override "$zip" "$app a11y override" matrix ;;
  *) echo "usage: a11y_job.sh android|ios [<App>]"; exit 2 ;;
esac
echo "SESSION a11y $platform: exit $?"
exit 0
