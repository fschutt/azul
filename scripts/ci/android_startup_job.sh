#!/usr/bin/env bash
# android_startup_job.sh: ONE runner job for the startup work: the plain phone library of the tree it runs in, AzWidgets against it, and the startup series
# (ten cold starts with the engine's own timeline, the floor APK, one start with the layout profile) of THAT build, so a fix that landed after an AFTER
# build can be measured without cutting all the apps again (about 25 minutes of library, 3 of app, 12 of series).
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/android_startup_job.sh' --codegen --artifacts 'demo-out/qa/**
#   demo-out/results/*
#   demo-out/libs/*/libazul.info.json'
# The result is demo-out/qa/qa.md and logs/ (the median start's [startup] table, `fonts:cache` marks of every start in the series log).
set -euxo pipefail
target=x86_64-linux-android
col=android-x86_64
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
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
features="$(python3 scripts/demo_apps.py features "$col" mobile)"
echo "== mobile: $features"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-mobile"
echo "LIB mobile SECONDS: $(( $(date +%s) - start ))"
python3 scripts/demo_build.py android --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant ""
ls -la demo-out/apps
set +e
bash scripts/ci/qa_android_startup.sh demo-out/apps
exit 0
