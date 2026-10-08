#!/usr/bin/env python3
"""SWITCH13 probe: what does one toggle of a Switch cost, in AzWidgets and on a settings page?

Three modes, ONE app at a time (inside the machine-wide lock, under the memory cap):

1. live (default): starts a prebuilt app headless over its debug server - no scripted clock,
   the window's own animation driver runs on the wall clock as on a desktop - toggles a Switch
   `--toggles` times and reports per toggle the frame counters over the click and its glide
   (`get_frame_report`: DOM regenerations, layout passes, display-list rebuilds, frames) and,
   with `--poll`, when transitions were in flight (`get_animations`; the debug server answers
   about every 45 ms, so this is coarse). `--profile` adds AZ_PROFILE=cpu: the per-phase dump on
   the app's stderr (`css_transition_tick`, `dl_regenerate_full`, `raster_damage_body` per glide
   frame).

       until mkdir /tmp/az_app_run.lock 2>/dev/null; do sleep 10; done
       scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 150 --log /tmp/s13/c.log -- \\
         python3 scripts/switch13_probe.py --app calc --theme flora:green --mode dark --poll \\
         --out /tmp/s13/calc-flora
       rmdir /tmp/az_app_run.lock

2. `--write-scenario OUT.json`: an AZ_E2E scenario for the app (scripted clock): the click,
   then one 16.7 ms frame per step, printing the knob's `transform`, the transitions in flight
   and the frame report after each. Run it with the app itself:

       AZ_BACKEND=headless AZ_E2E=OUT.json AZLIN_CONFIG=<tmp config> \\
         scripts/waves/tools/run_capped.sh ... -- target/release/AzCalculator --data-dir <tmp>

3. `--parse-log LOG`: that run's log as one line per frame (knob x, transitions, regenerations,
   layout passes, display-list rebuilds, paint damage). A glide whose display-list rebuild count
   climbs by one per frame is redrawn from scratch every frame; a patched or GPU-value glide
   keeps it flat.

The look: `--theme flat|flora|flora:green`, `--mode light|dark`. AzWidgets takes them over the
debug server (`set_theme`; its layout knows `flat` and `flora` only, so `flora:green` shows
flat there); an Azlin app reads them from a throwaway shared config (AZLIN_CONFIG=<tmp>/config
.json) - the user's own ~/.azlin is never read or written, and the data go to `--data-dir`.
"""

import argparse
import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

LIB = "/Users/fschutt/Development/azul/target/azul-lib"
BIN = "/Users/fschutt/Development/azul/target/release"

# app -> (binary, extra args, how to reach the switch, the switch's selector)
APPS = {
    "widgets": ("AzWidgets", [], None, ".__azul-native-switch"),
    "calc": ("AzCalculator", ["--size", "720x560"], "settings", "#__azcalc_set-grouping"),
}
KNOB = ".__azul-native-switch-knob"
RELEASED = {"shift": False, "ctrl": False, "alt": False, "meta": False}


def live(args):
    import azlin_e2e as e2e

    binary, extra, reach, selector = APPS[args.app]
    binary = os.path.join(BIN, binary)
    out = args.out
    data_dir = os.path.join(out, "data")
    os.makedirs(data_dir, exist_ok=True)
    env = {"DYLD_LIBRARY_PATH": LIB}
    app_args = list(extra)
    if args.app != "widgets":
        config = os.path.join(out, "config.json")
        with open(config, "w", encoding="utf-8") as f:
            json.dump({"currentTheme": args.theme, "mode": args.mode}, f)
        env["AZLIN_CONFIG"] = config
        app_args += ["--data-dir", data_dir]
    if args.profile:
        env["AZ_PROFILE"] = "cpu"
    app = e2e.App(args.app, binary, app_args, args.port, out, args.timeout, extra_env=env)
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
        app.log("pid %s" % app.process.pid)
        results = []
        for toggle in range(args.toggles):
            app.op("wait_settled", timeout_ms=3000)
            app.frame(2)
            app.must("reset_frame_counters")
            t0 = time.time()
            app.must("click", selector=selector)
            timeline = []
            end = time.time() + args.window
            while time.time() < end:
                if args.poll:
                    a = app.value("get_animations")
                    timeline.append((round((time.time() - t0) * 1000.0, 1),
                                     a.get("transitions"), a.get("active")))
                time.sleep(0.01 if args.poll else 0.05)
            report = app.value("get_frame_report")
            moving = [t for (t, tr, ac) in timeline if (tr or 0) + (ac or 0) > 0]
            glide = (moving[0], moving[-1]) if moving else None
            keys = ["dom_regenerations", "layout_passes", "dl_rebuilds", "frames_since_reset",
                    "accumulated_paint_damage_kind"]
            summary = {k: report.get(k) for k in keys if isinstance(report, dict)}
            results.append({"toggle": toggle, "report": summary, "glide_ms": glide,
                            "timeline": timeline})
            app.log("toggle %d: %s, transitions in flight (ms after the click) %s"
                    % (toggle, json.dumps(summary), glide))
        with open(os.path.join(out, "result.json"), "w", encoding="utf-8") as f:
            json.dump(results, f, indent=1)
        return True
    finally:
        app.stop()


def write_scenario(args):
    steps = [{"op": "wait_frame"}, {"op": "wait_frame"}, {"op": "wait_frame"}]
    _, _, reach, selector = APPS[args.app]
    if reach == "settings":
        meta = dict(RELEASED, meta=True)
        steps += [{"op": "key_down", "key": "comma", "modifiers": meta},
                  {"op": "key_up", "key": "comma", "modifiers": RELEASED},
                  {"op": "wait_frame"}, {"op": "wait_frame"}, {"op": "wait_frame"}]
    steps += [
        {"op": "wait_frame"},
        {"op": "print", "text": "BEFORE"},
        {"op": "get_node_css_properties", "selector": KNOB},
        {"op": "print_response"},
        {"op": "reset_frame_counters"},
        {"op": "click", "selector": selector},
        {"op": "print", "text": "AFTER-CLICK"},
        {"op": "get_node_css_properties", "selector": KNOB},
        {"op": "print_response"},
        {"op": "get_frame_report"},
        {"op": "print_response"},
    ]
    for i in range(args.frames):
        steps += [
            {"op": "tick_animations", "dt_micros": 16666, "steps": 1},
            {"op": "wait_frame"},
            {"op": "print", "text": "FRAME %d" % (i + 1)},
            {"op": "get_node_css_properties", "selector": KNOB},
            {"op": "print_response"},
            {"op": "get_animations"},
            {"op": "print_response"},
            {"op": "get_frame_report"},
            {"op": "print_response"},
        ]
    scenario = [{
        "name": "switch13_trajectory_%s" % args.app,
        "description": "a switch toggle, then one scripted 16.7 ms frame per step",
        "setup": {"window_width": 720, "window_height": 560},
        "steps": steps,
    }]
    with open(args.write_scenario, "w", encoding="utf-8") as f:
        json.dump(scenario, f, indent=1)
    print("wrote", args.write_scenario)
    return True


def parse_log(args):
    label, row, rows = None, {}, []
    for line in open(args.parse_log, encoding="utf-8", errors="replace").read().split("\n"):
        m = re.match(r"\[e2e\] (BEFORE|AFTER-CLICK|FRAME \d+)$", line.strip())
        if m:
            if row:
                rows.append(row)
            label, row = m.group(1), {"label": m.group(1)}
            continue
        if "[phases]" in line or "regenerate_layout (" in line or "SETTINGS_SAVED" in line:
            row.setdefault("passes", []).append(line.split("] ", 2)[-1][:150])
        if not line.startswith("[e2e] response: "):
            continue
        try:
            r = json.loads(line[len("[e2e] response: "):])
        except ValueError:
            continue
        v, t = r.get("value", {}), r.get("type")
        if t == "node_css_properties":
            row["transform"] = next((p for p in v.get("properties", [])
                                     if p.startswith("transform")), None)
        elif t == "animations":
            row["transitions"] = v.get("transitions")
        elif t == "frame_report":
            rects = [(round(x["x"]), round(x["y"]), round(x["width"]), round(x["height"]))
                     for x in v.get("paint_damage_rects", [])][:3]
            row["report"] = "regen=%s layout=%s dl_rebuilds=%s frames=%s damage=%s" % (
                v["dom_regenerations"], v["layout_passes"], v["dl_rebuilds"],
                v["frames_since_reset"], rects)
    if row:
        rows.append(row)
    for r in rows:
        print("%-12s %-34s transitions=%-4s %s" % (r.get("label", "-"), r.get("transform"),
                                                  r.get("transitions"), r.get("report", "")))
        for p in r.get("passes", []):
            print("             | " + p)
    return True


def main():
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--app", choices=sorted(APPS), default="calc")
    p.add_argument("--theme", default="flat")
    p.add_argument("--mode", default="light")
    p.add_argument("--toggles", type=int, default=3)
    p.add_argument("--window", type=float, default=0.8, help="seconds observed after a click")
    p.add_argument("--poll", action="store_true", help="poll get_animations while observing")
    p.add_argument("--profile", action="store_true", help="AZ_PROFILE=cpu (the app's stderr)")
    p.add_argument("--port", type=int, default=8791)
    p.add_argument("--timeout", type=int, default=100)
    p.add_argument("--out", help="live mode: the folder for the logs, data and result.json")
    p.add_argument("--write-scenario", help="write the AZ_E2E trajectory scenario here")
    p.add_argument("--frames", type=int, default=14, help="scenario: frames after the click")
    p.add_argument("--parse-log", help="print the trajectory of an AZ_E2E run's log")
    args = p.parse_args()
    if args.write_scenario:
        ok = write_scenario(args)
    elif args.parse_log:
        ok = parse_log(args)
    else:
        if not args.out:
            p.error("live mode needs --out")
        os.makedirs(args.out, exist_ok=True)
        import azlin_e2e as e2e
        try:
            ok = live(args)
        except e2e.Failure as e:
            print("[%s] FAIL: %s" % (args.app, e), flush=True)
            ok = False
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
