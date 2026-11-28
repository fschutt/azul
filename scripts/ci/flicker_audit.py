#!/usr/bin/env python3
"""flicker_audit.py: idle redraws, flash frames and pan jitter of a phone app (standard library only; ffmpeg for the video).

    flicker_audit.py android --out DIR --apk-plain AzWidgets.apk [--apk-e2e AzWidgets-e2e.apk] [--maps AzMaps.apk] [--serial S]

Three questions, one report (DIR/flicker.md, flicker.json, strips/*.png):
  1. IDLE: after the first frame, with nothing touching the app, does the engine present again? (`[Android] present` lines of AZ_DUMP_PRESENT
     over 3 s, and five screenshots 0.6 s apart that must be identical). The only allowed exception is a blinking caret: a few small partial
     presents. The report prints the count, the rows each one wrote and the intervals, so a caret and a full redraw are told apart.
  2. FLASH FRAMES: the screen is RECORDED (adb screenrecord) over a cold start, a rotation there and back, the dark mode on and off, the
     keyboard in and out, home and back, and the video is cut into frames (ffmpeg). Flagged: a frame of one flat colour between two normal
     ones, a frame that differs from both neighbours while the neighbours match (a one-frame flash), and a state shown twice out of order
     (A B A B). Every flagged frame is kept in a strip with its neighbours.
  3. PAN JITTER: a scripted finger pan (adb input swipe, the real finger path) on a long page and on the map: the present intervals during
     the pan (p50 / p95 / max), and the scroll offset, sampled through the debug server while it moves: it must advance without snapping back.

The analysis functions are pure (tests: scripts/ci/test_flicker_audit.py).
"""
import argparse
import json
import re
import shutil
import statistics
import subprocess
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mobile_qa as q  # noqa: E402

FRAME_W, FRAME_H = 108, 240           # the size a video is cut to for the analysis and the strips
FPS = 30                              # the rate the video is resampled to (a screenrecord of a static screen has few frames)
VIDEO_FLAT_FRACTION = 0.98              # a frame of a recording that is this much one colour between the system bars is flat
FLASH_DIFF = 0.06                     # a frame this far from both neighbours ...
NEIGHBOURS_ALIKE = 0.02               # ... while the neighbours are this close to each other is a flash
SAME_STATE = 0.02
IDLE_PRESENT_LIMIT = 6                # a blinking caret at 2 Hz over 3 s
CARET_ROWS_SHARE = 0.15               # a caret's partial present writes at most this share of the rows
PRESENT_LINE = re.compile(r"\[\s*(\d+)us\].*\[Android\] present (full|partial)\s*[-:]\s*rows (\d+)\.\.(\d+) of (\d+)")


# ---- pure analysis ----------------------------------------------------------------------------------------------------------------------

def present_events(text, window=None):
    """The engine's `[Android] present full|partial - rows a..b of n` lines of a logcat dump -> [{us, kind, rows, share}]; `window` = (from_us,
    to_us) of the engine's own clock keeps those inside."""
    found = []
    for line in (text or "").splitlines():
        m = PRESENT_LINE.search(line)
        if not m:
            continue
        us, kind, a, b, n = int(m.group(1)), m.group(2), int(m.group(3)), int(m.group(4)), int(m.group(5))
        if window and not (window[0] <= us <= window[1]):
            continue
        found.append({"us": us, "kind": kind, "rows": b - a, "share": (b - a) / n if n else 1.0})
    return found


def intervals_ms(events):
    return [(b["us"] - a["us"]) / 1000.0 for a, b in zip(events, events[1:])]


def percentile(values, p):
    v = sorted(values)
    if not v:
        return None
    k = (len(v) - 1) * p
    lo, hi = int(k), min(int(k) + 1, len(v) - 1)
    return v[lo] + (v[hi] - v[lo]) * (k - lo)


def frame_time_stats(events):
    """p50 / p95 / max of the intervals between presents, and their count."""
    iv = intervals_ms(events)
    if not iv:
        return None
    return {"n": len(iv), "p50": round(percentile(iv, 0.5), 1), "p95": round(percentile(iv, 0.95), 1), "max": round(max(iv), 1)}


def idle_verdict(events, screenshot_diffs):
    """-> (ok, text). Idle means no present at all, or only a caret: at most IDLE_PRESENT_LIMIT partial presents that each write a small share
    of the rows; and the screenshots must match."""
    big = [e for e in events if e["kind"] == "full" or e["share"] > CARET_ROWS_SHARE]
    worst = max(screenshot_diffs) if screenshot_diffs else 0.0
    parts = [f"{len(events)} presents in 3 s"]
    if events:
        parts.append("rows written: " + ", ".join(f"{e['kind']} {e['rows']}" for e in events[:8]) + (" ..." if len(events) > 8 else ""))
        iv = intervals_ms(events)
        if iv:
            parts.append(f"interval median {statistics.median(iv):.0f} ms")
    parts.append(f"screenshots differ at most {worst:.3f}")
    ok = (not events or (len(events) <= IDLE_PRESENT_LIMIT and not big)) and worst < q.SAME_BELOW
    if events and not ok and big:
        parts.append(f"{len(big)} of them are full or write more than {CARET_ROWS_SHARE * 100:.0f} % of the rows: a redraw, not a caret")
    return ok, "; ".join(parts)


def split_frames(raw, frame_bytes):
    """The bytes of a rawvideo stream -> the list of frames."""
    return [raw[i:i + frame_bytes] for i in range(0, len(raw) - frame_bytes + 1, frame_bytes)]


def signature_of(frame, width=FRAME_W, height=FRAME_H):
    """A 40 x 80 signature of a width x height RGB24 frame (the sample grid of mobile_qa, 3 bytes a pixel)."""
    xs, ys = q.sample_grid(width, height)
    out = bytearray()
    for y in ys:
        base = y * width * 3
        for x in xs:
            out += frame[base + x * 3:base + x * 3 + 3]
    return bytes(out)


def runs_of(sigs, same=SAME_STATE):
    """[(start, end_exclusive)] runs of consecutive frames that show the same state (each compared with the run's first frame)."""
    runs, start = [], 0
    for i in range(1, len(sigs)):
        if q.sig_diff(sigs[i], sigs[start]) >= same:
            runs.append((start, i))
            start = i
    if sigs:
        runs.append((start, len(sigs)))
    return runs


def flat_frame(sig):
    """A frame of a recording that is (nearly) one colour between the system bars: a fade into black is 99.5-99.8 % of it, not all."""
    return q.flat_fraction(sig) >= VIDEO_FLAT_FRACTION


def flat_runs(sigs):
    """[(start, end_exclusive)] maximal runs of flat frames with a normal frame on BOTH sides (a recording that starts or ends in one
    colour has no 'between')."""
    runs, i, n = [], 0, len(sigs)
    while i < n:
        if not flat_frame(sigs[i]):
            i += 1
            continue
        j = i
        while j < n and flat_frame(sigs[j]):
            j += 1
        if i > 0 and j < n:
            runs.append((i, j))
        i = j
    return runs


def flag_frames(sigs):
    """[{index, kind, detail, frames?}]: the frames of a recording that flicker. kinds: `flat` (a run of frames of one colour between two
    normal frames: `frames` long, from `index`), `flash` (a frame unlike both neighbours that match each other) and `out-of-order` (a
    state A shown again after B and then B again: A B A B)."""
    flagged = []
    n = len(sigs)
    flat = set()
    for start, end in flat_runs(sigs):
        length = end - start
        flagged.append({"index": start, "kind": "flat", "frames": length,
                        "detail": f"{length} frame(s) ({length / FPS:.2f} s) of one flat colour between two normal frames"})
        flat.update(range(start, end))
    for i in range(1, n - 1):
        if i in flat:
            continue
        before, here, after = sigs[i - 1], sigs[i], sigs[i + 1]
        d1, d2, around = q.sig_diff(here, before), q.sig_diff(here, after), q.sig_diff(before, after)
        if d1 > FLASH_DIFF and d2 > FLASH_DIFF and around < NEIGHBOURS_ALIKE:
            flagged.append({"index": i, "kind": "flash", "detail": f"differs {d1:.2f} / {d2:.2f} from its neighbours, which match ({around:.3f})"})
    runs = runs_of(sigs)
    for k in range(len(runs) - 3):
        a, b, a2, b2 = (sigs[runs[k + j][0]] for j in range(4))
        if q.sig_diff(a, a2) < SAME_STATE and q.sig_diff(b, b2) < SAME_STATE and q.sig_diff(a, b) > FLASH_DIFF:
            flagged.append({"index": runs[k + 2][0], "kind": "out-of-order", "detail": f"the state of frame {runs[k][0]} is shown again at {runs[k + 2][0]} after another one, and that one again at {runs[k + 3][0]}"})
    seen, unique = set(), []
    for f in sorted(flagged, key=lambda f: f["index"]):
        if (f["index"], f["kind"]) not in seen:
            seen.add((f["index"], f["kind"]))
            unique.append(f)
    return unique


def monotonic(samples, direction=1, tolerance=1.0):
    """samples = [offset, ...] read while a pan moves the page: does the offset go the way of the pan without snapping back? -> (ok, worst regress)."""
    worst = 0.0
    for a, b in zip(samples, samples[1:]):
        regress = (a - b) * direction
        worst = max(worst, regress)
    return worst <= tolerance, round(worst, 1)


# ---- the Android run --------------------------------------------------------------------------------------------------------------------

def ensure_ffmpeg():
    return shutil.which("ffmpeg")


def record_start(dev, remote="/sdcard/flicker.mp4"):
    """screenrecord on the device, in the background of the host: the process lives until it is interrupted."""
    dev.sh(f"rm -f {remote}")
    proc = subprocess.Popen(["adb"] + (["-s", dev.serial] if getattr(dev, "serial", "") else []) +
                            ["shell", f"screenrecord --size {FRAME_W * 5}x{FRAME_H * 5} --bit-rate 6000000 --time-limit 170 {remote}"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(1.5)
    return proc, remote


def record_stop(dev, proc, remote, local):
    dev.sh("pkill -SIGINT screenrecord")
    time.sleep(2.5)
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.terminate()
    dev.adb("pull", remote, str(local), timeout=120)


def cut_frames(video):
    """The video as raw RGB frames at FPS and FRAME_W x FRAME_H, or None without ffmpeg."""
    if not ensure_ffmpeg() or not Path(video).exists():
        return None
    done = subprocess.run(["ffmpeg", "-loglevel", "error", "-i", str(video), "-vf", f"fps={FPS},scale={FRAME_W}:{FRAME_H}:flags=area",
                           "-pix_fmt", "rgb24", "-f", "rawvideo", "-"], capture_output=True, timeout=300)
    return split_frames(done.stdout, FRAME_W * FRAME_H * 3)


def strip_indices(flag, total, around=2):
    """The frames a flagged entry shows in the strip: a flat run as the frame before it, its first, middle and last frame and the frame
    after it; any other flag as the frame with `around` frames on both sides."""
    if flag.get("kind") == "flat" and flag.get("frames"):
        start, length = flag["index"], flag["frames"]
        picks = [start - 1, start, start + length // 2, start + length - 1, start + length]
    else:
        picks = list(range(flag["index"] - around, flag["index"] + around + 1))
    seen, out = set(), []
    for i in picks:
        if 0 <= i < total and i not in seen:
            seen.add(i)
            out.append(i)
    return out


def write_strip(path, frames, rows):
    """One PNG: one row per flagged entry, the frames of `rows` (lists of frame indices) side by side."""
    pictures = []
    for picks in rows:
        row = bytearray()
        for y in range(FRAME_H):
            for i in picks:
                row += frames[i][y * FRAME_W * 3:(y + 1) * FRAME_W * 3]
        pictures.append((len(picks) * FRAME_W, bytes(row)))
    if not pictures:
        return False
    width = max(w for w, _r in pictures)
    rgba = bytearray()
    for w, row in pictures:
        for y in range(FRAME_H):
            line = row[y * w * 3:(y + 1) * w * 3]
            px = bytearray()
            for x in range(width):
                px += (line[x * 3:x * 3 + 3] + b"\xff") if x < w else b"\x20\x20\x20\xff"
            rgba += px
    q.write_png(path, width, FRAME_H * len(pictures), bytes(rgba))
    return True


def android_audit(args):
    run = q.Run(args.out, "android", {"label": "flicker audit"})
    out = Path(args.out)
    (out / "strips").mkdir(parents=True, exist_ok=True)
    report = {"idle": None, "flash": None, "pan": []}
    lines = ["# Flicker and jitter audit (Android emulator, CPU renderer)", ""]

    def device(apk):
        pkg, act = apk_identity(apk)
        dev = q.Android(run, pkg, act, args.serial)
        dev.adb("install", "-r", "-t", str(apk), timeout=300)
        dev.sh("settings put system accelerometer_rotation 0")
        dev.sh("setprop debug.az.env 'AZ_LOG=info;AZ_DUMP_PRESENT=1'")
        return dev

    # ---- 1. idle
    dev = device(args.apk_plain)
    dev.home()
    dev.settle(1.0)
    dev.launch(cold=True)
    dev.wait_for_front(timeout=60)
    dev.settle(5.0)
    dev.adb("logcat", "-c")
    sigs = []
    for _ in range(5):
        raw = dev.adb("exec-out", "screencap", binary=True, timeout=60)
        try:
            sigs.append(q.signature_android_raw(raw)[0])
        except ValueError:
            pass
        time.sleep(0.6)
    time.sleep(0.5)
    events = present_events(dev.adb("logcat", "-d", "-v", "threadtime", timeout=60))
    diffs = [q.sig_diff(a, b) for a, b in zip(sigs, sigs[1:])]
    ok, text = idle_verdict(events, diffs)
    report["idle"] = {"ok": ok, "detail": text, "presents": len(events)}
    lines += ["## 1. Idle", "", f"- {'PASS' if ok else 'FLAGGED'}: {text}", ""]

    # ---- 2. flash frames over a recording
    lines += ["## 2. Flash frames (recording of: cold start, rotation there and back, dark mode on and off, keyboard in and out, home and back)", ""]
    if not ensure_ffmpeg():
        lines += ["- ffmpeg is not installed on this runner: the recording was not cut into frames.", ""]
        report["flash"] = {"ok": None, "detail": "no ffmpeg"}
    else:
        marks = []
        dev.sh("am force-stop " + dev.package)
        dev.home()
        dev.settle(1.0)
        proc, remote = record_start(dev)
        t0 = time.time()

        def mark(label):
            marks.append((round(time.time() - t0, 1), label))

        mark("cold start")
        dev.launch(cold=True)
        dev.wait_for_front(timeout=60)
        dev.settle(2.5)
        mark("rotate to landscape")
        dev.rotate(1)
        dev.settle(3.0)
        mark("rotate to portrait")
        dev.rotate(0)
        dev.settle(3.0)
        mark("dark mode on")
        dev.sh("cmd uimode night yes")
        dev.settle(3.0)
        mark("dark mode off")
        dev.sh("cmd uimode night no")
        dev.settle(3.0)
        mark("keyboard in")
        at = dev.find_label("TextInput")
        if at:
            dev.tap(*at)
            dev.settle(2.5)
            mark("keyboard out")
            dev.back()
            dev.settle(2.0)
        mark("home")
        dev.home()
        dev.settle(2.0)
        mark("back in front")
        dev.resume()
        dev.settle(3.0)
        record_stop(dev, proc, remote, out / "flicker.mp4")
        frames = cut_frames(out / "flicker.mp4")
        if not frames:
            lines += ["- the recording could not be cut into frames.", ""]
            report["flash"] = {"ok": None, "detail": "no frames"}
        else:
            sigs = [signature_of(f) for f in frames]
            flagged = flag_frames(sigs)
            ok = not flagged
            report["flash"] = {"ok": ok, "frames": len(frames), "flagged": flagged, "marks": marks}
            lines += [f"- {len(frames)} frames at {FPS} fps ({len(frames) / FPS:.0f} s); scenario marks (s): " + ", ".join(f"{t} {l}" for t, l in marks),
                      f"- {'PASS: no flagged frame' if ok else 'FLAGGED: ' + str(len(flagged)) + ' frame(s)'}"]
            for f in flagged[:30]:
                label = max((m for m in marks if m[0] <= f["index"] / FPS + 0.3), default=(0, "?"))[1]
                lines.append(f"  - frame {f['index']} (t = {f['index'] / FPS:.1f} s, after '{label}'): {f['kind']}: {f['detail']}")
            if flagged and write_strip(out / "strips" / "flagged-frames.png", frames, [strip_indices(f, len(frames)) for f in flagged[:10]]):
                lines.append("  - strips/flagged-frames.png: one row per flag: a flat run as the frame before it, its first, middle and last frame and the one after; any other flag with two frames on each side")
            lines.append("")

    # ---- 3. pan jitter
    lines += ["## 3. Pan jitter (a finger pan through adb's input: the real finger path)", ""]
    for name, apk, pan in (("AzWidgets (long page)", args.apk_e2e or args.apk_plain, (540, 1800, 540, 600, 700)),
                           ("AzMaps (the map)", args.maps, (300, 1500, 900, 900, 700))):
        if not apk:
            continue
        d = device(apk)
        d.launch(cold=True)
        d.wait_for_front(timeout=60)
        d.settle(6.0 if "Maps" in name else 3.0)
        slug = name.split()[0].lower()
        # a system dialog over the app takes the swipes: look before the pan, and keep the pictures before and after
        d.sh("uiautomator dump /sdcard/qa-perm.xml", timeout=40)
        dialog = q.detect_permission_dialog(d.sh("dumpsys window | grep -E 'mCurrentFocus'"), d.sh("cat /sdcard/qa-perm.xml"), d.package)
        sig_before, _w, _h, shot_before = d.capture(f"pan-{slug}-before")
        d.adb("logcat", "-c")
        x1, y1, x2, y2, ms = pan
        for _ in range(3):
            d.swipe(x1, y1, x2, y2, ms)
            time.sleep(0.4)
        time.sleep(1.0)
        events = present_events(d.adb("logcat", "-d", "-v", "threadtime", timeout=60))
        stats = frame_time_stats(events)
        sig_after, _w, _h, shot_after = d.capture(f"pan-{slug}-after")
        moved = q.sig_diff(sig_before, sig_after)
        entry = {"app": name, "presents": len(events), "engine_frame_ms": stats, "picture_changed": round(moved, 3),
                 "permission_dialog": dialog, "shots": [shot_before, shot_after]}
        report["pan"].append(entry)
        lines.append(f"- {name}: {len(events)} presents during three pans; engine present interval " + (f"p50 {stats['p50']} / p95 {stats['p95']} / max {stats['max']} ms (n={stats['n']})" if stats else "n/a (fewer than two presents)"))
        lines.append(f"  - the picture after the pans differs {moved:.3f} from the one before (shots/{shot_before}, shots/{shot_after})"
                     + (" - the pans moved nothing" if moved < q.SAME_BELOW else ""))
        if dialog:
            lines.append(f"  - A SYSTEM DIALOG STANDS OVER THE APP before the pans, they went to the dialog: {dialog}")
    lines += ["", "Notes: the engine's present lines carry the engine's own clock; an emulator on a shared runner is not a phone, the shares and the order matter."]
    (out / "flicker.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    (out / "flicker.json").write_text(json.dumps(report, indent=1), encoding="utf-8")
    print("\n".join(lines))
    return 0


def apk_identity(apk):
    aapt = shutil.which("aapt2") or next(iter(sorted(Path("/usr/local/lib/android/sdk/build-tools").glob("*/aapt2"))), None)
    if not aapt:
        return "", ""
    text = subprocess.run([str(aapt), "dump", "badging", str(apk)], capture_output=True, text=True).stdout
    pkg = re.search(r"package: name='([^']*)'", text)
    act = re.search(r"launchable-activity: name='([^']*)'", text)
    return (pkg.group(1) if pkg else ""), (act.group(1) if act else "")


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="platform", required=True)
    a = sub.add_parser("android")
    a.add_argument("--out", required=True)
    a.add_argument("--apk-plain", required=True)
    a.add_argument("--apk-e2e", default="")
    a.add_argument("--maps", default="")
    a.add_argument("--serial", default="")
    args = ap.parse_args(argv)
    return android_audit(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
