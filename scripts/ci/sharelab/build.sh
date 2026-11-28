#!/usr/bin/env bash
# build.sh <out.apk>: the ShareLab test APK (Java only, no Gradle), signed with the repository's debug key.
#   ANDROID_HOME (build-tools 34.0.0, platforms;android-34), JAVA_HOME (a JDK 17). QA only: never shipped.
set -euo pipefail
out="${1:?out.apk}"
here="$(cd "$(dirname "$0")" && pwd)"
: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
BT="$ANDROID_HOME/build-tools/34.0.0"
PLATFORM="$ANDROID_HOME/platforms/android-34"
JAVAC="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/classes" "$work/dex"
"$JAVAC" -encoding UTF-8 -source 11 -target 11 -classpath "$PLATFORM/android.jar" -d "$work/classes" $(find "$here/src" -name '*.java')
"$BT/d8" --lib "$PLATFORM/android.jar" --output "$work/dex" $(find "$work/classes" -name '*.class')
"$BT/aapt2" link --manifest "$here/AndroidManifest.xml" -I "$PLATFORM/android.jar" -o "$work/base.apk"
( cd "$work/dex" && zip -q ../base.apk classes.dex )
"$BT/zipalign" -f 4 "$work/base.apk" "$work/aligned.apk"
"$BT/apksigner" sign --ks "$here/../../android/debug.keystore" --ks-key-alias androiddebugkey --ks-pass pass:android "$work/aligned.apk"
mkdir -p "$(dirname "$out")"
cp "$work/aligned.apk" "$out"
echo "built $out"
