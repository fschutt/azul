#!/usr/bin/env bash
# android_build_app.sh: ONE phone app (and the one library it links) for Android in one job on one Linux runner, for the QA that
# needs a single APK (AzSelfTest: the self-test on the emulator or a phone) and not the 25 minutes of every library and app of
# android_build_all.sh.
#
#   bash scripts/ci/android_build_app.sh <App> [<rust-target>]     default x86_64-linux-android (the emulator's ABI)
#
# The library is the QA variant the manifest names for a qa_only app (scripts/demo_apps.py e2e-lib), or the library of the
# app (mobile-lib) for a shipped one; the APK is demo-out/apps/<App>-e2e-<column>.apk for a QA build (qa_only), else
# demo-out/apps/<App>-<column>.apk. On a runner, from any ref that has this file:
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_build_app.sh AzSelfTest' --codegen \
#       --artifacts 'demo-out/apps/*
#   demo-out/results/*'
set -euxo pipefail
app="${1:?the app: AzSelfTest, AzWidgets, ...}"
target="${2:-x86_64-linux-android}"
case "$target" in
  x86_64-linux-android) col=android-x86_64 ;;
  aarch64-linux-android) col=android-arm64 ;;
  *) echo "usage: android_build_app.sh <App> [x86_64-linux-android | aarch64-linux-android]" >&2; exit 2 ;;
esac

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
# a qa_only app is built against its QA library and names it as the build's variant; a shipped app is the plain build ("")
if python3 scripts/demo_apps.py qa-only "$app"; then
  variant="$(python3 scripts/demo_apps.py e2e-lib "$app")"
  build_variant="$variant"
else
  variant="$(python3 scripts/demo_apps.py mobile-lib "$app")"
  build_variant=""
fi
features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
echo "== $app against $variant: $features"
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant"
python3 scripts/demo_build.py android --target "$target" --apps "$app" --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant "$build_variant"
ls -la demo-out/apps
for f in demo-out/results/*.json; do
  python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["column"], r.get("variant", ""), r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:300])' "$f"
done
python3 scripts/android_elf_check.py "demo-out/libs/demolib-$col-$variant/libazul.so" demo-out/apps/*.apk
