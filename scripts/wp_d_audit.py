#!/usr/bin/env python3
"""WP-D's audit pictures: AzERP as it is, in the host's headless window (CPU render), named.

    python3 scripts/wp_d_audit.py --bin <AzERP's QA build> --out <dir> [--debug-port 8791] [--timeout 1200]

Two runs of the app over the engine's debug server (scripts/azlin_e2e.py), the way a person meets it:

  1. a FIRST LAUNCH on an empty data folder, no `--sample` (what the user of DECISIONS item 50 saw: "no real UI");
  2. the same folder with `--sample` (the 12-asset register the old script uses), every page the views route to.

At the acceptance sizes of the brief (briefs/app_refactor_2026-11-28.md section 4) one PNG per state is written, named
`azerp-<form factor>-<NN>-<what>.png`, and for the phone-sized ones the phone-fit audit of the same moment
(scripts/ci/layout_audit.py: boxes over the window's edges, tappable nodes under 48 dp, text crossing an edge) as `phone-fit.md`.
A step that fails is written down (`audit-steps.txt`) and the walk goes on: the audit is about what the pictures show, a missing
control is a finding, not a stop (scripts/wp_e2_audit.py is the model; its `Walk` does the bookkeeping).

The ids are the OLD window's (`__azerp_table`, `__azerp_tabs`, ...): this is the BEFORE of the refactor. The AFTER is the scenario
files of examples/azul-erp/e2e.
"""

import argparse
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ci"))

import azlin_e2e as e2e  # noqa: E402
from azlin_e2e import Failure  # noqa: E402
from wp_e2_audit import DESKTOP, IPHONE, PIXEL8, PIXEL8_LANDSCAPE, SMALL, Walk  # noqa: E402

TODAY = "2026-10-03"
PHONES = (PIXEL8, IPHONE, SMALL, PIXEL8_LANDSCAPE)


def start(args, logs, binary, data_dir, extra, tag):
    """AzERP in its own data folder; the day is fixed so the book values are the same every run."""
    return e2e.App(tag, binary, ["--data-dir", data_dir, "--size", "1280x800", "--language", "en"] + extra,
                   args.debug_port, logs, args.timeout, extra_env={"AZERP_TODAY": TODAY})


def page(app):
    """What the window says it shows (the last AZERP_PAGE line), for the log."""
    return app.last("AZERP_PAGE") or "(none)"


def first_launch(w, app):
    """The first launch: an empty data folder. The old window ends in 'Fixed assets: none yet'."""
    app.until("the register page (READY)", lambda: app.printed("AZERP_READY", r"\d+"))
    app.frame(4)
    w.window(DESKTOP)
    app.must("set_mode", mode="light")
    w.step("desktop first launch", lambda: w.shot(DESKTOP, "first-launch"))
    w.step("desktop first launch dark", lambda: (app.must("set_mode", mode="dark"), app.frame(3), w.shot(DESKTOP, "first-launch-dark")))
    app.must("set_mode", mode="light")
    for form in PHONES:
        w.window(form)
        w.step("%s first launch" % form[0], lambda f=form: w.shot(f, "first-launch", audit=True))
        if form is PIXEL8:
            w.step("phone first launch dark", lambda f=form: (app.must("set_mode", mode="dark"), app.frame(3), w.shot(f, "first-launch-dark", audit=True)))
            app.must("set_mode", mode="light")


def section(app, text):
    """A section tab of the old window (the RecordsShell's tab row)."""
    app.click_within("#__azerp_tabs", text)
    app.frame(3)


def sampled(w, app):
    """The sample register: every page, desktop first, then the phones' windows."""
    app.until("the register (READY)", lambda: app.printed("AZERP_READY", r"\d+"))
    app.until("the sample's files", lambda: len(app.printed("AZERP_SAVED")) >= 28)
    app.frame(4)
    w.window(DESKTOP)
    app.must("set_mode", mode="light")

    def register():
        section(app, "Register")
        if not app.has_id("__azerp_table"):
            raise Failure("the register table (#__azerp_table) is not in the tree")

    w.step("desktop register", lambda: (register(), w.shot(DESKTOP, "register")))

    def open_asset():
        # Enter / a double click on a row opens its row action: the asset's page
        app.click_exact("ThinkPad X1 Carbon", double=True)
        app.until("the asset page", lambda: app.has_id("__azerp_detail"))
        app.frame(3)

    w.step("desktop asset page", lambda: (open_asset(), w.shot(DESKTOP, "asset")))
    w.step("desktop schedule tab", lambda: (app.click_within("#__azerp_detail-tabs", "Depreciation schedule"), app.frame(3), w.shot(DESKTOP, "schedule")))
    w.step("desktop maintenance tab", lambda: (app.click_within("#__azerp_detail-tabs", "Maintenance"), app.frame(3), w.shot(DESKTOP, "asset-maintenance")))
    w.step("desktop edit form", lambda: (app.click(text="Edit"), app.until("the form", lambda: app.has_id("__azerp_form")), app.frame(3), w.shot(DESKTOP, "edit-form")))
    app.key("escape")
    app.frame(2)
    w.step("desktop back to the register", lambda: (app.click(selector="#__azerp_back"), app.frame(3)))
    w.step("desktop new asset form", lambda: (app.click(text="New asset"), app.until("the form", lambda: app.has_id("__azerp_form")), app.frame(3), w.shot(DESKTOP, "new-asset-form")))
    app.key("escape")
    app.frame(2)
    for text, what in (("Categories", "categories"), ("Locations", "locations"), ("Maintenance", "maintenance-log"),
                       ("Check-outs", "checkouts"), ("Reports", "reports")):
        w.step("desktop %s" % what, lambda t=text, n=what: (section(app, t), w.shot(DESKTOP, n)))
    w.step("desktop depreciation run", lambda: (section(app, "Register"), app.click(text="Depreciation run"), app.frame(3), w.shot(DESKTOP, "depreciation-run")))
    w.step("desktop dark register", lambda: (section(app, "Register"), app.must("set_mode", mode="dark"), app.frame(3), w.shot(DESKTOP, "register-dark")))
    app.must("set_mode", mode="light")
    w.step("desktop settings", lambda: (app.key(",", primary=True), app.until("settings", lambda: app.has_id("appkit-settings")), w.shot(DESKTOP, "settings")))
    app.key("escape")
    app.frame(2)

    # ---- the phones' windows ----
    for form in PHONES:
        w.window(form)
        app.must("set_mode", mode="light")
        w.step("%s register" % form[0], lambda f=form: (section(app, "Register"), w.shot(f, "register", audit=True)))
        if form is PIXEL8:
            w.step("phone asset page", lambda f=form: (open_asset(), w.shot(f, "asset", audit=True)))
            w.step("phone schedule tab", lambda f=form: (app.click_within("#__azerp_detail-tabs", "Depreciation schedule"), app.frame(3), w.shot(f, "schedule", audit=True)))
            w.step("phone edit form", lambda f=form: (app.click(text="Edit"), app.until("the form", lambda: app.has_id("__azerp_form")), app.frame(3), w.shot(f, "edit-form", audit=True)))
            app.key("escape")
            app.frame(2)
            w.step("phone back", lambda: (app.click(selector="#__azerp_back"), app.frame(3)))
            w.step("phone reports", lambda f=form: (section(app, "Reports"), w.shot(f, "reports", audit=True)))
            w.step("phone register dark", lambda f=form: (section(app, "Register"), app.must("set_mode", mode="dark"), app.frame(3), w.shot(f, "register-dark", audit=True)))
            app.must("set_mode", mode="light")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--debug-port", type=int, default=8791)
    parser.add_argument("--timeout", type=int, default=1200)
    args = parser.parse_args()
    out = os.path.abspath(args.out)
    logs = os.path.join(out, "logs")
    os.makedirs(logs, exist_ok=True)
    binary = os.path.abspath(args.bin)
    empty_dir = os.path.join(logs, "data-empty")
    sample_dir = os.path.join(logs, "data-sample")
    os.makedirs(empty_dir, exist_ok=True)
    os.makedirs(sample_dir, exist_ok=True)
    status = 0
    for tag, data_dir, extra, walk in (("first", empty_dir, [], first_launch), ("sample", sample_dir, ["--sample"], sampled)):
        app = start(args, logs, binary, data_dir, extra, "azerp-" + tag)
        w = Walk(app, "azerp-" + tag, out)
        try:
            walk(w, app)
        except Failure as e:
            print("[azerp-%s] FAIL: %s" % (tag, e), flush=True)
            print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
            status = 1
        finally:
            w.finish()
            app.stop()
    sys.exit(status)


if __name__ == "__main__":
    main()
