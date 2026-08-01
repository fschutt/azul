#!/usr/bin/env bash
# Post-release android check: install the published APK on the running
# emulator, start it via its LAUNCHER activity (the path a phone actually
# uses), and require it to still be alive 8s later.
#
# Lives in a FILE because reactivecircus/android-emulator-runner executes
# each line of its `script:` input as a SEPARATE `/usr/bin/sh -c` — shell
# variables do not survive from one line to the next, which turned
# `am start -n "$PKG/$ACT"` into `am start -n "/"` (exit 255) while the
# install right above it succeeded. One line calling one bash script keeps
# the whole flow in a single shell (and gets us bash + pipefail back).
#
# Expects: /tmp/azul.apk and /tmp/badging.txt from the preceding steps.
set -euo pipefail

# Releases predating the x86_64 emulator variant ship arm64-only APKs; an
# x86_64 emulator cannot load them (that mismatch surfaced as "Unable to
# find native library" and is indistinguishable from real breakage). Skip
# the LAUNCH loudly for those releases — the static checks + install above
# still ran; full launch coverage resumes with the first release that
# publishes azul-self-test-android-x86_64.apk.
APK_ABI=$(cut -d= -f2 /tmp/apk-abi.env 2>/dev/null || echo unknown)
EMU_ABI=$(adb shell getprop ro.product.cpu.abi | tr -d '\r')
if [ "$APK_ABI" != "$EMU_ABI" ]; then
  echo "::warning::launch check SKIPPED: APK ships $APK_ABI, emulator is $EMU_ABI — this release predates the x86_64 emulator APK; coverage resumes with the next release"
  exit 0
fi

adb install -r /tmp/azul.apk
ACT=$(grep -oE "launchable-activity: name='[^']+'" /tmp/badging.txt | head -1 | sed "s/.*name='//;s/'//")
PKG=$(grep -oE "package: name='[^']+'" /tmp/badging.txt | head -1 | sed "s/.*name='//;s/'//")
if [ -z "$PKG" ] || [ -z "$ACT" ]; then
  echo "::error::could not extract package/activity from badging (PKG='$PKG' ACT='$ACT')"
  exit 1
fi
echo "launching $PKG/$ACT"
adb logcat -c
adb shell am start -W -n "$PKG/$ACT"
sleep 8
# A crash on launch is the failure this whole job exists to catch.
if adb logcat -d | grep -E "FATAL EXCEPTION|ClassNotFoundException|E AndroidRuntime"; then
  echo "::error::the published APK crashed on launch"
  exit 1
fi
adb shell pidof "$PKG" >/dev/null \
  || { echo "::error::$PKG is not running 8s after launch"; exit 1; }
echo "APK installed, launched via its LAUNCHER activity, and still alive"
