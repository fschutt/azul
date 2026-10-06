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


def color(text):
    """`#rrggbbaa` / `#rrggbb` as (r, g, b, a) with a in 0..1."""
    h = text.lstrip("#")
    r, g, b = int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16)
    a = int(h[6:8], 16) / 255.0 if len(h) >= 8 else 1.0
    return r, g, b, a


def over(top, base):
    r, g, b, a = top
    return tuple(c * a + d * (1.0 - a) for c, d in zip((r, g, b), base))


def luminance(rgb):
    def lin(v):
        v /= 255.0
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4

    r, g, b = rgb
    return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)


def ratio(a, b):
    la, lb = luminance(a), luminance(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


FILLS = ("rect", "linear_gradient", "radial_gradient", "conic_gradient")


def contrast_findings(items, base):
    """Text items whose ink reads under 2:1 against the rectangles painted under their centre
    (in paint order, in the text's scroll frame or one enclosing it) over the window's ground
    `base`. Only fills at EXACTLY the text's depth were counted: flora's title text sits in a
    frame of its own inside the title bar's, and was measured against the white window."""
    rects = []
    found = []
    for it in items:
        kind = it.get("type")
        if not it.get("color") or it.get("width") is None or it.get("height") is None:
            continue
        # A gradient (flora's title bar, its active ribbon tab) is a fill too: the debug server
        # lists it with its bounds and the mean of its stops.
        if kind in FILLS:
            rects.append(it)
            continue
        if kind not in ("text", "text_layout"):
            continue
        ink = color(it["color"])
        if ink[3] == 0 or it["width"] <= 0 or it["height"] <= 0:
            continue
        cx = it["x"] + it["width"] / 2.0
        cy = it["y"] + it["height"] / 2.0
        bg = base
        for r in rects:
            if (r.get("scroll_depth") or 0) > (it.get("scroll_depth") or 0):
                continue
            if r["x"] <= cx <= r["x"] + r["width"] and r["y"] <= cy <= r["y"] + r["height"]:
                bg = over(color(r["color"]), bg)
        seen = over(ink, bg)
        q = ratio(seen, bg)
        if q < 2.0:
            found.append(
                f"item {it.get('index')} at ({it['x']:.0f}, {it['y']:.0f}) {it['width']:.0f}x"
                f"{it['height']:.0f}: ink {it['color']} on {tuple(round(c) for c in bg)} = {q:.2f}:1"
            )
    return found


def text_box(nodes, prefix):
    """The box (x, y, width, height) of the first of `nodes` (a node hierarchy) whose text starts
    with `prefix` - its nearest ancestor's with a box, a text node having none - else None."""
    by_index = {n.get("index"): n for n in nodes}
    for n in nodes:
        if (n.get("text") or "").startswith(prefix):
            target = n
            while target is not None and not target.get("rect"):
                target = by_index.get(target.get("parent"))
            if target is None:
                return None
            r = target["rect"]
            return r["x"], r["y"], r["width"], r["height"]
    return None


def overlap_finding(nodes, prefixes):
    """None when the lines starting with `prefixes` each have a height and none is drawn over
    another (AzMail's wizard drew "Account: ..." over "Incoming: ...", AzContacts' groups tree
    "Imported (2)" over "Neighbours (21)"); else what is wrong."""
    boxes = [(text_box(nodes, p), p) for p in prefixes]
    missing = [p for b, p in boxes if b is None]
    if missing:
        return f"no line for {missing}"
    boxes.sort(key=lambda bp: bp[0][1])
    for (a, p), (b, q) in zip(boxes, boxes[1:]):
        if a[3] <= 0 or b[1] < a[1] + a[3] - 0.5:
            return f'"{p}" {a} and "{q}" {b} overlap'
    return None


def settle_animations(animations, frame, limit=3.0):
    """Waits (at most `limit` seconds) until no animation, exit or transition runs: a click
    lands where the node is PAINTED (an entrance animation moves it off its layout rect), and
    a screenshot should not catch a slide midway. `animations()` answers the debug op
    `get_animations` (its value), `frame()` lets the app run one frame. For drivers of their
    own (azmail_e2e.py) as much as for `App`."""
    end = time.time() + limit
    while time.time() < end:
        value = animations()
        if not isinstance(value, dict) or not (
                value.get("active") or value.get("zombies") or value.get("transitions")):
            return
        time.sleep(0.1)
        frame()


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

    def window_ids(self):
        """The ids of the app's windows (`list_windows`), the app's first window first. An open
        Modal / Popover / menu is a window of its own (`azul-transient`, `azul-menu`)."""
        value = self.value("list_windows")
        wins = value.get("windows") if isinstance(value, dict) else value
        return [w["window_id"] for w in wins or [] if isinstance(w, dict) and w.get("window_id")]

    def popup(self):
        """The id of the newest window besides the app's first (an open Modal's), or None."""
        ids = self.window_ids()
        return ids[-1] if len(ids) > 1 else None

    def dom_ids(self, window=None):
        """The ids of the window's DOMs (the debug server's `list_doms`): its own document (0),
        every VirtualView's document and every open popup's content."""
        value = self.value("list_doms", **({"window_id": window} if window else {}))
        doms = value.get("doms") if isinstance(value, dict) else None
        return [d["dom_id"] for d in doms or [] if isinstance(d, dict) and "dom_id" in d] or [0]

    def texts(self, every_dom=False):
        """The strings of the window's node hierarchy: of its own document (DOM 0), or with
        `every_dom` of every DOM it shows - a VirtualView's rows (AzMonitor's process table, its
        cards' headlines) live in a DOM of their own, which DOM 0's hierarchy does not hold."""
        if not every_dom:
            return list(strings(self.op("get_node_hierarchy")))
        found = []
        for dom in self.dom_ids():
            found.extend(strings(self.op("get_node_hierarchy", dom_id=dom)))
        return found

    def shows(self, text, every_dom=False):
        return any(text in t for t in self.texts(every_dom))

    def hierarchy(self, window=None):
        """The window's nodes (`index`, `type`, `text`, `classes`, `parent`, `children`): the
        app's first window's, or `window`'s (an open Modal's, `popup()`)."""
        answer = self.op("get_node_hierarchy", **({"window_id": window} if window else {}))
        return [d for d in dicts(answer) if "index" in d and "type" in d]

    def classes(self):
        """Every class a node of the window carries."""
        return {c for n in self.hierarchy() for c in (n.get("classes") or [])}

    def nodes_with_class(self, cls):
        return [n["index"] for n in self.hierarchy() if cls in (n.get("classes") or [])]

    def _within(self, scope):
        """The window's nodes, and a test: does a node lie inside the node(s) `scope` names -
        `#id` an id, anything else a class (a leading `.` optional)?"""
        nodes = self.hierarchy()
        by_index = {n["index"]: n for n in nodes}
        if scope.startswith("#"):
            def matches(node):
                return node.get("id") == scope[1:]
        else:
            cls = scope[1:] if scope.startswith(".") else scope

            def matches(node):
                return cls in (node.get("classes") or [])

        def inside(node):
            for _ in range(256):
                if node is None:
                    return False
                if matches(node):
                    return True
                node = by_index.get(node.get("parent"))
            return False

        return nodes, inside

    def texts_within(self, scope):
        """The texts of the window whose node lies inside the node(s) `scope` names (`#id` or a
        class; `shows` reads every text - a search field holding the word included)."""
        nodes, inside = self._within(scope)
        return [n["text"] for n in nodes if n.get("text") and inside(n)]

    def click_within(self, scope, text, frames=2):
        """Clicks the node holding exactly `text` inside the node(s) `scope` names (`#id` or a
        class), once it is there (a click by text takes the first node CONTAINING the text
        anywhere: a chart's "Sales by row" before the table's "Sales" header)."""
        def found():
            nodes, inside = self._within(scope)
            for n in nodes:
                if n.get("text") == text and inside(n):
                    return n.get("parent", n["index"])
            return None
        node = self.until('the text "%s" in %s' % (text, scope), found)
        self.settle(limit=2.0)
        self.must("click", node_id=node)
        self.frame(frames)

    def exact(self, text, window=None):
        """The node holding the text node whose text is exactly `text` (the first one)."""
        for n in self.hierarchy(window):
            if n.get("text") == text:
                return n.get("parent", n["index"])
        return None

    def text_rect(self, text):
        """The laid-out rect ({x, y, width, height}) of the first node showing exactly `text`, or
        None. A text node has no box of its own (`get_node_layout text=...` answers it with
        `rect: null`), so this is its nearest ancestor that has one - the label's button, cell or
        row."""
        nodes = self.hierarchy()
        by_index = {n["index"]: n for n in nodes}
        for n in nodes:
            if (n.get("text") or "").strip() != text:
                continue
            at, seen = n, 0
            while at is not None and not at.get("rect") and seen < 64:
                at, seen = by_index.get(at.get("parent")), seen + 1
            if at is not None and at.get("rect"):
                return {key: float(at["rect"].get(key, 0)) for key in ("x", "y", "width", "height")}
        return None

    def click_exact(self, text, button="left", double=False, frames=2, window=None):
        """Clicks (or double-clicks) the node holding exactly `text`, once it is there - in the
        app's first window, or in `window` (an open Modal's, `popup()`). `click(text=...)`
        takes the first text CONTAINING `text`: a dialog's "Kill" button lost to the
        paragraph explaining what Kill does."""
        node = self.until('the text "%s"' % text, lambda: self.exact(text, window))
        self.settle(limit=2.0, window=window)
        target = {"window_id": window} if window else {}
        op = "double_click" if double else "click"
        # An inline box (the <span> around a list item's title) has no rect of its own, and a
        # click by node id resolves no position for it: the click goes to the nearest ancestor
        # that has a box, as the server's own text click does.
        parents = {n["index"]: n.get("parent") for n in self.hierarchy(window)}
        last = None
        while isinstance(node, int) and node >= 0:
            last = self.op(op, node_id=node, button=button, **target)
            if isinstance(last, dict) and last.get("status") != "error":
                break
            node = parents.get(node)
        else:
            raise Failure("%s on %r: no node from its text up has a box (%s)"
                          % (op, text, json.dumps(last)[:200]))
        self.frame(frames)

    def settle(self, limit=3.0, window=None):
        """Waits (at most `limit` seconds) until no animation, exit or transition runs (in the
        app's first window, or `window`), so a screenshot does not catch a slide or a fade
        midway."""
        end = time.time() + limit
        while time.time() < end:
            value = self.value("get_animations", **({"window_id": window} if window else {}))
            if not isinstance(value, dict) or not (
                    value.get("active") or value.get("zombies") or value.get("transitions")):
                return
            time.sleep(0.1)
            self.frame(1)

    def has_id(self, node_id, every_dom=False):
        return self.has("#%s" % node_id, every_dom)

    def rect(self, node_id, every_dom=False):
        """The rect of the node `#node_id` - of DOM 0, or with `every_dom` of the first DOM that
        has it (AzPdf's pages live in a VirtualView's DOM; its rects are in that DOM's own
        coordinates, so compare them with each other, not with DOM 0's)."""
        if every_dom:
            for dom in self.dom_ids():
                if self._has_in("#%s" % node_id, dom):
                    value = self.value("get_node_layout", selector="#%s" % node_id, dom_id=dom)
                    return value.get("rect") or {}
            raise Failure("no DOM of the window has #%s" % node_id)
        value = self.value("get_node_layout", selector="#%s" % node_id)
        return value.get("rect") or {}

    def has(self, selector, every_dom=False):
        """Whether `selector` (any CSS selector the debug server reads) names a laid-out node -
        of DOM 0, or with `every_dom` of any DOM the window shows (a VirtualView's)."""
        if every_dom:
            try:
                doms = self.dom_ids()
            except (OSError, ValueError, Failure, urllib.error.URLError):
                return False
            return any(self._has_in(selector, dom) for dom in doms)
        return self._has_in(selector, None)

    def _has_in(self, selector, dom):
        params = {"selector": selector}
        if dom is not None:
            params["dom_id"] = dom
        try:
            answer = self.op("get_node_layout", **params)
        except (OSError, ValueError, urllib.error.URLError):
            return False
        if not isinstance(answer, dict) or answer.get("status") == "error":
            return False
        data = answer.get("data") or {}
        value = data.get("value") if isinstance(data, dict) else None
        return isinstance(value, dict) and value.get("node_id") is not None

    def laid_out(self, selector):
        """Whether `selector` names a node with a box in this window (a Modal's nodes are in
        its owner's DOM too, without one)."""
        return self.has(selector) and self.box(selector)["width"] > 0

    def box(self, selector):
        """Where `selector`'s box is on screen (window coordinates after its scroll containers'
        offsets - where a pointer has to go), as floats; its laid-out rect for a build whose
        debug server reports no `screen_rect`."""
        value = self.value("get_node_layout", selector=selector) or {}
        r = value.get("screen_rect") or value.get("rect") or {}
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

    def click(self, selector=None, text=None, frames=2, every_dom=False, window=None):
        # A click lands where the node IS: an entrance animation (AzCalculator's
        # Scientific keys slide in) moves it off its layout rect, and the engine
        # hits what is painted, as a user would. Settle first, or the click
        # misses the key it names (it hit a neighbour or nothing).
        self.settle(limit=2.0, window=window)
        target = {"selector": selector} if selector else {"text": text}
        if window:
            # An open Modal is a window of its own (`popup()`): its buttons are not in the
            # app's window, where a click at their rect lands on nothing.
            target["window_id"] = window
        if every_dom:
            # The first DOM that has the target: a VirtualView's rows (AzMonitor's process
            # table) are a DOM of their own, which a click naming no `dom_id` never searches.
            for dom in self.dom_ids(window):
                answer = self.op("click", dom_id=dom, **target)
                if isinstance(answer, dict) and answer.get("status") != "error":
                    break
            else:
                raise Failure("click %s: no DOM of the window has it" % json.dumps(target))
        else:
            self.must("click", **target)
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
        """The values of the lines `<KEY> <value>` whose value matches `pattern`. A bare line
        `<KEY>` (AZREADER_READY, AZWRITER_READY, AZTERM_READY print no value) counts as the
        value "" when `pattern` can match an empty value (the default `.*`)."""
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        if re.fullmatch(pattern, "") is not None:
            return re.findall(r"^%s(?: (%s))?$" % (re.escape(key), pattern), text, re.M)
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
        # Settle first (the house rule: nothing moving when the picture is taken). A rebuild
        # slides every moved node to its new place: AzSheets' Budget sample, shot mid-slide,
        # showed the grid's cell borders strewn over the empty rows.
        self.settle()
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure("take_screenshot returned no PNG: %s" % json.dumps(value)[:200])
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        self.log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


class InWindow(App):
    """`app` with every op addressed to one of its windows (the request's `window_id`): a
    `Modal` is a transient window of its own, and its nodes are laid out - and clicked - there,
    not in the main window (they are in the main window's DOM, without a box). Everything else
    (stdout, waits) is the app's; frames are the app's loop turns, so a click that closes the
    window still gets its frames."""

    def __init__(self, app, window_id):
        self.__dict__.update(app.__dict__)
        self.app = app
        self.window_id = window_id

    def op(self, op, **params):
        params.setdefault("window_id", self.window_id)
        return self.app.op(op, **params)

    def frame(self, n=1):
        self.app.frame(n)

    def stop(self):
        raise Failure("stop the app, not one of its windows")


def modal_window(app, known=()):
    """The open modal's window: the one window that is neither the app's own (the default) nor
    in `known`."""
    def other():
        windows = (app.value("list_windows") or {}).get("windows") or []
        ids = [w.get("window_id") for w in windows
               if not w.get("is_default") and w.get("window_id") not in known]
        return ids[0] if ids else None
    return InWindow(app, app.until("the modal's window", other))


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
