#!/usr/bin/env bash
# qa_android_audit.sh <dir of the QA-variant APKs>: the phone-fit audit of EVERY app that has a `mobile-e2e` build, in one runner job on one
# emulator boot: per app the layout audit in portrait and landscape (boxes over the window's edges, tappable and text boxes under the
# status and navigation bars, touch targets under 48 dp, text boxes crossing an edge), the engine's frame and the device's frame of
# each, the system dark mode asked of the engine (did the window's mode change, did the picture), and the app's own script (AzPaint:
# a stroke and its undo; AzWriter: typing and the keyboard; AzMaps: the sidebar toggle and a double tap).
#   scripts/remote.sh run <ref> ubuntu-22.04 'gh run download <build run id> -n remote-Linux -D apks && bash scripts/ci/qa_android_audit.sh apks/demo-out/apps' --artifacts 'demo-out/qa/**'
# The APKs are named <App>-e2e-android-x86_64.apk (scripts/ci/android_build_all.sh); the job fetches them from the build run's artifact itself.
set -uo pipefail
dir="${1:?directory of the *-e2e-android-x86_64.apk files}"
apks=()
for f in "$dir"/*-e2e-android-x86_64.apk; do [ -e "$f" ] && apks+=("$f"); done
[ "${#apks[@]}" -gt 0 ] || { echo "no *-e2e-android-x86_64.apk in $dir"; exit 2; }
first="${apks[0]}"
export AZ_QA_AUDIT_APKS="${apks[*]:1}"
bash "$(dirname "$0")/android_qa.sh" demo-out/qa "$first" swiftshader_indirect "audit" audit
