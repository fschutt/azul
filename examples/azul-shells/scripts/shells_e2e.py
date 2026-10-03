#!/usr/bin/env python3
"""AzShells end to end: every shell over the debug server.

    1. starts AzShells headless (AZ_BACKEND=headless, the debug server on --debug-port) on a
       fresh data folder (`--data-dir`) and sizes the window to --width x --height;
    2. the app fills its window: the shell area reaches the window's bottom edge (no UA body
       margin, no content-high body - the 2026-10-03 LOOK);
    3. for each of S1..S11 (and the settings layout): clicks its picker segment, reads the
       slot ids the app prints (`AZSHELLS_SLOTS <n> id,id,...`) and the F6 cycle
       (`AZSHELLS_PANES <n> id,id,...`), asks `get_node_layout` for every slot and asserts the
       rectangle lies inside the window, that every pane has a height (S5's had none), that the
       row's panes do not overlap; presses F6 (the first pane takes the focus), F6 (the second),
       Shift+F6 (back to the first) and checks `get_focus_state` against the pane nodes, then
       takes a screenshot;
    4. opens the command palette with Ctrl/Cmd+K on S4, asserts it is in the tree, closes it with
       Escape;
    5. Flora and Dark by the picker's buttons: the theme marker, a screenshot; both are kept in
       shells/settings.json; the gear opens azul-appkit's settings page, its About section the
       standard About box (`AZSHELLS_ABOUT open`), Escape closes the box, Escape the page;
    6. a second start opens on S4 (the shell shown last) in flora / dark (remembered).

Usage (from the azul repository, after building libazul with the debug server and AzShells; one
app at a time, through the capped runner):

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/shells.log -- \\
      python3 examples/azul-shells/scripts/shells_e2e.py --bin target/release/AzShells --out /tmp/shells

`AZSHELLS_BIN` also names the binary. The debug-server client is the shared one
(scripts/azlin_e2e.py).
"""

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.abspath(os.path.join(HERE, "..", "..", "..", "scripts")))

import azlin_e2e as e2e  # noqa: E402
from azlin_e2e import Failure  # noqa: E402

TAG = "shells"
PICKS = ["S1", "S2", "S3", "S4", "S5", "S6", "S7", "S8", "S9", "S10", "S11", "Settings"]
AREA = "__azshells_area"
WIDTH, HEIGHT = 1100, 720


def node_and_rect(app, node_id):
    value = app.value("get_node_layout", selector="#%s" % node_id)
    return value.get("node_id"), value.get("rect") or {}


def focused(app):
    value = app.value("get_focus_state", seat=0)
    node = value.get("focused_node") or {}
    return node.get("node_id"), node.get("selector") or ""


def classes(app):
    out = set()
    for d in e2e.dicts(app.op("get_node_hierarchy")):
        for key in ("classes", "class"):
            v = d.get(key)
            if isinstance(v, list):
                out.update(x for x in v if isinstance(x, str))
            elif isinstance(v, str):
                out.update(v.split())
    return out


def inside(rect):
    x, y = float(rect.get("x", -1)), float(rect.get("y", -1))
    w, h = float(rect.get("width", 0)), float(rect.get("height", 0))
    return w > 0 and h > 0 and x >= -0.5 and y >= -0.5 and x + w <= WIDTH + 0.5 and y + h <= HEIGHT + 0.5


def overlap(a, b):
    ax, ay, aw, ah = (float(a.get(k, 0)) for k in ("x", "y", "width", "height"))
    bx, by, bw, bh = (float(b.get(k, 0)) for k in ("x", "y", "width", "height"))
    return min(ax + aw, bx + bw) - max(ax, bx) > 1.0 and min(ay + ah, by + bh) - max(ay, by) > 1.0


def check_shell(app, n, pick, out):
    """Picks shell `pick` (index n, 1-based) and checks its slots, panes and F6."""
    before = len(app.printed("AZSHELLS_SHELL", r"\d+"))
    app.click(text=pick)
    app.until("the switch to %s" % pick, lambda: len(app.printed("AZSHELLS_SHELL", r"\d+")) > before)
    app.frame(2)
    slots_lines = app.printed("AZSHELLS_SLOTS", r"%d [^\n]+" % n)
    panes_lines = app.printed("AZSHELLS_PANES", r"%d [^\n]*" % n)
    if not slots_lines:
        raise Failure("%s: the app printed no AZSHELLS_SLOTS line" % pick)
    slots = slots_lines[-1].split(" ", 1)[1].split(",")
    panes = [p for p in (panes_lines[-1].split(" ", 1)[1].split(",") if panes_lines else []) if p]
    app.log("%s: slots %s, F6 cycle %s" % (pick, slots, panes))

    rects = {}
    for slot in slots:
        node, rect = node_and_rect(app, slot)
        if node is None:
            raise Failure("%s: slot #%s is not in the tree" % (pick, slot))
        if not inside(rect):
            raise Failure("%s: slot #%s lies outside the %dx%d window: %s" % (pick, slot, WIDTH, HEIGHT, rect))
        rects[slot] = (node, rect)

    # Every pane has a height: a content-high body collapsed S5's panes to nothing.
    for p in panes:
        if p in rects and float(rects[p][1].get("height", 0)) < 40.0:
            raise Failure("%s: pane #%s has no height: %s" % (pick, p, rects[p][1]))

    # The row's panes (the F6 cycle minus the bottom pane) never overlap.
    row = [p for p in panes if p in rects]
    for i, a in enumerate(row):
        for b in row[i + 1:]:
            if overlap(rects[a][1], rects[b][1]):
                raise Failure("%s: panes #%s and #%s overlap: %s / %s" % (pick, a, b, rects[a][1], rects[b][1]))

    # F6 walks the panes: first, second, and Shift+F6 back.
    if len(panes) >= 2:
        app.click(selector="#%s" % panes[-1], frames=1)
        focused_before = len(app.printed("AZSHELLS_PANE", r"\d+"))
        app.key("F6")
        app.until("F6 to reach a pane", lambda: len(app.printed("AZSHELLS_PANE", r"\d+")) > focused_before)
        first_node, first_sel = focused(app)
        app.key("F6")
        second_node, second_sel = focused(app)
        app.key("F6", shift=True)
        back_node, back_sel = focused(app)
        pane_nodes = [rects[p][0] for p in panes if p in rects]
        if first_node not in pane_nodes:
            raise Failure("%s: F6 focused %s (%s), not a pane of %s" % (pick, first_node, first_sel, panes))
        if second_node not in pane_nodes or second_node == first_node:
            raise Failure("%s: the second F6 focused %s (%s)" % (pick, second_node, second_sel))
        if back_node != first_node:
            raise Failure("%s: Shift+F6 focused %s (%s), not the first pane again" % (pick, back_node, back_sel))
        app.log("%s: F6 %s -> %s, Shift+F6 -> %s" % (pick, first_sel, second_sel, back_sel))

    app.screenshot(os.path.join(out, "%s.png" % pick.lower()))


def fills_the_window(app):
    _, area = node_and_rect(app, AREA)
    bottom = float(area.get("y", 0)) + float(area.get("height", 0))
    if abs(bottom - HEIGHT) > 0.5 or float(area.get("x", 1)) > 0.5:
        raise Failure("the shell area does not reach the window's edges (%dx%d): %s" % (WIDTH, HEIGHT, area))
    app.log("the shell area fills the window down to y %.1f" % bottom)


def body(args, logs, out):
    binary = e2e.find_binary("AzShells", args.bin, "AZSHELLS_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)

    def start(name):
        app = e2e.App(name, binary, ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT)],
                      args.debug_port, logs, args.timeout)
        app.until("AzShells' window", lambda: app.shows("S1"))
        app.must("resize", width=WIDTH, height=HEIGHT)
        app.frame(2)
        return app

    app = start("shells")
    try:
        fills_the_window(app)
        for n, pick in enumerate(PICKS, start=1):
            check_shell(app, n, pick, out)

        # The command palette: Ctrl/Cmd+K opens it over S4, Escape closes it.
        app.click(text="S4")
        app.key("k", primary=True)
        app.until("the command palette", lambda: "__azul-native-command-palette-panel" in classes(app))
        app.screenshot(os.path.join(out, "s4-palette.png"))
        app.key("escape")
        app.until("the palette to close", lambda: "__azul-native-command-palette-panel" not in classes(app))

        # Flora and dark by the picker's buttons: in effect and kept.
        saved = len(app.printed("AZSHELLS_SETTINGS_SAVED"))
        app.click(text="Flora")
        app.click(text="Dark", frames=3)
        app.until("the flora marker", lambda: "__azul-theme-flora" in classes(app))
        app.until("both choices saved", lambda: len(app.printed("AZSHELLS_SETTINGS_SAVED")) >= saved + 2)
        with open(os.path.join(data_dir, "shells", "settings.json"), "r", encoding="utf-8") as f:
            settings = json.load(f)
        if settings.get("theme") != "flora" or settings.get("mode") != "dark":
            raise Failure("shells/settings.json does not hold flora / dark: %s" % settings)
        app.screenshot(os.path.join(out, "s4-flora-dark.png"))

        # The gear: azul-appkit's settings page; About: the standard About box.
        app.click(selector="#__azshells_settings")
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        app.click(text="About")
        app.until("the About section", lambda: app.has_id("appkit-about-open"))
        app.click(selector="#appkit-about-open")
        app.expect_line("AZSHELLS_ABOUT", "open", "the About box opens")
        app.screenshot(os.path.join(out, "about-flora-dark.png"))
        app.key("escape")
        app.expect_line("AZSHELLS_ABOUT", "closed", "Escape closes the About box first")
        app.key("escape")
        app.until("the settings page to close", lambda: not app.has_id("appkit-settings"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # A second start opens on the shell shown last, in the kept theme and mode.
    app = start("remembered")
    try:
        if app.printed("AZSHELLS_SHELL", r"\d+")[:1] != ["4"]:
            raise Failure("the second start did not open on S4: %s" % app.printed("AZSHELLS_SHELL", r"\d+"))
        if "__azul-theme-flora" not in classes(app):
            raise Failure("the second start is not in the kept flora theme")
        mode = app.value("get_mode")
        if not isinstance(mode, dict) or mode.get("mode") != "dark":
            raise Failure("the second start is not in the kept dark mode: %s" % mode)
        app.screenshot(os.path.join(out, "s4-remembered.png"))
    finally:
        app.stop()
    app.log("PASS: %d shells checked, settings remembered; screenshots in %s" % (len(PICKS), out))
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8771)
