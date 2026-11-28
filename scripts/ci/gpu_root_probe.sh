#!/usr/bin/env bash
# gpu_root_probe.sh: the Android GPU root-cause hunt on a runner (EXPERIMENT: never part of a gate). The `mobile-e2e` AzWidgets APK of this
# checkout (its libazul carries the control ladder, the GL call trace and the probes of dll/src/desktop/shell2/common/gles_control.rs) on an
# x86_64 emulator, once per emulator GPU mode and per variant: a screenshot every second while the app starts, the whole logcat.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/gpu_root_probe.sh' --codegen --artifacts 'demo-out/gpuroot/**'
#
#   AZ_PROBE_MODES     the emulator GPU modes, default "swiftshader_indirect,swangle_indirect"
#   AZ_PROBE_VARIANTS  `name=ENV` pairs separated by `|` (ENV is the value of the system property debug.az.env), default
#                      "control=AZ_BACKEND=gpu;AZ_GPU_CONTROL=1|plain=AZ_BACKEND=gpu|cpu=AZ_BACKEND=cpu"
#   AZ_PROBE_SECONDS   how long each variant is watched, default 45
#   AZ_PROBE_IMAGE     the system image, default system-images;android-34;default;x86_64
#   AZ_PROBE_PREBUILT  a folder with AzWidgets-e2e-android-x86_64.apk (skips the lib and app build)
set -uo pipefail

target=x86_64-linux-android
col=android-x86_64
variant=mobile-e2e
out="demo-out/gpuroot"
mkdir -p "$out" demo-out/libs demo-out/apps demo-out/results

if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  python3 -m pip install --quiet --user tomli || true
fi
export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"

apk=""
if [ -n "${AZ_PROBE_PREBUILT:-}" ] && [ -f "$AZ_PROBE_PREBUILT/AzWidgets-e2e-android-x86_64.apk" ]; then
  apk="$AZ_PROBE_PREBUILT/AzWidgets-e2e-android-x86_64.apk"
else
  rustup target add "$target"
  features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
  echo "== $variant: $features"
  start=$(date +%s)
  bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant"
  status=$?
  echo "LIB $variant exit $status SECONDS: $(( $(date +%s) - start ))"
  [ "$status" -eq 0 ] || exit "$status"
  ls -l demo-out/libs/*/libazul.so
  python3 scripts/demo_build.py android --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps \
    --results demo-out/results --variant "$variant"
  echo "APP exit $?"
  apk="$(ls demo-out/apps/AzWidgets-e2e-android-x86_64.apk 2>/dev/null | head -n 1)"
fi
[ -n "$apk" ] || { echo "no APK was built"; exit 1; }
cp "$apk" "$out/" 2>/dev/null || true

sdk="$ANDROID_HOME/cmdline-tools/latest/bin"
export PATH="$sdk:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
image="${AZ_PROBE_IMAGE:-system-images;android-34;default;x86_64}"
aapt2="$ANDROID_HOME/build-tools/34.0.0/aapt2"
if [ -e /dev/kvm ]; then
  echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules >/dev/null
  sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=kvm || true
else
  echo "::warning::no /dev/kvm on this runner"
fi
yes | sdkmanager --licenses >/dev/null 2>&1 || true
sdkmanager --install "emulator" "platform-tools" "build-tools;34.0.0" "$image" >"$out/sdkmanager.log" 2>&1 \
  || { echo "sdkmanager failed:"; tail -n 20 "$out/sdkmanager.log"; exit 2; }
emulator -version 2>&1 | head -n 3 | tee "$out/emulator-version.txt"
badging="$("$aapt2" dump badging "$apk" 2>/dev/null || true)"
pkg="$(echo "$badging" | sed -n "s/^package: name='\([^']*\)'.*/\1/p" | head -n 1)"
act="$(echo "$badging" | sed -n "s/^launchable-activity: name='\([^']*\)'.*/\1/p" | head -n 1)"
[ -n "$pkg" ] || { echo "no package in the APK's badging"; exit 2; }
echo "apk: $apk package=$pkg activity=${act:-?}"

modes="${AZ_PROBE_MODES:-swiftshader_indirect,swangle_indirect}"
variants="${AZ_PROBE_VARIANTS:-control=AZ_BACKEND=gpu;AZ_GPU_CONTROL=1|plain=AZ_BACKEND=gpu|cpu=AZ_BACKEND=cpu}"
seconds="${AZ_PROBE_SECONDS:-45}"

IFS=',' read -r -a mode_list <<<"$modes"
for mode in "${mode_list[@]}"; do
  d="$out/$mode"; mkdir -p "$d/shots" "$d/logs"
  echo "================ emulator GPU mode: $mode ================"
  adb kill-server >/dev/null 2>&1 || true
  echo no | avdmanager create avd --force -n "probe_$mode" -k "$image" -d pixel_6 >"$d/avd.log" 2>&1 \
    || { cat "$d/avd.log"; echo "avdmanager failed"; continue; }
  cfg="$HOME/.android/avd/probe_$mode.avd/config.ini"
  { echo "hw.keyboard=no"; echo "hw.ramSize=3072"; echo "disk.dataPartition.size=4G"; } >>"$cfg" 2>/dev/null || true
  nohup emulator -avd "probe_$mode" -no-window -no-audio -no-boot-anim -no-snapshot -gpu "$mode" -accel on \
    -memory 3072 -cores 3 -no-metrics >"$d/emulator.log" 2>&1 &
  emu=$!
  adb start-server >/dev/null 2>&1
  adb wait-for-device
  booted=0
  for _ in $(seq 1 150); do
    if [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then booted=1; break; fi
    kill -0 "$emu" 2>/dev/null || break
    sleep 5
  done
  if [ "$booted" != "1" ]; then
    echo "the emulator did not finish booting in mode $mode"; tail -n 30 "$d/emulator.log"
    adb emu kill >/dev/null 2>&1 || true; kill "$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true
    continue
  fi
  echo "emulator booted: $(adb shell getprop ro.product.cpu.abi | tr -d '\r'), API $(adb shell getprop ro.build.version.sdk | tr -d '\r')"
  sleep 20
  { adb shell getprop ro.hardware.egl; adb shell getprop ro.kernel.qemu.gles; adb shell getprop ro.opengles.version; adb shell getprop ro.hardware.vulkan; } 2>&1 | tr -d '\r' >"$d/gl-props.txt"
  adb install -r -t "$apk" >"$d/install.log" 2>&1 || { echo "install failed"; cat "$d/install.log"; }
  adb logcat -G 64M >/dev/null 2>&1 || true
  adb shell setprop debug.egl.trace error
  adb shell setprop debug.egl.debug_proc "$pkg"
  # the screen's own pictures: the empty home screen first, as the baseline
  adb exec-out screencap -p >"$d/shots/000-home.png" 2>/dev/null || true

  IFS='|' read -r -a variant_list <<<"$variants"
  for item in "${variant_list[@]}"; do
    name="${item%%=*}"; env="${item#*=}"
    echo "---- $mode / $name: debug.az.env=AZ_LOG=info;$env"
    adb shell am force-stop "$pkg"; sleep 2
    adb shell "setprop debug.az.env 'AZ_LOG=info;$env'"
    adb logcat -c
    adb shell input keyevent KEYCODE_WAKEUP
    adb shell am start -n "$pkg/${act:-.MainActivity}" >"$d/logs/start-$name.txt" 2>&1
    t0=$(date +%s); i=0
    while [ $(( $(date +%s) - t0 )) -lt "$seconds" ]; do
      ts="$(adb shell date +%H%M%S 2>/dev/null | tr -d '\r')"
      adb exec-out screencap -p >"$d/shots/$name-$(printf %03d "$i")-$ts.png" 2>/dev/null || true
      i=$((i+1))
    done
    adb logcat -d -v threadtime >"$d/logs/logcat-$name.txt" 2>&1
    gzip -f "$d/logs/logcat-$name.txt" || true
    adb shell dumpsys SurfaceFlinger --list >"$d/logs/sf-layers-$name.txt" 2>&1 || true
    adb shell dumpsys SurfaceFlinger 2>&1 | gzip -c >"$d/logs/sf-full-$name.txt.gz" || true
    adb shell dumpsys meminfo "$pkg" >"$d/logs/meminfo-$name.txt" 2>&1 || true
    adb shell am force-stop "$pkg"
    echo "variant $name: $i screenshots"
  done
  adb shell "setprop debug.az.env ''"
  adb emu kill >/dev/null 2>&1 || true
  kill "$emu" 2>/dev/null || true
  wait "$emu" 2>/dev/null || true
done
echo "gpu root probe: done"
exit 0
