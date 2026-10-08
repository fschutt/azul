#!/usr/bin/env python3
"""CODESCROLL13 scroll probe: what does one wheel notch over AzCode's code view cost,
next to one over AzWidgets' page? (2026-10-08, "AzCode is heavily lagging when scrolling
in the code view, while AzWidgets is perfectly smooth".)

Starts ONE app headless with its debug server at 1280x800, wheels 40 notches of 100 px over
the scroll target and reports the round trip of each notch (the `wheel` op and a
`wait_frame`), the frame counters over the burst (`dom_regenerations` - the number that
matters: AzCode used to rebuild the window on every notch, AzWidgets' page never does -
`layout_passes`, `dl_rebuilds`) and the first line in view before and after. Headless, every
repaint is a relayout of the existing DOM (an idle frame is printed for comparison), so
the absolute numbers are higher than on a desktop shell; compare the two apps, and a build
before and after a change.

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 300 --log /tmp/scroll.log -- \\
      python3 scripts/azcode_scroll_probe.py --bin target/release/AzCode [--file huge.rs]
    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 300 --log /tmp/scroll.log -- \\
      python3 scripts/azcode_scroll_probe.py --bin target/release/AzWidgets --widgets

AzCode opens its sample workspace (`--sample`, data in a throwaway folder) and the file
`--file` (default huge.rs, 100,001 lines); `AZ_PROFILE=cpu` in the environment adds the
per-phase dumps to the app's stderr (in the logs folder the script prints).
"""

import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as e2e  # noqa: E402
from azlin_e2e import Failure  # noqa: E402

NOTCHES = 40


def listed(app, folder):
    for line in reversed(app.printed("AZCODE_LISTED")):
        name, _, n = line.rpartition(" ")
        if name == folder:
            return int(n)
    return None


def click_row(app, key):
    safe = "".join(c if (c.isascii() and c.isalnum()) or c in "_-" else "-" for c in key)
    row = "#__azcode_tree-" + safe
    app.until("the row of %s" % key, lambda: app.has(row, every_dom=True))
    app.click(selector=row, every_dom=True)


def first_line(app):
    """The first line number in view: the code view's gutter, in whichever DOM it is."""
    numbers = []
    for dom in app.dom_ids():
        nodes = [d for d in e2e.dicts(app.op("get_node_hierarchy", dom_id=dom))
                 if "index" in d and "type" in d]
        by_index = {n["index"]: n for n in nodes}
        for n in nodes:
            parent = by_index.get(n.get("parent"))
            grand = by_index.get(parent.get("parent")) if parent else None
            if (n.get("text") or "").strip().isdigit() and grand and \
                    "__azul-native-code-view-gutter" in (grand.get("classes") or []):
                numbers.append(int(n["text"]))
    return min(numbers) if numbers else None


def burst(app, x, y):
    """NOTCHES notches at (x, y): the round trip of each, and the frame counters."""
    app.must("reset_frame_counters")
    times = []
    for _ in range(NOTCHES):
        t0 = time.time()
        app.must("wheel", x=x, y=y, delta_x=0.0, delta_y=-100.0)
        app.must("wait_frame")
        times.append((time.time() - t0) * 1000.0)
    report = app.value("get_frame_report")
    ordered = sorted(times)
    app.log("%d notches: median %.1f ms, p90 %.1f ms, max %.1f ms" % (
        NOTCHES, ordered[len(ordered) // 2], ordered[int(len(ordered) * 0.9) - 1], ordered[-1]))
    keys = ("dom_regenerations", "layout_passes", "dl_rebuilds", "frames_since_reset")
    app.log("frame counters over the burst: %s" % json.dumps({k: report.get(k) for k in keys}))
    t0 = time.time()
    for _ in range(10):
        app.must("wait_frame")
    app.log("an idle frame: %.1f ms" % ((time.time() - t0) * 100.0))


def code(args, logs, out):
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    binary = e2e.find_binary("AzCode", args.bin, "AZCODE_BIN")
    app = e2e.App("azcode-scroll", binary, ["--sample", "--data-dir", data_dir, "--size",
                                             "1280x800", "--theme", "flat", "--mode", "light"],
                  args.debug_port, logs, args.timeout)
    try:
        app.until("the window", lambda: app.printed("AZCODE_READY", r".*"))
        app.until("the sample workspace", lambda: (listed(app, "/") or 0) >= 4)
        if "/" in args.file:
            click_row(app, args.file.split("/")[0] + "/")
        click_row(app, args.file)
        app.until("%s opened" % args.file,
                  lambda: app.printed("AZCODE_OPENED", re.escape(args.file) + r" \d+"))
        app.frame(3)
        app.must("wait_settled")
        box = app.rect("__azcode_editor")
        x, y = box["x"] + box["width"] / 2.0, box["y"] + box["height"] / 2.0
        app.log("first line in view: %s" % first_line(app))
        burst(app, x, y)
        app.log("first line in view after the burst: %s" % first_line(app))
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


def widgets(args, logs, out):
    binary = e2e.find_binary("AzWidgets", args.bin)
    app = e2e.App("azwidgets-scroll", binary, [], args.debug_port, logs, args.timeout)
    try:
        app.until("the window", lambda: len(app.hierarchy()) > 50)
        app.must("resize", width=1280, height=800)
        app.frame(5)
        app.must("wait_settled")
        # The page under the toolbar.
        burst(app, 640.0, 450.0)
        states = app.value("get_scroll_states").get("scroll_states") or []
        app.log("the page scrolled to %s px" % max((s.get("scroll_y") or 0.0) for s in states))
        return True
    finally:
        app.stop()


def body(args, logs, out):
    return (widgets if args.widgets else code)(args, logs, out)


if __name__ == "__main__":
    # The two switches of this probe, before the shared ones are parsed.
    argv = sys.argv[1:]
    file_name = "huge.rs"
    if "--file" in argv:
        i = argv.index("--file")
        file_name = argv[i + 1]
        del argv[i:i + 2]
    use_widgets = "--widgets" in argv
    argv = [a for a in argv if a != "--widgets"]

    def run_body(args, logs, out):
        args.file = file_name
        args.widgets = use_widgets
        return body(args, logs, out)

    e2e.run("azcodescroll", run_body, argv=argv, default_port=8797)
