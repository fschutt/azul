#!/usr/bin/env python3
"""AzDrive beside its design: the states of the HTML prototype (azul-apps
design/azdrive-ui/shots, the table of SPEC.md section 7) in the running app at the prototype's
1440 x 900, each screenshot saved beside the prototype's picture, and the SPEC's numbers measured
from the app's layout.

    python3 scripts/azdrive_visual_pairs.py --bin target/release/AzDrive \\
        --design ../azul-apps/design/azdrive-ui --out <folder>

Writes <out>/<state>-azul.png beside <out>/<state>-design.jpg for every state, and
<out>/numbers.txt: each SPEC number, the app's value and whether it holds (within 0.5 px).
It exits 1 when a number does not hold or a state cannot be shown. AzDrive runs headless under
its debug server (scripts/azdrive_e2e.py's driver) on a temporary Home with the sample files, in
the flora theme the prototype draws.

The pictures are for a person to compare (the prototype's files are its own sample data); the
numbers are what the script judges:

    SPEC 2.2  the tab buttons are 22 px tall and 6 px apart
    SPEC 2.6  Back and Forward are 26 px round buttons; the crumbs field is 28 px tall
    SPEC 2.7  the navigation pane's rows are 25 px; the pies' hit areas 15 x 25
    SPEC 2.8  the details rows are 25 px
"""

import argparse
import os
import shutil
import sys
import tempfile
import time

import azdrive_e2e as az
import azlin_e2e as e2e
from azlin_e2e import Failure

# (state, the prototype's picture, how to reach it from the window at rest on Documents).
STATES = [
    ("light", "light.jpg", "rest"),
    ("after-live", "after-live.jpg", "home-open"),
    ("view-tab", "view-tab.jpg", "view-open"),
    ("tab-overview", "tab-overview.jpg", "tab-overview"),
    ("cmdk", "cmdk.jpg", "palette"),
    ("searchB", "searchB.jpg", "search"),
    ("sync-popover", "sync-popover.jpg", "sync-popover"),
    ("dark", "dark.jpg", "rest"),
    ("after-live-dark", "after-live-dark.jpg", "home-open"),
]

# The numbers of SPEC.md this script measures, in px.
TAB_BUTTON_HEIGHT = 22.0
TAB_BUTTON_GAP = 6.0
ROUND_BUTTON = 26.0
CRUMBS_FIELD_HEIGHT = 28.0
NAV_ROW = 25.0
PIE = (15.0, 25.0)
DETAILS_ROW = 25.0
TOLERANCE = 0.5


def log(line):
    print("[azdrive-visual] %s" % line, flush=True)


def rect(app, node=None, selector=None):
    """The laid-out box of `node` (DOM 0) or of the node `selector` names."""
    params = {"node_id": node} if node is not None else {"selector": selector}
    value = app.value("get_node_layout", **params) or {}
    r = value.get("rect") or {}
    return {k: float(r.get(k, 0.0)) for k in ("x", "y", "width", "height")}


class Numbers:
    """The SPEC numbers measured: each with the app's value and its verdict."""

    def __init__(self):
        self.rows = []

    def check(self, what, spec, got):
        ok = abs(got - spec) <= TOLERANCE
        self.rows.append((what, spec, got, ok))
        log("%-48s spec %6.1f  app %6.1f  %s" % (what, spec, got, "PASS" if ok else "OFF"))

    def failed(self):
        return [r for r in self.rows if not r[3]]

    def write(self, path):
        with open(path, "w", encoding="utf-8") as f:
            f.write("# what\tspec px\tapp px\tverdict\n")
            for what, spec, got, ok in self.rows:
                f.write("%s\t%.1f\t%.1f\t%s\n" % (what, spec, got, "PASS" if ok else "OFF"))


def measure(app, numbers, mode):
    """The SPEC numbers of the window as it is (Documents in Details, at rest)."""
    tabs = [app.ribbon_node(name) for name in ("Home", "Share", "View")]
    if None in tabs:
        raise Failure("the ribbon's Home, Share and View tabs are not all laid out")
    boxes = [rect(app, node=n) for n in tabs]
    for name, box in zip(("Home", "Share", "View"), boxes):
        numbers.check("%s: the %s tab button's height" % (mode, name), TAB_BUTTON_HEIGHT,
                      box["height"])
    for (a, box_a), (b, box_b) in zip(zip(("Home", "Share"), boxes), zip(("Share", "View"),
                                                                       boxes[1:])):
        numbers.check("%s: the gap from %s to %s" % (mode, a, b), TAB_BUTTON_GAP,
                      box_b["x"] - (box_a["x"] + box_a["width"]))
    arrows = app.nodes_with_class(az.BAR_NAV)
    if len(arrows) < 2:
        raise Failure("the address row has no Back and Forward")
    for name, node in zip(("Back", "Forward"), arrows[:2]):
        box = rect(app, node=node)
        numbers.check("%s: %s's width" % (mode, name), ROUND_BUTTON, box["width"])
        numbers.check("%s: %s's height" % (mode, name), ROUND_BUTTON, box["height"])
    field = app.nodes_with_class(az.BAR_BOX)
    if not field:
        raise Failure("the address row has no crumbs field")
    numbers.check("%s: the crumbs field's height" % mode, CRUMBS_FIELD_HEIGHT,
                  rect(app, node=field[0])["height"])
    for row in ("side-this-pc", "side-drive-home", "side-fav-documents"):
        numbers.check("%s: the navigation row %s's height" % (mode, row), NAV_ROW,
                      rect(app, selector="#" + az.I(row))["height"])
    for pie in ("sync-pie", "space-pie"):
        box = rect(app, selector="#" + az.I(pie))
        numbers.check("%s: the %s's width" % (mode, pie), PIE[0], box["width"])
        numbers.check("%s: the %s's height" % (mode, pie), PIE[1], box["height"])
    found = az.item_node(app, "notes.txt")
    if found is None:
        raise Failure("Documents shows no notes.txt row")
    dom, node = found
    parents = {n["index"]: n.get("parent") for n in app.hierarchy_of(dom)}
    row = parents.get(node)
    row_box = (app.value("get_node_layout", node_id=row, dom_id=dom) or {}).get("rect") or {}
    numbers.check("%s: a details row's height" % mode, DETAILS_ROW,
                  float(row_box.get("height", 0.0)))


def reach(app, how, out):
    """Brings the window from rest (Documents, notes.txt selected, the tab content closed) to
    the state `how`; returns the window to screenshot (a popover is a window of its own) and
    the step that takes it back to rest."""
    if how == "rest":
        return app, lambda: None
    if how in ("home-open", "view-open"):
        name = "Home" if how == "home-open" else "View"
        node = app.until("the %s tab" % name, lambda: app.ribbon_node(name))
        app.press("double_click", 0, node, "the %s tab" % name)
        app.until("the %s tab's content locked open" % name, app.content_shows)

        def close():
            again = app.until("the %s tab" % name, lambda: app.ribbon_node(name))
            app.press("double_click", 0, again, "the %s tab" % name)
            app.until("the tab content closed", lambda: not app.content_shows())
        return app, close
    if how == "tab-overview":
        for _ in range(2):
            app.key("t", ctrl=True)
        app.until("three tabs", lambda: (app.last("AZDRIVE_TABS") or "").startswith("3 "))
        app.after("the tab overview", "AZDRIVE_TAB_OVERVIEW", r"open",
                  lambda: app.key("a", ctrl=True, shift=True))

        def close():
            app.after("the overview closed", "AZDRIVE_TAB_OVERVIEW", r"closed",
                      lambda: app.key("escape"))
            for _ in range(2):
                app.key("w", ctrl=True)
            app.until("one tab again", lambda: (app.last("AZDRIVE_TABS") or "").startswith("1 "))
        return app, close
    if how == "palette":
        app.after("the palette", "AZDRIVE_PALETTE", r"open", lambda: app.key("k", primary=True))
        app.until("the palette laid out", lambda: app.has("#" + az.I("palette")))
        app.must("text_input", text="gard")
        app.frame(3)

        def close():
            app.after("the palette closed", "AZDRIVE_PALETTE", r"closed",
                      lambda: app.key("escape"))
        return app, close
    if how == "search":
        az.focus_search_box(app)
        app.after("a search", "AZDRIVE_SEARCHED", r"\d+ \w+ report",
                  lambda: (app.must("text_input", text="report"), app.frame(2)))

        def close():
            az.close_search_view(app)
        return app, close
    if how == "sync-popover":
        before = len(app.window_ids())
        app.click(selector="#" + az.I("sync-pie"))
        pop = e2e.modal_window(app)
        pop.until("the sync popover", lambda: pop.has("#" + az.I("sync-popover")))

        def close():
            pop.key("escape")
            app.until("the popover closed", lambda: len(app.window_ids()) <= before)
        return pop, close
    raise Failure("no way to reach %r" % how)


def run_mode(args, logs, mode, numbers):
    """One AzDrive in `mode` (light, dark): its states and its numbers."""
    home = os.path.join(logs, mode, "home")
    os.makedirs(home)
    switches = [
        "--sample", "--theme", "flora", "--mode", mode, "--language", "en",
        "--size", "1440x900", "--home", home, "--open", "Home/Documents",
        "--layout", "details",
        "--downloads", os.path.join(logs, mode, "downloads"),
        "--data-dir", os.path.join(logs, mode, "data"),
        "--drives", os.path.join(logs, mode, "config", "drives.json"),
        "--dialogs", "inline",
        "--cache-dir", os.path.join(logs, mode, "cache"),
    ]
    app = az.Drive("azdrive-visual-" + mode, args.bin, switches, args.debug_port, logs,
                   args.timeout)
    missing = []
    try:
        app.until("Documents", lambda: app.printed("AZDRIVE_PLACE", r"home Documents/"))
        app.until("the debug server", lambda: app.op("get_dom_tree"))
        app.must("resize", width=1440.0, height=900.0)
        app.frame(3)
        app.until("Documents' rows", lambda: "notes.txt" in az.item_names(app))
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: az.select_item(app, "notes.txt"))
        app.settle(limit=2.0)
        measure(app, numbers, mode)
        for state, picture, how in STATES:
            wanted_dark = state.endswith("dark") or state == "dark"
            if wanted_dark != (mode == "dark"):
                continue
            design = os.path.join(args.design, "shots", picture)
            if not os.path.isfile(design):
                missing.append("%s (no %s)" % (state, design))
                continue
            try:
                window, back = reach(app, how, args.out)
                window.screenshot(os.path.join(args.out, "%s-azul.png" % state))
                shutil.copyfile(design, os.path.join(args.out, "%s-design.jpg" % state))
                log("%s: %s-azul.png beside %s" % (state, state, picture))
                back()
            except Failure as e:
                missing.append("%s (%s)" % (state, e))
    finally:
        app.stop()
    return missing


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--bin", help="AzDrive (default: target/release/AzDrive)")
    parser.add_argument("--design", required=True, help="azul-apps design/azdrive-ui")
    parser.add_argument("--out", required=True, help="the folder of the pairs")
    parser.add_argument("--debug-port", type=int, default=8791)
    parser.add_argument("--timeout", type=int, default=240)
    args = parser.parse_args(argv)
    args.bin = e2e.find_binary("AzDrive", args.bin, "AZDRIVE_BIN")
    os.makedirs(args.out, exist_ok=True)
    logs = tempfile.mkdtemp(prefix="azdrive-visual-")
    numbers = Numbers()
    missing = []
    try:
        for mode in ("light", "dark"):
            missing += run_mode(args, logs, mode, numbers)
    except Failure as e:
        log("FAIL: %s (logs in %s)" % (e, logs))
        return 1
    numbers.write(os.path.join(args.out, "numbers.txt"))
    off = numbers.failed()
    for state in missing:
        log("NOT SHOWN: %s" % state)
    if off or missing:
        log("FAIL: %d numbers off, %d states not shown (logs in %s)" % (len(off), len(missing),
                                                                      logs))
        return 1
    shutil.rmtree(logs, ignore_errors=True)
    log("PASS: %d pairs and %d numbers in %s" % (len(STATES), len(numbers.rows), args.out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
