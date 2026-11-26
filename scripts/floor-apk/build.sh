#!/usr/bin/env bash
# build.sh: the FLOOR APK of the startup measurements: an empty Java Activity with the SDK levels, the signing key and the launch
# path of the demo APKs, and NOTHING of azul in it (no native library, no engine): what the platform itself needs to start a process
# and show one window, to measure a demo app against.
#
#   bash scripts/floor-apk/build.sh [out.apk]        default: target/floor-apk/AzFloor-android.apk
#
# Needs build-tools 34.0.0 (aapt2, d8, zipalign, apksigner), platforms;android-34 and a JDK 17 (javac); no cargo, about a
# minute. The APK has no native library, so one file serves every ABI; it launches as
#   adb install -r AzFloor-android.apk && adb shell am start -W -n com.azul.floor/.FloorActivity
# and ActivityTaskManager logs "Displayed com.azul.floor/.FloorActivity: +<n>ms" for it as for every other app.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="${1:-$root/target/floor-apk/AzFloor-android.apk}"

: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
BT="$ANDROID_HOME/build-tools/34.0.0"
PLATFORM="$ANDROID_HOME/platforms/android-34"
for need in "$BT/aapt2" "$BT/d8" "$BT/zipalign" "$BT/apksigner" "$PLATFORM/android.jar"; do
    [[ -e "$need" ]] || { echo "missing $need: sdkmanager 'build-tools;34.0.0' 'platforms;android-34'" >&2; exit 3; }
done
JAVAC="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
command -v "$JAVAC" >/dev/null 2>&1 || { echo "javac not found (JDK 17: set JAVA_HOME)" >&2; exit 4; }

# THE debug keystore of the demo APKs (a public key, alias androiddebugkey, password android)
KS="${AZ_ANDROID_KEYSTORE:-$root/scripts/android/debug.keystore}"
[[ -f "$KS" ]] || { echo "missing $KS" >&2; exit 5; }

# The intermediate files go to a new directory next to the APK (a few KB per build): nothing here deletes anything, because the
# script is also run on a developer's machine, where only its owner removes files.
mkdir -p "$(dirname "$out")"
work="$(mktemp -d "$(dirname "$out")/floor-work.XXXXXX")"
mkdir -p "$work/classes" "$work/dex"

"$JAVAC" -source 11 -target 11 -classpath "$PLATFORM/android.jar" -d "$work/classes" "$here/FloorActivity.java"
"$BT/d8" --output "$work/dex" --min-api 24 $(find "$work/classes" -name '*.class')
[[ -f "$work/dex/classes.dex" ]] || { echo "d8 did not produce classes.dex" >&2; exit 6; }
"$BT/aapt2" link --manifest "$here/AndroidManifest.xml" -I "$PLATFORM/android.jar" -o "$work/base.apk"
( cd "$work/dex" && zip -q "$work/base.apk" classes.dex )
"$BT/zipalign" -f 4 "$work/base.apk" "$work/aligned.apk"
"$BT/apksigner" sign --ks "$KS" --ks-key-alias androiddebugkey --ks-pass pass:android "$work/aligned.apk"

mkdir -p "$(dirname "$out")"
cp "$work/aligned.apk" "$out"
echo "built: $out ($(wc -c < "$out" | tr -d ' ') bytes)"
