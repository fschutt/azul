#!/usr/bin/env python3
"""ios_phone_selftest.py: the capabilities the Simulator cannot show, on YOUR iPhone, with one command (standard library only).

    python3 scripts/ci/ios_phone_selftest.py <device> [--bundle com.azul.AzSelfTest] [--app AzSelfTest.app] [--attended] [--seconds 120] [--out ios-phone-selftest]

The Simulator has NO accelerometer, gyroscope, magnetometer, barometer, camera or Taptic Engine: the rows for them stay
"wired, not proven" until a real phone runs them. This installs the self-test app on the phone, starts it, tells you in plain
words what to do, and reads what it found.

  <device>   what `xcrun devicectl list devices` shows for the phone (its name or identifier): connected by cable, unlocked,
             trusted. Needs a Mac with Xcode 15 or newer.
  --app      the signed AzSelfTest.app to install first (signed with your own development profile; see
             doc/guide/en/deploying/mobile.md). Without it the app must be installed already.
             `bash scripts/ci/ios_phone_selftest_build.sh` builds the unsigned app from the sources on your Mac and, with
             IOS_SIGNING_IDENTITY and IOS_DEVICE set, signs it and runs this script.
  (default)  UNATTENDED: the app runs every row that needs no person, writes its report and exits; nothing for you to do.
             Rows that need a person (a shake, a word, a hand over the proximity sensor) are listed with what to do.
  --attended the app shows its list: press "Run all" and follow the instruction written on each row that says NEEDS-HUMAN
             (shake the phone, cover the top of it with your hand, say a word...). The script waits for the report.

What it prints and writes (--out): the console (`console.txt`), the report (`report.json`, the one `AZ_SELFTEST_REPORT {json}`
line the app prints) and `summary.txt`. The exit code is 1 when a row that says it is available FAILED, or when a row a real
phone must pass (the motion sensors, the barometer, the camera, haptics) says UNAVAILABLE: that is a finding, not a pass.
Nothing is installed or deleted on the phone apart from the app, and no permission is granted for you: a permission dialog
that appears is the app asking, and answering it is yours.
"""
import argparse
import json
import plistlib
import re
import subprocess
import sys
import threading
import time
from pathlib import Path

REPORT = re.compile(r"AZ_SELFTEST_REPORT (\{.*\})")

# The rows a real iPhone must PASS (the Simulator has none of these: it can only say UNAVAILABLE). ids are the stable
# `group.name` of examples/azul-selftest. A row of this list that says UNAVAILABLE on a phone is a finding.
PHONE_MUST_PASS = (
    "sensors.accelerometer",
    "sensors.gyroscope",
    "sensors.magnetometer",
    "sensors.barometer",
    "media.camera",
    "system.haptics",
)

# What to do, in plain words, for the rows a person has to help with (the app writes its own text into `detail`; these
# are the words the script adds in front when the app's row is NEEDS-HUMAN).
HUMAN_PREFIX = "NEEDS A PERSON: "


def report_in(text):
    """Whether the console already holds a complete report line."""
    return bool(parse_reports(text))


def parse_reports(text):
    """Every report line in a console: the JSON objects, in order (the app prints one per run)."""
    reports = []
    for line in text.splitlines():
        m = REPORT.search(line)
        if m:
            try:
                reports.append(json.loads(m.group(1)))
            except json.JSONDecodeError:
                continue
    return reports


def rows_by_id(report):
    return {row.get("id", ""): row for row in report.get("rows", [])}


def findings(report):
    """-> [str]: what is wrong in a report of a real phone. A FAIL is a bug (a row that said available and then did not
    work); an UNAVAILABLE row a phone must pass is a finding; a PENDING row is a hole in the app, said once."""
    out = []
    rows = rows_by_id(report)
    for row in report.get("rows", []):
        if row.get("verdict") == "FAIL":
            out.append(f"FAIL {row.get('id')}: {row.get('detail')}")
    for rid in PHONE_MUST_PASS:
        row = rows.get(rid)
        if row is None:
            out.append(f"MISSING {rid}: the app has no such row (it is a row a real phone must pass)")
        elif row.get("verdict") == "UNAVAILABLE":
            out.append(f"FINDING {rid}: UNAVAILABLE on a real phone ({row.get('detail')}): the capability says it is not there")
        elif row.get("verdict") == "DENIED":
            out.append(f"FINDING {rid}: DENIED ({row.get('detail')}): allow it in Settings > the app, then run again")
    return out


def render(report):
    """The table a person reads: one line per row, the instructions of the NEEDS-HUMAN rows under it."""
    lines = []
    width = max((len(r.get("id", "")) for r in report.get("rows", [])), default=10)
    for row in report.get("rows", []):
        lines.append(f"  {row.get('verdict', '?'):<12} {row.get('id', ''):<{width}}  {row.get('detail', '')}")
    human = [r for r in report.get("rows", []) if r.get("verdict") == "NEEDS-HUMAN"]
    if human:
        lines.append("")
        lines.append("what to do (run again with --attended, then press Run all):")
        for row in human:
            lines.append(f"  - {row.get('title', row.get('id'))}: {row.get('detail')}")
    counts = {k: report.get(k) for k in ("pass", "unavailable", "denied", "fail", "needs_human", "pending")}
    lines.append("")
    lines.append("counts: " + ", ".join(f"{k}={v}" for k, v in counts.items() if v is not None))
    return lines


def verdict(report):
    """-> (exit code, [lines]) for one report."""
    found = findings(report)
    lines = render(report)
    if found:
        lines.append("")
        lines += found
    return (1 if found else 0), lines


def launch(device, bundle, seconds, attended):
    """Start the app and keep its console for `seconds` (the client of `devicectl` is stopped then; an unattended app has
    exited by itself long before). -> the console text"""
    env = {"AZ_LOG": "info", "RUST_BACKTRACE": "1"}
    if not attended:
        env["AZ_SELFTEST"] = "1"
    cmd = ["xcrun", "devicectl", "device", "process", "launch", "--device", device, "--terminate-existing",
           "--environment-variables", json.dumps(env), "--console", bundle]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, errors="replace", bufsize=1)
    lines = []

    def pump():
        for line in proc.stdout:
            lines.append(line)

    reader = threading.Thread(target=pump, daemon=True)
    reader.start()
    # An iPhone app does not exit when its window closes (UIApplicationMain never returns): the console stays attached for
    # as long as the app lives, so the client is stopped as soon as the report is in (the app keeps running on the phone).
    deadline = time.time() + seconds
    while time.time() < deadline and proc.poll() is None:
        time.sleep(0.5)
        if report_in("".join(lines[-200:])):
            time.sleep(1.0)
            break
    if proc.poll() is None:
        proc.terminate()
    reader.join(timeout=10)
    return "".join(lines)


DEFAULT_BUNDLE = "com.azul.AzSelfTest"


def bundle_of(app, explicit):
    """The bundle identifier to launch: the one asked for, else the one the app says (CFBundleIdentifier), else the default."""
    if explicit:
        return explicit
    if app:
        try:
            with open(Path(app) / "Info.plist", "rb") as f:
                found = plistlib.load(f).get("CFBundleIdentifier")
            if found:
                return found
        except (OSError, plistlib.InvalidFileException):
            pass
    return DEFAULT_BUNDLE


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("device", help="the phone: its name or identifier from `xcrun devicectl list devices`")
    ap.add_argument("--bundle", help="the app's bundle identifier (default: the one of --app's Info.plist, else com.azul.AzSelfTest)")
    ap.add_argument("--app", help="a signed AzSelfTest.app to install first")
    ap.add_argument("--attended", action="store_true", help="show the list and wait for a person (press Run all)")
    ap.add_argument("--seconds", type=int, default=120, help="how long to keep the console")
    ap.add_argument("--out", default="ios-phone-selftest", help="where the files are written")
    args = ap.parse_args(argv)
    seconds = max(20, min(args.seconds, 900))
    args.bundle = bundle_of(args.app, args.bundle)
    try:
        listed = subprocess.run(["xcrun", "devicectl", "list", "devices"], capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired) as e:
        print(f"xcrun devicectl does not run here ({e}): this needs a Mac with Xcode 15 or newer")
        return 2
    if listed.returncode != 0 or args.device not in listed.stdout:
        print(f"`xcrun devicectl list devices` does not show {args.device!r}:\n{listed.stdout or listed.stderr}")
        return 2
    if args.app:
        app = Path(args.app)
        if not app.is_dir():
            print(f"{app} is not an app bundle (a folder ending in .app)")
            return 2
        print(f"installing {app} on {args.device} ...", flush=True)
        installed = subprocess.run(["xcrun", "devicectl", "device", "install", "app", "--device", args.device, str(app)],
                                   capture_output=True, text=True, timeout=600)
        print((installed.stdout + installed.stderr).strip()[-600:])
        if installed.returncode != 0:
            print("the install failed: is the app signed for this phone, and is the phone unlocked and trusted?")
            return 2
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    if args.attended:
        print("================ the app starts with its list: press Run all, and do what each NEEDS-HUMAN row says "
              f"(shake it, cover the top with your hand, say a word...). Waiting up to {seconds} s for the report ================",
              flush=True)
    else:
        print(f"================ the app runs every row that needs no person ({seconds} s at most; nothing for you to do) "
              "================", flush=True)
    t0 = time.time()
    text = launch(args.device, args.bundle, seconds, args.attended)
    (out / "console.txt").write_text(text, encoding="utf-8")
    reports = parse_reports(text)
    if not reports:
        print("the app printed no AZ_SELFTEST_REPORT line: is it installed (--app), unlocked on the phone, and a libazul with the "
              "self-test rows? The console is in " + str(out / "console.txt"))
        print("\n".join(text.splitlines()[-12:]))
        return 1
    report = reports[-1]
    (out / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    code, lines = verdict(report)
    (out / "summary.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n================ summary ================")
    print("\n".join(lines))
    print(f"({time.time() - t0:.0f} s) files: {out}/console.txt, report.json, summary.txt")
    return code


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
