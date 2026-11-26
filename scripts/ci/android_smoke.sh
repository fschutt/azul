#!/usr/bin/env bash
# android_smoke.sh: does an APK BOOT? Installs it on an x86_64 emulator, starts it, waits, takes a
# screenshot and reads the log for a crash.
#
#   scripts/ci/android_smoke.sh <apk> <package> <out-dir> [seconds-to-wait]
#
# For an Ubuntu runner with KVM (hardware acceleration: the emulator is unusable without it) and the
# runner's Android SDK. Writes into <out-dir>:
#   screenshot.png        what the app showed after the wait
#   logcat.txt            the whole log
#   result.json           {package, booted, alive, fatal: "<first fatal line>" | null, rust_log_lines, screen_nonblank}
# Exit 0 when the app is alive after the wait and the log holds no fatal line; 1 otherwise; 2 when the
# emulator itself could not be started (nothing is said about the app then).
set -uo pipefail

apk="${1:?apk}"; pkg="${2:?package}"; out="${3:?out-dir}"; wait_s="${4:-15}"
activity="${AZ_ANDROID_ACTIVITY:-com.azul.app.AzulActivity}"
mkdir -p "$out"
: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
export ANDROID_HOME
sdk="$ANDROID_HOME/cmdline-tools/latest/bin"
[ -x "$sdk/sdkmanager" ] || sdk="$(dirname "$(ls "$ANDROID_HOME"/cmdline-tools/*/bin/sdkmanager | tail -n 1)")"
export PATH="$sdk:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
image="${AZ_ANDROID_IMAGE:-system-images;android-34;default;x86_64}"

write_result() {  # booted alive fatal rust_lines nonblank
  python3 - "$out/result.json" "$pkg" "$1" "$2" "$3" "$4" "$5" <<'PY'
import json, sys
path, pkg, booted, alive, fatal, rust, nonblank = sys.argv[1:]
json.dump({"package": pkg, "booted": booted == "1", "alive": alive == "1", "fatal": fatal or None,
           "rust_log_lines": int(rust), "screen_nonblank": nonblank == "1"}, open(path, "w"), indent=2)
PY
}

# KVM: the runner user needs the device (the emulator refuses to start without acceleration)
if [ -e /dev/kvm ]; then
  echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules >/dev/null
  sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=kvm || true
else
  echo "::warning::no /dev/kvm on this runner: the emulator will be very slow or refuse to start"
fi

yes | sdkmanager --licenses >/dev/null 2>&1 || true
sdkmanager --install "emulator" "platform-tools" "$image" >"$out/sdkmanager.log" 2>&1 || { echo "sdkmanager failed:"; tail -n 20 "$out/sdkmanager.log"; write_result 0 0 "" 0 0; exit 2; }
echo no | avdmanager create avd --force -n smoke -k "$image" -d pixel_6 >"$out/avd.log" 2>&1 || { cat "$out/avd.log"; write_result 0 0 "" 0 0; exit 2; }

nohup emulator -avd smoke -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader_indirect -accel on \
  -memory 3072 -cores 3 >"$out/emulator.log" 2>&1 &
emu=$!
trap 'adb emu kill >/dev/null 2>&1 || true; kill "$emu" 2>/dev/null || true' EXIT

adb start-server >/dev/null 2>&1
adb wait-for-device
booted=0
for _ in $(seq 1 120); do   # 10 minutes
  if [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then booted=1; break; fi
  if ! kill -0 "$emu" 2>/dev/null; then break; fi
  sleep 5
done
if [ "$booted" != "1" ]; then
  echo "the emulator did not finish booting"; tail -n 30 "$out/emulator.log"
  write_result 0 0 "" 0 0; exit 2
fi
adb shell input keyevent 82 >/dev/null 2>&1 || true   # unlock
adb shell settings put global window_animation_scale 0 >/dev/null 2>&1 || true

adb logcat -c
if ! adb install -r -t "$apk" >"$out/install.log" 2>&1; then
  cat "$out/install.log"
  write_result 1 0 "adb install failed: $(tail -n 1 "$out/install.log")" 0 0
  exit 1
fi
adb shell am start -W -n "$pkg/$activity" >"$out/start.log" 2>&1 || true
cat "$out/start.log"
sleep "$wait_s"

adb exec-out screencap -p >"$out/screenshot.png" 2>/dev/null || true
alive=0
[ -n "$(adb shell pidof "$pkg" 2>/dev/null | tr -d '\r')" ] && alive=1
adb logcat -d -v threadtime >"$out/logcat.txt" 2>&1 || true

# A fatal line: the JVM or the native side said the process died. The first one is the reason.
fatal="$(grep -m1 -E 'FATAL EXCEPTION|Fatal signal|SIGSEGV|SIGABRT|UnsatisfiedLinkError|dlopen failed|has died|ANR in|Process '"$pkg"' .*(died|killed)' "$out/logcat.txt" || true)"
rust_lines="$(grep -c -E 'RustStdoutStderr| azul' "$out/logcat.txt" || true)"
# A screenshot of a window that never drew is a black or single-colour picture.
nonblank=0
python3 - "$out/screenshot.png" <<'PY' && nonblank=1
import sys, zlib, struct
data = open(sys.argv[1], "rb").read()
if data[:8] != b"\x89PNG\r\n\x1a\n":
    sys.exit(1)
# (cheap: compare the size of the compressed picture; a flat picture compresses to a few hundred bytes)
sys.exit(0 if len(data) > 8000 else 1)
PY
write_result 1 "$alive" "$fatal" "${rust_lines:-0}" "$nonblank"
cat "$out/result.json"
[ "$alive" = "1" ] && [ -z "$fatal" ]
