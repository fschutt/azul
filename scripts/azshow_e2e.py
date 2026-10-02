#!/usr/bin/env python3
"""AzShow end to end, headless over the debug server.

    1. starts AzShow headless (AZ_BACKEND=headless, the debug server on --debug-port) on File > New
       with an empty data folder (AZSHOW_DATA), sizes the window to --width x --height;
    2. creates a deck from the default theme (Create) and checks it is open (AZSHOW_DECK);
    3. adds a slide with HOME > New Slide and one with INSERT > the "Two Content" layout cell
       (AZSHOW_SLIDES 2 2, then 3 3), checks the rail lists three slides;
    4. double-clicks the title placeholder of the current slide, types a title, leaves the text with
       Escape and checks the title is in the tree (the canvas and the rail's preview);
    5. inserts a rectangle (INSERT > Rectangle), drags it 150 px to the right and checks the committed
       frame moved by 150 / scale slide units (AZSHOW_FRAME);
    6. opens the slide sorter (VIEW > Slide Sorter), drags slide 1 onto slide 3 and checks the order
       changed (AZSHOW_ORDER); when the engine's drag and drop does not deliver the drop headlessly it
       says so and reorders with Ctrl+Down on the focused thumbnail instead (the same Move);
    7. back in the normal view, saves with Ctrl+S and checks show/<id>/deck.json holds three slides and
       the typed title;
    8. starts the show with F5 (without the presenter window unless --presenter), steps with Space and
       Right through every shown slide to "End of slide show" (AZSHOW_SHOW_ENDED), checks the
       AZSHOW_SHOW lines, closes it with Escape (AZSHOW_SHOW_CLOSED);
    9. takes a screenshot after each step (--out), and one in flora + dark.

Usage (from the azul repository, after building libazul with the debug server and AzShow):

    python3 scripts/azshow_e2e.py [--bin target/release/AzShow] [--debug-port 8781]
        [--timeout 180] [--width 1280] [--height 800] [--out <dir>] [--presenter] [--keep-logs]

`AZSHOW_BIN` also names the binary. Run it through the capped runner (one app at a time).
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
    print("[azshow] %s" % line, flush=True)


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
    if os.environ.get("AZSHOW_BIN"):
        candidates.append(os.environ["AZSHOW_BIN"])
    for root in repo_roots():
        for sub in ("release", "debug"):
            candidates.append(os.path.join(root, "target", sub, "AzShow"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no AzShow binary; pass --bin or set AZSHOW_BIN (tried %s)" % candidates)


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
    """AzShow under its debug server."""

    def __init__(self, binary, args, port, env, logs, deadline):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "azshow.stdout")
        self.err_path = os.path.join(logs, "azshow.stderr")
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

    def printed(self, key, pattern=r".*"):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^%s ?(%s)$" % (re.escape(key), pattern), text, re.M)

    def until(self, what, check, interval=0.25, deadline=None):
        last = None
        end = deadline or self.deadline
        while time.time() < end:
            if self.process.poll() is not None:
                raise Failure("AzShow exited (%s) while waiting for %s" % (self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def soon(self, what, check, seconds=6.0):
        """`until` with a short deadline; None instead of a failure."""
        try:
            return self.until(what, check, deadline=time.time() + seconds)
        except Failure:
            return None

    def new_lines(self, key, before, pattern=r".*"):
        return self.until("%s" % key, lambda: self.printed(key, pattern)[before:] or None)

    def rect(self, selector):
        value = self.value("get_node_layout", selector=selector)
        r = (value or {}).get("rect") or {}
        return {k: float(r.get(k, 0)) for k in ("x", "y", "width", "height")}

    def click_text(self, text):
        self.must("click", text=text)
        self.frame(2)

    def key(self, key, shift=False, ctrl=False, meta=False):
        mods = {"shift": shift, "ctrl": ctrl, "alt": False, "meta": meta}
        self.must("key_down", key=key, modifiers=mods)
        self.must("key_up", key=key, modifiers=mods)
        self.frame(2)

    def drag(self, x0, y0, x1, y1, steps=8):
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

    def screenshot(self, path):
        value = self.value("take_screenshot")
        data = value.get("data") if isinstance(value, dict) else None
        if not isinstance(data, str) or "base64," not in data:
            log("take_screenshot returned no PNG (%s)" % json.dumps(value)[:120])
            return
        with open(path, "wb") as f:
            f.write(base64.b64decode(data.split("base64,", 1)[1]))
        log("screenshot %s (%d bytes)" % (path, os.path.getsize(path)))


def slide_box(app):
    """The slide's rectangle on screen and its scale (px per slide unit)."""
    r = app.rect("#azshow-slide")
    if r["width"] <= 0:
        raise Failure("the slide (#azshow-slide) is not laid out: %s" % r)
    return r, r["width"] / 1920.0


def run(args, logs, out):
    binary = find_binary(args.bin)
    deadline = time.time() + args.timeout
    data_root = os.path.join(logs, "data")
    os.makedirs(data_root, exist_ok=True)
    env = dict(os.environ)
    env.update({"AZ_BACKEND": "headless", "AZ_DEBUG": str(args.debug_port), "AZSHOW_DATA": data_root})
    argv = ["--screen", "backstage-new"]
    if not args.presenter:
        argv.append("--no-presenter")
    app = App(binary, argv, args.debug_port, env, logs, deadline)
    shot = lambda name: app.screenshot(os.path.join(out, name + ".png"))
    try:
        app.until("AzShow's window", lambda: app.printed("AZSHOW_READY") and app.shows("Create"))
        app.must("resize", width=args.width, height=args.height)
        app.frame(2)
        shot("01-new")

        # ---- a new deck ----
        app.click_text("Create")
        deck_id = app.until("the new deck", lambda: app.printed("AZSHOW_DECK", r"\S+"))[-1]
        log("deck %s" % deck_id)
        app.until("the normal view", lambda: app.shows("SLIDE 1 OF 1"))

        # ---- slides with layouts ----
        before = len(app.printed("AZSHOW_SLIDES", r"\d+ \d+"))
        app.click_text("New Slide")
        app.until("a second slide", lambda: app.printed("AZSHOW_SLIDES", r"\d+ \d+")[before:])
        app.click_text("INSERT")
        app.click_text("Two Content")
        slides = app.until("a third slide", lambda: [l for l in app.printed("AZSHOW_SLIDES", r"\d+ \d+") if l.startswith("3 ")])
        log("slides: %s" % slides[-1])
        app.until("the status bar's slide count", lambda: app.shows("SLIDE 3 OF 3"))
        shot("02-three-slides")

        # ---- type a title into the title placeholder ----
        box, scale = slide_box(app)
        title_x = box["x"] + (120 + 1680 / 2) * scale
        title_y = box["y"] + (60 + 160 / 2) * scale
        app.must("double_click", x=title_x, y=title_y)
        app.frame(3)
        time.sleep(0.3)
        app.frame(2)
        app.must("text_input", text="Quarterly plan")
        app.frame(3)
        app.key("escape")
        app.until("the typed title", lambda: app.shows("Quarterly plan"))
        log("typed the title")
        shot("03-title")

        # ---- insert a rectangle and move it ----
        app.click_text("INSERT")
        app.click_text("Rectangle")
        app.frame(2)
        box, scale = slide_box(app)
        cx = box["x"] + (760 + 200) * scale
        cy = box["y"] + (390 + 150) * scale
        before = len(app.printed("AZSHOW_FRAME", r".+"))
        app.drag(cx, cy, cx + 150, cy)
        frames = app.until("the committed move", lambda: app.printed("AZSHOW_FRAME", r".+")[before:])
        parts = frames[-1].split()
        moved_x = float(parts[1])
        want = 760 + 150 / scale
        if abs(moved_x - want) > 12:
            raise Failure("the rectangle went to x=%s, expected about %.0f (scale %.3f)" % (moved_x, want, scale))
        log("moved the rectangle to x=%s" % moved_x)
        shot("04-moved")

        # ---- reorder in the slide sorter ----
        app.click_text("VIEW")
        app.click_text("Slide Sorter")
        app.until("the sorter", lambda: app.printed("AZSHOW_VIEW", r"Slide Sorter"))
        app.frame(2)
        shot("05-sorter")
        items = app.value("get_node_layout", selector=".__azul-native-thumbnail-strip-item")
        thumbs = []
        for d in dicts(app.op("get_all_nodes_layout")):
            classes = d.get("classes") or []
            if "__azul-native-thumbnail-strip-item" in classes and isinstance(d.get("rect"), dict):
                thumbs.append({k: float(d["rect"].get(k, 0)) for k in ("x", "y", "width", "height")})
        if len(thumbs) < 3:
            # Fall back on the first item's rectangle and the grid's step.
            first = (items or {}).get("rect") or {}
            x0, y0 = float(first.get("x", 40)), float(first.get("y", 200))
            w0 = float(first.get("width", 240))
            thumbs = [{"x": x0 + i * (w0 + 8), "y": y0, "width": w0, "height": 140} for i in range(3)]
        center = lambda r: (r["x"] + r["width"] / 2, r["y"] + r["height"] / 2)
        before = len(app.printed("AZSHOW_ORDER", r".+"))
        (x1, y1), (x3, y3) = center(thumbs[0]), center(thumbs[2])
        app.drag(x1, y1, x3, y3, steps=12)
        order = app.soon("the drop", lambda: app.printed("AZSHOW_ORDER", r".+")[before:])
        if order:
            log("drag and drop reordered the slides: %s" % order[-1])
        else:
            log("NOTE: no drop arrived headlessly; reordering with Ctrl+Down on the thumbnail")
            app.must("click", x=x1, y=y1)
            app.frame(2)
            app.key("down", ctrl=True)
            order = app.until("the keyboard move", lambda: app.printed("AZSHOW_ORDER", r".+")[before:])
            log("Ctrl+Down reordered the slides: %s" % order[-1])
        shot("06-reordered")
        app.click_text("VIEW")
        app.click_text("Normal")
        app.frame(2)

        # ---- save ----
        before = len(app.printed("AZSHOW_SAVED", r"\S+"))
        app.key("s", ctrl=True, meta=sys.platform == "darwin")
        app.until("the save", lambda: app.printed("AZSHOW_SAVED", r"\S+")[before:])
        path = os.path.join(data_root, "show", deck_id, "deck.json")
        with open(path, "r", encoding="utf-8") as f:
            deck = json.load(f)
        if deck.get("format") != "azshow.deck" or len(deck.get("slides", [])) != 3:
            raise Failure("deck.json is not the three-slide deck: %s" % json.dumps(deck)[:300])
        if "Quarterly plan" not in json.dumps(deck):
            raise Failure("deck.json does not hold the typed title")
        log("saved %s (%d slides)" % (path, len(deck["slides"])))

        # ---- the show ----
        before = len(app.printed("AZSHOW_SHOW", r"\d+ \d+"))
        app.key("f5")
        app.until("the show", lambda: app.printed("AZSHOW_SHOW", r"\d+ \d+")[before:])
        shot("07-show")
        for key in ("space", "right", "space"):
            app.key(key)
        app.until("the end of the show", lambda: app.printed("AZSHOW_SHOW_ENDED"))
        steps = app.printed("AZSHOW_SHOW", r"\d+ \d+")[before:]
        log("show steps: %s" % steps)
        if len(steps) < 3:
            raise Failure("the show did not step through the slides: %s" % steps)
        app.key("escape")
        app.until("the show to close", lambda: app.printed("AZSHOW_SHOW_CLOSED"))

        # ---- the other theme and mode ----
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        shot("08-flora-dark")
        app.must("set_theme", theme="flat")
        app.must("set_mode", mode="light")
        app.frame(2)
        log("PASS: deck, layouts, title, move, reorder, save, show; screenshots in %s" % out)
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
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--out")
    parser.add_argument("--presenter", action="store_true", help="also open the presenter window")
    parser.add_argument("--keep-logs", action="store_true")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azshow-e2e-")
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    passed = False
    try:
        passed = run(args, logs, out)
    except Failure:
        passed = False
    finally:
        if passed and not args.keep_logs and not args.out:
            shutil.rmtree(os.path.join(logs, "data"), ignore_errors=True)
        log("logs in %s" % logs)
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
