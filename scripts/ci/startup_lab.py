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
    # the tree's hold WITHOUT the cut of the starting window's exit (the system's own exit animation: a cross-fade, and in the night mode a fade through a window
    # with no icon and no page) against the tree, which goes in one frame
    {"name": "fade-450", "apk": "fade", "props": {"p1": 450, "p2": 330}, "resume": True},
    {"name": "tree-450-dark", "apk": "tree", "props": {"p1": 450, "p2": 330, "dark": 1}, "night": True},
    {"name": "fade-450-dark", "apk": "fade", "props": {"p1": 450, "p2": 330, "dark": 1}, "night": True},
    # an engine that is slower than the hold's limit (4 s): the window must go ahead at the limit, never stay away (the log says how it went ahead)
    {"name": "tree-5000", "apk": "tree", "props": {"p1": 5000, "p2": 0}, "settle": 9.0, "starts": 2},
    {"name": "before-450-flat", "apk": "before", "props": {"p1": 450, "p2": 330, "flat": "0xFFFFFF"}},
    {"name": "tree-450-flat", "apk": "tree", "props": {"p1": 450, "p2": 330, "flat": "0xFFFFFF"}},
    {"name": "before-450-nogeom", "apk": "before", "props": {"p1": 450, "p2": 330, "geom": 0}},
    {"name": "before-450-dark", "apk": "before", "props": {"p1": 450, "p2": 330, "dark": 1}, "night": True},
    ]


def real_rows(apps):
    """The rows of the real apps (the engine and its Java side, no stand-in): every app in the light mode and in the night mode, the engine's
    startup timeline and its info log on (`AZ_LOG=info` shows the shell's own lines), the first app's light row also home and back."""
    rows = []
    for app in apps:
        for night in (False, True):
            rows.append({"name": f"{app}-{'dark' if night else 'light'}", "apk": app, "props": {}, "env": "AZ_LOG=info", "startup": True,
                         "night": night, "resume": not night, "relaunch": True, "settle": 6.0})
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


ICON_BOX = (0.25, 0.75, 0.30, 0.70)   # (columns, rows) as shares of the area between the system bars: where the starting window's icon sits


def window_flat(sig):
    """Whether a frame is ONE colour between the system bars but for the icon in its middle: Android 12 and later put the app icon on the starting window,
    and its size depends on the release (2.3 % of the samples on Android 15, 1.6 % on 14), so the share of the most common colour of the whole frame is no
    test of a starting window. The share outside the icon's box is."""
    inner = sig[q.SYSTEM_BAR_ROWS[0] * q.COLS * 3:(q.ROWS - q.SYSTEM_BAR_ROWS[1]) * q.COLS * 3] if len(sig) == q.COLS * q.ROWS * 3 else sig
    rows = len(inner) // (q.COLS * 3)
    c0, c1 = int(q.COLS * ICON_BOX[0]) * 3, int(q.COLS * ICON_BOX[1]) * 3
    r0, r1 = int(rows * ICON_BOX[2]), int(rows * ICON_BOX[3])
    out = bytearray()
    for r in range(rows):
        row = inner[r * q.COLS * 3:(r + 1) * q.COLS * 3]
        out += row[:c0] + row[c1:] if r0 <= r < r1 else row
    return q.flat_fraction(bytes(out)) >= fa.VIDEO_FLAT_FRACTION


def label_frames(sigs, night=False):
    """One letter per frame of a recording (see the module's doc): L before the first flat frame, then S / B for the flat ones (light / dark; in the night
    mode `dark_kind` tells the dark starting window from the dark gap), and for the others 2 when the frame is the recording's last picture, 1 when it
    is not, and in the night mode W when it is a LIGHT picture (the wrong colour)."""
    if not sigs:
        return []
    final = sigs[-1]
    # the settled picture first: a page that is mostly dark (a text editor in the night mode) must not be taken for the dark starting window
    settled = [q.sig_diff(s, final) < SETTLED_DIFF for s in sigs]
    if night:
        kinds = [None if settled[i] else dark_kind(s) for i, s in enumerate(sigs)]
    else:
        kinds = [None if settled[i] else (("B" if fa.mean_luma(s) < fa.BLACK_LUMA else "S") if window_flat(s) else None) for i, s in enumerate(sigs)]
    first_flat = next((i for i, k in enumerate(kinds) if k), None)
    out = []
    for i, s in enumerate(sigs):
        if first_flat is None or i < first_flat:
            out.append("L")
        elif kinds[i]:
            out.append(kinds[i])
        elif settled[i]:
            out.append("2")
        elif night and fa.mean_luma(s) > LIGHT_PICTURE:
            out.append("W")
        else:
            out.append("1")
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
BLEND_RESIDUAL = 0.04   # a frame this close to a mix of the starting window and the settled picture is a step of the system's cross-fade (a page that moved is
                        # 0.08 from every mix, a step of the fade 0.02; the emulator's compositor can show ONE step for several recorded frames)


def blend_residual(frame, window, settled):
    """How far `frame` is from the nearest mix `a * window + (1 - a) * settled` (a share of 255, the unit of `sig_diff`): about 0 for a step of the system's
    cross-fade from the starting window into the page, large for a picture with a layout of its own (a page that moved)."""
    if not frame or len(frame) != len(window) or len(frame) != len(settled):
        return 1.0
    num = sum((x - z) * (y - z) for x, y, z in zip(frame, window, settled))
    den = sum((y - z) ** 2 for y, z in zip(window, settled))
    a = max(0.0, min(1.0, num / den)) if den else 0.0
    return sum(abs(x - (a * y + (1 - a) * z)) for x, y, z in zip(frame, window, settled)) / (255.0 * len(frame))


def stable_intermediate(labels, sigs):
    """The longest run of frames labelled `1` (a picture that is not the settled one) that look alike: a picture that STOOD before the page changed (the first
    layout's picture before a second layout moved the page). The frames of the system's exit animation are `1` too, but each differs from the next (a fade),
    so they are no run; and a step of that fade that the recording shows several times (a slow compositor) is a mix of the starting window and the settled
    picture, which a page with a layout of its own is not."""
    best, run = 0, 0
    window = None
    for i, c in enumerate(labels):
        if c == "S":
            window = sigs[i]
        # the frames of the system's own opening animation (the launcher turning into the starting window) come before the last frame of the starting window
        # and are no picture of the app
        stands = (c == "1" and "S" not in labels[i + 1:] and i + 1 < len(labels) and labels[i + 1] == "1"
                  and q.sig_diff(sigs[i], sigs[i + 1]) < STABLE_DIFF)
        if stands and window is not None and blend_residual(sigs[i], window, sigs[-1]) < BLEND_RESIDUAL:
            stands = False
        if stands:
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


GAP_PAGE_SHARE = 0.2   # a dark frame with less than this much of the settled page showing through is a window with nothing in it


def page_share(frame, settled):
    """How much of the settled picture a dark frame shows: the `b` that fits `frame = b * settled` on the luma between the system bars (0 for the black window,
    1 for the page). The night mode's starting window is black (the manifest contract pins it), so a frame is a mix of black and the page."""
    f, p = bar_free_luma(frame), bar_free_luma(settled)
    den = sum(v * v for v in p)
    if not f or len(f) != len(p) or not den:
        return 0.0
    return max(0.0, min(1.0, sum(a * b for a, b in zip(f, p)) / den))


def black_split(labels, sigs, night):
    """(gap, fade): the black frames (B) after the starting window began, told apart. In the light mode every one is a GAP: a black window where the starting
    window was. In the night mode the starting window is black itself (the engine's own dark colour), so a dark frame without the icon is either a GAP (the
    window with nothing in it: less than `GAP_PAGE_SHARE` of the page shows) or a FADE (the system's exit animation lifting the black off a page that is
    nearly black itself: the page shows through, the picture was already posted)."""
    app = next((i for i, c in enumerate(labels) if c != "L"), len(labels))
    black = [i for i in range(app, len(labels)) if labels[i] == "B"]
    if not night or sigs is None:
        return len(black), 0
    gap = sum(1 for i in black if page_share(sigs[i], sigs[-1]) < GAP_PAGE_SHARE)
    return gap, len(black) - gap


LAUNCHER_DIFF = 0.08   # a frame this close to the recording's first frame (the home screen) is the launcher


def launcher_frames(labels, sigs):
    """How many frames show the LAUNCHER again after the starting window began: the system took the window away before the app had a picture and the home
    screen shows through (the E60 hold's 2 to 4 frames between the white window and the black). `L` frames before the window are the opening animation."""
    if sigs is None or not sigs:
        return 0
    first = next((i for i, c in enumerate(labels) if c in "SB"), None)
    if first is None:
        return 0
    return sum(1 for i in range(first, len(labels)) if q.sig_diff(sigs[i], sigs[0]) < LAUNCHER_DIFF)


def summarize_start(labels, sigs=None, night=False):
    """{splash, black, gap, fade, first, settled, jump, moved, wrong}: the frames of the starting window (S), of black (B) from the first frame of the app phase
    on (and of those, the `gap` with nothing showing and the `fade` of a page that shows through, see `black_split`), of pictures shown before the settled one
    (the exit animation's fade included), of the settled one, whether a picture STOOD before the page moved (`moved` frames of it; needs the signatures,
    else any `1` before a `2` counts), and the light pictures of a night-mode start (W: the wrong colour)."""
    app = next((i for i, c in enumerate(labels) if c != "L"), len(labels))
    moved = stable_intermediate(labels, sigs) if sigs is not None else None
    gap, fade = black_split(labels, sigs, night)
    return {
        "splash": labels[app:].count("S"),
        "black": labels[app:].count("B"),
        "gap": gap,
        "fade": fade,
        "launcher": launcher_frames(labels, sigs),
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
    m = re.search(r"Displayed \S+(?: for user \d+)?: \+(?:(\d+)s)?(\d+)ms", text or "")
    return int(m.group(1) or 0) * 1000 + int(m.group(2)) if m else None


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
        "gap_median": median([s.get("gap", s["black"]) for s in sums]),
        "gap_max": max((s.get("gap", s["black"]) for s in sums), default=None),
        "launcher_max": max((s.get("launcher", 0) for s in sums), default=None),
        "first_median": median([s["first"] for s in sums]),
        "wrong_max": max((s.get("wrong", 0) for s in sums), default=None),
        "jump": any(s["jump"] for s in sums),
    }


def relaunch_text(relaunch):
    """The line of a relaunch in the report: what was seen, and whether the process was the same one (it never is: the process ends with its activity)."""
    summary = relaunch.get("summary") or {}
    same = "the same process" if relaunch.get("same_process") else "a new process"
    if relaunch.get("delivered_to_top"):
        same += ", BUT the start was delivered to the activity that was there (a resume, not a relaunch)"
    return (f"relaunch (the task is cleared and the app started again; {same}): `{relaunch.get('runs', '?')}`; black {summary.get('black', '?')}"
            f", starting window {summary.get('splash', '?')} frames, wrong colour {summary.get('wrong', '?')}"
            + (f"; Displayed +{relaunch['displayed']} ms" if relaunch.get("displayed") else ""))


def report_text(results):
    """lab.md: one table, one row per lab row, then the first start's run lengths and events of every row."""
    lines = ["# Cold-start lab", "",
             "frames at 30 fps; S = the starting window (or a flat first frame), B = black after it, 1 = a first picture that moved afterwards, 2 = the settled picture, W = a light picture in the night mode",
             "", "| row | apk | knobs | starts | starting window (frames) | black (median / worst) | of which a window with nothing in it (median / worst) | first picture (frames) | page moved | wrong colour (worst) | launcher shows through (worst) |", "|---|---|---|---|---|---|---|---|---|---|---|"]
    for r in results:
        v = r["verdict"]
        knobs = ", ".join(f"{k}={val}" for k, val in r["props"].items()) + (", night" if r.get("night") else "")
        lines.append(f"| {r['name']} | {r['apk']} | {knobs} | {v['starts']} | {v['splash_median']} | {v['black_median']} / {v['black_max']} | {v.get('gap_median', v['black_median'])} / {v.get('gap_max', v['black_max'])} | {v['first_median']} | {'yes' if v['jump'] else 'no'} | {v.get('wrong_max')} | {v.get('launcher_max', 0)} |")
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
            if s.get("relaunch"):
                lines.append("    - " + relaunch_text(s["relaunch"]))
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


def one_start(dev, package, activity, row, out_dir, keep_resume, relaunch=False):
    """One cold start (and, when the row asks, the resume) -> the start's record. The recording of every cold start is kept (a few hundred KB: a number
    that says a page moved is looked at in the frames); the resume's only when `keep_resume`."""
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
        record.update({"runs": runs_text(labels), "summary": summarize_start(labels, sigs, night=bool(row.get("night"))), "frames": len(frames)})
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
        if not keep_resume:
            (out_dir / "resume.mp4").unlink(missing_ok=True)
    if relaunch and row.get("relaunch"):
        record["relaunch"] = relaunch_start(dev, package, activity, row, out_dir)
    return record


def relaunch_start(dev, package, activity, row, out_dir):
    """A RELAUNCH: the app is sent home and started again with FLAG_ACTIVITY_NEW_TASK | FLAG_ACTIVITY_CLEAR_TASK, which finishes the old activity and makes
    a new one in the same task. It is not a warm start: the process ends with its activity (`AzulActivity.onDestroy`, libazul is not re-entrant), so the new
    activity is in a NEW process whatever the flags, and the system makes no starting window for it (the old activity's window is replaced in its task):
    the window's own colour shows until the first picture (white or black, the engine's default for the mode, which is the page's base colour). Recorded
    like a cold start, as information about that path. (The developer setting "Don't keep activities", `always_finish_activities`, does not destroy the
    activity of a NativeActivity: the start was delivered to the one that was there, `result code=2`, which is a resume.)
    -> {runs, summary, same_process, ...}."""
    dev.sh("input keyevent KEYCODE_HOME")
    time.sleep(2.5)
    before = dev.sh(f"pidof {package}").strip()
    dev.adb("logcat", "-c")
    proc, remote = dev.record_start()
    dev.sh(f"am start -n {package}/{activity} -f 0x10008000")
    dev.wait_front(package)
    time.sleep(row.get("settle", 3.5))
    video = out_dir / "relaunch.mp4"
    dev.record_stop(proc, remote, video)
    after = dev.sh(f"pidof {package}").strip()
    log = dev.adb("logcat", "-d", "-v", "threadtime", timeout=120)
    (out_dir / "relaunch-logcat.txt").write_text(log)
    # the system's own word: `result code=0` is a new activity, 2 is a start delivered to the one that was there (a resume, not a relaunch)
    delivered_to_top = "result code=2" in log
    result = {"same_process": bool(before) and before == after, "delivered_to_top": delivered_to_top, "displayed": displayed_ms(log), "events": log_events(log)}
    frames = fa.cut_frames(video)
    if frames:
        sigs = [fa.signature_of(f) for f in frames]
        labels = label_frames(sigs, night=bool(row.get("night")))
        result.update({"runs": runs_text(labels), "summary": summarize_start(labels, sigs, night=bool(row.get("night")))})
    return result


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
            rec = one_start(dev, package, activity, row, out / row["name"] / f"start{i + 1}", keep_resume=(i == 0), relaunch=(i < 3))
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
