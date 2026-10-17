#!/usr/bin/env python3
"""AzDrive as the file picker of azul apps (`--pick`), headless, on a temporary Home folder.

The request comes in on stdin as one JSON line; the answer goes out on stdout as exactly one
`AZPICK <json>` line, and the process ends.

    1. a normal start writes the registration beside the drives file (file-picker.json: this
       program, `--pick`, protocol 1, its modes);
    2. Open with a Pictures filter (*.png): a.png and the folder sub show, notes.txt does not;
       a.png selected, Open -> its path, and AzDrive ends;
    3. Escape -> cancelled;
    4. Save: "report.txt" typed into the file name, Save -> <home>/report.txt;
    5. Open folder: into sub (a double-click), Select folder -> <home>/sub;
    6. a cloud object (the mock stack's S3 bucket as a drive): "handle" answers its drive, key
       and size and no path; "copy" answers a path under the cache's picked/ with its bytes.

Usage (from the azul repository, after building libazul with the debug server and AzDrive):

    python3 scripts/azdrive_pick_e2e.py [--bin target/release/AzDrive] [--debug-port 8781]
        [--timeout 180] [--keep-logs]
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import azlin_e2e as e2e  # noqa: E402
import azlin_mock_stack  # noqa: E402
from azlin_e2e import Failure  # noqa: E402
from azdrive_e2e import Drive, I, item_names, open_item, select_item  # noqa: E402

PREFIX = "AZPICK "


def log(line):
    print("[azdrive-pick-e2e] %s" % line, flush=True)


def same_path(a, b):
    return os.path.realpath(a) == os.path.realpath(b)


class Picker(Drive):
    """AzDrive started with `--pick`, its request written to its stdin (the shared driver
    starts an app with no stdin)."""

    def __init__(self, tag, binary, args, port, logs, timeout, request, extra_env=None):
        self.tag = tag
        self.port = port
        self.deadline = time.time() + timeout
        self.out_path = os.path.join(logs, "%s.stdout" % tag)
        self.err_path = os.path.join(logs, "%s.stderr" % tag)
        self.capped = False
        env = dict(os.environ)
        env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(port)})
        env.update(extra_env or {})
        self.process = subprocess.Popen(
            [binary] + list(args) + ["--pick"], env=env, stdin=subprocess.PIPE,
            stdout=open(self.out_path, "wb"), stderr=open(self.err_path, "wb"),
        )
        self.process.stdin.write((json.dumps(request) + "\n").encode("utf-8"))
        self.process.stdin.close()

    def ready(self, name):
        """Waits for the window and for the item `name` in the folder it opened."""
        self.until("the debug server", lambda: self.op("get_dom_tree"))
        self.must("resize", width=900.0, height=600.0)
        self.frame(3)
        self.until('"%s" in the folder' % name, lambda: name in names(self))

    def answer(self):
        """The one AZPICK line, once AzDrive ended."""
        end = time.time() + 30
        while self.process.poll() is None and time.time() < end:
            time.sleep(0.2)
        if self.process.poll() is None:
            raise Failure("AzDrive did not end after its answer")
        with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
            lines = [line[len(PREFIX):] for line in f.read().splitlines() if line.startswith(PREFIX)]
        if len(lines) != 1:
            raise Failure("%d AZPICK lines, not one: %r" % (len(lines), lines))
        return json.loads(lines[0])


def names(app):
    return [n.rstrip("/") for n in item_names(app)]


def request(op, home, **extra):
    out = {"v": 1, "op": op, "title": "Choose a file", "default_location": home, "app": "E2E"}
    out.update(extra)
    return out


def picked_path(answer, what):
    if answer.get("status") != "selected" or len(answer.get("items") or []) != 1:
        raise Failure("%s: not one item selected: %r" % (what, answer))
    path = answer["items"][0].get("path")
    if not path:
        raise Failure("%s: the item has no path: %r" % (what, answer))
    return path


def run(args, logs):
    binary = e2e.find_binary("AzDrive", args.bin, "AZDRIVE_BIN")
    home = os.path.join(logs, "home")
    os.makedirs(os.path.join(home, "sub"))
    for name, data in (("a.png", b"\x89PNG\r\n\x1a\n"), ("notes.txt", b"notes\n")):
        with open(os.path.join(home, name), "wb") as f:
            f.write(data)
    drives_file = os.path.join(logs, "config", "drives.json")
    cache = os.path.join(logs, "cache")
    switches = [
        "--theme", "flat", "--mode", "light", "--language", "en",
        "--home", home,
        "--downloads", os.path.join(logs, "downloads"),
        "--data-dir", os.path.join(logs, "data"),
        "--drives", drives_file,
        "--dialogs", "inline",
        "--cache-dir", cache,
    ]
    port = args.debug_port

    # 1. The registration.
    app = Drive("azdrive", binary, switches + ["--screen", "this-pc"], port, logs, args.timeout)
    try:
        app.until("the This PC view", lambda: app.printed("AZDRIVE_PLACE", r"this-pc"))
        registration = os.path.join(os.path.dirname(drives_file), "file-picker.json")
        app.until("the registration beside the drives file", lambda: os.path.isfile(registration))
    finally:
        app.stop()
    with open(registration, encoding="utf-8") as f:
        value = json.load(f)
    if (value.get("format"), value.get("protocol"), value.get("args")) != (
            "azul-storage.file-picker", 1, ["--pick"]):
        raise Failure("the registration says %r" % value)
    if not same_path(value.get("exec", ""), binary):
        raise Failure("the registration names %r, not %s" % (value.get("exec"), binary))
    if set(value.get("modes") or []) != {"open", "open-multiple", "open-directory", "save"}:
        raise Failure("the registration's modes are %r" % value.get("modes"))
    log("1. the registration beside the drives file names %s, --pick, protocol 1" % binary)

    # 2. Open, filtered.
    pictures = [{"name": "Pictures", "patterns": ["*.png"]}]
    app = Picker("pick-open", binary, switches, port, logs, args.timeout,
                 request("open", home, filters=pictures))
    try:
        app.ready("a.png")
        shown = names(app)
        if "sub" not in shown or "notes.txt" in shown:
            raise Failure("the Pictures filter shows %r (sub yes, notes.txt no)" % shown)
        select_item(app, "a.png")
        app.click(selector="#" + I("pick-ok"), frames=0)  # AzDrive answers and ends
        path = picked_path(app.answer(), "Open")
    finally:
        app.stop()
    if not same_path(path, os.path.join(home, "a.png")):
        raise Failure("Open answered %s, not a.png" % path)
    log("2. Open with *.png: a.png and sub shown, notes.txt not; Open answered %s" % path)

    # 3. Cancel.
    app = Picker("pick-cancel", binary, switches, port, logs, args.timeout, request("open", home))
    try:
        app.ready("a.png")
        # Escape answers and ends AzDrive at its press: no key_up can follow.
        app.must("key_down", key="escape",
                 modifiers={"shift": False, "ctrl": False, "alt": False, "meta": False})
        answer = app.answer()
    finally:
        app.stop()
    if answer.get("status") != "cancelled" or answer.get("items"):
        raise Failure("Escape answered %r" % answer)
    log("3. Escape answered cancelled")

    # 4. Save.
    app = Picker("pick-save", binary, switches, port, logs, args.timeout, request("save", home))
    try:
        app.ready("a.png")
        app.text_input("#" + I("pick-name"), "report.txt")
        app.click(selector="#" + I("pick-ok"), frames=0)  # AzDrive answers and ends
        path = picked_path(app.answer(), "Save")
    finally:
        app.stop()
    if not same_path(path, os.path.join(home, "report.txt")):
        raise Failure("Save answered %s, not report.txt in Home" % path)
    log("4. Save of report.txt answered %s" % path)

    # 5. A folder.
    app = Picker("pick-folder", binary, switches, port, logs, args.timeout,
                 request("open-directory", home))
    try:
        app.ready("sub")
        open_item(app, "sub")
        app.until("the folder sub", lambda: app.printed("AZDRIVE_LISTED", r"home sub/ \d+"))
        app.click(selector="#" + I("pick-ok"), frames=0)  # AzDrive answers and ends
        path = picked_path(app.answer(), "Select folder")
    finally:
        app.stop()
    if not same_path(path.rstrip("/"), os.path.join(home, "sub")):
        raise Failure("Select folder answered %s, not sub" % path)
    log("5. Select folder in sub answered %s" % path)

    # 6. A cloud object: the mock stack's bucket as a drive, its key in the run's keyring file.
    stack = azlin_mock_stack.start(os.path.join(logs, "s3"))
    try:
        bucket = os.path.join(logs, "s3", "e2e-cloud", "Notes")
        os.makedirs(bucket)
        body = b"the tusk ledger of the harbour\n"
        with open(os.path.join(bucket, "minutes.txt"), "wb") as f:
            f.write(body)
        os.makedirs(os.path.dirname(drives_file), exist_ok=True)
        with open(drives_file, "w", encoding="utf-8") as f:
            json.dump({"format": "azul-storage.drives", "version": 1, "drives": [{
                "id": "e2e-cloud", "name": "E2E Cloud",
                "location": {"kind": "s3", "endpoint": stack.s3_url,
                             "region": azlin_mock_stack.REGION, "bucket": "e2e-cloud",
                             "path_style": True}}]}, f)
        keyring_file = os.path.join(logs, "keyring.json")
        with open(keyring_file, "w", encoding="utf-8") as f:
            json.dump({"azul-storage/s3/e2e-cloud": json.dumps({
                "access_key_id": azlin_mock_stack.ACCESS_KEY,
                "secret_access_key": azlin_mock_stack.SECRET_KEY,
                "session_token": None})}, f)
        env = {"AZ_KEYRING_FILE": keyring_file}
        cloud = switches + ["--open", "E2E Cloud/Notes"]
        for mode in ("handle", "copy"):
            app = Picker("pick-cloud-%s" % mode, binary, cloud, port, logs, args.timeout,
                         {"v": 1, "op": "open", "cloud": mode, "app": "E2E"}, extra_env=env)
            try:
                app.ready("minutes.txt")
                select_item(app, "minutes.txt")
                app.click(selector="#" + I("pick-ok"), frames=0)  # AzDrive answers and ends
                answer = app.answer()
            finally:
                app.stop()
            items = answer.get("items") or []
            ref = (items[0].get("cloud") or {}) if items else {}
            if answer.get("status") != "selected" or (ref.get("drive_id"), ref.get("key"),
                                                      ref.get("size")) != (
                    "e2e-cloud", "Notes/minutes.txt", len(body)):
                raise Failure("the cloud %s answered %r" % (mode, answer))
            path = items[0].get("path")
            if mode == "handle" and path:
                raise Failure("a handle came with a path: %r" % answer)
            if mode == "copy":
                if not path or not os.path.realpath(path).startswith(
                        os.path.realpath(os.path.join(cache, "picked"))):
                    raise Failure("the copy is not under the cache's picked/: %r" % answer)
                with open(path, "rb") as f:
                    if f.read() != body:
                        raise Failure("the copy at %s does not hold the object's bytes" % path)
            log("6. the cloud object as a %s: %r" % (mode, items[0]))
    finally:
        stack.stop()
    log("PASS: registration, Open with a filter, Cancel, Save, Select folder, cloud handle and copy")
    return True


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8781)
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--keep-logs", action="store_true")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azdrive-pick-e2e-")
    ok = False
    try:
        ok = run(args, logs)
    except Failure as e:
        log("FAIL: %s" % e)
    finally:
        if ok and not args.keep_logs:
            shutil.rmtree(logs, ignore_errors=True)
        else:
            log("kept %s" % logs)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
