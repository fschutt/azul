#!/usr/bin/env python3
"""SWITCH13 probe: what does one toggle of a Switch cost, in AzWidgets and on a settings page?

Starts a prebuilt app headless over its debug server (no scripted clock: the window's own
animation driver runs on the wall clock, as on a desktop), toggles a Switch and reports

- the frame counters over the toggle and its glide (`get_frame_report`: DOM regenerations,
  layout passes, display-list rebuilds, frames),
- every logged pass after the click with its time stamp and cost (`get_logs`: `[phases]`,
  `regenerate_layout`, `incremental_relayout`),
- how long the glide ran (`get_animations`, polled): the transitions in flight over time.

The look is a run switch: `--theme flat|flora|flora:green`, `--mode light|dark` (AzWidgets takes
them over the debug server's `set_theme` / `set_mode`, an Azlin app from a throwaway shared
config, AZLIN_CONFIG=<tmp>/config.json - the user's own config is never read or written).

Run ONE app at a time, inside the machine-wide lock and under the memory cap:

    until mkdir /tmp/az_app_run.lock 2>/dev/null; do sleep 10; done
    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 120 --log /tmp/switch13/w.log -- \\
      python3 scripts/switch13_probe.py --app widgets --theme flat --out /tmp/switch13/w-flat
    rmdir /tmp/az_app_run.lock

usage: switch13_probe.py --app widgets|calc|clock|news|keys [--theme T] [--mode M]
                         [--toggles N] [--poll] [--port P] [--out DIR]
"""

import argparse
import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as e2e  # noqa: E402

LIB = "/Users/fschutt/Development/azul/target/azul-lib"
BIN = "/Users/fschutt/Development/azul/target/release"

# app -> (binary, extra args, how to reach the switch, the switch's selector)
APPS = {
    "widgets": ("AzWidgets", [], None, ".__azul-native-switch"),
    "calc": ("AzCalculator", ["--size", "720x560"], "settings", "#__azcalc_set-grouping"),
}


def take_logs(app):
    """The log lines collected since the last call (`get_logs` drains the buffer)."""
    value = app.value("get_logs")
    if isinstance(value, dict):
        return value.get("logs") or value.get("messages") or []
    return value or []


# The passes a frame can take, as the Window category logs them: `[phases]` closes every
# `regenerate_layout`, a span line (`<- name (1.2ms)`) closes every other timed pass.
INTERESTING = re.compile(r"\[phases\]|\u2190 |regenerate|relayout|display.list")


def body(args, logs, out):
    binary, extra, reach, selector = APPS[args.app]
    binary = os.path.join(BIN, binary)
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    env = {"DYLD_LIBRARY_PATH": LIB}
    app_args = list(extra)
    if args.app != "widgets":
        config = os.path.join(logs, "config.json")
        with open(config, "w", encoding="utf-8") as f:
            json.dump({"currentTheme": args.theme, "mode": args.mode}, f)
        env["AZLIN_CONFIG"] = config
        app_args += ["--data-dir", data_dir]
    if args.profile:
        env["AZ_PROFILE"] = "cpu"
    # The passes and their phases (the Window category); the rest at warn, so collecting the
    # log costs the frames little (Layout at debug is the firehose).
    env["AZ_LOG"] = args.log
    app = e2e.App(args.app, binary, app_args, args.port, logs, args.timeout, extra_env=env)
    try:
        app.until("the debug server", lambda: app.op("list_windows"))
        app.frame(3)
        if args.app == "widgets":
            app.must("set_theme", theme=args.theme)
            app.must("set_mode", mode=args.mode)
            app.frame(3)
        if reach == "settings":
            app.key("comma", primary=True)
            app.until("the settings page", lambda: app.has_id("appkit-settings"))
        app.until("the switch", lambda: app.has(selector))
        app.op("wait_settled", timeout_ms=3000)
        app.frame(2)
        app.log("pid %s" % app.process.pid)
        if args.pause:
            app.log("pausing %ss (sample now)" % args.pause)
        results = []
        for toggle in range(args.toggles):
            app.op("wait_settled", timeout_ms=3000)
            app.frame(2)
            take_logs(app)
            app.must("reset_frame_counters")
            t0 = time.time()
            app.must("click", selector=selector)
            t_click = time.time() - t0
            timeline = []
            end = time.time() + args.window
            while time.time() < end:
                if args.poll:
                    a = app.value("get_animations")
                    timeline.append((round((time.time() - t0) * 1000.0, 1),
                                     a.get("transitions"), a.get("active")))
                    time.sleep(0.01)
                else:
                    time.sleep(0.05)
            report = app.value("get_frame_report")
            every = take_logs(app)
            with open(os.path.join(out, "logs-%d.json" % toggle), "w", encoding="utf-8") as f:
                json.dump(every, f, indent=0)
            lines = [l for l in every
                     if isinstance(l, dict) and INTERESTING.search(l.get("message", ""))]
            first_ts = lines[0]["timestamp_us"] if lines else 0
            passes = ["%+8.1f ms  %s" % ((l["timestamp_us"] - first_ts) / 1000.0,
                                         l["message"][:220]) for l in lines]
            glide = None
            if timeline:
                moving = [t for (t, tr, ac) in timeline if (tr or 0) + (ac or 0) > 0]
                glide = (moving[0], moving[-1]) if moving else None
            keys = ["dom_regenerations", "layout_passes", "dl_rebuilds", "frames_since_reset",
                    "relayout_iterations", "last_dl_build_patched", "accumulated_paint_damage_kind"]
            summary = {k: report.get(k) for k in keys if isinstance(report, dict)}
            results.append({"toggle": toggle, "click_ms": round(t_click * 1000.0, 1),
                            "report": summary, "glide_ms": glide, "passes": passes,
                            "timeline": timeline})
            app.log("toggle %d: click op %.1f ms, %s, glide %s" % (
                toggle, t_click * 1000.0, json.dumps(summary), glide))
            for p in passes:
                app.log("   " + p)
        with open(os.path.join(out, "result.json"), "w", encoding="utf-8") as f:
            json.dump(results, f, indent=1)
        return True
    finally:
        app.stop()


def main():
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--app", choices=sorted(APPS), required=True)
    p.add_argument("--theme", default="flat")
    p.add_argument("--mode", default="light")
    p.add_argument("--toggles", type=int, default=3)
    p.add_argument("--window", type=float, default=0.8, help="seconds observed after a click")
    p.add_argument("--poll", action="store_true", help="poll get_animations every 10 ms")
    p.add_argument("--profile", action="store_true", help="AZ_PROFILE=cpu (stderr)")
    p.add_argument("--pause", type=float, default=0.0)
    p.add_argument("--log", default="warn,+window", help="AZ_LOG for the app")
    p.add_argument("--port", type=int, default=8791)
    p.add_argument("--timeout", type=int, default=100)
    p.add_argument("--out", required=True)
    args = p.parse_args()
    os.makedirs(args.out, exist_ok=True)
    ok = False
    try:
        ok = body(args, args.out, args.out)
    except e2e.Failure as e:
        print("[%s] FAIL: %s" % (args.app, e), flush=True)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
