#!/usr/bin/env python3
"""The whiteboard end to end, in its demo window (`cargo run --example whiteboard_demo`).

    1. starts the demo on a fresh data folder: a page of code on the left, the bar of the board on the
       right (`__azreview_demo-page`, `__azreview_whiteboard-surface`);
    2. a loop drawn round some code with the mouse arms its region (`WBDEMO_ARMED <first> <last>`); a
       stroke that is no loop (a quick slash across a word) arms nothing;
    3. the region is lifted by its grip, dragged onto the bar and dropped (`WBDEMO_LIFTED`,
       `WBDEMO_DROPPED`); its cutout is drawn on a Thread by the CPU renderer (`WBDEMO_RENDERED <w>x<h>`:
       the pixels of the window's scale) and the board is kept (`WBDEMO_SAVED <file>`): a zip with
       board.json and one PNG per cutout;
    4. a second loop is lifted by pressing and holding inside it (no grip) and dropped lower on the bar;
    5. an arrow is drawn from the rim of one cutout to the other; double click on a cutout jumps to its
       lines (`WBDEMO_JUMP <file> <first> <last> Fresh`);
    6. the restart: a second demo on the same data folder reads the board back (`WBDEMO_LOADED 2`).

With `--window cpu` (or `gpu`) it runs in a real window, at the display's scale, and the screenshots are the
window as the system composited it. Run ONE app at a time:

    python3 scripts/azwhiteboard_e2e.py --bin target/debug/examples/whiteboard_demo --out /tmp/azwhiteboard-shots
"""

import json
import math
import os
import re
import time
import zipfile

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azwhiteboard"
WIDTH, HEIGHT = 1400, 800
PAGE = "__azreview_demo-page"
SURFACE = "__azreview_whiteboard-surface"
BAR_W = 340


def ellipse(cx, cy, rx, ry, n=48, turns=0.97, start=-100.0):
    pts = []
    for i in range(n):
        a = math.radians(start + 360.0 * turns * i / (n - 1))
        pts.append((cx + rx * math.cos(a), cy + ry * math.sin(a)))
    return pts


def board_file(data_dir):
    folder = os.path.join(data_dir, "review", "boards")
    names = sorted(os.listdir(folder)) if os.path.isdir(folder) else []
    return os.path.join(folder, names[-1]) if names else None


def read_board(path):
    with zipfile.ZipFile(path) as z:
        manifest = json.loads(z.read("board.json").decode("utf-8"))
        pictures = {n: z.read(n) for n in z.namelist() if n.endswith(".png")}
    return manifest, pictures


class Demo:
    def __init__(self, app):
        self.app = app

    def page(self):
        r = self.app.rect(PAGE)
        if not r or r.get("width", 0) < 100:
            raise Failure("the demo's page has no box: %s" % r)
        return r

    def draw(self, points, pause=0.0):
        """A stroke over the page: `points` are the page's own px."""
        page = self.page()
        win = [(page["x"] + x, page["y"] + y) for x, y in points]
        self.app.must("mouse_move", x=win[0][0], y=win[0][1])
        self.app.must("mouse_down", x=win[0][0], y=win[0][1])
        if pause:
            time.sleep(pause)
        for x, y in win[1:]:
            self.app.must("mouse_move", x=x, y=y)
            self.app.frame(1)
        self.app.must("mouse_up", x=win[-1][0], y=win[-1][1])
        self.app.frame(2)

    def carry(self, from_page, to_window, hold=0.0):
        """Presses at a point of the page, holds, and drags to a point of the window, in steps."""
        page = self.page()
        x0, y0 = page["x"] + from_page[0], page["y"] + from_page[1]
        self.app.must("mouse_move", x=x0, y=y0)
        self.app.must("mouse_down", x=x0, y=y0)
        if hold:
            time.sleep(hold)
        steps = 12
        for i in range(1, steps + 1):
            t = i / steps
            self.app.must("mouse_move", x=x0 + (to_window[0] - x0) * t, y=y0 + (to_window[1] - y0) * t)
            self.app.frame(1)
        self.app.must("mouse_up", x=to_window[0], y=to_window[1])
        self.app.frame(3)


def find_binary(args):
    if args.bin:
        return args.bin
    for root in e2e.repo_roots():
        for sub in ("debug", "release"):
            path = os.path.join(root, "target", sub, "examples", "whiteboard_demo")
            if os.path.isfile(path):
                return path
    raise Failure("no whiteboard_demo binary; pass --bin (cargo build -p AzReview --example whiteboard_demo)")


def wait_for(app, key, pattern, what):
    app.until(what, lambda: app.count(key, pattern) > 0)
    return app.printed(key, pattern)[-1]


def body(args, logs, out):
    binary = find_binary(args)
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    argv = ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en"]
    app = e2e.App("whiteboard", binary, argv, args.debug_port, logs, args.timeout)
    try:
        app.until("the demo's window", lambda: app.has_id(SURFACE) and app.has_id(PAGE))
        app.frame(3)
        demo = Demo(app)
        app.screenshot(os.path.join(out, "1-empty.png"))

        # A quick slash across a word is no loop.
        demo.draw([(300.0, 107.0), (320.0, 106.0), (340.0, 105.0), (360.0, 104.0), (380.0, 103.0), (400.0, 102.0)])
        if app.count("WBDEMO_ARMED") != 0:
            raise Failure("a slash armed a region")

        # A loop round the code of lines 5 to 7: armed, its grip lifts it onto the bar.
        loop = ellipse(330.0, 104.0, 90.0, 26.0)
        demo.draw(loop)
        armed = wait_for(app, "WBDEMO_ARMED", r"\d+ \d+", "the loop arms its region")
        app.screenshot(os.path.join(out, "2-armed.png"))
        grip = max(loop, key=lambda p: p[0] - p[1])
        bar_x = WIDTH - BAR_W // 2
        before = app.count("WBDEMO_DROPPED")
        demo.carry(grip, (bar_x, 220.0))
        app.until("the drop", lambda: app.count("WBDEMO_DROPPED") > before)
        size = wait_for(app, "WBDEMO_RENDERED", r"\d+x\d+", "the cutout drawn by the CPU renderer")
        w, h = (int(v) for v in size.split("x"))
        if w < 100 or h < 40:
            raise Failure("the cutout's picture is %s: too small to be the loop's region" % size)
        saved = wait_for(app, "WBDEMO_SAVED", r".+", "the board kept")
        app.frame(3)
        app.screenshot(os.path.join(out, "3-dropped.png"))
        path = board_file(data_dir)
        manifest, pictures = read_board(path)
        cutouts = [i for i in manifest["items"] if i["kind"] == "cutout"]
        if len(cutouts) != 1 or not pictures:
            raise Failure("the kept board does not hold one cutout and its picture: %s" % [i["kind"] for i in manifest["items"]])
        if not pictures[cutouts[0]["image"]].startswith(b"\x89PNG"):
            raise Failure("the cutout's picture is no PNG")
        print("[azwhiteboard] armed %s, cutout %s, kept as %s" % (armed, size, os.path.basename(saved)))

        # A second loop, lifted by pressing and holding inside it: no grip.
        loop2 = ellipse(330.0, 236.0, 90.0, 26.0)
        demo.draw(loop2)
        app.until("the second loop arms", lambda: app.count("WBDEMO_ARMED") >= 2)
        before = app.count("WBDEMO_DROPPED")
        demo.carry((330.0, 236.0), (bar_x - 20, 520.0), hold=0.6)
        app.until("the second drop", lambda: app.count("WBDEMO_DROPPED") > before)
        app.until("the second picture", lambda: app.count("WBDEMO_RENDERED") >= 2)
        app.until("the board kept again", lambda: len(read_board(board_file(data_dir))[0]["items"]) == 2)
        app.frame(3)

        # An arrow from the rim of the first cutout to the second.
        first, second = (app.rect("__azreview_whiteboard-cutout-%d" % i) for i in (1, 2))
        if not first or not second:
            raise Failure("the cutouts have no boxes: %s %s" % (first, second))
        x0, y0 = first["x"] + first["width"] - 3.0, first["y"] + first["height"] / 2.0
        x1, y1 = second["x"] + second["width"] / 2.0, second["y"] + second["height"] / 2.0
        app.drag(x0, y0, x1, y1, steps=10)
        app.until("the arrow kept", lambda: any(i["kind"] == "arrow" for i in read_board(board_file(data_dir))[0]["items"]))
        app.screenshot(os.path.join(out, "4-arrow.png"))

        # A double click on the first cutout jumps back to its lines.
        cx, cy = first["x"] + first["width"] / 2.0, first["y"] + first["height"] / 2.0
        app.must("mouse_move", x=cx, y=cy)
        for _ in range(2):
            app.must("mouse_down", x=cx, y=cy)
            app.must("mouse_up", x=cx, y=cy)
        jump = wait_for(app, "WBDEMO_JUMP", r".+", "the jump")
        if not re.search(r"Fresh$", jump):
            raise Failure("the jump says %r, the file is as it was" % jump)
        app.frame(2)
        app.screenshot(os.path.join(out, "5-jumped.png"))
        print("[azwhiteboard] jump: %s" % jump)
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # The restart: the same data folder, the board comes back.
    app = e2e.App("whiteboard2", binary, argv, args.debug_port, logs, args.timeout)
    try:
        app.until("the second window", lambda: app.has_id(SURFACE))
        app.expect_line("WBDEMO_LOADED", "2", "the board read back")
        app.frame(3)
        for i in (1, 2):
            if not app.rect("__azreview_whiteboard-cutout-%d" % i):
                raise Failure("cutout %d did not come back" % i)
        app.screenshot(os.path.join(out, "6-restarted.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    print("[azwhiteboard] PASS: a loop became a cutout, two cutouts an arrow, the board was kept and came back")
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
