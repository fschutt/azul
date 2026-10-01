#!/usr/bin/env python3
"""AzDrive end to end, headless, on a temporary Home folder with the sample files.

Walks Explorer's main flows through azul's debug server and asserts on the node tree, the
node layout, AzDrive's stdout markers and the files on disk:

     1. This PC: the drive tiles, the ribbon's FILE / HOME / SHARE / VIEW / DRIVE;
     2. open the Home drive (double-click its tile);
     3. every layout of VIEW > Layout (the gallery), back to Details;
     4. sort: the Name header twice, then Size;
     5. into Documents; select (click, Ctrl+click, Shift+click), Ctrl+A, Escape;
     6. type-ahead ("r" selects report.md);
     7. F2: rename notes.txt to todo.txt in place (the file on disk), Ctrl+Z renames it back;
     8. Ctrl+Shift+N: a new folder (on disk), Escape keeps its name;
     9. Ctrl+C / Ctrl+V into the new folder, again: the conflict dialog, "Keep both files";
    10. Delete: into the trash folder (on disk), Ctrl+Z brings it back;
    11. Backspace (up), Alt+Left (back), Alt+Right (forward);
    12. the panes: Preview pane (a text and an image preview), Navigation pane off / on,
        Details pane off / on;
    13. Properties (Alt+Enter) in the in-window sheet, OK;
    14. FILE: the backstage with the Options, Escape;
    15. flora + dark: a screenshot.

Usage (from the azul repository, after building libazul with the debug server and AzDrive):

    python3 scripts/azdrive_e2e.py [--bin target/release/AzDrive] [--debug-port 8781]
        [--timeout 180] [--out /tmp/azdrive-shots] [--keep-logs]

Run it through the capped runner on a small machine:

    <scratchpad>/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/azdrive-e2e.log -- \\
        python3 scripts/azdrive_e2e.py --bin target/release/AzDrive

Every op that changes state is followed by `wait_frame`s and an `until` on what it must cause.
Every key_down has its key_up (the E2E key_up rule).
"""

import argparse
import base64
import glob
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
REPO = os.path.abspath(os.path.join(HERE, ".."))


def log(line):
    print("[azdrive-e2e] %s" % line, flush=True)


class Failure(Exception):
    pass


def repo_roots():
    roots = [REPO]
    try:
        common = subprocess.run(
            ["git", "-C", REPO, "rev-parse", "--path-format=absolute", "--git-common-dir"],
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


def tail(path, lines=40):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return "".join(f.readlines()[-lines:])
    except OSError:
        return "(no output)"


class App:
    """AzDrive under its debug server."""

    def __init__(self, argv, port, env, logs, deadline):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "azdrive.out")
        self.err_path = os.path.join(logs, "azdrive.err")
        self.process = subprocess.Popen(
            argv,
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

    def op(self, op, **params):
        """One op on the debug server; it answers once the app has processed it."""
        body = {"op": op}
        body.update(params)
        request = urllib.request.Request(
            "http://127.0.0.1:%d/" % self.port,
            data=json.dumps(body).encode("utf-8"),
            method="POST",
        )
        with urllib.request.urlopen(request, timeout=15) as response:
            return json.loads(response.read().decode("utf-8") or "{}")

    def must(self, op, **params):
        answer = self.op(op, **params)
        if isinstance(answer, dict) and answer.get("status") == "error":
            raise Failure("%s %s failed: %s" % (op, json.dumps(params), json.dumps(answer)[:240]))
        return answer

    def value(self, op, **params):
        answer = self.must(op, **params)
        data = answer.get("data") if isinstance(answer, dict) else None
        if isinstance(data, dict) and "value" in data:
            return data["value"]
        return data if data is not None else answer

    def frame(self, n=2):
        for _ in range(n):
            self.must("wait_frame")

    def hierarchy(self):
        return [d for d in dicts(self.op("get_node_hierarchy")) if "index" in d and "type" in d]

    def texts(self):
        return [n.get("text") for n in self.hierarchy() if n.get("text")]

    def shows(self, text):
        return any(text in t for t in self.texts())

    def classes(self):
        out = set()
        for n in self.hierarchy():
            for c in n.get("classes") or []:
                out.add(c)
        return out

    def nodes_with_class(self, cls):
        return [n["index"] for n in self.hierarchy() if cls in (n.get("classes") or [])]

    def exact(self, text):
        """The node holding the text node whose text is exactly `text` (the first one)."""
        for n in self.hierarchy():
            if n.get("text") == text:
                return n.get("parent", n["index"])
        return None

    def click_exact(self, text, button="left", double=False):
        node = self.until('the text "%s"' % text, lambda: self.exact(text))
        self.must("double_click" if double else "click", node_id=node, button=button)
        self.frame()

    def has(self, selector):
        answer = self.op("get_node_layout", selector=selector)
        if not isinstance(answer, dict) or answer.get("status") == "error":
            return False
        value = (answer.get("data") or {}).get("value") or {}
        rect = value.get("rect") or {}
        return rect.get("width", 0) > 0 and rect.get("height", 0) > 0

    def key(self, key, shift=False, ctrl=False, alt=False):
        mods = {"shift": shift, "ctrl": ctrl, "alt": alt, "meta": False}
        self.must("key_down", key=key, modifiers=mods)
        self.must("key_up", key=key, modifiers=mods)
        self.frame()

    def printed(self, key, pattern=r".*"):
        """Every `<KEY> <value>` line the app printed on stdout."""
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^%s (%s)$" % (re.escape(key), pattern), text, re.M)

    def count(self, key, pattern=r".*"):
        return len(self.printed(key, pattern))

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("AzDrive exited (%s) while waiting for %s" % (self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, KeyError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def after(self, what, key, pattern, action):
        """Runs `action`, then waits for a new `<KEY> <pattern>` line; returns the last value."""
        before = self.count(key, pattern)
        action()
        self.until(what, lambda: self.count(key, pattern) > before)
        return self.printed(key, pattern)[-1]

    def screenshot(self, path):
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure("take_screenshot returned no PNG: %s" % json.dumps(value)[:200])
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


# Explorer's layout keys: Ctrl+Shift+<digit>. (The ribbon's Layout gallery shows a strip that
# clips its later cells; the keys reach every layout.)
LAYOUTS = [
    ("1", "extra_large_icons"),
    ("2", "large_icons"),
    ("3", "medium_icons"),
    ("4", "small_icons"),
    ("5", "list"),
    ("7", "tiles"),
    ("8", "content"),
    ("6", "details"),
]


def item_names(app):
    """The names of the folder's items, in the order the view shows them."""
    names = []
    nodes = app.hierarchy()
    by_index = {n["index"]: n for n in nodes}
    for n in nodes:
        if "azdrive-name" in (n.get("classes") or []):
            if n.get("text"):
                names.append(n["text"])
                continue
            for child in n.get("children") or []:
                text = by_index.get(child, {}).get("text")
                if text:
                    names.append(text)
    return names


def run(args, logs):
    binary = find_binary(args.bin)
    log("AzDrive: %s" % binary)
    log("logs and data: %s" % logs)
    out = args.out or os.path.join(logs, "shots")
    os.makedirs(out, exist_ok=True)
    deadline = time.time() + args.timeout

    home = os.path.join(logs, "home")
    os.makedirs(home)
    env = dict(os.environ)
    env.update({
        "AZ_BACKEND": "headless",
        "AZ_DEBUG": str(args.debug_port),
        "AZDRIVE_HOME": home,
        "AZDRIVE_DOWNLOADS": os.path.join(logs, "downloads"),
        "AZDRIVE_SETTINGS": os.path.join(logs, "settings"),
        "AZUL_DRIVES": os.path.join(logs, "config", "drives.json"),
        "AZDRIVE_DIALOGS": "inline",
    })
    app = App([binary, "--sample", "--screen", "this-pc", "--theme", "flat", "--mode", "light"],
              args.debug_port, env, logs, deadline)
    docs = os.path.join(home, "Documents")
    try:
        # 1. This PC.
        app.until("the This PC view", lambda: app.printed("AZDRIVE_PLACE", r"this-pc"))
        app.until("the debug server", lambda: app.op("get_dom_tree"))
        app.must("resize", width=1280.0, height=800.0)
        app.frame(3)
        app.until("the drive tiles", lambda: app.nodes_with_class("azdrive-drive"))
        for tab in ("FILE", "HOME", "SHARE", "VIEW", "DRIVE"):
            app.until("the ribbon tab %s" % tab, lambda: app.exact(tab) is not None)
        app.until("This PC's groups", lambda: app.shows("Devices and drives"))
        if not app.has("#shell-tree") or not app.has("#shell-content"):
            raise Failure("the navigation pane and the content pane are not laid out")
        app.screenshot(os.path.join(out, "01-this-pc.png"))
        log("1. This PC: drive tiles, the five ribbon tabs, the navigation and content panes")

        # 2. The Home drive.
        tile = app.nodes_with_class("azdrive-drive")[0]
        listed = app.after("the Home drive's listing", "AZDRIVE_LISTED", r"home / \d+",
                           lambda: (app.must("double_click", node_id=tile), app.frame()))
        log("2. opened the Home drive (%s)" % listed)
        app.until("Documents in the view", lambda: "Documents" in item_names(app))
        if ".hidden-settings" in item_names(app):
            raise Failure("a hidden item shows while Hidden items is off")

        # 3. Every layout (Ctrl+Shift+1..8), and one through the ribbon's gallery.
        for digit, name in LAYOUTS:
            app.after("the layout %s" % name, "AZDRIVE_LAYOUT", re.escape(name),
                      lambda: app.key(digit, ctrl=True, shift=True))
            app.until("the %s view" % name, lambda: "azdrive-layout-%s" % name in app.classes())
            app.until("the items of %s" % name, lambda: len(app.nodes_with_class("azdrive-item")) >= 5)
            if name in ("large_icons", "tiles"):
                app.screenshot(os.path.join(out, "03-%s.png" % name))
        app.click_exact("VIEW")
        app.after("Large icons from the gallery", "AZDRIVE_LAYOUT", r"large_icons",
                  lambda: app.click_exact("Large icons"))
        app.after("Details from the status bar's switch", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.key("6", ctrl=True, shift=True))
        log("3. all eight layouts render the folder (keys and the ribbon's gallery)")

        # 4. Sort by the Name header (twice: descending), then by Size.
        app.until("the Details header", lambda: app.has("#details-header"))
        before = item_names(app)
        app.after("sort by name, descending", "AZDRIVE_SORT", r"Name desc",
                  lambda: app.click_exact("Name"))
        app.until("the reversed order",
                  lambda: [n for n in item_names(app)] == list(reversed(before)) or
                  item_names(app)[0] == sorted(before, key=str.lower)[-1])
        app.after("sort by name, ascending", "AZDRIVE_SORT", r"Name asc",
                  lambda: app.click_exact("Name"))
        app.after("sort by size", "AZDRIVE_SORT", r"Size asc", lambda: app.click_exact("Size"))
        app.after("sort by name again", "AZDRIVE_SORT", r"Name (asc|desc)",
                  lambda: app.click_exact("Name"))
        if app.printed("AZDRIVE_SORT", r"Name (asc|desc)")[-1] == "desc":
            app.after("sort by name, ascending", "AZDRIVE_SORT", r"Name asc",
                      lambda: app.click_exact("Name"))
        log("4. the Name and Size headers sort (a second click reverses)")

        # 5. Documents; selection.
        app.after("the Documents listing", "AZDRIVE_LISTED", r"home Documents/ \d+",
                  lambda: app.click_exact("Documents", double=True))
        app.until("notes.txt", lambda: "notes.txt" in item_names(app))
        app.after("one selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: app.click_exact("notes.txt"))
        app.after("Ctrl+click", "AZDRIVE_SELECTED", r"2 .*", lambda: (
            app.op("key_down", key="ctrl", modifiers={"ctrl": True}),
            app.click_exact("report.md"),
            app.op("key_up", key="ctrl", modifiers={"ctrl": False})))
        app.after("Shift+click", "AZDRIVE_SELECTED", r"3 .*", lambda: (
            app.op("key_down", key="shift", modifiers={"shift": True}),
            app.click_exact("data.csv"),
            app.op("key_up", key="shift", modifiers={"shift": False})))
        app.after("Escape", "AZDRIVE_SELECTED", r"0 -", lambda: app.key("escape"))
        app.after("Ctrl+A", "AZDRIVE_SELECTED", r"3 .*", lambda: app.key("a", ctrl=True))
        app.after("Escape", "AZDRIVE_SELECTED", r"0 -", lambda: app.key("escape"))
        log("5. click, Ctrl+click, Shift+click, Ctrl+A and Escape select as Explorer does")

        # 6. Type-ahead.
        app.after("type-ahead r", "AZDRIVE_SELECTED", r"1 Documents/report\.md",
                  lambda: app.key("r"))
        log("6. typing r selects report.md")

        # 7. F2: rename in place; Ctrl+Z.
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: app.click_exact("notes.txt"))
        app.after("the rename field", "AZDRIVE_RENAMING", r"Documents/notes\.txt",
                  lambda: app.key("f2"))
        app.until("the rename field laid out", lambda: app.has("#rename-field"))
        app.must("focus_node", selector="#rename-field")
        app.frame()
        app.key("end")
        for _ in range(len("notes.txt")):
            app.key("backspace")
        app.must("text_input", text="todo.txt")
        app.frame()
        app.after("the rename", "AZDRIVE_DONE", r"renamed Documents/todo\.txt",
                  lambda: app.key("enter"))
        app.until("todo.txt on disk", lambda: os.path.isfile(os.path.join(docs, "todo.txt")))
        if os.path.exists(os.path.join(docs, "notes.txt")):
            raise Failure("notes.txt is still there after the rename")
        app.key("z", ctrl=True)
        app.until("notes.txt back on disk (Ctrl+Z)",
                  lambda: os.path.isfile(os.path.join(docs, "notes.txt")))
        log("7. F2 renamed notes.txt to todo.txt on disk; Ctrl+Z renamed it back")

        # 8. A new folder.
        app.after("a new folder", "AZDRIVE_DONE", r"created Documents/New folder/",
                  lambda: app.key("n", ctrl=True, shift=True))
        app.until("the new folder on disk", lambda: os.path.isdir(os.path.join(docs, "New folder")))
        app.until("its rename field", lambda: app.has("#rename-field"))
        app.key("escape")
        app.until("the field gone", lambda: not app.has("#rename-field"))
        log("8. Ctrl+Shift+N made New folder on disk; Escape kept its name")

        # 9. Copy / paste; a conflict; Keep both.
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: app.click_exact("notes.txt"))
        app.after("Ctrl+C", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.key("c", ctrl=True))
        app.after("into New folder", "AZDRIVE_LISTED", r"home Documents/New folder/ \d+",
                  lambda: app.click_exact("New folder", double=True))
        target = os.path.join(docs, "New folder")
        app.after("the paste", "AZDRIVE_TRANSFER", r"\d+ done 1", lambda: app.key("v", ctrl=True))
        app.until("the copy on disk", lambda: os.path.isfile(os.path.join(target, "notes.txt")))
        app.after("the conflict", "AZDRIVE_TRANSFER", r"\d+ conflict 1",
                  lambda: app.key("v", ctrl=True))
        app.until("the conflict dialog", lambda: app.has("#conflict"))
        app.screenshot(os.path.join(out, "09-conflict.png"))
        app.after("keep both", "AZDRIVE_TRANSFER", r"\d+ done 1",
                  lambda: (app.must("click", selector="#conflict-keep-both"), app.frame()))
        app.until("notes (2).txt on disk", lambda: os.path.isfile(os.path.join(target, "notes (2).txt")))
        log("9. Ctrl+C / Ctrl+V copied into New folder; a second paste asked, Keep both made notes (2).txt")

        # 10. Delete into the trash; Ctrl+Z.
        app.after("notes (2).txt selected", "AZDRIVE_SELECTED", r"1 .*notes \(2\)\.txt",
                  lambda: app.click_exact("notes (2).txt"))
        app.after("the delete", "AZDRIVE_DELETED", r"1", lambda: app.key("delete"))
        app.until("notes (2).txt gone", lambda: not os.path.exists(os.path.join(target, "notes (2).txt")))
        trashed = glob.glob(os.path.join(home, ".azdrive-trash", "*", "Documents", "New folder", "notes (2).txt"))
        if not trashed:
            raise Failure("the deleted file is not in the trash folder")
        app.key("z", ctrl=True)
        app.until("notes (2).txt back (Ctrl+Z)",
                  lambda: os.path.isfile(os.path.join(target, "notes (2).txt")))
        log("10. Delete moved the file into .azdrive-trash; Ctrl+Z brought it back")

        # 11. Up, Back, Forward.
        app.after("Backspace (up)", "AZDRIVE_PLACE", r"home Documents/", lambda: app.key("backspace"))
        app.after("Alt+Left (back)", "AZDRIVE_PLACE", r"home Documents/New folder/",
                  lambda: app.key("left", alt=True))
        app.after("Alt+Right (forward)", "AZDRIVE_PLACE", r"home Documents/",
                  lambda: app.key("right", alt=True))
        log("11. Backspace went up, Alt+Left back, Alt+Right forward")

        # 12. The panes.
        app.click_exact("VIEW")
        app.after("the preview pane", "AZDRIVE_PANES", r"true true true",
                  lambda: app.click_exact("Preview pane"))
        app.until("the preview pane laid out", lambda: app.has("#shell-preview"))
        app.after("a text preview", "AZDRIVE_PREVIEW", r"text Documents/notes\.txt",
                  lambda: app.click_exact("notes.txt"))
        app.until("the text in the preview", lambda: app.has("#preview-text"))
        app.after("up to Home", "AZDRIVE_PLACE", r"home /", lambda: app.key("backspace"))
        app.after("Pictures", "AZDRIVE_LISTED", r"home Pictures/ \d+",
                  lambda: app.click_exact("Pictures", double=True))
        app.after("an image preview", "AZDRIVE_PREVIEW", r"image Pictures/gradient\.png",
                  lambda: app.click_exact("gradient.png"))
        app.until("the image in the preview", lambda: app.has("#preview-image"))
        app.screenshot(os.path.join(out, "12-preview.png"))
        app.after("the navigation pane off", "AZDRIVE_PANES", r"false true true",
                  lambda: app.click_exact("Navigation pane"))
        app.until("no tree", lambda: not app.has("#shell-tree"))
        app.after("the navigation pane on", "AZDRIVE_PANES", r"true true true",
                  lambda: app.click_exact("Navigation pane"))
        app.until("the tree back", lambda: app.has("#shell-tree"))
        app.after("the details pane off", "AZDRIVE_PANES", r"true true false",
                  lambda: app.click_exact("Details pane"))
        app.until("no details pane", lambda: not app.has("#shell-details"))
        app.after("the details pane on", "AZDRIVE_PANES", r"true true true",
                  lambda: app.click_exact("Details pane"))
        app.until("the details pane back", lambda: app.has("#shell-details"))
        log("12. Preview pane (text and image), Navigation pane and Details pane toggle")

        # 13. Properties.
        app.after("gradient.png selected", "AZDRIVE_SELECTED", r"1 Pictures/gradient\.png",
                  lambda: app.click_exact("gradient.png"))
        app.after("Properties", "AZDRIVE_DONE", r"properties 1", lambda: app.key("enter", alt=True))
        app.until("the Properties sheet", lambda: app.has("#properties"))
        app.until("its title", lambda: app.shows("gradient.png Properties"))
        app.screenshot(os.path.join(out, "13-properties.png"))
        app.click_exact("OK")
        app.until("the sheet closed", lambda: not app.has("#properties"))
        log("13. Alt+Enter opened Properties; OK closed it")

        # 14. FILE: the backstage and the Options.
        app.click_exact("FILE")
        app.until("the Options", lambda: app.has("#settings"))
        app.screenshot(os.path.join(out, "14-options.png"))
        app.key("escape")
        app.until("the backstage closed", lambda: not app.has("#settings"))
        log("14. FILE opened the backstage with the Options; Escape closed it")

        # 15. Flora, dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(4)
        app.screenshot(os.path.join(out, "15-flora-dark.png"))
        log("PASS: AzDrive browsed, laid out, sorted, selected, renamed, created, copied, "
            "resolved a conflict, deleted and undid, walked the history, toggled the panes, "
            "showed Properties and the Options")
        return True
    except Failure:
        for name, path in (("stdout", app.out_path), ("stderr", app.err_path)):
            print("\n----- azdrive %s (tail) -----\n%s" % (name, tail(path)))
        raise
    finally:
        app.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8781)
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--out")
    parser.add_argument("--keep-logs", action="store_true")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azdrive-e2e-")
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
