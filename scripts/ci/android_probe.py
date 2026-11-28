#!/usr/bin/env python3
"""android_probe.py: run an APK on the booted emulator (or device) and keep what it logs (standard library only).

  android_probe.py --apk X.apk --out DIR [--package P --activity A] [--prop key=value]... [--env 'K=V;K2=V2'] [--marker TEXT]
                   [--seconds 90] [--starts 1] [--clear-data] [--watch-permissions] [--startup] [--serial S]

What it does, per start (the app is force-stopped before each: every start is a cold one):
  - logcat is cleared, the launcher activity started (`am start -n`), and for `--seconds` (or until the marker line has been seen and
    3 more seconds have passed) the focused window is polled every 0.7 s; with `--watch-permissions` every poll asks whether it is the
    system's permission controller, and every ~2 s a `uiautomator dump` is searched for Allow / Don't allow buttons: ANY dialog that ever
    showed is kept with its time, and the run FAILS (exit 4);
  - the whole logcat of the start goes to DIR/start-N.log; the lines holding `--marker` to DIR/marker.txt (exit 3 when it never appeared);
    with `--startup` (the engine's own timeline, AZ_STARTUP=1) the `[startup]` and `[fonts]` lines of every start are kept apart in
    DIR/start-N.timeline.txt, so a first start (a font cache miss) and the second one (a hit) can be compared;
  - the app's cache directory is listed after each start (`adb root` first: the AOSP image allows it), DIR/start-N.cache.txt.
`--clear-data` runs `pm clear` once before the first start (a fresh cache). `--prop` sets a system property before the starts
(`debug.az.selftest=1`); `--env` is the engine's `debug.az.env` (AZ_LOG=info;AZ_DEBUG=8795 ...). DIR/probe.json and probe.md say what happened.
The helpers (dump_ui, find_node, tap_node, watch_dialogs, permission_dialog_in) can be imported by an attended script of its own.
Exit: 0 all good, 2 the device or the install failed, 3 the marker never appeared, 4 a permission dialog showed.
"""
import argparse
import json
import re
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mobile_qa as q  # noqa: E402

POLL_SECONDS = 0.7
DUMP_EVERY_POLLS = 3          # a uiautomator dump costs ~1 s on an emulator: every third poll
MARKER_SETTLE_SECONDS = 3.0


# ---- pure parts -------------------------------------------------------------------------------------------------------------------------

def marker_lines(text, marker):
    """The lines of a logcat dump that hold `marker`, without the logcat prefix (everything up to the tag's colon)."""
    out = []
    for line in (text or "").splitlines():
        if marker and marker in line:
            out.append(re.sub(r"^.*?\s[A-Z]\s[^:]+:\s", "", line, count=1) if re.match(r"^\d\d-\d\d ", line) else line)
    return out


def timeline_lines(text):
    """The engine's startup timeline of one start: the lines of the `[startup]` tables and of the font cache (`[fonts]`, `fonts:cache`...)."""
    keep = []
    for line in (text or "").splitlines():
        if "[startup" in line or "[fonts]" in line or "fonts:" in line:
            keep.append(re.sub(r"^\d\d-\d\d \d\d:\d\d:\d\d\.\d+\s+\d+\s+\d+\s+[A-Z]\s+[^:]+:\s", "", line))
    return keep


def permission_dialog_in(focus, ui_xml, package=""):
    """-> a description of the system permission dialog the focus or the dump shows, or None (mobile_qa.detect_permission_dialog)."""
    return q.detect_permission_dialog(focus, ui_xml, package)


def find_node(ui_xml, text=None, resource_id=None):
    """The centre (x, y) of the first node of a `uiautomator dump` whose text / content-desc equals `text` or whose resource-id equals
    `resource_id` (the permission controller's buttons have ids: com.android.permissioncontroller:id/permission_allow_foreground_only_button)
    and that has a box. -> (x, y) or None."""
    for m in re.finditer(r"<node\b([^>]*)>", ui_xml or ""):
        attrs = dict(re.findall(r'([\w-]+)="([^"]*)"', m.group(1)))
        hit = (resource_id and attrs.get("resource-id") == resource_id) or (text and text in (attrs.get("text"), attrs.get("content-desc")))
        b = re.match(r"\[(\d+),(\d+)\]\[(\d+),(\d+)\]", attrs.get("bounds", ""))
        if hit and b:
            l, t, r, bt = (int(v) for v in b.groups())
            if r > l and bt > t:
                return (l + r) // 2, (t + bt) // 2
    return None


class DialogLog:
    """What a watch saw: [(seconds into the start, description)] of every poll that found a permission dialog (the first of a run of
    identical findings only)."""

    def __init__(self):
        self.seen = []

    def observe(self, seconds, description):
        if description and not (self.seen and self.seen[-1][1] == description):
            self.seen.append((round(seconds, 1), description))
        return description


# ---- the device -------------------------------------------------------------------------------------------------------------------------

class Dev:
    def __init__(self, serial=""):
        self.serial = serial

    def adb(self, *args, timeout=90):
        cmd = ["adb"] + (["-s", self.serial] if self.serial else []) + list(args)
        try:
            p = subprocess.run(cmd, capture_output=True, timeout=timeout)
        except subprocess.TimeoutExpired:
            return ""
        return p.stdout.decode("utf-8", "replace").replace("\r", "")

    def sh(self, command, timeout=90):
        return self.adb("shell", command, timeout=timeout)

    def focus(self):
        return self.sh("dumpsys window | grep -E 'mCurrentFocus'", timeout=20)

    def dump_ui(self, path="/sdcard/probe-ui.xml"):
        self.sh(f"uiautomator dump {path}", timeout=40)
        return self.sh(f"cat {path}", timeout=20)

    def tap_node(self, ui_xml, text=None, resource_id=None):
        """Taps the node `find_node` finds; -> whether there was one."""
        at = find_node(ui_xml, text=text, resource_id=resource_id)
        if at:
            self.sh(f"input tap {at[0]} {at[1]}")
        return bool(at)


def watch_dialogs(dev, package, seconds, log=None, poll=POLL_SECONDS):
    """Polls the focused window (and, every few polls, a uiautomator dump) for `seconds`; -> the DialogLog. For an attended script that wants
    to know whether a permission dialog ever showed while it did something else."""
    log = log or DialogLog()
    t0, polls = time.time(), 0
    while time.time() - t0 < seconds:
        polls += 1
        focus = dev.focus()
        found = permission_dialog_in(focus, "", package)
        if not found and polls % DUMP_EVERY_POLLS == 0:
            found = permission_dialog_in(focus, dev.dump_ui(), package)
        log.observe(time.time() - t0, found)
        time.sleep(poll)
    return log


def launcher_of(apk):
    """(package, activity) of an APK from its manifest (aapt2), or ('', '')."""
    import flicker_audit
    return flicker_audit.apk_identity(apk)


# ---- the run ----------------------------------------------------------------------------------------------------------------------------

def run_probe(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    dev = Dev(args.serial)
    package, activity = args.package, args.activity
    if args.apk:
        if not package or not activity:
            pkg, act = launcher_of(args.apk)
            package, activity = package or pkg, activity or act
        text = dev.adb("install", "-r", "-t", str(args.apk), timeout=300)
        if "Success" not in text:
            print(f"the install failed: {text.strip()[:300]}")
            return 2
    if not package or not activity:
        print("no package and activity: give --package and --activity, or an APK whose manifest names them")
        return 2
    report = {"package": package, "activity": activity, "apk": str(args.apk), "starts": [], "marker": args.marker,
              "android": dev.sh("getprop ro.build.version.sdk").strip(), "abi": dev.sh("getprop ro.product.cpu.abi").strip()}
    report["adb_root"] = dev.adb("root", timeout=30).strip()[:80]          # best effort: only the cache listing needs it
    dev.adb("wait-for-device", timeout=60)
    if args.clear_data:
        report["clear_data"] = dev.sh(f"pm clear {package}").strip()
    for item in args.prop or []:
        key, _eq, value = item.partition("=")
        dev.sh(f"setprop {key} '{value}'")
    env = args.env or ""
    if args.startup and "AZ_STARTUP" not in env:
        env = (env + ";" if env else "") + "AZ_STARTUP=1"
    if env:
        dev.sh(f"setprop debug.az.env '{env}'")
        report["env"] = env
    found_any, dialogs_any = False, []
    for n in range(1, args.starts + 1):
        dev.sh(f"am force-stop {package}")
        time.sleep(1.0)
        dev.adb("logcat", "-c")
        start = {"n": n, "marker": [], "dialogs": []}
        t0 = time.time()
        dev.sh(f"am start -n {package}/{activity}")
        log, polls, marker_at = DialogLog(), 0, None
        while time.time() - t0 < args.seconds:
            polls += 1
            focus = dev.focus()
            if args.watch_permissions:
                found = permission_dialog_in(focus, "", package)
                if not found and polls % DUMP_EVERY_POLLS == 0:
                    found = permission_dialog_in(focus, dev.dump_ui(), package)
                log.observe(time.time() - t0, found)
            if args.marker and polls % 3 == 0:
                lines = marker_lines(dev.adb("logcat", "-d", "-v", "threadtime", timeout=60), args.marker)
                if lines and marker_at is None:
                    marker_at = time.time()
                if lines:
                    start["marker"] = lines
            if marker_at is not None and time.time() - marker_at >= MARKER_SETTLE_SECONDS:
                break
            if not args.marker and not args.watch_permissions and time.time() - t0 >= min(args.seconds, 10):
                break
            time.sleep(POLL_SECONDS)
        full = dev.adb("logcat", "-d", "-v", "threadtime", timeout=120)
        (out / f"start-{n}.log").write_text(full, encoding="utf-8")
        start["seconds"] = round(time.time() - t0, 1)
        start["alive"] = bool(dev.sh(f"pidof {package}").strip())
        if args.marker:
            start["marker"] = marker_lines(full, args.marker) or start["marker"]
        if args.startup or "AZ_STARTUP" in env:
            lines = timeline_lines(full)
            (out / f"start-{n}.timeline.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
            start["timeline_lines"] = len(lines)
        listing = dev.sh(f"ls -laR /data/data/{package}/cache 2>&1 | head -80")
        (out / f"start-{n}.cache.txt").write_text(listing, encoding="utf-8")
        crash = dev.adb("logcat", "-d", "-b", "crash", "-v", "threadtime", timeout=60)
        start["crash"] = [l for l in crash.splitlines() if package in l or "azul" in l.lower()][:3]
        start["dialogs"] = log.seen
        dialogs_any += [(n, t, d) for t, d in log.seen]
        found_any = found_any or bool(start["marker"])
        report["starts"].append(start)
    if args.marker:
        (out / "marker.txt").write_text("\n".join(l for s in report["starts"] for l in s["marker"]) + "\n", encoding="utf-8")
    report["marker_found"] = found_any if args.marker else None
    report["permission_dialogs"] = [{"start": n, "seconds": t, "what": d} for n, t, d in dialogs_any]
    (out / "probe.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    lines = [f"# android_probe: {package} on API {report['android']} {report['abi']}", ""]
    for s in report["starts"]:
        lines.append(f"- start {s['n']}: {s['seconds']} s, alive {s['alive']}, marker lines {len(s['marker'])}, permission dialogs {len(s['dialogs'])}"
                     + (f", crash {s['crash'][0][:100]}" if s["crash"] else ""))
        for line in s["marker"][:3]:
            lines.append(f"    {line[:600]}")
        for t, d in s["dialogs"]:
            lines.append(f"    DIALOG at {t} s: {d}")
    lines.append("")
    lines.append(f"marker {'FOUND' if found_any else 'NOT FOUND'}" if args.marker else "no marker asked for")
    lines.append(f"permission dialogs: {len(dialogs_any)}" + (" (watched)" if args.watch_permissions else " (not watched)"))
    (out / "probe.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    print("\n".join(lines))
    if args.watch_permissions and dialogs_any:
        return 4
    if args.marker and not found_any:
        return 3
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--apk", default="")
    ap.add_argument("--package", default="")
    ap.add_argument("--activity", default="")
    ap.add_argument("--out", required=True)
    ap.add_argument("--prop", action="append", default=[], help="system property key=value, set before the starts")
    ap.add_argument("--env", default="", help="the engine's debug.az.env: 'K=V;K2=V2'")
    ap.add_argument("--marker", default="", help="a text the log must hold (exit 3 when it never does)")
    ap.add_argument("--seconds", type=float, default=90.0)
    ap.add_argument("--starts", type=int, default=1)
    ap.add_argument("--clear-data", action="store_true")
    ap.add_argument("--watch-permissions", action="store_true")
    ap.add_argument("--startup", action="store_true", help="AZ_STARTUP=1 and keep the [startup] / [fonts] lines of every start")
    ap.add_argument("--serial", default="")
    return run_probe(ap.parse_args(argv))


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
