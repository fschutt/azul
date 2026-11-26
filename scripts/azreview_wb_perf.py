#!/usr/bin/env python3
"""AzReview with a loop that waits for the whiteboard: what the armed loop costs while it waits and while it is carried.

    python3 scripts/azreview_wb_perf.py --bin target/release/AzReview [--window cpu|gpu|default] [--out DIR]
        (a real window: announce it first, it is the user's screen)

Runs ONE AzReview (`AZREVIEW_PROFILE=1`, `AZ_DEBUG_DAMAGE=1`) on a folder of three pages and reports, in order:

  idle      the process's CPU (percent of one core) over five seconds with nothing armed, and the frames painted
  armed     the same over five seconds with a loop that waits (its ring runs out ten times a second: the layer of loops of the
            page is drawn again at each tick), and the frames painted
  carried   the loop carried from its grip across the page to the panel in thirty moves: the time of each move to the frame
            that follows it (p50 / p90 / max), and the frames painted
  after     the same idle measure once the loop is gone (the clock must have stopped itself)

A headless run has no display: frame times are the CPU renderer's. A real window (`--window`) is the user's screen.
"""

import argparse
import json
import os
import sys
import tempfile
import time

import azlin_e2e as e2e
from azlin_e2e import Failure
from azreview_conv_perf import cpu_percent
from azreview_e2e import WIDTH, HEIGHT, STRIP, board_of, draw_loop, fixture, line_box, pick, show_files, state
from azreview_perf import Tail, frames_of, quantiles, rss_mb


def measure(app, tail, seconds, label):
    """CPU and frames painted over `seconds` of doing nothing."""
    t0 = time.perf_counter()
    cpu = cpu_percent(app.process.pid, seconds)
    frames = frames_of(tail.lines, t0)
    return {"label": label, "seconds": seconds, "cpu_percent_of_one_core": cpu, "frames_painted": len(frames)}


def body(args, logs, out):
    binary = e2e.find_binary("AzReview", args.bin, "AZREVIEW_BIN")
    data_dir = os.path.join(logs, "data")
    folder = os.path.join(logs, "folder")
    os.makedirs(os.path.join(data_dir, "review"), exist_ok=True)
    fixture(folder)
    # a loop waits 30 s here: the measures need the time
    with open(os.path.join(data_dir, "review", "settings.json"), "w", encoding="utf-8") as f:
        json.dump({"values": {"whiteboard-armed-seconds": "30", "whiteboard-tip-seen": "1"}}, f)
    env = {"AZREVIEW_PROFILE": "1", "AZ_DEBUG_DAMAGE": "1"}
    app = e2e.App("wb-perf", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-input", "off", "--standby", "off", "--agent", "mock", "--agent-auto", "off"],
                  args.debug_port, logs, args.timeout, extra_env=env)
    tail = Tail(app.err_path)
    tail.start()
    result = {"window": "%dx%d" % (WIDTH, HEIGHT), "backend": args.window or "headless"}
    try:
        app.until("the window", lambda: app.has_id(STRIP))
        show_files(app)
        pick(app, "src", "lib.rs")
        app.until("lib.rs open", lambda: state(app)["tree"]["open"] == os.path.join("src", "lib.rs"))
        app.until("the board is read", lambda: board_of(app)["started"])
        if not state(app)["right_open"]:
            app.click(selector="#__azreview_right-toggle")
        app.until("the whiteboard's panel", lambda: app.has_id("__azreview_right-panel"))
        panel = app.rect("__azreview_right-panel")
        app.frame(10)
        result["idle"] = measure(app, tail, 5.0, "nothing armed")
        result["idle"]["memory"] = rss_mb(app)

        strip = app.rect(STRIP)
        draw_loop(app, line_box(app, strip, 7, 9))
        app.until("the loop is armed", lambda: len(board_of(app)["armed"]) == 1)
        armed = board_of(app)["armed"][0]
        result["armed"] = measure(app, tail, 5.0, "a loop waits")
        result["armed"]["memory"] = rss_mb(app)
        if len(board_of(app)["armed"]) != 1:
            raise Failure("the loop is gone before the measure was over: %s" % board_of(app)["armed"])

        # the carry: the grip to the panel in thirty moves, the time of each move to its frame
        overlay = app.rect("__azreview_wb-overlay-0", every_dom=True)
        grip = armed["grip"]
        gx = strip["x"] + overlay["x"] + grip["x"] + grip["w"] / 2.0
        gy = strip["y"] + overlay["y"] + grip["y"] + grip["h"] / 2.0
        tx, ty = panel["x"] + panel["width"] / 2.0, panel["y"] + 180.0
        app.must("mouse_move", x=gx, y=gy)
        app.must("mouse_down", x=gx, y=gy)
        app.must("reset_frame_counters")
        t0 = time.perf_counter()
        per_move = []
        for i in range(1, 31):
            t1 = time.perf_counter()
            app.must("mouse_move", x=gx + (tx - gx) * i / 30.0, y=gy + (ty - gy) * i / 30.0)
            app.must("wait_frame")
            per_move.append(round((time.perf_counter() - t1) * 1000.0, 2))
        frames = frames_of(tail.lines, t0)
        result["carried"] = {"moves": 30, "frames": len(frames), "move_ms": quantiles(per_move),
                             "frame_ms": quantiles([round((end - start) * 1000.0, 2) for start, end in frames])}
        app.must("mouse_up", x=tx, y=ty)
        app.until("the cutout is on the board", lambda: len(board_of(app)["cutouts"]) == 1)
        app.until("its picture is drawn", lambda: board_of(app)["cutouts"][0]["picture"] == "ready")
        app.frame(10)
        result["after"] = measure(app, tail, 5.0, "the loop was taken")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        tail.stop_now = True
        app.stop()
    print(json.dumps(result, indent=2))
    with open(os.path.join(out, "wb-perf.json"), "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8798)
    parser.add_argument("--timeout", type=int, default=300)
    parser.add_argument("--out")
    parser.add_argument("--window", nargs="?", const="cpu", choices=("cpu", "gpu", "default"),
                        help="a real window, with this backend (announce it first)")
    args = parser.parse_args()
    if args.window:
        e2e.BACKEND = args.window
    logs = tempfile.mkdtemp(prefix="azreview-wb-perf-")
    out = args.out or logs
    os.makedirs(out, exist_ok=True)
    try:
        body(args, logs, out)
    except Failure as e:
        print("[wb-perf] FAIL: %s" % e)
        print("[wb-perf] logs in %s" % logs)
        sys.exit(1)


if __name__ == "__main__":
    main()
