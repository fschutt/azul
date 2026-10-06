#!/usr/bin/env python3
"""AzERP end to end, headless, over the debug server: asset management on the
interpreted ERP views.

    1. starts AzERP with --sample in a fresh data folder (AZERP_TODAY=2026-10-03 so the
       book values are the same every run), waits for AZERP_READY 12 and the sample's
       28 files (erp/<kind>/<uuid>.json, one per record);
    2. NEW ASSET: "New asset" opens the form over the register (AZERP_FORM), a name, a
       German amount "2.400,00" and 8 years are typed, Save writes erp/assets/<id>.json and
       opens the asset; its schedule tab shows the first year's 3 months: 75.00;
    3. CHECK-OUT / CHECK-IN: the check-out modal takes a custodian, the status pill reads
       "Checked out", Check in reads "In use" again;
    4. MAINTENANCE: "Log maintenance" writes an entry; the Maintenance tab lists it;
       DELETE asks first (AZERP_ASK_DELETE <id>, the "Delete A-...?" question): its Cancel
       keeps the asset and its files, its Delete removes them (AZERP_REMOVED erp/assets/<id>.json);
    5. A WRONG FORM: a new asset without a name is refused (AZERP_REFUSED name: ...);
    6. EXPORT, REPORTS, RUN: "Export CSV" writes erp/exports/assets-2026-10-03.csv; the
       Reports tab shows the figures; the depreciation run of 2026 posts its journal to
       erp/exports/depreciation-run-2026.csv;
    7. RESTART + IMPORT: AzERP again on the same folder with a CSV file argument - the 12
       sample assets read back (the new one was deleted), the import preview names 1 new
       asset, Import writes it;
    8. screenshots after each step, flat light; dark at the end.

Usage (after building libazul with the debug server and AzERP; ONE app at a time, through
scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azerp_e2e.py [--bin target/release/AzERP] [--debug-port 8791]
        [--timeout 240] [--out <dir>] [--keep]
"""

import glob
import os

import azlin_e2e as e2e
from azlin_e2e import Failure, modal_window
# The one "click a standard dialog's button by its label" (the page has a "Delete" too).
from azwriter_e2e import click_dialog_button

TAG = "azerp"
TODAY = "2026-10-03"
SAMPLE_ASSETS = 12
SAMPLE_FILES = 28
# The asset page's tabs: a click by text alone takes the first node containing the text, and
# the section tabs' "Maintenance" (the log of every asset) come before the asset's own tab.
TABS = "#__azerp_detail-tabs"


def files(data_dir, folder, suffix=".json"):
    return sorted(glob.glob(os.path.join(data_dir, "erp", folder, "*" + suffix)))


def start(args, logs, binary, data_dir, extra, tag):
    return e2e.App(tag, binary, ["--data-dir", data_dir, "--size", "1280x800", "--theme", "flat",
                                 "--mode", "light"] + extra,
                   args.debug_port, logs, args.timeout, extra_env={"AZERP_TODAY": TODAY})


def saved(app):
    return len(app.printed("AZERP_SAVED"))


def delete_asks_first(app, data_dir, key, out):
    """4b: on the asset `key`'s page, Delete asks; Cancel keeps it, the question's Delete
    removes its files and it leaves the register."""
    asset_file = os.path.join(data_dir, *key.split("/"))
    asked = app.after("the delete question", "AZERP_ASK_DELETE", r"\S+",
                      lambda: app.click(selector="#__azerp_action-delete"))
    if asked not in key:
        raise Failure("Delete asked about %r, not the new asset (%s)" % (asked, key))
    app.until("the question", lambda: app.has_id("__azerp_confirm-delete"))
    question = modal_window(app)
    question.frame(2)
    removed = len(app.printed("AZERP_REMOVED"))
    click_dialog_button(question, "Cancel")
    app.until("the question gone after Cancel", lambda: not app.has_id("__azerp_confirm-delete"))
    app.frame(2)
    if len(app.printed("AZERP_REMOVED")) != removed or not os.path.exists(asset_file):
        raise Failure("Cancel removed files: %s" % app.printed("AZERP_REMOVED")[removed:])
    if not app.shows("Drill press"):
        raise Failure("the asset's page is gone after Cancel")
    app.after("the delete question again", "AZERP_ASK_DELETE", r"\S+",
              lambda: app.click(selector="#__azerp_action-delete"))
    app.until("the question again", lambda: app.has_id("__azerp_confirm-delete"))
    click_dialog_button(modal_window(app), "Delete")
    app.until("the asset's file removed", lambda: key in app.printed("AZERP_REMOVED"))
    app.frame(2)
    if os.path.exists(asset_file):
        raise Failure("%s is still on disk after Delete" % key)
    if app.has_id("__azerp_confirm-delete") or app.shows("Drill press"):
        raise Failure("the deleted asset is still shown")
    app.screenshot(os.path.join(out, "4b-deleted.png"))
    app.log("Cancel kept %s, Delete removed it: %s" % (key, app.printed("AZERP_REMOVED")[removed:]))


def body(args, logs, out):
    binary = e2e.find_binary("AzERP", args.bin, "AZERP_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)

    # ---- 1. the sample register ----
    app = start(args, logs, binary, data_dir, ["--sample"], TAG)
    try:
        app.until("the register", lambda: app.printed("AZERP_READY", r"\d+"))
        ready = int(app.printed("AZERP_READY", r"\d+")[-1])
        if ready != SAMPLE_ASSETS:
            raise Failure("the sample has %d assets, not %d" % (ready, SAMPLE_ASSETS))
        app.until("the sample's files", lambda: saved(app) >= SAMPLE_FILES)
        if len(files(data_dir, "assets")) != SAMPLE_ASSETS:
            raise Failure("erp/assets holds %d files" % len(files(data_dir, "assets")))
        app.frame(3)
        if not app.has_id("__azerp_table"):
            raise Failure("the register table (#__azerp_table) is not in the tree")
        for text in ("A-0001", "ThinkPad X1 Carbon", "Checked out"):
            if not app.shows(text):
                raise Failure("the register does not show %r" % text)
        app.screenshot(os.path.join(out, "1-register.png"))

        # ---- 2. a new asset ----
        app.after("the asset form", "AZERP_FORM", r"\S+", lambda: app.click(text="New asset"))
        if app.last("AZERP_FORM") != "assets_fixed_asset_form":
            raise Failure("New asset opened %r" % app.last("AZERP_FORM"))
        app.text_input("#__azerp_field-name", "Drill press")
        # The amount is a MoneyInput (FIX9 6.6a): its text field is the one inside, en-US format.
        app.text_input("#__azerp_field-acquisition_cost .__azul-native-text-input-container", "2,400.00")
        app.text_input("#__azerp_field-useful_life_years", "8")
        before = saved(app)
        app.after("the asset saved", "AZERP_SAVED_FORM", r"\S+", lambda: app.click(selector="#__azerp_form-save"))
        app.until("its file", lambda: saved(app) > before)
        key = app.last("AZERP_SAVED")
        if not key.startswith("erp/assets/"):
            raise Failure("the new asset wrote %r" % key)
        page = app.last("AZERP_SAVED_FORM")
        app.log("saved %s, opened %s" % (key, page))
        app.frame(3)
        if not app.shows("Drill press"):
            raise Failure("the new asset's page does not show its name")
        app.click_within(TABS, "Depreciation schedule")
        app.frame(2)
        if not app.has_id("__azerp_schedule-table"):
            raise Failure("the schedule tab shows no table")
        if not app.shows("75.00"):
            raise Failure("the first year (October to December of 2,400.00 over 8 years) is not 75.00")
        app.screenshot(os.path.join(out, "2-new-asset-schedule.png"))

        # ---- 3. check-out and check-in ----
        app.click_within(TABS, "Overview")
        app.after("the check-out form", "AZERP_FORM", r"\S+", lambda: app.click(text="Check out"))
        # A modal form (`form_modal`) is a Modal: a transient window of its own.
        form = modal_window(app)
        form.text_input("#__azerp_field-custodian", "Katherine Johnson")
        before = saved(app)
        form.click(selector="#__azerp_form-save")
        app.until("the check-out's files", lambda: saved(app) >= before + 2)
        app.frame(2)
        if not app.shows("Checked out") or not app.shows("Katherine Johnson"):
            raise Failure("the asset does not read checked out to Katherine Johnson")
        app.screenshot(os.path.join(out, "3-checked-out.png"))
        before = saved(app)
        app.click(text="Check in")
        app.until("the check-in's files", lambda: saved(app) >= before + 2)
        app.frame(2)
        if not app.shows("In use"):
            raise Failure("the asset does not read in use after Check in")

        # ---- 4. maintenance ----
        app.after("the maintenance form", "AZERP_FORM", r"\S+", lambda: app.click(text="Log maintenance"))
        form = modal_window(app)
        form.text_input("#__azerp_field-description", "First service")
        before = saved(app)
        form.click(selector="#__azerp_form-save")
        app.until("the entry's file", lambda: saved(app) > before)
        if not app.last("AZERP_SAVED").startswith("erp/maintenance/"):
            raise Failure("the entry wrote %r" % app.last("AZERP_SAVED"))
        app.click_within(TABS, "Maintenance")
        app.frame(2)
        if not app.shows("First service"):
            raise Failure("the Maintenance tab does not list the entry")
        app.screenshot(os.path.join(out, "4-maintenance.png"))

        # ---- 4b. delete asks first: Cancel keeps the asset, Delete removes its files ----
        delete_asks_first(app, data_dir, key, out)

        # ---- 5. a wrong form ----
        app.click(text="Register")
        app.after("the asset form", "AZERP_FORM", r"\S+", lambda: app.click(text="New asset"))
        # The form refuses every empty required field, one AZERP_REFUSED line each (name,
        # cost, useful life): the name must be among them.
        refused = len(app.printed("AZERP_REFUSED"))
        app.after("the refusal", "AZERP_REFUSED", r".*", lambda: app.click(selector="#__azerp_form-save"))
        app.frame(2)
        reasons = app.printed("AZERP_REFUSED")[refused:]
        if not any(r.startswith("name:") for r in reasons):
            raise Failure("the refusals %r do not name the name" % reasons)
        app.screenshot(os.path.join(out, "5-refused.png"))
        app.click(selector="#__azerp_form-cancel")

        # ---- 6. export, reports, the depreciation run ----
        app.after("the export", "AZERP_EXPORTED", r"\S+", lambda: app.click(text="Export CSV"))
        export = app.last("AZERP_EXPORTED")
        if export != "erp/exports/assets-%s.csv" % TODAY:
            raise Failure("the register was exported to %r" % export)
        path = os.path.join(data_dir, *export.split("/"))
        app.until("the export's file", lambda: os.path.exists(path))
        with open(path, encoding="utf-8") as f:
            header = f.readline()
        if not header.startswith("asset_number,name,category"):
            raise Failure("the export's header reads %r" % header)
        app.click(text="Reports")
        app.frame(2)
        if not app.has_id("__azerp_report-totals") or not app.shows("Book value"):
            raise Failure("the reports show no figures")
        app.screenshot(os.path.join(out, "6-reports.png"))
        app.click(text="Register")
        app.after("the run page", "AZERP_PAGE", r"\S+", lambda: app.click(text="Depreciation run"))
        app.after("the run preview", "AZERP_RUN_PREVIEW", r"\d+", lambda: app.click(selector="#__azerp_run-next"))
        app.frame(2)
        if not app.has_id("__azerp_run-total"):
            raise Failure("the run preview shows no total")
        app.screenshot(os.path.join(out, "7-run-preview.png"))
        app.after("the journal", "AZERP_EXPORTED", r"\S+", lambda: app.click(selector="#__azerp_run-post"))
        if app.last("AZERP_EXPORTED") != "erp/exports/depreciation-run-2026.csv":
            raise Failure("the run posted to %r" % app.last("AZERP_EXPORTED"))
        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "8-dark.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # ---- 7. restart: the files are read back; a CSV argument is imported ----
    csv_path = os.path.join(logs, "new-assets.csv")
    with open(csv_path, "w", encoding="utf-8") as f:
        f.write("Bezeichnung;Kategorie;Anschaffungsdatum;Anschaffungskosten;Nutzungsdauer\n")
        f.write("Ladder;Tools;01.03.2026;189,00;5\n")
    again = start(args, logs, binary, data_dir, [csv_path], TAG + "2")
    try:
        again.until("the register again", lambda: again.printed("AZERP_READY", r"\d+"))
        ready = int(again.printed("AZERP_READY", r"\d+")[-1])
        # The sample's assets (the new one was deleted in 4b).
        if ready != SAMPLE_ASSETS:
            raise Failure("%d assets read back, not %d" % (ready, SAMPLE_ASSETS))
        again.frame(3)
        if not again.has_id("__azerp_import-summary") or not again.shows("1 new"):
            raise Failure("the import preview does not name 1 new asset")
        again.screenshot(os.path.join(out, "9-import-preview.png"))
        again.after("the import", "AZERP_IMPORTED", r"\d+", lambda: again.click(selector="#__azerp_import-commit"))
        if again.last("AZERP_IMPORTED") != "1":
            raise Failure("imported %r assets" % again.last("AZERP_IMPORTED"))
        again.until("the imported files", lambda: len(files(data_dir, "assets")) == SAMPLE_ASSETS + 1)
        again.screenshot(os.path.join(out, "10-imported.png"))
        again.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(again.out_path), e2e.tail(again.err_path)))
        raise
    finally:
        again.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
