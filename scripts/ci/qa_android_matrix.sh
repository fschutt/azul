#!/usr/bin/env bash
# qa_android_matrix.sh <dir of the *-e2e-android-x86_64.apk files>: every e2e scenario file (tests/e2e/*.json, examples/*/e2e/*.json) against its host
# app on the Android emulator, one table (scripts/ci/e2e_matrix.py). The file is pushed, its path becomes the property debug.az.e2e, the e2e APK of
# the host app is started and exits when the file is done; the report is read from logcat.
#   scripts/remote.sh run <ref> ubuntu-22.04 'gh run download <build run id> -n remote-Linux -D apks && bash scripts/ci/qa_android_matrix.sh apks/demo-out/apps' --artifacts 'demo-out/qa-matrix/**'
set -uo pipefail
dir="${1:?directory of the *-e2e-android-x86_64.apk files}"
apks=()
for f in "$dir"/*-e2e-android-x86_64.apk; do [ -e "$f" ] && apks+=("$f"); done
[ "${#apks[@]}" -gt 0 ] || { echo "no *-e2e-android-x86_64.apk in $dir"; exit 2; }
first="$dir/AzWidgets-e2e-android-x86_64.apk"; [ -e "$first" ] || first="${apks[0]}"
export AZ_QA_MATRIX_APKS="${apks[*]}"
bash "$(dirname "$0")/android_qa.sh" demo-out/qa-matrix "$first" swiftshader_indirect "matrix" matrix
