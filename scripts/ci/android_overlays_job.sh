#!/usr/bin/env bash
# android_overlays_job.sh: ONE runner job: the popups, dialogs and menus of a window with ONE surface on an emulator, and AzMail's second window.
# It builds the QA variant of AzWidgets (the `mobile-e2e` phone library: debug server and e2e scripting) from the tree it runs in, starts an emulator
# (the Pixel 8 profile when the runner's avdmanager has it, else Pixel 6: same 1080 x 2400 screen) and runs scripts/ci/android_qa.sh's `tabs` session
# with the scenario examples/azul-widgets/e2e/overlays_phone.json, portrait then the phone turned to landscape: per orientation the device's own
# pictures of a plain start and, per file, a recording and the engine's frames (demo-out/qa-tabs/<mode>/tabs-<orientation>/). With
# AZ_OVERLAYS_AZMAIL_RUN=<run id of `android_build_smoke.sh AzMail`> AzMail's APK of that run goes on the same phone, its sample mailbox with it: the
# session taps `New E-mail` (scripts/ci/android_azmail.py) and takes the device's picture of what the screen shows then - the compose window, which
# is a view pushed over the inbox on a phone.
#
#   scripts/remote.sh run <full sha | ref> ubuntu-22.04 'AZ_OVERLAYS_AZMAIL_RUN=<id> bash scripts/ci/android_overlays_job.sh' --codegen --artifacts 'demo-out/qa-tabs/**
#   demo-out/results/*
#   demo-out/plan.txt'
set -uxo pipefail
target=x86_64-linux-android
col=android-x86_64
variant=mobile-e2e
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
  python3 -m pip install --quiet --user tomli || true
fi
export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
rustup target add "$target"
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
echo "FEATURES $variant: $features"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant" || { echo "LIB $variant FAILED"; exit 3; }
echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"
python3 scripts/demo_build.py android --target "$target" --apps "AzWidgets" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "$variant" \
  || echo "demo_build.py returned non-zero"
ls -la demo-out/apps
apk="demo-out/apps/AzWidgets-e2e-$col.apk"
[ -e "$apk" ] || { echo "no $apk"; exit 3; }

# AzMail: the APK of the build run (its Android shell is the one this tree compiled), on the same phone with the sample mailbox
if [ -n "${AZ_OVERLAYS_AZMAIL_RUN:-}" ]; then
  mkdir -p azmail-apk
  if gh run download "$AZ_OVERLAYS_AZMAIL_RUN" -R "$GITHUB_REPOSITORY" -n remote-Linux -D azmail-apk; then
    azmail_apk="$(ls azmail-apk/demo-out/apps/AzMail-android-x86_64.apk 2>/dev/null | head -n 1)"
    if [ -n "$azmail_apk" ]; then
      export AZ_QA_AZMAIL_APK="$azmail_apk"
      python3 scripts/ci/azmail_sample_data.py demo-out/azmail-sample --repo . && export AZ_QA_AZMAIL_DATA="demo-out/azmail-sample/azmail-data"
    else
      echo "no AzMail APK in run $AZ_OVERLAYS_AZMAIL_RUN"
    fi
  else
    echo "the artifact of run $AZ_OVERLAYS_AZMAIL_RUN could not be downloaded"
  fi
fi

# The soft keyboard is no part of the engine's frames and the recording of the session is not playable (screenrecord is stopped before it
# writes its index): while the session runs, a device picture is taken whenever the keyboard is on the screen (`mInputShown`), a frame that
# differs from the last, at most 24 of them - the dialog with the text field standing on the keyboard.
export PATH="$ANDROID_HOME/platform-tools:$PATH"
mkdir -p demo-out/qa-tabs/keyboard-up
(
  n=0
  last=""
  while [ "$n" -lt 24 ]; do
    if adb shell dumpsys input_method 2>/dev/null | grep -q "mInputShown=true"; then
      adb exec-out screencap -p > demo-out/qa-tabs/keyboard-up/current.png 2>/dev/null
      sum="$(md5sum demo-out/qa-tabs/keyboard-up/current.png 2>/dev/null | cut -d' ' -f1)"
      if [ -s demo-out/qa-tabs/keyboard-up/current.png ] && [ "$sum" != "$last" ]; then
        n=$((n + 1))
        cp demo-out/qa-tabs/keyboard-up/current.png "demo-out/qa-tabs/keyboard-up/$(printf '%03d' "$n").png"
        last="$sum"
      fi
      sleep 2
    else
      sleep 1
    fi
  done
) >/dev/null 2>&1 &
sampler=$!
AZ_QA_TABS_FILES="${AZ_QA_TABS_FILES:-overlays_phone}" \
AZ_ANDROID_DEVICE=pixel_8 AZ_ANDROID_OVERLAYS="com.android.internal.emulation.pixel_8 com.android.systemui.emulation.pixel_8" \
  timeout 4500 bash scripts/ci/android_qa.sh demo-out/qa-tabs "$apk" swiftshader_indirect "AzWidgets overlays" tabs
echo "SESSION overlays: exit $?"
kill "$sampler" 2>/dev/null || true
rm -f demo-out/qa-tabs/keyboard-up/current.png
ls -la demo-out/qa-tabs/keyboard-up | head -30
exit 0
