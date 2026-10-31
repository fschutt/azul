#!/usr/bin/env python3
"""ANIM8 probe: do CSS transitions animate in a prebuilt AzWidgets? (wave 8)

Starts AzWidgets headless with its debug server and samples, frame by frame:
  1. the Switch: clicks the track, then reads the knob's x and the track's
     background for ~12 frames (a slide shows intermediate x values, a jump shows
     the end value at once);
  2. a Button: moves the mouse over it and reads its background for ~12 frames
     (a hover fade shows intermediate colours).
Plus `get_animations` (active slides / css_transitions) after each change.

Run ONE app at a time, through the capped runner:

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 120 --log /tmp/anim8.log -- \\
      python3 scripts/anim8_probe.py --bin target/release/AzWidgets
"""

import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as e2e  # noqa: E402
from azlin_e2e import Failure  # noqa: E402


def css_of(app, node_id, names):
    answer = app.op("get_node_css_properties", node_id=node_id)
    data = answer.get("data") if isinstance(answer, dict) else None
    value = data.get("value") if isinstance(data, dict) else data
    found = {}
    text = json.dumps(value)
    if isinstance(value, dict):
        props = value.get("properties") or value.get("computed") or value
        if isinstance(props, list):
            for p in props:
                if isinstance(p, dict):
                    k = p.get("property") or p.get("name")
                    if k in names:
                        found[k] = p.get("value")
                elif isinstance(p, str):
                    for n in names:
                        if p.startswith(n + ":"):
                            found[n] = p.split(":", 1)[1].strip()
        elif isinstance(props, dict):
            for n in names:
                if n in props:
                    found[n] = props[n]
    if not found:
        found["raw"] = text[:300]
    return found


def rect_of(app, node_id):
    value = app.value("get_node_layout", node_id=node_id)
    r = (value or {}).get("rect") or {}
    return (round(float(r.get("x", 0)), 2), round(float(r.get("y", 0)), 2))


def animations(app):
    v = app.value("get_animations")
    if isinstance(v, dict):
        return {k: v.get(k) for k in ("active", "transitions", "zombies", "live_tracks")}
    return v


def pixel(app, out, name, x, y):
    """The screenshot's colour at logical (x, y) (PIL)."""
    from PIL import Image
    path = os.path.join(out, name)
    app.screenshot(path)
    img = Image.open(path).convert("RGB")
    return img.getpixel((int(x), int(y)))


def body(args, logs, out):
    binary = e2e.find_binary("AzWidgets", args.bin, "AZWIDGETS_BIN")
    app = e2e.App("widgets", binary, [], args.debug_port, logs, args.timeout)
    try:
        app.until("the Switch caption", lambda: app.shows("Switch"))
        app.must("resize", width=900.0, height=1300.0)
        app.frame(3)
        hier = app.hierarchy()
        tracks = [n for n in hier if any("switch" in c for c in (n.get("classes") or []))]
        app.log("switch-classed nodes: %s" % [(n["index"], n.get("classes")) for n in tracks][:6])
        track = None
        for n in tracks:
            if n.get("children"):
                track = n
                break
        if track is None:
            raise Failure("no switch track found")
        knob = track["children"][0]
        app.log("track %s knob %s" % (track["index"], knob))
        app.log("before: knob %s track %s anims %s" % (
            rect_of(app, knob), css_of(app, track["index"], ["background", "background-content"]),
            animations(app)))
        app.must("click", node_id=track["index"])
        t0 = time.time()
        for i in range(14):
            kx, ky = rect_of(app, knob)
            app.log("switch +%d (%.0f ms): knob %s anims %s track px %s" % (
                i, (time.time() - t0) * 1000, (kx, ky), animations(app),
                pixel(app, out, "sw%02d.png" % i, kx - 1, ky + 8)))
            app.frame(1)
        app.screenshot(os.path.join(out, "switch-after.png"))

        buttons = [n for n in hier if any(c.endswith("button") or "__azul-btn" in c or "button" in c
                                          for c in (n.get("classes") or []))]
        app.log("button-ish nodes: %s" % [(n["index"], n.get("classes")) for n in buttons][:8])
        if buttons:
            b = buttons[0]
            value = app.value("get_node_layout", node_id=b["index"])
            r = value.get("rect") or {}
            cx = float(r.get("x", 0)) + float(r.get("width", 0)) / 2
            cy = float(r.get("y", 0)) + float(r.get("height", 0)) / 2
            px, py = float(r.get("x", 0)) + 4, cy
            app.log("button %s at %s; before px %s" % (b["index"], r, pixel(app, out, "hb.png", px, py)))
            app.must("mouse_move", x=cx, y=cy)
            t0 = time.time()
            for i in range(14):
                app.log("hover +%d (%.0f ms): anims %s px %s" % (
                    i, (time.time() - t0) * 1000, animations(app),
                    pixel(app, out, "h%02d.png" % i, px, py)))
                app.frame(1)
        return True
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run("anim8", body, default_port=8791, binary_name="AzWidgets")
