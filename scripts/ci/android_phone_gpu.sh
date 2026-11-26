#!/usr/bin/env bash
# android_phone_gpu.sh: the GPU renderer on a real phone (an attached device, USB debugging on): the one thing the emulator and
# the runners cannot show, because Adreno, Mali and PowerVR are not SwiftShader.
#
#   scripts/ci/android_phone_gpu.sh <app.apk> [<package>] [<out-dir>]
#
# Installs the APK, starts it once with the CPU renderer and once with the GPU renderer (AZ_BACKEND through the system property the
# engine reads, `debug.az.env`), and keeps per renderer: the engine's `[render]` lines (which renderer ran, on which GL_RENDERER, and
# why not the GPU when it fell back), a screenshot, the first-frame time (`am start -W`), the frame times of a 3 s scroll
# (SurfaceFlinger) and the memory (`dumpsys meminfo`). Needs only `adb`; <package> defaults to the one the APK names (aapt2 on the
# PATH) or com.azul.AzWidgets. Nothing is deleted: the out-dir (default ./android-phone-gpu) gets the files, a second run overwrites.
set -uo pipefail

apk="${1:?the APK to install}"
pkg="${2:-}"
out="${3:-android-phone-gpu}"
mkdir -p "$out"

adb get-state >/dev/null 2>&1 || { echo "no device: attach a phone with USB debugging and accept the key"; exit 2; }
if [ -z "$pkg" ] && command -v aapt2 >/dev/null 2>&1; then
  pkg="$(aapt2 dump badging "$apk" 2>/dev/null | sed -n "s/^package: name='\([^']*\)'.*/\1/p" | head -n 1)"
fi
pkg="${pkg:-com.azul.AzWidgets}"
echo "device: $(adb shell getprop ro.product.model | tr -d '\r'), Android $(adb shell getprop ro.build.version.release | tr -d '\r'), GLES $(adb shell getprop ro.opengles.version | tr -d '\r')"
echo "package: $pkg"
adb install -r -t "$apk" | tail -n 1

for mode in cpu gpu; do
  echo "================ AZ_BACKEND=$mode ================"
  adb shell setprop debug.az.env "AZ_LOG=info;AZ_BACKEND=$mode"
  adb logcat -c
  adb shell input keyevent KEYCODE_WAKEUP
  act="$(adb shell cmd package resolve-activity --brief -c android.intent.category.LAUNCHER "$pkg" | tr -d '\r' | tail -n 1)"
  adb shell am start -W -S -n "$act" | tee "$out/$mode-start.txt" | grep -E "TotalTime|ThisTime"
  sleep 6
  adb exec-out screencap -p >"$out/$mode.png"
  adb logcat -d -v threadtime >"$out/$mode-logcat.txt"
  echo "--- what the engine says about its renderer:"
  grep -E "\[render\]" "$out/$mode-logcat.txt" | sed 's/^.*\[render\]/[render]/' | sort -u | head -n 12
  echo "--- scroll for 3 s, frame times (SurfaceFlinger):"
  layer="$(adb shell dumpsys SurfaceFlinger --list | tr -d '\r' | grep "$pkg" | grep -v -E "Surface\(|Task=|Splash|Background" | head -n 1)"
  adb shell dumpsys SurfaceFlinger --latency-clear "$layer" >/dev/null 2>&1
  size="$(adb shell wm size | tr -d '\r' | sed -n 's/.*: \([0-9]*\)x\([0-9]*\).*/\1 \2/p' | tail -n 1)"
  w="${size% *}"; h="${size#* }"
  adb shell input swipe $((w / 2)) $((h * 72 / 100)) $((w / 2)) $((h * 28 / 100)) 3000
  adb shell dumpsys SurfaceFlinger --latency "$layer" >"$out/$mode-latency.txt" 2>&1
  python3 - "$out/$mode-latency.txt" <<'PY'
import sys
rows = [l.split() for l in open(sys.argv[1]).read().splitlines()[1:] if len(l.split()) == 3]
ts = sorted(int(r[1]) for r in rows if 0 < int(r[1]) < 9223372036854775807)
iv = sorted((b - a) / 1e6 for a, b in zip(ts, ts[1:]))
if iv:
    print(f"frames {len(iv)}: median {iv[len(iv)//2]:.1f} ms, p95 {iv[int(len(iv)*0.95)]:.1f} ms, max {iv[-1]:.1f} ms")
else:
    print("no frame times (the layer was not found)")
PY
  pid="$(adb shell pidof "$pkg" | tr -d '\r' | awk '{print $1}')"
  adb shell dumpsys meminfo "${pid:-0}" | grep -E "TOTAL PSS|TOTAL RSS|Graphics" | head -n 3
done
adb shell setprop debug.az.env ""
echo "files: $out/{cpu,gpu}.png, *-logcat.txt (the whole log), *-latency.txt"
echo "if the GPU run says 'gpu unavailable at <stage>', send that line and $out/gpu-logcat.txt"
