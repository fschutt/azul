#!/usr/bin/env python3
"""WP-E2's audit pictures: AzMaps and AzCode as they are, in the host's headless window (CPU render), named.

    python3 scripts/wp_e2_audit.py maps|code --bin <the app's QA build> --out <dir> [--debug-port 8791] [--timeout 900]

One run of one app, over the engine's debug server (scripts/azlin_e2e.py), walks the window through the sizes and
states the card of the app asks about (the card: briefs/app_refactor_2026-11-28.md section 6) and writes one PNG
per state, named `<app>-<form factor>-<NN>-<what>.png`, and, for the phone-sized ones, the phone-fit audit of the
same moment (scripts/ci/layout_audit.py: boxes over the window's edges, tappable nodes under 48 dp, text crossing an
edge) as `phone-fit.md`. A step that fails is written down (`audit-steps.txt`) and the walk goes on: the audit is
about what the pictures show, a missing control is a finding, not a stop.

The same walk is what the scenario files of the refactor (examples/azul-maps/e2e, examples/azul-code/e2e) shoot
again after it, as the AFTER side of each pair; the sizes are the acceptance sizes of the brief (section 4).
"""

import argparse
import json
import os
import sys
import time
import traceback

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ci"))

import azlin_e2e as e2e  # noqa: E402
from azlin_e2e import Failure  # noqa: E402

try:
    import layout_audit as la  # noqa: E402
except ImportError:  # the audit is a bonus: the pictures are the point
    la = None

# (name, width, height, display preset): the acceptance sizes of the brief, in logical px.
DESKTOP = ("desktop", 1280, 800, "off")
PIXEL8 = ("phone", 411, 914, "pixel8")
PIXEL8_LANDSCAPE = ("phone-landscape", 914, 411, "pixel8")
IPHONE = ("iphone", 402, 874, "iphone_island")
SMALL = ("small", 360, 640, "off")


class Walk:
    """The named pictures of one app, and what became of each step."""

    def __init__(self, app, tag, out):
        self.app = app
        self.tag = tag
        self.out = out
        os.makedirs(out, exist_ok=True)
        self.count = 0
        self.steps = []
        self.audits = []
        self.size = (0, 0)

    def step(self, what, fn):
        """Runs `fn`; a failure is noted and the walk goes on. True when it worked."""
        started = time.time()
        try:
            fn()
            self.steps.append("ok    %-58s %5.1fs" % (what, time.time() - started))
            return True
        except Failure as e:
            self.steps.append("FAIL  %-58s %s" % (what, str(e)[:240].replace("\n", " ")))
        except Exception as e:  # noqa: BLE001 - a driver bug must not lose the pictures that were taken
            self.steps.append("ERROR %-58s %s" % (what, "".join(traceback.format_exception_only(type(e), e)).strip()[:240]))
        self.app.log(self.steps[-1])
        if not self.app.alive():
            raise Failure("%s exited during the audit" % self.tag)
        return False

    def window(self, form):
        """The window as `form` (name, width, height, preset): the size and the display preset the simulated phone has."""
        name, width, height, preset = form
        self.size = (width, height)
        self.app.must("set_display_preset", preset=preset)
        self.app.must("resize", width=float(width), height=float(height))
        self.app.frame(3)
        self.app.must("wait_settled")

    def shot(self, form, what, audit=False):
        """One picture, `<tag>-<form>-<NN>-<what>.png`; `audit`: the phone-fit audit of this moment too."""
        self.count += 1
        name = "%s-%s-%02d-%s" % (self.tag, form[0], self.count, what)
        self.app.screenshot(os.path.join(self.out, name + ".png"))
        if audit and la is not None:
            self.phone_fit(form, name)

    def phone_fit(self, form, name):
        orient = "landscape" if form[1] > form[2] else "portrait"
        try:
            layout = self.app.op("get_all_nodes_layout")
            tree = self.app.op("get_accessibility_tree")
            state = self.app.op("get_state")
            scroll = self.app.op("get_scroll_states")
            result = la.audit(layout, tree, state, None, orient, scroll)
            self.audits.append((name, result))
            self.app.log("phone-fit %s: %s" % (name, la.summary_line(result)))
        except Exception as e:  # noqa: BLE001
            self.steps.append("ERROR phone-fit %s: %s" % (name, str(e)[:200]))

    def finish(self):
        with open(os.path.join(self.out, "audit-steps.txt"), "w", encoding="utf-8") as f:
            f.write("\n".join(self.steps) + "\n")
        if self.audits and la is not None:
            lines = ["# Phone-fit audit of the host window, %s" % self.tag, ""]
            for name, result in self.audits:
                lines += ["## " + name, "", "    " + la.summary_line(result), ""]
            lines.append("The full lists of each moment are in phone-fit.json.")
            with open(os.path.join(self.out, "phone-fit.md"), "w", encoding="utf-8") as f:
                f.write("\n".join(lines) + "\n")
            with open(os.path.join(self.out, "phone-fit.json"), "w", encoding="utf-8") as f:
                json.dump({name: result for name, result in self.audits}, f, indent=1)
        print("\n".join(self.steps), flush=True)


# ---- AzMaps ----------------------------------------------------------------------------------------------------------------------

MAP = "__azmaps_map"


def tiles_drawn(app):
    """The map's tile counter says something is drawn (`AZ_MAP_TILES <ready> <pending> <fetching> <failed> <drawn>`)."""
    line = app.last("AZ_MAP_TILES")
    if not line:
        return False
    try:
        return int(line.split()[-1]) > 0
    except ValueError:
        return False


def maps_walk(w, app):
    """AzMaps: the card says no ribbon, no tab row, the map fills the window; a panel on a desktop (search, results, then the place
    or the route), a search bar with chips and a sheet from the bottom on a phone, floating zoom / locate / layers."""
    app.until("AzMaps' window", lambda: app.has_id(MAP))
    app.frame(2)
    # The tiles come from the network; the world at zoom 0 and 1 is in the binary, so a picture without them still shows a map.
    try:
        app.until("a tile drawn", lambda: tiles_drawn(app), interval=0.5)
    except Failure as e:
        w.steps.append("note  no tile drawn in time (%s): the pictures show the shipped world tiles" % str(e)[:80])
    app.frame(2)

    def sidebar(open_):
        if app.has_id("__azmaps_sidebar") != open_:
            app.click(selector="#__azmaps_sidebar-toggle")
            app.until("the sidebar %s" % ("open" if open_ else "closed"), lambda: app.has_id("__azmaps_sidebar") == open_)
        app.frame(2)

    def typed(field, text, wait_for):
        app.text_input("#__azmaps_travel-" + field, text)
        app.until("the suggestions for %r" % text, lambda: app.has_id("__azmaps_suggestions"), interval=0.3)
        app.frame(2)

    def pick(index=0):
        app.click(selector="#__azmaps_suggestion-%d" % index)
        app.frame(3)

    def route(from_text, to_text):
        sidebar(True)
        typed("to", to_text, "to")
        pick()
        typed("from", from_text, "from")
        pick()
        # The route worker: the straight-line estimate at once, the road route once the routing server answers.
        try:
            app.until("a road route", lambda: app.printed("AZMAPS_ROAD", r".*"), interval=0.5)
        except Failure:
            w.steps.append("note  no road route in time: the picture shows the estimate")
        app.frame(3)

    # ---- the desktop ----
    d = DESKTOP
    w.window(d)
    app.must("set_mode", mode="light")
    sidebar(True)
    w.step("desktop first look", lambda: w.shot(d, "first-look-light"))
    w.step("desktop typeahead", lambda: (typed("to", "Tübingen", "to"), w.shot(d, "typeahead")))
    w.step("desktop destination picked", lambda: (pick(), w.shot(d, "destination-picked")))
    w.step("desktop route", lambda: (route("Stuttgart", "Tübingen"), w.shot(d, "route")))
    w.step("desktop route dark", lambda: (app.must("set_mode", mode="dark"), app.frame(3), w.shot(d, "route-dark")))
    app.must("set_mode", mode="light")

    def card():
        sidebar(True)
        app.click(selector="#" + MAP)
        app.until("a pin", lambda: app.has_id("__azmaps_place-0"))
        app.click(selector="#__azmaps_place-0")
        app.until("the card", lambda: app.has_id("__azmaps_place-directions-0", every_dom=True))
        app.frame(2)

    w.step("desktop place card", lambda: (card(), w.shot(d, "place-card")))
    w.step("desktop sidebar hidden", lambda: (sidebar(False), w.shot(d, "sidebar-hidden")))
    w.step("desktop settings", lambda: (app.click(selector="#__azmaps_settings"), app.until("settings", lambda: app.has_id("appkit-settings")), w.shot(d, "settings")))
    app.key("escape")
    app.frame(2)

    # ---- the phones ----
    for form in (PIXEL8, IPHONE, SMALL, PIXEL8_LANDSCAPE):
        w.window(form)
        app.must("set_mode", mode="light")
        w.step("%s sidebar closed" % form[0], lambda f=form: (sidebar(False), w.shot(f, "first-look", audit=True)))
        w.step("%s sidebar open" % form[0], lambda f=form: (sidebar(True), w.shot(f, "sidebar-open", audit=True)))
        if form is PIXEL8:
            w.step("phone typeahead", lambda f=form: (typed("to", "Stuttgart", "to"), w.shot(f, "typeahead", audit=True)))
            w.step("phone route", lambda f=form: (route("Tübingen", "Stuttgart"), w.shot(f, "route", audit=True)))
            w.step("phone place card", lambda f=form: (card(), w.shot(f, "place-card", audit=True)))
            w.step("phone dark", lambda f=form: (app.must("set_mode", mode="dark"), app.frame(3), w.shot(f, "dark", audit=True)))
            app.must("set_mode", mode="light")
        w.step("%s sidebar closed again" % form[0], lambda: sidebar(False))


# ---- AzCode ----------------------------------------------------------------------------------------------------------------------

def code_row(app, key):
    safe = "".join(c if (c.isascii() and c.isalnum()) or c in "_-" else "-" for c in key)
    row = "#__azcode_tree-" + safe
    app.until("the row of %s" % key, lambda: app.has(row, every_dom=True))
    app.click(selector=row, every_dom=True)


def code_walk(w, app):
    """AzCode: the card says an activity bar (explorer, search, git, run), the side bar with the explorer tree, the editor with file
    tabs, a bottom panel (terminal, problems) and a status bar; on a phone the editor is the screen, the explorer a drawer, the panel a
    bottom sheet, a key row above the keyboard (Tab, braces, arrows)."""
    app.until("the window", lambda: app.printed("AZCODE_READY", r".*"))
    app.until("the sample workspace", lambda: int((app.printed("AZCODE_LISTED", r".*") or ["x 0"])[-1].rpartition(" ")[2] or 0) >= 4)
    app.frame(3)

    def open_main():
        code_row(app, "src/")
        app.until("src listed", lambda: any(v.startswith("src/ ") for v in app.printed("AZCODE_LISTED")))
        code_row(app, "src/main.rs")
        app.until("main.rs opened", lambda: app.printed("AZCODE_OPENED", r"src/main\.rs \d+"))
        app.frame(3)

    def terminal():
        if not app.has_id("__azcode_panel"):
            app.click(text="Terminal")
        app.until("the terminal panel", lambda: app.has_id("__azcode_panel"))
        try:
            app.until("a shell", lambda: app.printed("AZCODE_TERMINAL_READY", r".*"), interval=0.5)
        except Failure:
            w.steps.append("note  the terminal panel has no shell after the wait")
        app.frame(3)

    def close_panel():
        if app.has_id("__azcode_panel"):
            app.click(text="Terminal")
            app.until("no panel", lambda: not app.has_id("__azcode_panel"))

    def search(text):
        app.click(selector="#__azcode_activity-search")
        app.frame(2)
        app.text_input("#__azcode_search-input", text)
        app.key("return")
        app.until("the search ran", lambda: app.printed("AZCODE_SEARCHED", r".*"), interval=0.4)
        app.frame(3)

    def explorer():
        # the explorer is the side bar again (the activity icon in front hides it, so only when something else shows)
        if not app.has_id("__azcode_explorer") and not app.has_id("__azcode_no-folder"):
            app.click(selector="#__azcode_activity-explorer")
            app.frame(2)

    d = DESKTOP
    w.window(d)
    w.step("desktop first look (welcome page)", lambda: w.shot(d, "first-look"))
    w.step("desktop file open", lambda: (open_main(), w.shot(d, "file-open")))
    w.step("desktop terminal", lambda: (terminal(), w.shot(d, "terminal")))
    w.step("desktop search", lambda: (search("counts"), w.shot(d, "search")))
    w.step("desktop explorer again", lambda: explorer())

    def find():
        app.click(selector="#__azcode_editor")
        app.key("f", primary=True)
        app.text_input("#__azcode_find-input", "counts")
        app.frame(2)

    w.step("desktop find bar", lambda: (find(), w.shot(d, "find")))
    app.key("escape")

    def palette():
        app.key("p", primary=True, shift=True)
        app.until("the command palette", lambda: app.has_id("__azcode_command-palette", every_dom=True))
        app.frame(2)

    w.step("desktop command palette", lambda: (palette(), w.shot(d, "command-palette")))
    app.key("escape")
    app.frame(2)
    w.step("desktop light", lambda: (app.must("set_mode", mode="light"), app.frame(3), w.shot(d, "light")))
    app.must("set_mode", mode="dark")

    # ---- the phones: the window as a phone is, whatever the app does with it ----
    for form in (PIXEL8, IPHONE, SMALL, PIXEL8_LANDSCAPE):
        w.window(form)
        w.step("%s editor" % form[0], lambda f=form: (close_panel(), w.shot(f, "editor", audit=True)))
        if form is PIXEL8:
            w.step("phone terminal", lambda f=form: (terminal(), w.shot(f, "terminal", audit=True)))
            w.step("phone search", lambda f=form: (close_panel(), search("counts"), w.shot(f, "search", audit=True)))
            w.step("phone explorer", lambda f=form: (app.click(selector="#__azcode_activity-explorer"), app.frame(2), w.shot(f, "explorer", audit=True)))
            w.step("phone light", lambda f=form: (app.must("set_mode", mode="light"), app.frame(3), w.shot(f, "light", audit=True)))
            app.must("set_mode", mode="dark")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("app", choices=("maps", "code"))
    parser.add_argument("--bin", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--debug-port", type=int, default=8791)
    parser.add_argument("--timeout", type=int, default=900)
    args = parser.parse_args()
    out = os.path.abspath(args.out)
    logs = os.path.join(out, "logs")
    os.makedirs(logs, exist_ok=True)
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    if args.app == "maps":
        extra = ["--data-dir", data_dir, "--size", "1280x800", "--stats", "--language", "en", "--theme", "flat", "--mode", "light"]
        tag, walk = "maps", maps_walk
    else:
        extra = ["--sample", "--data-dir", data_dir, "--size", "1280x800", "--language", "en", "--theme", "flat",
                 "--shell", "/bin/sh"]
        tag, walk = "code", code_walk
    app = e2e.App(tag, os.path.abspath(args.bin), extra, args.debug_port, logs, args.timeout)
    w = Walk(app, tag, out)
    try:
        walk(w, app)
    except Failure as e:
        w.steps.append("STOP  %s" % str(e)[:300])
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
    finally:
        w.finish()
        app.stop()
    # The audit is a record: it passes when the app started and took at least one picture.
    return 0 if w.count else 1


if __name__ == "__main__":
    sys.exit(main())
