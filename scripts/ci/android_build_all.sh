#!/usr/bin/env bash
# android_build_all.sh: every phone app of scripts/demo_apps.toml for Android x86_64 (the emulator's ABI) in ONE job on ONE Linux runner:
# the plain phone library and the iroh one, the apps against them (AzWidgets, AzMaps, AzPaint, AzMeet, AzWriter, AzCompanion), and the QA
# variant of AzWidgets (the `mobile-e2e` library: debug server, e2e scripting and test hooks; an ARTIFACT, never shipped).
#
#   bash scripts/ci/android_build_all.sh [<rust-target>]            default x86_64-linux-android
#
# Everything lands in demo-out/: apps/<App>-android-x86_64.apk (and apps/AzWidgets-e2e-android-x86_64.apk), results/*.json (one per app,
# the QA build's with "variant"), libs/*/libazul.info.json (seconds and bytes of each library), elf-check-*.txt (every PT_LOAD of every
# library of every APK aligned for 16 KiB pages). The libraries share one cargo target directory, so the second and the third only build
# what differs: about 25 minutes of libraries cold and 5 of apps. No emulator is started here (the QA of the QA agent and the boot test
# of scripts/ci/android_smoke.sh take the APKs from the artifact).
#
# On a runner, from any ref that has this file:
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_build_all.sh' --codegen --artifacts 'demo-out/apps/*
#   demo-out/results/*
#   demo-out/libs/*/libazul.info.json
#   demo-out/elf-check-*.txt
#   demo-out/plan.txt'
set -euxo pipefail
target="${1:-x86_64-linux-android}"
case "$target" in
  x86_64-linux-android) col=android-x86_64 ;;
  aarch64-linux-android) col=android-arm64 ;;
  *) echo "usage: android_build_all.sh [x86_64-linux-android | aarch64-linux-android]" >&2; exit 2 ;;
esac

# a runner is disposable; nothing is deleted anywhere else
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
  python3 -m pip install --quiet --user tomli || true   # the runner's Python 3.10 has no tomllib
fi
df -h / | tail -n 1

export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
rustup target add "$target"

mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt

# the libraries: what ships (plain, and the iroh one for the companion) and the QA variant
for variant in mobile mobile-iroh mobile-e2e; do
  features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
  echo "== $variant: $features"
  start=$(date +%s)
  bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant"
  echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"
done

# the apps: every phone app against the library it names, then the QA build of AzWidgets
apps="$(python3 - "$col" <<'PY'
import json, sys
col = sys.argv[1]
plan = {}
for line in open("demo-out/plan.txt"):
    key, _, value = line.rstrip("\n").partition("=")
    plan[key] = value
print(next(e["apps"] for e in json.loads(plan["apps_android"])["include"] if e["leg"] == col and e["variant"] == ""))
PY
)"
start=$(date +%s)
python3 scripts/demo_build.py android --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant ""
python3 scripts/demo_build.py android --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant mobile-e2e
echo "APPS SECONDS: $(( $(date +%s) - start ))"
ls -la demo-out/apps

for f in demo-out/results/*.json; do
  python3 -c 'import json,sys; r=json.load(open(sys.argv[1])); print(r["app"], r["column"], r.get("variant", ""), r["status"], r.get("size"), r.get("seconds"), (r.get("first_error") or "")[:300])' "$f"
done
python3 scripts/android_elf_check.py demo-out/libs/*/libazul.so | tee demo-out/elf-check-libs.txt
python3 scripts/android_elf_check.py demo-out/apps/*.apk | tee demo-out/elf-check-apks.txt
