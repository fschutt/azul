#!/usr/bin/env bash
# android_qa.sh: the lifecycle QA of ONE APK on an x86_64 emulator, once per GPU mode of the emulator.
#
#   scripts/ci/android_qa.sh <out-dir> <apk> [<gpu-modes: comma list, default swiftshader_indirect,guest>] [<label>] [<session>]
#
#   session  qa (default): scripts/ci/mobile_qa.py, the lifecycle of the plain APK;
#            e2e: scripts/ci/mobile_e2e.py, the debug server of a `mobile-e2e` APK driven from the runner (the scenario
#                 files named in AZ_QA_SCENARIOS, space separated);
#            both: qa, then e2e on the same boot;
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
  if JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}" bash "$here/../floor-apk/build.sh" "$out/AzFloor-android.apk" >"$out/floor-build.log" 2>&1; then
    floor_apk="$out/AzFloor-android.apk"
  else
    echo "the floor APK did not build:"; tail -n 8 "$out/floor-build.log"
  fi
fi

IFS=',' read -r -a mode_list <<<"$modes"
for mode in "${mode_list[@]}"; do
  d="$out/$mode"; mkdir -p "$d"
  echo "================ emulator GPU mode: $mode ================"
  adb kill-server >/dev/null 2>&1 || true
  echo no | avdmanager create avd --force -n "qa_$mode" -k "$image" -d pixel_6 >"$d/avd.log" 2>&1 \
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
  if [ "$session" = qa ] || [ "$session" = both ]; then
    python3 "$here/mobile_qa.py" android --out "$d" --apk "$apk" --package "$pkg" --activity "${act:-}" \
      --label "$label" --gpu "$mode" ${AZ_QA_POINTS:+--points "$AZ_QA_POINTS"} \
      $( [ "$mode" = "${mode_list[0]}" ] && echo ${AZ_QA_SERIES:+--series "$AZ_QA_SERIES"} ${floor_apk:+--floor-apk "$floor_apk"} ${AZ_QA_STARTUP:+--startup} ) \
      ${AZ_QA_EXPECT_GPU:+--expect-gpu} ${AZ_QA_ONLY:+--only "$AZ_QA_ONLY"} 2>&1 | tee "$d/qa.log"
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
      python3 "$here/mobile_qa.py" android --out "$d/apps/$xn" --apk "$xapk" --package "$xpkg" --activity "${xact:-}" \
        --label "$xn" --gpu "$mode" --only "${AZ_QA_ONLY_EXTRA:-03,08,09,12,13}" ${xtap:+--tap-label "$xtap"} 2>&1 | tee "$d/apps/$xn.log"
    done
  fi
  if [ "$session" = audit ] && [ "$mode" = "${mode_list[0]}" ]; then
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
      python3 "$here/mobile_e2e.py" android --out "$d/audit/$an" --apk "$aapk" --package "$apkg" --activity "${aact:-}" \
        --label "$an" --audit-only ${ascript:+--script "$ascript"} 2>&1 | tee "$d/audit/$an.log"
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
