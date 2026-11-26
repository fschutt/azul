#!/usr/bin/env python3
"""AzReview on a big tree with the symbol index being built behind the window: what the index and the line count cost.

    python3 scripts/azreview_symbols_perf.py --bin target/release/AzReview --folder /path/to/checkout
        [--symbols on|off] [--window cpu|gpu|default] [--out DIR]

Runs ONE AzReview on the folder (`AZ_BACKEND=headless` unless `--window`, `--symbols` as asked) and reports:

  ready     how long after the spawn the index says it is done (`AZREVIEW_SYMBOLS ready`), what it found, and how long
            the tree needed to be on the screen
  cpu       the process's CPU seconds (user + system, `ps`) at the start of the scroll, when the index was done and
            when everything was quiet; the difference between a run with `--symbols off` and one with `--symbols on` is
            the index's cost, and a run with `--symbols off` on a folder of a few files against the same run on the
            big folder is the line count's (the census: every folder listed, every file read once)
  scroll    the tree scrolled by the wheel from the first moment the tree is there (so while the index is built) for
            as long as it takes: the frames painted meanwhile (p50 / p90 / p99 / max, from the engine's `[damage]`
            lines) and how many were painted in the first seconds against the rest
  memory    the resident memory at the end

No microphone (dwell and standby off). A headless run has no display: frame times are the CPU renderer's.
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
TREE = "__azreview_tree"
FILES = "__azreview_files"


def state(app):
    return app.must("azreview_state")["data"]["value"]["result"]["state"]


def cpu_seconds(pid):
    """The process's CPU time so far, in seconds (`ps -o time=` is `[[dd-]hh:]mm:ss.cc`)."""
    out = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True, text=True, check=False).stdout.strip()
    if not out:
        return None
    days = 0.0
    if "-" in out:
        d, out = out.split("-", 1)
        days = float(d)
    total = 0.0
    for part in out.split(":"):
        total = total * 60.0 + float(part)
    return round(days * 86400.0 + total, 2)


def body(args, logs, out):
    binary = e2e.find_binary("AzReview", args.bin, "AZREVIEW_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    env = {"AZREVIEW_PROFILE": "1", "AZ_DEBUG_DAMAGE": "1"}
    spawn = time.perf_counter()
    app = e2e.App("symbols-perf", binary,
                  [args.folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-input", "off", "--standby", "off", "--symbols", args.symbols],
                  args.debug_port, logs, args.timeout, extra_env=env)
    tail = Tail(app.err_path)
    tail.start()
    pid = app.process.pid
    result = {"folder": args.folder, "symbols": args.symbols, "backend": args.window or "headless"}
    try:
        app.until("the tree", lambda: app.has_id(TREE) and state(app)["tree"]["rows"] > 0)
        result["tree_on_screen_ms"] = round((time.perf_counter() - spawn) * 1000.0, 1)
        result["cpu_s_when_the_tree_is_there"] = cpu_seconds(pid)
        files = app.rect(FILES)
        x, y = files["x"] + 60.0, files["y"] + 200.0

        # every folder opened at once (the tree has rows to scroll through), then the wheel from the first moment: the frames
        # painted while the index is built (and the lines are counted)
        opened = app.must("azreview_expand_all")["data"]["value"]["result"]
        result["rows"] = opened["rows"]
        app.must("reset_frame_counters")
        t0 = time.perf_counter()
        per_step = []
        ready_at = None
        counted_at = None
        steps = 0
        deadline = t0 + args.max_seconds
        while time.perf_counter() < deadline:
            t1 = time.perf_counter()
            app.must("wheel", x=x, y=y, delta_x=0.0, delta_y=-20.0 * args.per_step)
            app.must("wait_frame")
            per_step.append(round((time.perf_counter() - t1) * 1000.0, 2))
            steps += 1
            if steps % 10 == 0:
                st = state(app)
                if ready_at is None and args.symbols == "on" and app.count("AZREVIEW_SYMBOLS", r"ready .*") > 0:
                    ready_at = time.perf_counter() - spawn
                    result["cpu_s_when_the_index_is_ready"] = cpu_seconds(pid)
                    result["symbols_ready_after_spawn_s"] = round(ready_at, 2)
                    result["symbols_line"] = app.printed("AZREVIEW_SYMBOLS", r"ready .*")[-1]
                if counted_at is None and not st["progress"]["counting"]:
                    counted_at = time.perf_counter() - spawn
                    result["lines_counted_after_spawn_s"] = round(counted_at, 2)
                    result["cpu_s_when_the_lines_are_counted"] = cpu_seconds(pid)
                    result["workspace"] = st["progress"]["workspace"]
                if (ready_at is not None or args.symbols == "off") and counted_at is not None and steps >= args.min_steps:
                    break
        app.frame(3)
        frames = frames_of(tail.lines, t0)
        durations = [round((end - start) * 1000.0, 2) for start, end in frames]
        first = [round((end - start) * 1000.0, 2) for start, end in frames if start - t0 < 5.0]
        result["scroll"] = {"steps": steps, "seconds": round(time.perf_counter() - t0, 2),
                            "frames": len(frames), "frame_ms": quantiles(durations),
                            "frames_in_the_first_5_s": len(first), "frame_ms_in_the_first_5_s": quantiles(first),
                            "ms_per_step_request_to_frame": quantiles(per_step)}
        # quiet: nothing is going on any more; the CPU after a few seconds of nothing
        time.sleep(3.0)
        result["cpu_s_at_the_end"] = cpu_seconds(pid)
        result["symbols_state"] = state(app)["symbols"]
        result["memory"] = rss_mb(app)
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        tail.stop_now = True
        app.stop()
    print(json.dumps(result, indent=2))
    with open(os.path.join(out, "symbols-perf-%s.json" % args.symbols), "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin")
    parser.add_argument("--folder", required=True)
    parser.add_argument("--symbols", choices=("on", "off"), default="on")
    parser.add_argument("--debug-port", type=int, default=8798)
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--out")
    parser.add_argument("--per-step", type=int, default=6, help="wheel notches per step")
    parser.add_argument("--min-steps", type=int, default=100, help="at least this many wheel steps")
    parser.add_argument("--max-seconds", type=int, default=90, help="the scroll stops after this long")
    parser.add_argument("--window", nargs="?", const="cpu", choices=("cpu", "gpu", "default"),
                        help="a real window, with this backend (announce it first)")
    args = parser.parse_args()
    if args.window:
        e2e.BACKEND = args.window
    logs = tempfile.mkdtemp(prefix="azreview-symbols-perf-")
    out = args.out or logs
    os.makedirs(out, exist_ok=True)
    try:
        body(args, logs, out)
    except Failure as e:
        print("[symbols-perf] FAIL: %s" % e)
        print("[symbols-perf] logs in %s" % logs)
        return 1
    shutil.rmtree(os.path.join(logs, "data"), ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
