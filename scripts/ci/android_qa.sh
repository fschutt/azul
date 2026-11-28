#!/usr/bin/env bash
# android_qa.sh: the lifecycle QA of ONE APK on an x86_64 emulator, once per GPU mode of the emulator.
#
#   scripts/ci/android_qa.sh <out-dir> <apk> [<gpu-modes: comma list, default swiftshader_indirect,guest>] [<label>] [<session>]
#
#   session  qa (default): scripts/ci/mobile_qa.py, the lifecycle of the plain APK;
#            e2e: scripts/ci/mobile_e2e.py, the debug server of a `mobile-e2e` APK driven from the runner (the scenario
#                 files named in AZ_QA_SCENARIOS, space separated);
#            both: qa, then e2e on the same boot;
#            flicker: idle redraws, flash frames and pan jitter (scripts/ci/flicker_audit.py; AZ_QA_FLICKER_E2E, AZ_QA_FLICKER_MAPS);
#            matrix: every e2e scenario file against its host app (scripts/ci/e2e_matrix.py; AZ_QA_MATRIX_APKS: the e2e APKs);
#            tabs: the tab strip (examples/azul-widgets/e2e/tab_touch*.json) on the emulator's phone, portrait then landscape: the
#                 engine's frames of every file (AZ_E2E_SHOT_DIR, pulled), the device's own pictures of a plain start (light, dark) and a
#                 recording of each run; AZ_ANDROID_DEVICE=pixel_8 (when avdmanager has the profile) and
#                 AZ_ANDROID_OVERLAYS=com.android.internal.emulation.pixel_8 give it the Pixel 8's camera cutout (first mode only);
#                 AZ_QA_AZMAIL_APK (and AZ_QA_AZMAIL_DATA) adds AzMail on the same phone per orientation (scripts/ci/android_azmail.py);
#            fontwait: the font-wait A/B of the startup agent's E73 lever (scripts/ci/font_wait_series.py): cold starts of one app, build A against build B, from an empty
#                 font cache (pm clear before every start) and with the cache filled; AZ_QA_FONTWAIT_PLAN="control:A=a.apk;control:B=b.apk;bundled:A=...;bundled:B=...",
#                 AZ_QA_FONTWAIT_STARTS starts per block (default 10); all builds are the same package, on one boot (first mode only);
#            audit: the phone-fit audit of QA-variant APKs (scripts/ci/mobile_e2e.py --audit-only) on the first mode only: the
#                 APK, then every APK of AZ_QA_AUDIT_APKS (space separated), each in its own folder <out>/<mode>/audit/<apk name>/
#                 (layout-audit.md and .json, the dumps, both pictures per orientation, the dark-mode verdict, and the app's own
#                 script: AzPaint paint, AzWriter writer, AzMaps maps, AzMeet meet).
#
# For an Ubuntu runner with KVM (without it the emulator is unusable) and the runner's Android SDK. Per GPU
# mode <out-dir>/<mode>/ holds what scripts/ci/mobile_qa.py writes (qa.json, qa.md, shots/, logs/) and the
# emulator's own log. Exit 0 when the emulators ran (the verdicts are in the qa.json files: a failing app is a
# RESULT of this job, not a failure of it); 2 when the SDK could not be set up.
#
# What the emulator's GPU modes are (emulator -help-gpu): `swiftshader_indirect` = OpenGL ES 3 in software on the
# host, so the guest's compositor (SurfaceFlinger) has a GL ES it can use; `guest` = the guest renders with its
# own software GL ES and the host only shows the pixels; `off` = no GL ES emulation at all. The azul app draws
# with its own CPU renderer into the window's buffer in every mode (dll/src/desktop/shell2/android: ANativeWindow_lock),
# so the modes differ in how the SYSTEM composes that buffer, not in how the app renders.
set -uo pipefail

out="${1:?out-dir}"; apk="${2:?apk}"; modes="${3:-swiftshader_indirect,guest}"; label="${4:-AzWidgets}"; session="${5:-qa}"
here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$out"

: "${ANDROID_HOME:=${ANDROID_SDK_ROOT:-/usr/local/lib/android/sdk}}"
export ANDROID_HOME ANDROID_SDK_ROOT="$ANDROID_HOME"
sdk="$ANDROID_HOME/cmdline-tools/latest/bin"
[ -x "$sdk/sdkmanager" ] || sdk="$(dirname "$(ls "$ANDROID_HOME"/cmdline-tools/*/bin/sdkmanager | tail -n 1)")"
export PATH="$sdk:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"
image="${AZ_ANDROID_IMAGE:-system-images;android-34;default;x86_64}"
# the device profile of the AVD (avdmanager list device): AZ_ANDROID_DEVICE=pixel_8; AZ_ANDROID_OVERLAYS: system overlays to enable after the boot
# (`com.android.internal.emulation.pixel_8` = the Pixel 8's camera cutout: the system draws it and the app gets its display cutout)
device="${AZ_ANDROID_DEVICE:-pixel_6}"
# The package the APK declares (aapt2 of build-tools 34)
aapt2="$ANDROID_HOME/build-tools/34.0.0/aapt2"

if [ -e /dev/kvm ]; then
  echo 'KERNEL=="kvm", GROUP="kvm", MODE="0666", OPTIONS+="static_node=kvm"' | sudo tee /etc/udev/rules.d/99-kvm4all.rules >/dev/null
  sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=kvm || true
else
  echo "::warning::no /dev/kvm on this runner: the emulator will be very slow or refuse to start"
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
printf '%s\n' "$badging" >"$out/badging.txt"

# AZ_QA_SERIES=10: ten cold starts of the app measured with `am start -W`; AZ_FLOOR=1 also builds the floor APK (an empty Java
# Activity, scripts/floor-apk/build.sh) and measures it the same way on the same emulator
floor_apk=""
if [ "${AZ_FLOOR:-}" = 1 ]; then
  yes | sdkmanager --install "platforms;android-34" >>"$out/sdkmanager.log" 2>&1 || true
  # an ABSOLUTE path: the floor script makes its work directory next to the APK and cd's into it (a relative demo-out/qa/floor-work.X broke the zip)
  out_abs="$(cd "$out" && pwd)"
  if JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}" bash "$here/../floor-apk/build.sh" "$out_abs/AzFloor-android.apk" >"$out/floor-build.log" 2>&1; then
    floor_apk="$out_abs/AzFloor-android.apk"
  else
    echo "the floor APK did not build:"; tail -n 8 "$out/floor-build.log"
  fi
fi

IFS=',' read -r -a mode_list <<<"$modes"
for mode in "${mode_list[@]}"; do
  d="$out/$mode${AZ_QA_BACKEND:+-$AZ_QA_BACKEND}"; mkdir -p "$d"   # AZ_QA_BACKEND=cpu|gpu|auto: a run of that renderer, its own folder
  echo "================ emulator GPU mode: $mode ================"
  adb kill-server >/dev/null 2>&1 || true
  avdmanager list device -c 2>/dev/null | grep -qx "$device" || { echo "no device profile $device: pixel_6"; device=pixel_6; }
  echo no | avdmanager create avd --force -n "qa_$mode" -k "$image" -d "$device" >"$d/avd.log" 2>&1 \
    || { cat "$d/avd.log"; echo '{"booted": false, "reason": "avdmanager failed"}' >"$d/boot.json"; continue; }
  # a soft keyboard on the screen (an emulator with a hardware keyboard shows none)
  cfg="$HOME/.android/avd/qa_$mode.avd/config.ini"
  { echo "hw.keyboard=no"; echo "hw.ramSize=3072"; echo "disk.dataPartition.size=4G"; } >>"$cfg" 2>/dev/null || true
  boot_start=$(date +%s)
  nohup emulator -avd "qa_$mode" -no-window -no-audio -no-boot-anim -no-snapshot -gpu "$mode" -accel on \
    -memory 3072 -cores 3 -no-metrics >"$d/emulator.log" 2>&1 &
  emu=$!
  adb start-server >/dev/null 2>&1
  adb wait-for-device
  booted=0
  for _ in $(seq 1 150); do   # 12.5 minutes
    if [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = "1" ]; then booted=1; break; fi
    kill -0 "$emu" 2>/dev/null || break
    sleep 5
  done
  if [ "$booted" != "1" ]; then
    echo "the emulator did not finish booting in mode $mode"; tail -n 30 "$d/emulator.log"
    printf '{"booted": false, "mode": "%s", "reason": "no sys.boot_completed"}\n' "$mode" >"$d/boot.json"
    adb emu kill >/dev/null 2>&1 || true; kill "$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true
    continue
  fi
  boot_s=$(( $(date +%s) - boot_start ))
  printf '{"booted": true, "mode": "%s", "boot_seconds": %d}\n' "$mode" "$boot_s" >"$d/boot.json"
  echo "emulator booted in ${boot_s}s: $(adb shell getprop ro.product.cpu.abi | tr -d '\r'), API $(adb shell getprop ro.build.version.sdk | tr -d '\r')"
  sleep 20   # the launcher and system UI settle (a first screenshot of a booting system shows an ANR dialog)
  if [ -n "${AZ_ANDROID_OVERLAYS:-}" ] && [ "$mode" = "${mode_list[0]}" ]; then
    # `com.android.internal.emulation.pixel_8` = the Pixel 8's camera cutout and `com.android.systemui.emulation.pixel_8` = its status bar and corners (the system
    # image carries one overlay of each per Pixel; the AVD's own profile is enabled at the boot: pixel_6 for a pixel_6 AVD). The wanted ones are enabled, then every
    # OTHER emulation overlay of the two families that is still on is turned off (two cutouts are not a phone): the first run of this job left systemui's pixel_6 on.
    # The first listing is what the image had, the last what it has now.
    adb shell cmd overlay list 2>/dev/null | tr -d '\r' | grep -i -E "cutout|emulation" | sed 's/^/before: /' >>"$d/overlays.log" || true
    for ov in $AZ_ANDROID_OVERLAYS; do adb shell cmd overlay enable "$ov" 2>&1 | tee -a "$d/overlays.log"; done
    for other in $(adb shell cmd overlay list 2>/dev/null | tr -d '\r' | sed -n 's/^\[x\] \(com\.android\.[a-z]*\.emulation\..*\)$/\1/p'); do
      case " $AZ_ANDROID_OVERLAYS " in *" $other "*) ;; *) adb shell cmd overlay disable "$other" 2>&1 | tee -a "$d/overlays.log" ;; esac
    done
    sleep 10
    adb shell cmd overlay list 2>/dev/null | tr -d '\r' | grep -i -E "cutout|emulation" | sed 's/^/after: /' >>"$d/overlays.log" || true
  fi
  if [ "$session" = qa ] || [ "$session" = both ]; then
    python3 "$here/mobile_qa.py" android --out "$d" --apk "$apk" --package "$pkg" --activity "${act:-}" \
      --label "$label" --gpu "$mode" ${AZ_QA_POINTS:+--points "$AZ_QA_POINTS"} \
      $( [ "$mode" = "${mode_list[0]}" ] && echo ${AZ_QA_SERIES:+--series "$AZ_QA_SERIES"} ${floor_apk:+--floor-apk "$floor_apk"} ${AZ_QA_STARTUP:+--startup} ) \
      ${AZ_QA_EXPECT_GPU:+--expect-gpu} ${AZ_QA_BACKEND:+--backend "$AZ_QA_BACKEND"} ${AZ_QA_EXPECT_HOOKS:+--expect-hooks} \
      ${AZ_QA_ONLY:+--only "$AZ_QA_ONLY"} 2>&1 | tee "$d/qa.log"
  fi
  # the e2e session runs on the FIRST mode only (it does not depend on how the emulator composes the window)
  if { [ "$session" = e2e ] || [ "$session" = both ]; } && [ "$mode" = "${mode_list[0]}" ]; then
    scen=()
    for f in ${AZ_QA_SCENARIOS:-}; do scen+=(--scenario "$f"); done
    python3 "$here/mobile_e2e.py" android --out "$d/e2e" --apk "$apk" --package "$pkg" --activity "${act:-}" \
      --label "$label e2e" ${scen[@]+"${scen[@]}"} 2>&1 | tee "$d/e2e.log"
  fi
  # AZ_QA_EXTRA_APKS="a.apk b.apk": a FIRST LOOK at other apps on the same boot (first mode only): the steps of AZ_QA_ONLY_EXTRA
  # (default 03 cold start, 08 rotate, 09 home and resume, 12 dark mode, 13 font scale), an out folder per app
  if [ "$mode" = "${mode_list[0]}" ]; then
    for xapk in ${AZ_QA_EXTRA_APKS:-}; do
      xb="$("$aapt2" dump badging "$xapk" 2>/dev/null || true)"
      xpkg="$(echo "$xb" | sed -n "s/^package: name='\([^']*\)'.*/\1/p" | head -n 1)"
      xact="$(echo "$xb" | sed -n "s/^launchable-activity: name='\([^']*\)'.*/\1/p" | head -n 1)"
      xn="$(basename "$xapk" .apk)"
      [ -n "$xpkg" ] || { echo "no package in the badging of $xapk"; continue; }
      echo "---- first look at $xn ($xpkg)"
      mkdir -p "$d/apps"
      # AZ_QA_TAPS: lines `<apk name>=<button label>`: step 03e presses that button of that app
      xtap="$(printf '%s\n' "${AZ_QA_TAPS:-}" | sed -n "s/^$xn=//p" | head -n 1)"
      # AZ_QA_ONLY_FOR: lines `<apk name>=<steps>`: the steps of THAT app instead of AZ_QA_ONLY_EXTRA (AzCompanion's UI is dark by design: no dark-mode step)
      xonly="$(printf '%s\n' "${AZ_QA_ONLY_FOR:-}" | sed -n "s/^$xn=//p" | head -n 1)"
      python3 "$here/mobile_qa.py" android --out "$d/apps/$xn" --apk "$xapk" --package "$xpkg" --activity "${xact:-}" \
        --label "$xn" --gpu "$mode" --only "${xonly:-${AZ_QA_ONLY_EXTRA:-03,08,09,12,13}}" ${xtap:+--tap-label "$xtap"} 2>&1 | tee "$d/apps/$xn.log"
    done
  fi
  if [ "$session" = flicker ] && [ "$mode" = "${mode_list[0]}" ]; then
    # idle redraws, flash frames of a recording, pan jitter (scripts/ci/flicker_audit.py): $apk = the plain AzWidgets APK, AZ_QA_FLICKER_E2E = its e2e
    # APK, AZ_QA_FLICKER_MAPS = the AzMaps APK
    python3 "$here/flicker_audit.py" android --out "$d/flicker" --apk-plain "$apk" ${AZ_QA_FLICKER_E2E:+--apk-e2e "$AZ_QA_FLICKER_E2E"} \
      ${AZ_QA_FLICKER_MAPS:+--maps "$AZ_QA_FLICKER_MAPS"} ${AZ_QA_FLICKER_FLASH_ONLY:+--flash-only} 2>&1 | tee "$d/flicker.log"
  fi
  if [ "$session" = matrix ] && [ "$mode" = "${mode_list[0]}" ]; then
    # every e2e scenario file against its host app (scripts/ci/e2e_matrix.py): the e2e APK of each host (AZ_QA_MATRIX_APKS, space separated)
    mapks=()
    for mapk in "$apk" ${AZ_QA_MATRIX_APKS:-}; do mapks+=(--apk "$mapk"); done
    # AZ_QA_MATRIX_ONLY=<text>: only the files whose path holds it (e.g. `sounds`: one scenario file of a new feature). The engine's frames of every
    # file (its take_screenshot steps) are kept in <matrix>/shots/<file stem>/shot-NNN.png (WP-E2's request: the pictures of a scenario run were lost)
    if [ -n "${AZ_QA_TEXT_SIZES:-}" ]; then
      # AZ_QA_TEXT_SIZES="1.0 1.3 2.0": the same files once per system font scale (settings put system font_scale), each in its own folder; 1.0 is put back at the end
      for ts in $AZ_QA_TEXT_SIZES; do
        adb shell settings put system font_scale "$ts"; sleep 3
        python3 "$here/e2e_matrix.py" android --repo . --out "$d/matrix-text-$ts" --shots "$d/matrix-text-$ts/shots" ${AZ_QA_MATRIX_ONLY:+--only "$AZ_QA_MATRIX_ONLY"} ${mapks[@]+"${mapks[@]}"} 2>&1 | tee -a "$d/e2e-matrix.log"
      done
      adb shell settings put system font_scale 1.0
    else
      python3 "$here/e2e_matrix.py" android --repo . --out "$d/matrix" --shots "$d/matrix/shots" ${AZ_QA_MATRIX_ONLY:+--only "$AZ_QA_MATRIX_ONLY"} ${mapks[@]+"${mapks[@]}"} 2>&1 | tee "$d/e2e-matrix.log"
    fi
  fi
  if [ "$session" = fontwait ] && [ "$mode" = "${mode_list[0]}" ]; then
    python3 "$here/font_wait_series.py" android --out "$d/fontwait" --package "$pkg" --activity "$act" --starts "${AZ_QA_FONTWAIT_STARTS:-10}" \
      --plan "${AZ_QA_FONTWAIT_PLAN:?AZ_QA_FONTWAIT_PLAN}" --label "$label" 2>&1 | tee "$d/fontwait.log"
  fi
  if [ "$session" = tabs ] && [ "$mode" = "${mode_list[0]}" ]; then
    # THE TAB STRIP on the phone: the files of AZ_QA_TABS_FILES (parts of the file names, default tab_touch_phone tab_touch.json tab_touch_pictures tab_looks tab_touch_diag tab_overflow_phone tab_overflow_pictures tab_content_toggle tab_label_frames: tab_touch_diag runs every step whatever the one before said, for the TABS agent's diagnosis of the clicks on a phone)
    # against the e2e APK, per orientation: (1) the device's own pictures of a plain start, light and dark (the system draws the cutout), (2) per file a
    # recording of its run and the engine's frames of it (AZ_E2E_SHOT_DIR, pulled by scripts/ci/e2e_matrix.py --shots), its verdicts in the table
    adb shell dumpsys window displays 2>/dev/null | grep -i -E "cutout|init=|DisplayInfo" | head -n 24 >"$d/display.txt" || true
    # the APK on the phone FIRST: the plain start below needs it, and the matrix runs install it only later
    adb install -r -t "$apk" >"$d/install.log" 2>&1 || echo "the e2e APK did not install: $(tail -n 2 "$d/install.log")"
    adb shell settings put system accelerometer_rotation 0
    # THE LAUNCHER LOCKS PORTRAIT: with it in front the display is portrait whatever user_rotation says, and an app started from it opens in portrait and turns
    # only seconds later (the first run: a 6 s scenario was portrait from its first frame to its last). The second run brought Settings to the front: the
    # display did not turn at all (no landscape picture exists). Now the system itself is told to IGNORE the orientation an app requests (wm
    # set-ignore-orientation-request true, API 31+): the display follows user_rotation whichever activity is in front, the launcher included. What the system says
    # about the rotation goes into <pass>/rotation.log at every step, so that a pass that stays portrait says why.
    wait_orientation() {   # wait_orientation <portrait|landscape> [seconds, default 45]: until the display is in it
      local want="cur=1080x2400"; [ "$1" = landscape ] && want="cur=2400x1080"
      for _ in $(seq 1 "${2:-45}"); do adb shell dumpsys window displays 2>/dev/null | grep -q "$want" && return 0; sleep 1; done
      echo "the display did not turn to $1 within ${2:-45} s"; return 1
    }
    rotation_state() {   # one line of what the system says about the display and its rotation lock
      echo "$(date +%H:%M:%S) $1: $(adb shell dumpsys window displays 2>/dev/null | tr -d '\r' | grep -o -E 'cur=[0-9]+x[0-9]+|mRotation=[0-9]+' | sort -u | tr '\n' ' ') ignore_orientation_request=$(adb shell wm get-ignore-orientation-request 2>&1 | tr -d '\r' | tr '\n' ' ') user_rotation=$(adb shell settings get system user_rotation | tr -d '\r') accelerometer_rotation=$(adb shell settings get system accelerometer_rotation | tr -d '\r') focus=$(adb shell dumpsys window 2>/dev/null | tr -d '\r' | grep -m1 mCurrentFocus | cut -c1-120)" >>"$td/rotation.log"
    }
    # THE KEEPER: the second run showed that no flag turns the display while the LAUNCHER is in front (rotation.log: cur=1080x2400, launcher focused), and a
    # scenario file starts its app from the launcher (the app's window was made portrait, the engine's frames came out 1080 x 2400 in the landscape pass). An
    # app of OURS that has been in front for a few seconds does turn the display (the plain start 30 s later was landscape, every time). So in the landscape pass
    # such an app (AZ_QA_KEEPER_APK, default the AzMail APK) is started and waited for until the display is landscape; the scenario's app then starts OVER it
    # (am start -S of another package: the keeper is never stopped, the launcher never shows) and the display stays turned.
    keeper_pkg=""; keeper_act=""
    keeper_apk="${AZ_QA_KEEPER_APK:-${AZ_QA_AZMAIL_APK:-}}"
    if [ -n "$keeper_apk" ] && [ -e "$keeper_apk" ]; then
      kb="$("$aapt2" dump badging "$keeper_apk" 2>/dev/null || true)"
      keeper_pkg="$(echo "$kb" | sed -n "s/^package: name='\([^']*\)'.*/\1/p" | head -n 1)"
      keeper_act="$(echo "$kb" | sed -n "s/^launchable-activity: name='\([^']*\)'.*/\1/p" | head -n 1)"
      adb install -r -t "$keeper_apk" >"$d/keeper-install.log" 2>&1 || echo "the keeper app did not install: $(tail -n 2 "$d/keeper-install.log")"
    fi
    bring_settings() {   # the display in the orientation of this pass (the name stays: the call sites are many)
      adb shell wm set-ignore-orientation-request true >>"$td/rotation.log" 2>&1 || true
      adb shell settings put system accelerometer_rotation 0
      adb shell settings put system user_rotation "$([ "$orient" = portrait ] && echo 0 || echo 1)"
      if [ "$orient" = landscape ] && [ -n "$keeper_pkg" ]; then
        if [ -n "$keeper_act" ]; then adb shell am start -n "$keeper_pkg/$keeper_act" >/dev/null 2>&1; else adb shell monkey -p "$keeper_pkg" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1; fi
        wait_orientation landscape 90 || true
      else
        wait_orientation "$orient" || true
      fi
      sleep 2
      rotation_state "set $orient"
    }
    for orient in portrait landscape; do
      adb shell settings put system user_rotation "$([ "$orient" = portrait ] && echo 0 || echo 1)"
      td="$d/tabs-$orient"; mkdir -p "$td/shots" "$td/videos"
      bring_settings
      adb shell dumpsys window displays 2>/dev/null | grep -i -E "cutout|init=|mRotation|rotation=" | head -n 12 >"$td/display.txt" || true
      adb shell am force-stop "$pkg" >/dev/null 2>&1
      if [ -n "${act:-}" ]; then adb shell am start -n "$pkg/$act" >/dev/null 2>&1; else adb shell monkey -p "$pkg" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1; fi
      sleep 30
      adb exec-out screencap -p >"$td/device-light-start.png" 2>/dev/null || true
      adb shell cmd uimode night yes >/dev/null 2>&1; sleep 8
      adb exec-out screencap -p >"$td/device-dark-start.png" 2>/dev/null || true
      adb shell cmd uimode night no >/dev/null 2>&1; sleep 4
      # the system text size (does the engine follow it?): fontScale 1.3 and 2.0, a picture each, back to 1.0; the light start above is the baseline
      for fs in 1.3 2.0; do
        adb shell settings put system font_scale "$fs"; sleep 6
        adb exec-out screencap -p >"$td/device-fontscale-$fs.png" 2>/dev/null || true
      done
      adb shell settings put system font_scale 1.0; sleep 3
      adb shell am force-stop "$pkg" >/dev/null 2>&1
      for part in ${AZ_QA_TABS_FILES:-tab_touch_phone tab_touch.json tab_touch_pictures tab_looks tab_touch_diag tab_overflow_phone tab_overflow_pictures tab_content_toggle tab_label_frames}; do
        bring_settings
        vid="/sdcard/tabs-$orient-${part%.json}.mp4"
        ( adb shell screenrecord --time-limit 170 "$vid" >/dev/null 2>&1 & )
        python3 "$here/e2e_matrix.py" android --repo . --out "$td/matrix-${part%.json}" --only "$part" --shots "$td/shots" --apk "$apk" 2>&1 | tee -a "$td/e2e-matrix.log"
        adb shell 'kill -2 $(pidof screenrecord)' >/dev/null 2>&1 || true
        sleep 4
        adb pull "$vid" "$td/videos/" >/dev/null 2>&1 || echo "no recording $vid"
      done
      # AZ_QA_TABS_EXTRA_APKS="a.apk b.apk": a first look at other apps on this phone in this orientation (the AzWriter ribbon): installed, started, 30 s,
      # a picture in light and in dark (the system draws the cutout)
      for xapk in ${AZ_QA_TABS_EXTRA_APKS:-}; do
        xb="$("$aapt2" dump badging "$xapk" 2>/dev/null || true)"
        xpkg="$(echo "$xb" | sed -n "s/^package: name='\([^']*\)'.*/\1/p" | head -n 1)"
        xact="$(echo "$xb" | sed -n "s/^launchable-activity: name='\([^']*\)'.*/\1/p" | head -n 1)"
        xn="$(basename "$xapk" .apk)"
        [ -n "$xpkg" ] || { echo "no package in the badging of $xapk"; continue; }
        adb install -r -t "$xapk" >"$td/$xn-install.log" 2>&1 || echo "$xn did not install"
        bring_settings
        adb shell am force-stop "$xpkg" >/dev/null 2>&1
        if [ -n "$xact" ]; then adb shell am start -n "$xpkg/$xact" >/dev/null 2>&1; else adb shell monkey -p "$xpkg" -c android.intent.category.LAUNCHER 1 >/dev/null 2>&1; fi
        sleep 30
        adb exec-out screencap -p >"$td/$xn-light.png" 2>/dev/null || true
        adb shell cmd uimode night yes >/dev/null 2>&1; sleep 8
        adb exec-out screencap -p >"$td/$xn-dark.png" 2>/dev/null || true
        adb shell cmd uimode night no >/dev/null 2>&1; sleep 4
        for fs in 1.3 2.0; do
          adb shell settings put system font_scale "$fs"; sleep 6
          adb exec-out screencap -p >"$td/$xn-fontscale-$fs.png" 2>/dev/null || true
        done
        adb shell settings put system font_scale 1.0; sleep 3
        adb shell am force-stop "$xpkg" >/dev/null 2>&1
      done
      # AZ_QA_AZMAIL_APK: AzMail on this phone in this orientation (scripts/ci/android_azmail.py: its first start, the sample mailbox when
      # AZ_QA_AZMAIL_DATA names the folder scripts/ci/azmail_sample_data.py wrote, a message, the compose command; the pictures are the answer)
      if [ -n "${AZ_QA_AZMAIL_APK:-}" ]; then
        bring_settings
        python3 "$here/android_azmail.py" --out "$td/azmail" --apk "$AZ_QA_AZMAIL_APK" --tag "$orient" \
          ${AZ_QA_AZMAIL_DATA:+--data "$AZ_QA_AZMAIL_DATA"} 2>&1 | tee "$td/azmail.log"
      fi
    done
    adb shell wm set-ignore-orientation-request false >/dev/null 2>&1 || true
    adb shell settings put system accelerometer_rotation 1
  fi
  if [ "$session" = audit ] && [ "$mode" = "${mode_list[0]}" ]; then
    aport=8795
    for aapk in "$apk" ${AZ_QA_AUDIT_APKS:-}; do
      ab="$("$aapt2" dump badging "$aapk" 2>/dev/null || true)"
      apkg="$(echo "$ab" | sed -n "s/^package: name='\([^']*\)'.*/\1/p" | head -n 1)"
      aact="$(echo "$ab" | sed -n "s/^launchable-activity: name='\([^']*\)'.*/\1/p" | head -n 1)"
      an="$(basename "$aapk" .apk)"
      [ -n "$apkg" ] || { echo "no package in the badging of $aapk"; continue; }
      ascript=""
      case "$an" in AzPaint-*) ascript=paint ;; AzWriter-*) ascript=writer ;; AzMaps-*) ascript=maps ;; AzMeet-*) ascript=meet ;; esac
      echo "---- phone-fit audit of $an ($apkg)${ascript:+, script $ascript}"
      mkdir -p "$d/audit"
      # one port per app, and the app of the session before is stopped: a debug server of the app before that still held the port
      # would answer for the app in front of it (the audit of the wrong DOM)
      python3 "$here/mobile_e2e.py" android --out "$d/audit/$an" --apk "$aapk" --package "$apkg" --activity "${aact:-}" --port "$aport" \
        --label "$an" --audit-only ${ascript:+--script "$ascript"} 2>&1 | tee "$d/audit/$an.log"
      adb shell am force-stop "$apkg" >/dev/null 2>&1 || true
      aport=$((aport + 1))
    done
  fi
  adb emu kill >/dev/null 2>&1 || true
  sleep 3
  kill "$emu" 2>/dev/null || true
  wait "$emu" 2>/dev/null || true
done

python3 - "$out" <<'PY'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
rows = []
for q in sorted(list(out.glob("*/qa.json")) + list(out.glob("*/e2e/qa.json")) + list(out.glob("*/apps/*/qa.json")) + list(out.glob("*/audit/*/qa.json"))):
    d = json.loads(q.read_text())
    failed = [s["name"] for s in d["steps"] if not s["ok"]]
    rows.append({"mode": str(q.parent.relative_to(out)), "ok": d["ok"], "steps": len(d["steps"]), "failed": failed})
(out / "summary.json").write_text(json.dumps(rows, indent=2))
for r in rows:
    print(f"[{'PASS' if r['ok'] else 'FAIL'}] gpu mode {r['mode']}: {r['steps'] - len(r['failed'])}/{r['steps']} steps" + (f"; failed: {', '.join(r['failed'])}" if r['failed'] else ""))
PY
exit 0
