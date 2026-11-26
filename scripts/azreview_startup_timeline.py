#!/usr/bin/env python3
"""The first window of AzReview, step by step: where the time between the process being spawned and the tree being on the
screen goes.

    python3 scripts/azreview_startup_timeline.py --bin target/release/AzReview --folder /path/to/checkout
        [--window cpu|gpu|default] [--out DIR]

Runs ONE AzReview on the folder (a real window with `--window`: announce it, it is the user's screen; headless without),
and lays three clocks on one line:

  - the harness's wall clock (the moment before the process was spawned, the moments it saw things from outside),
  - the app's own steps (`AZREVIEW_PHASE <name> unix_us=<wall clock>`: `main`, `args`, `kit`, `state`, `speech`,
    `app_created`, `run_called`, `window_created`, `first_layout`, `tree_rows`),
  - the engine's log (`AZ_LOG=debug`), which has two clocks of its own: microseconds since the logger started (INFO and
    WARN lines: `App::run starting` is where it meets the app's `run_called`) and since the window began to be made
    (the Debug lines: `[MacOSWindow::new] Starting window creation` is zero, and meets `[macOS] run() entry` of the first
    clock). The engine's steps are placed on the wall clock through those two meeting points.

It prints the table (time since the spawn, the step since the one before it), the machine's load average before and after
(`uptime`: the start is slow when other builds run), and the step that took longest. Writes `startup-timeline.json`.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time

import azlin_e2e as e2e
from azlin_e2e import Failure

WIDTH, HEIGHT = 1280, 820
TREE = "__azreview_tree"

PROCESS_CLOCK = re.compile(r"^\[\s*(\d+)us\] \[(?:INFO |WARN |ERROR|DEBUG)\s*\] \[([^\]]*)\] (.*?)(?:\s+\([\w./]+:\d+\))?$")
WINDOW_CLOCK = re.compile(r"^\[\s*(\d+)us\]\[(?:Debug|Info|Warn)\]\[(\w+)\] (.*)$")

# the engine's steps worth a row: (clock, pattern on the message, name)
ENGINE_STEPS = [
    ("process", r"App::run starting", "engine: App::run starting"),
    ("process", r"\[macOS\] run\(\) entry", "engine: macOS run() entry"),
    ("window", r"\[MacOSWindow::new\] Starting window creation", "engine: window creation begins"),
    ("window", r"\[Window Init\] Window created successfully", "engine: window object created"),
    ("window", r"\[Window Init\] Invoking create_callback", "engine: create callback"),
    ("window", r"\[regenerate_layout\] START", "engine: first layout starts"),
    ("window", r"Requesting fonts from registry", "engine: first layout asks for fonts"),
    ("window", r"Font registry snapshot complete", "engine: font registry snapshot done"),
    ("window", r"Layout completed", "engine: first layout done"),
    ("window", r"\[Window Init\] Making window visible", "engine: window made visible"),
    ("window", r"\[Window Init\] Window initialization complete", "engine: window initialisation complete"),
    ("window", r"WebRender|webrender initialised|GL Context: Some|OpenGL", "engine: GL / WebRender"),
]


def wall_us():
    return int(time.time() * 1_000_000)


def load_average():
    try:
        return subprocess.run(["uptime"], capture_output=True, text=True, check=False).stdout.strip()
    except OSError:
        return "?"


def read_lines(path):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return f.read().splitlines()
    except OSError:
        return []


def phases_of(app):
    out = {}
    for line in read_lines(app.out_path):
        m = re.match(r"^AZREVIEW_PHASE (\w+) unix_us=(\d+)$", line)
        if m and m.group(1) not in out:
            out[m.group(1)] = int(m.group(2))
    return out


def engine_events(app, phases):
    """The engine's steps on the wall clock, through the two meeting points with the app's `run_called`."""
    lines = read_lines(app.err_path)
    run_start = next((int(m.group(1)) for m in (PROCESS_CLOCK.match(l) for l in lines)
                      if m and "App::run starting" in m.group(3)), None)
    entry = next((int(m.group(1)) for m in (PROCESS_CLOCK.match(l) for l in lines)
                  if m and "run() entry" in m.group(3)), None)
    if run_start is None or entry is None or "run_called" not in phases:
        return []
    # the process clock at App::run starting is the wall clock at the app's run_called, to a few dozen microseconds
    to_wall_process = phases["run_called"] - run_start
    # the window clock is zero at run() entry
    window_zero = entry + to_wall_process
    events = []
    seen = set()
    for line in lines:
        pm = PROCESS_CLOCK.match(line)
        wm = None if pm else WINDOW_CLOCK.match(line)
        for clock, pattern, name in ENGINE_STEPS:
            if name in seen:
                continue
            if clock == "process" and pm and re.search(pattern, pm.group(3)):
                events.append((int(pm.group(1)) + to_wall_process, name))
                seen.add(name)
            elif clock == "window" and wm and re.search(pattern, wm.group(3)):
                events.append((int(wm.group(1)) + window_zero, name))
                seen.add(name)
    return events


def body(args, logs, out):
    binary = e2e.find_binary("AzReview", args.bin, "AZREVIEW_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    env = {"AZ_LOG": "debug"}
    before = load_average()
    spawn = wall_us()
    app = e2e.App("startup", binary,
                  [args.folder, "--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en",
                   "--dwell-input", "off", "--standby", "off"],
                  args.debug_port, logs, args.timeout, extra_env=env)
    seen_by_harness = {}
    try:
        # What the harness sees from outside: the debug server answering, the tree on the screen, the first frame.
        deadline = time.time() + args.timeout
        first_frame = None
        while time.time() < deadline:
            try:
                if "debug_server" not in seen_by_harness and app.value("get_frame_report") is not None:
                    seen_by_harness["debug_server"] = wall_us()
                report = app.value("get_frame_report")
                if first_frame is None and isinstance(report, dict) and report.get("frame_index", 0) >= 1:
                    first_frame = wall_us()
                    seen_by_harness["first_frame"] = first_frame
                if "tree_on_screen" not in seen_by_harness and app.has_id(TREE) and \
                        app.value("get_node_layout", selector="#" + TREE, dom_id=0).get("rect"):
                    seen_by_harness["tree_on_screen"] = wall_us()
            except (Failure, OSError, ValueError, KeyError, AttributeError):
                pass
            if "tree_on_screen" in seen_by_harness and first_frame is not None:
                break
            time.sleep(0.02)
        app.frame(3)
        after = load_average()
        phases = phases_of(app)
        rows = []
        rows.append((spawn, "harness: the process is spawned"))
        for name in ("main", "args", "kit", "state", "speech", "app_created", "run_called", "window_created", "first_layout",
                     "tree_rows"):
            if name in phases:
                rows.append((phases[name], "app: " + name))
        rows.extend(engine_events(app, phases))
        for name, label in (("debug_server", "harness: the debug server answers"),
                            ("first_frame", "harness: the first frame is painted (frame_index 1)"),
                            ("tree_on_screen", "harness: the tree is on the screen")):
            if name in seen_by_harness:
                rows.append((seen_by_harness[name], label))
        rows.sort()
        table, prev = [], rows[0][0]
        for t, label in rows:
            table.append({"at_ms": round((t - spawn) / 1000.0, 1), "step_ms": round((t - prev) / 1000.0, 1), "what": label})
            prev = t
        longest = max(table[1:], key=lambda r: r["step_ms"]) if len(table) > 1 else None
        result = {"window": args.window or "headless", "folder": args.folder, "load_before": before, "load_after": after,
                  "timeline": table, "longest_step": longest}
        for r in table:
            print("%9.1f ms  +%8.1f   %s" % (r["at_ms"], r["step_ms"], r["what"]))
        print("load before: %s\nload after:  %s" % (before, after))
        if longest:
            print("longest step: %s (%.1f ms)" % (longest["what"], longest["step_ms"]))
        with open(os.path.join(out, "startup-timeline.json"), "w", encoding="utf-8") as f:
            json.dump(result, f, indent=2)
        return result
    finally:
        app.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--folder", required=True)
    parser.add_argument("--debug-port", type=int, default=8794)
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--out")
    parser.add_argument("--window", nargs="?", const="cpu", choices=("cpu", "gpu", "default"),
                        help="a real window, with this backend (announce it first)")
    args = parser.parse_args()
    if args.window:
        e2e.BACKEND = args.window
    logs = tempfile.mkdtemp(prefix="azreview-startup-")
    out = args.out or logs
    os.makedirs(out, exist_ok=True)
    try:
        body(args, logs, out)
    except Failure as e:
        print("[startup] FAIL: %s" % e)
        print("[startup] logs in %s" % logs)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
