#!/usr/bin/env bash
# ios_smoke.sh: do iOS Simulator apps BOOT? Starts ONE simulator, then for every app: installs it, takes a picture of the
# home screen, starts it, waits, takes a screenshot, looks again a few seconds later, reads the log and looks for a crash report.
#
#   scripts/ci/ios_smoke.sh <out-dir> <seconds-to-wait> <app.zip | App.app> [...]
#
# For a macOS runner with Xcode (xcrun simctl). Needs no signing (a Simulator app runs ad-hoc signed).
# Per app <out-dir>/<app name>/ holds
#   baseline.png     the home screen before the launch (what an app that died leaves behind)
#   screenshot.png   the simulator's screen after the wait; screenshot-2.png the second look
#   console.log      what the app wrote to stdout and stderr (RUST_BACKTRACE=1);  log.txt  the unified log of its process
#   crash/           the crash reports (.ips) the app left, if any
#   result.json      the verdict of scripts/ci/smoke_verdict.py: {app, id, alive, alive_after, differs_from_home,
#                    screen_nonblank, fatal, pass, reason, ...}
# (and screenshot-after-tap.png, tap.json when the manifest names a button for the app: AZ_SMOKE_TAPS, a JSON map
# of app name -> the label to press, from scripts/demo_apps.toml's [app.smoke] tap; pressed with idb when it is
# installed; AZ_SMOKE_EXPECTS the same for the text the first screen must show) and <out-dir>/summary.json lists them.
# An app BOOTS (pass) when it is alive twice, is on the screen (not the home screen, not one flat colour) and left no
# crash report and no fatal line; the reason of a failure is in result.json. Exit 0 when every app boots; 1 when one does
# not; 2 when the simulator itself could not be started.
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
  alive=0; alive_after=0; fatal=""; rust_lines=0; tapped_err=""; expect_found=""
  verdict_args=()
  running() {  # the app's job in the simulator's launchd lists a pid (a dead one shows "-")
    local state pid
    state="$(xcrun simctl spawn "$udid" launchctl list 2>/dev/null | grep "UIKitApplication:$bid" || true)"
    pid="$(echo "$state" | awk '{print $1}')"
    case "$pid" in ''|-) return 1;; *) return 0;; esac
  }
  if ! xcrun simctl install "$udid" "$appdir" >"$dir/install.log" 2>&1; then
    fatal="simctl install failed: $(tail -n 1 "$dir/install.log")"
  else
    # The home screen as it is with the app installed: the picture the app's screen is told apart from
    # (scripts/ci/smoke_verdict.py: an app that crashed or never came to the front leaves it behind).
    sleep 2
    xcrun simctl io "$udid" screenshot "$dir/baseline.png" >/dev/null 2>&1 || true
    started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    marker="$work/marker-$name"; touch "$marker"
    # Launch with the app's console attached (its stdout and stderr: a Rust panic, the engine's own log lines): the launch
    # command stays until the app ends, so it runs in the background and its being gone is a verdict of its own.
    SIMCTL_CHILD_RUST_BACKTRACE=1 xcrun simctl launch --console "$udid" "$bid" >"$dir/console.log" 2>&1 &
    launch_pid=$!
    sleep 3
    cat "$dir/console.log"
    sleep "$wait_s"
    xcrun simctl io "$udid" screenshot "$dir/screenshot.png" >/dev/null 2>&1 || true
    running && alive=1
    # the app ended on its own when the launch command is gone
    kill -0 "$launch_pid" 2>/dev/null || echo "the app's console closed: the app ended"
    # The text the manifest expects on the first screen ([app.smoke] expect): in idb's description of the screen. Best effort,
    # like the button below: without idb the text is not looked for (and the verdict does not ask for it).
    expect_text="$(python3 -c 'import json,os,sys; print(json.loads(os.environ.get("AZ_SMOKE_EXPECTS") or "{}").get(sys.argv[1], ""))' "${name%-ios-sim}")"
    if [ -n "$expect_text" ] && [ "$alive" = "1" ]; then
      if command -v idb >/dev/null 2>&1; then
        idb ui describe-all --udid "$udid" --json >"$dir/ui-first.json" 2>"$dir/idb.err" || true
        if python3 - "$dir/ui-first.json" "$expect_text" <<'PY'
import json, sys
text = sys.argv[2].lower()
try:
    items = json.load(open(sys.argv[1]))
except Exception:
    sys.exit(1)
hay = " ".join(str(v) for it in items for k, v in it.items() if k in ("AXLabel", "label", "AXValue", "value", "title"))
sys.exit(0 if text in hay.lower() else 1)
PY
        then expect_found=yes; else expect_found=no; fi
        verdict_args+=(--expect-text "$expect_text" --expect-found "$expect_found")
      else
        echo "idb is not installed: the text \"$expect_text\" is not looked for"
      fi
    fi
    # A button the manifest names ([app.smoke] tap): press it with idb (a simulator has no tap of its own in simctl),
    # by its accessibility label; else where the app's own notes put it (130 pt above the bottom edge, centred). Best
    # effort: without idb the app is judged by its first screen alone and the result says so.
    tap_label="$(python3 -c 'import json,os,sys; print(json.loads(os.environ.get("AZ_SMOKE_TAPS") or "{}").get(sys.argv[1], ""))' "${name%-ios-sim}")"
    if [ -n "$tap_label" ] && [ "$alive" = "1" ]; then
      if command -v idb >/dev/null 2>&1; then
        idb ui describe-all --udid "$udid" --json >"$dir/ui-before.json" 2>"$dir/idb.err" || true
        read -r how tx ty < <(python3 - "$dir/ui-before.json" "$tap_label" "$dir/screenshot.png" <<'PY'
import json, struct, sys
label = sys.argv[2].lower()
try:
    items = json.load(open(sys.argv[1]))
except Exception:
    items = []
for it in items:
    if label in str(it.get("AXLabel") or it.get("label") or "").lower():
        f = it.get("frame") or {}
        if f:
            print("name", int(f["x"] + f["width"] / 2), int(f["y"] + f["height"] / 2))
            sys.exit(0)
# no node of that name: the point 130 pt above the bottom edge of the screen (the screenshot is 3x on every modern iPhone)
with open(sys.argv[3], "rb") as fh:
    head = fh.read(24)
w, h = struct.unpack(">II", head[16:24])
print("place", w // 6, h // 3 - 130)
PY
)
        xy="$tx $ty"
        idb ui tap --udid "$udid" $tx $ty >"$dir/tap.log" 2>&1 || tapped_err="$(tail -n 1 "$dir/tap.log")"
        sleep 5
        xcrun simctl io "$udid" screenshot "$dir/screenshot-after-tap.png" >/dev/null 2>&1 || true
        tap_alive=0; running && tap_alive=1
        tap_fatal="${tapped_err:+idb tap failed: $tapped_err}"
        python3 - "$dir/tap.json" "$tap_label" "$how" "$xy" "$tap_alive" "$tap_fatal" <<'PY'
import json, sys
path, label, how, xy, alive, fatal = sys.argv[1:]
json.dump({"label": label, "found": "by its " + how, "at": xy, "alive": alive == "1", "fatal": fatal or None}, open(path, "w"), indent=2)
PY
        verdict_args+=(--tap "$dir/tap.json")
      else
        echo "idb is not installed: \"$tap_label\" is not pressed"
      fi
    fi
    # The second look, a few seconds after the last thing the app was asked to do: still there, still on the screen.
    sleep 3
    running && alive_after=1
    xcrun simctl io "$udid" screenshot "$dir/screenshot-2.png" >/dev/null 2>&1 || true
    xcrun simctl spawn "$udid" log show --last 3m --info --debug --style compact --predicate "process == \"$exe\"" >"$dir/log.txt" 2>&1 || true
    xcrun simctl spawn "$udid" log show --last 3m --info --debug --style compact >"$dir/log-all.txt" 2>&1 || true
    # a crash report is written a few seconds after the crash: look for up to 20 s when the app is gone
    if [ "$alive_after" != "1" ]; then
      for _ in 1 2 3 4 5 6 7 8 9 10; do
        [ -n "$(find "$HOME/Library/Logs/DiagnosticReports" "/Library/Logs/DiagnosticReports" -type f -newer "$marker" 2>/dev/null | head -n 1)" ] && break
        sleep 2
      done
    fi
    find "$HOME/Library/Logs/DiagnosticReports" "/Library/Logs/DiagnosticReports" -type f -newer "$marker" -exec cp {} "$dir/crash/" \; 2>/dev/null || true
    ls -la "$HOME/Library/Logs/DiagnosticReports" >"$dir/crash/_ls.txt" 2>&1 || true
    kill "$launch_pid" 2>/dev/null || true
    crash="$(ls "$dir/crash" 2>/dev/null | grep -v '^_ls.txt$' | head -n 1)"
    [ -n "$crash" ] && fatal="crash report: $crash"
    [ -z "$fatal" ] && fatal="$(grep -m1 -E 'panicked at|[Ff]atal error|SIGABRT|SIGSEGV|abort\(\)|Terminating app' "$dir/console.log" "$dir/log.txt" 2>/dev/null | head -n 1 | cut -d: -f2-)"
    rust_lines="$(grep -c -v '^$' "$dir/console.log" 2>/dev/null || true)"
    if [ "$alive" != "1" ] || [ "$alive_after" != "1" ]; then
      echo "--- the app's console:"; tail -n 60 "$dir/console.log"
      echo "--- the app's log lines:"; tail -n 40 "$dir/log.txt"
      echo "--- crash reports:"; ls "$dir/crash"
      for f in "$dir"/crash/*.ips "$dir"/crash/*.crash; do [ -f "$f" ] && { echo "== $f"; head -c 4000 "$f"; echo; }; done
      echo "--- the simulator's log around the launch (lines naming the app or its job):"
      grep -i -E "$exe|$bid" "$dir/log-all.txt" | tail -n 40 | cut -c1-260
    fi
    xcrun simctl terminate "$udid" "$bid" >/dev/null 2>&1 || true
    xcrun simctl uninstall "$udid" "$bid" >/dev/null 2>&1 || true
  fi
  [ -f "$dir/baseline.png" ] && verdict_args+=(--baseline "$dir/baseline.png" --after "$dir/screenshot-2.png")
  # THE pass line: scripts/ci/smoke_verdict.py (alive twice, on the screen, drawn, no fatal line); nothing else decides
  python3 "$(dirname "$0")/smoke_verdict.py" --out "$dir/result.json" --app "$name" --id "$bid" --alive "$alive" --alive-after "$alive_after" \
    --fatal "$fatal" --rust-lines "${rust_lines:-0}" ${verdict_args[@]+"${verdict_args[@]}"} || all_ok=1
done
python3 - "$out" <<'PY'
import json, pathlib, sys
out = pathlib.Path(sys.argv[1])
rows = [json.loads(p.read_text()) for p in sorted(out.glob("*/result.json"))]
(out / "summary.json").write_text(json.dumps(rows, indent=2))
PY
exit "$all_ok"
