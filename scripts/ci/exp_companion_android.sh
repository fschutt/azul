#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): AzCompanion for Android x86_64 against the iroh variant of the phone libazul, booted on
# the emulator with the button and the text the manifest names ([app.smoke]) and judged by scripts/ci/smoke_verdict.py.
#   bash scripts/ci/exp_companion_android.sh
set -euxo pipefail
target=x86_64-linux-android; col=android-x86_64
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
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan | tee demo-out/plan.txt
features="$(python3 - "$col" <<'PY'
import json, sys
col = sys.argv[1]
for line in open("demo-out/plan.txt"):
    key, _, value = line.rstrip("\n").partition("=")
    if key == "libs_android":
        for e in json.loads(value)["include"]:
            if e["variant"] == "mobile-iroh" and e["leg"] == col:
                print(e["features"])
PY
)"
echo "FEATURES: $features"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-mobile-iroh"
echo "LIB SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
python3 scripts/demo_build.py android --target "$target" --apps AzCompanion --libs demo-out/libs --out demo-out/apps --results demo-out/results
echo "APP SECONDS: $(( $(date +%s) - start ))"
cat demo-out/results/*.json
export AZ_SMOKE_TAPS="$(grep '^smoke_taps=' demo-out/plan.txt | cut -d= -f2-)"
export AZ_SMOKE_EXPECTS="$(grep '^smoke_expects=' demo-out/plan.txt | cut -d= -f2-)"
echo "TAPS: $AZ_SMOKE_TAPS EXPECTS: $AZ_SMOKE_EXPECTS"
bash scripts/ci/android_smoke.sh demo-out/smoke 20 "demo-out/apps/AzCompanion-$col.apk" || echo "SMOKE FAILED"
cat demo-out/smoke/*/result.json || true
