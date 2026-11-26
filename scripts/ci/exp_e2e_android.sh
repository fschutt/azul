#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): the QA build of AzWidgets for Android against the artifact-only libazul with the
# engine's debug server (scripts/demo_apps.toml [lib.mobile-e2e]), made the way the demos workflow will make it:
# the features come from `demo_apps.py plan --e2e`, the lib from scripts/demo_lib.sh, the app from `demo_build.py android --variant`.
#   bash scripts/ci/exp_e2e_android.sh <rust-target>
set -euxo pipefail
target="${1:-x86_64-linux-android}"
sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
df -h / | tail -n 1
export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-$JAVA_HOME}"
python3 -m pip install --quiet --user tomli || true   # the runner's Python 3.10 has no tomllib
rustup target add "$target"
case "$target" in aarch64-linux-android) col=android-arm64 ;; x86_64-linux-android) col=android-x86_64 ;; esac
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
features="$(python3 - "$col" <<'PY'
import json, sys
col = sys.argv[1]
for line in open("demo-out/plan.txt"):
    key, _, value = line.rstrip("\n").partition("=")
    if key == "libs_android":
        for e in json.loads(value)["include"]:
            if e["variant"] == "mobile-e2e" and e["leg"] == col:
                print(e["features"])
PY
)"
echo "FEATURES: $features"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-mobile-e2e"
echo "LIB SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
python3 scripts/demo_build.py android --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant mobile-e2e
echo "APP SECONDS: $(( $(date +%s) - start ))"
ls -la demo-out/apps demo-out/results "demo-out/libs/demolib-$col-mobile-e2e"
unzip -l "demo-out/apps/AzWidgets-e2e-$col.apk" | tee "demo-out/apk-$col.txt" | tail -n 12
cat demo-out/results/*.json
