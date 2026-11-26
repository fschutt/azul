#!/usr/bin/env python3
"""AzReview on a big tree (the Azul checkout): how long the start takes and what the tree costs, headless.

    python3 scripts/azreview_tree_perf.py --bin target/release/AzReview --folder /path/to/checkout [--out DIR]
        [--window cpu|gpu|default]   (a real window: announce it first, the user's screen)

Runs ONE AzReview on the folder (`AZ_BACKEND=headless` unless `--window`, the debug server, `AZ_DEBUG_DAMAGE=1`) and
reports, in order:

  start     what the app says of its own start (`AZREVIEW_STARTUP`: ms to the root listed, ms to the window created),
            and the wall clock from the process being spawned to the tree being on the screen (this includes the
            harness's polling, 250 ms at most)
  idle      the resident memory with the root listed and one file open
  open      the time from a click on a folder to its rows being on the screen, for a few folders of the root
  all       every folder listed and opened (`azreview_expand_all`, a debug op that does it on the window's thread):
            how long the walk of the whole tree takes, how many files, folders and rows it holds, the memory after
  scroll    the tree scrolled by the wheel through all its rows, and the frames painted meanwhile (from the engine's
            `[damage]` lines, as `azreview_perf.py` reads them): frame times p50 / p99 / max and how many frames
  memory    the resident memory after all of it

No microphone: the dwell and the standby are off. A headless run has no display: frame times are the CPU renderer's.
"""

import argparse
import json
import os
import shutil
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
    answer = app.must("azreview_state")
    return answer["data"]["value"]["result"]["state"]


def rows(app):
    return state(app)["tree"]["rows"]


def millis(since):
    return round((time.perf_counter() - since) * 1000.0, 1)


def body(args, logs, out):
    binary = e2e.find_binary("AzReview", args.bin, "AZREVIEW_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    env = {"AZREVIEW_PROFILE": "1", "AZ_DEBUG_DAMAGE": "1"}
    spawn = time.perf_counter()
    app = e2e.App("tree-perf", binary,
                  [args.folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-input", "off", "--standby", "off"],
                  args.debug_port, logs, args.timeout, extra_env=env)
    tail = Tail(app.err_path)
    tail.start()
    result = {"folder": args.folder, "window": "%dx%d" % (WIDTH, HEIGHT), "backend": args.window or "headless"}
    try:
        app.until("the tree", lambda: app.has_id(TREE) and rows(app) > 0)
        result["start"] = {"to_tree_ms": millis(spawn)}
        app.until("AZREVIEW_STARTUP", lambda: app.printed("AZREVIEW_STARTUP", r".*"))
        for key, value in (kv.split("=") for kv in app.printed("AZREVIEW_STARTUP", r".*")[-1].split()):
            result["start"][key] = int(value) if value.isdigit() else value
        app.frame(5)
        result["idle"] = {**rss_mb(app), "rows": rows(app), "tree": state(app)["tree"]}

        # a few folders of the root, one after the other: a click to the rows on the screen
        opened = []
        for name in args.open:
            before = rows(app)
            t0 = time.perf_counter()
            try:
                app.click(text=name, every_dom=True, frames=1)
            except Failure:
                continue
            app.until("rows of %s" % name, lambda: rows(app) > before)
            opened.append({"folder": name, "ms": millis(t0), "rows_before": before, "rows_after": rows(app)})
        result["open"] = opened

        # the whole tree
        t0 = time.perf_counter()
        answer = app.must("azreview_expand_all")["data"]["value"]["result"]
        app.frame(5)
        result["all"] = {"walk_ms_on_the_window_thread": answer["ms"], "totals_of_the_whole_tree_us": answer.get("rollup_us"),
                         "wall_ms": millis(t0), "listed": answer["listed"],
                         "files": answer["files"], "folders": answer["folders"], "rows": answer["rows"],
                         **{"after_" + k: v for k, v in rss_mb(app).items()}}
        app.screenshot(os.path.join(out, "tree-all.png"))

        # the wheel through all of it
        files = app.rect(FILES)
        x, y = files["x"] + 60.0, files["y"] + 200.0
        total_rows = answer["rows"]
        steps = max(10, args.steps)
        app.must("reset_frame_counters")
        t0 = time.perf_counter()
        per_step = []
        for _ in range(steps):
            t1 = time.perf_counter()
            app.must("wheel", x=x, y=y, delta_x=0.0, delta_y=-20.0 * args.per_step)
            app.must("wait_frame")
            per_step.append(round((time.perf_counter() - t1) * 1000.0, 2))
        app.frame(3)
        scrolled_s = time.perf_counter() - t0
        frames = frames_of(tail.lines, t0)
        durations = [round((end - start) * 1000.0, 2) for start, end in frames]
        report = app.value("get_frame_report")
        counters = {k: v for k, v in report.items() if isinstance(v, (int, float)) and "rss" not in k}
        # the three slowest painted frames, with the damage lines the engine printed for them: what was painted, how much
        slowest = []
        for i in sorted(range(len(frames)), key=lambda k: frames[k][1] - frames[k][0], reverse=True)[:3]:
            a, b = frames[i]
            slowest.append({"ms": round((b - a) * 1000.0, 2), "seconds_into_the_scroll": round(a - t0, 2),
                            "frame_number": i,
                            "lines": [line[:220] for t, line in tail.lines if a - 0.002 <= t <= b + 0.002][:14]})
        result["scroll"] = {"steps": steps, "seconds": round(scrolled_s, 2), "slowest_frames": slowest,
                            "frames_from_damage_lines": len(frames),
                            "frame_ms_from_damage_lines": quantiles(durations),
                            "ms_per_step_request_to_frame": quantiles(per_step), "frame_report": counters}
        app.screenshot(os.path.join(out, "tree-scrolled.png"))
        result["memory"] = rss_mb(app)
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        tail.stop_now = True
        app.stop()
    print(json.dumps(result, indent=2))
    with open(os.path.join(out, "tree-perf.json"), "w", encoding="utf-8") as f:
        json.dump(result, f, indent=2)
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin")
    parser.add_argument("--folder", required=True)
    parser.add_argument("--debug-port", type=int, default=8793)
    parser.add_argument("--timeout", type=int, default=600)
    parser.add_argument("--out")
    parser.add_argument("--open", nargs="*", default=["layout", "dll", "core", "css", "doc", "examples"])
    parser.add_argument("--steps", type=int, default=400)
    parser.add_argument("--per-step", type=int, default=6, help="wheel notches per step")
    parser.add_argument("--window", nargs="?", const="cpu", choices=("cpu", "gpu", "default"),
                        help="a real window, with this backend (announce it first)")
    args = parser.parse_args()
    if args.window:
        e2e.BACKEND = args.window
    logs = tempfile.mkdtemp(prefix="azreview-tree-perf-")
    out = args.out or logs
    os.makedirs(out, exist_ok=True)
    try:
        body(args, logs, out)
    except Failure as e:
        print("[tree-perf] FAIL: %s" % e)
        print("[tree-perf] logs in %s" % logs)
        return 1
    shutil.rmtree(os.path.join(logs, "data"), ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
