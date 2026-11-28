#!/usr/bin/env bash
# android_probe.sh <out-dir> <apk> [android_probe.py options]: boots ONE x86_64 emulator (the AOSP image of the QA, API 34, KVM) and runs scripts/ci/android_probe.py
# on the APK: install, properties, cold starts, one logcat per start, a marker line, the permission dialogs that ever showed.
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_probe.sh out <apk> --prop debug.az.selftest=1 --marker AZ_SELFTEST_REPORT --watch-permissions' --artifacts 'out/**'
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_probe.sh out <apk> --clear-data --starts 2 --startup --marker AZ_STARTUP' --artifacts 'out/**'
# AZ_PROBE_PY names another script to run on the booted emulator in place of android_probe.py (it gets the same arguments; the attended
# permission proof of the Android shell, scripts/ci/android_permission_proof.py, imports android_probe for its helpers).
# AZ_ANDROID_IMAGE overrides the system image (default system-images;android-34;default;x86_64). Exit: 2 when the emulator or the install failed,
# else what android_probe.py says (0 good, 3 the marker never appeared, 4 a permission dialog showed). See android_probe.py for the options.
set -uo pipefail
out="${1:?out-dir}"; apk="${2:?apk}"; shift 2
mkdir -p "$out"
: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
export ANDROID_HOME
sdk="$ANDROID_HOME/cmdline-tools/latest/bin"
[ -x "$sdk/sdkmanager" ] || sdk="$(dirname "$(ls "$ANDROID_HOME"/cmdline-tools/*/bin/sdkmanager | tail -n 1)")"
export PATH="$sdk:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$ANDROID_HOME/build-tools/34.0.0:$PATH"
image="${AZ_ANDROID_IMAGE:-system-images;android-34;default;x86_64}"
if [ -e /dev/kvm ]; then
  echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules >/dev/null
  sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=kvm || true
else
  echo "::warning::no /dev/kvm on this runner: the emulator will be very slow or refuse to start"
fi
yes | sdkmanager --licenses >/dev/null 2>&1 || true
sdkmanager --install "emulator" "platform-tools" "build-tools;34.0.0" "$image" >"$out/sdkmanager.log" 2>&1 \
  || { echo "sdkmanager failed:"; tail -n 20 "$out/sdkmanager.log"; exit 2; }
echo no | avdmanager create avd --force -n probe -k "$image" -d pixel_6 >"$out/avd.log" 2>&1 || { cat "$out/avd.log"; exit 2; }
nohup emulator -avd probe -no-window -no-audio -no-boot-anim -no-snapshot -gpu swiftshader_indirect -accel on -memory 3072 -cores 3 >"$out/emulator.log" 2>&1 &
emu=$!
trap 'adb emu kill >/dev/null 2>&1 || true; kill "$emu" 2>/dev/null || true' EXIT
adb start-server >/dev/null 2>&1
adb wait-for-device
booted=0
for _ in $(seq 1 120); do
  if [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then booted=1; break; fi
  kill -0 "$emu" 2>/dev/null || break
  sleep 5
done
[ "$booted" = "1" ] || { echo "the emulator did not finish booting"; tail -n 30 "$out/emulator.log"; exit 2; }
adb shell input keyevent 82 >/dev/null 2>&1 || true
adb shell settings put global window_animation_scale 0 >/dev/null 2>&1 || true
echo "emulator booted: $(adb shell getprop ro.product.cpu.abi | tr -d '\r'), API $(adb shell getprop ro.build.version.sdk | tr -d '\r')"
python3 "${AZ_PROBE_PY:-$(dirname "$0")/android_probe.py}" --apk "$apk" --out "$out" "$@"
exit $?
