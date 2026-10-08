#!/usr/bin/env python3
"""AzDrive's Add drive dialog end to end, headless: Connect data source (an S3-compatible bucket,
a folder on this computer, a SQLite database whose tables show as folders) and Buy storage (the
mock token server's tiers, a test drive), each from another of the dialog's doors.

The dialog is azul's modal Dialog - a transient window of its own over AzDrive's window - and is
driven in ITS window (azlin_e2e.modal_window: the request's `window_id`), not as the in-window
sheet of `--dialogs inline`:

    1. This PC; the source list's "Add drive..." opens the dialog on its two choices (Buy
       storage, Connect data source);
    2. Connect data source: the sources in their groups; S3-compatible storage: the form (Name,
       Endpoint, Region, Bucket, Access key, Secret key, Path-style on), Test connection (ONE
       ListObjectsV2 with max-keys=1 at the mock stack's S3), Add drive: the dialog closes, the
       drive's row is in CLOUD, its root lists the bucket, the drives file holds no key;
    3. This PC's ribbon (Computer > Add drive) -> Folder on this computer: a typed path, Add
       drive: the folder's row is in LOCATIONS and lists its files;
    4. Home > Add drive (a folder's ribbon) -> SQLite file: the path of a database with two
       tables, Test connection, Add drive: its root lists the two tables as folders, a table
       folder its CSV, its schema and rows/, rows/ one JSON file per row;
    5. Add drive -> Buy storage: the mock token server's six tiers with their prices, Pay yearly
       turns them into yearly prices, a tier chosen, Create test drive (a development server):
       the new Azlin drive is in the drives file with its {"type": "azlin"} auth (no secret),
       its empty bucket lists, and a file put into the bucket shows after F5.

Buy (a checkout) is not driven here: AzDrive opens the payment page with azul's Url::open, which
starts the system's browser even in a headless run (an engine gap, see the progress file).

Usage (from the azul repository, after building libazul with the debug server and AzDrive with
its default features `opendal` and `sql`):

    python3 scripts/azdrive_add_e2e.py [--bin target/release/AzDrive] [--debug-port 8783]
        [--timeout 240] [--out /tmp/azdrive-add-shots] [--keep-logs]

Every key_down has its key_up (the E2E key_up rule); the shared Azlin config is a temporary one
(azlin_e2e sets AZLIN_CONFIG), the keyring the headless backend's in-memory one.
"""

import argparse
import json
import os
import re
import shutil
import sqlite3
import sys
import tempfile

import azlin_e2e as e2e
import azlin_mock_stack
from azlin_e2e import Failure
from azdrive_e2e import Drive, I, item_names, open_item

# The ids of the dialog (examples/azul-drive/src/ids.rs).
DIALOG = "#__azdrive_add_drive"
BUCKET = "e2e-photos"
BUCKET_FILES = {
    "2026/beach.txt": b"sand and sea\n",
    "2026/notes.txt": b"hello from the bucket\n",
    "readme.txt": b"An S3-compatible bucket for AzDrive's Add drive test.\n",
}
FOLDER_FILES = {"one.txt": b"1\n", "two.txt": b"22\n", "three.txt": b"333\n"}
# The form's own names of the sources (azul-storage src/catalog.rs): the default drive names.
S3_DEFAULT_NAME = "S3-compatible storage"


def log(line):
    print("[azdrive-add-e2e] %s" % line, flush=True)


def add_id(name):
    return "#__azdrive_add_" + name


def side_drive(drive_id):
    """The source list's row of a drive (ids.rs side_drive)."""
    return "#__azdrive_side_drive_" + re.sub(r"[^A-Za-z0-9_-]", "_", drive_id).lower()


def make_database(path):
    """A SQLite file with two tables (sqlite3 is Python's own)."""
    con = sqlite3.connect(path)
    con.executescript(
        """
        CREATE TABLE customers(id INTEGER PRIMARY KEY, name TEXT NOT NULL, city TEXT);
        INSERT INTO customers(name, city) VALUES
            ('Ada', 'London'), ('Grace', 'New York'), ('Linus', 'Helsinki');
        CREATE TABLE orders(order_no TEXT PRIMARY KEY, customer INTEGER, total REAL);
        INSERT INTO orders VALUES ('A-1', 1, 9.5), ('A-2', 3, 12.25);
        """
    )
    con.commit()
    con.close()


class Dialog:
    """The Add drive dialog in its own window."""

    def __init__(self, app):
        self.app = app
        self.win = e2e.modal_window(app)
        self.win.until("the dialog's content", lambda: self.win.has(DIALOG))

    def click(self, short, what=None):
        selector = add_id(short) if not short.startswith("#") else short
        self.win.until(what or selector, lambda: self.win.has(selector))
        self.win.click(selector=selector)

    def page(self, line, action):
        """Runs `action` and waits for the dialog's page `line` (AZDRIVE_ADD_PAGE)."""
        self.app.after("the dialog's page %s" % line, "AZDRIVE_ADD_PAGE", re.escape(line), action)

    def type_into(self, short, text, clear=0):
        """Types `text` into the field `short` (after `clear` Backspaces from its end)."""
        selector = add_id(short)
        self.win.until("the field %s" % short, lambda: self.win.has(selector))
        self.win.must("focus_node", selector=selector)
        self.win.frame(2)
        if clear:
            self.win.key("end")
            for _ in range(clear):
                self.win.key("backspace", frames=1)
        self.win.must("text_input", text=text)
        self.win.frame(2)

    def shows(self, text):
        return self.win.shows(text)

    def screenshot(self, path):
        self.win.screenshot(path)


def open_dialog(app, how, action):
    """Opens the dialog with `action` and waits for its two choices."""
    app.after("the dialog from %s" % how, "AZDRIVE_ADD_PAGE", r"choose", action)
    dialog = Dialog(app)
    for choice in ("choice_buy", "choice_connect"):
        dialog.win.until("the choice %s" % choice, lambda: dialog.win.has(add_id(choice)))
    return dialog


def wait_closed(app):
    app.until("the dialog closed", lambda: len(app.window_ids()) <= 1)


def drives_file_entries(path):
    with open(path, "r", encoding="utf-8") as f:
        text = f.read()
    return text, json.loads(text)["drives"]


def run(args, logs):
    binary = e2e.find_binary("AzDrive", args.bin, "AZDRIVE_BIN")
    log("AzDrive: %s" % binary)
    out = args.out or os.path.join(logs, "shots")
    os.makedirs(out, exist_ok=True)

    # The mock Azlin stack: the token server and the S3 balancer (one access key).
    stack = azlin_mock_stack.start(os.path.join(logs, "s3"))
    stack.s3.store.create_bucket(BUCKET)
    for key, data in BUCKET_FILES.items():
        stack.s3.store.write(BUCKET, key, data)
    log("mock token server %s, S3 %s" % (stack.token_url, stack.s3_url))

    home = os.path.join(logs, "home")
    os.makedirs(os.path.join(home, "Documents"))
    folder = os.path.join(logs, "projects")
    os.makedirs(folder)
    for name, data in FOLDER_FILES.items():
        with open(os.path.join(folder, name), "wb") as f:
            f.write(data)
    database = os.path.join(logs, "shop.sqlite")
    make_database(database)
    drives_file = os.path.join(logs, "config", "drives.json")

    switches = [
        "--screen", "this-pc", "--theme", "flat", "--mode", "light",
        "--home", home,
        "--downloads", os.path.join(logs, "downloads"),
        "--data-dir", os.path.join(logs, "data"),
        "--drives", drives_file,
        "--token-url", stack.token_url,
        "--profile", "local",
    ]
    app = Drive("azdrive", binary, switches, args.debug_port, logs, args.timeout)
    try:
        app.until("the This PC view", lambda: app.printed("AZDRIVE_PLACE", r"this-pc"))
        app.until("the debug server", lambda: app.op("get_dom_tree"))
        app.must("resize", width=1280.0, height=820.0)
        app.frame(3)

        # 1. The source list's "Add drive..." opens the dialog on its two choices.
        app.until("the source list's Add drive", lambda: app.has("#" + I("side-add-drive")))
        dialog = open_dialog(app, "the source list",
                             lambda: app.click(selector="#" + I("side-add-drive")))
        for text in ("Buy storage", "Connect data source"):
            if not dialog.shows(text):
                raise Failure("the dialog's choices do not say %r" % text)
        dialog.screenshot(os.path.join(out, "1-choose.png"))
        log("1. the source list's Add drive opened the dialog (its own window) on Buy storage "
            "and Connect data source")

        # 2. Connect data source -> S3-compatible storage.
        dialog.page("sources", lambda: dialog.click("choice_connect"))
        for group in ("Cloud object storage", "Network & NAS", "Consumer clouds", "Developer",
                      "Databases & key-value"):
            dialog.win.until("the group %s" % group, lambda: dialog.shows(group))
        for source in ("s3", "local", "sqlite"):
            dialog.win.until("the source %s" % source,
                             lambda: dialog.win.has(add_id("service_" + source)))
        dialog.screenshot(os.path.join(out, "2-sources.png"))
        dialog.page("form s3", lambda: dialog.click("service_s3"))
        dialog.type_into("name", "E2E Photos", clear=len(S3_DEFAULT_NAME))
        dialog.type_into("field_endpoint", stack.s3_url)
        dialog.type_into("field_bucket", BUCKET)
        dialog.type_into("field_access_key_id", azlin_mock_stack.ACCESS_KEY)
        dialog.type_into("field_secret_access_key", azlin_mock_stack.SECRET_KEY)
        stack.s3.clear_log()
        tested = app.after("the connection test", "AZDRIVE_TESTED", r"ok|error",
                           lambda: dialog.click("test"))
        if tested != "ok":
            raise Failure("Test connection said %s: %s" % (tested, dialog.win.texts()))
        dialog.win.until('"Connection OK"', lambda: dialog.shows("Connection OK"))
        calls = [(r["method"], r["op"], r["query"].get("max-keys")) for r in stack.s3.requests()]
        if calls != [("GET", "ListObjectsV2", "1")]:
            raise Failure("Test connection made %r, not one ListObjectsV2 with max-keys=1"
                          % calls)
        dialog.screenshot(os.path.join(out, "2-s3-form.png"))
        s3_id = app.after("the S3 drive added", "AZDRIVE_ADDED", r"\S+",
                          lambda: dialog.click("save"))
        wait_closed(app)
        app.until("the S3 drive's root", lambda: app.printed(
            "AZDRIVE_LISTED", r"%s / 2" % re.escape(s3_id)))
        app.until("its row in CLOUD", lambda: app.has(side_drive(s3_id)))
        app.until("the bucket's folder 2026", lambda: "2026" in item_names(app))
        text, entries = drives_file_entries(drives_file)
        if azlin_mock_stack.SECRET_KEY in text or azlin_mock_stack.ACCESS_KEY in text:
            raise Failure("the drives file holds a key: %s" % text)
        entry = next(e for e in entries if e["id"] == s3_id)
        if entry["name"] != "E2E Photos" or entry["location"]["bucket"] != BUCKET:
            raise Failure("the drives file's entry is %s" % entry)
        app.screenshot(os.path.join(out, "2-s3-drive.png"))
        log("2. Connect data source > S3-compatible storage: one ListObjectsV2 tested it, the "
            "drive %s lists the bucket, its row is in CLOUD, the drives file holds no key"
            % s3_id)

        # 3. This PC's ribbon -> a folder on this computer.
        app.after("This PC", "AZDRIVE_PLACE", r"this-pc",
                  lambda: app.click(selector="#" + I("side-this-pc")))
        dialog = open_dialog(app, "This PC's ribbon", lambda: app.ribbon("Add drive"))
        dialog.page("sources", lambda: dialog.click("choice_connect"))
        dialog.page("form local", lambda: dialog.click("service_local"))
        dialog.type_into("field_root", folder)
        local_id = app.after("the folder added", "AZDRIVE_ADDED", r"\S+",
                             lambda: dialog.click("save"))
        wait_closed(app)
        app.until("the folder's root", lambda: app.printed(
            "AZDRIVE_LISTED", r"%s / %d" % (re.escape(local_id), len(FOLDER_FILES))))
        app.until("its row in LOCATIONS", lambda: app.has(side_drive(local_id)))
        app.until("its files", lambda: "two.txt" in item_names(app))
        log("3. Computer > Add drive > Folder on this computer: %s lists the folder's %d files"
            % (local_id, len(FOLDER_FILES)))

        # 4. Home > Add drive -> a SQLite database: its tables are folders.
        app.after("the Home drive", "AZDRIVE_PLACE", r"home /",
                  lambda: app.click(selector="#" + I("side-drive-home")))
        app.tab("Home")
        dialog = open_dialog(app, "Home's ribbon", lambda: app.ribbon("Add drive"))
        dialog.page("sources", lambda: dialog.click("choice_connect"))
        dialog.page("form sqlite", lambda: dialog.click("service_sqlite"))
        dialog.type_into("field_path", database)
        tested = app.after("the database's connection test", "AZDRIVE_TESTED", r"ok|error",
                           lambda: dialog.click("test"))
        if tested != "ok":
            raise Failure("the database's test said %s: %s" % (tested, dialog.win.texts()))
        dialog.screenshot(os.path.join(out, "4-sqlite-form.png"))
        db_id = app.after("the database added", "AZDRIVE_ADDED", r"\S+",
                          lambda: dialog.click("save"))
        wait_closed(app)
        app.until("the database's tables", lambda: app.printed(
            "AZDRIVE_LISTED", r"%s / 2" % re.escape(db_id)))
        for table in ("customers", "orders"):
            app.until("the table folder %s" % table, lambda: table in item_names(app))
        app.screenshot(os.path.join(out, "4-tables.png"))
        app.after("the customers table", "AZDRIVE_LISTED",
                  r"%s customers/ 3" % re.escape(db_id), lambda: open_item(app, "customers"))
        for name in ("customers.csv", "schema.json", "rows"):
            app.until("the table's %s" % name, lambda: name in item_names(app))
        app.after("the customers' rows", "AZDRIVE_LISTED",
                  r"%s customers/rows/ 3" % re.escape(db_id), lambda: open_item(app, "rows"))
        for name in ("1.json", "2.json", "3.json"):
            app.until("the row file %s" % name, lambda: name in item_names(app))
        app.screenshot(os.path.join(out, "4-rows.png"))
        log("4. Home > Add drive > SQLite file: %s shows its two tables as folders, customers/ "
            "its CSV, schema and rows/, rows/ a JSON file per row" % db_id)

        # 5. Buy storage against the mock token server (a development server).
        app.after("This PC", "AZDRIVE_PLACE", r"this-pc",
                  lambda: app.click(selector="#" + I("side-this-pc")))
        dialog = open_dialog(app, "the source list",
                             lambda: app.click(selector="#" + I("side-add-drive")))
        tiers = app.after("the tiers", "AZDRIVE_TIERS", r"\d+",
                          lambda: dialog.page("buy", lambda: dialog.click("choice_buy")))
        if tiers != "6":
            raise Failure("Buy storage shows %s tiers, the token server has 6" % tiers)
        for text in ("100 GB", "EUR 0.99 a month", "12 TB", "EUR 34.99 a month"):
            dialog.win.until("the price %s" % text, lambda: dialog.shows(text))
        dialog.screenshot(os.path.join(out, "5-buy-monthly.png"))
        dialog.click("yearly")
        dialog.win.until("the yearly price", lambda: dialog.shows("EUR 9.90 a year"))
        dialog.click("tier_1")
        dialog.win.until("the Buy button's tier",
                         lambda: dialog.shows("Buy 500 GB - EUR 29.90 a year"))
        dialog.screenshot(os.path.join(out, "5-buy-yearly.png"))
        bought = app.after("the test drive", "AZDRIVE_ADDED", r"d_\S+",
                           lambda: dialog.click("create_test"))
        wait_closed(app)
        app.until("the new drive's empty bucket", lambda: app.printed(
            "AZDRIVE_LISTED", r"%s / 0" % re.escape(bought)))
        app.until("its row in CLOUD", lambda: app.has(side_drive(bought)))
        text, entries = drives_file_entries(drives_file)
        entry = next(e for e in entries if e["id"] == bought)
        auth = entry["location"].get("auth") or {}
        if auth.get("type") != "azlin" or auth.get("drive_id") != bought:
            raise Failure("the bought drive's auth is %s" % auth)
        if "dt_" in text or "secret" in text.lower():
            raise Failure("the drives file holds the drive token or a secret: %s" % text)
        drive = stack.token.state.drives.get(bought)
        if not drive or drive["tier"] != "500GB":
            raise Failure("the token server made %s" % stack.token.state.drives)
        stack.s3.store.write(drive["bucket"], "hello.txt", b"hello from Azlin\n")
        app.after("F5", "AZDRIVE_LISTED", r"%s / 1" % re.escape(bought), lambda: app.key("f5"))
        app.until("the bucket's file", lambda: "hello.txt" in item_names(app))
        app.screenshot(os.path.join(out, "5-azlin-drive.png"))
        log("5. Buy storage: six tiers with their prices, yearly ones after Pay yearly, Create "
            "test drive made %s (500 GB) - in the drives file with its azlin auth and no secret, "
            "its bucket listed (and its new file after F5)" % bought)

        log("PASS: Add drive connected an S3 bucket, a folder and a SQLite database (tables as "
            "folders) and bought a test drive, from the source list, This PC's ribbon and "
            "Home's ribbon, in the dialog's own window")
        return True
    except Failure:
        for name, path in (("stdout", app.out_path), ("stderr", app.err_path)):
            print("\n----- azdrive %s (tail) -----\n%s" % (name, e2e.tail(path)))
        raise
    finally:
        app.stop()
        stack.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8783)
    parser.add_argument("--timeout", type=float, default=240)
    parser.add_argument("--out")
    parser.add_argument("--keep-logs", action="store_true")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azdrive-add-e2e-")
    ok = False
    try:
        ok = run(args, logs)
    except Failure as e:
        log("FAIL: %s" % e)
    finally:
        if ok and not args.keep_logs and not args.out:
            shutil.rmtree(logs, ignore_errors=True)
        else:
            log("kept %s" % logs)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
