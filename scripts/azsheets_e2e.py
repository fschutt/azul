#!/usr/bin/env python3
"""AzSheets end to end over the debug server.

    1. starts AzSheets headless (AZ_BACKEND=headless, the debug server on --debug-port) with a
       fresh data folder (AZSHEETS_DATA), sizes the window;
    2. focuses the grid and TYPES: 4 Enter 5 Enter =SUM(A1:A2) Enter - checks the app's
       `AZSHEETS_CELL A3 9` line (the engine evaluated the formula) and the grid's text;
    3. selects A1:A3 with Shift+Up and checks the status bar's sum (`AZSHEETS_STATS ... sum=18`
       and "Sum: 18" in the tree);
    4. types 3 / 1 / 2 into C1:C3, selects C1:C3 with the grid's keys (Ctrl+Home, Right,
       Shift+Down), clicks "Sort A to Z" and checks C1 is 1;
    5. goes to B2, VIEW > Freeze Panes - checks `AZSHEETS_FROZEN 1 1` and the freeze line in
       the tree;
    6. saves with Ctrl+S - checks `AZSHEETS_SAVED <id>` and sheets/<id>.xlsx + .json on disk;
    7. stops the app, starts it again on the backstage's Open pane, opens the workbook and
       checks A3 is still 9, C1 is 1 and the panes are still frozen;
    8. screenshots: the workbook in flat light, then flora dark.

Usage (from the azul repository, after building libazul with the debug server and AzSheets):

    python3 scripts/azsheets_e2e.py [--bin target/release/AzSheets] [--debug-port 8772]
        [--timeout 180] [--width 1280] [--height 800] [--out <dir>]

`AZSHEETS_BIN` also names the binary. Run it through the capped runner on the 8 GB Mac:

    <scratchpad>/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/azsheets.log -- \\
        env DYLD_LIBRARY_PATH=$PWD/target/azul-lib python3 scripts/azsheets_e2e.py \\
        --bin target/release/AzSheets

Never run while another app listens on the same port.
"""

import argparse
import base64
import json
import os
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))


def log(line):
    print("[azsheets] %s" % line, flush=True)


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
    if os.environ.get("AZSHEETS_BIN"):
        candidates.append(os.environ["AZSHEETS_BIN"])
    for root in repo_roots():
        for sub in ("release", "debug"):
            candidates.append(os.path.join(root, "target", sub, "AzSheets"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no AzSheets binary; pass --bin or set AZSHEETS_BIN (tried %s)" % candidates)


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
    """AzSheets under its debug server."""

    def __init__(self, binary, port, env, logs, deadline, args, name):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "%s.stdout" % name)
        self.err_path = os.path.join(logs, "%s.stderr" % name)
        self.process = subprocess.Popen(
            [binary] + args, env=env, stdin=subprocess.DEVNULL,
            stdout=open(self.out_path, "wb"), stderr=open(self.err_path, "wb"),
        )

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
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

    def has_node(self, selector):
        try:
            value = self.value("get_node_layout", selector=selector)
        except Failure:
            return False
        return isinstance(value, dict) and value.get("node_id") is not None

    def frame(self, n=1):
        for _ in range(n):
            self.must("wait_frame")

    def texts(self):
        return list(strings(self.op("get_node_hierarchy")))

    def classes(self):
        out = set()
        for d in dicts(self.op("get_node_hierarchy")):
            for c in d.get("classes") or []:
                out.add(c)
        return out

    def stdout(self):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                return f.read()
        except OSError:
            return ""

    def printed(self, key, pattern=r".*"):
        return re.findall(r"^%s (%s)$" % (re.escape(key), pattern), self.stdout(), re.M)

    def count(self, key):
        return len(self.printed(key))

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("AzSheets exited (%s) while waiting for %s" % (self.process.returncode, what))
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

    def type_text(self, text):
        self.must("text_input", text=text)
        self.frame(1)

    def replies(self):
        return self.count("AZSHEETS_REPLY")

    def settle(self, before, what):
        """Waits until a reply after `before` replies arrived."""
        self.until(what, lambda: self.replies() > before)
        self.frame(2)

    def cell_line(self, a1):
        """The last `AZSHEETS_CELL <a1> <text>` text, or None."""
        found = self.printed("AZSHEETS_CELL", r"%s .*|%s" % (re.escape(a1), re.escape(a1)))
        if not found:
            return None
        return found[-1][len(a1):].strip()

    def focus_grid(self):
        self.must("focus_node", selector="#cell-grid")
        self.frame(1)

    def screenshot(self, path):
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure("take_screenshot returned no PNG: %s" % json.dumps(value)[:200])
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))

    def home(self, right=0, down=0, shift_down=0):
        """Ctrl+Home, then arrows: the grid's own keyboard navigation."""
        self.focus_grid()
        self.key("home", ctrl=True)
        for _ in range(right):
            self.key("right")
        for _ in range(down):
            self.key("down")
        for _ in range(shift_down):
            self.key("down", shift=True)
        self.frame(2)


def start(binary, port, data_dir, logs, deadline, args, name, width, height):
    env = dict(os.environ)
    env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(port), "AZSHEETS_DATA": data_dir})
    app = App(binary, port, env, logs, deadline, args + ["--size", "%dx%d" % (width, height)], name)
    app.until("AzSheets to be ready", lambda: app.count("AZSHEETS_READY") > 0 and app.op("get_node_hierarchy"))
    app.must("resize", width=width, height=height)
    app.frame(3)
    return app


def first_session(binary, args, data_dir, logs, out):
    deadline = time.time() + args.timeout
    app = start(binary, args.debug_port, data_dir, logs, deadline, [], "first", args.width, args.height)
    try:
        app.until("the grid in the tree", lambda: app.has_node("#cell-grid"))
        app.until("the first snapshot", lambda: app.replies() > 0)

        # 2. Type numbers and a SUM.
        app.focus_grid()
        for text in ("4", "5", "=SUM(A1:A2)"):
            before = app.replies()
            # Re-focus each time: a rebuild that loses the focus must not
            # send the next keys nowhere (the E2E checks the grid, not that).
            app.focus_grid()
            app.type_text(text)
            app.key("return")
            app.settle(before, "the engine to take %s" % text)
        app.key("up")
        app.until("A3 to show 9", lambda: app.cell_line("A3") == "9")
        if not any(t == "9" for t in app.texts()):
            raise Failure("the grid does not show 9: %s" % [t for t in app.texts() if t][:40])
        log("typed 4, 5, =SUM(A1:A2): A3 shows 9")

        # 3. The status bar's sum of A1:A3.
        before = app.replies()
        app.focus_grid()
        app.key("up", shift=True)
        app.key("up", shift=True)
        app.settle(before, "the selection's statistics")
        app.until("the sum of A1:A3", lambda: any(s.startswith("count=3 sum=18") for s in app.printed("AZSHEETS_STATS")))
        app.until("'Sum: 18' in the status bar", lambda: any("Sum: 18" in t for t in app.texts()))
        log("status bar: Sum: 18")

        # 4. Sort a range.
        app.home(right=2)
        for text in ("3", "1", "2"):
            before = app.replies()
            # Re-focus each time: a rebuild that loses the focus must not
            # send the next keys nowhere (the E2E checks the grid, not that).
            app.focus_grid()
            app.type_text(text)
            app.key("return")
            app.settle(before, "the engine to take %s" % text)
        app.home(right=2, shift_down=2)
        before = app.replies()
        app.must("click", text="Sort A to Z")
        app.settle(before, "the sort")
        app.home(right=2)
        app.until("C1 to be 1 after the sort", lambda: app.cell_line("C1") == "1")
        log("sorted C1:C3: C1 is 1")

        # 5. Freeze a pane at B2.
        app.home(right=1, down=1)
        app.must("click", text="VIEW")
        app.frame(2)
        before = app.replies()
        app.must("click", text="Freeze Panes")
        app.settle(before, "the freeze")
        app.until("the panes frozen at B2", lambda: "1 1" in app.printed("AZSHEETS_FROZEN"))
        app.until("the freeze line", lambda: "__azul-native-cell-grid-freeze" in app.classes())
        log("froze the panes at B2")
        app.screenshot(os.path.join(out, "workbook-flat-light.png"))

        # 6. Save.
        app.focus_grid()
        app.key("s", ctrl=True)
        saved = app.until("the save", lambda: app.printed("AZSHEETS_SAVED", r"[0-9a-f-]+"))
        doc_id = saved[-1]
        for ext in ("xlsx", "json"):
            path = os.path.join(data_dir, "sheets", "%s.%s" % (doc_id, ext))
            if not os.path.isfile(path) or os.path.getsize(path) == 0:
                raise Failure("the save wrote no %s" % path)
        log("saved sheets/%s.xlsx and .json" % doc_id)

        # 8a. Flora dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.screenshot(os.path.join(out, "workbook-flora-dark.png"))
        return doc_id
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (tail(app.out_path), tail(app.err_path)))
        raise
    finally:
        app.stop()


def second_session(binary, args, data_dir, logs, out, doc_id):
    deadline = time.time() + args.timeout
    app = start(binary, args.debug_port, data_dir, logs, deadline,
                ["--screen", "backstage-open"], "second", args.width, args.height)
    try:
        app.until("the workbook in the Open list", lambda: app.has_node("#open-0"))
        app.must("click", selector="#open-0")
        app.until("the workbook to open", lambda: doc_id in app.printed("AZSHEETS_OPENED", r"[0-9a-f-]+"))
        app.frame(3)
        app.until("the panes still frozen", lambda: app.printed("AZSHEETS_FROZEN") and app.printed("AZSHEETS_FROZEN")[-1] == "1 1")
        app.home(down=2)
        app.until("A3 still 9", lambda: app.cell_line("A3") == "9")
        app.home(right=2)
        app.until("C1 still 1", lambda: app.cell_line("C1") == "1")
        app.screenshot(os.path.join(out, "reopened.png"))
        log("reopened %s: A3 = 9, C1 = 1, panes frozen" % doc_id)
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (tail(app.out_path), tail(app.err_path)))
        raise
    finally:
        app.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8772)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--out")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azsheets-e2e-")
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    passed = False
    try:
        binary = find_binary(args.bin)
        doc_id = first_session(binary, args, data_dir, logs, out)
        second_session(binary, args, data_dir, logs, out, doc_id)
        passed = True
        log("PASS (logs and screenshots in %s)" % logs)
    except Failure as e:
        log("FAIL: %s (logs in %s)" % (e, logs))
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
