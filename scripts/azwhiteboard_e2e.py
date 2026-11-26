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
    8. the second cutout is carried by its body and resized by its corner handle (one step each, undone by Mod+Z);
    9. the boards: the header's list opens, a new board opens empty, is renamed (the name typed, Enter), board 1 is
       opened again with its cutouts, duplicated (the copy's zip holds its cutouts), one deleted (a bar says so)
       and brought back by Undo, one moved up; the list is kept in the data folder;
   10. the export: the list's Export opens a sheet that names the author and asks where the pictures go; the save
       dialog is answered from the mock store; the file is the publishable document (`azboard/1`): the repository's
       revision, the path in the repository, the lines, the SHA-256 of them, a link to them at the revision, the pictures
       (inside the file, or in a folder beside it, the very PNGs of the board's zip);
   11. the import: the exported file (and the one with its pictures beside it) become new boards with the same cutouts, a
       file of a newer format and one that is no board are told and change nothing;
   12. the history: a snapshot asks why (a sheet that says what changed since the last, a text area for the reason, a field
       for tags) and keeps the board with the reason, the code's revision and a SHA-256 any RFC 8785 library reproduces; a second
       one says what moved; the list shows them newest first; one is looked at read-only with its reason on a banner (the
       pointer changes nothing), set against its parent, restored as a snapshot of its own; the whole history exported and
       imported again, and a snapshot changed after it was written is told apart;
   13. the restart: a second demo on the same data folder reads the board back (`WBDEMO_LOADED 3`: the
       two cutouts and the arrow, with their labels, and its three snapshots);
   14. the code moves on: in a git repository of its own (the demo's page is a file of it, read from the disk each time
       the cutouts are looked for) two cutouts are made and a snapshot taken; lines are put above them and one is edited, a
       commit moves HEAD; the demo asks whether to take a new snapshot (with what was done since as the start of the
       reason), flags the cutout whose code moved and the one whose code changed, and the snapshot records both with the
       lines, hashes and revision a script checks against `git show`.

The pen is not driven here: the debug server's pen_down / pen_move / pen_up only move the mouse state (the engine does
not inject a pen state yet, see DebugEvent::PenDown), so `PointerKind::Pen` (a stroke from empty space is an arrow,
not a pan) is covered by the unit tests of whiteboard_input and by a hand with a pen.

With `--window cpu` (or `gpu`) it runs in a real window, at the display's scale, and the screenshots are the
window as the system composited it (macOS: `screencapture -l` of the demo's own window, which also works while
the display sleeps; the app's own capture then comes back white). The window is as large as the screen allows,
the bar lies against its right edge. Run ONE app at a time:

    python3 scripts/azwhiteboard_e2e.py --bin target/debug/examples/whiteboard_demo --out /tmp/azwhiteboard-shots
"""

import base64
import hashlib
import json
import math
import os
import re
import shutil
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


def board_file(data_dir, board="b1"):
    """The zip of board `board` (a board of the list has its id in its file's name; the file of the one-board
    version, which is board b1 in place, has none)."""
    folder = os.path.join(data_dir, "review", "boards")
    names = sorted(os.listdir(folder)) if os.path.isdir(folder) else []
    own = [n for n in names if n.endswith(".%s.azboard.zip" % board)]
    legacy = [n for n in names if n.endswith(".azboard.zip") and not re.search(r"\.b\d+\.azboard\.zip$", n)]
    pick = own or (legacy if board == "b1" else [])
    return os.path.join(folder, pick[-1]) if pick else None


def read_index(data_dir):
    """The list of boards as the demo keeps it (`azreview-boards/1`), or None."""
    folder = os.path.join(data_dir, "review", "boards")
    names = [n for n in sorted(os.listdir(folder))] if os.path.isdir(folder) else []
    found = [n for n in names if n.endswith(".azboards.json")]
    if not found:
        return None
    with open(os.path.join(folder, found[-1]), encoding="utf-8") as f:
        return json.load(f)


def shown_boards(data_dir):
    index = read_index(data_dir) or {}
    return [(b["id"], b["name"]) for b in index.get("boards", []) if "deleted_ms" not in b]


def read_board(path):
    with zipfile.ZipFile(path) as z:
        manifest = json.loads(z.read("board.json").decode("utf-8"))
        pictures = {n: z.read(n) for n in z.namelist() if n.endswith(".png")}
    return manifest, pictures


def picture_bytes(manifest, pictures, item):
    """The PNG of a cutout of a board's manifest (`azboard/1`): its asset's name in the zip."""
    asset = manifest["assets"][item["image"]["asset"]]
    return pictures[asset["src"]]


def git(root, *args):
    """What git says in `root`, or None (no git, no repository)."""
    try:
        done = subprocess.run(["git", "-C", root] + list(args), capture_output=True, text=True, timeout=20)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return done.stdout.strip() if done.returncode == 0 else None


def web_of(remote):
    """`https://github.com/owner/repo` of a GitHub remote, as the app writes it; None for another host."""
    m = re.match(r"^(?:https?://(?:[^@/]+@)?|ssh://git@|git@)github\.com[:/]([^/]+/[^/]+?)(?:\.git)?/?$", remote or "")
    return "https://github.com/%s" % m.group(1) if m else None


def recent(ms):
    """Whether `ms` is a Unix time in milliseconds of the last few days (what a board and a snapshot are dated with; the
    time since the panel started is a small number, and a date of 1970 on a snapshot's banner is what it looks like)."""
    return isinstance(ms, (int, float)) and abs(time.time() * 1000.0 - ms) < 3 * 86400 * 1000.0


def check_document(doc, board_name, author, repo, pictures_from):
    """What a published board must hold. `repo` is {rev, web} of the checkout the demo was pointed at (or None);
    `pictures_from(asset)` gives the PNG bytes an asset names."""
    if doc.get("format") != "azboard/1":
        raise Failure("the document's format is %r" % doc.get("format"))
    if doc["board"]["name"] != board_name or doc.get("author") != author:
        raise Failure("the document names board %r by %r, expected %r by %r" % (doc["board"]["name"], doc.get("author"), board_name, author))
    for key in ("created_ms", "modified_ms"):
        if not recent(doc["board"].get(key)):
            raise Failure("the board's %s is %r: not a Unix time of the last days" % (key, doc["board"].get(key)))
    if doc["canvas"].get("unit") != "px" or "view" not in doc["canvas"]:
        raise Failure("the canvas has no unit or view: %s" % doc["canvas"])
    if not isinstance(doc.get("snapshots"), list):
        raise Failure("the document has no history list")
    cutouts = [i for i in doc["items"] if i["kind"] == "cutout"]
    arrows = [i for i in doc["items"] if i["kind"] == "arrow"]
    if len(cutouts) != 2 or len(arrows) != 1:
        raise Failure("the document should hold two cutouts and an arrow: %s" % [i["kind"] for i in doc["items"]])
    if repo:
        r = doc["workspace"].get("repo") or {}
        if r.get("rev") != repo["rev"]:
            raise Failure("the document's revision is %r, the checkout's HEAD is %r" % (r.get("rev"), repo["rev"]))
        if repo["web"] and r.get("web_url") != repo["web"]:
            raise Failure("the document's web address is %r, expected %r" % (r.get("web_url"), repo["web"]))
    for c in cutouts:
        s = c["source"]
        digest = "sha256:" + hashlib.sha256(("\n".join(s["code"]) + "\n").encode("utf-8")).hexdigest()
        if s.get("hash") != digest:
            raise Failure("a cutout's hash %r is not the SHA-256 of the code it holds (%s)" % (s.get("hash"), digest))
        if s["line_start"] < 1 or s["line_end"] < s["line_start"] or len(s["code"]) != s["line_end"] - s["line_start"] + 1:
            raise Failure("a cutout's lines %s-%s do not match its %d lines of code" % (s["line_start"], s["line_end"], len(s["code"])))
        if s.get("language") != "rust":
            raise Failure("the language of a Rust file is %r" % s.get("language"))
        if repo:
            if s["path"] != "examples/azul-review/src/whiteboard_demo.rs" or s.get("rev") != repo["rev"]:
                raise Failure("a cutout names %r at %r" % (s["path"], s.get("rev")))
            if repo["web"]:
                lines = "L%d" % s["line_start"] if s["line_start"] == s["line_end"] else "L%d-L%d" % (s["line_start"], s["line_end"])
                want = "%s/blob/%s/%s#%s" % (repo["web"], repo["rev"], s["path"], lines)
                if s.get("permalink") != want:
                    raise Failure("a cutout's link is %r, expected %r" % (s.get("permalink"), want))
        png = pictures_from(doc["assets"][c["image"]["asset"]])
        if not png.startswith(b"\x89PNG"):
            raise Failure("a cutout's picture is no PNG")
    return cutouts


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


def code_moves(args, logs, out, binary, root):
    """The code moves on: references found again, flagged, recorded; the question when HEAD moves."""
    if git(root, "--version") is None and subprocess.run(["git", "--version"], capture_output=True).returncode != 0:
        print("[azwhiteboard] no git here: the steps of the code that moves on are skipped")
        return
    work = os.path.join(logs, "moves")
    os.makedirs(work, exist_ok=True)
    shutil.copy(os.path.join(root, "examples", "azul-review", "src", "whiteboard_demo.rs"), os.path.join(work, "whiteboard_demo.rs"))

    def g(*a):
        subprocess.run(["git", "-C", work, "-c", "user.name=E2E", "-c", "user.email=e2e@example.org", "-c", "commit.gpgsign=false"] + list(a), check=True, capture_output=True)

    g("init", "-q")
    g("add", "-A")
    g("commit", "-q", "-m", "the page, as it is")
    r1 = git(work, "rev-parse", "HEAD")
    material = os.path.join(logs, "material.txt")
    data = os.path.join(logs, "data3")
    os.makedirs(data, exist_ok=True)
    env = {"WBDEMO_FILES": work, "WBDEMO_WORKSPACE": work, "WBDEMO_AUTHOR": "E2E Author", "WBDEMO_MATERIAL": material}
    argv = ["--data-dir", data, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en"]
    app = e2e.App("whiteboard3", binary, argv, args.debug_port, logs, args.timeout, extra_env=env)
    try:
        app.until("the demo's window", lambda: app.has_id(SURFACE) and app.has_id(PAGE))
        app.frame(3)
        wait_for(app, "WBDEMO_REPO", re.escape(r1) + r" .*", "the demo read the new repository's revision")
        demo = Demo(app)
        bar = app.rect(SURFACE)
        bar_x = bar["x"] + bar["width"] / 2.0
        # Two cutouts, the second lifted by its hold.
        loop = ellipse(330.0, 104.0, 90.0, 26.0)
        demo.draw(loop)
        wait_for(app, "WBDEMO_ARMED", r"\d+ \d+", "the first loop arms")
        grip = max(loop, key=lambda p: p[0] - p[1])
        demo.carry(grip, (bar_x, 220.0))
        app.until("the first cutout is drawn", lambda: app.count("WBDEMO_RENDERED") >= 1)
        demo.draw(ellipse(330.0, 170.0, 90.0, 26.0))
        app.until("the second loop arms", lambda: app.count("WBDEMO_ARMED") >= 2)
        demo.carry((330.0, 170.0), (bar_x - 20, min(520.0, bar["y"] + bar["height"] - 90.0)), hold=0.6)
        app.until("the second cutout is drawn", lambda: app.count("WBDEMO_RENDERED") >= 2)
        armed = [tuple(int(v) for v in a.split()) for a in app.printed("WBDEMO_ARMED", r"\d+ \d+")]
        (a1, b1), (a2, b2) = armed[0], armed[1]
        app.until("both are kept", lambda: len(read_board(board_file(data, "b1"))[0]["items"]) == 2)
        # The first snapshot, at the first revision.
        def sheet_snapshot(why, shot):
            app.click("#__azreview_whiteboard-snapshot")
            app.until("the sheet", lambda: app.has_id("__azreview_whiteboard-sheet") and app.has_id("__azreview_whiteboard-why"))
            app.frame(2)
            if why:
                app.text_input("#__azreview_whiteboard-why", why)
                app.key("Tab")
            shoot(app, os.path.join(out, shot))
            before = len(read_board(board_file(data, "b1"))[0].get("snapshots", []))
            app.click("#__azreview_whiteboard-sheet-save")
            app.until("the snapshot is kept", lambda: len(read_board(board_file(data, "b1"))[0].get("snapshots", [])) == before + 1)
            return read_board(board_file(data, "b1"))[0]["snapshots"][-1]

        s1 = sheet_snapshot("Before the code moves.", "12a-before-the-move.png")
        if not recent(s1.get("taken_ms")):
            raise Failure("the first snapshot is dated %r: not a Unix time of the last days" % s1.get("taken_ms"))
        if s1["code"].get("rev") != r1 or any(i["source"].get("rev") != r1 for i in s1["items"] if i["kind"] == "cutout"):
            raise Failure("the first snapshot is not at the first revision: %s" % json.dumps(s1["code"]))
        # The code moves on: seven lines above, the last line of the second cutout edited; a commit.
        path = os.path.join(work, "whiteboard_demo.rs")
        with open(path, encoding="utf-8") as f:
            lines = f.read().split("\n")
        lines[b2 - 1] = lines[b2 - 1] + "  // edited by the e2e"
        lines = ["// put here by the e2e: line %d" % i for i in range(7)] + lines
        with open(path, "w", encoding="utf-8") as f:
            f.write("\n".join(lines))
        with open(material, "w", encoding="utf-8") as f:
            f.write("Conversation \"rename the page\": done\nPatch rename.patch applied\n")
        g("add", "-A")
        g("commit", "-q", "-m", "seven lines above, one line edited")
        r2 = git(work, "rev-parse", "HEAD")
        wait_for(app, "WBDEMO_HEAD", re.escape(r2), "the demo noticed that HEAD moved")
        app.until("the question", lambda: app.has_id("__azreview_whiteboard-head"))
        words = " ".join(app.texts())
        if "moved from %s to %s" % (r1[:7], r2[:7]) not in words or "2 things that were done" not in words:
            raise Failure("the question does not say what moved: %s" % words[:500])
        inside_the_bar(app, ["head-take", "head-dismiss"], "the question")
        shoot(app, os.path.join(out, "12b-the-question.png"))
        app.click("#__azreview_whiteboard-head-take")
        app.until("the sheet", lambda: app.has_id("__azreview_whiteboard-sheet"))
        app.until("with the flags on the cutouts", lambda: app.has_id("__azreview_whiteboard-flag-1") and app.has_id("__azreview_whiteboard-flag-2"))
        words = " ".join(app.texts())
        if "1 moved, 1 changed, 0 gone" not in words:
            raise Failure("the sheet does not say what the code did: %s" % words[:600])
        inside_the_bar(app, ["sheet-save", "sheet-cancel"], "the sheet")
        shoot(app, os.path.join(out, "12c-flags-and-sheet.png"))
        app.click("#__azreview_whiteboard-sheet-save")
        app.until("the second snapshot is kept", lambda: len(read_board(board_file(data, "b1"))[0].get("snapshots", [])) == 2)
        manifest, _ = read_board(board_file(data, "b1"))
        s2 = manifest["snapshots"][-1]
        if s2["code"].get("rev") != r2 or s2.get("parent") != "s1":
            raise Failure("the second snapshot is not at the second revision: %s" % json.dumps({k: s2.get(k) for k in ("parent", "code")}))
        if not s2["why"].startswith("Since the last snapshot:\n- Conversation \"rename the page\": done\n- Patch rename.patch applied"):
            raise Failure("the reason does not start with what was done: %r" % s2["why"])
        refs = {r["id"]: r for r in s2["summary"]["references"]}
        if sorted(refs) != [1, 2] or refs[1]["status"] != "moved" or refs[2]["status"] != "changed":
            raise Failure("the references the snapshot records are %s" % json.dumps(s2["summary"]["references"]))
        if refs[1]["from"] != [a1, b1] or refs[1]["to"] != [a1 + 7, b1 + 7] or refs[2]["from"] != [a2, b2] or refs[2]["to"] != [a2 + 7, b2 + 7]:
            raise Failure("the lines are %s, expected %s -> %s and %s -> %s" % (json.dumps(refs), [a1, b1], [a1 + 7, b1 + 7], [a2, b2], [a2 + 7, b2 + 7]))
        # What git says of the lines at the second revision is what the items hold: the text, the hash, the revision.
        shown = subprocess.run(["git", "-C", work, "show", "%s:whiteboard_demo.rs" % r2], capture_output=True, text=True).stdout.split("\n")
        for item in (i for i in s2["items"] if i["kind"] == "cutout"):
            src = item["source"]
            want = shown[src["line_start"] - 1:src["line_end"]]
            digest = "sha256:" + hashlib.sha256(("\n".join(want) + "\n").encode("utf-8")).hexdigest()
            if src["code"] != want or src["hash"] != digest or src.get("rev") != r2:
                raise Failure("cutout %d does not hold the lines %d-%d at %s: %s" % (item["id"], src["line_start"], src["line_end"], r2[:7], json.dumps(src)[:300]))
        if s2["hash"] != snapshot_hash_of(s2):
            raise Failure("the second snapshot does not hash to its hash")
        app.until("the question is answered", lambda: not app.has_id("__azreview_whiteboard-head"))
        app.until("and the flags are off the board", lambda: not app.has_id("__azreview_whiteboard-flag-1"))
        # Looked at: the snapshot says what the code did, in the same flags.
        app.click("#__azreview_whiteboard-history")
        app.click("#__azreview_whiteboard-snap-s2")
        app.until("the snapshot shows the flags it recorded", lambda: app.has_id("__azreview_whiteboard-flag-1") and app.has_id("__azreview_whiteboard-flag-2"))
        shoot(app, os.path.join(out, "12d-the-snapshot-flags.png"))
        app.click("#__azreview_whiteboard-back")
        # A third revision: asked again, "not now" is said once.
        with open(path, "a", encoding="utf-8") as f:
            f.write("\n// and one more line\n")
        g("add", "-A")
        g("commit", "-q", "-m", "one more line")
        r3 = git(work, "rev-parse", "HEAD")
        wait_for(app, "WBDEMO_HEAD", re.escape(r3), "the demo noticed the next revision")
        app.until("asked again for the new revision", lambda: app.has_id("__azreview_whiteboard-head"))
        app.click("#__azreview_whiteboard-head-dismiss")
        app.until("not now is said", lambda: not app.has_id("__azreview_whiteboard-head"))
        app.frame(5)
        if app.has_id("__azreview_whiteboard-head"):
            raise Failure("the question came back for the revision that was said 'not now' to")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    print("[azwhiteboard] code that moves on: moved %s -> %s, changed %s -> %s, snapshot at %s" % ([a1, b1], [a1 + 7, b1 + 7], [a2, b2], [a2 + 7, b2 + 7], r2[:7]))


def inside_the_bar(app, stems, what):
    """Every button `__azreview_whiteboard-<stem>` lies inside the bar: a button that runs out of it cannot be pressed."""
    bar = app.box("#" + SURFACE)
    for stem in stems:
        box = app.box("#__azreview_whiteboard-%s" % stem)
        if box["x"] < bar["x"] - 0.5 or box["x"] + box["width"] > bar["x"] + bar["width"] + 0.5:
            raise Failure("%s: the button %s lies at x %.0f..%.0f, the bar at %.0f..%.0f" % (what, stem, box["x"], box["x"] + box["width"], bar["x"], bar["x"] + bar["width"]))


def snapshot_hash_of(snapshot):
    """What any RFC 8785 (JCS) library gives for the snapshot without its hash: sorted members, no space."""
    body = {k: v for k, v in snapshot.items() if k != "hash"}
    text = json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return "sha256:" + hashlib.sha256(text.encode("utf-8")).hexdigest()


def body(args, logs, out):
    if os.environ.get("WBE2E_PERF"):
        return perf_body(args, logs, out)
    binary = find_binary(args)
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    argv = ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en"]
    # The demo's page is its own source: its folder is where the repository is looked at (the checkout this script is in).
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    folder = os.path.join(root, "examples", "azul-review", "src")
    rev = git(root, "rev-parse", "HEAD")
    repo = {"rev": rev, "web": web_of(git(root, "remote", "get-url", "origin"))} if rev and os.path.isdir(folder) else None
    env = {"WBDEMO_WORKSPACE": folder, "WBDEMO_AUTHOR": "E2E Author"}
    app = e2e.App("whiteboard", binary, argv, args.debug_port, logs, args.timeout, extra_env=env)
    try:
        app.until("the demo's window", lambda: app.has_id(SURFACE) and app.has_id(PAGE))
        app.frame(3)
        if repo:
            wait_for(app, "WBDEMO_REPO", re.escape(repo["rev"]) + r" .*", "the demo read the checkout's revision")
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
        size = wait_for(app, "WBDEMO_RENDERED", r"\d+x\d+(?: \d+ms)?", "the cutout drawn by the CPU renderer")
        w, h = (int(v) for v in size.split()[0].split("x"))
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
        if manifest.get("format") != "azboard/1":
            raise Failure("the kept board is %r, not azboard/1" % manifest.get("format"))
        if not picture_bytes(manifest, pictures, cutouts[0]).startswith(b"\x89PNG"):
            raise Failure("the cutout's picture is no PNG")
        print("[azwhiteboard] armed %s, cutout %s, kept as %s" % (armed, size, os.path.basename(saved)))

        # A second loop, lifted by pressing and holding inside it: no grip.
        loop2 = ellipse(330.0, 170.0, 90.0, 26.0)
        demo.draw(loop2)
        app.until("the second loop arms", lambda: app.count("WBDEMO_ARMED") >= 2)
        before = app.count("WBDEMO_DROPPED")
        demo.carry((330.0, 170.0), (bar_x - 20, low), hold=0.6)
        app.until("the second drop", lambda: app.count("WBDEMO_DROPPED") > before)
        if app.count("WBDEMO_HELD") < 1:
            raise Failure("the second loop was not lifted by the hold: no WBDEMO_HELD (the demo's 100 ms timer did not tick?)")
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
        def label_of(item_id):
            found = [i for i in read_board(board_file(data_dir))[0]["items"] if i["id"] == item_id]
            return found[0].get("label") if found else None

        def arrow_id():
            found = [i["id"] for i in read_board(board_file(data_dir))[0]["items"] if i["kind"] == "arrow"]
            return found[0] if found else None

        first, second = (app.rect("__azreview_whiteboard-cutout-%d" % i) for i in (1, 2))
        cx, cy = first["x"] + first["width"] / 2.0, first["y"] + first["height"] / 2.0
        app.must("mouse_move", x=cx, y=cy)
        app.must("mouse_down", x=cx, y=cy)
        app.must("mouse_up", x=cx, y=cy)
        app.frame(2)
        double_click(app, cx, first["y"] + first["height"] + 8.0)
        area = "__azreview_whiteboard-label-1"
        app.until("the label's text area", lambda: app.has_id(area))
        app.frame(2)
        focus = app.value("get_focus_state") or {}
        has = next((d["has_focus"] for d in [focus] + list(e2e.dicts(focus)) if isinstance(d, dict) and "has_focus" in d), None)
        print("[azwhiteboard] autofocus: %s (%s)" % ("the label's area has the keyboard" if has else "NO node has the keyboard", json.dumps(focus)[:200]))
        app.text_input("#" + area, "the parser")
        shoot(app, os.path.join(out, "8a-typing.png"))
        app.key("Tab")
        app.until("the cutout's label kept", lambda: label_of(1) == "the parser")
        # ... and on an arrow, on its line (the straight line from where it starts to the second cutout's middle).
        sx, sy = first["x"] + first["width"] - 3.0, first["y"] + first["height"] / 2.0
        ex, ey = second["x"] + second["width"] / 2.0, second["y"] + second["height"] / 2.0
        double_click(app, sx + 0.3 * (ex - sx), sy + 0.3 * (ey - sy))
        arrow_area = "__azreview_whiteboard-label-%d" % arrow_id()
        app.until("the arrow's text area", lambda: app.has_id(arrow_area))
        app.text_input("#" + arrow_area, "calls")
        app.key("Tab")
        app.until("the arrow's label kept", lambda: label_of(arrow_id()) == "calls")
        app.frame(2)
        shoot(app, os.path.join(out, "8b-labelled.png"))

        # Moving and resizing: the body dragged carries the cutout, the handle at the corner of the selected one
        # resizes it with its proportions kept; each gesture is ONE step (Mod+Z puts it back).
        def rect_of(item_id):
            found = [i for i in read_board(board_file(data_dir))[0]["items"] if i["id"] == item_id]
            return tuple(float(found[0][k]) for k in ("x", "y", "width", "height")) if found else None

        def near(r, x, y):
            return r is not None and abs(r[0] - x) < 3.0 and abs(r[1] - y) < 3.0

        bar = app.rect(SURFACE)
        r0 = rect_of(2)
        second = app.rect("__azreview_whiteboard-cutout-2")
        mx, my = second["x"] + second["width"] / 2.0, second["y"] + second["height"] / 2.0
        app.drag(mx, my, mx - 40.0, my + 60.0, steps=8)
        app.until("the cutout carried and kept", lambda: near(rect_of(2), r0[0] - 40.0, r0[1] + 60.0))
        r1 = rect_of(2)
        hx, hy = bar["x"] + r1[0] + r1[2], bar["y"] + r1[1] + r1[3]
        app.drag(hx, hy, hx + 30.0, hy + 20.0, steps=6)
        app.until("the cutout resized and kept", lambda: (rect_of(2) or r1)[2] > r1[2] + 12.0)
        r2 = rect_of(2)
        if abs(r2[2] / r2[3] - r1[2] / r1[3]) > 0.03:
            raise Failure("the resize did not keep the proportions: %s -> %s" % (r1, r2))
        shoot(app, os.path.join(out, "8c-resized.png"))
        app.key("z", primary=True)
        app.until("the resize is one step back", lambda: rect_of(2) == r1)
        app.key("z", primary=True)
        app.until("and the carry is one step back", lambda: rect_of(2) == r0)

        # The boards: the header's list opens, a new board opens empty, is renamed, the first one is opened again
        # with its cutouts, duplicated; one is deleted and brought back by Undo, another moved up.
        def boards():
            return shown_boards(data_dir)

        def say(marker, pattern=r".*"):
            return app.count(marker, pattern)

        if [n for _, n in boards()] != ["Board 1"]:
            raise Failure("a fresh workspace should list one board, Board 1: %s" % boards())
        app.click("#__azreview_whiteboard-boards")
        app.until("the list of boards", lambda: app.has_id("__azreview_whiteboard-board-list"))
        shoot(app, os.path.join(out, "9a-list.png"))
        app.click("#__azreview_whiteboard-board-new")
        app.until("board 2 is open", lambda: say("WBDEMO_BOARDS", r"2 b2 .*") >= 1)
        app.until("and its list is kept", lambda: [i for i, _ in boards()] == ["b1", "b2"])
        app.until("and it is empty", lambda: not app.has_id("__azreview_whiteboard-cutout-1"))
        shoot(app, os.path.join(out, "9b-new-board.png"))
        # Renamed: the field takes the keyboard, the name typed over the old one, Enter keeps it.
        app.click("#__azreview_whiteboard-boards")
        app.click("#__azreview_whiteboard-board-b2-rename")
        field = "__azreview_whiteboard-board-b2-field"
        app.until("the name field", lambda: app.has_id(field))
        app.must("focus_node", selector="#" + field)
        app.key("a", primary=True)
        app.must("text_input", text="Ideas")
        app.key("Enter")
        app.until("the new name is kept", lambda: [n for _, n in boards()][1:] and [n for _, n in boards()][1].endswith("Ideas"))
        shoot(app, os.path.join(out, "9c-renamed.png"))
        # Board 1 again, read from its file: its cutouts are back.
        app.click("#__azreview_whiteboard-board-b1")
        app.until("board 1 is read", lambda: say("WBDEMO_OPENED", r"b1 3") >= 1 or app.has_id("__azreview_whiteboard-cutout-1"))
        app.until("with its cutouts", lambda: app.has_id("__azreview_whiteboard-cutout-1") and app.has_id("__azreview_whiteboard-cutout-2"))
        # A copy of the open board.
        app.click("#__azreview_whiteboard-boards")
        app.click("#__azreview_whiteboard-board-b1-duplicate")
        app.until("the copy is listed", lambda: [i for i, _ in boards()] == ["b1", "b2", "b3"])
        app.until("and its file is written", lambda: board_file(data_dir, "b3") is not None)
        copy_manifest, copy_pictures = read_board(board_file(data_dir, "b3"))
        if len([i for i in copy_manifest["items"] if i["kind"] == "cutout"]) != 2 or not copy_pictures:
            raise Failure("the copy does not hold the board's two cutouts and their pictures")
        # Board 2 deleted: the bar tells it, Undo brings it back.
        app.click("#__azreview_whiteboard-boards")
        app.click("#__azreview_whiteboard-board-b2-delete")
        app.until("the board is deleted", lambda: [i for i, _ in boards()] == ["b1", "b3"])
        app.until("the bar with Undo", lambda: app.has_id("__azreview_whiteboard-toast-undo"))
        shoot(app, os.path.join(out, "9d-deleted.png"))
        app.click("#__azreview_whiteboard-toast-undo")
        app.until("the board is back", lambda: [i for i, _ in boards()] == ["b1", "b2", "b3"])
        # The copy moved up: it is second now.
        app.click("#__azreview_whiteboard-boards")
        app.click("#__azreview_whiteboard-board-b3-up")
        app.until("the copy is moved", lambda: [i for i, _ in boards()] == ["b1", "b3", "b2"])
        shoot(app, os.path.join(out, "9e-list.png"))
        print("[azwhiteboard] boards: %s" % boards())

        # The export: the sheet names the author and asks where the pictures go; the save dialog is answered from the mock
        # store; what is written is the publishable document.
        site = os.path.join(logs, "export")
        os.makedirs(site, exist_ok=True)
        manifest1, zipped = read_board(board_file(data_dir, "b1"))
        zip_pngs = [picture_bytes(manifest1, zipped, c) for c in manifest1["items"] if c["kind"] == "cutout"]

        def open_list():
            """The list of boards open (a click on the header's name toggles it)."""
            if not app.has_id("__azreview_whiteboard-board-list"):
                app.click("#__azreview_whiteboard-boards")
            app.until("the list of boards", lambda: app.has_id("__azreview_whiteboard-board-list"))

        def export(name, beside):
            target = os.path.join(site, name)
            before = say("WBDEMO_EXPORTED")
            open_list()
            app.click("#__azreview_whiteboard-board-export")
            app.until("the export sheet", lambda: app.has_id("__azreview_whiteboard-export"))
            words = " ".join(app.texts())
            if "E2E Author" not in words or "JSON" not in words:
                raise Failure("the sheet does not name the author or the file: %s" % words[:400])
            if beside:
                app.click("#__azreview_whiteboard-export-beside")
            shoot(app, os.path.join(out, "10a-export-sheet.png" if not beside else "10b-export-sheet-beside.png") if name.startswith("Parser") else os.path.join(out, "11h-export-sheet-history.png"))
            app.scenario_assert("mock", set={"save_file": {"path": target}})
            app.click("#__azreview_whiteboard-export-go")
            app.until("the file is written", lambda: say("WBDEMO_EXPORTED") > before)
            app.until("and the sheet is gone", lambda: not app.has_id("__azreview_whiteboard-export"))
            return target

        embedded = export("Parser.azboard.json", False)
        with open(embedded, encoding="utf-8") as f:
            doc = json.load(f)

        def inline(asset):
            src = asset["src"]
            if not src.startswith("data:image/png;base64,"):
                raise Failure("the picture is not inside the file: %s" % src[:60])
            return base64.b64decode(src.split(",", 1)[1])

        check_document(doc, "Board 1", "E2E Author", repo, inline)
        embedded_pngs = [inline(doc["assets"][c["image"]["asset"]]) for c in doc["items"] if c["kind"] == "cutout"]
        if embedded_pngs != zip_pngs:
            raise Failure("the pictures in the file are not the PNGs of the board's zip")
        if any(not isinstance(i["id"], int) for i in doc["items"]) or doc["next_id"] <= max(i["id"] for i in doc["items"]):
            raise Failure("the items' ids are not stable integers below next_id")
        print("[azwhiteboard] exported: %d bytes, %d links, rev %s" % (os.path.getsize(embedded), sum(1 for i in doc["items"] if i["kind"] == "cutout" and i["source"].get("permalink")), (doc["workspace"].get("repo") or {}).get("rev", "-")))
        beside_json = export("Parser2.azboard.json", True)
        with open(beside_json, encoding="utf-8") as f:
            doc2 = json.load(f)

        def from_folder(asset):
            src = asset["src"]
            if src.startswith("data:") or ".." in src or src.startswith("/"):
                raise Failure("the picture is not in the folder beside the file: %s" % src[:60])
            with open(os.path.join(site, src), "rb") as f:
                return f.read()

        check_document(doc2, "Board 1", "E2E Author", repo, from_folder)
        if "base64" in open(beside_json, encoding="utf-8").read():
            raise Failure("the file with its pictures beside it holds a picture")

        # The import: the file with its pictures inside becomes a board, and so does the one with its pictures beside it;
        # a file of a newer format and one that is no board are told and change nothing.
        def import_file(path, fresh):
            app.scenario_assert("mock", set={"file_open": {"path": path}})
            open_list()
            app.click("#__azreview_whiteboard-board-import")
            if fresh:
                app.until("the file is read", lambda: say("WBDEMO_IMPORTED") >= fresh)
            return app.printed("WBDEMO_IMPORTED", r".+")[-1] if fresh else None

        said = import_file(embedded, 1)
        if not re.match(r"b4 Board_1 3 0 0$", said):
            raise Failure("the import said %r, expected b4 Board_1 3 0 0" % said)
        app.until("the imported board is listed and kept", lambda: [i for i, _ in boards()][-1:] == ["b4"] and board_file(data_dir, "b4") is not None)
        app.until("and open with its cutouts", lambda: app.has_id("__azreview_whiteboard-cutout-1") and app.has_id("__azreview_whiteboard-cutout-2"))
        shoot(app, os.path.join(out, "10c-imported.png"))
        imported, _ = read_board(board_file(data_dir, "b4"))

        def essence(manifest):
            """What makes a board that board: the items without the pictures' asset ids and the times."""
            return [{k: v for k, v in i.items() if k != "image"} for i in manifest["items"]]

        if essence(imported) != essence(manifest1):
            raise Failure("the imported board is not the exported one:\n%s\n%s" % (json.dumps(essence(imported))[:600], json.dumps(essence(manifest1))[:600]))
        said = import_file(beside_json, 2)
        if not re.match(r"b5 Board_1 3 0 0$", said):
            raise Failure("the import of the file with its pictures beside it said %r" % said)
        app.until("the second imported board is kept", lambda: board_file(data_dir, "b5") is not None)
        shoot(app, os.path.join(out, "10d-imported-beside.png"))
        # Told, not opened.
        newer = os.path.join(site, "newer.azboard.json")
        with open(newer, "w", encoding="utf-8") as f:
            f.write('{"format": "azboard/2", "items": []}')
        junk = os.path.join(site, "junk.azboard.json")
        with open(junk, "w", encoding="utf-8") as f:
            f.write("this is not a board")
        before_boards = boards()
        import_file(newer, 0)
        app.until("a newer format is told", lambda: say("WBDEMO_IMPORT_ERROR", r".*newer.*") >= 1)
        import_file(junk, 0)
        app.until("a file that is no board is told", lambda: say("WBDEMO_IMPORT_ERROR") >= 2)
        app.until("and said in the panel", lambda: any("could not be opened" in t for t in app.texts()))
        shoot(app, os.path.join(out, "10e-import-told.png"))
        if boards() != before_boards:
            raise Failure("a refused file changed the boards: %s -> %s" % (before_boards, boards()))
        # Back to board 1 for the restart.
        open_list()
        app.click("#__azreview_whiteboard-board-b1")
        app.until("board 1 is open again", lambda: say("WBDEMO_OPENED", r"b1 3") >= 2)

        # The history: a snapshot asks why (a sheet with what changed since the last one worked out, a text area for the
        # reason, a field for tags), the list of snapshots, one looked at read-only with its reason, set against its
        # parent, restored as a snapshot of its own; the whole history exported, imported, and one that was changed
        # after it was written told apart.
        def open_history():
            """The list of snapshots open (a click on the History button toggles it)."""
            if not app.has_id("__azreview_whiteboard-history-list"):
                app.click("#__azreview_whiteboard-history")
            app.until("the list of snapshots", lambda: app.has_id("__azreview_whiteboard-history-list"))

        def snapshots_of(board="b1"):
            manifest, _ = read_board(board_file(data_dir, board))
            return manifest.get("snapshots", [])

        def snapshot_hash(snapshot):
            """What any RFC 8785 (JCS) library gives for the snapshot without its hash: sorted members, no space."""
            body = {k: v for k, v in snapshot.items() if k != "hash"}
            text = json.dumps(body, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
            return "sha256:" + hashlib.sha256(text.encode("utf-8")).hexdigest()

        def take_snapshot(why, tags, shot):
            before = len(snapshots_of())
            app.click("#__azreview_whiteboard-snapshot")
            app.until("the sheet that asks why", lambda: app.has_id("__azreview_whiteboard-sheet") and app.has_id("__azreview_whiteboard-why"))
            app.frame(2)
            words = " ".join(app.texts())
            if "Why?" not in words or "Since" not in words and "The first snapshot" not in words:
                raise Failure("the sheet does not ask why or say what changed: %s" % words[:500])
            app.text_input("#__azreview_whiteboard-why", why)
            app.key("Tab")
            app.must("focus_node", selector="#__azreview_whiteboard-tags")
            app.must("text_input", text=tags)
            app.key("Tab")
            shoot(app, os.path.join(out, shot))
            app.click("#__azreview_whiteboard-sheet-save")
            app.until("the snapshot is kept", lambda: len(snapshots_of()) == before + 1)
            app.until("and the sheet is gone", lambda: not app.has_id("__azreview_whiteboard-sheet"))
            return snapshots_of()[-1]

        why1 = "The first cut: two boxes and an arrow between them."
        s1 = take_snapshot(why1, "ADR-7, #123", "11a-snapshot-sheet.png")
        if (s1["id"], s1.get("parent"), s1["why"], s1["tags"], s1.get("author")) != ("s1", None, why1, ["ADR-7", "#123"], "E2E Author"):
            raise Failure("the first snapshot is %s" % json.dumps({k: s1.get(k) for k in ("id", "parent", "why", "tags", "author")}))
        if s1["hash"] != snapshot_hash(s1):
            raise Failure("the snapshot's hash %s is not what a JCS library gives, %s" % (s1["hash"], snapshot_hash(s1)))
        if not recent(s1.get("taken_ms")):
            raise Failure("the snapshot is dated %r: not a Unix time of the last days" % s1.get("taken_ms"))
        if len(s1["summary"]["added"]) != 3 or s1["summary"]["moved"] or s1["summary"]["removed"]:
            raise Failure("the first snapshot's summary is %s" % json.dumps(s1["summary"]))
        if repo and (s1["code"].get("rev") != repo["rev"] or (repo["web"] and s1["code"]["repo"].get("web_url") != repo["web"])):
            raise Failure("the snapshot's code is %s, the checkout is at %s" % (json.dumps(s1["code"]), repo["rev"]))
        if len([i for i in s1["items"] if i["kind"] == "cutout"]) != 2 or not all(i["source"].get("hash") for i in s1["items"] if i["kind"] == "cutout"):
            raise Failure("the snapshot does not hold the two cutouts with their references")
        print("[azwhiteboard] snapshot s1: %s" % s1["hash"][:23])
        # The second cutout carried to the right (as the board was kept before): a second snapshot says what changed.
        live_before = app.rect("__azreview_whiteboard-cutout-2")
        r_before = rect_of(2)
        mx, my = live_before["x"] + live_before["width"] / 2.0, live_before["y"] + live_before["height"] / 2.0
        app.drag(mx, my, mx + 40.0, my + 50.0, steps=8)
        app.until("the cutout is carried and kept", lambda: near(rect_of(2), r_before[0] + 40.0, r_before[1] + 50.0))
        live_after = app.rect("__azreview_whiteboard-cutout-2")
        why2 = "The second box moved right and down: it is the solver's now, not the parser's."
        s2 = take_snapshot(why2, "", "11b-second-sheet.png")
        if (s2["id"], s2.get("parent")) != ("s2", "s1") or s2["hash"] != snapshot_hash(s2):
            raise Failure("the second snapshot is %s" % json.dumps({k: s2.get(k) for k in ("id", "parent")}))
        if [m["id"] for m in s2["summary"]["moved"]] != [2] or s2["summary"]["added"] or s2["summary"]["removed"]:
            raise Failure("what changed since the first is %s" % json.dumps(s2["summary"]))
        # The list: the newest first, each with what was said.
        open_history()
        words = app.texts()
        if not (any(w.startswith("s2  ") for w in words) and any(w.startswith("s1  ") for w in words) and why1 in words):
            raise Failure("the list does not show both snapshots with a reason: %s" % words)
        shoot(app, os.path.join(out, "11c-history-list.png"))
        # Looked at: its reason on a banner, the board as it was (the second cutout where it was), read-only.
        app.click("#__azreview_whiteboard-snap-s1")
        app.until("the banner", lambda: app.has_id("__azreview_whiteboard-viewing"))
        if why1 not in app.texts() or not any("read-only" in w for w in app.texts()):
            raise Failure("the banner does not say why and that it is read-only: %s" % app.texts())
        app.frame(2)
        inside_the_bar(app, ["compare", "restore", "back"], "the banner")
        seen = app.rect("__azreview_whiteboard-cutout-2")
        if abs(seen["x"] - live_before["x"]) > 1.5 or abs(seen["x"] - live_after["x"]) < 20.0:
            raise Failure("the snapshot shows the second cutout at %s, it was at %s and is at %s" % (seen["x"], live_before["x"], live_after["x"]))
        shoot(app, os.path.join(out, "11d-viewing.png"))
        # Nothing the pointer does changes it: a drag on the cutout leaves the board as it was kept.
        kept_before = rect_of(2)
        app.drag(seen["x"] + seen["width"] / 2.0, seen["y"] + seen["height"] / 2.0, seen["x"] + 100.0, seen["y"] + 80.0, steps=6)
        app.frame(3)
        if rect_of(2) != kept_before or len(snapshots_of()) != 2:
            raise Failure("looking at a snapshot changed the board or the history")
        # Set against its parent: what moved is told and drawn.
        app.click("#__azreview_whiteboard-back")
        app.until("back on the board", lambda: not app.has_id("__azreview_whiteboard-viewing"))
        open_history()
        app.click("#__azreview_whiteboard-snap-s2")
        app.until("the second snapshot is shown", lambda: app.has_id("__azreview_whiteboard-viewing"))
        app.click("#__azreview_whiteboard-compare")
        app.until("the comparison's legend", lambda: app.has_id("__azreview_whiteboard-compare-legend"))
        legend = [w for w in app.texts() if w.startswith("Against s1")]
        if not legend or "1 moved" not in legend[0]:
            raise Failure("the comparison says %s" % legend)
        app.frame(2)
        shoot(app, os.path.join(out, "11e-compare.png"))
        app.click("#__azreview_whiteboard-back")
        app.until("back on the board again", lambda: not app.has_id("__azreview_whiteboard-viewing"))
        # Restored: the old state is a new snapshot, and the board is what it was.
        open_history()
        app.click("#__azreview_whiteboard-snap-s1")
        app.until("the first snapshot is shown", lambda: app.has_id("__azreview_whiteboard-viewing"))
        app.click("#__azreview_whiteboard-restore")
        app.until("the sheet that restores", lambda: app.has_id("__azreview_whiteboard-sheet"))
        app.frame(2)
        shoot(app, os.path.join(out, "11f-restore-sheet.png"))
        app.click("#__azreview_whiteboard-sheet-save")
        app.until("the restore is a snapshot", lambda: len(snapshots_of()) == 3)
        s3 = snapshots_of()[-1]
        if (s3["id"], s3["parent"], s3["why"]) != ("s3", "s2", "Restored the board of snapshot s1.") or s3["hash"] != snapshot_hash(s3):
            raise Failure("the restoring snapshot is %s" % json.dumps({k: s3.get(k) for k in ("id", "parent", "why")}))
        app.until("and the board is what it was", lambda: near(rect_of(2), r_before[0], r_before[1]))
        if [s["id"] for s in snapshots_of()] != ["s1", "s2", "s3"] or snapshots_of()[0] != s1 or snapshots_of()[1] != s2:
            raise Failure("the snapshots that were there are not as they were")
        # The whole history exported: the snapshots in order, each as it was written; the pictures once.
        history_file = export("History.azboard.json", False)
        with open(history_file, encoding="utf-8") as f:
            hdoc = json.load(f)
        if [s["id"] for s in hdoc["snapshots"]] != ["s1", "s2", "s3"] or [s.get("parent") for s in hdoc["snapshots"]] != [None, "s1", "s2"]:
            raise Failure("the exported history is %s" % [(s["id"], s.get("parent")) for s in hdoc["snapshots"]])
        if any(s["hash"] != snapshot_hash(s) for s in hdoc["snapshots"]):
            raise Failure("a snapshot of the exported history does not hash to its hash")
        if sorted(hdoc["assets"]) != sorted(set("img-%d" % i["id"] for i in hdoc["items"] if i["kind"] == "cutout")):
            raise Failure("the pictures of the history are not each there once: %s" % sorted(hdoc["assets"]))
        # Imported: three snapshots, all of them checking; one changed after it was written is told apart.
        said = import_file(history_file, 3)
        if not re.match(r"b6 Board_1 3 0 0$", said):
            raise Failure("the import of the history said %r" % said)
        app.until("the history came with it", lambda: say("WBDEMO_HISTORY", r"3") >= 1)
        app.until("the imported board is kept", lambda: board_file(data_dir, "b6") is not None and len(snapshots_of("b6")) == 3)
        open_history()
        app.until("its list of snapshots", lambda: app.has_id("__azreview_whiteboard-snap-s3"))
        if any(app.has_id("__azreview_whiteboard-snap-s%d-unverified" % n) for n in (1, 2, 3)):
            raise Failure("a snapshot that was not changed says it does not check")
        app.click("#__azreview_whiteboard-history")
        changed = os.path.join(site, "Changed.azboard.json")
        with open(history_file, encoding="utf-8") as f:
            text = f.read()
        if why1 not in text:
            raise Failure("the first reason is not in the exported file")
        with open(changed, "w", encoding="utf-8") as f:
            f.write(text.replace(why1, "Nothing to see here."))
        import_file(changed, 4)
        app.until("the changed one is imported", lambda: say("WBDEMO_IMPORTED", r"b7 .*") >= 1)
        open_history()
        app.until("its list of snapshots", lambda: app.has_id("__azreview_whiteboard-snap-s1"))
        if not app.has_id("__azreview_whiteboard-snap-s1-unverified") or app.has_id("__azreview_whiteboard-snap-s2-unverified"):
            raise Failure("only the snapshot that was changed after it was written should say so")
        shoot(app, os.path.join(out, "11g-unverified.png"))
        app.click("#__azreview_whiteboard-history")
        # Back to board 1 for the restart.
        open_list()
        app.click("#__azreview_whiteboard-board-b1")
        app.until("board 1 is open for the restart", lambda: say("WBDEMO_OPENED", r"b1 3") >= 3)
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
        app.expect_line("WBDEMO_BOARDS", "7 b1 Board 1", "the list of boards read back, board 1 open")
        app.expect_line("WBDEMO_HISTORY", "3", "the three snapshots of board 1 read back")
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
    code_moves(args, logs, out, binary, root)
    print("[azwhiteboard] PASS: a loop became a cutout, two cutouts an arrow, the board was kept, exported, imported, given a history and came back")
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
