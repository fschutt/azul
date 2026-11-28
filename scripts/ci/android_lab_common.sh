# android_lab_common.sh: the SDK and the emulator of the cold-start lab (scripts/ci/android_startup_lab.sh, android_startup_real.sh), sourced.
# The emulator is the QA's (scripts/ci/android_qa.sh): API 34 AOSP image, Pixel 6 profile, swiftshader_indirect, 3 GB, 3 cores; AZ_ANDROID_IMAGE,
# AZ_ANDROID_DEVICE and AZ_LAB_GPU change it. Needs a runner with KVM.

lab_sdk() {   # lab_sdk <out dir>: the SDK on the PATH, the packages installed, ANDROID_NDK_HOME set
  local out="$1"
  : "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
  export ANDROID_HOME ANDROID_SDK_ROOT="$ANDROID_HOME"
  local sdk="$ANDROID_HOME/cmdline-tools/latest/bin"
  [ -x "$sdk/sdkmanager" ] || sdk="$(dirname "$(ls "$ANDROID_HOME"/cmdline-tools/*/bin/sdkmanager | tail -n 1)")"
  export PATH="$sdk:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
  export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
  LAB_IMAGE="${AZ_ANDROID_IMAGE:-system-images;android-34;default;x86_64}"
  LAB_DEVICE="${AZ_ANDROID_DEVICE:-pixel_6}"
  LAB_GPU="${AZ_LAB_GPU:-swiftshader_indirect}"
  yes | sdkmanager --licenses >/dev/null 2>&1 || true
  sdkmanager --install "emulator" "platform-tools" "build-tools;34.0.0" "platforms;android-34" "ndk;27.0.12077973" "$LAB_IMAGE" >"$out/sdkmanager.log" 2>&1 \
    || { echo "sdkmanager failed:"; tail -n 20 "$out/sdkmanager.log"; return 2; }
  export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
  emulator -version 2>&1 | head -n 3 | tee "$out/emulator-version.txt"
}

lab_emulator_up() {   # lab_emulator_up <out dir>: an emulator booted and settled, LAB_EMU = its pid
  local out="$1"
  echo no | avdmanager create avd --force -n qa_lab -k "$LAB_IMAGE" -d "$LAB_DEVICE" >"$out/avd.log" 2>&1 || { cat "$out/avd.log"; return 4; }
  local cfg="$HOME/.android/avd/qa_lab.avd/config.ini"
  { echo "hw.keyboard=no"; echo "hw.ramSize=3072"; echo "disk.dataPartition.size=4G"; } >>"$cfg" 2>/dev/null || true
  if [ -e /dev/kvm ]; then
    echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules >/dev/null
    sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=kvm || true
  fi
  adb kill-server >/dev/null 2>&1 || true
  nohup emulator -avd qa_lab -no-window -no-audio -no-boot-anim -no-snapshot -gpu "$LAB_GPU" -accel on -memory 3072 -cores 3 -no-metrics >"$out/emulator.log" 2>&1 &
  LAB_EMU=$!
  adb start-server >/dev/null 2>&1
  adb wait-for-device
  local booted=0
  for _ in $(seq 1 150); do
    if [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then booted=1; break; fi
    kill -0 "$LAB_EMU" 2>/dev/null || break
    sleep 5
  done
  [ "$booted" = 1 ] || { echo "the emulator did not boot"; tail -n 30 "$out/emulator.log"; return 5; }
  sleep 20
  { adb shell getprop ro.build.fingerprint; adb shell getprop ro.build.version.sdk; adb shell dumpsys window displays | grep -E "init=|cur=" | head -n 3; } | tr -d '\r' | tee "$out/device.txt"
}

lab_emulator_down() {
  adb emu kill >/dev/null 2>&1 || true
  sleep 3
  kill "${LAB_EMU:-0}" 2>/dev/null || true
  wait "${LAB_EMU:-0}" 2>/dev/null || true
}
