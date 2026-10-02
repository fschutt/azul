#!/usr/bin/env python3
"""AzShells end to end: every shell over the debug server.

    1. starts AzShells headless (AZ_BACKEND=headless, the debug server on --debug-port) and
       sizes the window to --width x --height;
    2. for each of S1..S11 (and the settings layout): clicks its picker segment, reads the
       slot ids the app prints (`AZSHELLS_SLOTS <n> id,id,...`) and the F6 cycle
       (`AZSHELLS_PANES <n> id,id,...`), asks `get_node_layout` for every slot and asserts the
       rectangle lies inside the window, asserts the row's panes do not overlap, presses F6
       (the first pane takes the focus), F6 (the second), Shift+F6 (back to the first) and
       checks `get_focus_state` against the pane nodes, then takes a screenshot;
    3. opens the command palette with Ctrl+K on S4, asserts it is in the tree, closes it with
       Escape;
    4. switches the app theme to flora and the mode to dark, takes S4 again, and restores.

Usage (from the azul repository, after building libazul with the debug server and AzShells):

    python3 examples/azul-shells/scripts/shells_e2e.py [--bin target/release/AzShells]
        [--debug-port 8771] [--timeout 120] [--width 1100] [--height 720] [--out <dir>]
        [--keep-logs]

`AZSHELLS_BIN` also names the binary. Screenshots go to --out (default: a temporary folder
printed at the end). Never run while an app is running on the same port.
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
PICKS = ["S1", "S2", "S3", "S4", "S5", "S6", "S7", "S8", "S9", "S10", "S11", "Settings"]


def log(line):
    print("[shells] %s" % line, flush=True)


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
        if main and main not in roots:
            roots.append(main)
    except (OSError, subprocess.CalledProcessError):
        pass
    return roots


def find_binary(explicit):
    candidates = []
    if explicit:
        candidates.append(explicit)
    if os.environ.get("AZSHELLS_BIN"):
        candidates.append(os.environ["AZSHELLS_BIN"])
    for root in repo_roots():
        for sub in ("release", "debug", "consumer/release", "consumer/debug"):
            candidates.append(os.path.join(root, "target", sub, "AzShells"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no AzShells binary; pass --bin or set AZSHELLS_BIN (tried %s)" % candidates)


class App:
    """AzShells under its debug server."""

    def __init__(self, binary, port, env, logs, deadline):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "azshells.stdout")
        self.err_path = os.path.join(logs, "azshells.stderr")
        self.process = subprocess.Popen(
            [binary], env=env, stdin=subprocess.DEVNULL,
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
            raise Failure("%s %s failed: %s" % (op, json.dumps(params), json.dumps(answer)[:200]))
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
        answer = self.op("get_node_hierarchy")
        out = set()
        for d in dicts(answer):
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
                raise Failure("AzShells exited (%s) while waiting for %s" % (self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def rect(self, node_id):
        """(node id, rect dict) of the node with the DOM id `node_id`."""
        value = self.value("get_node_layout", selector="#%s" % node_id)
        rect = value.get("rect") or {}
        return value.get("node_id"), rect

    def focused(self):
        value = self.value("get_focus_state", seat=0)
        node = value.get("focused_node") or {}
        return node.get("node_id"), node.get("selector") or ""

    def key(self, key, shift=False, ctrl=False, meta=False, primary=False):
        # `primary`: the platform's shortcut modifier, as the apps read it
        # (KeyModifiers::primary_down) - Cmd on macOS, Ctrl elsewhere.
        if primary:
            if sys.platform == "darwin":
                meta = True
            else:
                ctrl = True
        mods = {"shift": shift, "ctrl": ctrl, "alt": False, "meta": meta}
        self.must("key_down", key=key, modifiers=mods)
        self.must("key_up", key=key, modifiers=mods)
        self.frame(2)

    def screenshot(self, path):
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure("take_screenshot returned no PNG: %s" % json.dumps(value)[:200])
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


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


def inside(rect, width, height):
    x, y = float(rect.get("x", -1)), float(rect.get("y", -1))
    w, h = float(rect.get("width", 0)), float(rect.get("height", 0))
    return w > 0 and h > 0 and x >= -0.5 and y >= -0.5 and x + w <= width + 0.5 and y + h <= height + 0.5


def overlap(a, b):
    ax, ay, aw, ah = (float(a.get(k, 0)) for k in ("x", "y", "width", "height"))
    bx, by, bw, bh = (float(b.get(k, 0)) for k in ("x", "y", "width", "height"))
    dx = min(ax + aw, bx + bw) - max(ax, bx)
    dy = min(ay + ah, by + bh) - max(ay, by)
    return dx > 1.0 and dy > 1.0


def check_shell(app, n, pick, width, height, out):
    """Picks shell `pick` (index n, 1-based) and checks its slots, panes and F6."""
    before = len(app.printed("AZSHELLS_SHELL", r"\d+"))
    app.must("click", text=pick)
    app.until("the switch to %s" % pick,
              lambda: len(app.printed("AZSHELLS_SHELL", r"\d+")) > before)
    app.frame(2)
    slots_lines = app.printed("AZSHELLS_SLOTS", r"%d [^\n]+" % n)
    panes_lines = app.printed("AZSHELLS_PANES", r"%d [^\n]*" % n)
    if not slots_lines:
        raise Failure("%s: the app printed no AZSHELLS_SLOTS line" % pick)
    slots = slots_lines[-1].split(" ", 1)[1].split(",")
    panes = panes_lines[-1].split(" ", 1)[1].split(",") if panes_lines else []
    panes = [p for p in panes if p]
    log("%s: slots %s, F6 cycle %s" % (pick, slots, panes))

    rects = {}
    for slot in slots:
        node, rect = app.rect(slot)
        if node is None:
            raise Failure("%s: slot #%s is not in the tree" % (pick, slot))
        if not inside(rect, width, height):
            raise Failure("%s: slot #%s lies outside the %dx%d window: %s" % (pick, slot, width, height, rect))
        rects[slot] = (node, rect)

    # The row's panes (the F6 cycle minus the bottom pane) never overlap.
    row = [p for p in panes if p in rects]
    for i, a in enumerate(row):
        for b in row[i + 1:]:
            if overlap(rects[a][1], rects[b][1]):
                raise Failure("%s: panes #%s and #%s overlap: %s / %s" % (pick, a, b, rects[a][1], rects[b][1]))

    # F6 walks the panes: first, second, and Shift+F6 back.
    if len(panes) >= 2:
        app.must("click", selector="#%s" % panes[-1])
        app.frame(1)
        focused_before = len(app.printed("AZSHELLS_PANE", r"\d+"))
        app.key("F6")
        app.until("F6 to reach a pane", lambda: len(app.printed("AZSHELLS_PANE", r"\d+")) > focused_before)
        first_node, first_sel = app.focused()
        app.key("F6")
        second_node, second_sel = app.focused()
        app.key("F6", shift=True)
        back_node, back_sel = app.focused()
        pane_nodes = [rects[p][0] for p in panes if p in rects]
        if first_node not in pane_nodes:
            raise Failure("%s: F6 focused %s (%s), not a pane of %s" % (pick, first_node, first_sel, panes))
        if second_node not in pane_nodes or second_node == first_node:
            raise Failure("%s: the second F6 focused %s (%s)" % (pick, second_node, second_sel))
        if back_node != first_node:
            raise Failure("%s: Shift+F6 focused %s (%s), not the first pane again" % (pick, back_node, back_sel))
        log("%s: F6 %s -> %s, Shift+F6 -> %s" % (pick, first_sel, second_sel, back_sel))

    app.screenshot(os.path.join(out, "%s.png" % pick.lower()))


def run(args, logs, out):
    binary = find_binary(args.bin)
    deadline = time.time() + args.timeout
    env = dict(os.environ)
    env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(args.debug_port)})
    app = App(binary, args.debug_port, env, logs, deadline)
    try:
        app.until("AzShells' window", lambda: app.shows("S1"))
        app.must("resize", width=args.width, height=args.height)
        app.frame(2)
        for n, pick in enumerate(PICKS, start=1):
            check_shell(app, n, pick, args.width, args.height, out)

        # The command palette: Ctrl+K opens it over S4, Escape closes it.
        app.must("click", text="S4")
        app.frame(2)
        app.key("k", primary=True)
        app.until("the command palette", lambda: "__azul-native-command-palette-panel" in app.classes())
        app.screenshot(os.path.join(out, "s4-palette.png"))
        app.key("Escape")
        app.until("the palette to close", lambda: "__azul-native-command-palette-panel" not in app.classes())

        # Flora and dark: the same shell in the other theme and mode.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.until("the flora marker", lambda: "__azul-theme-flora" in app.classes())
        app.screenshot(os.path.join(out, "s4-flora-dark.png"))
        app.must("set_theme", theme="flat")
        app.must("set_mode", mode="light")
        app.frame(2)
        log("PASS: %d shells checked, screenshots in %s" % (len(PICKS), out))
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
    parser.add_argument("--debug-port", type=int, default=8771)
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--width", type=int, default=1100)
    parser.add_argument("--height", type=int, default=720)
    parser.add_argument("--out")
    parser.add_argument("--keep-logs", action="store_true")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azshells-e2e-")
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    passed = False
    try:
        passed = run(args, logs, out)
    except Failure:
        passed = False
    finally:
        if passed and not args.keep_logs and not args.out:
            log("logs and screenshots in %s" % logs)
        else:
            log("logs in %s" % logs)
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
