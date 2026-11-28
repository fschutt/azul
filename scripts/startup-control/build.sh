#!/usr/bin/env bash
# build.sh: the CONTROL APK of the cold-start lab (scripts/ci/android_startup_lab.sh): the real Java side of an Azul APK over a
# stand-in native library, so that what the system does with a NativeActivity's window at a cold start can be measured WITHOUT the
# engine (no layout, no fonts, no libazul: a build of about a minute instead of half an hour).
#
#   bash scripts/startup-control/build.sh <out.apk> [--android-dir <dir>] [--activity <AzulActivity.java>] [--package <id>] [--abi x86_64|arm64-v8a] [--label <name>]
#
# What is the real thing: scripts/android/AndroidManifest.xml (the template, filled in the way scripts/build-android.sh fills it),
# scripts/android/res (the theme of every Azul activity, light and night), the launcher icon (scripts/demo_icon.py), every
# scripts/android/*.java (AzulActivity, the bridges) compiled and dexed as build-android.sh does. --activity compiles another
# AzulActivity.java in the place of the one in the android directory (a variant to try), --android-dir takes the manifest, the theme and
# the Java of another checkout's scripts/android (the lab's BEFORE build: the whole Java side of an older commit). What is the stand-in: libazctl.so
# (ctl.c), which does to the window what the Rust shell does with the costs of the layout replaced by sleeps (see its header), and
# a stub for every native method of the Java classes (gen_jni_stubs.py).
#
# Needs: build-tools 34.0.0 (aapt2, d8, zipalign, apksigner), platforms;android-34, the NDK, JDK 17 (javac with -h), zip, python3.
set -euo pipefail

out="${1:?usage: build.sh <out.apk> [--activity f] [--package id] [--abi a] [--label l]}"
shift
activity=""
android_dir=""
package="com.azul.startctl"
abi="x86_64"
label="AzStartCtl"
while [ $# -gt 0 ]; do
    case "$1" in
        --activity) activity="$2"; shift 2 ;;
        --android-dir) android_dir="$2"; shift 2 ;;
        --package) package="$2"; shift 2 ;;
        --abi) abi="$2"; shift 2 ;;
        --label) label="$2"; shift 2 ;;
        *) echo "unknown option $1" >&2; exit 2 ;;
    esac
done

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
android_dir="$(cd "${android_dir:-$root/scripts/android}" && pwd)"
: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
: "${ANDROID_NDK_HOME:=$ANDROID_HOME/ndk/27.0.12077973}"
BT="$ANDROID_HOME/build-tools/34.0.0"
PLATFORM="$ANDROID_HOME/platforms/android-34"
NDKBIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
case "$abi" in
    x86_64) clang="$NDKBIN/x86_64-linux-android24-clang" ;;
    arm64-v8a) clang="$NDKBIN/aarch64-linux-android24-clang" ;;
    *) echo "unknown ABI $abi" >&2; exit 2 ;;
esac
for need in "$BT/aapt2" "$BT/d8" "$BT/zipalign" "$BT/apksigner" "$PLATFORM/android.jar" "$clang"; do
    [ -e "$need" ] || { echo "missing $need" >&2; exit 3; }
done
JAVAC="${JAVA_HOME:+$JAVA_HOME/bin/}javac"
command -v "$JAVAC" >/dev/null 2>&1 || { echo "javac not found (JDK 17: set JAVA_HOME)" >&2; exit 4; }

mkdir -p "$(dirname "$out")"
work="$(mktemp -d "$(dirname "$out")/ctl-work.XXXXXX")"
mkdir -p "$work/classes" "$work/dex" "$work/jni" "$work/res" "$work/lib/$abi" "$work/src"

# the Java side: the tree's sources, with the lab's AzulActivity in place of the tree's when one is given
find "$android_dir" -maxdepth 1 -name '*.java' | while read -r f; do
    if [ -n "$activity" ] && [ "$(basename "$f")" = "AzulActivity.java" ]; then continue; fi
    cp "$f" "$work/src/"
done
[ -z "$activity" ] || cp "$activity" "$work/src/AzulActivity.java"
"$JAVAC" -source 11 -target 11 -classpath "$PLATFORM/android.jar" -d "$work/classes" -h "$work/jni" "$work"/src/*.java
"$BT/d8" --output "$work/dex" --min-api 24 $(find "$work/classes" -name '*.class')
[ -f "$work/dex/classes.dex" ] || { echo "d8 did not produce classes.dex" >&2; exit 6; }

# the native side: the stand-in and a stub for every native method the Java classes declare
python3 "$here/gen_jni_stubs.py" "$work/jni" "$work/stubs.c"
"$clang" -shared -fPIC -O2 -Wall -Wno-unused-parameter -Wl,-z,max-page-size=16384 -o "$work/lib/$abi/libazctl.so" \
    "$here/ctl.c" "$work/stubs.c" -landroid -llog

# the manifest: the template, as build-android.sh fills it
cp -R "$android_dir/res/." "$work/res/"
icon=""
if [ -f "$root/scripts/demo_icon.py" ]; then
    python3 "$root/scripts/demo_icon.py" android "$label" "$work/res" >/dev/null
    icon='android:icon="@mipmap/ic_launcher"'
fi
"$BT/aapt2" compile --dir "$work/res" -o "$work/compiled-res.zip"
: > "$work/empty.xml"
sed \
    -e "s|@ICON_ATTR@|$icon|g" \
    -e "/@EXTRA_PERMISSIONS@/r $work/empty.xml" -e "/@EXTRA_PERMISSIONS@/d" \
    -e "/@AUTH_REDIRECT_ACTIVITY@/r $work/empty.xml" -e "/@AUTH_REDIRECT_ACTIVITY@/d" \
    -e "s|@PACKAGE@|$package|g" -e "s|@LABEL@|$label|g" -e "s|@LIB_NAME@|azctl|g" \
    -e "s|@VERSION_CODE@|1|g" -e "s|@VERSION_NAME@|1.0|g" \
    -e 's|android:hasCode="false"|android:hasCode="true"|g' \
    "$android_dir/AndroidManifest.xml" > "$work/AndroidManifest.xml"

( cd "$work" && "$BT/aapt2" link --manifest AndroidManifest.xml -I "$PLATFORM/android.jar" -o base.apk compiled-res.zip \
    && zip -qr base.apk "lib/$abi/" && ( cd dex && zip -q ../base.apk classes.dex ) \
    && "$BT/zipalign" -f 4 base.apk aligned.apk \
    && "$BT/apksigner" sign --ks "$root/scripts/android/debug.keystore" --ks-key-alias androiddebugkey --ks-pass pass:android aligned.apk )
cp "$work/aligned.apk" "$out"
echo "built: $out ($(wc -c < "$out" | tr -d ' ') bytes, package $package, ABI $abi)"
