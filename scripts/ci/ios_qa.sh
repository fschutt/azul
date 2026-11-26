#!/usr/bin/env bash
# ios_qa.sh: the lifecycle QA of ONE Simulator app on the newest iOS runtime and a modern iPhone.
#
#   scripts/ci/ios_qa.sh <out-dir> <app.zip | App.app> [<label>] [<session>]
#
#   session  qa (default): scripts/ci/mobile_qa.py, the life of the app with simctl;
#            e2e: scripts/ci/mobile_e2e.py, the debug server of a `mobile-e2e` app driven from the runner (scenario files
#                 named in AZ_QA_SCENARIOS, space separated; the Simulator shares the host's loopback: no port forwarding);
#            both: qa, then e2e on the same simulator.
#
# For a macOS runner with Xcode (xcrun simctl). Needs no signing (a Simulator app runs ad-hoc signed). Writes what
# scripts/ci/mobile_qa.py writes (qa.json, qa.md, shots/, logs/) into <out-dir>. Exit 0 when the simulator ran (the
# verdicts are in qa.json: a failing app is a RESULT of this job, not a failure of it); 2 when the simulator itself
# could not be started.
#
# Environment: AZ_QA_EXPECT_GPU=1 (a gpu request that ran the CPU renderer FAILS), AZ_QA_ONLY=03,05 (only those steps),
# AZ_QA_BACKEND=cpu|gpu|auto (every step of the lifecycle starts the app with that renderer; gpu also requires that no GL command
# goes out in the background), AZ_QA_DEBUG_PORT (the debug server of the e2e app, default 8795), AZ_QA_SCENARIOS (the e2e files),
# AZ_QA_E2E_ARGS (more options for mobile_e2e.py). SIMCTL_CHILD_AZ_BACKEND=gpu in the environment runs the e2e session on the GPU.
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
print(rt["identifier"], types[-1]["identifier"])
PY
)" || { echo "$pick"; exit 2; }
read -r runtime devtype <<<"$pick"
echo "simulator: $runtime / $devtype"

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
  # scripted swipe of step 04 reads the frame times through it); the plain `qa` is the app that ships and has neither.
  extra=()
  if [ "$session" = both ]; then extra+=(--expect-hooks --debug-port "${AZ_QA_DEBUG_PORT:-8795}"); fi
  # AZ_QA_BACKEND=gpu|cpu|auto: every step of the lifecycle starts the app with that renderer (gpu: the checks that no GL command
  # goes out in the background are required)
  python3 "$here/mobile_qa.py" ios --out "$out" --udid "$udid" --bundle "$bid" --exe "$exe" --app "$appdir" --label "$label" \
    ${AZ_QA_EXPECT_GPU:+--expect-gpu} ${AZ_QA_ONLY:+--only "$AZ_QA_ONLY"} ${AZ_QA_BACKEND:+--backend "$AZ_QA_BACKEND"} \
    ${extra[@]+"${extra[@]}"} 2>&1 | tee "$out/qa.log"
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
