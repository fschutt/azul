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
    4. a second loop is lifted by pressing and holding inside it (no grip: `WBDEMO_HELD`, then `WBDEMO_LIFTED`)
       and dropped lower on the bar;
    5. an arrow is drawn from the rim of one cutout to the other; double click on a cutout jumps to its
       lines (`WBDEMO_JUMP <file> <first> <last> Fresh`);
    6. Delete takes the selected cutout and its arrow off the board in one step, Mod+Z brings both back,
       Shift+Mod+Z takes them off again, Mod+Z once more (the board is kept after each);
    7. a label is typed under the selected cutout (double click under it) and on the arrow (double click on its
       line); the text area takes the keyboard by itself; leaving it keeps the label;
    8. the restart: a second demo on the same data folder reads the board back (`WBDEMO_LOADED 3`: the
       two cutouts and the arrow, with their labels).

With `--window cpu` (or `gpu`) it runs in a real window, at the display's scale, and the screenshots are the
window as the system composited it (macOS: `screencapture -l` of the demo's own window, which also works while
the display sleeps; the app's own capture then comes back white). The window is as large as the screen allows,
the bar lies against its right edge. Run ONE app at a time:

    python3 scripts/azwhiteboard_e2e.py --bin target/debug/examples/whiteboard_demo --out /tmp/azwhiteboard-shots
"""

import json
import math
import os
import re
import subprocess
import sys
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


def window_number(pid):
    """The window server's number of the largest window of process `pid` (macOS, through the system's own
    JavaScript for Automation: no Python module is needed), or None."""
    script = (
        'ObjC.import("CoreGraphics");'
        'var l = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionAll, 0)));'
        'l.filter(function (w) { return w.kCGWindowOwnerPID === %d && w.kCGWindowLayer === 0; })'
        '.map(function (w) { return w.kCGWindowNumber + " " + (w.kCGWindowBounds.Width * w.kCGWindowBounds.Height); })'
        '.join("\\n");' % pid
    )
    try:
        done = subprocess.run(["osascript", "-l", "JavaScript", "-e", script], capture_output=True, text=True, timeout=20)
    except (OSError, subprocess.SubprocessError):
        return None
    best = None
    for line in done.stdout.split("\n"):
        parts = line.split()
        if len(parts) == 2 and (best is None or float(parts[1]) > best[1]):
            best = (int(parts[0]), float(parts[1]))
    return best[0] if best else None


def shoot(app, path):
    """A picture of the window. A real window is the system's capture of the demo's own window; where that cannot
    be had (headless, another system) it is the harness's."""
    if e2e.BACKEND != "headless" and sys.platform == "darwin":
        app.settle()
        number = window_number(app.process.pid)
        if number is not None:
            done = subprocess.run(["screencapture", "-x", "-o", "-l", str(number), path],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if done.returncode == 0 and os.path.exists(path) and os.path.getsize(path) > 0:
                app.log("screenshot %s (%d bytes, screencapture of window %d)" % (path, os.path.getsize(path), number))
                return
    app.screenshot(path)


def double_click(app, x, y):
    app.must("mouse_move", x=x, y=y)
    app.frame(1)
    for _ in range(2):
        app.must("mouse_down", x=x, y=y)
        app.must("mouse_up", x=x, y=y)
    app.frame(2)


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

    def carry(self, from_page, to_window, hold=0.0, shot=None):
        """Presses at a point of the page, holds, and drags to a point of the window, in steps; with `shot`, the
        window is shot on the way (the carried outline, a third of the way) and over the target (the bar's frame)."""
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
            if shot and i == steps // 3:
                self.app.frame(2)
                shoot(self.app, shot % "on-the-way")
        if shot:
            self.app.frame(2)
            shoot(self.app, shot % "over-the-bar")
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


def perf_body(args, logs, out):
    """The frame times of a full board (`WBE2E_PERF=n`: `n` cutouts and the arrows between them, `WBDEMO_FILL`):
    a cutout dragged over the bar, the board panned by the wheel; the demo prints its own surface and layout
    times (`WBDEMO_PERF`), which are what this run reports."""
    binary = find_binary(args)
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    argv = ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en"]
    fill = os.environ.get("WBE2E_PERF", "200")
    app = e2e.App("whiteboard-perf", binary, argv, args.debug_port, logs, args.timeout,
                  extra_env={"WBDEMO_FILL": fill, "WBDEMO_PROFILE": "1"})
    try:
        app.until("the demo's window", lambda: app.has_id(SURFACE) and app.has_id(PAGE))
        app.frame(3)
        filled = wait_for(app, "WBDEMO_FILLED", r"\d+", "the full board")
        demo = Demo(app)
        # A dab on the page starts the demo's clock (its 100 ms timer prints the times every two seconds).
        demo.draw([(600.0, 400.0), (601.0, 400.0), (602.0, 401.0)])
        bar = app.rect(SURFACE)
        first = app.rect("__azreview_whiteboard-cutout-1")
        if not bar or not first:
            raise Failure("no bar or no first cutout: %s %s" % (bar, first))
        cx, cy = first["x"] + first["width"] / 2.0, first["y"] + first["height"] / 2.0
        shoot(app, os.path.join(out, "p1-full.png"))
        # The first cutout carried over the board, to and fro, a frame each step.
        app.must("mouse_move", x=cx, y=cy)
        app.must("mouse_down", x=cx, y=cy)
        for i in range(1, 61):
            t = math.sin(i / 60.0 * math.pi * 2.0)
            app.must("mouse_move", x=cx + 90.0 * t, y=cy + 60.0 * (1.0 - math.cos(i / 60.0 * math.pi * 2.0)) / 2.0)
            app.frame(1)
        app.must("mouse_up", x=cx, y=cy)
        app.frame(2)
        # The board panned by the wheel.
        mx, my = bar["x"] + bar["width"] / 2.0, bar["y"] + bar["height"] / 2.0
        for i in range(24):
            app.must("wheel", x=mx, y=my, delta_x=0.0, delta_y=-60.0 if i % 2 == 0 else 60.0)
            app.frame(2)
        shoot(app, os.path.join(out, "p2-panned.png"))
        time.sleep(5.0)
        lines = app.printed("WBDEMO_PERF", r".+")
        if not lines:
            raise Failure("the demo printed no frame times")
        for line in lines[-6:]:
            print("[azwhiteboard] perf (board of %s): %s" % (filled, line))
    finally:
        app.stop()
    print("[azwhiteboard] PERF DONE")
    return True


def body(args, logs, out):
    if os.environ.get("WBE2E_PERF"):
        return perf_body(args, logs, out)
    binary = find_binary(args)
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    argv = ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en"]
    app = e2e.App("whiteboard", binary, argv, args.debug_port, logs, args.timeout)
    try:
        app.until("the demo's window", lambda: app.has_id(SURFACE) and app.has_id(PAGE))
        app.frame(3)
        demo = Demo(app)
        shoot(app, os.path.join(out, "1-empty.png"))

        # A quick slash across a word is no loop.
        demo.draw([(300.0, 107.0), (320.0, 106.0), (340.0, 105.0), (360.0, 104.0), (380.0, 103.0), (400.0, 102.0)])
        if app.count("WBDEMO_ARMED") != 0:
            raise Failure("a slash armed a region")

        # A loop round the code of lines 5 to 7: armed, its grip lifts it onto the bar.
        loop = ellipse(330.0, 104.0, 90.0, 26.0)
        demo.draw(loop)
        armed = wait_for(app, "WBDEMO_ARMED", r"\d+ \d+", "the loop arms its region")
        shoot(app, os.path.join(out, "2-armed.png"))
        grip = max(loop, key=lambda p: p[0] - p[1])
        bar = app.rect(SURFACE)
        if not bar or bar.get("width", 0) < 100:
            raise Failure("the bar has no box: %s" % bar)
        # The bar lies against the window's right edge, and the demo's idea of it (where a drop is measured
        # from) is that edge: the surface says where it is.
        win = app.value("get_state").get("window_state", {})
        if abs(bar["x"] + bar["width"] - float(win.get("logical_width", bar["x"] + bar["width"]))) > 1.5:
            raise Failure("the bar is not against the window's right edge: %s in a window %s wide" % (bar, win.get("logical_width")))
        bar_x = bar["x"] + bar["width"] / 2.0
        low = min(520.0, bar["y"] + bar["height"] - 90.0)
        before = app.count("WBDEMO_DROPPED")
        demo.carry(grip, (bar_x, 220.0), shot=os.path.join(out, "3-%s.png"))
        app.until("the drop", lambda: app.count("WBDEMO_DROPPED") > before)
        size = wait_for(app, "WBDEMO_RENDERED", r"\d+x\d+", "the cutout drawn by the CPU renderer")
        w, h = (int(v) for v in size.split("x"))
        if w < 100 or h < 40:
            raise Failure("the cutout's picture is %s: too small to be the loop's region" % size)
        saved = wait_for(app, "WBDEMO_SAVED", r".+", "the board kept")
        app.frame(3)
        shoot(app, os.path.join(out, "4-dropped.png"))
        path = board_file(data_dir)
        manifest, pictures = read_board(path)
        cutouts = [i for i in manifest["items"] if i["kind"] == "cutout"]
        if len(cutouts) != 1 or not pictures:
            raise Failure("the kept board does not hold one cutout and its picture: %s" % [i["kind"] for i in manifest["items"]])
        if not pictures[cutouts[0]["image"]].startswith(b"\x89PNG"):
            raise Failure("the cutout's picture is no PNG")
        print("[azwhiteboard] armed %s, cutout %s, kept as %s" % (armed, size, os.path.basename(saved)))

        # A second loop, lifted by pressing and holding inside it: no grip.
        loop2 = ellipse(330.0, 170.0, 90.0, 26.0)
        demo.draw(loop2)
        app.until("the second loop arms", lambda: app.count("WBDEMO_ARMED") >= 2)
        before = app.count("WBDEMO_DROPPED")
        demo.carry((330.0, 170.0), (bar_x - 20, low), hold=0.6)
        app.until("the second drop", lambda: app.count("WBDEMO_DROPPED") > before)
        if app.count("WBDEMO_HELD") != 1:
            raise Failure("the second loop was not lifted by the hold: %d holds" % app.count("WBDEMO_HELD"))
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
        shoot(app, os.path.join(out, "5-arrow.png"))

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
        shoot(app, os.path.join(out, "6-jumped.png"))
        print("[azwhiteboard] jump: %s" % jump)

        # Delete takes the selected cutout and the arrow on it off the board as ONE step; Mod+Z brings both back,
        # Shift+Mod+Z takes them off again, and one more Mod+Z leaves the board as it was kept.
        def items():
            return len(read_board(board_file(data_dir))[0]["items"])

        app.key("Delete")
        app.until("the cutout and its arrow are off the board", lambda: items() == 1)
        shoot(app, os.path.join(out, "7-deleted.png"))
        app.key("z", primary=True)
        app.until("one step back brings both", lambda: items() == 3)
        app.key("z", primary=True, shift=True)
        app.until("one step forward takes them off again", lambda: items() == 1)
        app.key("z", primary=True)
        app.until("and back for the restart", lambda: items() == 3)
        shoot(app, os.path.join(out, "8-restored.png"))

        # A label under a cutout: the selected cutout has its label zone under it, a double click there types the
        # label (the text area takes the keyboard when it mounts), leaving the area keeps it.
        def label_of(kind, nth=0):
            found = [i for i in read_board(board_file(data_dir))[0]["items"] if i["kind"] == kind]
            return (found[nth].get("label"), found[nth]["id"]) if len(found) > nth else (None, None)

        first, second = (app.rect("__azreview_whiteboard-cutout-%d" % i) for i in (1, 2))
        cx, cy = first["x"] + first["width"] / 2.0, first["y"] + first["height"] / 2.0
        app.must("mouse_move", x=cx, y=cy)
        app.must("mouse_down", x=cx, y=cy)
        app.must("mouse_up", x=cx, y=cy)
        app.frame(2)
        double_click(app, cx, first["y"] + first["height"] + 8.0)
        area = "__azreview_whiteboard-label-%d" % label_of("cutout", 0)[1]
        app.until("the label's text area", lambda: app.has_id(area))
        app.frame(2)
        focus = app.value("get_focus_state") or {}
        print("[azwhiteboard] autofocus: %s" % ("the label's area has the keyboard" if focus.get("has_focus") else "NO node has the keyboard"))
        app.text_input("#" + area, "the parser")
        shoot(app, os.path.join(out, "8a-typing.png"))
        app.key("Tab")
        app.until("the cutout's label kept", lambda: label_of("cutout", 0)[0] == "the parser")
        # ... and on an arrow, on its line (the straight line from where it starts to the second cutout's middle).
        sx, sy = first["x"] + first["width"] - 3.0, first["y"] + first["height"] / 2.0
        ex, ey = second["x"] + second["width"] / 2.0, second["y"] + second["height"] / 2.0
        double_click(app, sx + 0.3 * (ex - sx), sy + 0.3 * (ey - sy))
        arrow_area = "__azreview_whiteboard-label-%d" % label_of("arrow", 0)[1]
        app.until("the arrow's text area", lambda: app.has_id(arrow_area))
        app.text_input("#" + arrow_area, "calls")
        app.key("Tab")
        app.until("the arrow's label kept", lambda: label_of("arrow", 0)[0] == "calls")
        app.frame(2)
        shoot(app, os.path.join(out, "8b-labelled.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # The restart: the same data folder, the board comes back.
    app = e2e.App("whiteboard2", binary, argv, args.debug_port, logs, args.timeout)
    try:
        app.until("the second window", lambda: app.has_id(SURFACE))
        app.expect_line("WBDEMO_LOADED", "3", "the board read back (two cutouts and the arrow)")
        app.frame(3)
        for i in (1, 2):
            if not app.rect("__azreview_whiteboard-cutout-%d" % i):
                raise Failure("cutout %d did not come back" % i)
        app.until("the labels are on the restarted board", lambda: {"the parser", "calls"} <= set(app.texts()))
        shoot(app, os.path.join(out, "9-restarted.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    print("[azwhiteboard] PASS: a loop became a cutout, two cutouts an arrow, the board was kept and came back")
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
