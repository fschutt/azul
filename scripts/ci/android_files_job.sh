#!/usr/bin/env bash
# android_files_job.sh: the emulator proof of the files / sharing / open-with work (ledger WP-F) in ONE job on ONE Linux runner:
# AzSelfTest (the QA build, with the `files.*` rows) built from THIS tree, ShareLab (scripts/ci/sharelab: the test target and sender),
# the QA's x86_64 emulator, and scripts/ci/android_files_proof.py driving the steps with pictures.
#
#   bash scripts/ci/android_files_job.sh [--only share_one,share_many,...]
#
# Lands in demo-out/files/: files-proof.md / files-proof.json (every expectation, ok or FAIL, with what was seen), pictures/*.png (the
# chooser, the receiver), start logs, sharelab.apk, and demo-out/apps/AzSelfTest-e2e-android-x86_64.apk. The exit code is the proof's.
#
# On a runner (ONE of the two jobs a work package may have at once):
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_files_job.sh' --codegen --artifacts 'demo-out/files/**
#   demo-out/files/pictures/*
#   demo-out/results/*.json'
set -uxo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here/../.."

# 1. the app, built from this tree (android_build_app.sh: the QA library the manifest names for AzSelfTest, then the APK)
bash scripts/ci/android_build_app.sh AzSelfTest || { echo "BUILD OF AzSelfTest FAILED"; exit 1; }
apk="demo-out/apps/AzSelfTest-e2e-android-x86_64.apk"
[ -f "$apk" ] || { echo "no $apk"; ls -la demo-out/apps; exit 1; }

# 2. the emulator and the proof (android_probe.sh boots the emulator, then runs the script named by AZ_PROBE_PY instead of its own)
export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
export AZ_PROBE_PY="$here/android_files_proof.py"
bash scripts/ci/android_probe.sh demo-out/files "$apk" "$@"
code=$?
echo "files proof exit: $code"
cat demo-out/files/files-proof.md 2>/dev/null | head -150 || true
exit "$code"
