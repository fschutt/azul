#!/usr/bin/env bash
# e2e_job.sh <android|ios> <App> "<scenario file> [<scenario file> ...]": ONE runner job: the QA variant of <App> (the `mobile-e2e` phone library and the app against it, built from the tree the job runs in:
# a SHA or a branch) and the e2e SESSION on it: the app is started with its debug server on and each scenario file's tests run THROUGH THE SERVER (`run_e2e_tests`), whose answer keeps the RESPONSE of every step
# (get_animations, get_display_list, get_node_css_properties, ...): demo-out/qa-e2e/**/logs/scenario-<file stem>.json, the pictures of its take_screenshot steps as scenario-<stem>-shot-NNN.png, and the log.
# AZ_QA_EXTRA_ENV="KEY=VALUE;KEY=VALUE" hands more knobs to the app (TABS: AZ_RECONCILE_DEBUG=1 prints the [reconcile] lines: Android reconcile-<stem>.txt, iOS console-N.err.txt).
#   scripts/remote.sh run <full sha> ubuntu-22.04 'AZ_QA_EXTRA_ENV=AZ_RECONCILE_DEBUG=1 bash scripts/ci/e2e_job.sh android AzWidgets examples/azul-widgets/e2e/tab_label_frames.json' --codegen --artifacts 'demo-out/qa-e2e/**
#   !demo-out/qa-e2e/**/*.mp4'
#   scripts/remote.sh run <full sha> macos-15 'AZ_QA_EXTRA_ENV=AZ_RECONCILE_DEBUG=1 bash scripts/ci/e2e_job.sh ios AzWidgets examples/azul-widgets/e2e/tab_label_frames.json' --codegen --artifacts '...'
set -uxo pipefail
platform="${1:?android or ios}"; app="${2:?an app name of the manifest, e.g. AzWidgets}"; files="${3:?the scenario file(s), space separated}"
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
    AZ_QA_SCENARIOS="$files" timeout 3000 bash scripts/ci/android_qa.sh demo-out/qa-e2e "$apk" swiftshader_indirect "$app e2e" e2e ;;
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
    AZ_QA_SCENARIOS="$files" bash scripts/ci/ios_qa.sh demo-out/qa-e2e "$zip" "$app e2e" e2e ;;
  *) echo "usage: e2e_job.sh android|ios <App> \"<files>\""; exit 2 ;;
esac
echo "SESSION e2e $platform: exit $?"
exit 0
