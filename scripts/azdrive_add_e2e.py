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
       the new Azlin drive is ENCRYPTED AS IT IS MADE ("we always encrypt") - its recovery
       sheet: the kit saved as a PDF, Escape keeps the sheet, the making finishes only with the
       four groups it asks for typed back (a wrong one refused; the recovery key at the token
       server); it is in the drives file with its {"type": "azlin"} auth (no secret), its empty
       drive lists, a file copied into it from step 3's folder shows after F5 - and the bucket
       holds only the encryption's random names (no file name, no plaintext).
    6. A paid drive reaches AzDrive however late (the claim, CLAIM CONTRACT v1): Buy names a
       claim key (the mock keeps its public half), "Stop waiting", the payment is approved at
       the mock - the drive joins the source list in the background, without taking over the
       window; then a second checkout, "Stop waiting", AzDrive closes, the payment is approved
       while it is closed, AzDrive starts again and the drive arrives at its start: in the
       drives file under the name typed, its session in the keyring, the checkout off the
       keyring's list, its bucket listed - each one encrypted as it arrived (its recovery
       sheet, shown once no other dialog is open). Each paid checkout's period tokens (AZLINSEC17 F24)
       are issued against the issue key of its sealed sign-up before its checkout leaves the
       list, and AzDrive keeps them (a 0600 file per drive beside the drives file), each one
       a token the mock's issuer key verifies. The first drive's period ends in two days: a
       token buys it a month at once (redeemed under the drive's lock), the second one's is a
       month away: its tokens wait.
       Then (6c - 6h) the paid drive at the mock: its period bought by the daily look, a
       node's error in the table's words, a pending recovery-key lockdown cancelled, vouchers,
       a device added at the token server announced (AZDRIVE_NEW_DEVICE), and Options > Drives
       > "Restore as of..." putting its bucket back as it was at a time (AZDRIVE_RESTORED).

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
   12. A consumer cloud's sign-in (AuthSession): Connect data source > Dropbox without an OAuth
       client id says which setting is missing (AZDRIVE_DROPBOX_CLIENT_ID); Google Drive with
       one (AZDRIVE_GOOGLE_CLIENT_ID, the mock's token endpoint as AZDRIVE_GOOGLE_TOKEN_URL)
       signs in through azul's headless sign-in fake (AZ_AUTH_SESSION_REDIRECT: the redirect
       carries the mock's code for the PKCE challenge and the request's state), the mock's
       token endpoint checks the code against the verifier (PKCE S256), the client and the
       redirect URI; Add drive keeps the refresh token in the keyring only (the drives file has
       the client id and the token endpoint), and the drive refreshes its access token at that
       endpoint before its first listing (googleapis.com answered by azul's request mock: an
       empty My Drive).
   13. The recovery methods of an encrypted drive (C14; `--recovery`): on step 5's test drive
       (encrypted as it was made, its sheet in step 5) a drill from Options > Drives (a wrong code fails, the kit's code passes); three trusted
       contacts without AzDrive (printed shares, two saved as PDFs: Recovery health green); a
       second AzDrive that never had the drive recovers it with two printed shares - the code
       they give back signs the lockdown, the token server holds it 48 hours without
       credentials for it while the owner's AzDrive shows it, then (the mock's clock advanced)
       hands the drive over, the owner's old token is refused and the code unlocks the drive.

Url::open starts no browser in a headless run (the engine's stand-in), so the payment page of
step 6 stays closed and the mock's test provider is paid directly; in steps 7 - 11 nothing loads
in the web views either: the scenario plays the pages (simulate_webview_*) and the providers
(their signed webhooks).

Usage (from the azul repository, after building libazul with the debug server and AzDrive with
its default features `opendal` and `sql` plus `fake-providers` - steps 7 - 11 need the fakes in
azul-pay's registry: `cargo build --release -p AzDrive --features fake-providers`):

    python3 scripts/azdrive_add_e2e.py [--bin target/release/AzDrive] [--debug-port 8783]
        [--timeout 240] [--out /tmp/azdrive-add-shots] [--keep-logs] [--recovery]

(`encryption` is one of AzDrive's default features: every drive bought here is encrypted as it is
made, and a build without it fails at step 5. `--recovery`: step 13; the second AzDrive of step
13 uses the debug port after `--debug-port`.)

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
import time

import azlin_claim
import azlin_client
import azlin_e2e as e2e
import azlin_mock_stack
import azlin_period
from azlin_e2e import Failure
from azdrive_e2e import Drive, I, item_names, open_item, select_item

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


def check_period_tokens(stack, drives_file, checkout, drive_id, redeemed=0):
    """AZLINSEC17 F24: the paid checkout's period tokens were issued (against the issue key of
    its sealed sign-up) and AzDrive keeps those not `redeemed` yet - one 0600 file per drive
    beside the drives file, none once all are spent - each one a token the mock's issuer key
    verifies. How many were issued."""
    record = stack.token.state.checkouts.get(checkout) or {}
    months = record.get("months")
    if not months or record.get("tokens_issued") != months:
        raise Failure("the checkout %s's period tokens were not issued: %s of %r"
                      % (checkout, record.get("tokens_issued"), months))
    path = os.path.join(os.path.dirname(drives_file), "period-tokens", drive_id + ".json")
    if months == redeemed:
        if os.path.exists(path):
            raise Failure("every period token of %s is spent, but %s is left" % (drive_id, path))
        return months
    try:
        with open(path, "r", encoding="utf-8") as f:
            tokens = json.load(f).get("tokens") or []
    except (OSError, ValueError) as e:
        raise Failure("AzDrive keeps no period tokens of %s at %s: %s" % (drive_id, path, e))
    if os.name == "posix" and os.stat(path).st_mode & 0o077:
        raise Failure("the period tokens file is readable by others: %o" % os.stat(path).st_mode)
    n, e, _ = stack.token.state.issuer
    if len(tokens) != months - redeemed or not all(azlin_period.verify(n, e, t) for t in tokens):
        raise Failure("the kept period tokens of %s are %d, not %d that verify"
                      % (drive_id, len(tokens), months - redeemed))
    return months


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


class Payment:
    """One Buy through azul-pay: its checkout id, the dialog, the surface it opened (the
    AZDRIVE_PAY_SURFACE or AZDRIVE_OPEN_BROWSER line printed after THIS Buy: `fields <host>`,
    `page <host>`, `browser <host>`) and the drives AzDrive had added before it (AZDRIVE_ADDED:
    a drive of an earlier step - a voucher's, a claim's - is never taken for this one's)."""

    def __init__(self, checkout, dialog, surface, added):
        self.checkout, self.dialog, self.surface, self.added = checkout, dialog, surface, added


def open_payment(app, pill, what):
    """Add drive -> Buy storage (the fake providers' pills) -> the pill `pill`, the consent
    ticked -> Buy, and the surface the checkout opened: a Payment."""
    added = set(app.printed("AZDRIVE_ADDED", r"d_\S+"))
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
    shown = app.count("AZDRIVE_PAY_SURFACE", r".+")
    browsers = app.count("AZDRIVE_OPEN_BROWSER", r".+")
    checkout = app.after(what, "AZDRIVE_CHECKOUT", r"ck_\S+", lambda: dialog.click("buy_button"))

    def surface():
        if app.count("AZDRIVE_PAY_SURFACE", r".+") > shown:
            return app.printed("AZDRIVE_PAY_SURFACE", r".+")[-1]
        if app.count("AZDRIVE_OPEN_BROWSER", r".+") > browsers:
            return "browser " + app.printed("AZDRIVE_OPEN_BROWSER", r".+")[-1]
        return None
    return Payment(checkout, dialog, app.until("the surface of %s" % checkout, surface), added)


def popover_of(app, dialog):
    """The payment popover's window: a transient window of the dialog's - the one that shows
    the popover now, looked for anew on every try (a window that closed meanwhile, or one whose
    id an earlier popover had, is never taken)."""
    def showing():
        for window in app.window_ids()[1:]:
            if window == dialog.win.window_id:
                continue
            win = e2e.InWindow(app, window)
            if win.has(POPOVER):
                return win
        return None
    return app.until("the payment popover", showing)


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


def new_drive(app, payment):
    """The drive `payment` brought: the first AZDRIVE_ADDED after its Buy."""
    return app.until("the drive of %s" % payment.checkout, lambda: [
        d for d in app.printed("AZDRIVE_ADDED", r"d_\S+") if d not in payment.added])[0]


def hand_back(app, popover, view, url, redirect=False):
    """The page in the popover hands the payment back (a bridge result, a return page, a login
    for the browser): the dialog waits for the drive, and the popover - which shows a page only
    while the payment is presented (CHECKOUT-PLAN §2.4) - closes. Nothing is asked of the
    popover's window afterwards: it is gone."""
    app.after("the dialog waiting for the drive", "AZDRIVE_PAY", r"waiting",
              lambda: navigate(popover, view, url, redirect))
    app.until("the popover closed", lambda: popover.window_id not in app.window_ids())


# ==== 13. Recovery methods (C14): AzDrive built with the encryption feature ====

# The recovery code as the sheet shows it, and the groups the sheet asks for.
CODE_RE = re.compile(r"\b[0-9A-Z]{5}-[0-9A-Z]{5}-[0-9A-Z]{5}-[0-9A-Z]{5}-[0-9A-Z]{6}\b")
ASKED_RE = re.compile(r"type groups ([0-9, and]+) of the code")
SHARE_RE = re.compile(r"\bS[123]-[0-9A-F]{8}-[0-9A-Z]{5}-[0-9A-Z]{5}-[0-9A-Z]{5}-[0-9A-Z]{5}-"
                      r"[0-9A-Z]{6}\b")


def drive_menu(app, drive_id, entry):
    """Right-clicks the drive's row in the source list and picks `entry` from its menu."""
    before = len(app.window_ids())
    app.must("click", selector=side_drive(drive_id), button="right")
    app.frame(2)
    menu = app.until("the drive's menu",
                     lambda: app.popup() if len(app.window_ids()) > before else None)
    app.click_exact(entry, window=menu)


def method_button(drive_id, method, action):
    """Options > Drives' button of a recovery method (ids.rs method_button)."""
    return "#__azdrive_method_%s_%s_%s" % (re.sub(r"[^A-Za-z0-9_-]", "_", drive_id).lower(),
                                           method, action)


def retype(win, selector, old, text):
    """Replaces the `old` text of the field `selector` with `text` (End, Backspaces, typing)."""
    win.must("focus_node", selector=selector)
    win.frame(2)
    win.key("end")
    for _ in range(len(old)):
        win.key("backspace", frames=1)
    if text:
        win.must("text_input", text=text)
    win.frame(2)


def found(texts, pattern):
    for text in texts:
        match = pattern.search(text or "")
        if match:
            return match
    return None


def saved_pdf(app, what):
    """The newest export recorded by FileDialog::save_bytes is a PDF of some size."""
    answer = app.op("assert_saved_file", name_ends_with=".pdf", mime="application/pdf",
                    min_len=2000, contains="%PDF")
    if not isinstance(answer, dict) or answer.get("status") == "error":
        raise Failure("%s was not saved as a PDF: %s" % (what, json.dumps(answer)[:300]))


def settings_text(data_dir):
    """AzDrive's settings file in the data folder (drive/view.json)."""
    for root, _dirs, files in os.walk(data_dir):
        if "view.json" in files and os.path.basename(root) == "drive":
            with open(os.path.join(root, "view.json"), "r", encoding="utf-8") as f:
                return f.read()
    return ""


def finish_new_drive_sheet(app, drive_id, out=None, thorough=False, stack=None, logs=None):
    """A new drive's recovery sheet ("we always encrypt": the drive is encrypted as it is made):
    the code and the four groups it asks for typed back; the drive is ready then
    (AZDRIVE_ENCRYPTED_NEW_DRIVE). `thorough` (step 5, once): the kit saved as a PDF first,
    Escape keeps the sheet, "I have written it down" without the groups and with a wrong one is
    refused, a group in lower case is taken; the recovery key reaches the token server and the
    settings file keeps the check, never the code. Returns the code."""
    popup = e2e.modal_window(app)
    popup.until("the recovery sheet of %s" % drive_id,
                lambda: popup.has("#__azdrive_sheet_group_0"))
    texts = popup.texts()
    code = found(texts, CODE_RE)
    asked = found(texts, ASKED_RE)
    if not code or not asked:
        raise Failure("the sheet shows no code or no groups to type: %r" % texts[:20])
    code = code.group(0)
    groups = code.split("-")
    numbers = [int(n) for n in re.findall(r"\d+", asked.group(1))]
    if len(numbers) != 4 or len(set(numbers)) != 4 or not all(1 <= n <= 5 for n in numbers):
        raise Failure("the sheet asks for groups %r: four of five, each once" % numbers)
    if thorough:
        # Item 7: the code as text with a Copy (cleared again in a minute), the kit, the groups.
        if not popup.has("#__azdrive_sheet_code"):
            raise Failure("the sheet shows its code as no selectable text")
        if not popup.shows("Your new drive's recovery code") and not popup.shows(
                "Your new drive is encrypted"):
            raise Failure("the sheet is not the purchase's (a new drive's)")
        app.after("the code copied", "AZDRIVE_CODE_COPIED", re.escape(drive_id),
                  lambda: popup.click(selector="#__azdrive_sheet_copy"))
        popup.until("the copy's note", lambda: popup.shows("cleared in a minute"))
        app.op("mock", set={"save_bytes": {"accept": True}})
        app.after("the kit saved", "AZDRIVE_KIT_SAVED", r"\d+",
                  lambda: popup.click(selector="#__azdrive_kit_save"))
        saved_pdf(app, "the emergency kit")
        if out:
            popup.screenshot(os.path.join(out, "5-recovery-sheet.png"))
        popup.key("escape")
        app.frame(3)
        popup = e2e.modal_window(app)
        if not popup.has("#__azdrive_sheet_group_0"):
            raise Failure("Escape took the recovery sheet away before its groups were typed")
        popup.click(selector="#__azdrive_sheet_done", frames=3)
        if app.printed("AZDRIVE_RECOVERY_VERIFIED", re.escape(drive_id)):
            raise Failure("the setup finished without the groups typed back")
        if not popup.shows("are not all the code's"):
            raise Failure("the sheet did not say the groups are missing")
        for slot, number in enumerate(numbers):
            typed = groups[number - 1] if slot else "WRONG"
            popup.text_input("#__azdrive_sheet_group_%d" % slot, typed)
        popup.click(selector="#__azdrive_sheet_done", frames=3)
        if app.printed("AZDRIVE_RECOVERY_VERIFIED", re.escape(drive_id)):
            raise Failure("the setup finished with a wrong group")
        if app.printed("AZDRIVE_ENCRYPTED_NEW_DRIVE", re.escape(drive_id)):
            raise Failure("the drive was ready before its sheet was done")
        retype(popup, "#__azdrive_sheet_group_0", "WRONG", groups[numbers[0] - 1].lower())
    else:
        for slot, number in enumerate(numbers):
            popup.text_input("#__azdrive_sheet_group_%d" % slot, groups[number - 1])
    app.after("the setup finished", "AZDRIVE_RECOVERY_VERIFIED", re.escape(drive_id),
              lambda: popup.click(selector="#__azdrive_sheet_done"))
    app.until("the drive ready", lambda: app.printed(
        "AZDRIVE_ENCRYPTED_NEW_DRIVE", re.escape(drive_id)))
    if thorough:
        app.until("the recovery key registered", lambda: app.printed(
            "AZDRIVE_RECOVERY_KEY", re.escape(drive_id)))
        if not stack.token.state.drives[drive_id].get("recovery_pubkey"):
            raise Failure("the token server has no recovery key of %s" % drive_id)
        app.until("the recovery state in the settings file",
                  lambda: '"code_checked"' in settings_text(os.path.join(logs, "data")))
        if code in settings_text(os.path.join(logs, "data")):
            raise Failure("the settings file holds the recovery code")
    return code


def new_drive_encrypted(app, drive_id):
    """A drive that just arrived is encrypted as it was made: its keys, then its sheet."""
    app.until("the encryption of %s" % drive_id, lambda: app.printed(
        "AZDRIVE_ENCRYPTING_NEW_DRIVE", re.escape(drive_id)))
    return finish_new_drive_sheet(app, drive_id)


def bucket_holds_ciphertext_only(stack, bucket, names):
    """An encrypted drive's bucket: the encryption's own keys (.azlin/: the keys, the drive
    index; data/: the objects under random ids) - no file's name, no plaintext."""
    keys = stack.s3.store.keys(bucket)
    strays = [k for k in keys if not (k.startswith(".azlin/") or k.startswith("data/"))]
    if strays:
        raise Failure("the encrypted drive's bucket has plaintext keys: %r" % strays[:10])
    for name in names:
        if any(name in k for k in keys):
            raise Failure("the bucket names %s: %r" % (name, keys[:20]))
    if not any(k.startswith("data/") for k in keys):
        raise Failure("the bucket holds no encrypted object: %r" % keys[:20])
    return len(keys)


def recovery_steps(app, stack, args, logs, out, binary, switches, env, drives_file, keyring_file,
                   drive_id, code):
    """13. The recovery methods of an encrypted drive (C14, D51), on the test drive of step 5
    (encrypted as it was made: its sheet - the kit saved as a PDF, Escape, the four groups - in
    step 5, its code `code`): 13b a drill from Options > Drives; 13c
    three trusted contacts, all printed, two shares handed over (Recovery health green); 13d a
    second AzDrive that never had the drive recovers it with two of the printed shares - the
    token server holds the lockdown 48 hours (no credentials meanwhile), the owner's devices are
    told, then the drive is handed over and the code the shares gave back unlocks it."""
    app.after("the drive", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(drive_id),
              lambda: app.click(selector=side_drive(drive_id)))

    log("13a. (step 5) the recovery sheet came with the drive's making: the kit saved as a PDF, "
        "Escape kept the sheet, the groups typed back finished it (the recovery key at the token "
        "server, the check - never the code - in the settings file)")

    # 13b. A drill: Options > Drives > Recovery > the code's Test.
    app.tab("View")
    app.ribbon("Options")
    app.click(text="Drives")
    app.until("the recovery methods", lambda: app.has(method_button(drive_id, "code", "test")))
    if not app.has("#__azdrive_method_warning_" + re.sub(r"[^A-Za-z0-9_-]", "_",
                                                          drive_id).lower()):
        raise Failure("one method and no warning")
    app.click(selector=method_button(drive_id, "code", "test"))
    popup = e2e.modal_window(app)
    popup.until("the drill", lambda: popup.has("#__azdrive_drill_code"))
    popup.text_input("#__azdrive_drill_code", "00000-00000-00000-00000-000000")
    app.after("a wrong code", "AZDRIVE_DRILL_FAILED", re.escape(drive_id),
              lambda: popup.click(selector="#__azdrive_drill_check"))
    retype(popup, "#__azdrive_drill_code", "00000-00000-00000-00000-000000",
           code.replace("-", " ").lower())
    app.after("the drill passed", "AZDRIVE_DRILL_PASSED", re.escape(drive_id),
              lambda: popup.click(selector="#__azdrive_drill_check"))
    popup = e2e.modal_window(app)
    popup.click_exact("Close", frames=3)
    log("13b. A drill from Options > Drives: a wrong code failed, the kit's code (typed with "
        "spaces, lower case) passed")

    # 13c. Three trusted contacts without AzDrive: their shares printed; two handed over.
    app.click(selector=method_button(drive_id, "contacts", "add"))
    popup = e2e.modal_window(app)
    popup.until("the contacts' page", lambda: popup.has("#__azdrive_contacts_code"))
    popup.text_input("#__azdrive_contacts_code", code)
    for slot, person in enumerate(("Ada", "Grace", "Linus")):
        popup.text_input("#__azdrive_contacts_name_%d" % slot, person)
    app.after("the shares made", "AZDRIVE_CONTACTS_SHARED", r"%s 3" % re.escape(drive_id),
              lambda: popup.click(selector="#__azdrive_contacts_make"))
    popup.until("the shares", lambda: popup.has("#__azdrive_contacts_share_2"))
    shares = [m.group(0) for m in (SHARE_RE.search(t or "") for t in popup.texts()) if m]
    if len(shares) != 3 or sorted(s[:2] for s in shares) != ["S1", "S2", "S3"]:
        raise Failure("the printed shares are %r" % shares)
    for row in (0, 2):
        app.after("share %d saved" % (row + 1), "AZDRIVE_KIT_SAVED", r"\d+",
                  lambda: popup.click(selector="#__azdrive_share_save_%d" % row))
        saved_pdf(app, "share %d" % (row + 1))
    popup.screenshot(os.path.join(out, "13c-shares.png"))
    popup.click(selector="#__azdrive_contacts_done", frames=3)
    wait_closed(app)
    app.until("two methods", lambda: not app.has(
        "#__azdrive_method_warning_" + re.sub(r"[^A-Za-z0-9_-]", "_", drive_id).lower()))
    if not app.shows("Green: 2 methods"):
        raise Failure("the Recovery health is not green with the code and two shares out")
    app.key("escape")
    log("13c. Trusted contacts: three printed shares %s, two saved as PDFs (handed over); the "
        "warning gone, Recovery health green" % ", ".join(s[:2] for s in shares))

    # 13d. Another computer recovers the drive with shares 1 and 3.
    other = os.path.join(logs, "other")
    os.makedirs(os.path.join(other, "config"), exist_ok=True)
    other_drives = os.path.join(other, "config", "drives.json")
    text, entries = drives_file_entries(drives_file)
    kept = json.loads(text)
    kept["drives"] = [e for e in entries if e["id"] == drive_id]
    with open(other_drives, "w", encoding="utf-8") as f:
        json.dump(kept, f)
    other_keyring = os.path.join(other, "keyring.json")
    other_switches = [s for s in switches]
    for flag, value in (("--drives", other_drives), ("--data-dir", os.path.join(other, "data")),
                        ("--cache-dir", os.path.join(other, "cache")),
                        ("--home", os.path.join(other, "home"))):
        if flag in other_switches:
            other_switches[other_switches.index(flag) + 1] = value
        else:
            other_switches += [flag, value]
    os.makedirs(os.path.join(other, "home"), exist_ok=True)
    other_env = dict(env, AZ_KEYRING_FILE=other_keyring)

    def start_other(tag):
        drive = Drive(tag, binary, other_switches, args.debug_port + 1, logs, args.timeout,
                      extra_env=other_env)
        drive.until("the This PC view", lambda: drive.printed("AZDRIVE_PLACE", r"this-pc"))
        drive.until("the debug server", lambda: drive.op("get_dom_tree"))
        drive.until("the drive's row", lambda: drive.has(side_drive(drive_id)))
        return drive

    second = start_other("azdrive-other")
    try:
        drive_menu(second, drive_id, "Recover with trusted contacts…")
        second.until("the request", lambda: second.printed(
            "AZDRIVE_CONTACTS_REQUEST", re.escape(drive_id)))
        popup = e2e.modal_window(second)
        popup.until("the answers' boxes", lambda: popup.has("#__azdrive_contacts_answer_0"))
        popup.text_input("#__azdrive_contacts_answer_0", shares[0].lower().replace("-", " "))
        popup.text_input("#__azdrive_contacts_answer_1", shares[2])
        second.after("the code back and the lockdown", "AZDRIVE_CONTACTS_RECOVERED",
                     re.escape(drive_id),
                     lambda: popup.click(selector="#__azdrive_contacts_recover"))
        popup = e2e.modal_window(second)
        popup.until("the code given back", lambda: popup.has("#__azdrive_rebuilt_code"))
        if found(popup.texts(), CODE_RE).group(0) != code:
            raise Failure("the shares gave back another code")
        popup.screenshot(os.path.join(out, "13d-rebuilt.png"))
        pending = stack.token.state.drives[drive_id].get("lockdown_pending_until")
        if not pending or pending < stack.token.state.now() + 47 * 3600:
            raise Failure("the token server holds no 48 h lockdown: %r" % pending)
        session = json.loads(keyring_entries(other_keyring).get("azul-storage/s3/" + drive_id)
                             or "{}")
        token = session.get("drive_token") or ""
        client = azlin_client.TokenClient(stack.token_url)
        status, value, _ = client.call("POST", "/v1/drives/%s/credentials" % drive_id, {},
                                       bearer=token)
        if status != 403 or (value or {}).get("error") != "lockdown_pending":
            raise Failure("the recovering computer got credentials during the notice: HTTP %d %r"
                          % (status, value))
        app.until("the owner's device told", lambda: app.printed(
            "AZDRIVE_LOCKDOWN_PENDING", r"%s \S+" % re.escape(drive_id)))
        log("13d. Another computer: two printed shares (one typed in lower case with spaces) gave "
            "back the code, which signed the recovery-key lockdown - pending 48 h at the token "
            "server, no credentials meanwhile, the owner's AzDrive shows it")

        # The 48 hours pass at the token server: the drive is handed over.
        second.stop()
        stack.token.state.advance(azlin_mock_stack.LOCKDOWN_PENDING_SECS + 60)
        owner = json.loads(keyring_entries(keyring_file).get("azul-storage/s3/" + drive_id)
                           or "{}").get("drive_token") or ""
        status, value, _ = client.call("POST", "/v1/drives/%s/credentials" % drive_id, {},
                                       bearer=owner)
        if status != 401:
            raise Failure("the old devices keep the drive after the hand-over: HTTP %d" % status)
        second = start_other("azdrive-other-after")
        drive_menu(second, drive_id, "Unlock with the recovery code…")
        popup = e2e.modal_window(second)
        popup.until("the code's box", lambda: popup.has("#__azdrive_unlock_code"))
        popup.text_input("#__azdrive_unlock_code", code)
        second.after("the drive unlocked and listed", "AZDRIVE_LISTED",
                     r"%s / [1-9]\d*" % re.escape(drive_id),
                     lambda: popup.click_exact("Unlock"))
        second.until("its file", lambda: "one.txt" in item_names(second))
        second.screenshot(os.path.join(out, "13d-recovered.png"))
        log("13d. After the 48 h the token server handed the drive over (the owner's old devices "
            "refused), and the code the shares gave back unlocked it: one.txt in its listing")
    finally:
        second.stop()


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
    if args.recovery:
        # Step 13: the encrypted drive's index copies in this run's folder.
        switches += ["--cache-dir", os.path.join(logs, "cache")]
    # The headless keyring in a file of this run: it outlives AzDrive's restart (step 6). The
    # payer pays from Germany whatever this machine's locale is (the pills of steps 7 - 11).
    keyring_file = os.path.join(logs, "keyring.json")
    # The daily look at the drives' periods every 3 s (step 6c); English texts whatever the
    # computer's locale (step 6d reads them).
    google_token_url = stack.token_url + "/oauth/google/token"
    env = {"AZ_KEYRING_FILE": keyring_file, "AZLIN_COUNTRY": "DE",
           "AZDRIVE_PERIOD_CHECK_SECS": "3", "LC_ALL": "en_US.UTF-8",
           # Step 12: Google Drive's OAuth client and token endpoint (the mock's), the headless
           # sign-in fake's redirect, and the local stack as the HTTP a run may still send once
           # step 12's `mock` op armed azul's request mock (googleapis.com).
           "AZDRIVE_GOOGLE_CLIENT_ID": azlin_mock_stack.OAUTH_CLIENT_ID,
           "AZDRIVE_GOOGLE_TOKEN_URL": google_token_url,
           "AZ_AUTH_SESSION_REDIRECT": "{redirect_uri}?code=e2e-{code_challenge}&state={state}",
           "AZ_E2E_ALLOW_HTTP": "http://127.0.0.1:*,http://localhost:*"}
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
        # "We always encrypt": the drive's keys and its recovery sheet are part of its making.
        app.until("the new drive's encryption", lambda: app.printed(
            "AZDRIVE_ENCRYPTING_NEW_DRIVE", re.escape(bought)))
        code = finish_new_drive_sheet(app, bought, out=out, thorough=True, stack=stack, logs=logs)
        wait_closed(app)
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
        # A file into the encrypted drive through AzDrive: step 3's one.txt, copied and pasted.
        app.after("the folder of step 3", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(local_id),
                  lambda: app.click(selector=side_drive(local_id)))
        app.after("one.txt selected", "AZDRIVE_SELECTED", r"1 one\.txt",
                  lambda: select_item(app, "one.txt"))
        app.tab("Home")
        app.after("Home > Copy", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.ribbon("Copy"))
        app.after("the encrypted drive", "AZDRIVE_LISTED", r"%s / 0" % re.escape(bought),
                  lambda: app.click(selector=side_drive(bought)))
        app.after("Home > Paste", "AZDRIVE_TRANSFER", r"\d+ done 1",
                  lambda: app.ribbon("Paste"))
        app.after("F5", "AZDRIVE_LISTED", r"%s / 1" % re.escape(bought), lambda: app.key("f5"))
        app.until("the pasted file", lambda: "one.txt" in item_names(app))
        keys = bucket_holds_ciphertext_only(stack, drive["bucket"], ["one.txt"])
        app.screenshot(os.path.join(out, "5-azlin-drive.png"))
        log("5. Buy storage: six tiers with their prices, yearly ones after Pay yearly, Create "
            "test drive made %s (500 GB) ENCRYPTED as it was made - its recovery sheet (the kit "
            "saved as a PDF, Escape kept it, the four groups typed back; the recovery key at the "
            "token server) - in the drives file with its azlin auth and no secret; one.txt "
            "pasted into it lists, the bucket holds %d keys of the encryption only" % (bought, keys))

        # 6a. Paid after "Stop waiting": the drive joins the list in the background.
        place_lines = app.count("AZDRIVE_PLACE")
        first, dialog = buy_and_stop_waiting(app, stack, keyring_file, "Paid later")
        drives_before = set(stack.token.state.drives)
        stack.token.state.pay(first, {"card_number": azlin_mock_stack.APPROVING_CARD})
        # Its period ends in two days (a free month nearly gone): the first token is due.
        for new in set(stack.token.state.drives) - drives_before:
            stack.token.state.drives[new]["period_until"] = int(time.time()) + 2 * 86400
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
        # Due: one token redeemed at once, under the drive's lock, with its newest token.
        app.until("the period token redeemed", lambda: app.printed(
            "AZDRIVE_PERIOD_REDEEMED", r"%s 1 \S+" % re.escape(late)))
        months = check_period_tokens(stack, drives_file, first, late, redeemed=1)
        until = stack.token.state.drives[late]["period_until"]
        if until < int(time.time()) + 29 * 86400 or len(stack.token.state.redeemed) != 1:
            raise Failure("the redemption did not reach the mock: period until %s, %d redeemed"
                          % (until, len(stack.token.state.redeemed)))
        dialog.click("cancel")
        # Its recovery sheet waited for the dialog: it shows now.
        new_drive_encrypted(app, late)
        wait_closed(app)
        log("6a. Buy -> Stop waiting -> paid at the token server: %s joined the source list in "
            "the background (the window stayed where it was), its %d period token(s) issued "
            "against the sealed issue key and kept, then its checkout left the keyring's list; "
            "its period due in two days, one token bought it a month" % (late, months))

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
        new_drive_encrypted(app, paid)
        wait_closed(app)
        app.until("the checkout off the keyring's list",
                  lambda: second not in pending_checkouts(keyring_file))
        check_period_tokens(stack, drives_file, second, paid)
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
            "the start under the name typed, its session in the keyring, its period tokens "
            "kept, its checkout off the keyring's list, its bucket listed" % paid)

        # 6c. While AzDrive runs, its daily look at the periods (every few seconds in this run:
        # AZDRIVE_PERIOD_CHECK_SECS) finds the drive's period nearly over and buys a month.
        stack.token.state.drives[paid]["period_until"] = int(time.time()) + 2 * 86400
        app.until("the period token redeemed by the daily look", lambda: app.printed(
            "AZDRIVE_PERIOD_REDEEMED", r"%s 1 \S+" % re.escape(paid)))
        check_period_tokens(stack, drives_file, second, paid, redeemed=1)
        if stack.token.state.drives[paid]["period_until"] < int(time.time()) + 29 * 86400:
            raise Failure("the daily look's redemption did not reach the mock")
        log("6c. AzDrive running, the period of %s nearly over at the token server: the next "
            "look bought it a month with its kept token" % paid)

        # 6d. The drive's node answers "read-only, unpaid" (x-azlin-error): AzDrive says it in
        # the table's words with the request ID as the error ID, and notifies once.
        bucket = stack.token.state.drives[paid]["bucket"]
        stack.s3.fail_bucket(bucket, 403, "AccessDenied", "the drive takes no writes",
                             {"x-azlin-error": "read_only_unpaid"})
        problem = app.after("the refused listing", "AZDRIVE_PROBLEM",
                            r"%s read_only_unpaid \S+" % re.escape(paid), lambda: app.key("f5"))
        refused = [r for r in stack.s3.requests() if r.get("bucket") == bucket][-1]
        if problem.split()[-1] != refused.get("request_id"):
            raise Failure("the error ID %r is not the node's request ID %r"
                          % (problem.split()[-1], refused.get("request_id")))
        app.until("the table's text with the error ID", lambda: app.shows(
            "Your last payment didn't go through") and app.shows(
            "Error ID: %s" % refused.get("request_id")))
        # The notification: AzDrive says on stdout that it posted it (once).
        app.until("the notification", lambda: app.printed(
            "AZDRIVE_PROBLEM_NOTIFIED", re.escape(paid)))
        notified = app.count("AZDRIVE_PROBLEM_NOTIFIED", re.escape(paid))
        if notified != 1:
            raise Failure("the unpaid drive was notified %d times, not once" % notified)
        stack.s3.clear_faults()
        app.after("the drive answering again", "AZDRIVE_PROBLEM_GONE", re.escape(paid),
                  lambda: app.key("f5"))
        log("6d. The node refused %s as unpaid (x-azlin-error read_only_unpaid): the table's "
            "text with error ID %s; after it answered again the problem left the status line"
            % (paid, refused.get("request_id")))

        # 6e. A pending recovery-key lockdown (made elsewhere, with the recovery code): this
        # device of the owner shows it with Cancel at its next look, and cancels it.
        stack.token.state.drives[paid]["lockdown_pending_until"] = int(time.time()) + 2 * 86400
        app.until("the pending lockdown seen", lambda: app.printed(
            "AZDRIVE_LOCKDOWN_PENDING", r"%s \S+" % re.escape(paid)))
        app.until("its bar", lambda: app.has("#__azdrive_lockdown_bar"))
        if not app.shows("lockdown with the recovery code is pending"):
            raise Failure("the pending lockdown's bar does not say what it is")
        app.after("the lockdown cancelled", "AZDRIVE_LOCKDOWN_CANCELLED", re.escape(paid),
                  lambda: app.click(selector="#__azdrive_lockdown_cancel"))
        if stack.token.state.drives[paid].get("lockdown_pending_until"):
            raise Failure("the mock still has the lockdown pending")
        app.until("the bar gone", lambda: not app.has("#__azdrive_lockdown_bar"))
        log("6e. A recovery-key lockdown of %s pending at the token server: AzDrive's next look "
            "showed it with Cancel, and Cancel called it off" % paid)

        # 6f. Vouchers: one buys a new drive in Add drive > Buy storage ("I have a voucher"),
        # one adds days to a drive in Options > Drives.
        stack.token.state.add_voucher("AZ-E2E-NEW", months=1)
        dialog = open_dialog(app, "the source list",
                             lambda: app.click(selector="#" + I("side-add-drive")))
        app.after("the tiers", "AZDRIVE_TIERS", r"\d+",
                  lambda: dialog.page("buy", lambda: dialog.click("choice_buy")))
        dialog.page("voucher", lambda: dialog.click("voucher"))
        dialog.type_into("voucher_code", "AZ-E2E-NEW")
        gift = app.after("the voucher's drive", "AZDRIVE_VOUCHER", r"new d_\S+",
                         lambda: dialog.click("voucher_redeem")).split()[1]
        app.until("its row in CLOUD", lambda: app.has(side_drive(gift)))
        new_drive_encrypted(app, gift)
        wait_closed(app)
        if gift not in stack.token.state.drives or "AZ-E2E-NEW" in stack.token.state.vouchers:
            raise Failure("the voucher made no drive at the mock")
        stack.token.state.add_voucher("AZ-E2E-DAYS", months=2)
        before = stack.token.state.drives[paid]["period_until"]
        app.tab("View")
        app.ribbon("Options")
        app.click(text="Drives")
        app.click(selector="#__azdrive_voucher_" + re.sub(r"[^A-Za-z0-9_-]", "_", paid).lower())
        popup = e2e.modal_window(app)
        popup.until("the voucher field", lambda: popup.has("#__azdrive_voucher_code"))
        popup.text_input("#__azdrive_voucher_code", "AZ-E2E-DAYS")
        days = app.after("the voucher's days", "AZDRIVE_VOUCHER", r"%s \d+" % re.escape(paid),
                         lambda: popup.click(selector="#__azdrive_voucher_redeem")).split()[-1]
        if int(days) != 60 or stack.token.state.drives[paid]["period_until"] <= before:
            raise Failure("the voucher added %s days; the mock's period %s -> %s"
                          % (days, before, stack.token.state.drives[paid]["period_until"]))
        log("6f. Vouchers: AZ-E2E-NEW bought %s in Add drive, AZ-E2E-DAYS added %s days to %s "
            "in Options > Drives" % (gift, days, paid))

        # 6g. A new device of the paid drive (another token family at the token server, as a
        # join from another computer makes one): AzDrive's next look announces it (D42).
        with stack.token.state.lock:
            stack.token.state.new_family(paid, "m_e2e-laptop")
        app.until("the new device announced", lambda: app.printed(
            "AZDRIVE_NEW_DEVICE", r"%s m_e2e-laptop" % re.escape(paid)))
        log("6g. A device added to %s at the token server: AzDrive's next look announced it"
            % paid)

        # 6h. Restore as of (D42) of an encrypted drive: a file pasted into the paid drive after
        # a time; Options > Drives > "Restore as of..." with that time puts the drive back as its
        # drive index had it (one new commit of the encrypted metadata repository): the file goes.
        # The step closes what it opens and leaves the main window on its source list (step 7
        # starts there).
        if app.has("#" + I("settings")):
            app.key("escape")
            app.until("the Options closed", lambda: not app.has("#" + I("settings")))
        # Elsewhere first, so that every click below changes the place (and lists it).
        app.after("the folder of step 3", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(local_id),
                  lambda: app.click(selector=side_drive(local_id)))
        listed = app.after("the paid drive", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(paid),
                           lambda: app.click(selector=side_drive(paid)))
        before = int(listed.split()[-1])
        time.sleep(1.2)
        as_of = int(time.time())
        time.sleep(1.2)
        app.after("the folder of step 3", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(local_id),
                  lambda: app.click(selector=side_drive(local_id)))
        app.after("two.txt selected", "AZDRIVE_SELECTED", r"1 two\.txt",
                  lambda: select_item(app, "two.txt"))
        app.tab("Home")
        app.after("Home > Copy", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.ribbon("Copy"))
        app.after("the paid drive", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(paid),
                  lambda: app.click(selector=side_drive(paid)))
        app.after("Home > Paste", "AZDRIVE_TRANSFER", r"\d+ done 1",
                  lambda: app.ribbon("Paste"))
        app.after("F5", "AZDRIVE_LISTED", r"%s / %d" % (re.escape(paid), before + 1),
                  lambda: app.key("f5"))
        bucket_holds_ciphertext_only(stack, stack.token.state.drives[paid]["bucket"],
                                     ["two.txt"])
        app.tab("View")
        app.ribbon("Options")
        app.click(text="Drives")
        app.click(selector="#__azdrive_restore_" + re.sub(r"[^A-Za-z0-9_-]", "_", paid).lower())
        popup = e2e.modal_window(app)
        popup.until("the time field", lambda: popup.has("#__azdrive_restore_time"))
        popup.must("focus_node", selector="#__azdrive_restore_time")
        popup.frame(2)
        popup.key("end")
        for _ in range(len("1 hour ago")):
            popup.key("backspace", frames=1)
        popup.must("text_input", text=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(as_of)))
        popup.frame(2)
        restored = app.after("the drive restored", "AZDRIVE_RESTORED",
                             r"%s \S+ objects \d+" % re.escape(paid),
                             lambda: popup.click(selector="#__azdrive_restore_go")).split()
        if int(restored[-1]) < 1:
            raise Failure("the restore changed %s files" % restored[-1])
        wait_closed(app)
        app.key("escape")
        app.until("the Options closed", lambda: not app.has("#" + I("settings")))
        app.after("the restored drive", "AZDRIVE_LISTED", r"%s / %d" % (re.escape(paid), before),
                  lambda: app.key("f5"))
        app.until("two.txt gone", lambda: "two.txt" not in item_names(app))
        app.until("the source list's Add drive", lambda: app.has("#" + I("side-add-drive")))
        log("6h. Options > Drives > Restore as of %s of the encrypted %s: the file pasted after "
            "that time went (%s files of its index changed)" % (restored[1], paid, restored[-1]))

        # 7. A card payment in the popover: Fake Stripe's fields in the web view.
        stack.token.state.set_providers(list(azlin_mock_stack.DEFAULT_PROVIDERS))
        token = stack.token_url
        payment = open_payment(app, "card", "the card checkout")
        card, dialog = payment.checkout, payment.dialog
        if not re.fullmatch(r"fields 127\.0\.0\.1:\d+", payment.surface):
            raise Failure("the card checkout opened %r, not the fields in the popover"
                          % payment.surface)
        record = stack.token.state.checkouts.get(card) or {}
        if record.get("provider") != "fake-stripe" or record.get("method") != "card":
            raise Failure("the checkout went through %r" % {k: record.get(k) for k in
                                                            ("provider", "method", "surface")})
        if len(base64.b64decode(record.get("claim_key") or "")) != 32:
            raise Failure("the card checkout names no claim key")
        if card not in pending_checkouts(keyring_file):
            raise Failure("the card checkout is not on the keyring's list before it shows")
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
        hand_back(app, popover, view, token + "/_bridge/result?v=succeeded")
        by_card = new_drive(app, payment)
        new_drive_encrypted(app, by_card)
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
        payment = open_payment(app, "sepa_debit", "the direct debit checkout")
        debit, dialog = payment.checkout, payment.dialog
        if (stack.token.state.checkouts.get(debit) or {}).get("provider") != "fake-gocardless":
            raise Failure("the direct debit went through %s"
                          % (stack.token.state.checkouts.get(debit) or {}).get("provider"))
        if not re.fullmatch(r"page 127\.0\.0\.1:\d+", payment.surface):
            raise Failure("the direct debit opened %r, not the hosted page in the popover"
                          % payment.surface)
        popover = popover_of(app, dialog)
        view = the_view(popover)
        if "/fake-gocardless/flow/BRQpr_" not in view.get("src", ""):
            raise Failure("the web view shows %r" % view.get("src", "")[:80])
        popover.until("the chip", lambda: popover.shows("payment page of Fake GoCardless"))
        popover.screenshot(os.path.join(out, "8-debit-popover.png"))
        # The payer confirms the mandate on the page: the provider's webhook, then its redirect to
        # the return page - which the app takes as "go and ask" (azul-pay cancels a return page's
        # navigation and waits; machine.rs's tests pin the cancel): the dialog waits, the
        # popover closes.
        stack.token.state.provider_pays(debit, True)
        hand_back(app, popover, view, token + "/return/ok", redirect=True)
        by_debit = new_drive(app, payment)
        new_drive_encrypted(app, by_debit)
        wait_closed(app)
        log("8. Direct debit via Fake GoCardless: its hosted page in the popover; its return "
            "redirect made the dialog wait and closed the popover; the mandate's webhook "
            "approved: %s arrived" % by_debit)

        # 9. Off the provider's origins: blocked; a PayPal login: the system browser.
        payment = open_payment(app, "card", "the checkout that jumps to PayPal")
        jumped, dialog = payment.checkout, payment.dialog
        if not payment.surface.startswith("fields "):
            raise Failure("the card checkout opened %r" % payment.surface)
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
        # The login left for the browser: the dialog waits, the popover closes.
        app.until("the dialog waiting", lambda: app.last("AZDRIVE_PAY") == "waiting")
        app.until("the popover closed", lambda: popover.window_id not in app.window_ids())
        dialog.win.until("Open the page again", lambda: dialog.win.has(add_id("open_again")))
        stack.token.state.provider_pays(jumped, True)
        by_browser = new_drive(app, payment)
        new_drive_encrypted(app, by_browser)
        wait_closed(app)
        log("9. evil.example and its look-alikes were blocked with the chip unchanged; the PayPal "
            "login went to the system browser (%s) and the payment there brought %s"
            % (opened, by_browser))

        # 10. Load failures: the hosted page of the same checkout, then the system browser.
        payment = open_payment(app, "card", "the checkout whose page fails")
        failing, dialog = payment.checkout, payment.dialog
        if not payment.surface.startswith("fields "):
            raise Failure("the card checkout opened %r" % payment.surface)
        popover = popover_of(app, dialog)
        view = the_view(popover)
        # The same popover shows the next surface of the same checkout: its content changes, it
        # stays open (and is asked again for its window, which may be a new one).
        app.after("the hosted page", "AZDRIVE_PAY_SURFACE", r"page .+",
                  lambda: popover.must("simulate_webview_load_failed", webview=view["id"],
                                       reason="offline"))
        if app.last("AZDRIVE_PAY") != "presenting":
            raise Failure("the fallback left the checkout %s" % app.last("AZDRIVE_PAY"))
        popover = popover_of(app, dialog)
        popover.until("the hosted page in the web view", lambda: any(
            "/fake-stripe/c/pay/cs_test_" in v.get("src", "") for v in webviews(popover)))
        view = the_view(popover)
        opened = app.after("the system browser", "AZDRIVE_OPEN_BROWSER", r"\S+",
                           lambda: popover.must("simulate_webview_load_failed",
                                                webview=view["id"], reason="offline"))
        if not opened.startswith("127.0.0.1:"):
            raise Failure("the browser opened %r" % opened)
        # Off to the browser: the dialog waits, the popover closes.
        app.until("the popover closed", lambda: popover.window_id not in app.window_ids())
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
        payment = open_payment(app, "card", "the checkout that is closed")
        closed, dialog = payment.checkout, payment.dialog
        if not payment.surface.startswith("fields "):
            raise Failure("the card checkout opened %r" % payment.surface)
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

        # 12. A consumer cloud's sign-in. Dropbox has no client id in this run: the form says
        # which setting is missing.
        app.after("This PC", "AZDRIVE_PLACE", r"this-pc",
                  lambda: app.click(selector="#" + I("side-this-pc")))
        dialog = open_dialog(app, "the source list",
                             lambda: app.click(selector="#" + I("side-add-drive")))
        dialog.page("sources", lambda: dialog.click("choice_connect"))
        dialog.page("form dropbox", lambda: dialog.click("service_dropbox"))
        dialog.win.until("the sign-in's missing setting",
                         lambda: dialog.shows("AZDRIVE_DROPBOX_CLIENT_ID"))
        dialog.screenshot(os.path.join(out, "12-dropbox-no-client.png"))
        dialog.page("sources", lambda: dialog.click("back"))
        # Google Drive signs in: the fake redirect's code is the mock's for the PKCE challenge.
        dialog.page("form gdrive", lambda: dialog.click("service_gdrive"))
        dialog.type_into("name", "E2E Google", clear=len("Google Drive"))
        oauth = stack.token.state.oauth_requests
        before = len(oauth)
        signed = app.after("the sign-in", "AZDRIVE_SIGNED_IN", r"gdrive \S+",
                           lambda: dialog.click("sign_in"))
        if signed != "gdrive ok":
            raise Failure("the sign-in said %s: %s" % (signed, dialog.win.texts()))
        exchanges = [r for r in oauth[before:] if r["grant_type"] == "authorization_code"]
        if len(exchanges) != 1 or not exchanges[0]["ok"] \
                or exchanges[0]["client_id"] != azlin_mock_stack.OAUTH_CLIENT_ID \
                or exchanges[0]["redirect_uri"] != "http://127.0.0.1/" \
                or exchanges[0]["provider"] != "google":
            raise Failure("the token endpoint saw %r" % exchanges)
        refresh_token = exchanges[0]["refresh_token"]
        dialog.win.until('"Signed in to Google Drive"',
                         lambda: dialog.shows("Signed in to Google Drive"))
        dialog.screenshot(os.path.join(out, "12-gdrive-signed-in.png"))
        # The drive's API (googleapis.com) through azul's request mock: an empty My Drive. The
        # mock stack stays reachable (AZ_E2E_ALLOW_HTTP).
        app.must("mock", set={"http": {"https://www.googleapis.com/*": {
            "status": 200, "text": '{"files": []}', "content_type": "application/json"}}})
        gdrive_id = app.after("the Google Drive added", "AZDRIVE_ADDED", r"\S+",
                              lambda: dialog.click("save"))
        wait_closed(app)
        app.until("the Google Drive's (empty) root", lambda: app.printed(
            "AZDRIVE_LISTED", r"%s / 0" % re.escape(gdrive_id)))
        app.until("its row in CLOUD", lambda: app.has(side_drive(gdrive_id)))
        text, entries = drives_file_entries(drives_file)
        location = next(e for e in entries if e["id"] == gdrive_id)["location"]
        options = location.get("options") or {}
        if location.get("kind") != "opendal" or location.get("scheme") != "gdrive" \
                or not location.get("keyring") \
                or options.get("client_id") != azlin_mock_stack.OAUTH_CLIENT_ID \
                or options.get("token_url") != google_token_url:
            raise Failure("the Google Drive's entry is %s" % location)
        if "e2e-refresh" in text or "e2e-access" in text:
            raise Failure("the drives file holds a token: %s" % text)
        kept = keyring_entries(keyring_file).get("azul-storage/s3/" + gdrive_id)
        if not kept or json.loads(kept).get("refresh_token") != refresh_token:
            raise Failure("the keyring does not hold the drive's refresh token")
        refreshed = [r for r in oauth if r["grant_type"] == "refresh_token" and r["ok"]
                     and r.get("refresh_token") == refresh_token]
        if not refreshed:
            raise Failure("the drive did not refresh its access token at %s: %r"
                          % (google_token_url, oauth))
        app.screenshot(os.path.join(out, "12-gdrive-drive.png"))
        log("12. Dropbox without a client id named AZDRIVE_DROPBOX_CLIENT_ID; Google Drive signed "
            "in through the headless sign-in (PKCE checked at the mock's token endpoint), %s keeps "
            "its refresh token in the keyring only and refreshed its access token before its "
            "first listing" % gdrive_id)

        # 13. The recovery methods of an encrypted drive (C14): AzDrive with `encryption`.
        if args.recovery:
            recovery_steps(app, stack, args, logs, out, binary, switches, env, drives_file,
                           keyring_file, bought, code)
        else:
            log("13. skipped (run with --recovery): the drill, trusted contacts and a recovery by "
                "two shares")

        log("PASS: Add drive connected an S3 bucket, a folder and a SQLite database (tables as "
            "folders), bought a test drive, and claimed two paid drives - one in the background "
            "after Stop waiting, one at the start after AzDrive was closed - from the source list, "
            "This PC's ribbon and Home's ribbon, in the dialog's own window; and paid through "
            "azul-pay: a card in the popover's fields, a direct debit on a hosted page, PayPal in "
            "the system browser, with blocked navigations, the fallback chain and an abandon; "
            "and signed in to Google Drive through azul's sign-in session")
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
    parser.add_argument("--recovery", action="store_true",
                        help="step 13: the recovery methods (AzDrive built with encryption)")
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
