#!/usr/bin/env bash
# android_tabs_job.sh: ONE runner job: the tab strip on an emulator with the Pixel 8's camera cutout (examples/azul-widgets/e2e/tab_touch*.json).
# It builds the QA variant of AzWidgets (the `mobile-e2e` phone library: debug server and e2e scripting) from the tree it runs in, starts an emulator
# (the Pixel 8 profile when the runner's avdmanager has it, else Pixel 6: same 1080 x 2400 screen), turns the Pixel 8's cutout on (cmd overlay
# enable-exclusive com.android.internal.emulation.pixel_8; the overlay named ...cutout.emulation.hole does not exist on the AOSP image) and runs
# scripts/ci/android_qa.sh's `tabs` session: portrait, then the phone turned to landscape (the hole at the side); per orientation the device's own
# pictures of a plain start (light, dark), and per scenario file a recording and the engine's frames (demo-out/qa-tabs/<mode>/tabs-<orientation>/).
#
#   scripts/remote.sh run <full sha | ref> ubuntu-22.04 'bash scripts/ci/android_tabs_job.sh' --codegen --artifacts 'demo-out/qa-tabs/**
#   demo-out/results/*
#   demo-out/plan.txt'
# AZ_TABS_FLICKER=1 adds, AFTER the tab strip session, the plain AzWidgets (the phone library without the QA hooks, built here too) and the flicker and
# jitter audit of it with the QA APK for the pan (scripts/ci/qa_android_flicker.sh; add 'demo-out/qa-flicker*/**' to the artifacts): the black screens
# at the start and on resume (E46, E47), idle redraws, the pan of the page.
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
python3 scripts/demo_build.py android --target "$target" --apps "AzWidgets AzWriter" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "$variant" \
  || echo "demo_build.py returned non-zero"
ls -la demo-out/apps
apk="demo-out/apps/AzWidgets-e2e-$col.apk"
[ -e "$apk" ] || { echo "no $apk"; exit 3; }

# AzMail (the user's ask: how does it look on a phone): AZ_TABS_AZMAIL_RUN=<run id of `android_build_smoke.sh AzMail`> takes its APK from that run's
# artifact (demo-out/apps/AzMail-android-x86_64.apk) and puts the app on the same phone, portrait and landscape; the sample mailbox (the files the
# kit's `--sample` writes, which a phone cannot be given) is generated from examples/azul-mail/scripts/sample_mail by azmail_sample_data.py
if [ -n "${AZ_TABS_AZMAIL_RUN:-}" ]; then
  mkdir -p azmail-apk
  if gh run download "$AZ_TABS_AZMAIL_RUN" -R "$GITHUB_REPOSITORY" -n remote-Linux -D azmail-apk; then
    azmail_apk="$(ls azmail-apk/demo-out/apps/AzMail-android-x86_64.apk 2>/dev/null | head -n 1)"
    if [ -n "$azmail_apk" ]; then
      export AZ_QA_AZMAIL_APK="$azmail_apk"
      python3 scripts/ci/azmail_sample_data.py demo-out/azmail-sample --repo . && export AZ_QA_AZMAIL_DATA="demo-out/azmail-sample/azmail-data"
    else
      echo "no AzMail APK in run $AZ_TABS_AZMAIL_RUN"
    fi
  else
    echo "the artifact of run $AZ_TABS_AZMAIL_RUN could not be downloaded"
  fi
fi

# the AzWriter ribbon: its QA build gets a first look on the same phone (the tab strip's other home)
[ -e "demo-out/apps/AzWriter-e2e-$col.apk" ] && export AZ_QA_TABS_EXTRA_APKS="demo-out/apps/AzWriter-e2e-$col.apk"

# the emulator: Pixel 8 (its profile), the hole overlay; portrait and landscape inside the session
AZ_ANDROID_DEVICE=pixel_8 AZ_ANDROID_OVERLAYS="com.android.internal.emulation.pixel_8 com.android.systemui.emulation.pixel_8" \
  timeout 4500 bash scripts/ci/android_qa.sh demo-out/qa-tabs "$apk" swiftshader_indirect "AzWidgets tabs" tabs
echo "SESSION tabs: exit $?"
if [ "${AZ_TABS_FLICKER:-}" = 1 ]; then
  plain_features="$(python3 scripts/demo_apps.py features "$col" mobile)"
  start=$(date +%s)
  if bash scripts/demo_lib.sh android "$target" "$plain_features" "demo-out/libs/demolib-$col-mobile" \
     && python3 scripts/demo_build.py android --target "$target" --apps "AzWidgets AzWriter" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant ""; then
    echo "PLAIN SECONDS: $(( $(date +%s) - start ))"
    mkdir -p flick && cp "demo-out/apps/AzWidgets-$col.apk" "$apk" flick/
    timeout 2400 bash scripts/ci/qa_android_flicker.sh flick
    echo "SESSION flicker: exit $?"
    # E46 once more for AzWriter: the flash audit (cold start, rotation, dark mode, home and back) of its plain APK on its own boot, its own folder
    if [ -e "demo-out/apps/AzWriter-$col.apk" ]; then
      AZ_QA_OUT=demo-out/qa-flicker-azwriter timeout 1500 bash scripts/ci/qa_android_flicker.sh "demo-out/apps/AzWriter-$col.apk"
      echo "SESSION flicker azwriter: exit $?"
    fi
  else
    echo "the plain AzWidgets did not build: no flicker audit"
  fi
fi
exit 0
