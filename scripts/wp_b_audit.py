#!/usr/bin/env python3
"""WP-B's audit pictures: AzNotes, AzContacts and AzTasks in the host's headless window (CPU render), named.

    python3 scripts/wp_b_audit.py notes|contacts|tasks --bin <the app's QA build> --out <dir> \
        [--label before|after] [--debug-port 8801] [--timeout 900]

One run of one app, over the engine's debug server (scripts/azlin_e2e.py), walks the window through the acceptance sizes of the
brief (briefs/app_refactor_2026-11-28.md section 4: desktop 1280 x 800, Pixel 8, iPhone 16 Pro, a small phone, a phone on its side)
and the states of the app's card, and writes one PNG per state, named `<app>-<form>-<NN>-<what>.png`, and, for the phone-sized
windows, the phone-fit audit of the same moment (scripts/ci/layout_audit.py: boxes over the window's edges, tappable nodes under
48 dp, text crossing an edge) as `phone-fit.md`. A step that fails is written down (`audit-steps.txt`) and the walk goes on: the
audit is about what the pictures show, a missing control is a finding, not a stop.

`--label before` walks the apps as the refactor found them (the ids of their old windows), `--label after` the refactored ones
(the ids of the widget family's frame); a step whose control is not there says so in the step list.

The same walk is what the scenario files of the refactor (examples/azul-notes/e2e, azul-contacts/e2e, azul-tasks/e2e) shoot again
after it, as the AFTER side of each pair.
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
TABLET = ("tablet", 834, 1112, "off")


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


def mode(app, which):
    app.must("set_mode", mode=which)
    app.frame(3)


def theme(app, which):
    app.must("set_theme", theme=which)
    app.frame(3)


def text_scale_steps(w, app, form, what):
    """The same window at the system text sizes 1.3 and 2.0 (the engine's `set_system_accessibility` op; a build without it says so)."""
    for scale in (1.3, 2.0):
        def one(scale=scale):
            app.must("set_system_accessibility", text_scale=scale)
            app.frame(3)
            app.must("wait_settled")
            w.shot(form, "%s-text-%s" % (what, ("%.1f" % scale).replace(".", "-")), audit=True)
        w.step("%s text size %.1f" % (form[0], scale), one)
    w.step("%s text size 1.0 again" % form[0], lambda: (app.must("set_system_accessibility", text_scale=1.0), app.frame(3)))


# ---- the walks ----------------------------------------------------------------------------------------------------------------

def notes_ready(app):
    app.until("AzNotes' window", lambda: app.printed("AZNOTES_LOADED", r".*"))
    # the first note opens by itself once the library is read
    app.until("a note open", lambda: app.printed("AZNOTES_OPEN", r".*") or app.has_id("__aznotes_note-body"), interval=0.4)
    app.frame(4)


def notes_walk(w, app, label):
    notes_ready(app)
    d = DESKTOP
    w.window(d)
    mode(app, "light")
    w.step("desktop first look light", lambda: w.shot(d, "first-look-light"))
    w.step("desktop flat", lambda: (theme(app, "flat"), w.shot(d, "first-look-flat"), theme(app, "flora")))
    w.step("desktop dark", lambda: (mode(app, "dark"), w.shot(d, "first-look-dark")))
    mode(app, "light")

    def rail():
        # the sidebar's toggle (a rail afterwards); the old window has the navigation pane's own collapse
        app.click(selector=".__azul-native-sidebar-toggle")
        app.frame(3)
        app.must("wait_settled")

    if label == "after":
        w.step("desktop rail", lambda: (rail(), w.shot(d, "rail"), rail()))
        w.step("desktop file tab is the location", lambda: (app.click(text="File"), app.frame(3), w.shot(d, "file-location")))
        w.step("desktop home ribbon", lambda: (app.click(text="Home"), app.frame(3), w.shot(d, "home-ribbon")))
    else:
        w.step("desktop settings", lambda: (app.click(selector="#__aznotes_open-settings"), app.until("settings", lambda: app.has_id("__aznotes_settings") or app.has_id("appkit-settings")), w.shot(d, "settings")))
        app.key("escape")
        app.frame(2)
    for form in (PIXEL8, IPHONE, SMALL, PIXEL8_LANDSCAPE, TABLET):
        w.window(form)
        mode(app, "light")
        w.step("%s first look" % form[0], lambda f=form: w.shot(f, "first-look", audit=True))
        if form is PIXEL8:
            w.step("phone dark", lambda f=form: (mode(app, "dark"), w.shot(f, "dark", audit=True), mode(app, "light")))
            text_scale_steps(w, app, form, "first-look")
    w.window(DESKTOP)


def contacts_ready(app):
    app.until("AzContacts' window", lambda: app.printed("AZCONTACTS_LOADED", r".*"))
    app.frame(4)


def contacts_walk(w, app, label):
    contacts_ready(app)
    d = DESKTOP
    w.window(d)
    mode(app, "light")
    w.step("desktop first look light", lambda: w.shot(d, "first-look-light"))
    w.step("desktop flat", lambda: (theme(app, "flat"), w.shot(d, "first-look-flat"), theme(app, "flora")))
    w.step("desktop dark", lambda: (mode(app, "dark"), w.shot(d, "first-look-dark")))
    mode(app, "light")

    def first_contact():
        if label == "after":
            app.click(selector=".__azcontacts_contact-row", every_dom=True)
        else:
            app.click(selector=".__azcontacts_contact-row", every_dom=True)
        app.until("a contact selected", lambda: app.printed("AZCONTACTS_SELECTED", r".*"))
        app.frame(3)

    w.step("desktop a contact selected", lambda: (first_contact(), w.shot(d, "contact-selected")))
    if label == "after":
        w.step("desktop rail", lambda: (app.click(selector=".__azul-native-sidebar-toggle"), app.frame(3), w.shot(d, "rail"), app.click(selector=".__azul-native-sidebar-toggle"), app.frame(3)))
        w.step("desktop file tab is the location", lambda: (app.click(text="File"), app.frame(3), w.shot(d, "file-location")))
    else:
        w.step("desktop duplicates", lambda: (app.click(selector="#__azcontacts_toolbar-duplicates"), app.frame(3), w.shot(d, "duplicates")))
    for form in (PIXEL8, IPHONE, SMALL, PIXEL8_LANDSCAPE, TABLET):
        w.window(form)
        mode(app, "light")
        w.step("%s first look" % form[0], lambda f=form: w.shot(f, "first-look", audit=True))
        if form is PIXEL8:
            w.step("phone dark", lambda f=form: (mode(app, "dark"), w.shot(f, "dark", audit=True), mode(app, "light")))
            text_scale_steps(w, app, form, "first-look")
    w.window(DESKTOP)


def tasks_ready(app):
    app.until("AzTasks' window", lambda: app.has_id("__aztasks_task-list"))
    app.frame(4)


def tasks_walk(w, app, label):
    tasks_ready(app)
    d = DESKTOP
    w.window(d)
    mode(app, "light")
    w.step("desktop first look light", lambda: w.shot(d, "first-look-light"))
    w.step("desktop flat", lambda: (theme(app, "flat"), w.shot(d, "first-look-flat"), theme(app, "flora")))
    w.step("desktop dark", lambda: (mode(app, "dark"), w.shot(d, "first-look-dark")))
    mode(app, "light")

    def first_task():
        app.click(selector=".__aztasks_task-row", every_dom=True)
        app.until("a task selected", lambda: app.printed("AZTASKS_SELECTED", r".*"))
        app.frame(3)

    w.step("desktop a task selected", lambda: (first_task(), w.shot(d, "task-selected")))
    if label == "after":
        w.step("desktop rail", lambda: (app.click(selector=".__azul-native-sidebar-toggle"), app.frame(3), w.shot(d, "rail"), app.click(selector=".__azul-native-sidebar-toggle"), app.frame(3)))
        w.step("desktop file tab is the location", lambda: (app.click(text="File"), app.frame(3), w.shot(d, "file-location")))
    for form in (PIXEL8, IPHONE, SMALL, PIXEL8_LANDSCAPE, TABLET):
        w.window(form)
        mode(app, "light")
        w.step("%s first look" % form[0], lambda f=form: w.shot(f, "first-look", audit=True))
        if form is PIXEL8:
            w.step("phone dark", lambda f=form: (mode(app, "dark"), w.shot(f, "dark", audit=True), mode(app, "light")))
            text_scale_steps(w, app, form, "first-look")
    w.window(DESKTOP)


APPS = {
    "notes": ("notes", notes_walk, lambda data: ["--sample", "--data-dir", data, "--size", "1280x800", "--language", "en", "--theme", "flora", "--mode", "light"]),
    "contacts": ("contacts", contacts_walk, lambda data: ["--sample", "--data-dir", data, "--size", "1280x800", "--language", "en", "--theme", "flora", "--mode", "light"]),
    "tasks": ("tasks", tasks_walk, lambda data: ["--sample", "--data", data, "--size", "1280x800", "--language", "en", "--theme", "flora", "--mode", "light"]),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("app", choices=sorted(APPS))
    parser.add_argument("--bin", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--label", choices=("before", "after"), default="before")
    parser.add_argument("--debug-port", type=int, default=8801)
    parser.add_argument("--timeout", type=int, default=900)
    args = parser.parse_args()
    out = os.path.abspath(args.out)
    logs = os.path.join(out, "logs")
    os.makedirs(logs, exist_ok=True)
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    tag, walk, argv = APPS[args.app]
    app = e2e.App(tag, os.path.abspath(args.bin), argv(data_dir), args.debug_port, logs, args.timeout,
                  extra_env={"AZLIN_CONFIG": "off", "AZLIN_DATA": data_dir})
    w = Walk(app, tag, out)
    try:
        walk(w, app, args.label)
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
