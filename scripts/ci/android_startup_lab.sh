#!/usr/bin/env bash
# android_startup_lab.sh: the COLD-START LAB of the Android shell (ledger E46 / E47 / E60): what a person sees between the system's starting
# window and the first picture, frame by frame, with the engine taken out of the question.
#
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_startup_lab.sh' --artifacts 'demo-out/lab/**'
#   (no --codegen, no cargo: the control APKs are built in about a minute each, the emulator boots in about five)
#
# The control APK (scripts/startup-control) is the REAL Java side of an Azul APK (scripts/android: AzulActivity, the bridges, the manifest
# and the theme) over a stand-in native library that does to the window what the Rust shell does, with the layout's cost replaced by a
# sleep. Three builds of the Java side:
#   before  the commit AZ_LAB_BEFORE_REF (default: master 98d415cb4e, the state E60 measured): its whole scripts/android directory
#   none    `before` with the starting-window hold taken out (the system's own behaviour)
#   tree    the checkout this job runs in (what a change under test does)
# and the rows of scripts/ci/startup_lab.py (DEFAULT_ROWS, or a JSON file: AZ_LAB_ROWS), each AZ_LAB_STARTS cold starts (default 4), recorded
# with adb screenrecord at 30 fps like scripts/ci/flicker_audit.py does and cut into frames: the verdict is the run lengths of the frames of
# the starting window (S), of black (B), of the first picture (1) and of the settled one (2).
#
# Output, demo-out/lab/: lab.md (the table, the run lengths and the system's own events of every start), lab.json, <row>/strip.png (a contact
# sheet of the first start), <row>/start1/cold.mp4 and logcat.txt of every start. AZ_LAB_ONLY="text text": only the rows whose name holds one of them.
# The emulator is the QA's (API 34 AOSP image, Pixel 6 profile, swiftshader_indirect: AZ_ANDROID_IMAGE / AZ_ANDROID_DEVICE / AZ_LAB_GPU change it).
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
cd "$root"
out="${AZ_LAB_OUT:-demo-out/lab}"
starts="${AZ_LAB_STARTS:-4}"
before_ref="${AZ_LAB_BEFORE_REF:-98d415cb4edd27927b3ba397df9336de9ca03225}"
mkdir -p "$out" demo-out/lab-apks

if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
  command -v ffmpeg >/dev/null 2>&1 || sudo apt-get install -y ffmpeg >/dev/null 2>&1 || echo "ffmpeg could not be installed"
fi
# shellcheck source=scripts/ci/android_lab_common.sh
. "$here/android_lab_common.sh"
lab_sdk "$out" || exit 2

# ---- the control APKs
build() {   # build <name> <extra build.sh options...>
  local name="$1"; shift
  bash scripts/startup-control/build.sh "demo-out/lab-apks/$name.apk" --package "com.azul.startctl.$name" "$@" >"$out/build-$name.log" 2>&1 \
    && tail -n 1 "$out/build-$name.log" || { echo "the $name control did not build:"; tail -n 30 "$out/build-$name.log"; return 1; }
}
apks=()
build tree && apks+=(--apk "tree=demo-out/lab-apks/tree.apk")
# BEFORE: the whole scripts/android directory of the old commit (a shallow checkout has no history: fetch just that commit)
old="demo-out/lab-before"
mkdir -p "$old"
if git fetch --depth=1 origin "$before_ref" >"$out/fetch-before.log" 2>&1 && git archive FETCH_HEAD scripts/android | tar -x -C "$old"; then
  build before --android-dir "$old/scripts/android" && apks+=(--apk "before=demo-out/lab-apks/before.apk")
  # NONE: the same without the hold: the call is the one line that sets it up
  none="demo-out/lab-none"
  mkdir -p "$none" && cp -R "$old/scripts/android/." "$none/"
  python3 - "$none/AzulActivity.java" <<'PY'
import sys
p = sys.argv[1]
s = open(p).read()
assert "Splash31.hold(this);" in s, "the hold call is not where the lab expects it"
open(p, "w").write(s.replace("Splash31.hold(this);", "/* no hold: the system's own behaviour */"))
PY
  build none --android-dir "$none" && apks+=(--apk "none=demo-out/lab-apks/none.apk")
else
  echo "could not fetch $before_ref (see $out/fetch-before.log): no before / none builds"
fi
[ "${#apks[@]}" -gt 0 ] || { echo "no control APK built"; exit 3; }

# ---- the emulator
lab_emulator_up "$out" || exit 5

# ---- the lab
only=()
for o in ${AZ_LAB_ONLY:-}; do only+=(--only "$o"); done
python3 "$here/startup_lab.py" run --out "$out" "${apks[@]}" --starts "$starts" ${AZ_LAB_ROWS:+--rows "$AZ_LAB_ROWS"} ${only[@]+"${only[@]}"} 2>&1 | tee "$out/lab.log"

lab_emulator_down
exit 0
