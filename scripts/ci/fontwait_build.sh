#!/usr/bin/env bash
# fontwait_build.sh <android|ios>: ONE runner job that builds what the font-wait A/B measures for the tree it runs in (a SHA or a branch): the PLAIN phone library (`mobile`)
# and AzWidgets twice,
#   control  AzWidgets as it is (its CSS names `system:ui` and `system:monospace`: a stack that opens with a system family, so the first layout MUST wait for the font scan:
#            the negative control, build A and build B must be equal),
#   bundled  the same app with every `system:ui` / `system:monospace` of examples/azul-widgets/src replaced by `EB Garamond` (azul bundles it): the first layout may skip the wait
#            when the faces registered in memory draw every character of the DOM (the positive case; the `fonts:first_layout_wait` mark says `skipped` or `waited`: if some text still
#            opens with a system family, widen the replacement and build again).
# The bundled variant is built from an edited copy of the app's source IN THE RUNNER'S CHECKOUT (the job's own, thrown away with the job).
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/fontwait_build.sh android' --codegen --artifacts 'demo-out/fontwait/**'
#   scripts/remote.sh run <full sha> macos-15     'bash scripts/ci/fontwait_build.sh ios'     --codegen --artifacts 'demo-out/fontwait/**'
# Run it on the commit of build A and on the commit of build B, then scripts/ci/fontwait_measure.sh <platform> <run of A> <run of B> (both builds from ONE master: B = that master + the lever).
set -uxo pipefail
platform="${1:?android or ios}"
mkdir -p demo-out/libs demo-out/apps demo-out/results demo-out/fontwait
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt

build_apps() {   # build_apps <name suffix of the copy>: the AzWidgets app against the lib built above
  case "$platform" in
    android) python3 scripts/demo_build.py android --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" || return 1
             cp demo-out/apps/AzWidgets-android-x86_64.apk "demo-out/fontwait/AzWidgets-$1.apk" ;;
    ios)     python3 scripts/demo_build.py ios --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" || return 1
             z="$(ls demo-out/apps/AzWidgets-ios-sim*.zip 2>/dev/null | grep -v -- '-e2e-' | head -n 1)"; [ -n "$z" ] || return 1
             cp "$z" "demo-out/fontwait/AzWidgets-$1.zip" ;;
  esac
}

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
    features="$(python3 scripts/demo_apps.py features "$col" mobile)"
    echo "FEATURES mobile: $features"
    bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-mobile" || { echo "LIB mobile FAILED"; exit 3; } ;;
  ios)
    target=aarch64-apple-ios-sim
    rustup target add "$target"
    features="$(python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios mobile ios-sim)" || exit 3
    echo "FEATURES mobile: $features"
    bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-ios-sim-mobile" || { echo "LIB mobile FAILED"; exit 3; } ;;
  *) echo "usage: fontwait_build.sh android|ios"; exit 2 ;;
esac

build_apps control || { echo "the control app did not build"; exit 3; }
# the bundled variant: every system family of the app's own CSS is EB Garamond now
python3 - <<'PY'
import pathlib
changed = 0
for path in sorted(pathlib.Path("examples/azul-widgets/src").glob("*.rs")):
    text = path.read_text(encoding="utf-8")
    new = text.replace("font-family: system:monospace", "font-family: EB Garamond").replace("font-family: system:ui", "font-family: EB Garamond")
    if new != text:
        path.write_text(new, encoding="utf-8")
        changed += text.count("font-family: system:")
print("font-family replacements:", changed)
PY
grep -rn "system:ui\|system:monospace" examples/azul-widgets/src | head -20 || true
build_apps bundled || echo "the bundled variant did not build"
ls -la demo-out/fontwait
git rev-parse HEAD > demo-out/fontwait/commit.txt 2>/dev/null || true
exit 0
