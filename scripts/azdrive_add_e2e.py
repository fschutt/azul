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
       node's error in the table's words, a pending recovery-key lockdown cancelled with the
       drive's recovery code (F12: another code refused), vouchers,
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
       With SRV17's keys (the mock mirrors them): the setup registers the code's findable key;
       a fresh profile recovers with two shares found by that key (then called off with the
       code), the owner adds a second kit and removes keys down to the last, which stays
       (last_recovery_key), and another fresh profile recovers by the kit's lookup alone.
   14. Cash by post (cash contract v1): with the mock's "cash" offer Buy storage shows the "Cash
       by post" pill; Buy makes a cash checkout (awaiting_cash at the mock, on the keyring's list
       with its slip and method "cash"), the dialog says "Waiting for your letter ..." and shows
       the AZK1 claim code (the checkout id and the kept claim secret, as scripts/azlin_claim.py
       writes it), both pages are saved as PDFs (FileDialog::save_bytes under the mock store), a
       look finds it waiting (AZDRIVE_CASH_WAITING) and the drive list shows the order; the
       operator activates it through the mock's switch and the next daily look
       (AZDRIVE_PERIOD_CHECK_SECS) brings the drive; on a second AzDrive profile (its own drives
       file and keyring) "Pick up a paid drive with a claim code" takes the code as typed (lower
       case, blanks) and the drive arrives there too - as a device of its own: it claimed a token
       family with the sealed sign-up's ticket (POST /v1/drives/<id>/claim), never the buyer's.
   15. A ban with a grace period (ban contract v1) on step 14's drive: the mock bans it for 48
       hours; the next look shows the banner with its hours, a paste into it and a new folder
       are refused with the reason (nothing reaches its bucket), "Copy everything to this
       computer" downloads its files into a folder picked (the mock store's file_open); the
       mock's clock past the end: the drive shows "This drive was closed on <date> because
       <reason>." and nothing else.

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
import hashlib
import hmac
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
import azlin_ed25519
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
    """The newest export recorded by FileDialog::save_bytes is a PDF of some size. A headless
    run's save goes to the armed mock store, not to a file, and `assert_saved_file` reads it
    there - a scenario step, so it runs through `run_e2e_tests`."""
    try:
        app.scenario_assert("assert_saved_file", name_ends_with=".pdf", mime="application/pdf",
                            min_len=2000, contains="%PDF")
    except Failure as e:
        raise Failure("%s was not saved as a PDF: %s" % (what, e))


def settings_text(data_dir):
    """AzDrive's settings file in the data folder (drive/view.json)."""
    for root, _dirs, files in os.walk(data_dir):
        if "view.json" in files and os.path.basename(root) == "drive":
            with open(os.path.join(root, "view.json"), "r", encoding="utf-8") as f:
                return f.read()
    return ""


CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"


def code_bytes(text):
    """A recovery code's 16 bytes from its text (RecoveryCode::parse: Crockford base32, O for 0,
    I and L for 1, dashes and spaces left out)."""
    value, bits, out = 0, 0, bytearray()
    for c in text.upper():
        if c in "- ":
            continue
        c = {"O": "0", "I": "1", "L": "1"}.get(c, c)
        value = (value << 5) | CROCKFORD.index(c)
        bits += 5
        if bits >= 8:
            bits -= 8
            out.append((value >> bits) & 0xFF)
    return bytes(out[:16])


def hkdf_sha256(salt, ikm, info, length=32):
    """RFC 5869 HKDF-SHA256 (azcloud-kit's recovery key derivation)."""
    prk = hmac.new(salt, ikm, hashlib.sha256).digest()
    out, block, counter = b"", b"", 1
    while len(out) < length:
        block = hmac.new(prk, block + info + bytes([counter]), hashlib.sha256).digest()
        out += block
        counter += 1
    return out[:length]


def signed_by_code(code, drive_id, what):
    """{"nonce", "signature"}: the code's drive key (azcloud-kit's RecoveryKey::derive -
    HKDF-SHA256, salt azlin-recovery-lockdown-v1, info the drive id - then Ed25519) signs
    `<what>:<nonce>`."""
    seed = hkdf_sha256(b"azlin-recovery-lockdown-v1", code_bytes(code), drive_id.encode())
    nonce = os.urandom(16).hex()
    message = ("%s:%s" % (what, nonce)).encode("utf-8")
    return {"nonce": nonce, "signature": azlin_ed25519.sign_b64(seed, message)}


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
        app.until("the findable key registered", lambda: app.printed(
            "AZDRIVE_RECOVERY_FINDABLE", re.escape(drive_id)))
        keys = stack.token.state.drives[drive_id].get("recovery_keys") or []
        if sorted(k.get("label") for k in keys) != ["recovery code",
                                                    "recovery code (finds the drive)"]:
            raise Failure("the token server has the recovery keys %r of %s"
                          % ([k.get("label") for k in keys], drive_id))
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


def bucket_of(stack, drive_id):
    """The bucket of the drive `drive_id` at the mock."""
    return stack.token.state.drives[drive_id]["bucket"]


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
    step 5 with the code's drive key and findable key, its code `code`): 13b a drill from
    Options > Drives; 13c three trusted contacts, all printed, two shares handed over (Recovery
    health green); 13d a fresh profile asks two contacts - their shares give the code back, its
    findable key names the drive, the lockdown is pending 48 h (no credentials meanwhile, the
    owner told), the owner's code calls it off; 13e the owner's keys listed and a second kit
    added; 13f keys removed with a code's signature, the last one kept; 13g a fresh profile
    recovers by the kit's lookup alone - 48 h, the hand-over, Finish, the code unlocks it."""
    app.after("the drive", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(drive_id),
              lambda: app.click(selector=side_drive(drive_id)))

    log("13a. (step 5) the recovery sheet came with the drive's making: the kit saved as a PDF, "
        "Escape kept the sheet, the groups typed back finished it (the code's drive key and "
        "findable key at the token server, the check - never the code - in the settings file)")

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

    # A fresh profile: its own drives file (empty), keyring, data, cache and home.
    def profile(name):
        root = os.path.join(logs, name)
        os.makedirs(os.path.join(root, "config"), exist_ok=True)
        os.makedirs(os.path.join(root, "home"), exist_ok=True)
        switched = list(switches)
        for flag, value in (("--drives", os.path.join(root, "config", "drives.json")),
                            ("--data-dir", os.path.join(root, "data")),
                            ("--cache-dir", os.path.join(root, "cache")),
                            ("--home", os.path.join(root, "home"))):
            if flag in switched:
                switched[switched.index(flag) + 1] = value
            else:
                switched += [flag, value]
        keyring = os.path.join(root, "keyring.json")
        return switched, dict(env, AZ_KEYRING_FILE=keyring), keyring

    def start(tag, switched, environment):
        drive = Drive(tag, binary, switched, args.debug_port + 1, logs, args.timeout,
                      extra_env=environment)
        drive.until("the This PC view", lambda: drive.printed("AZDRIVE_PLACE", r"this-pc"))
        drive.until("the debug server", lambda: drive.op("get_dom_tree"))
        return drive

    def options_drives(drive):
        drive.tab("View")
        drive.ribbon("Options")
        drive.click(text="Drives")

    client = azlin_client.TokenClient(stack.token_url)
    part = re.sub(r"[^A-Za-z0-9_-]", "_", drive_id).lower()

    def keys_at_mock():
        return list(stack.token.state.drives[drive_id].get("recovery_keys") or [])

    # 13d. A computer that never had the drive asks two trusted contacts: shares 1 and 3 give
    # the code back, its findable key names the drive, the lockdown is pending.
    contacts_switches, contacts_env, contacts_keyring = profile("contacts-profile")
    second = start("azdrive-contacts", contacts_switches, contacts_env)
    try:
        options_drives(second)
        second.after("the request", "AZDRIVE_CONTACTS_REQUEST", r".*",
                     lambda: second.click(selector="#__azdrive_contacts_recover_new"))
        popup = e2e.modal_window(second)
        popup.until("the answers' boxes", lambda: popup.has("#__azdrive_contacts_answer_0"))
        popup.text_input("#__azdrive_contacts_answer_0", shares[0].lower().replace("-", " "))
        popup.text_input("#__azdrive_contacts_answer_1", shares[2])
        second.after("the code back, the drive found, the lockdown", "AZDRIVE_CONTACTS_RECOVERED",
                     re.escape(drive_id),
                     lambda: popup.click(selector="#__azdrive_contacts_recover"))
        second.until("the recovery noted", lambda: second.printed(
            "AZDRIVE_RECOVERY_PENDING", re.escape(drive_id)))
        popup = e2e.modal_window(second)
        popup.until("the code given back", lambda: popup.has("#__azdrive_rebuilt_code"))
        if found(popup.texts(), CODE_RE).group(0) != code:
            raise Failure("the shares gave back another code")
        popup.screenshot(os.path.join(out, "13d-rebuilt.png"))
        pending = stack.token.state.drives[drive_id].get("lockdown_pending_until")
        if not pending or pending < stack.token.state.now() + 47 * 3600:
            raise Failure("the token server holds no 48 h lockdown: %r" % pending)
        session = json.loads(keyring_entries(contacts_keyring).get("azul-storage/s3/" + drive_id)
                             or "{}")
        status, value, _ = client.call("POST", "/v1/drives/%s/credentials" % drive_id, {},
                                       bearer=session.get("drive_token") or "")
        if status != 403 or (value or {}).get("error") != "lockdown_pending":
            raise Failure("the recovering computer got credentials during the notice: HTTP %d %r"
                          % (status, value))
        app.until("the owner's device told", lambda: app.printed(
            "AZDRIVE_LOCKDOWN_PENDING", r"%s \S+" % re.escape(drive_id)))
    finally:
        second.stop()
    # The owner calls it off with the recovery code (F12: a recovery key's signature, no
    # token) - here at the token server directly; AzDrive's lockdown bar signs it with the code.
    status, value, _ = client.call("POST", "/v1/drives/%s/lockdown/cancel" % drive_id,
                                   signed_by_code(code, drive_id,
                                                  "lockdown-cancel:%s" % drive_id))
    if status != 200 or stack.token.state.drives[drive_id].get("lockdown_pending_until"):
        raise Failure("the owner could not call the contacts' recovery off: HTTP %d %r"
                      % (status, value))
    log("13d. A computer that never had the drive: two printed shares (one in lower case with "
        "spaces) gave back the code, its findable key named %s at the token server, the code "
        "signed the lockdown - pending 48 h, no credentials meanwhile, the owner's AzDrive shows "
        "it; the owner's recovery code called it off" % drive_id)

    # 13e. The owner's recovery keys: listed, a second kit added.
    options_drives(app)
    app.after("the keys listed", "AZDRIVE_RECOVERY_KEYS", r"%s 2" % re.escape(drive_id),
              lambda: app.click(selector="#__azdrive_recovery_keys_check_" + part))
    app.click(selector="#__azdrive_recovery_keys_add_" + part)
    popup = e2e.modal_window(app)
    popup.until("the code's box", lambda: popup.has("#__azdrive_recovery_sign_code"))
    popup.text_input("#__azdrive_recovery_sign_code", code)
    app.after("the second kit", "AZDRIVE_RECOVERY_CODE_ADDED", re.escape(drive_id),
              lambda: popup.click(selector="#__azdrive_recovery_sign_ok"))
    popup = e2e.modal_window(app)
    popup.until("the second kit's sheet", lambda: popup.has("#__azdrive_sheet_group_0"))
    texts = popup.texts()
    second_code = found(texts, CODE_RE).group(0)
    if second_code == code:
        raise Failure("the second kit has the first code")
    second_groups = second_code.split("-")
    for slot, number in enumerate(int(n) for n in re.findall(r"\d+",
                                                              found(texts, ASKED_RE).group(1))):
        popup.text_input("#__azdrive_sheet_group_%d" % slot, second_groups[number - 1])
    app.after("its groups typed back", "AZDRIVE_RECOVERY_CODE_VERIFIED", re.escape(drive_id),
              lambda: popup.click(selector="#__azdrive_sheet_done"))
    app.until("three keys listed", lambda: app.printed(
        "AZDRIVE_RECOVERY_KEYS", r"%s 3" % re.escape(drive_id)))
    keys = keys_at_mock()
    labels = sorted(k.get("label") for k in keys)
    if len(keys) != 3 or "another recovery code" not in labels:
        raise Failure("the token server has the keys %r" % labels)
    extra = [k for k in stack.s3.store.keys(stack.token.state.drives[drive_id]["bucket"])
             if k.startswith(".azlin/keys/recovery-")]
    if len(extra) != 1:
        raise Failure("the bucket has %d further recovery wraps: %r" % (len(extra), extra))
    log("13e. The owner's recovery keys: the drive key and the findable key listed; \"Add another "
        "recovery code\" signed with the first code made a second kit (its own wrap, its sheet's "
        "groups typed back) and a third key at the token server")

    # 13f. Keys removed with a code's signature; the last one stays.
    def remove(key, typed):
        app.click(selector="#__azdrive_recovery_key_remove_%s_%s"
                  % (part, re.sub(r"[^A-Za-z0-9_-]", "_", key["key_id"]).lower()))
        win = e2e.modal_window(app)
        win.until("the code's box", lambda: win.has("#__azdrive_recovery_sign_code"))
        win.text_input("#__azdrive_recovery_sign_code", typed)
        win.click(selector="#__azdrive_recovery_sign_ok")
        return win

    by_label = {k["label"]: k for k in keys_at_mock()}
    remove(by_label["another recovery code"], code)
    app.until("the second kit's key removed", lambda: app.printed(
        "AZDRIVE_RECOVERY_KEY_REMOVED", r"%s %s" % (re.escape(drive_id),
                                                    re.escape(by_label["another recovery code"]
                                                              ["key_id"]))))
    app.until("two keys listed again", lambda: app.count(
        "AZDRIVE_RECOVERY_KEYS", r"%s 2" % re.escape(drive_id)) >= 2)
    remove(by_label["recovery code"], second_code.lower())
    app.until("the drive key refused as the code is gone", lambda: app.shows(
        "not a recovery code of this drive"))
    app.key("escape")
    remove(by_label["recovery code"], code)
    app.until("the drive key removed", lambda: app.printed(
        "AZDRIVE_RECOVERY_KEY_REMOVED", r"%s %s" % (re.escape(drive_id),
                                                    re.escape(by_label["recovery code"]
                                                              ["key_id"]))))
    app.until("one key listed", lambda: app.printed(
        "AZDRIVE_RECOVERY_KEYS", r"%s 1" % re.escape(drive_id)))
    last = keys_at_mock()
    if [k["label"] for k in last] != ["recovery code (finds the drive)"]:
        raise Failure("the token server kept %r" % [k["label"] for k in last])
    win = remove(last[0], code)
    app.until("the last key kept", lambda: app.printed(
        "AZDRIVE_RECOVERY_KEY_KEPT", r"%s %s" % (re.escape(drive_id),
                                                 re.escape(last[0]["key_id"]))))
    win.until("the reason", lambda: win.shows("last recovery key"))
    if len(keys_at_mock()) != 1:
        raise Failure("the last recovery key was removed")
    win.screenshot(os.path.join(out, "13f-last-key.png"))
    app.key("escape")
    app.key("escape")
    log("13f. The second kit's key and then the drive key removed with a code's signature (the "
        "second kit's code no longer counts), the last key - the findable one - kept: the token "
        "server's last_recovery_key, said in the dialog")

    # 13g. A fresh profile recovers by the kit's lookup: the findable key names the drive, the
    # code signs the lockdown (the drive key is gone: the findable key does), 48 h, Finish.
    kit_switches, kit_env, kit_keyring = profile("kit-profile")
    third = start("azdrive-kit", kit_switches, kit_env)
    try:
        options_drives(third)
        third.click(selector="#__azdrive_kit_recover")
        popup = e2e.modal_window(third)
        popup.until("the kit's code box", lambda: popup.has("#__azdrive_kit_recover_code"))
        popup.text_input("#__azdrive_kit_recover_code", code.lower())
        third.after("the drive found", "AZDRIVE_KIT_LOOKUP", r"1",
                    lambda: popup.click(selector="#__azdrive_kit_recover_find"))
        popup = e2e.modal_window(third)
        popup.until("the drive named", lambda: popup.shows(drive_id))
        third.after("the lockdown", "AZDRIVE_KIT_LOCKDOWN", re.escape(drive_id),
                    lambda: popup.click(selector="#__azdrive_kit_recover_lockdown_0"))
        third.until("the recovery noted", lambda: third.printed(
            "AZDRIVE_RECOVERY_PENDING", re.escape(drive_id)))
        pending = stack.token.state.drives[drive_id].get("lockdown_pending_until")
        if not pending or pending < stack.token.state.now() + 47 * 3600:
            raise Failure("the token server holds no 48 h lockdown: %r" % pending)
        session = json.loads(keyring_entries(kit_keyring).get("azul-storage/s3/" + drive_id)
                             or "{}")
        status, value, _ = client.call("POST", "/v1/drives/%s/credentials" % drive_id, {},
                                       bearer=session.get("drive_token") or "")
        if status != 403 or (value or {}).get("error") != "lockdown_pending":
            raise Failure("the kit's computer got credentials during the notice: HTTP %d %r"
                          % (status, value))
        third.key("escape")
        app.until("the owner's device told again", lambda: app.count(
            "AZDRIVE_LOCKDOWN_PENDING", r"%s \S+" % re.escape(drive_id)) >= 2)
        third.screenshot(os.path.join(out, "13g-pending.png"))
    finally:
        third.stop()
    stack.token.state.advance(azlin_mock_stack.LOCKDOWN_PENDING_SECS + 60)
    owner = json.loads(keyring_entries(keyring_file).get("azul-storage/s3/" + drive_id)
                       or "{}").get("drive_token") or ""
    status, value, _ = client.call("POST", "/v1/drives/%s/credentials" % drive_id, {},
                                   bearer=owner)
    if status != 401:
        raise Failure("the old devices keep the drive after the hand-over: HTTP %d" % status)
    third = start("azdrive-kit-after", kit_switches, kit_env)
    try:
        options_drives(third)
        third.after("the recovery finished", "AZDRIVE_RECOVERY_FINISHED", re.escape(drive_id),
                    lambda: third.click(selector="#__azdrive_recovery_finish_" + part))
        third.until("its row in CLOUD", lambda: third.has(side_drive(drive_id)))
        third.key("escape")
        drive_menu(third, drive_id, "Unlock with the recovery code…")
        popup = e2e.modal_window(third)
        popup.until("the code's box", lambda: popup.has("#__azdrive_unlock_code"))
        popup.text_input("#__azdrive_unlock_code", code)
        third.after("the drive unlocked and listed", "AZDRIVE_LISTED",
                    r"%s / [1-9]\d*" % re.escape(drive_id),
                    lambda: popup.click_exact("Unlock"))
        third.until("its file", lambda: "hello.txt" in item_names(third))
        third.screenshot(os.path.join(out, "13g-recovered.png"))
    finally:
        third.stop()
    log("13g. A fresh profile recovered %s by its kit alone: the findable key looked it up, the "
        "code signed the lockdown (pending 48 h, no credentials, the owner told), after the 48 h "
        "the owner's devices were refused and Finish added the drive; the code unlocked it: "
        "hello.txt in its listing" % drive_id)


# ==== 14. Cash by post, 15. a ban (cash and ban contracts v1) ====

# A claim code as the dialog shows it (AZK1, base32 in blocks of four).
CLAIM_CODE_RE = re.compile(r"\bAZK1(?:-[A-Z2-7]{1,4})+\b")
WAITING = "Waiting for your letter: postal cash takes a while, AzDrive checks once a day."


def other_profile(logs, name, switches, env):
    """A second AzDrive profile of this run: its own drives file, data, home and keyring file
    (another computer); the switches and the environment it starts with."""
    root = os.path.join(logs, name)
    os.makedirs(os.path.join(root, "config"), exist_ok=True)
    os.makedirs(os.path.join(root, "home"), exist_ok=True)
    other = list(switches)
    for flag, value in (("--drives", os.path.join(root, "config", "drives.json")),
                        ("--data-dir", os.path.join(root, "data")),
                        ("--downloads", os.path.join(root, "downloads")),
                        ("--home", os.path.join(root, "home"))):
        if flag in other:
            other[other.index(flag) + 1] = value
        else:
            other += [flag, value]
    if "--cache-dir" in other:
        other[other.index("--cache-dir") + 1] = os.path.join(root, "cache")
    return other, dict(env, AZ_KEYRING_FILE=os.path.join(root, "keyring.json"))


def cash_steps(app, stack, args, logs, out, binary, switches, env, keyring_file):
    """14. Cash by post: the pill, the checkout and its slip, both PDFs, the waiting order, the
    operator's activation and the daily look that brings the drive, the claim code picked up on
    a second profile. The drive's id."""
    stack.token.state.set_providers(list(azlin_mock_stack.DEFAULT_PROVIDERS) + ["cash"])
    added = set(app.printed("AZDRIVE_ADDED", r"d_\S+"))
    dialog = open_dialog(app, "the source list",
                         lambda: app.click(selector="#" + I("side-add-drive")))
    pills = app.after("the payment pills", "AZDRIVE_PILLS", r".+",
                      lambda: dialog.page("buy", lambda: dialog.click("choice_buy")))
    if "cash:cash" not in pills.split():
        raise Failure("Buy storage shows no cash pill: %r" % pills)
    dialog.click("tier_0")
    dialog.type_into("name", "Paid in cash", clear=len("Azlin Storage"))
    dialog.click("pill_cash")
    dialog.click("consent")
    checkout = app.after("the cash checkout", "AZDRIVE_CHECKOUT", r"ck_\S+",
                         lambda: dialog.click("buy_button"))
    app.until("the slip offered", lambda: app.printed("AZDRIVE_CASH_SLIP", re.escape(checkout)))
    record = stack.token.state.checkouts.get(checkout) or {}
    if record.get("method") != "cash" or record.get("status") != "awaiting_cash":
        raise Failure("the mock made %r" % {k: record.get(k) for k in ("method", "status")})
    kept = pending_checkouts(keyring_file).get(checkout) or {}
    if kept.get("method") != "cash" \
            or (kept.get("cash") or {}).get("activation_code") != record.get("activation_code"):
        raise Failure("the keyring's list keeps no slip of %s: %r"
                      % (checkout, {k: v for k, v in kept.items() if k != "claim_secret"}))
    dialog.win.until("the waiting line", lambda: dialog.shows(WAITING))
    code = dialog.win.until("the claim code", lambda: found(dialog.win.texts(), CLAIM_CODE_RE))
    code = code.group(0)
    if code != azlin_claim.claim_code(checkout, kept["claim_secret"]):
        raise Failure("the claim code shown is not the checkout id and its kept claim secret")
    app.op("mock", set={"save_bytes": {"accept": True}})
    app.after("the buyer's copy saved", "AZDRIVE_CASH_SAVED", r"copy \d+",
              lambda: dialog.click("#__azdrive_cash_copy_save"))
    saved_pdf(app, "the buyer's copy")
    app.after("the slip saved", "AZDRIVE_CASH_SAVED", r"slip \d+",
              lambda: dialog.click("#__azdrive_cash_slip_save"))
    saved_pdf(app, "the slip")
    app.until("a look found it waiting", lambda: app.printed(
        "AZDRIVE_CASH_WAITING", re.escape(checkout)))
    dialog.screenshot(os.path.join(out, "14-cash-posted.png"))
    dialog.click("cancel")
    wait_closed(app)
    app.until("the drive list's waiting order", lambda: app.has("#__azdrive_side_cash_0_line")
              and app.shows("Waiting for your letter"))
    if checkout not in pending_checkouts(keyring_file):
        raise Failure("closing the dialog took the cash checkout off the keyring's list")
    app.screenshot(os.path.join(out, "14-cash-waiting.png"))
    log("14. Cash by post: %s awaits its letter at the mock, on the keyring's list with its "
        "slip; the dialog showed the waiting line and the claim code, both pages were saved as "
        "PDFs, a look found it waiting and the drive list shows it" % checkout)

    # The operator activates it (the mock's AzCtl stand-in): the next daily look brings it.
    stack.token.state.activate_cash(checkout)
    drive_id = app.until("the drive of the cash order", lambda: [
        d for d in app.printed("AZDRIVE_ADDED", r"d_\S+") if d not in added])[0]
    app.until("its claim", lambda: app.printed(
        "AZDRIVE_CLAIMED", r"%s %s" % (re.escape(checkout), re.escape(drive_id))))
    app.until("its row in CLOUD", lambda: app.has(side_drive(drive_id)))
    app.until("the order off the drive list",
              lambda: not app.has("#__azdrive_side_cash_0_line"))
    app.until("the checkout off the keyring's list",
              lambda: checkout not in pending_checkouts(keyring_file))
    log("14. The operator activated %s at the mock; the next daily look claimed %s" %
        (checkout, drive_id))

    # Another computer picks the drive up with the claim code.
    other_switches, other_env = other_profile(logs, "cash-other", switches, env)
    second = Drive("azdrive-cash-other", binary, other_switches, args.debug_port + 1, logs,
                   args.timeout, extra_env=other_env)
    try:
        second.until("the This PC view", lambda: second.printed("AZDRIVE_PLACE", r"this-pc"))
        second.until("the debug server", lambda: second.op("get_dom_tree"))
        picker = open_dialog(second, "the source list",
                             lambda: second.click(selector="#" + I("side-add-drive")))
        picker.page("claim-code", lambda: picker.click("choice_claim"))
        picker.win.until("the page says each computer gets its own key",
                         lambda: picker.shows("gets a key of its own"))
        picker.type_into("claim_code", code.lower().replace("-", " "))
        picker.type_into("name", "Picked up", clear=len("Azlin Storage"))
        second.after("the code picked up", "AZDRIVE_PICKED_UP", re.escape(checkout),
                     lambda: picker.click("pick_up"))
        claimed = second.until("the drive picked up", lambda: second.printed(
            "AZDRIVE_CLAIMED", r"%s \S+" % re.escape(checkout)))[-1]
        if claimed.split()[-1] != drive_id:
            raise Failure("the claim code brought %s, not %s" % (claimed, drive_id))
        # Two devices, not one family: the second claimed its own with the drive's ticket.
        def family_of(path):
            text = keyring_entries(path).get("azul-storage/s3/" + drive_id) or "{}"
            token = json.loads(text).get("drive_token") or ""
            return token[3:].split(".")[0] if token.startswith("dt_") else ""
        first_family = family_of(keyring_file)
        second_family = second.until("the picked-up drive's session",
                                     lambda: family_of(other_env["AZ_KEYRING_FILE"]))
        if not first_family or first_family == second_family:
            raise Failure("the buyer's computer and the one that picked the drive up share the "
                          "token family %r" % first_family)
        claims = (stack.token.state.drives[drive_id].get("claim") or {}).get("claims")
        if claims != 1:
            raise Failure("the mock counted %r pick-ups of %s, not 1" % (claims, drive_id))
        picker.click("cancel")
        wait_closed(second)
        second.after("the picked-up drive's bucket", "AZDRIVE_LISTED",
                     r"%s / \d+" % re.escape(drive_id),
                     lambda: second.click(selector=side_drive(drive_id)))
        second.screenshot(os.path.join(out, "14-picked-up.png"))
    finally:
        second.stop()
    log("14. A second profile picked %s up with the claim code typed in lower case with "
        "blanks: AZDRIVE_CLAIMED there, its bucket listed" % drive_id)
    return drive_id


def ban_steps(app, stack, logs, out, drive_id):
    """15. A ban with a grace period on `drive_id`: the banner with its hours, writes refused,
    Copy everything to this computer, the closed drive past the end."""
    drive = stack.token.state.drives[drive_id]
    stack.s3.store.write(drive["bucket"], "notes/keep.txt", b"keep me\n")
    stack.s3.store.write(drive["bucket"], "photo.txt", b"a photo\n")
    reason = "spam distribution"
    until = stack.token.state.ban(drive_id, reason, 48 * 3600)
    hours = app.until("the ban seen by the daily look", lambda: app.printed(
        "AZDRIVE_BANNED", r"%s \d+" % re.escape(drive_id)))[-1].split()[-1]
    # The mock's clock may be ahead of this computer's (step 13 moved it): AzDrive counts by its
    # own.
    expected = max(1, -(-(until - int(time.time())) // 3600))
    if abs(int(hours) - expected) > 1:
        raise Failure("the banner counts %s hours, not about %d" % (hours, expected))
    app.after("the banned drive's listing", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(drive_id),
              lambda: app.click(selector=side_drive(drive_id)))
    banner = ("Due to %s, your account has been banned, but you have %s hours to migrate your "
              "files." % (reason, hours))
    app.until("the banner", lambda: app.has("#__azdrive_ban_bar") and app.shows(banner))
    app.screenshot(os.path.join(out, "15-banned.png"))

    # Writes are refused with the reason, before anything reaches the bucket.
    refused = "This drive is banned (%s)" % reason
    stack.s3.clear_log()
    app.key("n", primary=True, shift=True)
    app.until("the new folder refused", lambda: app.shows(refused))
    with open(os.path.join(logs, "home", "Documents", "upload-me.txt"), "wb") as f:
        f.write(b"an upload\n")
    app.after("Documents", "AZDRIVE_LISTED", r"home Documents/ \d+",
              lambda: app.click(selector="#" + I("side-fav-documents")))
    app.after("the file selected", "AZDRIVE_SELECTED", r"1 .*upload-me\.txt",
              lambda: select_item(app, "upload-me.txt"))
    app.after("Ctrl+C", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.key("c", primary=True))
    app.after("back to the banned drive", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(drive_id),
              lambda: app.click(selector=side_drive(drive_id)))
    app.key("v", primary=True)
    app.until("the paste refused", lambda: app.shows(refused))
    writes = [r for r in stack.s3.requests() if r.get("bucket") == drive["bucket"]
              and r.get("method") in ("PUT", "POST", "DELETE")]
    if writes:
        raise Failure("a refused write reached the bucket: %r" % writes[:3])
    log("15. Banned for %s: the banner says %s hours; a new folder and a paste were refused "
        "with the reason, nothing reached the bucket" % (reason, hours))

    # Copy everything to this computer: a folder picked, the whole drive downloaded into it.
    copy_to = os.path.join(logs, "ban-copy")
    os.makedirs(copy_to, exist_ok=True)
    app.op("mock", set={"file_open": {"path": copy_to}})
    app.after("Copy everything", "AZDRIVE_COPY_EVERYTHING", re.escape(drive_id),
              lambda: app.click(selector="#__azdrive_ban_copy"))
    copied = os.path.join(copy_to, "Paid in cash")

    def files_copied():
        try:
            with open(os.path.join(copied, "notes", "keep.txt"), "rb") as f1, \
                    open(os.path.join(copied, "photo.txt"), "rb") as f2:
                return f1.read() == b"keep me\n" and f2.read() == b"a photo\n"
        except OSError:
            return False
    app.until("the drive's files on this computer", files_copied)
    log("15. Copy everything to this computer downloaded the drive into %s" % copied)

    # Past the end: the drive is closed.
    stack.token.state.advance(48 * 3600 + 60)
    app.until("the drive closed", lambda: app.printed("AZDRIVE_CLOSED", re.escape(drive_id)))
    closed = "This drive was closed on %s because %s." % (
        time.strftime("%Y-%m-%d", time.gmtime(until)), reason)
    app.until("the closed drive's message", lambda: app.has("#__azdrive_ban_closed")
              and app.shows(closed))
    if app.has("#__azdrive_ban_bar"):
        raise Failure("the closed drive still shows the banner")
    app.screenshot(os.path.join(out, "15-closed.png"))
    log("15. Past the end (the mock's clock moved on): %s" % closed)


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
        "--language", "en",  # the clicks read English words (the system may be German)
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
        # (The Azlin drives' buckets may be asked for their space meanwhile: not this one.)
        calls = [(r["method"], r["op"], r["query"].get("max-keys")) for r in stack.s3.requests()
                 if r.get("bucket") == BUCKET]
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
        paid_code = new_drive_encrypted(app, paid)
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
        # Its space as its node counts it (SRV17: HeadBucket's x-azlin-used-bytes and
        # x-azlin-quota-bytes - the stored bytes, the tier's quota), no estimate. The drive's
        # first count came as it arrived, before its encryption wrote the keys (0 bytes); the
        # setup's writes ask the node again at once.
        counted = app.until("the node's count of %s after its encryption's writes" % paid,
                            lambda: app.printed("AZDRIVE_SPACE",
                                                r"%s [1-9]\d* \d+" % re.escape(paid)))[-1].split()
        quota = stack.token.state.drives[paid]["quota_bytes"]
        used = int(counted[1])
        if int(counted[2]) != quota or not 0 < used <= stack.s3.store.stored_bytes(bucket_of(
                stack, paid)) + (1 << 20):
            raise Failure("the node's count %r is not the bucket's (quota %d)" % (counted, quota))
        available = "%d GB available" % ((quota - used + 500_000_000) // 1_000_000_000)
        app.until("the status line's space, as the node counts it", lambda: app.shows(available))
        if app.shows("about " + available):
            raise Failure("the node's count is shown as an estimate")
        app.screenshot(os.path.join(out, "6-claimed.png"))
        log("6b. Buy -> Stop waiting -> AzDrive closed -> paid -> AzDrive started: %s arrived at "
            "the start under the name typed, its session in the keyring, its period tokens "
            "kept, its checkout off the keyring's list, its bucket listed, its space as its node "
            "counts it (HeadBucket: %d stored bytes of %d)" % (paid, used, quota))

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
        # the table's words with the request ID as the error ID, and notifies once. The drive is
        # encrypted: its names come from the drive index, and F5 asks the bucket again (the
        # index pulls at once, its poll time aside) - the node's refusal of that read is what
        # AzDrive says.
        bucket = stack.token.state.drives[paid]["bucket"]
        stack.s3.fail_bucket(bucket, 403, "AccessDenied", "the drive takes no writes",
                             {"x-azlin-error": "read_only_unpaid"})
        problem = app.after("the refused listing", "AZDRIVE_PROBLEM",
                            r"%s read_only_unpaid \S+" % re.escape(paid), lambda: app.key("f5"))
        # The node's refusals of the drive's reads - the index's pull starts with a HeadObject
        # of its manifest; a HeadBucket (the usage line's count, asked in the background, no
        # key) is none: the error ID is one of their request IDs.
        refusals = {r.get("request_id"): r for r in stack.s3.requests()
                    if r.get("bucket") == bucket and r.get("status") == 403
                    and (r.get("key") or r.get("method") != "HEAD")}
        refused = refusals.get(problem.split()[-1])
        if refused is None:
            raise Failure("the error ID %r is none of the node's request IDs %r"
                          % (problem.split()[-1], sorted(refusals)))
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
        # device of the owner shows it with Cancel at its next look. F12, "the recovery code
        # always wins": Cancel asks for the code - another one is refused before anything is
        # signed, the drive's own (from its sheet in 6b) signs the cancel, no drive token sent.
        stack.token.state.drives[paid]["lockdown_pending_until"] = int(time.time()) + 2 * 86400
        app.until("the pending lockdown seen", lambda: app.printed(
            "AZDRIVE_LOCKDOWN_PENDING", r"%s \S+" % re.escape(paid)))
        app.until("its bar", lambda: app.has("#__azdrive_lockdown_bar"))
        if not app.shows("lockdown with the recovery code is pending"):
            raise Failure("the pending lockdown's bar does not say what it is")
        app.until("the paid drive's recovery key at the token server (its sheet's)",
                  lambda: stack.token.state.drives[paid].get("recovery_keys"))
        signed_before = len(stack.token.state.drives[paid].get("recovery_nonces") or ())
        app.click(selector="#__azdrive_lockdown_cancel")
        popup = e2e.modal_window(app)
        popup.until("the cancel asks for the recovery code",
                    lambda: popup.has("#__azdrive_lockdown_cancel_code"))
        wrong = "00000-00000-00000-00000-000000"
        popup.text_input("#__azdrive_lockdown_cancel_code", wrong)
        popup.click(selector="#__azdrive_lockdown_cancel_confirm", frames=3)
        popup.until("another code refused", lambda: popup.shows(
            "not this drive's current recovery code"))
        if not stack.token.state.drives[paid].get("lockdown_pending_until"):
            raise Failure("another code cancelled the lockdown")
        retype(popup, "#__azdrive_lockdown_cancel_code", wrong,
               paid_code.replace("-", " ").lower())
        app.after("the lockdown cancelled", "AZDRIVE_LOCKDOWN_CANCELLED", re.escape(paid),
                  lambda: popup.click(selector="#__azdrive_lockdown_cancel_confirm"))
        if stack.token.state.drives[paid].get("lockdown_pending_until"):
            raise Failure("the mock still has the lockdown pending")
        if len(stack.token.state.drives[paid].get("recovery_nonces") or ()) <= signed_before:
            raise Failure("the cancel was not signed with the recovery code")
        app.until("the bar gone", lambda: not app.has("#__azdrive_lockdown_bar"))
        log("6e. A recovery-key lockdown of %s pending at the token server: AzDrive's next look "
            "showed it with Cancel; Cancel asked for the recovery code - another code was "
            "refused, the drive's own (typed with spaces, lower case) signed the cancel" % paid)

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
        if before == 0:
            # The drive's index (its encrypted metadata repository) is made with its first file,
            # and a restore goes back no further than it: one.txt first, so the time below is
            # one the index knows (before it, the drive did not exist yet for the restore).
            app.after("the folder of step 3", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(local_id),
                      lambda: app.click(selector=side_drive(local_id)))
            app.after("one.txt selected", "AZDRIVE_SELECTED", r"1 one\.txt",
                      lambda: select_item(app, "one.txt"))
            app.tab("Home")
            app.after("Home > Copy", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.ribbon("Copy"))
            app.after("the paid drive", "AZDRIVE_LISTED", r"%s / \d+" % re.escape(paid),
                      lambda: app.click(selector=side_drive(paid)))
            app.after("Home > Paste", "AZDRIVE_TRANSFER", r"\d+ done 1",
                      lambda: app.ribbon("Paste"))
            app.after("F5", "AZDRIVE_LISTED", r"%s / 1" % re.escape(paid), lambda: app.key("f5"))
            before = 1
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
                                     ["one.txt", "two.txt"])
        app.tab("View")
        app.ribbon("Options")
        app.click(text="Drives")
        app.click(selector="#__azdrive_restore_" + re.sub(r"[^A-Za-z0-9_-]", "_", paid).lower())
        popup = e2e.modal_window(app)
        popup.until("the time field", lambda: popup.has("#__azdrive_restore_time"))
        popup.must("focus_node", selector="#__azdrive_restore_time")
        popup.frame(2)
        # The time typed over what the field opens with, whatever its words: all of it selected,
        # then replaced.
        popup.key("a", primary=True)
        popup.key("backspace")
        popup.must("text_input", text=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(as_of)))
        popup.frame(2)
        # An encrypted drive is restored by its drive index (`files`), never by its bucket; a
        # restore the dialog refused or that failed says why on a line of its own.
        pattern = r"%s \S+ files \d+" % re.escape(paid)
        done = app.count("AZDRIVE_RESTORED", pattern)
        not_done = [app.count(key, re.escape(paid) + r" .*")
                    for key in ("AZDRIVE_RESTORE_REFUSED", "AZDRIVE_RESTORE_FAILED")]
        popup.click(selector="#__azdrive_restore_go")

        def restored_or_why():
            for key, seen in zip(("AZDRIVE_RESTORE_REFUSED", "AZDRIVE_RESTORE_FAILED"), not_done):
                lines = app.printed(key, re.escape(paid) + r" .*")
                if len(lines) > seen:
                    raise Failure("the restore of %s was not done: %s %s" % (paid, key, lines[-1]))
            return app.count("AZDRIVE_RESTORED", pattern) > done
        app.until("the drive restored", restored_or_why)
        restored = app.printed("AZDRIVE_RESTORED", pattern)[-1].split()
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

        # 14. Cash by post; 15. a ban with a grace period on its drive.
        cash_drive = cash_steps(app, stack, args, logs, out, binary, switches, env, keyring_file)
        ban_steps(app, stack, logs, out, cash_drive)

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
