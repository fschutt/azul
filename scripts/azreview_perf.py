#!/usr/bin/env python3
"""AzReview's ink under measurement, headless (the CPU renderer): what a pen sample costs.

    python3 scripts/azreview_perf.py --bin target/release/AzReview [--points 1500] [--strokes 40] [--out DIR]

Runs ONE AzReview on a folder of code (`AZ_BACKEND=headless`, the debug server, `AZREVIEW_PROFILE=1`,
`AZ_DEBUG_DAMAGE=1`) and reports, in order:

  idle      the resident memory of the window with a page of code open (`get_frame_report`)
  hover     a sweep of N pointer moves over the page with the dwell on (the cost of watching a pen rest)
  stroke    one stroke of N points on a page of code: per move, AzReview's handler (`AZREVIEW_PERF`:
            `move_us`), the frame of the ink layer (`draw_us`), and from the `[damage]` lines the engine
            prints (timestamped here when they ARRIVE, as mapedit3's perf_matrix does) how long each
            painted frame took (first `rect(s) after merge` line to the last `painted` line) and how
            long after a move was sent the next frame was painted
  archive   S strokes of 300 points each, one after the other: the time to build the session archive
            (`AZREVIEW_ARCHIVE`, on the window's thread, after every stroke) as the session grows
  memory    the resident memory after all of it

What the numbers are NOT: the GPU path (a real window draws the ink into a texture; the headless backend
has only the CPU renderer), the pen's own reports (this driver moves the cursor; the pen's pressure and tilt
are read by `ui::sample` and cost nothing more), a display's vsync (frames here are driven by the runner's
16 ms timer, so move-to-paint latency includes up to a timer tick that a real window does not wait). One
process at a time; no window is ever opened.
"""

import json
import math
import os
import re
import statistics
import sys
import tempfile
import threading
import time

import azlin_e2e as e2e
from azlin_e2e import Failure

WIDTH, HEIGHT = 1280, 820


class Tail(threading.Thread):
    """The lines a file gains, each with the time it ARRIVED (a frame's lines are the paint's clock)."""

    def __init__(self, path):
        super().__init__(daemon=True)
        self.path = path
        self.lines = []
        self.stop_now = False

    def run(self):
        while not os.path.exists(self.path) and not self.stop_now:
            time.sleep(0.01)
        with open(self.path, "r", encoding="utf-8", errors="replace") as f:
            while not self.stop_now:
                line = f.readline()
                if line:
                    self.lines.append((time.perf_counter(), line.rstrip("\n")))
                else:
                    time.sleep(0.0005)


def quantiles(values):
    if not values:
        return {"n": 0}
    v = sorted(values)

    def at(q):
        return v[min(len(v) - 1, max(0, math.ceil(q * len(v)) - 1))]

    return {"n": len(v), "p50": at(0.5), "p90": at(0.9), "p99": at(0.99), "max": v[-1],
            "mean": round(statistics.fmean(v), 3)}


def frames_of(lines, since):
    """(start, end) of every painted frame in `lines` after `since`: the `after merge` line to the last
    `painted` line before the next one."""
    frames, start, end = [], None, None
    for t, line in lines:
        if t < since:
            continue
        if "[damage]" in line and "after merge" in line:
            if start is not None and end is not None:
                frames.append((start, end))
            start, end = t, t
        elif "[damage]" in line and "painted" in line and start is not None:
            end = t
    if start is not None and end is not None:
        frames.append((start, end))
    return frames


def rss_mb(app):
    report = app.value("get_frame_report")
    return {"rss_mb": round(report.get("rss_bytes", 0) / 1048576.0, 1),
            "peak_rss_mb": round(report.get("peak_rss_bytes", 0) / 1048576.0, 1)}


def point(i, per_row=300):
    """The i-th point of a scribble over a page of code: rows of a gentle wave, 1.8 px apart."""
    col, row = i % per_row, i // per_row
    return 140.0 + col * 1.8, 60.0 + (row % 40) * 14.0 + 5.0 * math.sin(col / 6.0)


def stroke(app, origin, points, per_row=300, per_frame=2, stamps=None):
    """A stroke of `points` points: two moves per frame (a 120 Hz pen on 60 Hz frames). `stamps` collects
    the time each move was sent."""
    ox, oy = origin
    x, y = point(0, per_row)
    app.must("mouse_move", x=ox + x, y=oy + y)
    app.must("mouse_down", x=ox + x, y=oy + y)
    for i in range(1, points):
        x, y = point(i, per_row)
        if stamps is not None:
            stamps.append(time.perf_counter())
        app.must("mouse_move", x=ox + x, y=oy + y)
        if i % per_frame == 0:
            app.must("wait_frame")
    app.must("mouse_up", x=ox + x, y=oy + y)
    app.frame(3)


def perf_line(app, count_before):
    """The `AZREVIEW_PERF` line a stroke's end printed after `count_before` lines, as a dict."""
    app.until("the stroke's timing line", lambda: len(app.printed("AZREVIEW_PERF", r".*")) > count_before)
    text = app.printed("AZREVIEW_PERF", r".*")[-1]
    return {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", text)}


def us_to_ms(d):
    return {k: (round(v / 1000.0, 3) if k.endswith(("p50", "p99", "max")) else v) for k, v in d.items()}


def body(args, logs, out):
    binary = e2e.find_binary("AzReview", args.bin, "AZREVIEW_BIN")
    data_dir = os.path.join(logs, "data")
    folder = os.path.join(logs, "folder")
    os.makedirs(os.path.join(folder, "src"), exist_ok=True)
    os.makedirs(data_dir, exist_ok=True)
    with open(os.path.join(folder, "src", "lib.rs"), "w", encoding="utf-8") as f:
        for i in range(200):
            f.write("pub fn step_%d(x: u32) -> u32 { x + %d } // a line of code to review\n" % (i, i))
    app = e2e.App("perf", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-input", "any", "--dwell-ms", "10000"],
                  args.debug_port, logs, args.timeout,
                  extra_env={"AZREVIEW_PROFILE": "1", "AZ_DEBUG_DAMAGE": "1"})
    tail = Tail(app.err_path)
    tail.start()
    result = {"points": args.points, "strokes": args.strokes, "window": "%dx%d" % (WIDTH, HEIGHT)}
    try:
        app.until("AzReview's window", lambda: app.has_id("__azreview_sheet-strip"))
        app.frame(2)
        app.click(text="lib.rs")
        app.until("the files folded away", lambda: not app.has_id("__azreview_flyout"))
        strip = app.rect("__azreview_sheet-strip")
        origin = (strip["x"], strip["y"])
        app.frame(5)
        result["idle"] = rss_mb(app)

        # hover: the dwell watches every move, nothing is drawn.
        before = len(app.printed("AZREVIEW_PERF", r".*"))
        app.must("mouse_move", x=origin[0] + 140, y=origin[1] + 60)
        for i in range(args.points):
            x, y = point(i)
            app.must("mouse_move", x=origin[0] + x, y=origin[1] + y)
            if i % 2 == 0:
                app.must("wait_frame")
        # a tap-sized stroke flushes the timing line (it is printed when a stroke ends)
        stroke(app, origin, 5)
        result["hover"] = us_to_ms(perf_line(app, before))

        # one long stroke
        before = len(app.printed("AZREVIEW_PERF", r".*"))
        stamps, since = [], time.perf_counter()
        stroke(app, origin, args.points, stamps=stamps)
        result["stroke"] = us_to_ms(perf_line(app, before))
        lines = list(tail.lines)
        frames = frames_of(lines, since)
        paint_ms = [(e - s) * 1000.0 for s, e in frames]
        ends = [e for _, e in frames]
        latency = []
        j = 0
        for t in stamps:
            while j < len(ends) and ends[j] < t:
                j += 1
            if j < len(ends):
                latency.append((ends[j] - t) * 1000.0)
        result["stroke_frames"] = {"painted_frames": len(frames), "paint_ms": quantiles(paint_ms),
                                   "move_to_painted_ms": quantiles(latency)}
        counters = app.value("get_frame_report")
        result["stroke_cpu_last"] = counters.get("cpu_last", {})

        # a session growing: S strokes of 300 points, the archive built after each
        archives_before = len(app.printed("AZREVIEW_ARCHIVE", r".*"))
        for k in range(args.strokes):
            stroke(app, origin, 300, per_row=300, per_frame=6)
        kept = int(app.last("AZREVIEW_STROKES"))
        # a save asked for while one is in flight waits and is built once: the last archive has every stroke
        app.until("the last archive", lambda: any(("strokes=%d " % kept) in t
                                                  for t in app.printed("AZREVIEW_ARCHIVE", r".*")[archives_before:]))
        rows = []
        for text in app.printed("AZREVIEW_ARCHIVE", r".*")[archives_before:]:
            d = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", text)}
            rows.append(d)
        result["archive_growth"] = [{"strokes": r["strokes"], "bytes": r["bytes"], "ms": round(r["us"] / 1000.0, 2)}
                                    for r in rows[:: max(1, len(rows) // 8)] + rows[-1:]]
        result["archive_ms"] = quantiles([r["us"] / 1000.0 for r in rows])
        app.frame(5)
        result["after"] = rss_mb(app)
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        tail.stop_now = True
        app.stop()
    text = json.dumps(result, indent=2)
    print(text)
    if args.out:
        os.makedirs(args.out, exist_ok=True)
        with open(os.path.join(args.out, "azreview_perf.json"), "w", encoding="utf-8") as f:
            f.write(text + "\n")
    return True


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8791)
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument("--points", type=int, default=1500)
    parser.add_argument("--strokes", type=int, default=40)
    parser.add_argument("--out")
    cli = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azreview-perf-")
    ok = False
    try:
        ok = bool(body(cli, logs, cli.out))
    except Failure as e:
        print("[azreview-perf] FAIL: %s" % e, flush=True)
    sys.exit(0 if ok else 1)
