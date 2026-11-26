#!/usr/bin/env bash
# android_smoke.sh: do APKs BOOT? Starts ONE x86_64 emulator, then for every APK: installs it, starts its
# launcher activity, waits, takes a screenshot and reads the log for a crash.
#
#   scripts/ci/android_smoke.sh <out-dir> <seconds-to-wait> <apk> [<apk> ...]
#
# For an Ubuntu runner with KVM (without hardware acceleration the emulator is unusable) and the runner's
# Android SDK (build-tools 34 for aapt2). Per APK <out-dir>/<apk name without .apk>/ holds
#   screenshot.png   what the app showed after the wait
#   logcat.txt       the log of that run
#   result.json      {app, package, booted, alive, fatal: "<first fatal line>" | null, rust_log_lines, screen_nonblank}
# and <out-dir>/summary.json lists them. Exit 0 when every app is alive after the wait with no fatal line in
# the log; 1 when one is not; 2 when the emulator itself could not be started (nothing is said about the apps).
set -uo pipefail

out="${1:?out-dir}"; wait_s="${2:?seconds}"; shift 2
[ $# -gt 0 ] || { echo "no APK given" >&2; exit 2; }
mkdir -p "$out"
: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
export ANDROID_HOME
sdk="$ANDROID_HOME/cmdline-tools/latest/bin"
[ -x "$sdk/sdkmanager" ] || sdk="$(dirname "$(ls "$ANDROID_HOME"/cmdline-tools/*/bin/sdkmanager | tail -n 1)")"
export PATH="$sdk:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
image="${AZ_ANDROID_IMAGE:-system-images;android-34;default;x86_64}"
aapt2="$ANDROID_HOME/build-tools/34.0.0/aapt2"

# KVM: the runner user needs the device (the emulator refuses to start without acceleration)
if [ -e /dev/kvm ]; then
  echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules >/dev/null
  sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=kvm || true
else
  echo "::warning::no /dev/kvm on this runner: the emulator will be very slow or refuse to start"
fi

yes | sdkmanager --licenses >/dev/null 2>&1 || true
sdkmanager --install "emulator" "platform-tools" "build-tools;34.0.0" "$image" >"$out/sdkmanager.log" 2>&1 \
  || { echo "sdkmanager failed:"; tail -n 20 "$out/sdkmanager.log"; exit 2; }
echo no | avdmanager create avd --force -n smoke -k "$image" -d pixel_6 >"$out/avd.log" 2>&1 \
  || { cat "$out/avd.log"; exit 2; }

nohup emulator -avd smoke -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader_indirect -accel on \
  -memory 3072 -cores 3 >"$out/emulator.log" 2>&1 &
emu=$!
trap 'adb emu kill >/dev/null 2>&1 || true; kill "$emu" 2>/dev/null || true' EXIT

adb start-server >/dev/null 2>&1
adb wait-for-device
booted=0
for _ in $(seq 1 120); do   # 10 minutes
  if [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then booted=1; break; fi
  kill -0 "$emu" 2>/dev/null || break
  sleep 5
done
if [ "$booted" != "1" ]; then
  echo "the emulator did not finish booting"; tail -n 30 "$out/emulator.log"
  exit 2
fi
adb shell input keyevent 82 >/dev/null 2>&1 || true   # unlock
adb shell settings put global window_animation_scale 0 >/dev/null 2>&1 || true
echo "emulator booted: $(adb shell getprop ro.product.cpu.abi | tr -d '\r'), API $(adb shell getprop ro.build.version.sdk | tr -d '\r')"

all_ok=0
for apk in "$@"; do
  app="$(basename "$apk" .apk)"
  dir="$out/$app"; mkdir -p "$dir"
  badging="$("$aapt2" dump badging "$apk" 2>/dev/null)"
  pkg="$(echo "$badging" | sed -n "s/^package: name='\([^']*\)'.*/\1/p" | head -n 1)"
  act="$(echo "$badging" | sed -n "s/^launchable-activity: name='\([^']*\)'.*/\1/p" | head -n 1)"
  alive=0; fatal=""; rust_lines=0; nonblank=0
  if [ -z "$pkg" ] || [ -z "$act" ]; then
    fatal="no package or launchable activity in the APK's badging"
  else
    adb logcat -c
    if ! adb install -r -t "$apk" >"$dir/install.log" 2>&1; then
      fatal="adb install failed: $(tail -n 1 "$dir/install.log")"
    else
      adb shell am start -W -n "$pkg/$act" >"$dir/start.log" 2>&1 || true
      sleep "$wait_s"
      adb exec-out screencap -p >"$dir/screenshot.png" 2>/dev/null || true
      [ -n "$(adb shell pidof "$pkg" 2>/dev/null | tr -d '\r')" ] && alive=1
      adb logcat -d -v threadtime >"$dir/logcat.txt" 2>&1 || true
      # the first fatal line OF THIS APP: the JVM or the native side said its process died (other processes of the
      # emulator crash now and then and are not this app's verdict)
      fatal="$(grep -E "FATAL EXCEPTION|Fatal signal|SIGSEGV|SIGABRT|UnsatisfiedLinkError|dlopen failed|ANR in $pkg|Process $pkg .*(died|killed)|AndroidRuntime.*$pkg" "$dir/logcat.txt" | head -n 1)"
      rust_lines="$(grep -c -E 'RustStdoutStderr| azul' "$dir/logcat.txt" || true)"
      python3 - "$dir/screenshot.png" <<'PY' && nonblank=1
import sys
data = open(sys.argv[1], "rb").read()
sys.exit(0 if data[:8] == b"\x89PNG\r\n\x1a\n" and len(data) > 8000 else 1)
PY
      adb shell am force-stop "$pkg" >/dev/null 2>&1 || true
      adb uninstall "$pkg" >/dev/null 2>&1 || true
    fi
  fi
  python3 - "$dir/result.json" "$app" "$pkg" "$alive" "$fatal" "${rust_lines:-0}" "$nonblank" <<'PY'
import json, sys
path, app, pkg, alive, fatal, rust, nonblank = sys.argv[1:]
json.dump({"app": app, "package": pkg, "booted": True, "alive": alive == "1", "fatal": fatal or None,
           "rust_log_lines": int(rust), "screen_nonblank": nonblank == "1"}, open(path, "w"), indent=2)
PY
  echo "[$([ "$alive" = 1 ] && [ -z "$fatal" ] && echo PASS || echo FAIL)] $app: alive=$alive fatal=${fatal:-none} rust_lines=${rust_lines:-0} screen=$nonblank"
  { [ "$alive" = "1" ] && [ -z "$fatal" ]; } || all_ok=1
done
python3 - "$out" <<'PY'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
rows = [json.loads(p.read_text()) for p in sorted(out.glob("*/result.json"))]
(out / "summary.json").write_text(json.dumps(rows, indent=2))
PY
exit "$all_ok"
