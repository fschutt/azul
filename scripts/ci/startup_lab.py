#!/usr/bin/env python3
"""startup_lab.py: the cold-start lab of the Android shell (ledger E46 / E47 / E60): what a person SEES between the system's starting
window and the first picture, per variant of the Java side and per setting of the stand-in engine, frame by frame.

    startup_lab.py run --out DIR [--serial S] --apk NAME=PATH [--apk NAME=PATH ...] [--rows rows.json | --real] [--starts N] [--only TEXT]
    startup_lab.py report DIR                 (the table of the last run again)

It drives an emulator that is already booted (scripts/ci/android_startup_lab.sh boots it and builds the control APKs of scripts/startup-control;
scripts/ci/android_startup_real.sh builds the real apps and runs `--real`: every app in the light mode and in the night mode, the engine's startup
timeline and its info log on). Per row (an APK, the stand-in's knobs `debug.azctl.*`, light or night) and per start: the app is force-stopped, the
home screen is shown, the screen is RECORDED (adb screenrecord, as scripts/ci/flicker_audit.py records it), the app is started, left for a few
seconds, the recording is cut into frames at 30 fps (ffmpeg) and every frame gets a letter:

    L  before the app: the launcher and the system's opening animation (every frame until the first flat one)
    S  the system's starting window (the app icon on the window colour: one light colour between the system bars; in the night mode a dark frame with the
       icon on it), or a flat first frame
    B  one dark colour with nothing on it (black): the window is up and has nothing in it
    1  a picture of the app that is not the one the recording ends with (the first picture when a second one follows, and the frames of the system's exit
       animation, which fade into the page)
    2  the picture the recording ends with (the settled one)
    W  a light picture in the night mode: the wrong colour

and the run lengths are the verdict: `L63 S14 B33 1x5 2x30` is 14 frames (0.47 s) of the starting window, 33 frames (1.1 s) of black, 5 frames
of the first picture, then the settled one. A page that MOVED is told apart from a fade: a run of `1` frames that look alike is a picture that stood
before the page changed (`moved`). The log of each start (the system's `Displayed`, the stand-in's `[azctl]` lines, the engine's own) is cut to the
lines that say WHEN, relative to the system's `START`. Home and back (the resume) is recorded too when the row asks.

The analysis functions are pure (tests: scripts/ci/test_startup_lab.py).
"""
import argparse
import json
import re
import statistics
import subprocess
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import flicker_audit as fa  # noqa: E402
import mobile_qa as q  # noqa: E402

FPS = fa.FPS
SETTLED_DIFF = 0.02          # a frame this close to the recording's last frame is "the settled picture"

# The rows of a default lab: an Azul app's cold start as the system sees it, with the stand-in engine's first layout at 450 ms (a big first screen),
# a Resize pass of 330 ms after it, and the Java side as it was (`before`: the hold with the exit listener), with no hold at all (`none`) and as it is
# in the tree (`tree`). The package of an APK is com.azul.startctl.<name>.
DEFAULT_ROWS = [
    {"name": "before-450", "apk": "before", "props": {"p1": 450, "p2": 330}, "resume": True},
    {"name": "none-450", "apk": "none", "props": {"p1": 450, "p2": 330}},
    {"name": "tree-450", "apk": "tree", "props": {"p1": 450, "p2": 330}, "resume": True},
    {"name": "before-100", "apk": "before", "props": {"p1": 100, "p2": 330}},
    {"name": "tree-100", "apk": "tree", "props": {"p1": 100, "p2": 330}},
    {"name": "before-1000", "apk": "before", "props": {"p1": 1000, "p2": 330}},
    {"name": "tree-1000", "apk": "tree", "props": {"p1": 1000, "p2": 330}},
    {"name": "before-450-flat", "apk": "before", "props": {"p1": 450, "p2": 330, "flat": "0xFFFFFF"}},
    {"name": "tree-450-flat", "apk": "tree", "props": {"p1": 450, "p2": 330, "flat": "0xFFFFFF"}},
    {"name": "before-450-nogeom", "apk": "before", "props": {"p1": 450, "p2": 330, "geom": 0}},
    {"name": "before-450-dark", "apk": "before", "props": {"p1": 450, "p2": 330, "dark": 1}, "night": True},
    {"name": "tree-450-dark", "apk": "tree", "props": {"p1": 450, "p2": 330, "dark": 1}, "night": True},
]


def real_rows(apps):
    """The rows of the real apps (the engine and its Java side, no stand-in): every app in the light mode and in the night mode, the engine's
    startup timeline and its info log on (`AZ_LOG=info` shows the shell's own lines), the first app's light row also home and back."""
    rows = []
    for app in apps:
        for night in (False, True):
            rows.append({"name": f"{app}-{'dark' if night else 'light'}", "apk": app, "props": {}, "env": "AZ_LOG=info", "startup": True,
                         "night": night, "resume": not night, "settle": 6.0})
    return rows


# ---- pure analysis ---------------------------------------------------------------------------------------------------------------------

DARK_MEAN = 30           # a frame of a night-mode recording whose mean luma (of 255) between the system bars is under this is a dark frame ...
DARK_BRIGHT_SHARE = 0.06  # ... when no more than this share of its samples is brighter than DARK_MEAN: the dark window, with at most an icon on it
ICON_PEAK = 60            # the icon of the starting window has samples brighter than this; a window with nothing in it has none
LIGHT_PICTURE = 140       # a picture of the night mode whose mean luma is over this is the WRONG colour (a light first picture on a dark phone)


def bar_free_luma(sig):
    """The luma (0..255) of every sample of a signature between the system bars."""
    inner = sig[q.SYSTEM_BAR_ROWS[0] * q.COLS * 3:(q.ROWS - q.SYSTEM_BAR_ROWS[1]) * q.COLS * 3] if len(sig) == q.COLS * q.ROWS * 3 else sig
    return [0.299 * inner[i] + 0.587 * inner[i + 1] + 0.114 * inner[i + 2] for i in range(0, len(inner) - 2, 3)]


def dark_kind(sig):
    """S, B or None for a frame of a night-mode recording: a dark window with an icon on it (the starting window), a dark window with nothing on it (black),
    or neither (a picture, or the launcher). A dark starting window is no more one flat colour than a light one is: the icon is a few percent of it."""
    luma = bar_free_luma(sig)
    if not luma:
        return None
    mean = sum(luma) / len(luma)
    bright = sum(1 for v in luma if v > DARK_MEAN) / len(luma)
    if mean < DARK_MEAN and bright <= DARK_BRIGHT_SHARE:
        return "S" if max(luma) > ICON_PEAK else "B"
    return None


def label_frames(sigs, night=False):
    """One letter per frame of a recording (see the module's doc): L before the first flat frame, then S / B for the flat ones (light / dark; in the night
    mode `dark_kind` tells the dark starting window from the dark gap), and for the others 2 when the frame is the recording's last picture, 1 when it
    is not, and in the night mode W when it is a LIGHT picture (the wrong colour)."""
    if not sigs:
        return []
    if night:
        kinds = [dark_kind(s) for s in sigs]
    else:
        kinds = [(("B" if fa.mean_luma(s) < fa.BLACK_LUMA else "S") if fa.flat_frame(s) else None) for s in sigs]
    first_flat = next((i for i, k in enumerate(kinds) if k), None)
    final = sigs[-1]
    out = []
    for i, s in enumerate(sigs):
        if first_flat is None or i < first_flat:
            out.append("L")
        elif kinds[i]:
            out.append(kinds[i])
        elif night and fa.mean_luma(s) > LIGHT_PICTURE:
            out.append("W")
        else:
            out.append("2" if q.sig_diff(s, final) < SETTLED_DIFF else "1")
    return out


def run_lengths(labels):
    """'LLSSB' -> [('L', 2), ('S', 2), ('B', 1)]."""
    runs = []
    for c in labels:
        if runs and runs[-1][0] == c:
            runs[-1][1] += 1
        else:
            runs.append([c, 1])
    return [(c, n) for c, n in runs]


STABLE_DIFF = 0.01       # two frames of a still picture differ less than this; the frames of a fade (the system's exit animation) differ more
MOVED_FRAMES = 3        # a picture that stood this many frames and was not the settled one is a page that MOVED


def stable_intermediate(labels, sigs):
    """The longest run of frames labelled `1` (a picture that is not the settled one) that look alike: a picture that STOOD before the page changed (the first
    layout's picture before a second layout moved the page). The frames of the system's exit animation are `1` too, but each differs from the next (a fade),
    so they are no run."""
    best, run = 0, 0
    for i, c in enumerate(labels):
        if c == "1" and i + 1 < len(labels) and labels[i + 1] == "1" and q.sig_diff(sigs[i], sigs[i + 1]) < STABLE_DIFF:
            run += 1
            best = max(best, run + 1)
        else:
            run = 0
    return best


def runs_text(labels):
    """The run lengths as the lab prints them: `L63 S14 B33 1x5 2x30` (a digit label is written `1x5` so that it reads as a letter and a count)."""
    return " ".join(f"{c}{'x' if c.isdigit() else ''}{n}" for c, n in run_lengths(labels))


def count_after_first(labels, letter, start="S"):
    """How many frames of `letter` follow the first frame of `start` (the black AFTER the starting window, not one before it)."""
    try:
        first = labels.index(start)
    except ValueError:
        return 0
    return sum(1 for c in labels[first:] if c == letter)


def summarize_start(labels, sigs=None):
    """{splash, black, first, settled, jump, moved, wrong}: the frames of the starting window (S), of black (B) from the first frame of the app phase on, of
    pictures shown before the settled one (the exit animation's fade included), of the settled one, whether a picture STOOD before the page moved (`moved`
    frames of it; needs the signatures, else any `1` before a `2` counts), and the light pictures of a night-mode start (W: the wrong colour)."""
    app = next((i for i, c in enumerate(labels) if c != "L"), len(labels))
    moved = stable_intermediate(labels, sigs) if sigs is not None else None
    return {
        "splash": labels[app:].count("S"),
        "black": labels[app:].count("B"),
        "first": labels.count("1"),
        "settled": labels.count("2"),
        "jump": (moved >= MOVED_FRAMES) if moved is not None else ("1" in labels and "2" in labels),
        "moved": moved,
        "wrong": labels.count("W"),
    }


THREADTIME = re.compile(r"^(\d\d)-(\d\d) (\d\d):(\d\d):(\d\d)\.(\d{3})\s+\d+\s+\d+\s+([VDIWEF])\s+(\S+)\s*:\s?(.*)$")


def logcat_ms(line):
    """The time of a `logcat -v threadtime` line in milliseconds of its day, or None."""
    m = THREADTIME.match(line)
    if not m:
        return None
    _mo, _d, hh, mm, ss, ms = (int(m.group(i)) for i in range(1, 7))
    return ((hh * 60 + mm) * 60 + ss) * 1000 + ms


# The lines of a start's log that say WHEN something happened to the window: (a label, a regex on the whole line).
EVENTS = [
    ("START", re.compile(r"ActivityTaskManager: START u0 .*cmp=")),
    ("starting window", re.compile(r"SplashScreenView: Build")),
    ("Displayed", re.compile(r"ActivityTaskManager: Displayed ")),
    ("splash exiting", re.compile(r"Splash Screen .* EXITING")),
    ("surface", re.compile(r"\[azctl\] InitWindow|android:surface_created|\[Android\] InitWindow")),
    ("flat posted", re.compile(r"\[azctl\] POSTED flat")),
    ("page1 posted", re.compile(r"\[azctl\] POSTED page1|first_present ")),
    ("page2 posted", re.compile(r"\[azctl\] POSTED page2")),
    ("first draw released", re.compile(r"AzulActivity: the window's first draw goes ahead")),
    ("redraw needed", re.compile(r"\[azctl\] RedrawNeeded")),
    ("retained posted", re.compile(r"\[azctl\] POSTED retained")),
    ("insets applied", re.compile(r"\[Android\] insets top=")),
    ("insets known at the surface", re.compile(r"the window's first insets are known at the surface")),
    ("no insets at the surface", re.compile(r"no insets yet at the surface")),
    ("layout 1 begins", re.compile(r"layout_begin\b.*#1\b")),
    ("layout 2 begins", re.compile(r"layout_begin\b.*#2\b")),
    ("layout 3 begins", re.compile(r"layout_begin\b.*#3\b")),
]


def log_events(text):
    """[(label, ms after START, line)] for the lines of EVENTS in a `logcat -v threadtime` dump, the first of each label; START is the first
    `ActivityTaskManager: START` line (0 when there is none: the times are then of the day)."""
    found = {}
    start = None
    for line in (text or "").splitlines():
        t = logcat_ms(line)
        if t is None:
            continue
        for label, pattern in EVENTS:
            if pattern.search(line):
                if label == "START" and start is None:
                    start = t
                found.setdefault(label, (t, line.strip()))
    base = start if start is not None else 0
    return sorted(((label, t - base, line) for label, (t, line) in found.items()), key=lambda e: e[1])


def displayed_ms(text):
    """The `+NNNms` of the system's `Displayed` line, or None."""
    m = re.search(r"Displayed \S+(?: for user \d+)?: \+(\d+)ms", text or "")
    return int(m.group(1)) if m else None


def median(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def row_verdict(starts):
    """The row's numbers over its starts: the median and the worst of the black frames, the median starting window and first-picture frames."""
    sums = [s["summary"] for s in starts if s.get("summary")]
    blacks = [s["black"] for s in sums]
    return {
        "starts": len(sums),
        "splash_median": median([s["splash"] for s in sums]),
        "black_median": median(blacks),
        "black_max": max(blacks) if blacks else None,
        "first_median": median([s["first"] for s in sums]),
        "wrong_max": max((s.get("wrong", 0) for s in sums), default=None),
        "jump": any(s["jump"] for s in sums),
    }


def report_text(results):
    """lab.md: one table, one row per lab row, then the first start's run lengths and events of every row."""
    lines = ["# Cold-start lab", "",
             "frames at 30 fps; S = the starting window (or a flat first frame), B = black after it, 1 = a first picture that moved afterwards, 2 = the settled picture, W = a light picture in the night mode",
             "", "| row | apk | knobs | starts | starting window (frames) | black (median / worst) | first picture (frames) | page moved | wrong colour (worst) |", "|---|---|---|---|---|---|---|---|---|"]
    for r in results:
        v = r["verdict"]
        knobs = ", ".join(f"{k}={val}" for k, val in r["props"].items()) + (", night" if r.get("night") else "")
        lines.append(f"| {r['name']} | {r['apk']} | {knobs} | {v['starts']} | {v['splash_median']} | {v['black_median']} / {v['black_max']} | {v['first_median']} | {'yes' if v['jump'] else 'no'} | {v.get('wrong_max')} |")
    lines.append("")
    for r in results:
        lines.append(f"## {r['name']}")
        for i, s in enumerate(r["starts"]):
            lines.append(f"- start {i + 1}: `{s.get('runs', '?')}`" + (f"; Displayed +{s['displayed']} ms" if s.get("displayed") else ""))
            if i == 0:
                for label, ms, line in s.get("events", []):
                    lines.append(f"    - +{ms} ms {label}: `{line[:170]}`")
                if s.get("resume"):
                    lines.append(f"    - resume (home and back): `{s['resume']}`")
        lines.append("")
    return "\n".join(lines)


# ---- the device --------------------------------------------------------------------------------------------------------------------------

class Device:
    def __init__(self, serial=""):
        self.serial = serial

    def adb(self, *args, timeout=90, binary=False):
        cmd = ["adb"] + (["-s", self.serial] if self.serial else []) + list(args)
        try:
            done = subprocess.run(cmd, capture_output=True, timeout=timeout)
        except subprocess.TimeoutExpired:
            return b"" if binary else ""
        return done.stdout if binary else done.stdout.decode("utf-8", "replace")

    def sh(self, command, timeout=90):
        return self.adb("shell", command, timeout=timeout)

    def wait_front(self, package, timeout=25.0):
        end = time.time() + timeout
        while time.time() < end:
            if package in self.sh("dumpsys window | grep -E 'mCurrentFocus|mFocusedApp'"):
                return True
            time.sleep(0.25)
        return False

    def record_start(self, remote="/sdcard/lab.mp4"):
        self.sh(f"rm -f {remote}")
        proc = subprocess.Popen(["adb"] + (["-s", self.serial] if self.serial else []) +
                                ["shell", f"screenrecord --size {fa.FRAME_W * 5}x{fa.FRAME_H * 5} --bit-rate 6000000 --time-limit 120 {remote}"],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(1.5)
        return proc, remote

    def record_stop(self, proc, remote, local):
        self.sh("pkill -SIGINT screenrecord")
        time.sleep(2.5)
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.terminate()
        self.adb("pull", remote, str(local), timeout=120)


def set_props(dev, row):
    """The stand-in's knobs (`debug.azctl.*`) of a control row, and the engine's own (`debug.az.env`, the startup timeline) of a real app's."""
    props = row.get("props", {})
    for key in ("p1", "p2", "flat", "geom", "sig", "shift", "dark"):
        dev.sh(f"setprop debug.azctl.{key} ''" if key not in props else f"setprop debug.azctl.{key} {props[key]}")
    dev.sh(f"setprop debug.az.env '{row.get('env', '')}'")
    dev.sh(f"setprop debug.az.startup {1 if row.get('startup') else 0}")


def one_start(dev, package, activity, row, out_dir, keep_video):
    """One cold start (and, when the row asks, the resume) -> the start's record."""
    out_dir.mkdir(parents=True, exist_ok=True)
    dev.sh(f"am force-stop {package}")
    dev.sh("input keyevent KEYCODE_HOME")
    set_props(dev, row)
    dev.sh(f"cmd uimode night {'yes' if row.get('night') else 'no'}")
    time.sleep(2.0)
    dev.adb("logcat", "-c")
    proc, remote = dev.record_start()
    dev.sh(f"am start -n {package}/{activity}")
    dev.wait_front(package)
    time.sleep(row.get("settle", 3.5))
    video = out_dir / "cold.mp4"
    dev.record_stop(proc, remote, video)
    log = dev.adb("logcat", "-d", "-v", "threadtime", timeout=120)
    (out_dir / "logcat.txt").write_text(log)
    record = {"displayed": displayed_ms(log), "events": log_events(log)}
    frames = fa.cut_frames(video)
    if frames:
        sigs = [fa.signature_of(f) for f in frames]
        labels = label_frames(sigs, night=bool(row.get("night")))
        record.update({"runs": runs_text(labels), "summary": summarize_start(labels, sigs), "frames": len(frames)})
        record["_frames"] = frames
        record["_labels"] = labels
    if row.get("resume"):
        proc, remote = dev.record_start()
        dev.sh("input keyevent KEYCODE_HOME")
        time.sleep(1.5)
        dev.sh(f"am start -n {package}/{activity}")
        time.sleep(3.0)
        rvideo = out_dir / "resume.mp4"
        dev.record_stop(proc, remote, rvideo)
        rframes = fa.cut_frames(rvideo)
        if rframes:
            rsigs = [fa.signature_of(f) for f in rframes]
            runs = fa.flat_runs(rsigs)
            record["resume"] = "; ".join(f"{'BLACK' if fa.mean_luma(rsigs[a + (b - a) // 2]) < fa.BLACK_LUMA else 'flat'} {b - a} frames at {a}" for a, b in runs) or "no flat frame"
        if not keep_video:
            (out_dir / "resume.mp4").unlink(missing_ok=True)
    if not keep_video:
        video.unlink(missing_ok=True)
    return record


def write_strip_of(path, record):
    """A contact sheet of the app phase of a start: every 2nd frame from the first flat one on, 14 to a row."""
    frames, labels = record.get("_frames"), record.get("_labels")
    if not frames or not labels:
        return
    first = next((i for i, c in enumerate(labels) if c != "L"), 0)
    picks = list(range(max(0, first - 2), len(frames), 2))[:84]
    rows = [picks[i:i + 14] for i in range(0, len(picks), 14)]
    fa.write_strip(path, frames, rows)


def run(args):
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    dev = Device(args.serial)
    apks = dict(spec.split("=", 1) for spec in args.apk)
    rows = json.loads(Path(args.rows).read_text()) if args.rows else (real_rows(list(apks)) if args.real else DEFAULT_ROWS)
    if args.only:
        rows = [r for r in rows if any(o in r["name"] for o in args.only)]
    installed = set()
    results = []
    for row in rows:
        name = row["apk"]
        if name not in apks:
            print(f"row {row['name']}: no APK named {name}, skipped")
            continue
        package, activity = fa.apk_identity(apks[name])
        package = package or f"com.azul.startctl.{name}"
        activity = activity or "com.azul.app.AzulActivity"
        if name not in installed:
            print(dev.adb("install", "-r", "-t", apks[name], timeout=300).strip())
            dev.sh("settings put system accelerometer_rotation 0")
            installed.add(name)
        starts = []
        for i in range(args.starts if not row.get("starts") else row["starts"]):
            rec = one_start(dev, package, activity, row, out / row["name"] / f"start{i + 1}", keep_video=(i == 0))
            if i == 0:
                write_strip_of(out / row["name"] / "strip.png", rec)
            rec.pop("_frames", None)
            rec.pop("_labels", None)
            starts.append(rec)
            print(f"{row['name']} start {i + 1}: {rec.get('runs', '?')}  displayed {rec.get('displayed')}", flush=True)
        results.append({**row, "starts": starts, "verdict": row_verdict(starts)})
        (out / "lab.json").write_text(json.dumps(results, indent=1, default=str))
    (out / "lab.md").write_text(report_text(results))
    print((out / "lab.md").read_text())
    return 0


def report(args):
    results = json.loads((Path(args.dir) / "lab.json").read_text())
    print(report_text(results))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--out", required=True)
    r.add_argument("--serial", default="")
    r.add_argument("--apk", action="append", required=True, help="NAME=PATH, NAME is the row's `apk`")
    r.add_argument("--rows", default="")
    r.add_argument("--real", action="store_true", help="the rows of the real apps named by --apk (light and night), not DEFAULT_ROWS")
    r.add_argument("--starts", type=int, default=4)
    r.add_argument("--only", action="append", default=[], help="only the rows whose name holds this text")
    r.set_defaults(func=run)
    p = sub.add_parser("report")
    p.add_argument("dir")
    p.set_defaults(func=report)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
