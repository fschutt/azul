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
        [--card 4242424242424242] [--pay card|blik] [--upload-mb 5] [--window]

`--pay blik` pays the way a payer in Poland does (`--country PL --locale pl_PL` by default then):
the tiers in zloty, the BLIK pill first, Buy opens the sandbox's BLIK page in the popover; the
payer types the code from the banking app (the page's form posted with the sandbox's test code),
approves on the fake phone the page sends them to, and the page hands back to the token server's
return page. With the card run (steps 1 - 3) this is azdrive_add_e2e's 16a, 7 and 16b against the
local stack instead of the mock.

`--window` runs the buyer's AzDrive in a real window (the second computer stays headless: its
unlock is in the drive's context menu, a native menu on macOS that no script may drive): the
popover's web view loads the sandbox's real page, which the sandbox serves with the test card
typed in (`/_dev/fill`: nothing can type into a real web view), and the page itself confirms on
Pay. The screenshots are azul's render of the windows, without the web view's page (a native
view; the OS's capture of it needs the Screen Recording permission). Each profile keeps its
keyring in a file of the run (AZ_KEYRING_FILE, which a windowed run honours too), never in the
system keyring.
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
from azdrive_e2e import Drive, I, focus_search_box, open_item, select_item
from azdrive_add_e2e import (add_id, drive_menu, hand_back, keyring_entries, navigate,
                             navigation_to, new_drive_encrypted, open_dialog, other_profile,
                             pay_id, pending_checkouts, popover_of, side_drive, the_view,
                             wait_closed, webviews)

# The cardholder typed in the popover; the sandbox's test cards (its page lists them).
NAME = "Erika Example"
CARD_3DS = "4000002500003155"
# The sandbox's documented BLIK test code: any other code is refused and not kept.
BLIK_CODE = "123456"
# Per way to pay: the pill the stack offers for it, the surface Buy opens (`AZDRIVE_PAY_SURFACE`)
# and the words of the popover's chip after the page's host.
PAYS = {
    "card": ("card:fake-stripe", r"fields 127\.0\.0\.1:\d+", "card fields by Fake Stripe"),
    "blik": ("blik:fake-stripe", r"page 127\.0\.0\.1:\d+", "payment page of Fake Stripe"),
}
# The country and locale a way to pay is bought from unless the command line names them, and
# the currency the tiers then show.
PAY_FROM = {"card": ("DE", "de_DE", "EUR"), "blik": ("PL", "pl_PL", "PLN")}


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


def pack_name(i):
    """The pack proof's file `i`: one in three is kept, the others are deleted later. The names
    sort keep, drop, drop, keep, ... so one paste of the folder puts both in every pack, and the
    delete leaves each pack a third live - below the half under which a maintenance round
    repacks it."""
    return "n%05d-%s.txt" % (i, "keep" if i % 3 == 0 else "drop")


def make_pack(documents, count):
    """Documents/fullsim-pack/: `count` small text files, 20 bytes to 4 KB each - what an
    encrypted Azlin drive packs into pack objects instead of an object a file."""
    root = os.path.join(documents, PACK)
    os.makedirs(root)
    for i in range(count):
        size = 20 + (i * 7919) % 4000
        line = ("small file %d of the pack proof\n" % i).encode()
        with open(os.path.join(root, pack_name(i)), "wb") as f:
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
                 during=None, source_shown=False):
    """Copies the items `names` of one folder (opened by `source_click`, listed when the
    AZDRIVE_LISTED line matches `source_listed`) into another (`target_click`,
    `target_listed`), one item at a time, and waits for each paste to finish. `during` runs
    while the first paste is under way (after its "planned" line, before its "done" one).
    `source_shown`: the source folder shows and is listed already, so the first item is taken
    from it as it is (a click on the folder that shows lists nothing again)."""
    for i, name in enumerate(names):
        if not (source_shown and i == 0):
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


def play_blik_page(app, popover, view, sandbox, token_url):
    """The sandbox's BLIK page, played (a headless run loads nothing in a web view): the payer
    types the code from the banking app - the page's form posted with it, as the page sends it -
    and approves on the fake phone the page sends them to; the sandbox's signed webhook approves
    the checkout at the token server, and the page hands back to the token server's return
    page."""
    src = view.get("src", "")
    if not src.startswith(sandbox + "/stripe/c/pay/"):
        raise Failure("the popover shows %r, not the sandbox's BLIK page" % src[:80])
    form = urllib.parse.urlencode({"code": BLIK_CODE}).encode("ascii")
    # The page answers the code with a redirect to the phone (followed as a GET).
    with urllib.request.urlopen(urllib.request.Request(src, data=form, method="POST"),
                                timeout=10) as response:
        phone = response.geturl()
    if "/stripe/phone/" not in phone:
        raise Failure("the BLIK code led to %r, not the fake phone" % phone)
    form = urllib.parse.urlencode({"action": "authorize"}).encode("ascii")
    with urllib.request.urlopen(urllib.request.Request(phone, data=form, method="POST"),
                                timeout=10) as response:
        if response.status != 200:
            raise Failure("the fake phone answered the approval with %s" % response.status)
    hand_back(app, popover, view, token_url + "/return/ok")


def shot(win, path, window):
    """A screenshot of `win`: azul's own render of the window (a real window's web view is a
    native view that render leaves out; the OS's capture of it needs the Screen Recording
    permission, without which it is blank)."""
    win.screenshot(path)


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


def pack_delete_drop(app, drive_id, count, result):
    """The pack proof's delete: the two thirds of fullsim-pack/'s files named "-drop" go for
    good, found by a search of the drive's names and selected all at once. Every pack keeps a
    third of its files live: sparse, none empty."""
    drop = sum(1 for i in range(count) if "-drop" in pack_name(i))
    drive = re.escape(drive_id)
    app.after("the drive", "AZDRIVE_LISTED", r"%s / \d+" % drive,
              lambda: app.click(selector=side_drive(drive_id)))
    app.after("the pack folder", "AZDRIVE_LISTED", r"%s %s/ %d" % (drive, PACK, count),
              lambda: open_item(app, PACK))
    focus_search_box(app)
    app.after("the search for -drop", "AZDRIVE_SEARCHED", r"\d+ \w+ -drop",
              lambda: (app.must("text_input", text="-drop"), app.frame(2)))
    # names only: the files' text never says "-drop", and reading 5,000 files is not the step
    app.after("names only", "AZDRIVE_SEARCHED", r"%d names -drop" % drop,
              lambda: app.click_within("#" + I("find-bar"), "File contents"))
    app.after("Ctrl+A", "AZDRIVE_SELECTED", r"%d .*-drop\.txt" % drop,
              lambda: app.key("a", primary=True))
    windows = len(app.window_ids())
    app.key("delete")
    app.until("the delete's question", lambda: len(app.window_ids()) > windows)
    gone = app.after("the delete", "AZDRIVE_DELETED", r"\d+",
                     lambda: e2e.click_dialog_button(app, "Delete", closes=True))
    if int(gone) != drop:
        raise Failure("the delete took %s files, not the %d named -drop" % (gone, drop))
    result["pack_deleted"] = int(gone)
    log("6p. the %s files named -drop deleted for good from %s, each pack a third live"
        % (gone, drive_id))


def pack_reads(app, drive_id):
    """(ranged, whole, bytes): the drive's reads of packs since AzDrive started, as its last
    AZDRIVE_PACK_READS line counts them (after each finished transfer); zeros before any."""
    lines = app.printed("AZDRIVE_PACK_READS", r"%s .*" % re.escape(drive_id)) or []
    if not lines:
        return (0, 0, 0)
    fields = dict(f.split("=", 1) for f in lines[-1].split()[1:])
    return (int(fields["ranged"]), int(fields["whole"]), int(fields["bytes"]))


def pack_after_maintenance(binary, switches, env, home, args, logs, drive_id, uploaded, result):
    """The pack proof's end: the second computer starts again and its first idle minute runs a
    maintenance round of the drive's index, which repacks the packs the delete left a third
    live; AzDrive says when the round ended and what it did (AZDRIVE_MAINTAINED). Then one
    kept file is read alone - one ranged GET of its bytes, not its pack downloaded whole - and
    the whole fullsim-pack/ (the kept files) is copied into Documents/after-delete/: every kept
    file reads back byte for byte from the new packs."""
    keyring_file = env["AZ_KEYRING_FILE"]
    drives_file = switches[switches.index("--drives") + 1]
    before = {k for k, _ in bucket_objects(drives_file, keyring_file, drive_id)}
    one = os.path.join(home, "Documents", "one-file")
    target = os.path.join(home, "Documents", "after-delete")
    os.makedirs(one)
    os.makedirs(target)
    app = start_app("azdrive-second-again", binary, switches, args.debug_port + 1, logs,
                    args.timeout, env, False)
    drive = re.escape(drive_id)
    try:
        log("6q. the second computer started again: up to %d s for its maintenance round"
            % args.maintain_wait)
        deadline = time.time() + args.maintain_wait
        done = None
        while done is None:
            if app.printed("AZDRIVE_MAINTAIN_STOPPED", r".+"):
                raise Failure("the maintenance round stopped: %s"
                              % app.printed("AZDRIVE_MAINTAIN_STOPPED", r".+"))
            lines = app.printed("AZDRIVE_MAINTAINED", r"%s .*" % drive)
            if lines:
                done = lines[-1]
            elif time.time() > deadline:
                raise Failure("no maintenance round ended within %d s" % args.maintain_wait)
            else:
                time.sleep(1)
        fields = dict(f.split("=", 1) for f in done.split()[1:] if "=" in f)
        result["maintained"] = fields
        if int(fields.get("packs_read", 0)) < 1 or int(fields.get("files_moved", 0)) < 1:
            raise Failure("the maintenance round repacked nothing: %s" % done)
        after = {k for k, _ in bucket_objects(drives_file, keyring_file, drive_id)}
        result["bucket_after_maintenance"] = {"objects": len(after), "new": len(after - before),
                                              "gone": len(before - after)}
        if not after - before:
            raise Failure("the maintenance round wrote no new pack: %s" % done)
        app.after("the drive", "AZDRIVE_LISTED", r"%s / \d+" % drive,
                  lambda: app.click(selector=side_drive(drive_id)))
        kept = {k: v for k, v in uploaded.items() if k.startswith(PACK + "/")
                and k.endswith("-keep.txt")}
        app.after("the pack folder", "AZDRIVE_LISTED", r"%s %s/ %d" % (drive, PACK, len(kept)),
                  lambda: open_item(app, PACK))
        first = pack_name(3)
        app.after("one kept file selected", "AZDRIVE_SELECTED", r"1 .*%s" % re.escape(first),
                  lambda: select_item(app, first))
        app.after("Ctrl+C", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.key("c", primary=True))
        app.after("Documents", "AZDRIVE_LISTED", r"home Documents/ \d+",
                  lambda: app.click(selector="#" + I("side-fav-documents")))
        app.after("one-file", "AZDRIVE_LISTED", r"home Documents/one-file/ 0",
                  lambda: open_item(app, "one-file"))
        reads = pack_reads(app, drive_id)
        copied = app.count("AZDRIVE_TRANSFER", r"\d+ done \d+")
        app.key("v", primary=True)
        app.until("the one file copied", lambda: app.count(
            "AZDRIVE_TRANSFER", r"\d+ done \d+") > copied)
        app.until("the pack reads counted", lambda: pack_reads(app, drive_id) != reads)
        ranged, whole, nbytes = (b - a for a, b in zip(reads, pack_reads(app, drive_id)))
        size = os.path.getsize(os.path.join(one, first))
        result["one_file_read"] = {"ranged": ranged, "whole": whole, "bytes": nbytes,
                                   "file": size}
        # its record: the file's bytes compressed and sealed, never more than the file and a
        # little framing; a pack holds hundreds of them
        if (ranged, whole) != (1, 0) or nbytes > size + 4096:
            raise Failure("reading one small file fetched %s, not one ranged GET of its bytes"
                          % result["one_file_read"])
        log("6r. one kept file read alone: one ranged GET of %d bytes, its pack not downloaded"
            % nbytes)
        app.after("the drive again", "AZDRIVE_LISTED", r"%s / \d+" % drive,
                  lambda: app.click(selector=side_drive(drive_id)))
        app.after("fullsim-pack selected", "AZDRIVE_SELECTED", r"1 .*%s/?" % PACK,
                  lambda: select_item(app, PACK))
        app.after("Ctrl+C", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.key("c", primary=True))
        app.after("Documents", "AZDRIVE_LISTED", r"home Documents/ \d+",
                  lambda: app.click(selector="#" + I("side-fav-documents")))
        app.after("after-delete", "AZDRIVE_LISTED", r"home Documents/after-delete/ 0",
                  lambda: open_item(app, "after-delete"))
        copied = app.count("AZDRIVE_TRANSFER", r"\d+ done \d+")
        app.key("v", primary=True)
        app.until("the kept files copied", lambda: app.count(
            "AZDRIVE_TRANSFER", r"\d+ done \d+") > copied)
    finally:
        app.stop()
    back = {k: v for k, v in tree_digests(target).items()}
    if back != kept:
        raise Failure("after the delete and the maintenance round %d of %d kept files read back "
                      "the same" % (sum(1 for k in kept if back.get(k) == kept[k]), len(kept)))
    log("6s. after the delete and a maintenance round (%s, %s) the %d kept files read back "
        "byte for byte" % (fields, result["bucket_after_maintenance"], len(kept)))


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
    result = {"token_url": args.token_url, "sandbox_url": args.sandbox_url, "pay": args.pay,
              "card": args.card,
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
        pill, surface_shape, chip_words = PAYS[args.pay]
        if pill not in pills.split():
            raise Failure("the pills are %r: no %s (is AzDrive built with --features "
                          "fake-providers, is the stack's sandbox on?)" % (pills, pill))
        if args.country == PAY_FROM[args.pay][0]:
            currency = PAY_FROM[args.pay][2]
            dialog.win.until("the prices in %s" % currency,
                             lambda: dialog.shows("%s " % currency))
        shot(dialog.win, os.path.join(out, "02-tiers.png"), args.window)
        log("1. Buy storage asked for the country (the guess %s picked), %s tiers, pills %s"
            % (args.country, tiers, pills))

        # 2. The smallest tier, the way to pay, Buy.
        dialog.click("tier_0")
        dialog.type_into("name", "Full system", clear=len("Azlin Storage"))
        dialog.click("pill_" + args.pay)
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
        if not re.fullmatch(surface_shape, surface):
            raise Failure("the %s checkout opened %r, not %s in the popover"
                          % (args.pay, surface, surface_shape))
        kept = pending_checkouts(keyring_file).get(checkout)
        if not kept:
            raise Failure("the checkout %s is not on the keyring's list (%s): a window run needs a "
                          "libazul whose keyring honours AZ_KEYRING_FILE" % (checkout, keyring_file))
        claim_code = azlin_claim.claim_code(checkout, base64.b64decode(kept["claim_secret"]))
        result.update(checkout=checkout, claim_code=claim_code)
        log("2. Buy: checkout %s, its %s in the popover" % (checkout, surface))

        # 3. The page's part, the webhook, the drive.
        popover = popover_of(app, dialog)
        view = the_view(popover)
        for text in (chip_words,) + (("CARDHOLDER",) if args.pay == "card" else ()):
            popover.until("the popover's %r" % text, lambda: popover.shows(text))
        # The chip reads "<host> - <words>": the host and its words keep a gap.
        host = popover.box("#__azdrive_pay_host")
        words = popover.text_rect("- " + chip_words)
        if not words or words["x"] < host["x"] + host["width"] + 2:
            raise Failure("the chip runs its host into its words: the host ends at %.1f, the "
                          "words start at %s" % (host["x"] + host["width"],
                                                 words and round(words["x"], 1)))
        shot(popover, os.path.join(out, "04-popover.png"), args.window)
        if args.pay == "blik":
            play_blik_page(app, popover, view, args.sandbox_url, args.token_url)
            last4 = None
        elif args.window:
            last4 = real_page_pays(app, popover, args.sandbox_url, out)
            app.until("the popover closed", lambda: popover.window_id not in app.window_ids())
        else:
            last4 = play_fields_page(app, popover, view, args.sandbox_url, args.card,
                                     decline_first=args.decline_first, out=out)
            popover.screenshot(os.path.join(out, "05-paid.png"))
            hand_back(app, popover, view, args.sandbox_url + "/_bridge/result?v=succeeded")
        drive_id = app.until("the drive of %s" % checkout, lambda: [
            d for d in app.printed("AZDRIVE_ADDED", r"d_\S+") if d not in added])[0]
        paid_with = ("the card ending %s" % last4) if last4 else ("BLIK (code %s, approved on "
                                                                  "the fake phone)" % BLIK_CODE)
        log("3. Paid with %s in the sandbox; the webhook approved %s; drive %s arrived"
            % (paid_with, checkout, drive_id))

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
        # The unlock opens the drive: its bucket lists.
        second.after("the unlocked drive's listing", "AZDRIVE_LISTED",
                     r"%s / \d+" % re.escape(drive_id),
                     lambda: popup.click_exact("Unlock"))
        second.until("the drive unlocked", lambda: second.shows("is unlocked on this computer"))
        names = tops + (["after-the-step.txt"] if args.after_upload else [])
        # The drive shows already: its first file is copied from there, the others after a
        # click on the drive opens it again.
        copy_between(second, lambda: second.click(selector=side_drive(drive_id)),
                     r"%s / \d+" % re.escape(drive_id), names,
                     lambda: second.click(selector="#" + I("side-fav-documents")),
                     r"home Documents/ \d+", source_shown=True)
        shot(second, os.path.join(out, "07-second-computer.png"), False)
        if args.pack_files:
            pack_delete_drop(second, drive_id, args.pack_files, result)
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
    parser.add_argument("--pay", choices=sorted(PAYS), default="card",
                        help="the way to pay: the card in the fields (default) or BLIK on its "
                        "page (a payer in Poland)")
    parser.add_argument("--country", help="the buyer's country (default: the way to pay's, DE "
                        "for the card, PL for BLIK)")
    parser.add_argument("--locale", help="the system locale, whose region is the guess "
                        "(default: de_DE for the card, pl_PL for BLIK)")
    parser.add_argument("--upload-mb", type=int, default=5)
    parser.add_argument("--small-files", type=int, default=24)
    parser.add_argument("--window", action="store_true", help="real windows, not headless")
    parser.add_argument("--pack-files", type=int, default=0,
                        help="the pack proof: this many small files in one folder (one in three "
                        "named -keep, the rest -drop) uploaded in one paste, read back on the "
                        "second computer, the -drop files deleted, a maintenance round that "
                        "repacks the sparse packs, one kept file read alone (one ranged GET), "
                        "the kept files read back again")
    parser.add_argument("--maintain-wait", type=int, default=180,
                        help="seconds the restarted second computer may take until its "
                        "maintenance round ends (the pack proof)")
    parser.add_argument("--during-upload", help="a shell command run while the big file uploads "
                        "(a storage fault)")
    parser.add_argument("--after-upload", help="a shell command run after the upload (a node "
                        "restart); AzDrive then lists the drive again and takes one more file")
    args = parser.parse_args()
    country, locale, _ = PAY_FROM[args.pay]
    args.country = args.country or country
    args.locale = args.locale or locale
    if args.pay != "card" and (args.window or args.decline_first):
        parser.error("--window and --decline-first pay by card")
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
