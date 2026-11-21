#!/usr/bin/env python3
"""ANIM8 idle probe: does an app at rest stop drawing? (wave 8)

Starts an app headless with its debug server, lets it settle, resets the frame
counters, idles `--idle` seconds and reports: frames rendered, display-list
rebuilds, layout passes, and what `get_animations` still holds (FLIP slides,
CSS transitions, keyframe tracks, zombies). An idle app renders 0 frames. App arguments: `ANIM8_ARGS="..."`.

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 120 --log /tmp/idle.log -- \\
      python3 scripts/anim8_idle_probe.py --bin target/release/AzWidgets
"""

import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as e2e  # noqa: E402


def body(args, logs, out):
    binary = args.bin
    extra = os.environ.get("ANIM8_ARGS", "").split()
    app = e2e.App("idle", binary, extra, args.debug_port, logs, args.timeout)
    try:
        app.until("the window", lambda: len(app.hierarchy()) > 5)
        time.sleep(4.0)
        app.must("reset_frame_counters")
        t0 = time.time()
        time.sleep(float(os.environ.get("ANIM8_IDLE_S", "3")))
        report = app.value("get_frame_report")
        anims = app.value("get_animations")
        idle = time.time() - t0
        keys = ("frames_since_reset", "dl_rebuilds", "layout_passes", "dom_regenerations",
                "relayout_iterations", "accumulated_paint_damage_kind")
        app.log("idle %.1f s: %s" % (idle, json.dumps({k: report.get(k) for k in keys})))
        app.log("animations: %s" % json.dumps({k: anims.get(k) for k in (
            "active", "transitions", "zombies", "live_tracks")}))
        nodes = anims.get("nodes") or []
        app.log("slides (first 10): %s" % json.dumps(nodes[:10]))
        return True
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run("anim8idle", body, default_port=8793)
