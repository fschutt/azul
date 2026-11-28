#!/usr/bin/env python3
"""android_phone_selftest.py: AzSelfTest on a real Android phone over USB, the things an emulator cannot show (the
fingerprint sensor, the haptic motor, the proximity sensor, a real camera, GPS, the soft keyboard of the phone's own IME)
(standard library only).

    python3 scripts/ci/android_phone_selftest.py --apk AzSelfTest-e2e-android-arm64.apk [--serial <adb serial>] [--grant] [--out android-phone-selftest]
    python3 scripts/ci/android_phone_selftest.py --no-install --rows sensors.geolocation,system.haptics     # only these rows

The phone must be connected by USB with USB debugging on (Settings > Developer options) and trusted for this computer (accept the
"Allow USB debugging?" question on the phone once). `adb` is taken from PATH or from $ANDROID_HOME/platform-tools. The APK is the
QA build `AzSelfTest-e2e-android-arm64.apk`: `scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_build_app.sh AzSelfTest aarch64-linux-android'
--codegen --artifacts 'demo-out/apps/*'` builds just that one (about 30 minutes on a runner), `gh run download <run id>` fetches it; the `demos` runs that
ask for the QA variants build it too. `--no-install` runs the app that is already installed.

What it does, in this order, telling you what to do:

  1. installs the APK (replacing the old one, keeping its data) and starts the app UNATTENDED: every probe that needs no person runs,
     the report comes back as one line (`AZ_SELFTEST_REPORT {json}` in logcat) and is printed as a table. A permission dialog is NEVER
     shown in this pass (probes that need a permission list what to grant); `--grant` grants location, notifications, camera and
     microphone first (`adb shell pm grant`), `--revoke` takes them away (the denied path).
  2. starts the app again WITH you: the list appears on the phone. It tells you in plain words which rows need a person and what to
     do for each (touch the fingerprint sensor, feel for the buzz, cover the proximity sensor, answer the permission dialogs), you tap
     "Run all", and it collects the report the app prints when the last row is done (press Enter here to stop waiting).
  3. writes <out>/report-unattended.json, <out>/report-attended.json, <out>/logcat.txt and prints the table of both runs.

The exit code is 1 when a capability that says it is available FAILED in either run, else 0. UNAVAILABLE (the phone has no such
thing), DENIED (a permission was refused) and NEEDS-HUMAN (nobody did it) are not failures. Nothing on the phone is deleted: the app's
own data (and its stored report) stays until you uninstall it.
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

REPORT_PREFIX = "AZ_SELFTEST_REPORT"
PART_PREFIX = REPORT_PREFIX + "_PART"
PART_LINE = re.compile(re.escape(PART_PREFIX) + r" (\d+)/(\d+) (.*)$", re.S)
PACKAGE = "com.azul.AzSelfTest"
ACTIVITY = "com.azul.app.AzulActivity"
PERMISSIONS = (
    "android.permission.ACCESS_FINE_LOCATION",
    "android.permission.ACCESS_COARSE_LOCATION",
    "android.permission.POST_NOTIFICATIONS",
    "android.permission.CAMERA",
    "android.permission.RECORD_AUDIO",
)
UNATTENDED_WAIT_S = 150
ATTENDED_WAIT_S = 600


def reports_in(text):
    """Every report of a log, as dicts, in order: the one line `AZ_SELFTEST_REPORT {json}` and a report that was longer than a logcat
    line holds (logcat drops what is past ~4000 bytes of a line without a word), sent as `AZ_SELFTEST_REPORT_PART i/n <slice>` lines whose
    slices, joined in order, are the JSON. A line that is not valid JSON is skipped (a log line cut in two); a set of parts that is not
    complete (a part lost, another report in between) is dropped."""
    found, parts, want = [], [], 0
    for raw in text.split("\n"):
        line = raw[:-1] if raw.endswith("\r") else raw
        at = line.find(PART_PREFIX + " ")
        if at >= 0:
            match = PART_LINE.match(line[at:])
            if match:
                index, count, slice_ = int(match.group(1)), int(match.group(2)), match.group(3)
                if index == 1:
                    parts, want = [slice_], count
                elif parts and index == len(parts) + 1 and count == want:
                    parts.append(slice_)
                else:
                    parts, want = [], 0
                if parts and len(parts) == want:
                    try:
                        found.append(json.loads("".join(parts)))
                    except ValueError:
                        pass
                    parts, want = [], 0
            continue
        at = line.find(REPORT_PREFIX + " {")
        if at < 0:
            continue
        parts, want = [], 0
        try:
            found.append(json.loads(line[at + len(REPORT_PREFIX) + 1:]))
        except ValueError:
            continue
    return found


def table(report):
    """The rows of a report as lines a person reads: `VERDICT  id  detail`, then the counts. -> [str]"""
    rows = report.get("rows", [])
    width = max([len(r.get("id", "")) for r in rows] + [10])
    lines = [f"{r.get('verdict', '?'):<11} {r.get('id', ''):<{width}}  {r.get('detail', '')}" for r in rows]
    counts = ", ".join(f"{report.get(k, 0)} {k.replace('_', ' ')}" for k in ("pass", "unavailable", "denied", "fail", "needs_human", "pending"))
    lines.append(f"-- {counts} ({report.get('platform', '?')} {report.get('arch', '?')}, {report.get('total_ms', 0)} ms)")
    return lines


def failures(report):
    """The ids of the rows that FAILED."""
    return [r.get("id", "?") for r in report.get("rows", []) if r.get("verdict") == "FAIL"]


def people_to_do(report):
    """What a person has to do, from the rows that say NEEDS-HUMAN: `(id, instruction)`."""
    return [(r.get("id", "?"), r.get("detail", "")) for r in report.get("rows", []) if r.get("verdict") == "NEEDS-HUMAN"]


def permission_dialog_focused(dumpsys_window):
    """Whether the focused window in the text of `adb shell dumpsys window` is the system's permission dialog
    (GrantPermissionsActivity of the permission controller, which has different package names on AOSP, Google and vendor
    builds). The unattended pass must never show one: a permission dialog comes only when the person asks for the feature."""
    for line in dumpsys_window.splitlines():
        if "mCurrentFocus" not in line and "mFocusedWindow" not in line:
            continue
        low = line.lower()
        if "grantpermissionsactivity" in low or "permissioncontroller" in low:
            return True
    return False


def find_adb():
    """`adb` from PATH or from the Android SDK the environment names; None when there is none."""
    found = shutil.which("adb")
    if found:
        return found
    for var in ("ANDROID_HOME", "ANDROID_SDK_ROOT"):
        root = os.environ.get(var)
        if root:
            candidate = Path(root) / "platform-tools" / "adb"
            if candidate.is_file():
                return str(candidate)
    return None


class Phone:
    def __init__(self, adb, serial):
        self.adb = adb
        self.serial = serial
        self.dialog_seen = False

    def run(self, *args, timeout=60, check=False):
        cmd = [self.adb] + (["-s", self.serial] if self.serial else []) + list(args)
        done = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
        if check and done.returncode != 0:
            raise SystemExit(f"adb {' '.join(args)} failed: {(done.stderr or done.stdout).strip()}")
        return done.stdout

    def sh(self, command, **kwargs):
        return self.run("shell", command, **kwargs)

    def prop(self, name):
        return self.sh(f"getprop {name}").strip()

    def logcat(self):
        return self.run("logcat", "-d", "-v", "brief", timeout=120)

    def launch(self):
        self.sh(f"am force-stop {PACKAGE}")
        self.run("logcat", "-c")
        self.sh(f"am start -n {PACKAGE}/{ACTIVITY}", check=True)

    def permission_dialog_up(self):
        """Whether the system's permission dialog has the focus right now."""
        return permission_dialog_focused(self.sh("dumpsys window | grep -E 'mCurrentFocus|mFocusedWindow'"))

    def wait_for_report(self, seconds, after=0, watch_dialog=False):
        """The newest report in logcat once there are more than `after`, else the newest there is (or None). With
        `watch_dialog` the focused window is looked at on every round: `self.dialog_seen` says whether the system's
        permission dialog was ever in front."""
        deadline = time.time() + seconds
        reports = []
        while time.time() < deadline:
            if watch_dialog and self.permission_dialog_up():
                self.dialog_seen = True
            reports = reports_in(self.logcat())
            if len(reports) > after:
                return reports[-1], reports
            time.sleep(3)
        return (reports[-1] if reports else None), reports


def pick_device(adb, serial):
    """The serial to use: the one asked for, or the only connected phone. SystemExit with plain words when there is none or several."""
    listing = subprocess.run([adb, "devices"], capture_output=True, text=True, timeout=30).stdout.splitlines()[1:]
    devices = [line.split()[0] for line in listing if line.strip().endswith("device")]
    unauthorized = [line.split()[0] for line in listing if line.strip().endswith("unauthorized")]
    if serial:
        if serial not in devices:
            raise SystemExit(f"{serial} is not a connected phone (adb sees: {', '.join(devices) or 'none'})")
        return serial
    if unauthorized and not devices:
        raise SystemExit("the phone is connected but has not allowed this computer: unlock it and accept the 'Allow USB debugging?' question")
    if not devices:
        raise SystemExit("no phone found: connect it by USB, turn on USB debugging (Settings > Developer options) and run adb devices")
    if len(devices) > 1:
        raise SystemExit(f"more than one device ({', '.join(devices)}): choose with --serial")
    return devices[0]


def main(argv=None):
    ap = argparse.ArgumentParser(description="AzSelfTest on a real Android phone over USB")
    ap.add_argument("--apk", help="the AzSelfTest APK (arm64) to install")
    ap.add_argument("--no-install", action="store_true", help="run the app that is installed already")
    ap.add_argument("--serial", help="the adb serial, when more than one device is connected")
    ap.add_argument("--grant", action="store_true", help="grant location, notifications, camera and microphone first")
    ap.add_argument("--revoke", action="store_true", help="revoke them first (to see the denied path)")
    ap.add_argument("--unattended-only", action="store_true", help="skip the pass with a person")
    ap.add_argument("--rows", default="", help="run only these rows, comma separated ids (sensors.geolocation,system.haptics): the app runs them and prints the report")
    ap.add_argument("--out", default="android-phone-selftest", help="where the reports and the log go")
    args = ap.parse_args(argv)

    adb = find_adb()
    if not adb:
        raise SystemExit("adb not found: install the Android platform-tools or set ANDROID_HOME")
    phone = Phone(adb, pick_device(adb, args.serial))
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    print(f"phone: {phone.prop('ro.product.manufacturer')} {phone.prop('ro.product.model')}, Android {phone.prop('ro.build.version.release')} "
          f"(API {phone.prop('ro.build.version.sdk')}), {phone.prop('ro.product.cpu.abi')}")

    if not args.no_install:
        if not args.apk:
            raise SystemExit("give the APK with --apk (or --no-install to run the app that is installed)")
        print(f"installing {args.apk} ...")
        phone.run("install", "-r", "-d", args.apk, timeout=300, check=True)
    for perm in PERMISSIONS:
        if args.grant:
            phone.sh(f"pm grant {PACKAGE} {perm}")   # a permission the Android version does not know is refused: fine
        elif args.revoke:
            phone.sh(f"pm revoke {PACKAGE} {perm}")

    if args.rows:
        # only the named rows, with the phone's person (or this script's feed): one report, no list to tap through
        phone.sh(f"setprop debug.az.selftest_rows '{args.rows}'")
        print(f"\nrows: {args.rows} (the app runs only these; answer the dialog that appears)")
        phone.sh("setprop debug.az.selftest 0")
        phone.launch()
        report, _ = phone.wait_for_report(ATTENDED_WAIT_S)
        phone.sh("setprop debug.az.selftest_rows ''")
        (out / "logcat.txt").write_text(phone.logcat(), encoding="utf-8")
        if report is None:
            raise SystemExit(f"no AZ_SELFTEST_REPORT line: see {out / 'logcat.txt'}")
        (out / "report-rows.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
        print("\n".join(table(report)))
        return 1 if failures(report) else 0

    print("\n== 1/2: the probes that need no person (the phone's screen shows the list while it runs; touch nothing) ==")
    phone.sh("setprop debug.az.selftest 1")
    try:
        phone.launch()
        unattended, _ = phone.wait_for_report(UNATTENDED_WAIT_S, watch_dialog=True)
    finally:
        phone.sh("setprop debug.az.selftest 0")
    (out / "logcat.txt").write_text(phone.logcat(), encoding="utf-8")
    if unattended is None:
        raise SystemExit(f"no AZ_SELFTEST_REPORT line within {UNATTENDED_WAIT_S} s: see {out / 'logcat.txt'} (is the app installed and does it start?)")
    (out / "report-unattended.json").write_text(json.dumps(unattended, indent=1), encoding="utf-8")
    print("\n".join(table(unattended)))

    attended = None
    if not args.unattended_only:
        todo = people_to_do(unattended)
        print("\n== 2/2: the probes that need YOU. The app opens now with its list. ==")
        if todo:
            print("Do these when the row asks (tap 'Run all' at the top, or 'Run' on one row):")
            for rid, what in todo:
                print(f"  - {rid}: {what}")
        print("Answer the permission dialogs that appear (each once). Press Enter here when the last row is done "
              f"(or after {ATTENDED_WAIT_S // 60} minutes).")
        phone.launch()
        # the app prints its report whenever a run of rows is over; Enter ends the wait early
        started = time.time()
        seen = 0
        import select
        while time.time() - started < ATTENDED_WAIT_S:
            ready, _, _ = select.select([sys.stdin], [], [], 5)
            if ready:
                sys.stdin.readline()
                break
            seen = len(reports_in(phone.logcat()))
        attended = (reports_in(phone.logcat()) or [None])[-1]
        (out / "logcat.txt").write_text(phone.logcat(), encoding="utf-8")
        if attended is None:
            print("no report from the attended run: tap 'Run all' on the phone, wait until every row has an answer, then run this again with --no-install.")
        else:
            (out / "report-attended.json").write_text(json.dumps(attended, indent=1), encoding="utf-8")
            print("\n".join(table(attended)))

    bad = failures(unattended) + (failures(attended) if attended else [])
    if phone.dialog_seen:
        bad.append("permission-dialog-in-unattended-run")
        print("\nA PERMISSION DIALOG SHOWED UP IN THE UNATTENDED RUN: nothing may ask for a permission until a person asks for the feature.")
    print()
    if bad:
        print("FAILED: " + ", ".join(sorted(set(bad))) + f"  (the log: {out / 'logcat.txt'})")
        return 1
    print(f"no capability that says it is available failed. Reports in {out}/")
    return 0


if __name__ == "__main__":
    sys.exit(main())
