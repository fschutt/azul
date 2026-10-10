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
    6. A paid drive reaches AzDrive however late (the claim, CLAIM CONTRACT v1): Buy names a
       claim key (the mock keeps its public half), "Stop waiting", the payment is approved at
       the mock - the drive joins the source list in the background, without taking over the
       window; then a second checkout, "Stop waiting", AzDrive closes, the payment is approved
       while it is closed, AzDrive starts again and the drive arrives at its start: in the
       drives file under the name typed, its session in the keyring, the checkout off the
       keyring's list, its bucket listed.

    7. A card payment in the popover (CHECKOUT-PLAN §4.3, the mock's fake providers on): Buy
       storage shows the pills (direct debit via Fake GoCardless, card via Fake Stripe), the card
       pill and the consent, Buy: the checkout goes through fake-stripe with a claim key; the
       payment popover - its own window - shows the verified chip, the card artwork and Fake
       Stripe's fields page in its <webview> (list_webviews); the fields page "says" ready,
       visa and complete (simulate_webview_navigation to /_bridge/...: each cancelled), the
       artwork shows VISA and the typed name, Pay tells the page to confirm (a fragment
       navigation of the same page, with the name), the fake provider's signed webhook
       approves, the page "says" succeeded: the drive arrives.
    8. Direct debit through Fake GoCardless's hosted page in the popover: the provider's return
       redirect to /return/ok is cancelled and the dialog waits; the mandate's webhook approves:
       the drive arrives.
    9. A navigation off the provider's origins is blocked (the chip stays the provider's), a
       PayPal login the card page jumps to goes to the system browser (AZDRIVE_OPEN_BROWSER, the
       host only) and the dialog waits: the payment made there brings the drive.
   10. A load failure falls back to the hosted page of the same checkout, a second one to the
       system browser; Stop waiting keeps the claim.
   11. Closing the dialog while the popover shows the fields abandons the checkout at the token
       server and takes it off the keyring's list.

Url::open starts no browser in a headless run (the engine's stand-in), so the payment page of
step 6 stays closed and the mock's test provider is paid directly; in steps 7 - 11 nothing loads
in the web views either: the scenario plays the pages (simulate_webview_*) and the providers
(their signed webhooks).

Usage (from the azul repository, after building libazul with the debug server and AzDrive with
its default features `opendal` and `sql` plus `fake-providers` - steps 7 - 11 need the fakes in
azul-pay's registry: `cargo build --release -p AzDrive --features fake-providers`):

    python3 scripts/azdrive_add_e2e.py [--bin target/release/AzDrive] [--debug-port 8783]
        [--timeout 240] [--out /tmp/azdrive-add-shots] [--keep-logs]

Every key_down has its key_up (the E2E key_up rule); the shared Azlin config is a temporary one
(azlin_e2e sets AZLIN_CONFIG), the keyring the headless backend's stand-in kept in a file of the
run (AZ_KEYRING_FILE: it outlives AzDrive's restart in step 6).
"""

import argparse
import base64
import json
import os
import re
import shutil
import sqlite3
import sys
import tempfile

import azlin_claim
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
        """Clicks the dialog's control `short` - scrolled into view first: the sources and a
        long form scroll inside the dialog, and a click lands where the control is painted."""
        selector = add_id(short) if not short.startswith("#") else short
        self.win.until(what or selector, lambda: self.win.has(selector))
        self.win.op("scroll_into_view", selector=selector, block="center", behavior="instant")
        self.win.frame(2)
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


# The keyring entries azcloud-kit keeps: the unfinished checkouts, a drive's session.
PENDING_KEY = "azcloud/checkouts"


def keyring_entries(path):
    """The headless keyring kept in the run's file (AZ_KEYRING_FILE): entry name -> text."""
    try:
        with open(path, "r", encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return {}
    return json.loads(text) if text.strip() else {}


def pending_checkouts(path):
    """The keyring's list of unfinished checkouts, by checkout id."""
    text = keyring_entries(path).get(PENDING_KEY)
    if not text:
        return {}
    return {c["checkout_id"]: c for c in json.loads(text).get("checkouts") or []}


def buy_and_stop_waiting(app, stack, keyring_file, name):
    """Add drive -> Buy storage -> 100 GB named `name` -> Buy -> Stop waiting: the checkout's id
    (its claim key checked at the mock and on the keyring's list) and the dialog."""
    dialog = open_dialog(app, "the source list",
                         lambda: app.click(selector="#" + I("side-add-drive")))
    app.after("the tiers", "AZDRIVE_TIERS", r"\d+",
              lambda: dialog.page("buy", lambda: dialog.click("choice_buy")))
    dialog.click("tier_0")
    dialog.type_into("name", name, clear=len("Azlin Storage"))
    checkout = app.after("the checkout", "AZDRIVE_CHECKOUT", r"ck_\S+",
                         lambda: dialog.click("buy_button"))
    record = stack.token.state.checkouts.get(checkout) or {}
    if len(base64.b64decode(record.get("claim_key") or "")) != 32:
        raise Failure("the checkout names no 32-byte claim key: %r" % record.get("claim_key"))
    kept = pending_checkouts(keyring_file).get(checkout)
    if not kept:
        raise Failure("the checkout %s is not on the keyring's list: %r"
                      % (checkout, sorted(pending_checkouts(keyring_file))))
    # The keyring keeps the secret, the token server got its public half only.
    secret = base64.b64decode(kept["claim_secret"])
    if base64.b64encode(azlin_claim.public_key(secret)).decode("ascii") != record["claim_key"]:
        raise Failure("the claim key at the token server is not the kept secret's public half")
    if kept.get("name") != name or kept.get("tier") != "100GB":
        raise Failure("the kept checkout is %r" % {k: v for k, v in kept.items()
                                                   if k != "claim_secret"})
    dialog.win.until("Stop waiting", lambda: dialog.win.has(add_id("stop")))
    dialog.click("stop")
    dialog.win.until("stopped waiting", lambda: dialog.shows("Stopped waiting"))
    return checkout, dialog


# ==== The payment through azul-pay (steps 7 - 11) ====

# The payment popover's ids (examples/azul-drive/src/ids.rs).
POPOVER = "#__azdrive_pay_popover"


def pay_id(name):
    return "#__azdrive_pay_" + name


def open_payment(app, pill, what):
    """Add drive -> Buy storage (the fake providers' pills) -> the pill `pill`, the consent
    ticked -> Buy: the checkout's id and the dialog."""
    dialog = open_dialog(app, "the source list",
                         lambda: app.click(selector="#" + I("side-add-drive")))
    pills = app.after("the payment pills", "AZDRIVE_PILLS", r".+",
                      lambda: dialog.page("buy", lambda: dialog.click("choice_buy")))
    if pills == "-":
        raise Failure("Buy storage shows no payment pills: build AzDrive with --features "
                      "fake-providers (azul-pay's fakes are taken from a local token server only)")
    for wanted in ("sepa_debit:fake-gocardless", "card:fake-stripe"):
        if wanted not in pills.split():
            raise Failure("the pills are %r, without %s" % (pills, wanted))
    dialog.click("pill_" + pill)
    dialog.click("consent")
    checkout = app.after(what, "AZDRIVE_CHECKOUT", r"ck_\S+", lambda: dialog.click("buy_button"))
    return checkout, dialog


def popover_of(app, dialog):
    """The payment popover's window: a transient window of the dialog's."""
    win = e2e.modal_window(app, known=(dialog.win.window_id,))
    win.until("the payment popover", lambda: win.has(POPOVER))
    return win


def webviews(win):
    """The web views of `win` (`list_webviews`)."""
    value = win.value("list_webviews")
    if not isinstance(value, dict):
        return []
    return value.get("webviews") or []


def the_view(win):
    """The one web view of the popover's window."""
    views = win.until("the popover's web view", lambda: webviews(win))
    if len(views) != 1:
        raise Failure("the popover's window has %d web views: %s" % (len(views), views))
    return views[0]


def navigate(win, view, url, redirect=False):
    """The page in `view` navigates to `url` (a link, a script, a server redirect)."""
    win.must("simulate_webview_navigation", webview=view["id"], url=url, redirect=redirect)
    win.frame(3)


def navigation_to(win, needle):
    """The last navigation of the popover's web view whose URL holds `needle`."""
    found = [n for v in webviews(win) for n in v.get("navigations") or []
             if needle in n.get("url", "")]
    return found[-1] if found else None


def new_drive(app, known):
    """The drive AZDRIVE_ADDED names that is none of `known`."""
    return app.until("a new drive", lambda: [d for d in app.printed("AZDRIVE_ADDED", r"d_\S+")
                                              if d not in known])[-1]


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
    # The headless keyring in a file of this run: it outlives AzDrive's restart (step 6). The
    # payer pays from Germany whatever this machine's locale is (the pills of steps 7 - 11).
    keyring_file = os.path.join(logs, "keyring.json")
    env = {"AZ_KEYRING_FILE": keyring_file, "AZLIN_COUNTRY": "DE"}
    app = Drive("azdrive", binary, switches, args.debug_port, logs, args.timeout, extra_env=env)
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

        # 6a. Paid after "Stop waiting": the drive joins the list in the background.
        place_lines = app.count("AZDRIVE_PLACE")
        first, dialog = buy_and_stop_waiting(app, stack, keyring_file, "Paid later")
        stack.token.state.pay(first, {"card_number": azlin_mock_stack.APPROVING_CARD})
        late = app.until("the drive paid after Stop waiting", lambda: [
            d for d in app.printed("AZDRIVE_ADDED", r"d_\S+") if d != bought])[-1]
        app.until("its row in CLOUD", lambda: app.has(side_drive(late)))
        if app.count("AZDRIVE_PLACE") != place_lines:
            raise Failure("the late drive took the window elsewhere: %s"
                          % app.last("AZDRIVE_PLACE"))
        if len(app.window_ids()) < 2:
            raise Failure("the late drive closed the dialog it did not come from")
        app.until("the checkout off the keyring's list",
                  lambda: first not in pending_checkouts(keyring_file))
        dialog.click("cancel")
        wait_closed(app)
        log("6a. Buy -> Stop waiting -> paid at the token server: %s joined the source list in "
            "the background (the window stayed where it was), its checkout left the keyring's "
            "list" % late)

        # 6b. Paid while AzDrive is closed: the drive arrives at the next start.
        second, dialog = buy_and_stop_waiting(app, stack, keyring_file, "Paid while closed")
        dialog.click("cancel")
        wait_closed(app)
        app.stop()
        stack.token.state.pay(second, {"card_number": azlin_mock_stack.APPROVING_CARD})
        if second not in pending_checkouts(keyring_file):
            raise Failure("the checkout %s did not outlive AzDrive in the keyring" % second)
        app = Drive("azdrive-restarted", binary, switches, args.debug_port, logs, args.timeout,
                    extra_env=env)
        app.until("the This PC view", lambda: app.printed("AZDRIVE_PLACE", r"this-pc"))
        app.until("the debug server", lambda: app.op("get_dom_tree"))
        paid = app.until("the drive paid while AzDrive was closed", lambda: [
            d for d in app.printed("AZDRIVE_ADDED", r"d_\S+") if d not in (bought, late)])[-1]
        app.until("its row in CLOUD", lambda: app.has(side_drive(paid)))
        app.until("the checkout off the keyring's list",
                  lambda: second not in pending_checkouts(keyring_file))
        text, entries = drives_file_entries(drives_file)
        entry = next((e for e in entries if e["id"] == paid), None)
        if not entry or entry["name"] != "Paid while closed":
            raise Failure("the drives file's entry of %s is %s" % (paid, entry))
        session = json.loads(keyring_entries(keyring_file).get("azul-storage/s3/" + paid) or "{}")
        if session.get("drive_id") != paid or not session.get("drive_token"):
            raise Failure("the keyring has no session of %s" % paid)
        if "dt_" in text:
            raise Failure("the drives file holds a drive token: %s" % text)
        if (stack.token.state.drives.get(paid) or {}).get("tier") != "100GB":
            raise Failure("the token server made %s" % stack.token.state.drives.get(paid))
        app.after("the paid drive's bucket", "AZDRIVE_LISTED", r"%s / 0" % re.escape(paid),
                  lambda: app.click(selector=side_drive(paid)))
        app.screenshot(os.path.join(out, "6-claimed.png"))
        log("6b. Buy -> Stop waiting -> AzDrive closed -> paid -> AzDrive started: %s arrived at "
            "the start under the name typed, its session in the keyring, its checkout off the "
            "keyring's list, its bucket listed" % paid)

        # 7. A card payment in the popover: Fake Stripe's fields in the web view.
        stack.token.state.set_providers(list(azlin_mock_stack.DEFAULT_PROVIDERS))
        token = stack.token_url
        known = {bought, late, paid}
        card, dialog = open_payment(app, "card", "the card checkout")
        record = stack.token.state.checkouts.get(card) or {}
        if record.get("provider") != "fake-stripe" or record.get("method") != "card":
            raise Failure("the checkout went through %r" % {k: record.get(k) for k in
                                                            ("provider", "method", "surface")})
        if len(base64.b64decode(record.get("claim_key") or "")) != 32:
            raise Failure("the card checkout names no claim key")
        if card not in pending_checkouts(keyring_file):
            raise Failure("the card checkout is not on the keyring's list before it shows")
        app.until("the fields in the popover", lambda: app.printed(
            "AZDRIVE_PAY_SURFACE", r"fields 127\.0\.0\.1:\d+"))
        popover = popover_of(app, dialog)
        view = the_view(popover)
        if not view.get("src", "").startswith(token + "/fields/fake-stripe/v1#pk="):
            raise Failure("the web view shows %r, not the fields page with its inputs in the "
                          "fragment" % view.get("src", "")[:60])
        if "?" in view["src"].split("#")[0]:
            raise Failure("the fields page's inputs went into its query")
        for text in ("127.0.0.1", "card fields by Fake Stripe", "CARDHOLDER"):
            popover.until("the popover's %r" % text, lambda: popover.shows(text))
        for message in ("ready", "brand?v=visa", "complete?v=1"):
            navigate(popover, view, token + "/_bridge/" + message)
            heard = navigation_to(popover, "/_bridge/" + message)
            if not heard or heard["allowed"]:
                raise Failure("the bridge message %s was not cancelled: %s" % (message, heard))
        popover.until("VISA on the card", lambda: popover.shows("VISA"))
        popover.must("focus_node", selector=pay_id("name"))
        popover.frame(2)
        popover.must("text_input", text="Erika Example")
        popover.frame(3)
        popover.until("the name on the card", lambda: popover.shows("ERIKA EXAMPLE"))
        popover.screenshot(os.path.join(out, "7-card-popover.png"))
        popover.click(selector=pay_id("confirm"))
        confirm = popover.until("the confirm command", lambda: navigation_to(popover, "cmd=confirm"))
        if not confirm["allowed"] or "name=Erika+Example" not in confirm["url"] \
                or not confirm["url"].startswith(token + "/fields/fake-stripe/v1#pk="):
            raise Failure("Pay told the page %r" % confirm)
        if app.last("AZDRIVE_PAY") != "confirming":
            raise Failure("Pay left the checkout %s" % app.last("AZDRIVE_PAY"))
        # The fake provider takes the card: its signed webhook reaches the token server.
        stack.token.state.provider_pays(card, True)
        if (stack.token.state.webhooks[-1:] or [{}])[0].get("outcome") != "approved":
            raise Failure("the provider's webhook did %s" % stack.token.state.webhooks[-1:])
        navigate(popover, view, token + "/_bridge/result?v=succeeded")
        by_card = new_drive(app, known)
        known.add(by_card)
        wait_closed(app)
        app.until("the card checkout off the keyring's list",
                  lambda: card not in pending_checkouts(keyring_file))
        if (stack.token.state.drives.get(by_card) or {}).get("tier") != "100GB":
            raise Failure("the token server made %s" % stack.token.state.drives.get(by_card))
        log("7. Card via Fake Stripe: the popover (its own window) showed the chip, the artwork "
            "(VISA, ERIKA EXAMPLE) and the fields page in its web view; ready / brand / complete "
            "were cancelled bridge messages; Pay was a fragment command with the name; the "
            "signed webhook approved; %s arrived" % by_card)

        # 8. Direct debit: Fake GoCardless's hosted page in the popover.
        debit, dialog = open_payment(app, "sepa_debit", "the direct debit checkout")
        if (stack.token.state.checkouts.get(debit) or {}).get("provider") != "fake-gocardless":
            raise Failure("the direct debit went through %s"
                          % (stack.token.state.checkouts.get(debit) or {}).get("provider"))
        app.until("the hosted page in the popover", lambda: app.printed(
            "AZDRIVE_PAY_SURFACE", r"page 127\.0\.0\.1:\d+"))
        popover = popover_of(app, dialog)
        view = the_view(popover)
        if "/fake-gocardless/flow/BRQpr_" not in view.get("src", ""):
            raise Failure("the web view shows %r" % view.get("src", "")[:80])
        popover.until("the chip", lambda: popover.shows("payment page of Fake GoCardless"))
        popover.screenshot(os.path.join(out, "8-debit-popover.png"))
        # The payer confirms the mandate on the page: the provider's webhook, then its redirect.
        stack.token.state.provider_pays(debit, True)
        navigate(popover, view, token + "/return/ok", redirect=True)
        returned = navigation_to(popover, "/return/ok")
        if not returned or returned["allowed"]:
            raise Failure("the return page loaded in the web view: %s" % returned)
        by_debit = new_drive(app, known)
        known.add(by_debit)
        wait_closed(app)
        log("8. Direct debit via Fake GoCardless: its hosted page in the popover, the return "
            "redirect cancelled, the mandate's webhook approved: %s arrived" % by_debit)

        # 9. Off the provider's origins: blocked; a PayPal login: the system browser.
        jumped, dialog = open_payment(app, "card", "the checkout that jumps to PayPal")
        app.until("the fields", lambda: app.count("AZDRIVE_PAY_SURFACE", r"fields .+") >= 2)
        popover = popover_of(app, dialog)
        view = the_view(popover)
        navigate(popover, view, token + "/fields/fake-stripe/v1#pk=pk_test_fake_local")
        blocked = app.after("the blocked navigation", "AZDRIVE_PAY_BLOCKED", r"\S+",
                            lambda: navigate(popover, view, "https://evil.example/login"))
        if blocked != "evil.example":
            raise Failure("the blocked host is %r" % blocked)
        for url in ("https://127.0.0.1.evil.example/", "http://evil.example/",
                    "javascript:alert(1)", token.replace("http://", "http://user@") + "/"):
            navigate(popover, view, url)
        if any(n["allowed"] for n in (webviews(popover)[0].get("navigations") or [])
               if "evil" in n["url"] or "javascript" in n["url"] or "user@" in n["url"]):
            raise Failure("a navigation off the provider's origins went ahead: %s"
                          % webviews(popover)[0].get("navigations"))
        popover.until("the chip still the provider's", lambda: popover.shows("127.0.0.1"))
        popover.until("the notice", lambda: popover.shows("it was blocked"))
        popover.screenshot(os.path.join(out, "9-blocked.png"))
        ref = stack.token.state.checkouts[jumped]["provider_ref"]
        login = stack.token.state.login_url + "/fake-paypal/checkoutnow?token=" + ref
        opened = app.after("the system browser", "AZDRIVE_OPEN_BROWSER", r"\S+",
                           lambda: navigate(popover, view, login, redirect=True))
        if not opened.startswith("localhost:"):
            raise Failure("the browser opened %r, not the login's host" % opened)
        app.until("the dialog waiting", lambda: app.last("AZDRIVE_PAY") == "waiting")
        app.until("the popover closed", lambda: len(app.window_ids()) <= 2)
        dialog.win.until("Open the page again", lambda: dialog.win.has(add_id("open_again")))
        stack.token.state.provider_pays(jumped, True)
        by_browser = new_drive(app, known)
        known.add(by_browser)
        wait_closed(app)
        log("9. evil.example and its look-alikes were blocked with the chip unchanged; the PayPal "
            "login went to the system browser (%s) and the payment there brought %s"
            % (opened, by_browser))

        # 10. Load failures: the hosted page of the same checkout, then the system browser.
        failing, dialog = open_payment(app, "card", "the checkout whose page fails")
        app.until("the fields", lambda: app.count("AZDRIVE_PAY_SURFACE", r"fields .+") >= 3)
        popover = popover_of(app, dialog)
        view = the_view(popover)
        page_lines = app.count("AZDRIVE_PAY_SURFACE", r"page .+")
        popover.must("simulate_webview_load_failed", webview=view["id"], reason="offline")
        app.until("the hosted page", lambda: app.count("AZDRIVE_PAY_SURFACE", r"page .+")
                  > page_lines)
        popover = popover_of(app, dialog)
        popover.until("the hosted page in the web view", lambda: any(
            "/fake-stripe/c/pay/cs_test_" in v.get("src", "") for v in webviews(popover)))
        view = the_view(popover)
        opened = app.after("the system browser", "AZDRIVE_OPEN_BROWSER", r"\S+",
                           lambda: popover.must("simulate_webview_load_failed",
                                                webview=view["id"], reason="offline"))
        if not opened.startswith("127.0.0.1:"):
            raise Failure("the browser opened %r" % opened)
        if stack.token.state.checkouts[failing]["surface"] != "browser":
            raise Failure("the checkout is on %s" % stack.token.state.checkouts[failing]["surface"])
        if sum(1 for c in stack.token.state.checkouts.values()
               if c.get("claim_key") == stack.token.state.checkouts[failing]["claim_key"]) != 1:
            raise Failure("a fallback made a second checkout")
        dialog.win.until("Stop waiting", lambda: dialog.win.has(add_id("stop")))
        app.after("stopped", "AZDRIVE_PAY", r"stopped", lambda: dialog.click("stop"))
        dialog.win.until("Check again", lambda: dialog.win.has(add_id("check_again")))
        if failing not in pending_checkouts(keyring_file):
            raise Failure("Stop waiting dropped the claim")
        dialog.click("cancel")
        wait_closed(app)
        log("10. A failed load moved %s to its hosted page, a second one to the system browser "
            "(%s) - the same checkout; Stop waiting kept its claim" % (failing, opened))

        # 11. Closing the dialog while the popover shows abandons the checkout.
        closed, dialog = open_payment(app, "card", "the checkout that is closed")
        app.until("the fields", lambda: app.count("AZDRIVE_PAY_SURFACE", r"fields .+") >= 4)
        popover_of(app, dialog)
        abandoned = app.after("the abandon", "AZDRIVE_ABANDONED", r"\S+ \S+",
                              lambda: dialog.click("cancel"))
        if abandoned != "%s ok" % closed or closed not in stack.token.state.abandoned:
            raise Failure("the closed checkout: %r, the mock abandoned %s"
                          % (abandoned, stack.token.state.abandoned))
        wait_closed(app)
        app.until("the abandoned checkout off the keyring's list",
                  lambda: closed not in pending_checkouts(keyring_file))
        if (stack.token.state.checkouts.get(closed) or {}).get("status") != "expired":
            raise Failure("the abandoned checkout is %s"
                          % stack.token.state.checkouts.get(closed, {}).get("status"))
        log("11. Closing the dialog while the popover showed the fields abandoned %s at the "
            "token server and took it off the keyring's list" % closed)

        log("PASS: Add drive connected an S3 bucket, a folder and a SQLite database (tables as "
            "folders), bought a test drive, and claimed two paid drives - one in the background "
            "after Stop waiting, one at the start after AzDrive was closed - from the source list, "
            "This PC's ribbon and Home's ribbon, in the dialog's own window; and paid through "
            "azul-pay: a card in the popover's fields, a direct debit on a hosted page, PayPal in "
            "the system browser, with blocked navigations, the fallback chain and an abandon")
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
