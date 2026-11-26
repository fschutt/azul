#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): the Android x86_64 leg of the demos pipeline on one runner: the plain phone libazul
# (16 KiB pages, gated by scripts/android_elf_check.py), every app that asks for it against it (demo_build.py), and the boot test
# of each on the emulator with the manifest's buttons and texts (scripts/ci/android_smoke.sh, the one pass line).
#   bash scripts/ci/exp_android_apps.sh
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
rustup target add "$target"
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan | tee demo-out/plan.txt
read -r features apps < <(python3 - "$col" <<'PY'
import json, sys
col = sys.argv[1]
plan = {}
for line in open("demo-out/plan.txt"):
    key, _, value = line.rstrip("\n").partition("=")
    plan[key] = value
feats = next(e["features"] for e in json.loads(plan["libs_android"])["include"] if e["variant"] == "mobile" and e["leg"] == col)
apps = next(e["apps"] for e in json.loads(plan["apps_android"])["include"] if e["leg"] == col and e["variant"] == "")
print(feats, " ".join(a for a in apps.split() if a != "AzCompanion").replace(" ", "+"))
PY
)
apps="${apps//+/ }"
echo "FEATURES: $features"; echo "APPS: $apps"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-mobile"
echo "LIB SECONDS: $(( $(date +%s) - start ))"
python3 scripts/android_elf_check.py "demo-out/libs/demolib-$col-mobile/libazul.so" | tee demo-out/elf-check-lib.txt
start=$(date +%s)
python3 scripts/demo_build.py android --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant ""
echo "APP SECONDS: $(( $(date +%s) - start ))"
ls -la demo-out/apps
for f in demo-out/results/*.json; do python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:160])' "$f"; done
python3 scripts/android_elf_check.py demo-out/apps/*.apk | tee demo-out/elf-check-apks.txt || true
export AZ_SMOKE_TAPS="$(grep '^smoke_taps=' demo-out/plan.txt | cut -d= -f2-)"
export AZ_SMOKE_EXPECTS="$(grep '^smoke_expects=' demo-out/plan.txt | cut -d= -f2-)"
bash scripts/ci/android_smoke.sh demo-out/smoke 20 demo-out/apps/*.apk || echo "SMOKE FAILED"
for f in demo-out/smoke/*/result.json; do python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], "PASS" if r["pass"] else "FAIL", "-", r["reason"])' "$f"; done
