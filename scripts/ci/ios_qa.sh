#!/usr/bin/env bash
# ios_qa.sh: the lifecycle QA of ONE Simulator app on the newest iOS runtime and a modern iPhone.
#
#   scripts/ci/ios_qa.sh <out-dir> <app.zip | App.app> [<label>] [<session>]
#
#   session  qa (default): scripts/ci/mobile_qa.py, the life of the app with simctl;
#            e2e: scripts/ci/mobile_e2e.py, the debug server of a `mobile-e2e` app driven from the runner (scenario files
#                 named in AZ_QA_SCENARIOS, space separated; the Simulator shares the host's loopback: no port forwarding);
#            both: qa, then e2e on the same simulator;
#            matrix: every e2e scenario file against its host app, scripts/ci/e2e_matrix.py (AZ_QA_MATRIX_APPS: the app zips).
#            tabs: the tab strip (examples/azul-widgets/e2e/tab_touch*.json) on the Simulator's phone, portrait then landscape (Device > Rotate Left
#                 through osascript): the engine's frames of every file (AZ_E2E_SHOT_DIR), the device's pictures of a plain start (light, dark)
#                 and a recording of each run, in <out>/tabs/<orientation>/; AZ_QA_IOS_DEVICE="iPhone 16 Pro" names the device type.
#            startup: cold starts of the app with the engine's startup timeline on (AZ_STARTUP=1; scripts/ci/ios_startup.py, AZ_QA_STARTS=2): the
#                 first_present row, the layout passes, the font cache rows (startup.md, the console of each start);
#            flicker: scripts/ci/flicker_audit.py ios: the idle pictures and the flash frames (a recording of the cold start, the dark
#                 appearance, rotation when the Simulator's menu can be pressed, leaving the app and back; AZ_QA_BACKEND=gpu for the GPU path).
#
# For a macOS runner with Xcode (xcrun simctl). Needs no signing (a Simulator app runs ad-hoc signed). Writes what
# scripts/ci/mobile_qa.py writes (qa.json, qa.md, shots/, logs/) into <out-dir>. Exit 0 when the simulator ran (the
# verdicts are in qa.json: a failing app is a RESULT of this job, not a failure of it); 2 when the simulator itself
# could not be started.
#
# Environment: AZ_QA_EXPECT_GPU=1 (a gpu request that ran the CPU renderer FAILS), AZ_QA_ONLY=03,05 (only those steps),
# AZ_QA_BACKEND=cpu|gpu|auto (every step of the lifecycle starts the app with that renderer; gpu also requires that no GL command
# goes out in the background), AZ_QA_DEBUG_PORT (the debug server of the e2e app, default 8795), AZ_QA_SCENARIOS (the e2e files),
# AZ_QA_E2E_ARGS (more options for mobile_e2e.py), AZ_QA_HOOKS=1 (a `qa` session of the mobile-e2e app: the failure hooks and the debug
# server are expected, as in the `both` session). SIMCTL_CHILD_AZ_BACKEND=gpu in the environment runs the e2e session on the GPU.
#
# What it can and cannot press on a runner: simctl launches, terminates, installs, switches the appearance and the
# text size and takes pictures; it has no touch. Rotation and the memory warning go through the Simulator app's
# menu with osascript (the runner may refuse the accessibility permission: the step says so). Touches need the
# app's own debug server (scripts/ci/mobile_e2e.py).
set -uo pipefail

out="${1:?out-dir}"; app="${2:?app}"; label="${3:-AzWidgets}"; session="${4:-qa}"
here="$(cd "$(dirname "$0")" && pwd)"
mkdir -p "$out"
work="$(mktemp -d)"

pick="$(python3 - <<'PY'
import json, re, subprocess
def sh(*a): return json.loads(subprocess.check_output(a))
runtimes = [r for r in sh("xcrun", "simctl", "list", "runtimes", "-j")["runtimes"]
            if r.get("isAvailable", True) and r["identifier"].startswith("com.apple.CoreSimulator.SimRuntime.iOS")]
if not runtimes:
    raise SystemExit("no iOS simulator runtime is installed")
runtimes.sort(key=lambda r: [int(x) for x in r["version"].split(".")])
rt = runtimes[-1]
supported = {d["identifier"] for d in rt.get("supportedDeviceTypes", [])}
types = [d for d in sh("xcrun", "simctl", "list", "devicetypes", "-j")["devicetypes"]
         if d["name"].startswith("iPhone") and (not supported or d["identifier"] in supported)]
def rank(d):
    m = re.search(r"iPhone (\d+)", d["name"])
    plain = not re.search(r"(Pro|Plus|Max|mini|SE|e\b)", d["name"])
    return (int(m.group(1)) if m else 0, plain)
types.sort(key=rank)
import os
want = os.environ.get("AZ_QA_IOS_DEVICE", "")
if want:   # a device type by its name (exact, else the first that holds the text): iPhone 16 Pro, iPhone SE (3rd generation)
    named = [d for d in types if d["name"] == want] or [d for d in types if want in d["name"]]
    if not named:
        raise SystemExit("no device type named " + want + " on " + rt["identifier"] + ": " + ", ".join(d["name"] for d in types))
    types = named
print(rt["identifier"], types[-1]["identifier"])
PY
)" || { echo "$pick"; exit 2; }
read -r runtime devtype <<<"$pick"
echo "simulator: $runtime / $devtype"

# a soft keyboard on the screen: the Simulator connects the Mac's hardware keyboard by default, which keeps it away (a runner only: this is the user's preference on a Mac)
[ "${GITHUB_ACTIONS:-}" = "true" ] && defaults write com.apple.iphonesimulator ConnectHardwareKeyboard -bool false >/dev/null 2>&1
udid="$(xcrun simctl create azul-qa "$devtype" "$runtime")" || exit 2
trap 'xcrun simctl shutdown "$udid" >/dev/null 2>&1; xcrun simctl delete "$udid" >/dev/null 2>&1' EXIT
boot_start=$(date +%s)
xcrun simctl boot "$udid" || exit 2
xcrun simctl bootstatus "$udid" -b >"$out/boot.log" 2>&1 || { tail -n 20 "$out/boot.log"; exit 2; }
echo "simulator booted in $(( $(date +%s) - boot_start )) s: $udid"
sleep 5

if [ -d "$app" ]; then appdir="$app"; else
  d="$work/app"; mkdir -p "$d"; unzip -q "$app" -d "$d" && appdir="$(ls -d "$d"/*.app | head -n 1)"
fi
bid="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$appdir/Info.plist" 2>/dev/null)"
exe="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$appdir/Info.plist" 2>/dev/null)"
echo "app: $appdir bundle=$bid executable=$exe"

if [ "$session" = qa ] || [ "$session" = both ]; then
  # The session names the app: `both` is the mobile-e2e app, which has the engine's failure hooks (step 04b) and its debug server (the
  # scripted swipe of step 04 reads the frame times through it); the plain `qa` is the app that ships and has neither, unless
  # AZ_QA_HOOKS=1 says that this `qa` session is of the mobile-e2e app too.
  extra=()
  if [ "$session" = both ] || [ -n "${AZ_QA_HOOKS:-}" ]; then extra+=(--expect-hooks --debug-port "${AZ_QA_DEBUG_PORT:-8795}"); fi
  # AZ_QA_BACKEND=gpu|cpu|auto: every step of the lifecycle starts the app with that renderer (gpu: the checks that no GL command
  # goes out in the background are required)
  python3 "$here/mobile_qa.py" ios --out "$out" --udid "$udid" --bundle "$bid" --exe "$exe" --app "$appdir" --label "$label" \
    ${AZ_QA_EXPECT_GPU:+--expect-gpu} ${AZ_QA_ONLY:+--only "$AZ_QA_ONLY"} ${AZ_QA_BACKEND:+--backend "$AZ_QA_BACKEND"} \
    ${extra[@]+"${extra[@]}"} 2>&1 | tee "$out/qa.log"
fi
if [ "$session" = matrix ]; then
  # every e2e scenario file against its host app (scripts/ci/e2e_matrix.py): AZ_QA_MATRIX_APPS = the zips of the mobile-e2e Simulator apps
  # (space separated; the app of this session when unset)
  apps=()
  for z in ${AZ_QA_MATRIX_APPS:-$app}; do apps+=(--app "$z"); done
  # AZ_QA_MATRIX_RECORD=1: the Simulator's screen recording of the whole matrix run ($out/matrix.mp4): what the engine's frames do not show (the soft keyboard, a system sheet)
  rec=""
  if [ "${AZ_QA_MATRIX_RECORD:-}" = 1 ]; then
    xcrun simctl io "$udid" recordVideo --codec=h264 --force "$out/matrix.mp4" >"$out/matrix-record.log" 2>&1 &
    rec=$!
  fi
  if [ -n "${AZ_QA_TEXT_SIZES:-}" ]; then
    # AZ_QA_TEXT_SIZES="large accessibility-extra-extra-extra-large": the same files once per system text size (simctl ui content_size), each in its own folder
    # (the matrix of the first size is the plain matrix of a normal run; the default size `large` is put back at the end). AzMail's text_size.json asks for it.
    for ts in $AZ_QA_TEXT_SIZES; do
      xcrun simctl ui "$udid" content_size "$ts" >/dev/null 2>&1; sleep 3
      python3 "$here/e2e_matrix.py" ios --repo . --out "$out/text-$ts" --udid "$udid" --shots "$out/text-$ts/shots" ${AZ_QA_MATRIX_ONLY:+--only "$AZ_QA_MATRIX_ONLY"} ${apps[@]+"${apps[@]}"} 2>&1 | tee -a "$out/e2e-matrix.log"
    done
    xcrun simctl ui "$udid" content_size large >/dev/null 2>&1
  else
    python3 "$here/e2e_matrix.py" ios --repo . --out "$out" --udid "$udid" --shots "$out/shots" ${AZ_QA_MATRIX_ONLY:+--only "$AZ_QA_MATRIX_ONLY"} ${apps[@]+"${apps[@]}"} 2>&1 | tee "$out/e2e-matrix.log"
  fi
  if [ -n "$rec" ]; then kill -INT "$rec" 2>/dev/null; wait "$rec" 2>/dev/null; fi
fi
if [ "$session" = tabs ]; then
  # THE TAB STRIP on the phone: per orientation (1) the device's own pictures of a plain start, light and dark, (2) per file of AZ_QA_TABS_FILES a
  # recording of its run and the engine's frames of it (AZ_E2E_SHOT_DIR, scripts/ci/e2e_matrix.py --shots), its verdicts in the table
  xcrun simctl list devices | grep "$udid" | head -n 1 >"$out/device.txt" || true
  # the app on the Simulator FIRST: the plain start below needs it, and the matrix runs install it only later (simctl launch of an app that is not
  # there starts nothing: the iOS flicker audit recorded the home screen for 80 s that way)
  xcrun simctl install "$udid" "$appdir" >"$out/install.log" 2>&1 || echo "the app did not install: $(tail -n 2 "$out/install.log")"
  for orient in portrait landscape; do
    if [ "$orient" = landscape ]; then
      open -a Simulator --args -CurrentDeviceUDID "$udid"; sleep 10
      osascript -e 'tell application "Simulator" to activate' -e 'delay 1' \
        -e 'tell application "System Events" to keystroke (ASCII character 28) using {command down}' >"$out/rotate.log" 2>&1 \
        || echo "the runner refused the rotation (the pictures stay portrait): $(cat "$out/rotate.log")"
      sleep 5
    fi
    td="$out/tabs/$orient"; mkdir -p "$td/shots" "$td/videos"
    # the engine's debug server (AZ_DEBUG, a mobile-e2e app) answers what the window knows of this phone's display: the cutouts the tab strip builds its
    # lanes from, the status bar, the corners (get_display_features), kept beside the pictures: the Simulator has the island, does the app hear of it?
    env SIMCTL_CHILD_AZ_DEBUG=8797 xcrun simctl launch "$udid" "$bid" >"$td/launch.log" 2>&1; sleep 30
    curl -s -m 10 -X POST http://127.0.0.1:8797/ -d '{"op":"get_display_features"}' >"$td/display-features-start.json" 2>&1 || echo "no debug server answered on 8797"
    xcrun simctl io "$udid" screenshot "$td/device-light-start.png" >/dev/null 2>&1 || true
    xcrun simctl ui "$udid" appearance dark >/dev/null 2>&1; sleep 8
    xcrun simctl io "$udid" screenshot "$td/device-dark-start.png" >/dev/null 2>&1 || true
    xcrun simctl ui "$udid" appearance light >/dev/null 2>&1; sleep 4
    # the system text size (does the engine follow it?): the largest accessibility size, a picture, back to the default; the light start above is the baseline
    xcrun simctl ui "$udid" content_size accessibility-extra-extra-extra-large >/dev/null 2>&1; sleep 8
    xcrun simctl io "$udid" screenshot "$td/device-text-xxxl.png" >/dev/null 2>&1 || true
    xcrun simctl ui "$udid" content_size large >/dev/null 2>&1; sleep 3
    xcrun simctl terminate "$udid" "$bid" >/dev/null 2>&1 || true
    for part in ${AZ_QA_TABS_FILES:-tab_touch_phone tab_touch.json tab_touch_pictures tab_looks tab_touch_diag tab_overflow_phone tab_overflow_pictures tab_content_toggle}; do
      xcrun simctl io "$udid" recordVideo --codec=h264 --force "$td/videos/${part%.json}.mp4" >"$td/videos/${part%.json}.log" 2>&1 &
      rec=$!
      python3 "$here/e2e_matrix.py" ios --repo . --out "$td/matrix-${part%.json}" --udid "$udid" --only "$part" --shots "$td/shots" --app "$app" 2>&1 | tee -a "$td/e2e-matrix.log"
      kill -INT "$rec" 2>/dev/null; wait "$rec" 2>/dev/null
    done
    # AZ_QA_TABS_EXTRA_APPS="a.zip b.zip": a first look at other apps on this phone in this orientation (the AzWriter ribbon): installed, started, 30 s,
    # a picture in light and in dark
    for xzip in ${AZ_QA_TABS_EXTRA_APPS:-}; do
      xn="$(basename "$xzip" .zip)"; xd="$work/extra-$xn"; mkdir -p "$xd"
      unzip -q -o "$xzip" -d "$xd" 2>/dev/null
      xapp="$(ls -d "$xd"/*.app 2>/dev/null | head -n 1)"
      xbid="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$xapp/Info.plist" 2>/dev/null)"
      [ -n "$xbid" ] || { echo "no app in $xzip"; continue; }
      xcrun simctl install "$udid" "$xapp" >"$td/$xn-install.log" 2>&1 || echo "$xn did not install"
      xcrun simctl terminate "$udid" "$xbid" >/dev/null 2>&1 || true
      env SIMCTL_CHILD_AZ_DEBUG=8797 xcrun simctl launch "$udid" "$xbid" >"$td/$xn-launch.log" 2>&1; sleep 30
      curl -s -m 10 -X POST http://127.0.0.1:8797/ -d '{"op":"get_display_features"}' >"$td/$xn-display-features.json" 2>&1 || echo "no debug server answered on 8797 ($xn)"
      xcrun simctl io "$udid" screenshot "$td/$xn-light.png" >/dev/null 2>&1 || true
      xcrun simctl ui "$udid" appearance dark >/dev/null 2>&1; sleep 8
      xcrun simctl io "$udid" screenshot "$td/$xn-dark.png" >/dev/null 2>&1 || true
      xcrun simctl ui "$udid" appearance light >/dev/null 2>&1; sleep 4
      xcrun simctl ui "$udid" content_size accessibility-extra-extra-extra-large >/dev/null 2>&1; sleep 8
      xcrun simctl io "$udid" screenshot "$td/$xn-text-xxxl.png" >/dev/null 2>&1 || true
      xcrun simctl ui "$udid" content_size large >/dev/null 2>&1; sleep 3
      xcrun simctl terminate "$udid" "$xbid" >/dev/null 2>&1 || true
    done
  done
fi
if [ "$session" = fontwait ]; then
  # the font-wait A/B (scripts/ci/font_wait_series.py): AZ_QA_FONTWAIT_PLAN="control:A=a.app;control:B=b.app;..." (.app folders, one bundle id), AZ_QA_FONTWAIT_STARTS per block
  python3 "$here/font_wait_series.py" ios --out "$out/fontwait" --udid "$udid" --bundle "$bid" --starts "${AZ_QA_FONTWAIT_STARTS:-10}" \
    --plan "${AZ_QA_FONTWAIT_PLAN:?AZ_QA_FONTWAIT_PLAN}" --label "$label" 2>&1 | tee "$out/fontwait.log"
fi
if [ "$session" = startup ]; then
  python3 "$here/ios_startup.py" --out "$out" --udid "$udid" --bundle "$bid" --exe "$exe" --app "$appdir" --starts "${AZ_QA_STARTS:-2}" 2>&1 | tee "$out/startup.log"
fi
if [ "$session" = flicker ]; then
  # the recording is cut into frames with ffmpeg, which a macOS runner may not have
  command -v ffmpeg >/dev/null 2>&1 || brew install ffmpeg >/dev/null 2>&1 || echo "ffmpeg could not be installed: the flash-frame audit cannot cut its recording"
  python3 "$here/flicker_audit.py" ios --out "$out" --udid "$udid" --bundle "$bid" --exe "$exe" --app "$appdir" \
    ${AZ_QA_BACKEND:+--backend "$AZ_QA_BACKEND"} 2>&1 | tee "$out/flicker.log"
fi
if [ "$session" = e2e ] || [ "$session" = both ]; then
  scen=()
  for f in ${AZ_QA_SCENARIOS:-}; do scen+=(--scenario "$f"); done
  # AZ_QA_E2E_ARGS: more options for mobile_e2e.py (a GPU run on the Simulator needs `--first-frame-timeout 240 --server-timeout 300`:
  # the first frame compiles every shader on Apple's software GL)
  # shellcheck disable=SC2086
  python3 "$here/mobile_e2e.py" ios --out "$out/e2e" --udid "$udid" --bundle "$bid" --exe "$exe" --apk "$appdir" \
    --label "$label e2e" ${scen[@]+"${scen[@]}"} ${AZ_QA_E2E_ARGS:-} 2>&1 | tee "$out/e2e.log"
fi
exit 0
