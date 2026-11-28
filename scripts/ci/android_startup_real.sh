#!/usr/bin/env bash
# android_startup_real.sh [<App> ...]  (default: AzWidgets AzWriter): the cold-start lab on the REAL apps (scripts/ci/startup_lab.py --real): the plain
# phone library and the plain apps are built here (as scripts/ci/android_e46_job.sh builds them), then per app AZ_LAB_STARTS cold starts (default 10) in the
# light mode and as many in the night mode, recorded at 30 fps and cut into frames, the engine's startup timeline and its info log of every start kept,
# and home and back once. The same job on two refs (the state to compare against and the change) is the BEFORE and the AFTER.
#
#   scripts/remote.sh run <full sha | ref> ubuntu-22.04 'bash scripts/ci/android_startup_real.sh AzWidgets AzWriter' --codegen --artifacts 'demo-out/lab/**'
#
# AZ_ANDROID_DEVICE=pixel_8 AZ_ANDROID_OVERLAYS=com.android.internal.emulation.pixel_8 puts the Pixel 8's camera cutout on the emulator (the QA's tab jobs do).
#
# Output: demo-out/lab/lab.md (a table with the frames of the starting window (S), of black (B) and of the first picture (1) per row, the run lengths and
# the engine's and the system's events of every start), lab.json, <row>/strip.png (a contact sheet of the first start) and <row>/start1/cold.mp4.
set -uxo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
cd "$root"
apps="${*:-AzWidgets AzWriter}"
target=x86_64-linux-android
col=android-x86_64
out="${AZ_LAB_OUT:-demo-out/lab}"
starts="${AZ_LAB_STARTS:-10}"
mkdir -p "$out" demo-out/libs demo-out/apps demo-out/results
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
  python3 -m pip install --quiet --user tomli || true
  command -v ffmpeg >/dev/null 2>&1 || sudo apt-get install -y ffmpeg >/dev/null 2>&1 || echo "ffmpeg could not be installed"
fi
# shellcheck source=scripts/ci/android_lab_common.sh
. "$here/android_lab_common.sh"
lab_sdk "$out" || exit 2
rustup target add "$target"
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
features="$(python3 scripts/demo_apps.py features "$col" mobile)"
echo "FEATURES mobile: $features"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-mobile" || { echo "LIB mobile FAILED"; exit 3; }
echo "LIB mobile SECONDS: $(( $(date +%s) - start ))"
python3 scripts/demo_build.py android --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" \
  || echo "demo_build.py returned non-zero: apps that did not build are skipped below"
ls -la demo-out/apps

lab_apks=()
for app in $apps; do
  apk="demo-out/apps/$app-$col.apk"
  [ -e "$apk" ] && lab_apks+=(--apk "$app=$apk") || echo "no $apk: $app did not build"
done
[ "${#lab_apks[@]}" -gt 0 ] || { echo "no app built"; exit 4; }

lab_emulator_up "$out" || exit 5
only=()
for o in ${AZ_LAB_ONLY:-}; do only+=(--only "$o"); done
python3 "$here/startup_lab.py" run --out "$out" --real "${lab_apks[@]}" --starts "$starts" ${only[@]+"${only[@]}"} 2>&1 | tee "$out/lab.log"
lab_emulator_down
exit 0
