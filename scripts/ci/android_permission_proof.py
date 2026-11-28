#!/usr/bin/env python3
"""The permission proof of the Android shell, on an emulator: AzSelfTest's geolocation row with the system's dialog answered by this script.

Run through the QA's emulator tool (it boots the emulator and calls this script in place of android_probe.py):

    AZ_PROBE_PY=scripts/ci/android_permission_proof.py bash scripts/ci/android_probe.sh out AzSelfTest-e2e-android-x86_64.apk

What it proves, each with the evidence it prints and writes to <out>/permission-proof.md and permission-proof.json:

  1. ASK ONCE, START AT THE GRANT. Nothing is granted (pm clear + pm revoke). The app starts with the row sensors.geolocation only
     (`debug.az.selftest_rows`). A tap on the window is the person's first gesture. The system's permission dialog comes ONCE, the script
     answers "While using the app", sends a location (`adb emu geo fix`) and the row must PASS with that fix: before the fix of
     2026-11-28 a subscription made before the answer never started and no fix came until the app was started a second time.
  2. A DENIAL IS REMEMBERED. pm clear again, the same start, the dialog is answered "Don't allow": the row says DENIED. The app is started
     a second time: NO dialog comes (the denial is remembered by the app, the engine does not ask behind the user's back) and the row
     says DENIED at once.

  3. THE FONT CACHE (android_font_cache_proof.py, on the same emulator): a fresh install misses and writes it, the next start hits and
     writes nothing, a damaged manifest is rebuilt, the start after it hits again; the numbers are in permission-proof.md.

The exit status is 0 when every expectation held, 1 when one did not, 2 when the app could not be started.
"""
import argparse
import json
import re
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import android_font_cache_proof as fonts  # noqa: E402
import android_probe as probe  # noqa: E402

ROW = "sensors.geolocation"
ALLOW_IDS = (
    "com.android.permissioncontroller:id/permission_allow_foreground_only_button",
    "com.android.permissioncontroller:id/permission_allow_button",
    "com.google.android.permissioncontroller:id/permission_allow_foreground_only_button",
    "com.google.android.permissioncontroller:id/permission_allow_button",
)
DENY_IDS = (
    "com.android.permissioncontroller:id/permission_deny_button",
    "com.google.android.permissioncontroller:id/permission_deny_button",
    "com.android.permissioncontroller:id/permission_deny_and_dont_ask_again_button",
)
LOCATION = ("android.permission.ACCESS_FINE_LOCATION", "android.permission.ACCESS_COARSE_LOCATION")


def report_rows(log_text):
    """The rows of the newest AZ_SELFTEST_REPORT line of a logcat: {row id: (verdict, detail)}."""
    rows = {}
    for line in probe.marker_lines(log_text, "AZ_SELFTEST_REPORT"):
        match = re.search(r"AZ_SELFTEST_REPORT\s+(\{.*\})", line)
        if not match:
            continue
        try:
            report = json.loads(match.group(1))
        except ValueError:
            continue
        rows = {r["id"]: (r.get("verdict"), r.get("detail", "")) for r in report.get("rows", [])}
    return rows


def tap_any(dev, xml, ids):
    return any(dev.tap_node(xml, resource_id=i) for i in ids)


class Scenario:
    def __init__(self, dev, package, activity, out):
        self.dev, self.package, self.activity, self.out = dev, package, activity, Path(out)
        self.lines = []
        self.ok = True

    def say(self, text):
        print(text, flush=True)
        self.lines.append(text)

    def expect(self, condition, what):
        self.say(f"  [{'ok' if condition else 'FAIL'}] {what}")
        self.ok = self.ok and bool(condition)

    def fresh(self):
        """No memory of any earlier answer: the app's data cleared, the location permission revoked."""
        self.dev.sh(f"am force-stop {self.package}")
        self.dev.sh(f"pm clear {self.package}")
        for perm in LOCATION:
            self.dev.sh(f"pm revoke {self.package} {perm}")
        self.dev.sh("setprop debug.az.selftest 0")
        self.dev.sh(f"setprop debug.az.selftest_rows {ROW}")

    def start(self):
        self.dev.sh(f"am force-stop {self.package}")
        time.sleep(1.0)
        self.dev.adb("logcat", "-c")
        self.dev.sh(f"am start -n {self.package}/{self.activity}")
        time.sleep(6.0)   # the first frame
        # the person's first gesture of the window: the engine holds a permission request until one was seen
        self.dev.sh("input tap 20 200")

    def dialog_up(self):
        return bool(probe.permission_dialog_in(self.dev.focus(), "", self.package))

    def answer_dialog(self, ids, seconds=25.0):
        """Waits up to `seconds` for the permission dialog and taps one of `ids`; -> whether it appeared (and was answered)."""
        t0 = time.time()
        while time.time() - t0 < seconds:
            if self.dialog_up():
                xml = self.dev.dump_ui()
                if tap_any(self.dev, xml, ids):
                    return True
            time.sleep(0.7)
        return False

    def verdict(self, seconds, feed_fix=False):
        """The row's verdict from the newest report in the log, waiting up to `seconds` (feeding the emulator a location while it waits)."""
        t0 = time.time()
        last = None
        while time.time() - t0 < seconds:
            if feed_fix:
                self.dev.adb("emu", "geo", "fix", "8.67", "48.52")
            rows = report_rows(self.dev.adb("logcat", "-d", "-v", "threadtime", timeout=60))
            if ROW in rows:
                last = rows[ROW]
                if last[0] in ("PASS", "DENIED", "FAIL", "NEEDS-HUMAN", "UNAVAILABLE"):
                    return last
            time.sleep(2.0)
        return last

    def dialogs_for(self, seconds):
        """How many distinct permission dialogs showed in `seconds` (a dialog that stays is one)."""
        log = probe.watch_dialogs(self.dev, self.package, seconds)
        return len(log.seen)

    def run(self):
        self.say("== 1. ask once, start at the grant ==")
        self.fresh()
        self.start()
        answered = self.answer_dialog(ALLOW_IDS)
        self.expect(answered, "the system's permission dialog came and was answered 'While using the app'")
        verdict = self.verdict(40.0, feed_fix=True)
        self.say(f"  row {ROW}: {verdict}")
        self.expect(bool(verdict) and verdict[0] == "PASS", "the row passed with a fix after the grant (it never started before the fix)")
        again = self.dialogs_for(4.0)
        self.expect(again == 0, "no second dialog came")

        self.say("== 2. a denial is remembered ==")
        self.fresh()
        self.start()
        answered = self.answer_dialog(DENY_IDS)
        self.expect(answered, "the dialog came and was answered 'Don't allow'")
        verdict = self.verdict(40.0)
        self.say(f"  row {ROW}: {verdict}")
        self.expect(bool(verdict) and verdict[0] == "DENIED", "the row says DENIED")
        # the second start: the data is NOT cleared, the answer was no
        self.dev.sh(f"am force-stop {self.package}")
        time.sleep(1.0)
        self.dev.adb("logcat", "-c")
        self.dev.sh(f"am start -n {self.package}/{self.activity}")
        time.sleep(6.0)
        self.dev.sh("input tap 20 200")
        shown = self.dialogs_for(12.0)
        self.expect(shown == 0, "the second start shows NO dialog (the refusal is remembered)")
        verdict = self.verdict(30.0)
        self.say(f"  row {ROW} after the second start: {verdict}")
        self.expect(bool(verdict) and verdict[0] == "DENIED", "and the row says DENIED at once")
        self.dev.sh("setprop debug.az.selftest_rows ''")
        return self.ok


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apk", default="")
    ap.add_argument("--package", default="")
    ap.add_argument("--activity", default="")
    ap.add_argument("--out", required=True)
    ap.add_argument("--serial", default="")
    args, _ = ap.parse_known_args(argv)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    dev = probe.Dev(args.serial)
    package, activity = args.package, args.activity
    if args.apk:
        if not package or not activity:
            pkg, act = probe.launcher_of(args.apk)
            package, activity = package or pkg, activity or act
        text = dev.adb("install", "-r", "-t", str(args.apk), timeout=300)
        if "Success" not in text:
            print(f"the install failed: {text.strip()[:300]}")
            return 2
    if not package or not activity:
        print("no package and activity: give --package and --activity, or an APK whose manifest names them")
        return 2
    scenario = Scenario(dev, package, activity, out)
    ok = scenario.run()
    # the font cache on the same emulator: miss, hit, damaged, hit (scripts/ci/android_font_cache_proof.py)
    fonts.run_font_scenarios(dev, package, activity, scenario.say, scenario.expect)
    ok = ok and scenario.ok
    (out / "permission-proof.md").write_text("# permission proof\n\n" + "\n".join(scenario.lines) + "\n", encoding="utf-8")
    (out / "permission-proof.json").write_text(json.dumps({"ok": ok, "lines": scenario.lines}, indent=1), encoding="utf-8")
    full = dev.adb("logcat", "-d", "-v", "threadtime", timeout=120)
    (out / "permission-proof.log").write_text(full, encoding="utf-8")
    print("PERMISSION PROOF: " + ("all held" if ok else "SOMETHING DID NOT HOLD"))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
