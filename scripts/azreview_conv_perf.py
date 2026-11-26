#!/usr/bin/env python3
"""AzReview with fifty conversations: what the margins of the pages, the popover and the agents at work cost.

    python3 scripts/azreview_conv_perf.py --bin target/release/AzReview [--window cpu|gpu|default] [--out DIR]
        (a real window: announce it first, it is the user's screen)

Runs ONE AzReview with the mock agent on a folder of three pages (`AZREVIEW_PROFILE=1`, `AZ_DEBUG_DAMAGE=1`) and reports,
in order:

  none      the frames of a wheel scroll across the three pages with no conversation (the base)
  fifty     fifty conversations added (the debug op, no microphone) spread over the pages: the same scroll again, the
            frames painted (p50 / p90 / p99 / max, from the engine's `[damage]` lines) and the resident memory
  open      a conversation opened and closed several times by a click on its chip: from the click to the frame
  working   two agents at work (the mock takes a minute; two is what the pool runs at once): the frames painted in ten
            seconds of doing nothing (a spinner that keeps asking for frames is a cost) and the process's CPU in that time
  scroll    the wheel scroll again with two agents at work and a conversation open

A headless run has no display: frame times are the CPU renderer's. A real window (`--window`) is the user's screen.
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

import azlin_e2e as e2e
from azlin_e2e import Failure
from azreview_perf import Tail, frames_of, quantiles, rss_mb

WIDTH, HEIGHT = 1280, 820
STRIP = "__azreview_sheet-strip"


def millis(since):
    return round((time.perf_counter() - since) * 1000.0, 1)


def state(app):
    return app.must("azreview_state")["data"]["value"]["result"]["state"]


def fixture(root):
    os.makedirs(os.path.join(root, "src"), exist_ok=True)
    with open(os.path.join(root, "src", "lib.rs"), "w", encoding="utf-8") as f:
        for i in range(120):
            f.write("pub fn step_%d(x: u32) -> u32 { x + %d }\n" % (i, i))
    with open(os.path.join(root, "README.md"), "w", encoding="utf-8") as f:
        f.write("# A folder to review\n\nTwo files.\n")


def add_note(app, text, y, page=0, ask=False):
    answer = app.must("azreview_add_note", text=text, page=page, y=y, ask=ask, open=False)["data"]["value"]["result"]
    if not answer.get("success"):
        raise Failure("azreview_add_note: %s" % answer)
    return answer["id"]


def scroll_across(app, tail, steps, label):
    """The page strip scrolled to the right and back by the wheel, `steps` notches each way: the frames painted meanwhile."""
    rect = app.rect(STRIP)
    x, y = rect["x"] + rect["width"] / 2.0, rect["y"] + rect["height"] / 2.0
    app.must("reset_frame_counters")
    t0 = time.perf_counter()
    per_step = []
    for direction in (1.0, -1.0):
        for _ in range(steps):
            t1 = time.perf_counter()
            app.must("wheel", x=x, y=y, delta_x=direction * 40.0, delta_y=0.0)
            app.must("wait_frame")
            per_step.append(round((time.perf_counter() - t1) * 1000.0, 2))
    app.frame(2)
    frames = frames_of(tail.lines, t0)
    durations = [round((end - start) * 1000.0, 2) for start, end in frames]
    return {"label": label, "seconds": round(time.perf_counter() - t0, 2), "frames": len(frames),
            "frame_ms": quantiles(durations), "step_ms": quantiles(per_step)}


def cpu_percent(pid, seconds):
    """The process's CPU over `seconds`, in percent of one core (ps reports the average since the start, so two samples of
    its CPU time are taken)."""
    def cputime():
        out = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True, check=False).stdout.strip()
        parts = out.replace("-", ":").split(":")
        total = 0.0
        for part in parts:
            total = total * 60.0 + float(part)
        return total
    before = cputime()
    time.sleep(seconds)
    return round((cputime() - before) / seconds * 100.0, 1)


def body(args, logs, out):
    binary = e2e.find_binary("AzReview", args.bin, "AZREVIEW_BIN")
    data_dir = os.path.join(logs, "data")
    folder = os.path.join(logs, "folder")
    os.makedirs(data_dir, exist_ok=True)
    fixture(folder)
    env = {"AZREVIEW_PROFILE": "1", "AZ_DEBUG_DAMAGE": "1"}
    app = e2e.App("conv-perf", binary,
                  [folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-input", "off", "--standby", "off", "--agent", "mock", "--agent-delay-ms", "60000"],
                  args.debug_port, logs, args.timeout, extra_env=env)
    tail = Tail(app.err_path)
    tail.start()
    result = {"window": "%dx%d" % (WIDTH, HEIGHT), "backend": args.window or "headless"}
    try:
        app.until("the window", lambda: app.has_id(STRIP))
        app.click(text="src", every_dom=True, frames=1)
        app.until("lib.rs listed", lambda: app.shows("lib.rs", every_dom=True))
        app.click(text="lib.rs", every_dom=True, frames=1)
        app.until("lib.rs open", lambda: state(app)["tree"]["open"] == os.path.join("src", "lib.rs"))
        app.frame(5)
        result["idle"] = rss_mb(app)
        result["none"] = scroll_across(app, tail, args.steps, "no conversation")

        # fifty conversations over the three pages, a few of them close together
        t0 = time.perf_counter()
        ids = [add_note(app, "conversation number %d about this line" % i, 40.0 + (i % 9) * 15.0 * 4, page=i % 3) for i in range(50)]
        result["fifty_added_ms"] = millis(t0)
        app.until("fifty", lambda: len(state(app)["conversations"]["notes"]) == 50)
        app.frame(3)
        result["fifty"] = scroll_across(app, tail, args.steps, "fifty conversations")
        result["fifty"]["memory"] = rss_mb(app)
        app.screenshot(os.path.join(out, "conv-perf-fifty.png"))

        # a conversation opened and closed by a click on its chip
        opened = []
        target = ids[0]
        for _ in range(args.opens):
            t1 = time.perf_counter()
            app.click(selector="#__azreview_conv-%d" % target, every_dom=True, frames=1)
            app.until("open", lambda: state(app)["conversations"]["open"] == target)
            app.must("wait_frame")
            opened.append(millis(t1))
            app.click(selector="#__azreview_conv-%d" % target, every_dom=True, frames=1)
            app.until("closed", lambda: state(app)["conversations"]["open"] is None)
        result["open"] = quantiles(opened)

        # two agents at work (the pool runs two at once) and a conversation open
        for note in ids[:2]:
            app.click(selector="#__azreview_conv-%d" % note, every_dom=True, frames=1)
            app.until("open", lambda note=note: state(app)["conversations"]["open"] == note)
            app.click(selector="#__azreview_conv-send-%d" % note, every_dom=True, frames=1)
        app.until("two work", lambda: sum(1 for n in state(app)["conversations"]["notes"] if n["phase"] == "working") == 2)
        app.frame(3)
        # ten seconds of doing nothing: the frames painted and the CPU
        pid = app.process.pid
        mark = time.perf_counter()
        idle_cpu = cpu_percent(pid, 10.0)
        idle_frames = frames_of(tail.lines, mark)
        result["working"] = {"seconds": 10, "frames_painted": len(idle_frames), "cpu_percent_of_one_core": idle_cpu}
        app.screenshot(os.path.join(out, "conv-perf-working.png"))
        result["scroll"] = scroll_across(app, tail, args.steps, "two agents at work, one conversation open")
        result["memory"] = rss_mb(app)
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        tail.stop_now = True
        app.stop()
    print(json.dumps(result, indent=2))
    with open(os.path.join(out, "conv-perf.json"), "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8797)
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--out")
    parser.add_argument("--steps", type=int, default=60, help="wheel notches each way")
    parser.add_argument("--opens", type=int, default=8, help="how many times a conversation is opened and closed")
    parser.add_argument("--window", nargs="?", const="cpu", choices=("cpu", "gpu", "default"),
                        help="a real window, with this backend (announce it first)")
    args = parser.parse_args()
    if args.window:
        e2e.BACKEND = args.window
    logs = tempfile.mkdtemp(prefix="azreview-conv-perf-")
    out = args.out or logs
    os.makedirs(out, exist_ok=True)
    try:
        body(args, logs, out)
    except Failure as e:
        print("[conv-perf] FAIL: %s" % e)
        print("[conv-perf] logs in %s" % logs)
        return 1
    shutil.rmtree(os.path.join(logs, "data"), ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
