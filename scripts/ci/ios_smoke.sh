#!/usr/bin/env bash
# ios_smoke.sh: does an iOS Simulator app BOOT? Creates a simulator, installs the .app, starts it,
# waits, takes a screenshot and reads the log and the crash reports.
#
#   scripts/ci/ios_smoke.sh <app.zip | App.app> <bundle-id> <out-dir> [seconds-to-wait]
#
# For a macOS runner with Xcode (xcrun simctl). Needs no signing (a Simulator app runs ad-hoc signed).
# Writes into <out-dir>:
#   screenshot.png   the simulator's screen after the wait
#   log.txt          the unified log of the app's process
#   crash/           the crash reports (.ips) the app left, if any
#   result.json      {bundle_id, booted, launched, alive, fatal, rust_log_lines, screen_nonblank}
# Exit 0 when the app is alive after the wait and left no crash report; 1 otherwise; 2 when the
# simulator itself could not be started (nothing is said about the app then).
set -uo pipefail

app="${1:?app.zip or App.app}"; bid="${2:?bundle id}"; out="${3:?out-dir}"; wait_s="${4:-12}"
mkdir -p "$out/crash"
work="$(mktemp -d)"
if [ -d "$app" ]; then appdir="$app"; else unzip -q "$app" -d "$work" && appdir="$(ls -d "$work"/*.app | head -n 1)"; fi
exe="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$appdir/Info.plist")"

write_result() {  # booted launched alive fatal rust_lines nonblank
  python3 - "$out/result.json" "$bid" "$1" "$2" "$3" "$4" "$5" "$6" <<'PY'
import json, sys
path, bid, booted, launched, alive, fatal, rust, nonblank = sys.argv[1:]
json.dump({"bundle_id": bid, "booted": booted == "1", "launched": launched == "1", "alive": alive == "1",
           "fatal": fatal or None, "rust_log_lines": int(rust), "screen_nonblank": nonblank == "1"},
          open(path, "w"), indent=2)
PY
}

# The newest installed iOS runtime and a modern iPhone that runtime supports.
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
)" || { echo "$pick"; write_result 0 0 0 "" 0 0; exit 2; }
read -r runtime devtype <<<"$pick"
echo "simulator: $runtime / $devtype"

udid="$(xcrun simctl create azul-smoke "$devtype" "$runtime")" || { write_result 0 0 0 "" 0 0; exit 2; }
trap 'xcrun simctl shutdown "$udid" >/dev/null 2>&1; xcrun simctl delete "$udid" >/dev/null 2>&1' EXIT
xcrun simctl boot "$udid" || { write_result 0 0 0 "" 0 0; exit 2; }
xcrun simctl bootstatus "$udid" -b >"$out/boot.log" 2>&1 || { tail -n 20 "$out/boot.log"; write_result 0 0 0 "" 0 0; exit 2; }

if ! xcrun simctl install "$udid" "$appdir" >"$out/install.log" 2>&1; then
  cat "$out/install.log"
  write_result 1 0 0 "simctl install failed: $(tail -n 1 "$out/install.log")" 0 0
  exit 1
fi
started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
launch="$(xcrun simctl launch "$udid" "$bid" 2>&1)"; echo "$launch"
launched=0; case "$launch" in *"$bid: "*) launched=1;; esac
sleep "$wait_s"

xcrun simctl io "$udid" screenshot "$out/screenshot.png" >/dev/null 2>&1 || true
# alive: the app's job in the simulator's launchd lists a pid (a dead one shows "-")
alive=0
state="$(xcrun simctl spawn "$udid" launchctl list 2>/dev/null | grep "UIKitApplication:$bid" || true)"
echo "launchctl: ${state:-<no job>}"
pid="$(echo "$state" | awk '{print $1}')"
case "$pid" in ''|-) ;; *) alive=1;; esac

xcrun simctl spawn "$udid" log show --start "$(date -u -v-5M +%Y-%m-%d\ %H:%M:%S 2>/dev/null || echo "$started")" --style compact \
  --predicate "process == \"$exe\"" >"$out/log.txt" 2>&1 || true
# crash reports of this app on the host (the simulator writes them there)
find "$HOME/Library/Logs/DiagnosticReports" -name "$exe*.ips" -newermt "$started" -exec cp {} "$out/crash/" \; 2>/dev/null || true

fatal=""
crash="$(ls "$out/crash" 2>/dev/null | head -n 1)"
[ -n "$crash" ] && fatal="crash report: $crash"
[ -z "$fatal" ] && fatal="$(grep -m1 -E 'Terminating app|SIGABRT|SIGSEGV|Fatal error|panicked at' "$out/log.txt" || true)"
rust_lines="$(grep -c -v '^$' "$out/log.txt" 2>/dev/null || true)"
nonblank=0
python3 - "$out/screenshot.png" <<'PY' && nonblank=1
import sys
data = open(sys.argv[1], "rb").read()
sys.exit(0 if data[:8] == b"\x89PNG\r\n\x1a\n" and len(data) > 8000 else 1)
PY
write_result 1 "$launched" "$alive" "$fatal" "${rust_lines:-0}" "$nonblank"
cat "$out/result.json"
[ "$alive" = "1" ] && [ -z "$fatal" ]
