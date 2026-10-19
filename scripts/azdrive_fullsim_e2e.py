#!/usr/bin/env python3
"""AzDrive against a whole local Azlin system: the real token server, the payment sandbox and the
storage servers of a local stack (not the Python mock of scripts/azlin_mock_stack.py). It buys a
drive the way a person does and moves files through it:

    1. Add drive -> Buy storage: the buyer declared no country, so the dialog asks "Where do
       you live?" with the guess from the locale (LC_ALL de_DE: Germany) picked; Continue
       declares it and the tiers show in the country's currency, the pills are the stack's;
    2. the smallest tier, the card pill (Fake Stripe), the consent, Buy: a checkout at the token
       server with a claim key, its fields page - the payment sandbox's - in the payment
       popover's web view;
    3. the page's part is played the way the sandbox's page plays it (a headless run loads
       nothing in a web view): the bridge says ready, visa, complete; the cardholder's name is
       typed in the popover and Pay tells the page to confirm (a fragment command); the page's
       own request goes to the sandbox (POST /stripe/confirm with the test card - with
       --card 4000002500003155 the 3-D Secure step is answered as the test bank's Complete
       button does); the sandbox's signed webhook approves the checkout at the token server;
       the page says succeeded through the bridge;
    4. the drive arrives (the sealed sign-up opened with the claim key, the period tokens
       kept), is encrypted as it arrives (its recovery sheet, the groups typed back);
    5. a file of a few megabytes and a folder of small files are copied into it from Documents;
    6. a second AzDrive profile (another computer) picks the drive up with the purchase's claim
       code (azlin_claim.claim_code of the checkout and the claim secret the buyer's keyring
       kept), unlocks it with the recovery code, copies the file and the folder into its own
       Documents: the bytes are compared with the originals.

The result (drive, bucket, checkout, the uploaded names and their sha256) goes to
<logs>/result.json for the stack's own checks (where the shards lie, the books).

Usage (AzDrive built with `--features fake-providers` against a libazul with the debug server;
the stack from azul-apps' local/fullsim.sh):

    python3 scripts/azdrive_fullsim_e2e.py --bin <AzDrive> [--token-url http://127.0.0.1:8081]
        [--sandbox-url http://127.0.0.1:8784] [--debug-port 8800] [--logs <dir>]
        [--card 4242424242424242] [--upload-mb 5] [--window]

`--window` runs the buyer's AzDrive in a real window (the second computer stays headless: its
unlock is in the drive's context menu, a native menu on macOS that no script may drive): the
popover's web view loads the sandbox's real page, which the sandbox serves with the test card typed in (`/_dev/fill`, nothing can type into a
real web view), and the page itself confirms on Pay; the screenshots are the OS's captures (the
page included). Each profile keeps its keyring in a file of the run (AZ_KEYRING_FILE, which a
windowed run honours too), never in the system keyring.
Every key_down has its key_up (the E2E key_up rule).
"""

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request

import azlin_claim
import azlin_client
import azlin_e2e as e2e
from azlin_e2e import Failure
from azdrive_e2e import Drive, I, open_item, select_item
from azdrive_add_e2e import (add_id, drive_menu, hand_back, keyring_entries, navigate,
                             navigation_to, new_drive_encrypted, open_dialog, other_profile,
                             pay_id, pending_checkouts, popover_of, side_drive, the_view,
                             wait_closed, webviews)

# The cardholder typed in the popover; the sandbox's test cards (its page lists them).
NAME = "Erika Example"
CARD_3DS = "4000002500003155"


def log(line):
    print("[azdrive-fullsim-e2e] %s" % line, flush=True)


def post_json(url, body, timeout=30):
    """POSTs `body` as JSON and returns (status, answer)."""
    request = urllib.request.Request(url, data=json.dumps(body).encode("utf-8"), method="POST",
                                     headers={"content-type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            return response.status, json.loads(response.read().decode("utf-8") or "{}")
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read().decode("utf-8") or "{}")


def sha256_of(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def tree_digests(root):
    """Every file under `root`: its path relative to `root` -> its sha256."""
    out = {}
    for base, _dirs, files in os.walk(root):
        for name in files:
            path = os.path.join(base, name)
            out[os.path.relpath(path, root)] = sha256_of(path)
    return out


def make_upload(documents, upload_mb, small_files):
    """Documents/<big file> of `upload_mb` MB of random bytes and Documents/<folder> of
    `small_files` small text files: what goes into the drive. Their names."""
    big = "fullsim-photo.bin"
    with open(os.path.join(documents, big), "wb") as f:
        f.write(os.urandom(upload_mb * 1_000_000))
    folder = "fullsim-notes"
    os.makedirs(os.path.join(documents, folder, "sub"))
    for i in range(small_files):
        sub = "sub" if i % 3 == 0 else ""
        with open(os.path.join(documents, folder, sub, "note-%02d.txt" % i), "wb") as f:
            f.write(("note %d of the full system run\n" % i).encode() * (i + 1))
    return big, folder


PACK = "fullsim-pack"


def make_pack(documents, count):
    """Documents/fullsim-pack/keep/ and drop/: `count` small text files in all, 20 bytes to 4 KB
    each - what an encrypted Azlin drive packs into pack objects instead of an object a file."""
    root = os.path.join(documents, PACK)
    for half in ("keep", "drop"):
        os.makedirs(os.path.join(root, half))
    for i in range(count):
        half = "keep" if i % 2 == 0 else "drop"
        size = 20 + (i * 7919) % 4000
        line = ("small file %d of the pack proof\n" % i).encode()
        with open(os.path.join(root, half, "n%05d.txt" % i), "wb") as f:
            f.write((line * (size // len(line) + 1))[:size])


def bucket_objects(drives_file, keyring_file, drive_id):
    """(key, size) of every object in the drive's bucket, listed with the session AzDrive keeps
    for it (its keyring file)."""
    with open(drives_file, "r", encoding="utf-8") as f:
        entry = next(e for e in json.load(f)["drives"] if e["id"] == drive_id)
    location = entry["location"]
    session = json.loads(keyring_entries(keyring_file)["azul-storage/s3/" + drive_id])
    bundle = {"drive": {"id": drive_id, "location": {"bucket": location["bucket"],
                                                     "endpoint": location["endpoint"],
                                                     "region": location.get("region")}},
              "credentials": {k: session[k] for k in
                              ("access_key_id", "secret_access_key", "session_token")}}
    return azlin_client.Bucket(bundle).list("")[0]


def copy_between(app, source_click, source_listed, names, target_click, target_listed,
                 during=None):
    """Copies the items `names` of one folder (opened by `source_click`, listed when the
    AZDRIVE_LISTED line matches `source_listed`) into another (`target_click`,
    `target_listed`), one item at a time, and waits for each paste to finish. `during` runs
    while the first paste is under way (after its "planned" line, before its "done" one)."""
    for name in names:
        app.after("the source folder", "AZDRIVE_LISTED", source_listed, source_click)
        app.after("%s selected" % name, "AZDRIVE_SELECTED", r"1 .*%s/?" % re.escape(name),
                  lambda: select_item(app, name))
        app.after("Ctrl+C", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.key("c", primary=True))
        app.after("the target folder", "AZDRIVE_LISTED", target_listed, target_click)
        planned = app.count("AZDRIVE_TRANSFER", r"\d+ planned \d+")
        done = app.count("AZDRIVE_TRANSFER", r"\d+ done \d+")
        app.key("v", primary=True)
        if during:
            app.until("the paste of %s under way" % name, lambda: app.count(
                "AZDRIVE_TRANSFER", r"\d+ planned \d+") > planned)
            during()
            if app.count("AZDRIVE_TRANSFER", r"\d+ done \d+") > done:
                raise Failure("the paste of %s was done before the step meant for its middle "
                              "ended: give it a bigger file (--upload-mb)" % name)
            during = None
        app.until("the paste of %s done" % name, lambda: app.count(
            "AZDRIVE_TRANSFER", r"\d+ done \d+") > done)


def run_hook(what, command, result):
    """Runs one of the stack's steps (a shell command: a disk failed, a node restarted, ...),
    keeps its output in the result, and fails the run when it fails."""
    log("%s: %s" % (what, command))
    started = time.time()
    done = subprocess.run(command, shell=True, capture_output=True, text=True, timeout=900)
    output = (done.stdout + done.stderr).strip()
    result.setdefault("hooks", []).append({"what": what, "command": command,
                                           "status": done.returncode,
                                           "seconds": round(time.time() - started, 1),
                                           "output": output[-4000:]})
    if done.returncode != 0:
        raise Failure("%s failed (%d): %s" % (what, done.returncode, output[-800:]))
    for line in output.splitlines()[-6:]:
        log("   %s" % line)


def start_app(tag, binary, switches, port, logs, timeout, env, window):
    if window:
        # A real window: the platform's own backend (azul's default, CPU-drawn), the debug server
        # on its port.
        env = dict(env, AZ_BACKEND="cpu")
    app = Drive(tag, binary, switches, port, logs, timeout, extra_env=env)
    app.until("the This PC view", lambda: app.printed("AZDRIVE_PLACE", r"this-pc"))
    app.until("the debug server", lambda: app.op("get_dom_tree"))
    app.must("resize", width=1280.0, height=820.0)
    app.frame(3)
    return app


def confirms(popover):
    """How many times Pay told the page to confirm (fragment commands `cmd=confirm`)."""
    return len([n for v in webviews(popover) for n in v.get("navigations") or []
                if "cmd=confirm" in n.get("url", "")])


def confirm_at_sandbox(app, popover, view, sandbox, secret, card):
    """Pay in the popover, then what the page does on its confirm command: its request to the
    sandbox with `card` (a 3-D Secure step answered with the test bank's Complete). The
    sandbox's last answer."""
    bridge = sandbox + "/_bridge/"
    before = confirms(popover)
    popover.click(selector=pay_id("confirm"))
    popover.until("the confirm command", lambda: confirms(popover) > before)
    confirm = navigation_to(popover, "cmd=confirm")
    if not confirm["allowed"] or "name=Erika+Example" not in confirm["url"]:
        raise Failure("Pay told the page %r" % confirm)
    status, answer = post_json(sandbox + "/stripe/confirm", {
        "client_secret": secret, "card_number": card, "exp": "1234", "cvc": "123",
        "iban": "", "country": "DE"})
    if answer.get("result") == "requires_action":
        # The page shows the test bank's box and grows; the payer presses Complete.
        navigate(popover, view, bridge + "height?v=560")
        navigate(popover, view, bridge + "result?v=requires_action")
        status, answer = post_json(sandbox + "/stripe/3ds/" + answer["pi"], {"action": "complete"})
        log("3-D Secure: the test bank's Complete answered %s" % answer.get("result"))
    return status, answer


def play_fields_page(app, popover, view, sandbox, card, decline_first=None, out=None):
    """The sandbox's fields page, played: the bridge messages its script sends while the card
    is typed, the confirm the app's Pay asks for, the page's own request to the sandbox with
    the card, a 3-D Secure step answered with the test bank's Complete, the outcome said
    through the bridge. `decline_first`: a card the sandbox declines is tried first - the page
    says the error and the failed result, the popover shows the provider's words, the payer
    types the good card and presses Pay again. Returns the last four digits charged."""
    src = view.get("src", "")
    if not src.startswith(sandbox + "/fields/fake-stripe/v1#pk="):
        raise Failure("the popover shows %r, not the sandbox's fields page" % src[:80])
    fragment = urllib.parse.parse_qs(src.split("#", 1)[1])
    secret = (fragment.get("cs") or [""])[0]
    if not secret:
        raise Failure("the fields page got no client secret in its fragment")
    bridge = sandbox + "/_bridge/"
    for message in ("ready", "brand?v=visa", "complete?v=1"):
        navigate(popover, view, bridge + message)
        heard = navigation_to(popover, "/_bridge/" + message)
        if not heard or heard["allowed"]:
            raise Failure("the bridge message %s was not cancelled: %s" % (message, heard))
    popover.until("VISA on the card", lambda: popover.shows("VISA"))
    popover.must("focus_node", selector=pay_id("name"))
    popover.frame(2)
    popover.must("text_input", text=NAME)
    popover.frame(3)
    popover.until("the name on the card", lambda: popover.shows(NAME.upper()))
    if decline_first:
        status, answer = confirm_at_sandbox(app, popover, view, sandbox, secret, decline_first)
        if answer.get("result") != "failed" or not answer.get("message"):
            raise Failure("the sandbox did not decline %s: %s %s" % (decline_first, status, answer))
        words = answer["message"]
        navigate(popover, view, bridge + "error?" + urllib.parse.urlencode(
            {"code": answer.get("code") or "failed", "message": words}))
        navigate(popover, view, bridge + "result?" + urllib.parse.urlencode(
            {"v": "failed", "code": answer.get("code") or "failed"}))
        popover.until("the decline's words in the popover", lambda: popover.shows(words))
        if out:
            popover.screenshot(os.path.join(out, "05a-declined.png"))
        log("the card %s was declined (%s): the popover says %r" % (
            decline_first[-4:], answer.get("code"), words))
        # The payer types another card: the page says incomplete, then complete again.
        for message in ("complete?v=0", "complete?v=1"):
            navigate(popover, view, bridge + message)
    status, answer = confirm_at_sandbox(app, popover, view, sandbox, secret, card)
    if status != 200 or answer.get("result") != "succeeded":
        raise Failure("the sandbox answered the page's confirm with %s %s" % (status, answer))
    last4 = answer.get("last4") or card[-4:]
    navigate(popover, view, bridge + "last4?v=" + last4)
    return last4


def shot(win, path, window):
    """A screenshot of `win`: azul's own render headless; in a real window the OS's capture of
    the window, which holds the web view's page too (azul's render leaves it out)."""
    if not window:
        win.screenshot(path)
        return
    win.settle()
    value = win.value("take_native_screenshot")
    data = value.get("data") if isinstance(value, dict) else value
    if not isinstance(data, str) or "base64," not in data:
        raise Failure("take_native_screenshot returned no PNG: %s" % json.dumps(value)[:200])
    with open(path, "wb") as f:
        f.write(base64.b64decode(data.split("base64,", 1)[1]))
    log("native screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


def real_page_pays(app, popover, sandbox, out):
    """The sandbox's fields page as it really runs in the popover's web view (a real window):
    it came with the test card typed in (`/_dev/fill` before Buy), says ready, the brand and
    complete through its own navigations, which the app cancels; Pay sends it the confirm
    command and the page confirms with the sandbox itself, then says the last four digits and
    succeeded. The last four digits."""
    def heard(message):
        n = navigation_to(popover, "/_bridge/" + message)
        return n if n and not n["allowed"] else None
    for message in ("ready", "brand?v=visa", "complete?v=1"):
        popover.until("the page's own %s" % message, lambda m=message: heard(m))
    popover.until("VISA on the card", lambda: popover.shows("VISA"))
    popover.must("focus_node", selector=pay_id("name"))
    popover.frame(2)
    popover.must("text_input", text=NAME)
    popover.frame(3)
    popover.until("the name on the card", lambda: popover.shows(NAME.upper()))
    shot(popover, os.path.join(out, "04b-filled.png"), True)
    before = confirms(popover)
    popover.click(selector=pay_id("confirm"))
    popover.until("the confirm command", lambda: confirms(popover) > before)
    last4 = popover.until("the page's last four digits", lambda: heard("last4?v=4242"))
    app.until("the dialog waiting for the drive", lambda: app.last("AZDRIVE_PAY") == "waiting")
    return last4["url"].rsplit("=", 1)[-1]


def pack_delete_half(app, drive_id, result):
    """The pack proof's delete: the drive's fullsim-pack/drop/ (half of the small files) goes
    for good, which leaves their packs sparse or empty."""
    drive = re.escape(drive_id)
    app.after("the drive", "AZDRIVE_LISTED", r"%s / \d+" % drive,
              lambda: app.click(selector=side_drive(drive_id)))
    app.after("the pack folder", "AZDRIVE_LISTED", r"%s %s/ 2" % (drive, PACK),
              lambda: open_item(app, PACK))
    app.after("drop selected", "AZDRIVE_SELECTED", r"1 .*drop/?", lambda: select_item(app, "drop"))
    windows = len(app.window_ids())
    app.key("delete")
    app.until("the delete's question", lambda: len(app.window_ids()) > windows)
    gone = app.after("the delete", "AZDRIVE_DELETED", r"\d+",
                     lambda: e2e.click_dialog_button(app, "Delete", closes=True))
    result["pack_deleted"] = int(gone)
    log("6p. drop/ deleted for good from %s (%s items)" % (drive_id, gone))


def pack_after_maintenance(binary, switches, env, home, args, logs, drive_id, uploaded, result):
    """The pack proof's end: the second computer starts again (its first idle minute runs a
    maintenance round of the drive's index: sparse packs repacked, emptied ones retired), then
    copies the drive's fullsim-pack/keep/ into Documents/after-delete/: every kept file reads
    back byte for byte."""
    keyring_file = env["AZ_KEYRING_FILE"]
    drives_file = switches[switches.index("--drives") + 1]
    before = {k for k, _ in bucket_objects(drives_file, keyring_file, drive_id)}
    target = os.path.join(home, "Documents", "after-delete")
    os.makedirs(target)
    app = start_app("azdrive-second-again", binary, switches, args.debug_port + 1, logs,
                    args.timeout, env, False)
    try:
        log("6q. the second computer started again: %d s for its maintenance round"
            % args.maintain_wait)
        time.sleep(args.maintain_wait)
        if app.printed("AZDRIVE_MAINTAIN_STOPPED", r".+"):
            raise Failure("the maintenance round stopped: %s"
                          % app.printed("AZDRIVE_MAINTAIN_STOPPED", r".+"))
        after = {k for k, _ in bucket_objects(drives_file, keyring_file, drive_id)}
        result["bucket_after_maintenance"] = {"objects": len(after), "new": len(after - before),
                                              "gone": len(before - after)}
        drive = re.escape(drive_id)
        app.after("the drive", "AZDRIVE_LISTED", r"%s / \d+" % drive,
                  lambda: app.click(selector=side_drive(drive_id)))
        app.after("the pack folder", "AZDRIVE_LISTED", r"%s %s/ 1" % (drive, PACK),
                  lambda: open_item(app, PACK))
        app.after("keep selected", "AZDRIVE_SELECTED", r"1 .*keep/?", lambda: select_item(app, "keep"))
        app.after("Ctrl+C", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.key("c", primary=True))
        app.after("Documents", "AZDRIVE_LISTED", r"home Documents/ \d+",
                  lambda: app.click(selector="#" + I("side-fav-documents")))
        app.after("after-delete", "AZDRIVE_LISTED", r"home Documents/after-delete/ 0",
                  lambda: open_item(app, "after-delete"))
        done = app.count("AZDRIVE_TRANSFER", r"\d+ done \d+")
        app.key("v", primary=True)
        app.until("the kept files copied", lambda: app.count(
            "AZDRIVE_TRANSFER", r"\d+ done \d+") > done)
    finally:
        app.stop()
    kept = {k: v for k, v in uploaded.items() if k.startswith(PACK + "/keep/")}
    back = {os.path.join(PACK, k): v for k, v in
            tree_digests(target).items()}
    if back != kept:
        raise Failure("after the delete and the maintenance round %d of %d kept files read back "
                      "the same" % (sum(1 for k in kept if back.get(k) == kept[k]), len(kept)))
    log("6r. after the delete and a maintenance round (%s) the %d kept files read back byte "
        "for byte" % (result["bucket_after_maintenance"], len(kept)))


def run(args, logs):
    binary = e2e.find_binary("AzDrive", args.bin, "AZDRIVE_BIN")
    out = args.out or os.path.join(logs, "shots")
    os.makedirs(out, exist_ok=True)
    home = os.path.join(logs, "home")
    documents = os.path.join(home, "Documents")
    os.makedirs(documents)
    big, folder = make_upload(documents, args.upload_mb, args.small_files)
    tops = [big, folder]
    if args.pack_files:
        make_pack(documents, args.pack_files)
        tops.append(PACK)
    uploaded = tree_digests(documents)
    drives_file = os.path.join(logs, "config", "drives.json")
    keyring_file = os.path.join(logs, "keyring.json")
    switches = [
        "--screen", "this-pc", "--theme", "flat", "--mode", "light", "--language", "en",
        "--home", home, "--downloads", os.path.join(logs, "downloads"),
        "--data-dir", os.path.join(logs, "data"), "--drives", drives_file,
        "--token-url", args.token_url,
    ]
    # No AZLIN_COUNTRY: the buyer declares the country in the dialog; the locale's region is
    # the guess. The fakes' pages are http on this computer.
    env = {"AZ_KEYRING_FILE": keyring_file, "LC_ALL": "%s.UTF-8" % args.locale,
           "AZ_E2E_ALLOW_HTTP": "http://127.0.0.1:*,http://localhost:*"}
    result = {"token_url": args.token_url, "sandbox_url": args.sandbox_url, "card": args.card,
              "uploaded": uploaded, "big": big, "folder": folder}
    app = start_app("azdrive-buyer", binary, switches, args.debug_port, logs, args.timeout, env,
                    args.window)
    try:
        # 1. The country first, guessed from the locale.
        dialog = open_dialog(app, "the source list",
                             lambda: app.click(selector="#" + I("side-add-drive")))
        dialog.page("country", lambda: dialog.click("choice_buy"))
        guess = add_id("country_option_%s" % args.country.lower())
        dialog.win.until("the guess %s in the list" % args.country, lambda: dialog.win.has(guess))
        shot(dialog.win, os.path.join(out, "01-country.png"), args.window)

        def declare():
            dialog.click("country_option_%s" % args.country.lower())
            dialog.click("country_continue")
        pills = app.after("the pills", "AZDRIVE_PILLS", r".+", lambda: app.after(
            "the country declared", "AZDRIVE_COUNTRY", re.escape(args.country), declare))
        tiers = app.until("the tiers", lambda: app.printed("AZDRIVE_TIERS", r"\d+"))[-1]
        if "card:fake-stripe" not in pills.split():
            raise Failure("the pills are %r: no card through Fake Stripe (is AzDrive built with "
                          "--features fake-providers, is the stack's sandbox on?)" % pills)
        shot(dialog.win, os.path.join(out, "02-tiers.png"), args.window)
        log("1. Buy storage asked for the country (the guess %s picked), %s tiers, pills %s"
            % (args.country, tiers, pills))

        # 2. The smallest tier, the card, Buy.
        dialog.click("tier_0")
        dialog.type_into("name", "Full system", clear=len("Azlin Storage"))
        dialog.click("pill_card")
        dialog.click("consent")
        shot(dialog.win, os.path.join(out, "03-buy.png"), args.window)
        added = set(app.printed("AZDRIVE_ADDED", r"d_\S+"))
        surfaces = app.count("AZDRIVE_PAY_SURFACE", r".+")
        if args.window:
            # Nothing can type into a real web view: the sandbox serves the next fields page
            # with the test card in its inputs, as if typed.
            status, _ = post_json(args.sandbox_url + "/_dev/fill", {
                "card_number": args.card, "exp": "1234", "cvc": "123"})
            if status != 200:
                raise Failure("the sandbox took no fill (%s): is it the build with /_dev/fill?"
                              % status)
        checkout = app.after("the checkout", "AZDRIVE_CHECKOUT", r"ck_\S+",
                             lambda: dialog.click("buy_button"))
        surface = app.until("the payment surface", lambda: app.count(
            "AZDRIVE_PAY_SURFACE", r".+") > surfaces and app.printed("AZDRIVE_PAY_SURFACE", r".+")[-1])
        if not surface.startswith("fields 127.0.0.1:"):
            raise Failure("the card checkout opened %r, not the fields in the popover" % surface)
        kept = pending_checkouts(keyring_file).get(checkout)
        if not kept:
            raise Failure("the checkout %s is not on the keyring's list (%s): a window run needs a "
                          "libazul whose keyring honours AZ_KEYRING_FILE" % (checkout, keyring_file))
        claim_code = azlin_claim.claim_code(checkout, base64.b64decode(kept["claim_secret"]))
        result.update(checkout=checkout, claim_code=claim_code)
        log("2. Buy: checkout %s, its fields page in the popover (%s)" % (checkout, surface))

        # 3. The page's part, the webhook, the drive.
        popover = popover_of(app, dialog)
        view = the_view(popover)
        for text in ("card fields by Fake Stripe", "CARDHOLDER"):
            popover.until("the popover's %r" % text, lambda: popover.shows(text))
        # The chip reads "<host> - card fields by ...": the host and its words keep a gap.
        host = popover.box("#__azdrive_pay_host")
        words = popover.text_rect("- card fields by Fake Stripe")
        if not words or words["x"] < host["x"] + host["width"] + 2:
            raise Failure("the chip runs its host into its words: the host ends at %.1f, the "
                          "words start at %s" % (host["x"] + host["width"],
                                                 words and round(words["x"], 1)))
        shot(popover, os.path.join(out, "04-popover.png"), args.window)
        if args.window:
            last4 = real_page_pays(app, popover, args.sandbox_url, out)
            app.until("the popover closed", lambda: popover.window_id not in app.window_ids())
        else:
            last4 = play_fields_page(app, popover, view, args.sandbox_url, args.card,
                                     decline_first=args.decline_first, out=out)
            popover.screenshot(os.path.join(out, "05-paid.png"))
            hand_back(app, popover, view, args.sandbox_url + "/_bridge/result?v=succeeded")
        drive_id = app.until("the drive of %s" % checkout, lambda: [
            d for d in app.printed("AZDRIVE_ADDED", r"d_\S+") if d not in added])[0]
        log("3. Paid with the card ending %s in the sandbox; the webhook approved %s; drive %s "
            "arrived" % (last4, checkout, drive_id))

        # 4. Encrypted as it arrived.
        recovery = new_drive_encrypted(app, drive_id)
        wait_closed(app)
        app.until("the checkout off the keyring's list",
                  lambda: checkout not in pending_checkouts(keyring_file))
        app.until("its row in CLOUD", lambda: app.has(side_drive(drive_id)))
        tokens = app.until("the period tokens kept", lambda: app.printed(
            "AZDRIVE_PERIOD_TOKENS", r"%s %s \d+" % (re.escape(checkout), re.escape(drive_id))))
        result.update(drive=drive_id, recovery_code=recovery, period_tokens=tokens)
        log("4. %s encrypted as it arrived (recovery sheet done); %s period tokens kept"
            % (drive_id, tokens[-1].split()[-1]))

        # 5. The file and the folder into the drive.
        during = None
        if args.during_upload:
            during = lambda: run_hook("during the upload", args.during_upload, result)
        copy_between(app, lambda: app.click(selector="#" + I("side-fav-documents")),
                     r"home Documents/ \d+", tops,
                     lambda: app.click(selector=side_drive(drive_id)),
                     r"%s / \d+" % re.escape(drive_id), during=during)
        app.after("F5", "AZDRIVE_LISTED", r"%s / %d" % (re.escape(drive_id), len(tops)),
                  lambda: app.key("f5"))
        shot(app, os.path.join(out, "06-uploaded.png"), args.window)
        log("5. %s (%d files in all) copied into %s" % (", ".join(tops), len(uploaded), drive_id))
        if args.pack_files:
            # An encrypted drive packs small files: far fewer objects than files.
            objects = bucket_objects(drives_file, keyring_file, drive_id)
            result["bucket_after_upload"] = {"objects": len(objects),
                                             "bytes": sum(n for _, n in objects)}
            if len(objects) * 10 > args.pack_files:
                raise Failure("%d small files became %d objects: they were not packed"
                              % (args.pack_files, len(objects)))
            log("5p. %d small files are %d objects in the bucket (packed)"
                % (args.pack_files, len(objects)))
        if args.after_upload:
            # The stack changes under the running AzDrive (a node restarted): the drive lists
            # again and takes one more file.
            run_hook("after the upload", args.after_upload, result)
            late = "after-the-step.txt"
            with open(os.path.join(documents, late), "wb") as f:
                f.write(b"written after the stack's step\n")
            uploaded[late] = sha256_of(os.path.join(documents, late))
            copy_between(app, lambda: app.click(selector="#" + I("side-fav-documents")),
                         r"home Documents/ \d+", [late],
                         lambda: app.click(selector=side_drive(drive_id)),
                         r"%s / \d+" % re.escape(drive_id))
            app.after("F5", "AZDRIVE_LISTED", r"%s / %d" % (re.escape(drive_id), len(tops) + 1),
                      lambda: app.key("f5"))
            log("5b. after the step AzDrive listed %s again and copied %s into it"
                % (drive_id, late))
    finally:
        app.stop()
    with open(os.path.join(logs, "result.json"), "w", encoding="utf-8") as f:
        json.dump(result, f, indent=1)

    # 6. Another computer: the claim code, the recovery code, the files back.
    other_switches, other_env = other_profile(logs, "second", switches, env)
    other_home = other_switches[other_switches.index("--home") + 1]
    os.makedirs(os.path.join(other_home, "Documents"))
    # The second computer runs headless in a window run too: its unlock is in the drive's
    # context menu, which macOS shows as a native menu that no script may drive.
    second = start_app("azdrive-second", binary, other_switches, args.debug_port + 1, logs,
                       args.timeout, other_env, False)
    try:
        picker = open_dialog(second, "the source list",
                             lambda: second.click(selector="#" + I("side-add-drive")))
        picker.page("claim-code", lambda: picker.click("choice_claim"))
        picker.type_into("claim_code", claim_code)
        picker.type_into("name", "Full system", clear=len("Azlin Storage"))
        second.after("the code picked up", "AZDRIVE_PICKED_UP", re.escape(checkout),
                     lambda: picker.click("pick_up"))
        claimed = second.until("the drive picked up", lambda: second.printed(
            "AZDRIVE_CLAIMED", r"%s \S+" % re.escape(checkout)))[-1]
        if claimed.split()[-1] != drive_id:
            raise Failure("the claim code brought %s, not %s" % (claimed, drive_id))
        second.until("the drive encrypted elsewhere", lambda: second.printed(
            "AZDRIVE_ENCRYPTED_ELSEWHERE", re.escape(drive_id)))
        picker.click("cancel")
        wait_closed(second)
        drive_menu(second, drive_id, "Unlock with the recovery code…")
        popup = e2e.modal_window(second)
        popup.until("the code's box", lambda: popup.has("#__azdrive_unlock_code"))
        popup.text_input("#__azdrive_unlock_code", recovery)
        popup.click_exact("Unlock")
        second.until("the drive unlocked", lambda: second.shows("is unlocked on this computer"))
        names = tops + (["after-the-step.txt"] if args.after_upload else [])
        copy_between(second, lambda: second.click(selector=side_drive(drive_id)),
                     r"%s / \d+" % re.escape(drive_id), names,
                     lambda: second.click(selector="#" + I("side-fav-documents")),
                     r"home Documents/ \d+")
        shot(second, os.path.join(out, "07-second-computer.png"), False)
        if args.pack_files:
            pack_delete_half(second, drive_id, result)
    finally:
        second.stop()
    downloaded = tree_digests(os.path.join(other_home, "Documents"))
    if downloaded != uploaded:
        missing = sorted(set(uploaded) - set(downloaded))
        differ = sorted(k for k in uploaded if k in downloaded and downloaded[k] != uploaded[k])
        raise Failure("the second computer's copy differs: missing %s, different %s"
                      % (missing[:10], differ[:10]))
    log("6. A second profile picked %s up with the claim code, unlocked it with the recovery "
        "code and copied %d files back: identical bytes" % (drive_id, len(downloaded)))
    result["downloaded"] = downloaded
    if args.pack_files:
        pack_after_maintenance(binary, other_switches, other_env, other_home, args, logs,
                               drive_id, uploaded, result)
    with open(os.path.join(logs, "result.json"), "w", encoding="utf-8") as f:
        json.dump(result, f, indent=1)
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--token-url", default="http://127.0.0.1:8081")
    parser.add_argument("--sandbox-url", default="http://127.0.0.1:8784")
    parser.add_argument("--debug-port", type=int, default=8800)
    parser.add_argument("--timeout", type=float, default=600)
    parser.add_argument("--logs", help="the run's folder (default: a new temporary one)")
    parser.add_argument("--out", help="screenshots (default: <logs>/shots)")
    parser.add_argument("--card", default="4242424242424242")
    parser.add_argument("--decline-first", nargs="?", const="4000000000000002",
                        help="try a card the sandbox declines first (default 4000 0000 0000 0002)")
    parser.add_argument("--country", default="DE")
    parser.add_argument("--locale", default="de_DE")
    parser.add_argument("--upload-mb", type=int, default=5)
    parser.add_argument("--small-files", type=int, default=24)
    parser.add_argument("--window", action="store_true", help="real windows, not headless")
    parser.add_argument("--pack-files", type=int, default=0,
                        help="the pack proof: this many small files (half in keep/, half in "
                        "drop/) uploaded, read back on the second computer, drop/ deleted, a "
                        "maintenance round, keep/ read back again")
    parser.add_argument("--maintain-wait", type=int, default=120,
                        help="seconds the restarted second computer is left idle for its "
                        "maintenance round (the pack proof)")
    parser.add_argument("--during-upload", help="a shell command run while the big file uploads "
                        "(a storage fault)")
    parser.add_argument("--after-upload", help="a shell command run after the upload (a node "
                        "restart); AzDrive then lists the drive again and takes one more file")
    args = parser.parse_args()
    if args.logs:
        if os.path.exists(args.logs):
            shutil.rmtree(args.logs)
        os.makedirs(args.logs)
        logs = args.logs
    else:
        logs = tempfile.mkdtemp(prefix="azdrive-fullsim-")
    passed = False
    try:
        passed = run(args, logs)
    except Failure as e:
        log("FAIL: %s" % e)
    finally:
        log("logs and screenshots in %s" % logs)
    if passed:
        log("PASS")
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
