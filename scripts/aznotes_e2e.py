#!/usr/bin/env python3
"""AzNotes end to end over the debug server (headless).

    1. starts AzNotes on an empty, temporary notes folder (--data) and checks the empty state;
    2. creates a note (the "New note" button), types its title, then its body WITH MARKDOWN
       SHORTCUTS - "# " (heading), "- " (bullets), Enter on an empty item (out of the list),
       "[ ] " (a check item) - adds a tag (the tag field, Enter) and pins it;
    3. waits for the autosave and reads the note's FILE back from disk:
       notes/Notes/<id>.md with its front matter (title, tags, pinned) and Markdown body;
    4. searches the list ("bakery" finds the note, "bakeryzz" finds nothing);
    5. opens the command palette (Ctrl+K) and closes it (Escape); exports the note as PDF
       (save_bytes mocked) and checks the app reports the bytes;
    6. closes the window (pending saves first), starts AzNotes again on the same folder and
       checks the note, its heading and its tag are read back from the file;
    7. takes screenshots: flat light, flora dark, the settings screen.

Usage (from the azul repository, after building libazul with the debug server and AzNotes):

    python3 scripts/aznotes_e2e.py [--bin target/release/AzNotes] [--debug-port 8773]
        [--timeout 180] [--out <dir>] [--keep]

`AZNOTES_BIN` also names the binary. Run it through the capped runner on a small machine.
Never run while an app is running on the same port.
"""

import argparse
import base64
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


def log(line):
    print("[aznotes] %s" % line, flush=True)


class Failure(Exception):
    pass


def repo_roots():
    repo = os.path.abspath(os.path.join(HERE, ".."))
    roots = [repo]
    try:
        common = subprocess.run(
            ["git", "-C", repo, "rev-parse", "--path-format=absolute", "--git-common-dir"],
            check=True, capture_output=True, text=True,
        ).stdout.strip()
        main = os.path.dirname(common)
        if main and main not in roots:
            roots.append(main)
    except (OSError, subprocess.CalledProcessError):
        pass
    return roots


def find_binary(explicit):
    candidates = []
    if explicit:
        candidates.append(explicit)
    if os.environ.get("AZNOTES_BIN"):
        candidates.append(os.environ["AZNOTES_BIN"])
    for root in repo_roots():
        for sub in ("release", "debug", "consumer/release", "consumer/debug"):
            candidates.append(os.path.join(root, "target", sub, "AzNotes"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no AzNotes binary; pass --bin or set AZNOTES_BIN (tried %s)" % candidates)


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
    """AzNotes under its debug server, on the notes folder `data`."""

    def __init__(self, binary, port, data, logs, deadline, run):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "aznotes-%d.stdout" % run)
        self.err_path = os.path.join(logs, "aznotes-%d.stderr" % run)
        env = dict(os.environ)
        env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(port)})
        self.process = subprocess.Popen(
            [binary, "--data", data, "--size", "1200x760"], env=env, stdin=subprocess.DEVNULL,
            stdout=open(self.out_path, "wb"), stderr=open(self.err_path, "wb"),
        )

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()

    def op(self, op, **params):
        body = {"op": op}
        body.update(params)
        request = urllib.request.Request(
            "http://127.0.0.1:%d/" % self.port,
            data=json.dumps(body).encode("utf-8"), method="POST",
        )
        with urllib.request.urlopen(request, timeout=15) as response:
            return json.loads(response.read().decode("utf-8") or "{}")

    def must(self, op, **params):
        answer = self.op(op, **params)
        if isinstance(answer, dict) and answer.get("status") == "error":
            raise Failure("%s %s failed: %s" % (op, json.dumps(params), json.dumps(answer)[:300]))
        return answer

    def value(self, op, **params):
        answer = self.must(op, **params)
        data = answer.get("data") if isinstance(answer, dict) else None
        if isinstance(data, dict) and "value" in data:
            return data["value"]
        return data if data is not None else answer

    def frame(self, n=1):
        for _ in range(n):
            self.must("wait_frame")

    def texts(self):
        return list(strings(self.op("get_node_hierarchy")))

    def shows(self, text):
        return any(text in t for t in self.texts())

    def classes(self):
        out = set()
        for d in dicts(self.op("get_node_hierarchy")):
            for c in d.get("classes") or []:
                out.add(c)
        return out

    def printed(self, key, pattern=r".+"):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^%s (%s)$" % (re.escape(key), pattern), text, re.M)

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("AzNotes exited (%s) while waiting for %s" % (self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def key(self, key, shift=False, ctrl=False, meta=False):
        mods = {"shift": shift, "ctrl": ctrl, "alt": False, "meta": meta}
        self.must("key_down", key=key, modifiers=mods)
        self.must("key_up", key=key, modifiers=mods)
        self.frame(2)

    def type(self, text):
        self.must("text_input", text=text)
        self.frame(2)

    def click(self, selector):
        self.must("click", selector=selector)
        self.frame(2)

    def type_into(self, selector, text):
        """Types into a text field (AzMail's / AzCalendar's way: focus the field's root by
        its id, let a redraw settle, focus again, then type)."""
        self.must("focus_node", selector=selector)
        self.frame(1)
        time.sleep(0.2)
        self.must("focus_node", selector=selector)
        self.frame(1)
        self.type(text)

    def screenshot(self, path):
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure("take_screenshot returned no PNG: %s" % json.dumps(value)[:200])
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


def read_note(data, notebook, note_id):
    path = os.path.join(data, "notes", notebook, "%s.md" % note_id)
    try:
        with open(path, "r", encoding="utf-8") as f:
            return f.read()
    except OSError:
        return ""


def expect_in(text, needles, what):
    missing = [n for n in needles if n not in text]
    if missing:
        raise Failure("%s lacks %s:\n%s" % (what, missing, text))


def first_session(app, data, out):
    app.until("the empty library to load", lambda: app.printed("AZNOTES_LOADED", r"\d+"))
    app.must("resize", width=1200, height=760)
    app.frame(2)
    if not app.shows("No notes yet"):
        raise Failure("an empty folder shows no 'No notes yet' empty state")
    log("empty state shown")

    # A new note, its title.
    app.click("#new-note")
    note_id = app.until("the new note", lambda: (app.printed("AZNOTES_NEW", r"[0-9a-f-]+") or [None])[-1])
    app.until("the new note to open", lambda: note_id in app.printed("AZNOTES_OPEN", r"[0-9a-f-]+"))
    log("new note %s" % note_id)
    app.type_into("#note-title", "Shopping list")

    # The body, with Markdown shortcuts.
    app.must("focus_node", selector="#note-body")
    app.frame(2)
    app.click("#nb-0")  # the caret into the note's first (empty) block
    app.type("# ")
    app.type("Groceries")
    app.key("return")
    app.type("- ")
    app.type("milk")
    app.key("return")
    app.type("eggs")
    app.key("return")
    app.key("return")  # an empty item leaves the list
    app.type("[ ] ")
    app.type("call the bakery")

    # A tag, the pin.
    app.type_into("#tag-input", "errands")
    app.key("return")
    app.until("the tag chip", lambda: app.shows("#errands"))
    app.click("#pin-note")
    app.until("the note counted as pinned", lambda: app.shows("Pinned (1)"))
    log("typed, tagged, pinned")

    # The autosave writes the file.
    def saved():
        text = read_note(data, "Notes", note_id)
        return text if "call the bakery" in text and "pinned: true" in text else None

    text = app.until("the note's file on disk", saved)
    expect_in(
        text,
        ["title: Shopping list", "tags: [errands]", "pinned: true", "# Groceries", "- milk", "- eggs",
         "- [ ] call the bakery"],
        "notes/Notes/%s.md" % note_id,
    )
    log("file on disk:\n%s" % text)

    # Search as you type.
    app.click(".__azul-native-message-list-search")
    app.type("bakery")
    app.frame(2)
    if app.shows("Nothing matches"):
        raise Failure("searching 'bakery' finds nothing")
    app.type("zz")
    app.until("the search to find nothing", lambda: app.shows("Nothing matches"))
    log("search finds and filters")

    # The command palette.
    app.key("k", ctrl=True)
    app.until("the command palette", lambda: "__azul-native-command-palette-panel" in app.classes())
    app.screenshot(os.path.join(out, "palette.png"))
    app.key("escape")
    app.until("the palette to close", lambda: "__azul-native-command-palette-panel" not in app.classes())

    # Export as PDF (the save dialog answers yes).
    app.must("mock", set={"save_bytes": {"accept": True}})
    before = len(app.printed("AZNOTES_EXPORTED", r"pdf \d+"))
    app.click("#export-pdf")
    app.until("the PDF export", lambda: len(app.printed("AZNOTES_EXPORTED", r"pdf \d+")) > before)
    log("exported %s" % app.printed("AZNOTES_EXPORTED", r"pdf \d+")[-1])

    # Looks.
    app.screenshot(os.path.join(out, "flat-light.png"))
    app.must("set_theme", theme="flora")
    app.must("set_mode", mode="dark")
    app.frame(3)
    app.screenshot(os.path.join(out, "flora-dark.png"))
    app.must("set_theme", theme="flat")
    app.must("set_mode", mode="light")
    app.click("#open-settings")
    app.until("the settings", lambda: app.shows("Keyboard shortcuts"))
    app.screenshot(os.path.join(out, "settings.png"))
    app.key("escape")
    return note_id


def second_session(app, note_id):
    app.until("the library to load again", lambda: app.printed("AZNOTES_LOADED", r"\d+"))
    loaded = int(app.printed("AZNOTES_LOADED", r"\d+")[-1])
    if loaded != 1:
        raise Failure("the second start read %d notes, not 1" % loaded)
    app.frame(2)
    for text in ("Shopping list", "Groceries", "#errands", "call the bakery"):
        app.until("'%s' after the restart" % text, lambda t=text: app.shows(t))
    if note_id not in app.printed("AZNOTES_OPEN", r"[0-9a-f-]+"):
        raise Failure("the note was not opened after the restart")
    log("the restart reads the note back from its file")


def run(args, logs, out, data):
    binary = find_binary(args.bin)
    deadline = time.time() + args.timeout
    app = App(binary, args.debug_port, data, logs, deadline, 1)
    try:
        note_id = first_session(app, data, out)
        # Close: pending saves run first, then the window closes.
        app.op("close")
        end = time.time() + 20
        while app.process.poll() is None and time.time() < end:
            time.sleep(0.25)
        if app.process.poll() is None:
            log("the window did not close by itself; stopping it")
    except Failure as e:
        log("FAIL: %s" % e)
        print("---- stdout ----\n%s---- stderr ----\n%s" % (tail(app.out_path), tail(app.err_path)))
        raise
    finally:
        app.stop()

    app = App(binary, args.debug_port, data, logs, deadline, 2)
    try:
        second_session(app, note_id)
        app.screenshot(os.path.join(out, "restart.png"))
        log("PASS: screenshots in %s" % out)
        return True
    except Failure as e:
        log("FAIL: %s" % e)
        print("---- stdout ----\n%s---- stderr ----\n%s" % (tail(app.out_path), tail(app.err_path)))
        raise
    finally:
        app.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8773)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--out")
    parser.add_argument("--keep", action="store_true", help="keep the notes folder and the logs")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="aznotes-e2e-")
    data = os.path.join(logs, "data")
    os.makedirs(data, exist_ok=True)
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    passed = False
    try:
        passed = run(args, logs, out, data)
    except Failure:
        passed = False
    finally:
        log("logs, notes folder and screenshots in %s" % logs)
        if passed and not args.keep and not args.out:
            shutil.rmtree(data, ignore_errors=True)
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
