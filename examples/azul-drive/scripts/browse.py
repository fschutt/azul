#!/usr/bin/env python3
"""AzDrive end to end: add an S3 drive through the dialog, browse to /mail/inbox/, download ONE file,
walk the source list, go Back / Forward / Up.

    1. starts the local S3 (s3_server.py, stdlib backend, in this process) with the bucket
       `azdrive-e2e` holding mail/inbox/0001.eml, 0002.eml, mail/sent/0003.eml, docs/readme.txt
       and 60 objects under bulk/;
    2. starts AzDrive headless (AZ_BACKEND=headless, the debug server on --debug-port) with a
       temporary Home folder, drives file and Downloads folder (`--home`, `--drives`,
       `--downloads`); it opens on "This PC";
    3. through AzDrive's debug server: This PC's "Add drive" on the ribbon's Computer tab,
       the dialog's Connect data source > S3-compatible storage, types name, endpoint, region,
       bucket, access key and secret key into the form, clicks "Test connection" (asserts it
       says "Connection OK" after exactly one ListObjectsV2 call) and "Add drive";
    4. asserts the drives file names the drive and holds neither key, and the source list shows
       the drive in CLOUD (its row selected, its eject button);
    5. double-clicks the "mail" and "inbox" folders (the view shows 0001.eml), and asserts that
       browsing fetched listings only, not one object;
    6. selects 0001.eml, clicks "Download" (the ribbon's Share tab: a transfer into the
       Downloads folder), and
       asserts the downloaded bytes are the object's and that the server saw exactly one
       GetObject, for mail/inbox/0001.eml;
    7. clicks Home in the source list (`#__azdrive_side_drive_home`: the Home drive lists,
       notes.txt shows), then the address bar's Back (mail/inbox/ again), Forward (Home again)
       and Up ("This PC").

Usage (from the azul repository, after building libazul with the debug server and AzDrive):

    python3 examples/azul-drive/scripts/browse.py [--bin target/release/AzDrive]
        [--debug-port 8769] [--timeout 90] [--keep-logs] [--window-dialogs]

`AZDRIVE_BIN` also names the binary. By default the form is AzDrive's in-window sheet
(`--dialogs inline`); `--window-dialogs` drives the real modal Dialog window instead, by its
DOM id (list_doms), which needs the debug server to route popup DOMs. Logs and the temporary
folders go to a directory printed at the end (kept on failure, or with --keep-logs).
"""

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import s3_server  # noqa: E402

BUCKET = "azdrive-e2e"
ACCESS = "AKIDAZDRIVEE2E"
SECRET = "azdrive-e2e-secret-key"
REGION = "us-east-1"
DRIVE_NAME = "E2E Drive"
TARGET = "mail/inbox/0001.eml"
SEED = {
    "mail/inbox/0001.eml": b"From: ann@example.com\r\nSubject: first\r\n\r\nHello from the bucket.\r\n",
    "mail/inbox/0002.eml": b"From: ben@example.com\r\nSubject: second\r\n\r\nAnother one.\r\n",
    "mail/sent/0003.eml": b"From: me@example.com\r\nSubject: sent\r\n\r\nSent mail.\r\n",
    "docs/readme.txt": b"A bucket for AzDrive's end-to-end test.\n",
}
FORM_ERRORS = ("Give the drive", "Enter the", "The endpoint", "The connection failed",
               "The drive could not", "There is no configuration")


def log(line):
    print("[browse] %s" % line, flush=True)


class Failure(Exception):
    pass


def repo_roots():
    repo = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
    roots = [repo]
    try:
        common = subprocess.run(
            ["git", "-C", repo, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            check=True, capture_output=True, text=True,
        ).stdout.strip()
        main = os.path.dirname(common)
        if main not in roots:
            roots.append(main)
    except (OSError, subprocess.CalledProcessError):
        pass
    return roots


def find_binary(explicit):
    exe = "AzDrive.exe" if os.name == "nt" else "AzDrive"
    candidates = [explicit, os.environ.get("AZDRIVE_BIN")]
    for root in repo_roots():
        for parts in (("release",), ("debug",), ("consumer", "release"), ("consumer", "debug")):
            candidates.append(os.path.join(root, "target", *parts, exe))
    for candidate in candidates:
        if candidate and os.path.isfile(candidate):
            return os.path.abspath(candidate)
    raise Failure("the AzDrive binary was not found (pass --bin); tried:\n  " +
                  "\n  ".join(c for c in candidates if c))


class App:
    """AzDrive under its debug server."""

    def __init__(self, binary, switches, port, env, logs, deadline):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "azdrive.out")
        self.err_path = os.path.join(logs, "azdrive.err")
        self.dom_id = None
        self.process = subprocess.Popen(
            [binary] + list(switches),
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=open(self.out_path, "wb"),
            stderr=open(self.err_path, "wb"),
        )

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()

    def op(self, op, dom_id=None, **params):
        """One op on the debug server; it answers once the app has processed it."""
        body = {"op": op}
        body.update(params)
        if dom_id is not None:
            body["dom_id"] = dom_id
        request = urllib.request.Request(
            "http://127.0.0.1:%d/" % self.port,
            data=json.dumps(body).encode("utf-8"),
            method="POST",
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            return json.loads(response.read().decode("utf-8") or "{}")

    def must(self, op, dom_id=None, **params):
        answer = self.op(op, dom_id=dom_id, **params)
        shown = json.dumps(answer)[:160]
        if isinstance(answer, dict) and answer.get("status") == "error":
            raise Failure("%s %s failed: %s" % (op, json.dumps(params), shown))
        target = params.get("text") or params.get("selector") or ""
        log("%s %s -> %s" % (op, ('"%s"' % target) if target else "", shown))
        return answer

    def texts(self, dom_id=None):
        return list(strings(self.op("get_node_hierarchy", dom_id=dom_id)))

    def nodes_with_class(self, cls, dom_id=None):
        """The node indices carrying the class `cls`, in document order."""
        answer = self.op("get_node_hierarchy", dom_id=dom_id)
        return [d["index"] for d in dicts(answer)
                if isinstance(d.get("classes"), list) and cls in d["classes"] and "index" in d]

    def shows(self, text, dom_id=None):
        return any(text in t for t in self.texts(dom_id))

    def nodes(self, dom_id=None):
        """The window's nodes (`index`, `type`, `id`, `classes`, `text`, ...)."""
        answer = self.op("get_node_hierarchy", dom_id=dom_id)
        return [d for d in dicts(answer) if "index" in d and "type" in d]

    def ids(self, dom_id=None):
        """The DOM ids of the window's nodes."""
        return {n.get("id") for n in self.nodes(dom_id) if n.get("id")}

    def classes_of(self, node_id, dom_id=None):
        """The classes of the node whose DOM id is `node_id` (none when it is not there)."""
        for n in self.nodes(dom_id):
            if n.get("id") == node_id:
                return n.get("classes") or []
        return []

    def printed(self, key, pattern=r"\S+"):
        """Every `<KEY> <value>` line the app printed on stdout."""
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^%s (%s)$" % (re.escape(key), pattern), text, re.M)

    def until(self, what, check, interval=0.3):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("AzDrive exited (%s) while waiting for %s" % (self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for v in value:
            yield from strings(v)
    elif isinstance(value, dict):
        for v in value.values():
            yield from strings(v)


def dicts(value):
    if isinstance(value, dict):
        yield value
        for v in value.values():
            yield from dicts(v)
    elif isinstance(value, list):
        for v in value:
            yield from dicts(v)


def tail(path, lines=30):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return "".join(f.readlines()[-lines:])
    except OSError:
        return "(no output)"


def seed(root):
    for key, data in SEED.items():
        path = os.path.join(root, BUCKET, *key.split("/"))
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)
    for i in range(60):
        path = os.path.join(root, BUCKET, "bulk", "%03d.bin" % i)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(bytes([i % 256]) * 512)


def open_folder(app, label, prefix):
    """Double-clicks the item `label` and waits for the listing of `prefix`."""
    listed = r"\S+ %s \d+" % re.escape(prefix)
    before = len(app.printed("AZDRIVE_LISTED", listed))
    click_anywhere(app, "double_click", label)
    try:
        app.until("the listing of %s" % prefix,
                  lambda: len(app.printed("AZDRIVE_LISTED", listed)) > before)
    except Failure:
        # The double-click did not open it: select the item and press Enter (Explorer's Open).
        log("WARNING: double-clicking %s did not open it; selecting it and pressing Enter" % label)
        click_anywhere(app, "click", label)
        time.sleep(0.3)
        mods = {"shift": False, "ctrl": False, "alt": False, "meta": False}
        app.must("key_down", key="enter", modifiers=mods)
        app.must("key_up", key="enter", modifiers=mods)
        app.until("the listing of %s (Open)" % prefix,
                  lambda: len(app.printed("AZDRIVE_LISTED", listed)) > before)


NAV_CLASS = "__azul-native-address-bar-nav"
RIBBON_ID = "__azdrive_ribbon"


def norm(text):
    """A label as it reads: a large ribbon button sets its label on two lines with no-break
    spaces between the words of a line."""
    return " ".join((text or "").replace("\u00a0", " ").split())


def dom_ids(app):
    """The window's DOMs, the virtual views' first: a folder's rows are the virtual view's own
    DOM (src/ui_view.rs), which a text search of DOM 0 never reaches."""
    answer = app.op("list_doms")
    ids = [d["dom_id"] for d in dicts(answer) if isinstance(d.get("dom_id"), int)]
    ids = sorted(set(ids), key=lambda d: (d == 0, d))
    return ids or [0]


def shows_anywhere(app, text):
    """Whether any DOM of the window shows `text` (a row of the virtual view included)."""
    return any(app.shows(text, dom_id=d) for d in dom_ids(app))


def click_anywhere(app, op, text):
    """`op` (click, double_click) on the first node holding `text`, the virtual views' DOMs
    searched first."""
    for d in dom_ids(app):
        answer = app.op(op, dom_id=d, text=text)
        if isinstance(answer, dict) and answer.get("status") != "error":
            log("%s %r in DOM %s" % (op, text, d))
            return answer
    raise Failure("%s %r: no DOM of the window has it" % (op, text))


def ribbon_click(app, label):
    """Clicks the ribbon's control (or tab) labelled `label`: the node under the ribbon
    (#__azdrive_ribbon) whose text reads `label`, through its nearest ancestor with a box."""
    def found():
        nodes = app.nodes()
        by_index = {n["index"]: n for n in nodes}

        def inside(n):
            for _ in range(256):
                if n is None:
                    return False
                if n.get("id") == RIBBON_ID:
                    return True
                n = by_index.get(n.get("parent"))
            return False

        for n in nodes:
            if norm(n.get("text")) == label and inside(n):
                return n
        return None

    node = app.until('the ribbon\'s "%s"' % label, found)
    parents = {n["index"]: n.get("parent") for n in app.nodes()}
    at = node.get("parent", node["index"])
    while isinstance(at, int) and at >= 0:
        answer = app.op("click", node_id=at, button="left")
        if isinstance(answer, dict) and answer.get("status") != "error":
            log('ribbon: "%s" (node %d)' % (label, at))
            time.sleep(0.3)
            return
        at = parents.get(at)
    raise Failure('click on the ribbon\'s "%s": no node from its label up has a box' % label)


def nav_click(app, index, what):
    """Clicks the address bar's button `index` (0 Back, 1 Forward, 2 Recent locations, 3 Up,
    4 Refresh), found afresh each time."""
    arrows = app.until("the address bar's arrows", lambda: app.nodes_with_class(NAV_CLASS))
    if len(arrows) < 4:
        raise Failure("the address bar has %d buttons, not Back / Forward / Recent / Up" % len(arrows))
    log("%s: arrow node %d" % (what, arrows[index]))
    app.must("click", node_id=arrows[index])


def run(args, logs):
    binary = find_binary(args.bin)
    log("AzDrive: %s" % binary)
    log("logs and data: %s" % logs)
    deadline = time.time() + args.timeout

    s3_root = os.path.join(logs, "s3")
    seed(s3_root)
    server = s3_server.start(s3_root, access_key=ACCESS, secret_key=SECRET, region=REGION,
                             buckets=[BUCKET], log_path=os.path.join(logs, "s3-requests.jsonl"))
    log("local S3 on %s, bucket %s" % (server.url, BUCKET))

    home = os.path.join(logs, "home")
    downloads = os.path.join(logs, "downloads")
    drives_file = os.path.join(logs, "config", "drives.json")
    os.makedirs(home)
    os.makedirs(downloads)
    with open(os.path.join(home, "notes.txt"), "w") as f:
        f.write("home\n")
    # The engine's variables; AzDrive's own settings are switches (src/args.rs), and a switch
    # wins over any AZDRIVE_* variable the caller's environment holds.
    env = dict(os.environ)
    env.update({
        "AZ_BACKEND": "headless",
        "AZ_DEBUG": str(args.debug_port),
        # Not the user's shared Azlin config (~/.azlin/config.json: the look, the endpoints),
        # which a settings save would also write.
        "AZLIN_CONFIG": "off",
    })
    switches = [
        # The run's own data root (drive/view.json, the "Azlin" drive), not the user's.
        "--data-dir", os.path.join(logs, "data"),
        "--home", home,
        "--downloads", downloads,
        "--drives", drives_file,
        "--dialogs", "window" if args.window_dialogs else "inline",
    ]

    app = App(binary, switches, args.debug_port, env, logs, deadline)
    try:
        app.until("AzDrive's window", lambda: app.shows("This PC"))
        app.until("the This PC view", lambda: app.printed("AZDRIVE_PLACE", r"this-pc"))

        # 3. The Add drive dialog: This PC's ribbon tab, Computer > Network > Add drive; its
        # Connect data source > S3-compatible storage opens the bucket's form.
        ribbon_click(app, "Add drive")
        dom = None
        if args.window_dialogs:
            def dialog_dom():
                answer = app.op("list_doms")
                ids = [d["dom_id"] for d in dicts(answer) if "dom_id" in d and not d.get("is_root", True)]
                return ids[-1] if ids else None
            dom = app.until("the dialog window's DOM (list_doms)", dialog_dom)
            log("the dialog is DOM %s" % dom)
        app.until("the Add drive dialog's choices", lambda: app.shows("Connect data source", dom_id=dom))
        app.must("click", dom_id=dom, selector="#__azdrive_add_choice_connect")
        app.until("the dialog's sources", lambda: app.printed("AZDRIVE_ADD_PAGE", r"sources"))
        app.must("click", dom_id=dom, selector="#__azdrive_add_service_s3")
        app.until("the S3 form", lambda: app.printed("AZDRIVE_ADD_PAGE", r"form s3"))
        app.until("the Add drive form", lambda: app.shows("Test connection", dom_id=dom))
        # The name field holds the source's own name: typed over (End, a Backspace per character;
        # every key_down has its key_up).
        mods = {"shift": False, "ctrl": False, "alt": False, "meta": False}
        app.must("focus_node", dom_id=dom, selector="#__azdrive_add_name")
        time.sleep(0.15)
        for key in ["end"] + ["backspace"] * len("S3-compatible storage"):
            app.must("key_down", key=key, modifiers=mods)
            app.must("key_up", key=key, modifiers=mods)
        time.sleep(0.15)
        app.must("text_input", dom_id=dom, text=DRIVE_NAME)
        time.sleep(0.15)
        # The form's fields are its source's settings (azul-storage src/catalog.rs).
        for short, text in (
            ("add_field_endpoint", server.url),
            ("add_field_region", REGION),
            ("add_field_bucket", BUCKET),
            ("add_field_access_key_id", ACCESS),
            ("add_field_secret_access_key", SECRET),
        ):
            selector = "#__azdrive_" + short
            app.must("focus_node", dom_id=dom, selector=selector)
            time.sleep(0.15)
            app.must("text_input", dom_id=dom, text=text)
            time.sleep(0.15)

        server.clear_log()
        app.must("click", dom_id=dom, text="Test connection")
        tested = app.until("the connection test (AZDRIVE_TESTED)", lambda: app.printed("AZDRIVE_TESTED"))
        if tested[-1] != "ok":
            problem = [t for t in app.texts(dom) if t.startswith("The connection failed")]
            raise Failure("the connection test failed: %s" % (problem or tested))
        app.until('"Connection OK" in the form', lambda: app.shows("Connection OK", dom_id=dom))
        calls = [(r["method"], r["op"], r["query"].get("max-keys")) for r in server.requests()]
        if calls != [("GET", "ListObjectsV2", "1")]:
            raise Failure("Test connection made %r, not one ListObjectsV2 with max-keys=1" % calls)
        log("Test connection: one ListObjectsV2 call, max-keys=1, answered OK")

        app.must("click", dom_id=dom, selector="#__azdrive_add_save")
        added = app.until("the drive to be saved (AZDRIVE_ADDED)", lambda: app.printed("AZDRIVE_ADDED"))
        drive_id = added[-1]
        app.until("the drive's root listing", lambda: app.printed("AZDRIVE_LISTED", r"%s / \d+" % re.escape(drive_id)))
        log("AzDrive saved %s and lists its root" % drive_id)
        # The source list's CLOUD section: the drive's row (selected: the window shows it) and its
        # eject button (src/ui_sidebar.rs; the ids of src/ids.rs).
        part = re.sub(r"[^A-Za-z0-9_-]", "_", drive_id).lower()
        row = "__azdrive_side_drive_" + part
        app.until("the drive's row in CLOUD", lambda: row in app.ids())
        app.until("its eject button", lambda: "__azdrive_side_eject_" + part in app.ids())
        app.until("its row selected", lambda: "__azdrive_side_selected" in app.classes_of(row))
        log("the source list shows %s in CLOUD, selected, with its eject button" % drive_id)

        # 4. The drives file: the drive, no keys.
        with open(drives_file, "r", encoding="utf-8") as f:
            text = f.read()
        saved = json.loads(text)
        names = [d["name"] for d in saved["drives"]]
        if saved.get("format") != "azul-storage.drives" or names != [DRIVE_NAME]:
            raise Failure("the drives file is %s" % text)
        if SECRET in text or ACCESS in text:
            raise Failure("the drives file holds a key: %s" % text)
        location = saved["drives"][0]["location"]
        if location.get("bucket") != BUCKET or location.get("endpoint") != server.url:
            raise Failure("the drives file names another bucket: %s" % text)
        log("the drives file names the drive and holds no key")

        # 5. Browse to mail/inbox/.
        server.clear_log()
        app.until("the mail folder in the list", lambda: shows_anywhere(app, "mail"))
        open_folder(app, "mail", "mail/")
        app.until("inbox in the list", lambda: shows_anywhere(app, "inbox"))
        open_folder(app, "inbox", "mail/inbox/")
        app.until("0001.eml in the list", lambda: shows_anywhere(app, "0001.eml"))
        ops = sorted({r["op"] for r in server.requests()})
        if server.object_gets() or ops != ["ListObjectsV2"]:
            raise Failure("browsing fetched more than listings: %r" % server.requests())
        prefixes = [r["query"].get("prefix") for r in server.requests()]
        log("browsing made %d listing call(s) (%s) and fetched no object" % (len(prefixes), prefixes))

        # 6. Download ONE file: select it, Share > Cloud > Download on the ribbon (a transfer of
        # the queue).
        click_anywhere(app, "click", "0001.eml")
        time.sleep(0.3)
        ribbon_click(app, "Share")
        ribbon_click(app, "Download")
        app.until("the download (AZDRIVE_TRANSFER done)",
                  lambda: app.printed("AZDRIVE_TRANSFER", r"\d+ done 1"))
        path = os.path.join(downloads, "0001.eml")
        if os.path.dirname(os.path.abspath(path)) != os.path.abspath(downloads):
            raise Failure("the file went to %s, not into %s" % (path, downloads))
        with open(path, "rb") as f:
            data = f.read()
        if data != SEED[TARGET]:
            raise Failure("the downloaded bytes differ: %r" % data[:80])
        gets = server.object_gets()
        if gets != [TARGET]:
            raise Failure("the server served %r, not exactly one GetObject of %s" % (gets, TARGET))
        heads = [r for r in server.requests() if r["op"] == "HeadObject"]
        log("downloaded %s (%d bytes, identical); the server saw one GetObject (%s)%s" % (
            path, len(data), gets[0], ", and %d HEAD" % len(heads) if heads else ""))

        # 7. The source list's Home (LOCATIONS), then Back / Forward / Up on the address bar.
        home_listed = r"home / \d+"
        before = len(app.printed("AZDRIVE_LISTED", home_listed))
        app.must("click", selector="#__azdrive_side_drive_home")
        app.until("the Home drive listed from the source list",
                  lambda: len(app.printed("AZDRIVE_LISTED", home_listed)) > before)
        app.until("notes.txt in the Home drive", lambda: shows_anywhere(app, "notes.txt"))
        inbox_listed = r"%s mail/inbox/ \d+" % re.escape(drive_id)
        before = len(app.printed("AZDRIVE_LISTED", inbox_listed))
        nav_click(app, 0, "Back")
        app.until("Back to mail/inbox/",
                  lambda: len(app.printed("AZDRIVE_LISTED", inbox_listed)) > before)
        before = len(app.printed("AZDRIVE_LISTED", home_listed))
        nav_click(app, 1, "Forward")
        app.until("Forward to the Home drive",
                  lambda: len(app.printed("AZDRIVE_LISTED", home_listed)) > before)
        before = len(app.printed("AZDRIVE_PLACE", r"this-pc"))
        nav_click(app, 3, "Up")
        app.until("Up to This PC", lambda: len(app.printed("AZDRIVE_PLACE", r"this-pc")) > before)
        log("the source list opened the Home drive; Back, Forward and Up walked the history")
        log("PASS: AzDrive added an S3 drive, browsed to /mail/inbox/, downloaded exactly one "
            "object, and walked the source list and the history")
        return True
    except Failure:
        for name, path in (("stdout", app.out_path), ("stderr", app.err_path)):
            print("\n----- azdrive %s (tail) -----\n%s" % (name, tail(path)))
        raise
    finally:
        app.stop()
        server.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--bin", help="the AzDrive binary")
    parser.add_argument("--debug-port", type=int, default=8769)
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--keep-logs", action="store_true")
    parser.add_argument("--window-dialogs", action="store_true",
                        help="drive the modal Dialog window instead of the in-window sheet")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azdrive-browse-")
    passed = False
    try:
        passed = run(args, logs)
    except Failure as e:
        log("FAIL: %s" % e)
    finally:
        if passed and not args.keep_logs:
            shutil.rmtree(logs, ignore_errors=True)
        else:
            log("logs kept in %s" % logs)
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
