#!/usr/bin/env bash
# ios_smoke.sh: do iOS Simulator apps BOOT? Starts ONE simulator, then for every app: installs it, starts it,
# waits, takes a screenshot, reads the log and looks for a crash report.
#
#   scripts/ci/ios_smoke.sh <out-dir> <seconds-to-wait> <app.zip | App.app> [...]
#
# For a macOS runner with Xcode (xcrun simctl). Needs no signing (a Simulator app runs ad-hoc signed).
# Per app <out-dir>/<app name>/ holds
#   screenshot.png   the simulator's screen after the wait
#   log.txt          the unified log of the app's process
#   crash/           the crash reports (.ips) the app left, if any
#   result.json      {app, bundle_id, booted, launched, alive, fatal, rust_log_lines, screen_nonblank}
# and <out-dir>/summary.json lists them. Exit 0 when every app is alive after the wait and left no crash
# report; 1 when one is not; 2 when the simulator itself could not be started.
set -uo pipefail

out="${1:?out-dir}"; wait_s="${2:?seconds}"; shift 2
[ $# -gt 0 ] || { echo "no app given" >&2; exit 2; }
mkdir -p "$out"
work="$(mktemp -d)"

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
)" || { echo "$pick"; exit 2; }
read -r runtime devtype <<<"$pick"
echo "simulator: $runtime / $devtype"

udid="$(xcrun simctl create azul-smoke "$devtype" "$runtime")" || exit 2
trap 'xcrun simctl shutdown "$udid" >/dev/null 2>&1; xcrun simctl delete "$udid" >/dev/null 2>&1' EXIT
xcrun simctl boot "$udid" || exit 2
xcrun simctl bootstatus "$udid" -b >"$out/boot.log" 2>&1 || { tail -n 20 "$out/boot.log"; exit 2; }
echo "simulator booted: $udid"

all_ok=0
for app in "$@"; do
  if [ -d "$app" ]; then appdir="$app"; else
    d="$work/$(basename "$app" .zip)"; mkdir -p "$d"; unzip -q "$app" -d "$d" && appdir="$(ls -d "$d"/*.app | head -n 1)"
  fi
  name="$(basename "$app" | sed 's/\.app\.zip$//; s/\.zip$//; s/\.app$//')"
  dir="$out/$name"; mkdir -p "$dir/crash"
  bid="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$appdir/Info.plist" 2>/dev/null)"
  exe="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$appdir/Info.plist" 2>/dev/null)"
  launched=0; alive=0; fatal=""; rust_lines=0; nonblank=0
  if ! xcrun simctl install "$udid" "$appdir" >"$dir/install.log" 2>&1; then
    fatal="simctl install failed: $(tail -n 1 "$dir/install.log")"
  else
    started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    launch="$(xcrun simctl launch "$udid" "$bid" 2>&1)"; echo "$launch"
    case "$launch" in *"$bid: "*) launched=1;; esac
    sleep "$wait_s"
    xcrun simctl io "$udid" screenshot "$dir/screenshot.png" >/dev/null 2>&1 || true
    # alive: the app's job in the simulator's launchd lists a pid (a dead one shows "-")
    state="$(xcrun simctl spawn "$udid" launchctl list 2>/dev/null | grep "UIKitApplication:$bid" || true)"
    echo "launchctl: ${state:-<no job>}"
    pid="$(echo "$state" | awk '{print $1}')"
    case "$pid" in ''|-) ;; *) alive=1;; esac
    xcrun simctl spawn "$udid" log show --last 3m --style compact --predicate "process == \"$exe\"" >"$dir/log.txt" 2>&1 || true
    find "$HOME/Library/Logs/DiagnosticReports" -name "$exe*.ips" -newermt "$started" -exec cp {} "$dir/crash/" \; 2>/dev/null || true
    crash="$(ls "$dir/crash" 2>/dev/null | head -n 1)"
    [ -n "$crash" ] && fatal="crash report: $crash"
    [ -z "$fatal" ] && fatal="$(grep -m1 -E 'Terminating app|SIGABRT|SIGSEGV|Fatal error|panicked at' "$dir/log.txt" || true)"
    rust_lines="$(grep -c -v '^$' "$dir/log.txt" 2>/dev/null || true)"
    python3 - "$dir/screenshot.png" <<'PY' && nonblank=1
import sys
data = open(sys.argv[1], "rb").read()
sys.exit(0 if data[:8] == b"\x89PNG\r\n\x1a\n" and len(data) > 8000 else 1)
PY
    xcrun simctl terminate "$udid" "$bid" >/dev/null 2>&1 || true
    xcrun simctl uninstall "$udid" "$bid" >/dev/null 2>&1 || true
  fi
  python3 - "$dir/result.json" "$name" "$bid" "$launched" "$alive" "$fatal" "${rust_lines:-0}" "$nonblank" <<'PY'
import json, sys
path, app, bid, launched, alive, fatal, rust, nonblank = sys.argv[1:]
json.dump({"app": app, "bundle_id": bid, "booted": True, "launched": launched == "1", "alive": alive == "1",
           "fatal": fatal or None, "rust_log_lines": int(rust), "screen_nonblank": nonblank == "1"}, open(path, "w"), indent=2)
PY
  echo "[$([ "$alive" = 1 ] && [ -z "$fatal" ] && echo PASS || echo FAIL)] $name: launched=$launched alive=$alive fatal=${fatal:-none} log_lines=${rust_lines:-0} screen=$nonblank"
  { [ "$alive" = "1" ] && [ -z "$fatal" ]; } || all_ok=1
done
python3 - "$out" <<'PY'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
rows = [json.loads(p.read_text()) for p in sorted(out.glob("*/result.json"))]
(out / "summary.json").write_text(json.dumps(rows, indent=2))
PY
exit "$all_ok"
