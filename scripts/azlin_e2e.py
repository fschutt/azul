#!/usr/bin/env python3
"""Shared driver for the Azlin apps' headless E2E scripts (azcalculator_e2e.py,
azcontacts_e2e.py): start an app with AZ_BACKEND=headless and AZ_DEBUG=<port>,
talk to its debug server (ops in layout/src/e2e/full.rs), read the lines it
prints, take screenshots.

Every op that changes state is followed by `frame()` (wait_frame). The app's
stdout is read from a log file; `printed(key)` returns the values of the lines
`<KEY> <value>`, `until(what, check)` polls until `check()` is truthy.
"""

import base64
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))


class Failure(Exception):
    pass


# No modifier held (the end of a key tap).
RELEASED = {"shift": False, "ctrl": False, "alt": False, "meta": False}


def repo_roots():
    """This checkout, and the main checkout when this is a git worktree."""
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


def find_binary(name, explicit=None, env_var=None):
    candidates = []
    if explicit:
        candidates.append(explicit)
    if env_var and os.environ.get(env_var):
        candidates.append(os.environ[env_var])
    for root in repo_roots():
        for sub in ("release", "debug"):
            candidates.append(os.path.join(root, "target", sub, name))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no %s binary; pass --bin (tried %s)" % (name, candidates))


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


def read_png(path):
    """(width, height, rows) of an 8-bit RGB / RGBA PNG (what take_screenshot
    writes); rows[y][x] is an (r, g, b, a) tuple. No third-party modules."""
    import struct
    import zlib

    with open(path, "rb") as f:
        data = f.read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise Failure("%s is not a PNG" % path)
    pos, idat, width, height, channels = 8, b"", 0, 0, 0
    while pos < len(data):
        length, kind = struct.unpack(">I4s", data[pos:pos + 8])
        chunk = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if kind == b"IHDR":
            width, height, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
            if depth != 8 or color not in (2, 6) or interlace:
                raise Failure("%s: only 8-bit RGB / RGBA, not interlaced (depth %d, color %d)"
                              % (path, depth, color))
            channels = 3 if color == 2 else 4
        elif kind == b"IDAT":
            idat += chunk
        elif kind == b"IEND":
            break
    raw = zlib.decompress(idat)
    stride = width * channels
    rows, prev, i = [], bytearray(stride), 0
    for _ in range(height):
        kind, line = raw[i], bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - channels] if x >= channels else 0
            b = prev[x]
            c = prev[x - channels] if x >= channels else 0
            if kind == 1:
                line[x] = (line[x] + a) & 0xFF
            elif kind == 2:
                line[x] = (line[x] + b) & 0xFF
            elif kind == 3:
                line[x] = (line[x] + (a + b) // 2) & 0xFF
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if pa <= pb and pa <= pc else (b if pb <= pc else c)
                line[x] = (line[x] + pred) & 0xFF
        rows.append([tuple(line[x:x + channels]) + ((255,) if channels == 3 else ())
                     for x in range(0, stride, channels)])
        prev = line
    return width, height, rows


def dark_pixels(path, rect, scale=1.0, threshold=100):
    """How many pixels of the logical `rect` (x, y, width, height) of the
    screenshot at `path` are dark (r, g and b below `threshold`)."""
    _, _, rows = read_png(path)
    x0, y0, w, h = (int(round(v * scale)) for v in rect)
    count = 0
    for y in range(max(y0, 0), min(y0 + h, len(rows))):
        row = rows[y]
        for x in range(max(x0, 0), min(x0 + w, len(row))):
            r, g, b, _ = row[x]
            if r < threshold and g < threshold and b < threshold:
                count += 1
    return count


def tail(path, lines=40):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return "".join(f.readlines()[-lines:])
    except OSError:
        return "(no output)"


class App:
    """One app under its debug server.

    `capped` (scripts/waves/tools/run_capped.sh) runs the app under its own memory cap
    (`cap_mb`, `cap_seconds`) - where no outer runner caps the whole script (two apps at once);
    the runner holds a machine-wide lock, so never inside an outer one. The app's stdout and
    stderr then share `out_path` (the runner's log), and `stop` stops the runner's whole
    process group."""

    def __init__(self, tag, binary, args, port, logs, timeout, extra_env=None, capped=None,
                 cap_mb=1500, cap_seconds=None):
        self.tag = tag
        self.name = tag
        self.port = port
        self.deadline = time.time() + timeout
        self.out_path = os.path.join(logs, "%s.stdout" % tag)
        self.err_path = os.path.join(logs, "%s.stderr" % tag)
        env = dict(os.environ)
        env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(port)})
        env.update(extra_env or {})
        command = [binary] + list(args)
        self.capped = bool(capped)
        if capped:
            command = [capped, "--cap-mb", str(cap_mb), "--seconds", str(int(cap_seconds or timeout)),
                       "--log", self.out_path, "--"] + command
            self.err_path = self.out_path
            runner = os.path.join(logs, "%s.runner" % tag)
            stdout, stderr = open(runner, "wb"), subprocess.STDOUT
        else:
            stdout, stderr = open(self.out_path, "wb"), open(self.err_path, "wb")
        self.process = subprocess.Popen(
            command, env=env, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
            start_new_session=self.capped,
        )

    def log(self, line):
        print("[%s] %s" % (self.tag, line), flush=True)

    def alive(self):
        return self.process.poll() is None

    def tail(self, lines=40):
        return tail(self.out_path, lines)

    def stop(self):
        if self.process.poll() is not None:
            return
        if self.capped:
            # The runner and the app it started: the whole process group.
            try:
                os.killpg(self.process.pid, signal.SIGTERM)
                self.process.wait(timeout=3)
            except (OSError, subprocess.TimeoutExpired):
                try:
                    os.killpg(self.process.pid, signal.SIGKILL)
                except OSError:
                    pass
            return
        self.process.terminate()
        try:
            self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            self.process.kill()

    # ---- the debug server ----

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

    # ---- what the window shows ----

    def texts(self):
        return list(strings(self.op("get_node_hierarchy")))

    def shows(self, text):
        return any(text in t for t in self.texts())

    def hierarchy(self):
        """The window's nodes (`index`, `type`, `text`, `classes`, `parent`, `children`)."""
        return [d for d in dicts(self.op("get_node_hierarchy")) if "index" in d and "type" in d]

    def classes(self):
        """Every class a node of the window carries."""
        return {c for n in self.hierarchy() for c in (n.get("classes") or [])}

    def nodes_with_class(self, cls):
        return [n["index"] for n in self.hierarchy() if cls in (n.get("classes") or [])]

    def exact(self, text):
        """The node holding the text node whose text is exactly `text` (the first one)."""
        for n in self.hierarchy():
            if n.get("text") == text:
                return n.get("parent", n["index"])
        return None

    def click_exact(self, text, button="left", double=False, frames=2):
        """Clicks (or double-clicks) the node holding exactly `text`, once it is there."""
        node = self.until('the text "%s"' % text, lambda: self.exact(text))
        self.must("double_click" if double else "click", node_id=node, button=button)
        self.frame(frames)

    def settle(self, limit=3.0):
        """Waits (at most `limit` seconds) until no animation, exit or transition runs, so a
        screenshot does not catch a slide or a fade midway."""
        end = time.time() + limit
        while time.time() < end:
            value = self.value("get_animations")
            if not isinstance(value, dict) or not (
                    value.get("active") or value.get("zombies") or value.get("transitions")):
                return
            time.sleep(0.1)
            self.frame(1)

    def has_id(self, node_id):
        return self.has("#%s" % node_id)

    def rect(self, node_id):
        value = self.value("get_node_layout", selector="#%s" % node_id)
        return value.get("rect") or {}

    def has(self, selector):
        """Whether `selector` (any CSS selector the debug server reads) names a laid-out node."""
        try:
            answer = self.op("get_node_layout", selector=selector)
        except (OSError, ValueError, urllib.error.URLError):
            return False
        if not isinstance(answer, dict) or answer.get("status") == "error":
            return False
        data = answer.get("data") or {}
        value = data.get("value") if isinstance(data, dict) else None
        return isinstance(value, dict) and value.get("node_id") is not None

    def box(self, selector):
        """The laid-out rect of `selector` (window coordinates before scrolling) as floats."""
        value = self.value("get_node_layout", selector=selector)
        r = (value or {}).get("rect") or {}
        return {key: float(r.get(key, 0)) for key in ("x", "y", "width", "height")}

    # ---- the app's DOM names ----
    # Every Azlin app's ids and classes carry its prefix (`__azcontacts_`, ...: the wave-6 prefix
    # ruling, each app's src/ids.rs); a build from before the ruling used the bare names.
    # `detect_naming` notes which one is running, `sel(stem)` / `name(stem)` give the app's
    # selector / name of `stem` either way.
    prefix = ""

    def detect_naming(self, prefix, probe):
        """Waits for the app's id `probe` (a stem), under `prefix` or bare; returns the prefix
        the app uses from now on ("" for an older build)."""
        def found():
            if self.has_id(prefix + probe):
                return (prefix,)
            if self.has_id(probe):
                return ("",)
            return None
        self.prefix = self.until("#%s%s (or #%s)" % (prefix, probe, probe), found)[0]
        self.log("names: %s" % ("%s prefixed" % prefix if self.prefix else "unprefixed (older build)"))
        return self.prefix

    def name(self, stem):
        """The app's id or class `stem`, with the app's prefix."""
        return self.prefix + stem

    def sel(self, stem):
        """The selector of the app's id `stem` (`#<prefix><stem>`)."""
        return "#" + self.name(stem)

    # ---- input ----

    def click(self, selector=None, text=None, frames=2):
        if selector:
            self.must("click", selector=selector)
        else:
            self.must("click", text=text)
        self.frame(frames)

    def key(self, key, shift=False, ctrl=False, alt=False, meta=False, frames=2, primary=False):
        # `primary`: the platform's shortcut modifier, as the apps read it
        # (KeyModifiers::primary_down) - Cmd on macOS, Ctrl elsewhere.
        if primary:
            if sys.platform == "darwin":
                meta = True
            else:
                ctrl = True
        mods = {"shift": shift, "ctrl": ctrl, "alt": alt, "meta": meta}
        self.must("key_down", key=key, modifiers=mods)
        # A tap of the chord: the key and its modifiers come up together. An op's `modifiers`
        # are the whole modifier state at its key (layout/src/e2e/full.rs), so a key_up with the
        # chord's modifiers would leave them held - every later click a Cmd / Shift + click.
        self.must("key_up", key=key, modifiers=RELEASED)
        self.frame(frames)

    def drag(self, x0, y0, x1, y1, steps=8):
        """A mouse drag from (x0, y0) to (x1, y1) in `steps` moves, a frame each: what starts
        an app's drag (`draggable`, DragStart) and drops it (DragOver, Drop)."""
        self.must("mouse_move", x=x0, y=y0)
        self.frame(1)
        self.must("mouse_down", x=x0, y=y0)
        self.frame(1)
        for i in range(1, steps + 1):
            t = i / float(steps)
            self.must("mouse_move", x=x0 + (x1 - x0) * t, y=y0 + (y1 - y0) * t)
            self.frame(1)
        self.must("mouse_up", x=x1, y=y1)
        self.frame(2)

    def type_keys(self, keys):
        """keys: a list of key names or (name, {"shift": True}) pairs."""
        for k in keys:
            if isinstance(k, tuple):
                self.key(k[0], **k[1])
            else:
                self.key(k)

    def text_input(self, selector, text):
        self.must("focus_node", selector=selector)
        self.frame()
        self.must("text_input", text=text)
        self.frame(2)

    # ---- stdout ----

    def printed(self, key, pattern=r".*"):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^%s (%s)$" % (re.escape(key), pattern), text, re.M)

    def last(self, key):
        values = self.printed(key)
        return values[-1] if values else None

    def count(self, key, pattern=r".*"):
        return len(self.printed(key, pattern))

    def after(self, what, key, pattern, action):
        """Runs `action`, then waits for a new `<KEY> <pattern>` line; returns the last value."""
        before = self.count(key, pattern)
        action()
        self.until(what, lambda: self.count(key, pattern) > before)
        return self.printed(key, pattern)[-1]

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("%s exited (%s) while waiting for %s" % (self.tag, self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, KeyError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def expect_line(self, key, expected, what=None):
        """Waits until the last `<key> ...` line equals `expected`."""
        def check():
            return self.last(key) == expected
        try:
            self.until(what or "%s %r" % (key, expected), check)
        except Failure:
            raise Failure("%s: expected %r, last %r" % (what or key, expected, self.last(key)))

    def screenshot(self, path):
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure("take_screenshot returned no PNG: %s" % json.dumps(value)[:200])
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        self.log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


def run(tag, body, argv=None, default_port=8781, binary_name=None, binary_env=None):
    """Parses the common switches, starts nothing itself: `body(args, logs, out)`
    does the work and returns True. Exit code 0 on success."""
    import argparse

    parser = argparse.ArgumentParser(description=sys.modules["__main__"].__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=default_port)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--out")
    parser.add_argument("--keep", action="store_true", help="keep the data folder and logs")
    args = parser.parse_args(argv)
    logs = tempfile.mkdtemp(prefix="%s-e2e-" % tag)
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    passed = False
    try:
        passed = bool(body(args, logs, out))
    except Failure as e:
        print("[%s] FAIL: %s" % (tag, e), flush=True)
        passed = False
    finally:
        print("[%s] logs and screenshots in %s" % (tag, logs), flush=True)
    sys.exit(0 if passed else 1)
