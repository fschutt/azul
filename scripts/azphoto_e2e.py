#!/usr/bin/env python3
"""AzPhoto end to end over the debug server (headless).

    1. starts AzPhoto with --sample (the photo, a light leak in Screen mode, a Curves
       adjustment) on a fresh data folder (--data-dir), headless with the debug server;
    2. checks the document (1920 x 1080, three layers) and that the canvas node is laid out;
    3. selects the Photo layer and paints a brush stroke across the canvas: the History
       ends in "Brush" and the canvas got PARTIAL updates (`AZPHOTO_UPDATE` rects smaller
       than the view - the dirty-rect path, never the whole canvas per pointer move);
    4. adds a layer (Layers panel "+"), sets its opacity to 50 % (Move tool, key 5),
       undoes that (Ctrl+Z: the History is back at "New Layer"); drags the Photo layer
       with the Move tool (canvas updates WHILE dragging, one "Move" state); sets a text
       with the Text tool (azul's RawImage::from_text: a fifth layer, one "Text" state);
    5. exports (Ctrl+Shift+E, Export) into the data tree (photo/<uuid>/exports/) and checks the PNG;
    6. saves (Ctrl+S) and checks photo/<uuid>/doc.json and the layer tiles;
    7. takes screenshots (flat light, flora dark).

Usage (from the azul repository, after building libazul with the debug server and AzPhoto):

    python3 scripts/azphoto_e2e.py [--bin target/release/AzPhoto] [--debug-port 8781]
        [--timeout 180] [--width 1400] [--height 900] [--out <dir>]

`AZPHOTO_BIN` also names the binary. Run it through the capped runner on a small machine.
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
PNG_MAGIC = b"\x89PNG\r\n\x1a\n"


def log(line):
    print("[azphoto] %s" % line, flush=True)


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
    if os.environ.get("AZPHOTO_BIN"):
        candidates.append(os.environ["AZPHOTO_BIN"])
    for root in repo_roots():
        for sub in ("release", "debug"):
            candidates.append(os.path.join(root, "target", sub, "AzPhoto"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no AzPhoto binary; pass --bin or set AZPHOTO_BIN (tried %s)" % candidates)


def strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for v in value:
            yield from strings(v)
    elif isinstance(value, dict):
        for v in value.values():
            yield from strings(v)


def tail(path, lines=40):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            return "".join(f.readlines()[-lines:])
    except OSError:
        return "(no output)"


class App:
    """AzPhoto under its debug server."""

    def __init__(self, binary, args, port, env, logs, deadline):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "azphoto.stdout")
        self.err_path = os.path.join(logs, "azphoto.stderr")
        self.process = subprocess.Popen(
            [binary] + args, env=env, stdin=subprocess.DEVNULL,
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
        with urllib.request.urlopen(request, timeout=30) as response:
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

    def has_id(self, node_id):
        answer = self.op("get_node_layout", selector="#%s" % node_id)
        if not isinstance(answer, dict) or answer.get("status") == "error":
            return False
        data = answer.get("data") or {}
        value = data.get("value") if isinstance(data, dict) else None
        return isinstance(value, dict) and value.get("node_id") is not None

    def printed(self, key, pattern=r".*"):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^%s ?(%s)$" % (re.escape(key), pattern), text, re.M)

    def last(self, key, pattern=r".*"):
        found = self.printed(key, pattern)
        return found[-1] if found else None

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("AzPhoto exited (%s) while waiting for %s" % (self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def rect(self, selector):
        value = self.value("get_node_layout", selector=selector)
        rect = (value or {}).get("rect") or {}
        if not rect or float(rect.get("width", 0)) <= 0:
            raise Failure("%s has no layout: %s" % (selector, json.dumps(value)[:200]))
        return rect

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

    def click(self, selector):
        self.must("click", selector=selector)
        self.frame(2)

    def screenshot(self, path):
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            raise Failure("take_screenshot returned no PNG: %s" % json.dumps(value)[:200])
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


def history(app):
    """(count, current index, label) of the last AZPHOTO_HISTORY line."""
    line = app.last("AZPHOTO_HISTORY", r"\d+ \d+.*")
    if not line:
        return None
    parts = line.split(" ", 2)
    return int(parts[0]), int(parts[1]), parts[2] if len(parts) > 2 else ""


def run(args, logs, out):
    binary = find_binary(args.bin)
    deadline = time.time() + args.timeout
    data = os.path.join(logs, "data")
    os.makedirs(data, exist_ok=True)
    env = dict(os.environ)
    env.update({
        "AZ_BACKEND": "headless",
        "AZ_DEBUG": str(args.debug_port),
    })
    app = App(binary, ["--sample", "--theme", "flat", "--mode", "light", "--data-dir", data],
              args.debug_port, env, logs, deadline)
    try:
        # 1-2: the sample is open.
        app.until("the sample document", lambda: app.last("AZPHOTO_DOC", r"\S+ \d+ .*"))
        doc = app.last("AZPHOTO_DOC", r"\S+ \d+ .*")
        if not doc.startswith("1920x1080 3 "):
            raise Failure("the sample should be 1920x1080 with three layers, got %r" % doc)
        app.until("the window", lambda: app.shows("LAYERS"))
        app.must("resize", width=args.width, height=args.height)
        app.frame(3)
        app.until("the canvas size", lambda: app.last("AZPHOTO_VIEW", r"\d+x\d+"))
        canvas = app.rect("#__azphoto_canvas")
        view = app.last("AZPHOTO_VIEW", r"\d+x\d+")
        vw, vh = (int(v) for v in view.split("x"))
        log("canvas %s, view %dx%d px" % (canvas, vw, vh))
        app.screenshot(os.path.join(out, "azphoto-sample.png"))

        # 3: the Photo layer, then a brush stroke across the canvas.
        app.click("#__azphoto_layer-row-1")
        app.until("the Photo layer to be active", lambda: (app.last("AZPHOTO_LAYERS", r"\d+ .*") or "").endswith("Photo"))
        app.click("#__azphoto_tool-brush")
        updates_before = len(app.printed("AZPHOTO_UPDATE", r"-?\d+ -?\d+ \d+ \d+"))
        x0 = float(canvas["x"]) + float(canvas["width"]) * 0.3
        y0 = float(canvas["y"]) + float(canvas["height"]) * 0.45
        app.must("mouse_move", x=x0, y=y0)
        app.must("mouse_down", x=x0, y=y0)
        app.frame(1)
        for i in range(1, 13):
            app.must("mouse_move", x=x0 + i * 12.0, y=y0 + (i % 3) * 3.0)
            app.frame(1)
        app.must("mouse_up", x=x0 + 144.0, y=y0)
        app.frame(3)
        app.until("the stroke in the History", lambda: (history(app) or (0, 0, ""))[2] == "Brush")
        updates = app.printed("AZPHOTO_UPDATE", r"-?\d+ -?\d+ \d+ \d+")[updates_before:]
        if not updates:
            raise Failure("the stroke sent no partial canvas update")
        for u in updates:
            _, _, w, h = (int(v) for v in u.split(" "))
            if w * h >= vw * vh:
                raise Failure("a stroke update covered the whole view (%s); it must be a dirty rect" % u)
        log("stroke: %d partial updates, largest %s" % (
            len(updates), max(updates, key=lambda u: int(u.split()[2]) * int(u.split()[3]))))

        # 4: a new layer, its opacity, undo.
        app.click("#__azphoto_layer-new")
        app.until("the new layer", lambda: (app.last("AZPHOTO_LAYERS", r"\d+ .*") or "").startswith("4 "))
        app.click("#__azphoto_tool-move")
        app.key("5")
        app.until("the opacity", lambda: (app.last("AZPHOTO_OPACITY", r"\d+ \d+") or "").endswith(" 50"))
        count, current, label = history(app)
        if label != "Opacity":
            raise Failure("the History should end in Opacity, got %r" % label)
        app.key("z", primary=True)
        app.until("the undo", lambda: (history(app) or (0, 0, ""))[1] == current - 1)
        if history(app)[2] != "New Layer":
            raise Failure("undo should go back to New Layer, got %r" % (history(app),))
        log("layer added, opacity 50 %%, undone (History %s)" % (history(app),))

        # 4b: the Move tool moves the Photo layer WHILE dragging (canvas
        # updates before the release), one "Move" History state after it.
        app.click("#__azphoto_layer-row-1")
        app.click("#__azphoto_tool-move")
        steps = history(app)[0]
        mx = float(canvas["x"]) + float(canvas["width"]) * 0.5
        my = float(canvas["y"]) + float(canvas["height"]) * 0.5
        app.must("mouse_move", x=mx, y=my)
        app.must("mouse_down", x=mx, y=my)
        app.frame(1)
        before = len(app.printed("AZPHOTO_UPDATE", r"-?\d+ -?\d+ \d+ \d+"))
        for i in range(1, 6):
            app.must("mouse_move", x=mx + i * 8.0, y=my + i * 4.0)
            app.frame(1)
        live = len(app.printed("AZPHOTO_UPDATE", r"-?\d+ -?\d+ \d+ \d+")) - before
        if live == 0:
            raise Failure("the move drag did not redraw the canvas before the release (no live preview)")
        if history(app)[0] != steps:
            raise Failure("the move drag recorded History before the release: %s" % (history(app),))
        app.must("mouse_up", x=mx + 40.0, y=my + 20.0)
        app.frame(3)
        app.until("the Move in the History", lambda: (history(app) or (0, 0, ""))[2] == "Move")
        if history(app)[0] != steps + 1:
            raise Failure("the move should be ONE History state: %s" % (history(app),))
        log("live move: %d canvas updates while dragging, one Move state" % live)

        # 4c: the Text tool on azul's text raster (RawImage::from_text): a
        # click starts a text, typing sets it as a live layer, Enter places it.
        app.click("#__azphoto_tool-text")
        app.must("mouse_move", x=mx - 200.0, y=my - 100.0)
        app.must("mouse_down", x=mx - 200.0, y=my - 100.0)
        app.must("mouse_up", x=mx - 200.0, y=my - 100.0)
        app.frame(3)
        app.until("the text field", lambda: app.has_id("__azphoto_text-field"))
        app.must("focus_node", selector="#__azphoto_text-field")
        app.frame(1)
        app.must("text_input", text="Hello")
        app.frame(3)
        app.key("enter")
        app.until("the Text in the History", lambda: (history(app) or (0, 0, ""))[2] == "Text")
        layers = app.last("AZPHOTO_LAYERS", r"\d+ .*") or ""
        if not layers.startswith("5 "):
            raise Failure("the text should be a fifth layer, got %r" % layers)
        app.screenshot(os.path.join(out, "azphoto-text.png"))
        log("text placed: %s" % layers)

        # 5: export into the data tree, beside the document.
        app.key("e", primary=True, shift=True)
        app.until("the export sheet", lambda: app.shows("Export"))
        app.click("#__azphoto_sheet-ok")
        exported = app.until("the export", lambda: app.last("AZPHOTO_EXPORTED", r"\d+ .+"))
        size, key = exported.split(" ", 1)
        if not key.startswith("photo/") or "/exports/" not in key:
            raise Failure("the export is not in the data tree: %s" % key)
        path = os.path.join(data, *key.split("/"))
        with open(path, "rb") as f:
            head = f.read(8)
        if head != PNG_MAGIC or int(size) < 1000:
            raise Failure("the export is not a PNG (%s, %s bytes)" % (path, size))
        log("exported %s (%s bytes)" % (path, size))

        # 6: save the document as files.
        app.key("s", primary=True)
        saved = app.until("the save", lambda: app.last("AZPHOTO_SAVED", r"\S+ \d+"))
        uuid, tiles = saved.split(" ")
        folder = os.path.join(data, "photo", uuid)
        if not os.path.isfile(os.path.join(folder, "doc.json")):
            raise Failure("no doc.json in %s" % folder)
        with open(os.path.join(folder, "doc.json"), "r", encoding="utf-8") as f:
            saved_doc = json.load(f)
        pngs = [os.path.join(dp, fn) for dp, _, fns in os.walk(os.path.join(folder, "layers")) for fn in fns]
        if len(pngs) != int(tiles) or not pngs:
            raise Failure("saved %s tiles but found %d files" % (tiles, len(pngs)))
        if saved_doc.get("width") != 1920 or len(saved_doc.get("layers", [])) != 5:
            raise Failure("doc.json does not describe the document: %s" % json.dumps(saved_doc)[:300])
        log("saved photo/%s: doc.json + %d tiles" % (uuid, len(pngs)))

        # 7: the look in both themes and modes.
        app.screenshot(os.path.join(out, "azphoto-flat-light.png"))
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(4)
        app.screenshot(os.path.join(out, "azphoto-flora-dark.png"))
        log("PASS: sample, stroke (dirty rects), layer, opacity, undo, live move, text, export, save; "
            "screenshots in %s" % out)
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
    parser.add_argument("--debug-port", type=int, default=8781)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--width", type=int, default=1400)
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--out")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azphoto-e2e-")
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    passed = False
    try:
        passed = run(args, logs, out)
    except Failure:
        passed = False
    finally:
        log("logs in %s" % logs)
        if passed and not args.out:
            shutil.rmtree(os.path.join(logs, "data"), ignore_errors=True)
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
