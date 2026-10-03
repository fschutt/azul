#!/usr/bin/env python3
"""AzMeet's CPU budget in a two-person call, measured (the idle_cpu_probe.py way: `ps -o %cpu=`
once a second) and checked against what the call should cost.

Two scenarios, one after the other (two capped apps at a time, 1000 MB each):

  quiet  every microphone muted, every camera off. The pump slows to 250 ms after a quiet
         second and nothing repaints: each app's CPU should be near 0 (--quiet-max, default
         1.0 %), no DOM rebuild and no display-list rebuild in the measured window.
  video  both cameras on (the test pattern, 15 fps, at the rendition the other's tile asks for):
         every encoder must run in hardware (AzMeet prints `AZMEET_ENCODER <stream> <size>
         hardware|software` per encoder it opens), video frames must rebuild no display list,
         and every damaged rectangle must lie inside the other's camera tile - the window repaints
         the tile, nothing else. The CPU is reported (the budget: well under 10 % per client).

The debug server answers the frame-report ops (`reset_frame_counters`, `get_frame_report`);
its own poll (250 ms when quiet, see IDLE_CPU) is part of what is measured.

Usage (after building libazul with the debug server and AzMeet):

    python3 scripts/azmeet_cpu.py [--bin ...] [--worker-dir ...] [--capped ...]
        [--seconds 10] [--quiet-max 1.0] [--only quiet|video] [--json]
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azmeet_e2e as e2e  # noqa: E402  (the process, app and debug-server helpers)


def app_pid(proc):
    """The AzMeet process: the capped runner's child, or the process itself."""
    try:
        out = subprocess.run(["pgrep", "-P", str(proc.process.pid)], capture_output=True, text=True).stdout
        children = [int(p) for p in out.split()]
        if children:
            return children[0]
    except (OSError, ValueError):
        pass
    return proc.process.pid


def sample_cpu(pids, seconds):
    """{name: [cpu %, ...]} sampled once a second for `seconds`."""
    samples = {name: [] for name in pids}
    for _ in range(seconds):
        for name, pid in pids.items():
            out = subprocess.run(["ps", "-o", "%cpu=", "-p", str(pid)], capture_output=True, text=True).stdout
            try:
                samples[name].append(float(out.strip()))
            except ValueError:
                pass
        time.sleep(1)
    return samples


def inside(rect, outer):
    x, y, w, h = (float(rect.get(k, 0)) for k in ("x", "y", "width", "height"))
    ox, oy, ow, oh = (float(outer.get(k, 0)) for k in ("x", "y", "width", "height"))
    return x >= ox - 1 and y >= oy - 1 and x + w <= ox + ow + 1 and y + h <= oy + oh + 1


def run_scenario(name, args, binary, worker, logs, extra):
    deadline = time.time() + args.timeout
    procs = []
    result = {"scenario": name}
    try:
        ada = e2e.App("%s-ada" % name, binary, args.port_a,
                      e2e.app_env(worker, "Ada", args.port_a, dict(extra, AZMEET_AUTOCREATE="1")),
                      logs, args.capped, 1000, args.app_seconds)
        procs.append(ada)
        link = e2e.until("Ada's link", lambda: (ada.printed("AZMEET_LINK") or [None])[0], deadline, procs)
        ben = e2e.App("%s-ben" % name, binary, args.port_b,
                      e2e.app_env(worker, "Ben", args.port_b, dict(extra, AZMEET_JOIN=link)),
                      logs, args.capped, 1000, args.app_seconds)
        procs.append(ben)
        for app, other in ((ada, "ben"), (ben, "ada")):
            e2e.until("%s's debug server" % app.name, lambda app=app: app.op("wait_frame") is not None,
                      deadline, procs)
            e2e.until("%s's tile in %s" % (other, app.name),
                      lambda app=app, other=other: app.rect(app.id("tile-%s-camera" % other))[0] is not None,
                      deadline, procs)
        if name == "video":
            for app, other in ((ada, "Ben"), (ben, "Ada")):
                e2e.until("%s decoding %s" % (app.name, other),
                          lambda app=app, other=other: (e2e.decoded_from(app, other) or (None, 0))[1] > 0,
                          deadline, procs)
            # The statistics panel repaints with its numbers; the measurement looks at the tiles.
            for app in (ada, ben):
                app.must("click", text="People")
                app.frame()
        time.sleep(args.warmup)
        for app in (ada, ben):
            app.must("reset_frame_counters")
        pids = {"Ada": app_pid(ada), "Ben": app_pid(ben)}
        samples = sample_cpu(pids, args.seconds)
        for label, app, other in (("Ada", ada, "ben"), ("Ben", ben, "ada")):
            report = app.value("get_frame_report")
            cpu = samples[label]
            entry = {
                "cpu_mean": round(sum(cpu) / len(cpu), 2) if cpu else None,
                "cpu_max": max(cpu) if cpu else None,
                "dom_regenerations": report.get("dom_regenerations"),
                "dl_rebuilds": report.get("dl_rebuilds"),
                "frames": report.get("frames_since_reset"),
                "damage": report.get("accumulated_paint_damage_rects"),
            }
            _, tile = app.rect(app.id("tile-%s-camera" % other))
            entry["damage_inside_tile"] = all(inside(r, tile) for r in (entry["damage"] or []))
            if name == "video":
                # `AZMEET_ENCODER <track-rendition> <w>x<h> hardware|software`, per encoder opened.
                lines = app.printed("AZMEET_ENCODER")
                entry["encoder"] = "; ".join(lines) if lines else None
                entry["hardware"] = bool(lines) and all(l.endswith(" hardware") for l in lines)
            result[label] = entry
    finally:
        for p in reversed(procs):
            p.stop()
    return result


def judge(result, args):
    """The broken promises of a scenario's result."""
    broken = []
    for label in ("Ada", "Ben"):
        r = result.get(label) or {}
        if result["scenario"] == "quiet":
            if r.get("cpu_mean") is None or r["cpu_mean"] > args.quiet_max:
                broken.append("%s: %s %% CPU in a quiet call (max %.1f)" % (label, r.get("cpu_mean"), args.quiet_max))
            if r.get("dom_regenerations"):
                broken.append("%s: %s DOM rebuilds in a quiet call" % (label, r["dom_regenerations"]))
            if r.get("dl_rebuilds"):
                broken.append("%s: %s display-list rebuilds in a quiet call" % (label, r["dl_rebuilds"]))
        else:
            if not r.get("hardware"):
                broken.append("%s: the encoder is not in hardware: %s" % (label, r.get("encoder")))
            if r.get("dl_rebuilds"):
                broken.append("%s: %s display-list rebuilds for video frames" % (label, r["dl_rebuilds"]))
            if not r.get("damage_inside_tile"):
                broken.append("%s: damage outside the video tile: %s" % (label, r.get("damage")))
    return broken


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--worker-dir")
    parser.add_argument("--capped", default=os.environ.get("AZ_RUN_CAPPED", e2e.DEFAULT_CAPPED))
    parser.add_argument("--port-a", type=int, default=8783)
    parser.add_argument("--port-b", type=int, default=8784)
    parser.add_argument("--worker-port", type=int, default=8791)
    parser.add_argument("--seconds", type=int, default=10)
    parser.add_argument("--warmup", type=float, default=3.0)
    parser.add_argument("--timeout", type=int, default=90)
    parser.add_argument("--app-seconds", type=int, default=120)
    parser.add_argument("--quiet-max", type=float, default=1.0)
    parser.add_argument("--only", choices=["quiet", "video"])
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    if args.capped and not os.access(args.capped, os.X_OK):
        args.capped = None
        e2e.log("WARNING: no capped runner: the apps run uncapped")

    logs = tempfile.mkdtemp(prefix="azmeet-cpu-")
    dev = None
    results = []
    try:
        binary = e2e.find_binary(args.bin)
        worker_dir = e2e.find_worker(args.worker_dir)
        worker = "http://127.0.0.1:%d" % args.worker_port
        dev = e2e.Process("worker", [shutil.which("node") or "node", os.path.join(worker_dir, "dev-server.mjs"),
                                     "--memory", "--port", str(args.worker_port)], dict(os.environ), logs)
        e2e.until("the dev server", lambda: e2e.http_json(worker + "/health").get("ok") is True,
                  time.time() + 30, [dev])
        scenarios = [
            ("quiet", {"AZMEET_TEST_TONE": "", "AZMEET_TEST_PATTERN": "", "AZMEET_PANEL": "people"}),
            ("video", {"AZMEET_TEST_TONE": "", "AZMEET_TEST_PATTERN": "1"}),
        ]
        for name, extra in scenarios:
            if args.only and args.only != name:
                continue
            e2e.log("scenario %s ..." % name)
            results.append(run_scenario(name, args, binary, worker, logs, extra))
    except e2e.Failure as e:
        e2e.log("FAIL: %s (logs in %s)" % (e, logs))
        return 2
    finally:
        if dev:
            dev.stop()
    broken = [b for r in results for b in judge(r, args)]
    if args.json:
        print(json.dumps({"results": results, "broken": broken}, indent=2))
    else:
        for r in results:
            for label in ("Ada", "Ben"):
                x = r.get(label) or {}
                e2e.log("%s %s: CPU mean %s %% max %s %%, DOM rebuilds %s, DL rebuilds %s, frames %s%s" % (
                    r["scenario"], label, x.get("cpu_mean"), x.get("cpu_max"), x.get("dom_regenerations"),
                    x.get("dl_rebuilds"), x.get("frames"),
                    ", encoder: %s" % x.get("encoder") if "encoder" in x else ""))
        for b in broken:
            e2e.log("BROKEN: %s" % b)
        e2e.log("PASS" if not broken else "FAIL")
    return 0 if not broken else 1


if __name__ == "__main__":
    sys.exit(main())
