#!/usr/bin/env bash
# text_size_job.sh <android|ios> <App> <scenario part> [<sizes>]: ONE runner job: the QA variant of <App> (the `mobile-e2e` phone library and the app against it, built from the tree
# the job runs in: a SHA or a branch) and the matrix session's scenario files whose path holds <scenario part> run once per SYSTEM TEXT SIZE on one emulator boot / one Simulator
# (AZ_QA_TEXT_SIZES: Android `settings put system font_scale`, iOS `simctl ui content_size`; the default size is put back at the end).
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/text_size_job.sh android AzMail text_size' --codegen --artifacts 'demo-out/qa-textsize/**
#   demo-out/results/*'
#   scripts/remote.sh run <full sha> macos-15 'bash scripts/ci/text_size_job.sh ios AzMail text_size' --codegen --artifacts 'demo-out/qa-textsize/**
#   demo-out/results/*'
# Sizes (3rd argument of the shell, default): Android "1.0 1.3 2.0", iOS "large accessibility-extra-extra-extra-large". Result: demo-out/qa-textsize/ (Android:
# swiftshader_indirect/matrix-text-<size>/, iOS: text-<size>/ with e2e-<platform>.json, e2e-matrix.md and the engine's frames in shots/).
# AzMail's text_size.json (examples/azul-mail/e2e/) asserts that nothing of the frame leaves the phone's window at any size and takes three pictures per size; until the engine follows the
# system text size every size gives the same pictures (the engine ignores it on both phones), which is the finding the sizes keep measuring.
set -uxo pipefail
platform="${1:?android or ios}"; app="${2:?an app name of the manifest, e.g. AzMail}"; part="${3:?a part of the scenario file path, e.g. text_size}"
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
case "$platform" in
  android)
    target=x86_64-linux-android; col=android-x86_64; sizes="${4:-1.0 1.3 2.0}"
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
    AZ_QA_TEXT_SIZES="$sizes" AZ_QA_MATRIX_ONLY="$part" timeout 3000 bash scripts/ci/android_qa.sh demo-out/qa-textsize "$apk" swiftshader_indirect "$app text size" matrix ;;
  ios)
    target=aarch64-apple-ios-sim; sizes="${4:-large accessibility-extra-extra-extra-large}"
    rustup target add "$target"
    features="$(python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios mobile-e2e ios-sim)" || exit 3
    echo "FEATURES mobile-e2e: $features"
    bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-ios-sim-mobile-e2e" || { echo "LIB mobile-e2e FAILED"; exit 3; }
    python3 scripts/demo_build.py ios --target "$target" --apps "$app" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant mobile-e2e || echo "demo_build.py returned non-zero"
    ls -la demo-out/apps
    zip="$(ls demo-out/apps/$app-e2e-ios-sim*.zip 2>/dev/null | head -n 1)"
    [ -n "$zip" ] || { echo "no e2e Simulator app of $app"; exit 3; }
    AZ_QA_TEXT_SIZES="$sizes" AZ_QA_MATRIX_ONLY="$part" AZ_QA_MATRIX_APPS="$zip" bash scripts/ci/ios_qa.sh demo-out/qa-textsize "$zip" "$app text size" matrix ;;
  *) echo "usage: text_size_job.sh android|ios <App> <scenario part>"; exit 2 ;;
esac
echo "SESSION text size $platform: exit $?"
exit 0
