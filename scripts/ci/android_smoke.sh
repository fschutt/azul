#!/usr/bin/env bash
# android_smoke.sh: do APKs BOOT? Starts ONE x86_64 emulator, then for every APK: installs it, takes a picture of the
# home screen, starts the launcher activity, waits, takes a screenshot, looks again a few seconds later and reads the log.
#
#   scripts/ci/android_smoke.sh <out-dir> <seconds-to-wait> <apk> [<apk> ...]
#
# For an Ubuntu runner with KVM (without hardware acceleration the emulator is unusable) and the runner's
# Android SDK (build-tools 34 for aapt2). Per APK <out-dir>/<apk name without .apk>/ holds
#   baseline.png     the home screen before the launch (what an app that never came to the front leaves behind)
#   screenshot.png   what the app showed after the wait; screenshot-2.png the second look
#   logcat.txt       the log of that run;  activities.txt  what the system says is resumed;  tombstones.txt
#   result.json      the verdict of scripts/ci/smoke_verdict.py: {app, id, alive, alive_after, resumed, differs_from_home,
#                    screen_nonblank, fatal, pass, reason, ...}
# (and screenshot-after-tap.png, tap.json when the manifest names a button for the app: AZ_SMOKE_TAPS, a JSON map
# of app name -> the label to press, from scripts/demo_apps.toml's [app.smoke] tap; AZ_SMOKE_EXPECTS the same for the text
# the first screen must show) and <out-dir>/summary.json lists them. An app BOOTS (pass) when it is alive twice, is the
# resumed activity, is on the screen (not the home screen, not one flat colour) and left no fatal line; the reason of a
# failure is in result.json. Exit 0 when every app boots; 1 when one does not; 2 when the emulator itself could not be
# started (nothing is said about the apps).
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
  alive=0; alive_after=0; fatal=""; rust_lines=0; resumed=""; expect_text=""; expect_found=""
  verdict_args=()
  if [ -z "$pkg" ] || [ -z "$act" ]; then
    fatal="no package or launchable activity in the APK's badging"
  else
    adb logcat -c
    if ! adb install -r -t "$apk" >"$dir/install.log" 2>&1; then
      fatal="adb install failed: $(tail -n 1 "$dir/install.log")"
    else
      # The launcher, as it is with the app installed: the picture the app's screen is told apart from
      # (scripts/ci/smoke_verdict.py: an app that never came to the front leaves it behind).
      adb shell input keyevent KEYCODE_HOME >/dev/null 2>&1; sleep 2
      adb exec-out screencap -p >"$dir/baseline.png" 2>/dev/null || true
      adb shell am start -W -n "$pkg/$act" >"$dir/start.log" 2>&1 || true
      sleep "$wait_s"
      adb exec-out screencap -p >"$dir/screenshot.png" 2>/dev/null || true
      [ -n "$(adb shell pidof "$pkg" 2>/dev/null | tr -d '\r')" ] && alive=1
      # which app the system itself says is in front
      adb shell dumpsys activity activities >"$dir/activities.txt" 2>&1 || true
      if grep -E "ResumedActivity" "$dir/activities.txt" | grep -q "$pkg/"; then resumed=yes; else resumed=no; fi
      # the text the manifest expects on the first screen ([app.smoke] expect): in uiautomator's dump of it
      expect_text="$(python3 -c 'import json,os,sys; print(json.loads(os.environ.get("AZ_SMOKE_EXPECTS") or "{}").get(sys.argv[1], ""))' "${app%-android-x86_64}")"
      if [ -n "$expect_text" ] && [ "$alive" = "1" ]; then
        adb shell uiautomator dump /sdcard/ui-first.xml >/dev/null 2>&1 && adb pull /sdcard/ui-first.xml "$dir/ui-first.xml" >/dev/null 2>&1
        if grep -qiF -- "$expect_text" "$dir/ui-first.xml" 2>/dev/null; then expect_found="yes"; else expect_found="no"; fi
      fi
      # A button the manifest names ([app.smoke] tap): press it, wait, and look again. Found by its accessibility
      # name in uiautomator's dump; else where the app's own notes put it (130 dp above the bottom edge, centred).
      tap_label="$(python3 -c 'import json,os,sys; print(json.loads(os.environ.get("AZ_SMOKE_TAPS") or "{}").get(sys.argv[1], ""))' "${app%-android-x86_64}")"
      if [ -n "$tap_label" ] && [ "$alive" = "1" ]; then
        tapped="no"
        adb shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1 && adb pull /sdcard/ui.xml "$dir/ui-before.xml" >/dev/null 2>&1
        xy="$(python3 - "$dir/ui-before.xml" "$tap_label" <<'PY'
import re, sys, xml.etree.ElementTree as ET
try:
    root = ET.parse(sys.argv[1]).getroot()
except Exception:
    sys.exit(1)
for n in root.iter("node"):
    if sys.argv[2].lower() in (n.get("text", "") + " " + n.get("content-desc", "")).lower():
        m = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", n.get("bounds", ""))
        if m:
            a, b, c, d = map(int, m.groups())
            print((a + c) // 2, (b + d) // 2)
            sys.exit(0)
sys.exit(1)
PY
)" && tapped="by its name"
        if [ -z "$xy" ]; then
          wh="$(adb shell wm size | grep -o '[0-9]*x[0-9]*' | tail -n 1)"; dpi="$(adb shell wm density | grep -o '[0-9]*' | tail -n 1)"
          xy="$(( ${wh%x*} / 2 )) $(( ${wh#*x} - 130 * ${dpi:-160} / 160 ))"; tapped="by its place"
        fi
        # shellcheck disable=SC2086
        adb shell input tap $xy
        sleep 5
        adb exec-out screencap -p >"$dir/screenshot-after-tap.png" 2>/dev/null || true
        tap_alive=0
        [ -n "$(adb shell pidof "$pkg" 2>/dev/null | tr -d '\r')" ] && tap_alive=1
        adb logcat -d -v threadtime >"$dir/logcat-after-tap.txt" 2>&1 || true
        tap_fatal="$(grep -E "FATAL EXCEPTION|Fatal signal|SIGSEGV|SIGABRT|UnsatisfiedLinkError|ANR in $pkg|Process $pkg .*(died|killed)|AndroidRuntime.*$pkg" "$dir/logcat-after-tap.txt" | head -n 1)"
        python3 - "$dir/tap.json" "$tap_label" "$tapped" "$xy" "$tap_alive" "$tap_fatal" <<'PY'
import json, sys
path, label, how, xy, alive, fatal = sys.argv[1:]
json.dump({"label": label, "found": how, "at": xy, "alive": alive == "1", "fatal": fatal or None}, open(path, "w"), indent=2)
PY
        verdict_args+=(--tap "$dir/tap.json")
      fi
      # The second look, a few seconds after the last thing the app was asked to do: still there, still on the screen.
      sleep 3
      [ -n "$(adb shell pidof "$pkg" 2>/dev/null | tr -d '\r')" ] && alive_after=1
      adb exec-out screencap -p >"$dir/screenshot-2.png" 2>/dev/null || true
      adb logcat -d -v threadtime >"$dir/logcat.txt" 2>&1 || true
      # the first fatal line OF THIS APP: the JVM or the native side said its process died (other processes of the
      # emulator crash now and then and are not this app's verdict)
      fatal="$(grep -E "FATAL EXCEPTION|Fatal signal|SIGSEGV|SIGABRT|UnsatisfiedLinkError|dlopen failed|ANR in $pkg|Process $pkg .*(died|killed)|AndroidRuntime.*$pkg" "$dir/logcat.txt" | head -n 1)"
      rust_lines="$(grep -c -E 'RustStdoutStderr| azul' "$dir/logcat.txt" || true)"
      # tombstones, when the native side died
      adb shell 'ls /data/tombstones 2>/dev/null' >"$dir/tombstones.txt" 2>&1 || true
      adb shell am force-stop "$pkg" >/dev/null 2>&1 || true
      adb uninstall "$pkg" >/dev/null 2>&1 || true
    fi
  fi
  [ -n "$expect_text" ] && verdict_args+=(--expect-text "$expect_text" --expect-found "$expect_found")
  [ -f "$dir/baseline.png" ] && verdict_args+=(--baseline "$dir/baseline.png" --after "$dir/screenshot-2.png")
  # THE pass line: scripts/ci/smoke_verdict.py (alive twice, on the screen, drawn, no fatal line); nothing else decides
  python3 "$(dirname "$0")/smoke_verdict.py" --out "$dir/result.json" --app "$app" --id "${pkg:-}" --alive "$alive" --alive-after "$alive_after" \
    --fatal "$fatal" --rust-lines "${rust_lines:-0}" --resumed "$resumed" ${verdict_args[@]+"${verdict_args[@]}"} || all_ok=1
done
python3 - "$out" <<'PY'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
rows = [json.loads(p.read_text()) for p in sorted(out.glob("*/result.json"))]
(out / "summary.json").write_text(json.dumps(rows, indent=2))
PY
exit "$all_ok"
