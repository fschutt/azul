#!/usr/bin/env bash
# android_azmail_apk.sh: AzMail's APK for the Android emulator (build + boot test) from the tree the job runs in.
#
# AzMail is no phone app of scripts/demo_apps.toml on master: the manifest part of "AzMail on the phones" stays on the branch ci-wip/azmail-mobile
# until a demos run has shown the desktop links (the comment of its entry). This job gives the RUNNER'S copy of the manifest the same entry the branch
# has (platforms android and ios, the lean phone library) and builds the APK like any phone app of the manifest; nothing is changed in the repository.
# The APK is what scripts/ci/android_overlays_job.sh (AZ_OVERLAYS_AZMAIL_RUN=<id of this run>) puts on the emulator next to AzWidgets: AzMail's compose
# window is a second window of the app, which a phone shows as a view pushed over the inbox.
#
#   scripts/remote.sh run <full sha | ref> ubuntu-22.04 'bash scripts/ci/android_azmail_apk.sh' --codegen --artifacts 'demo-out/apps/*
#   demo-out/results/*
#   demo-out/libs/*/libazul.info.json
#   demo-out/smoke/**
#   demo-out/plan.txt'
set -euo pipefail
python3 - <<'PY'
import re
path = "scripts/demo_apps.toml"
text = open(path, encoding="utf-8").read()
entry = re.search(r'(\[\[app\]\]\npackage = "AzMail"\n.*?)(?=\n\[\[app\]\]|\Z)', text, re.S)
if entry is None:
    raise SystemExit("android_azmail_apk: no [[app]] entry for AzMail in scripts/demo_apps.toml")
body = entry.group(1)
if "mobile_lib" in body:
    print("android_azmail_apk: AzMail is a phone app of this manifest already")
    raise SystemExit(0)
platforms = re.search(r'^platforms = \[(.*?)\]$', body, re.M)
if platforms is None:
    raise SystemExit("android_azmail_apk: the AzMail entry has no platforms line")
patched = body.replace(
    platforms.group(0),
    'platforms = [' + platforms.group(1) + ', "android", "ios"]\nmobile_lib = "mobile"',
    1,
)
open(path, "w", encoding="utf-8").write(text.replace(body, patched, 1))
print("android_azmail_apk: AzMail is a phone app of the runner's copy of the manifest (mobile_lib mobile)")
PY
python3 scripts/demo_apps.py mobile-lib AzMail
exec bash scripts/ci/android_build_smoke.sh AzMail
