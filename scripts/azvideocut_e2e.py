#!/usr/bin/env python3
"""AzVideoCut end to end over the debug server.

    1. starts AzVideoCut headless with --sample (AZ_BACKEND=headless, the debug server on
       --debug-port, a fresh data root by --data-dir) and waits for the sample project
       (`AZVIDEOCUT_PROJECT <id> 4 200`: three clips on V1, a picture in picture on V2, 200
       frames) and its first program frame (`AZVIDEOCUT_FRAME 0`);
    2. checks the S3 layout: the media bin, the source and program monitors, the effect
       controls and the timeline lie inside the window and do not overlap; the timeline shows
       its clips (`__azul-native-timeline-clip`) and the playhead;
    3. moves the playhead: Shift+Right twice (two seconds, frame 50), Home, Right;
    4. cuts: Ctrl+K at frame 50 (the razor on every track: 4 -> 6 clips), then the razor tool
       (C) and a click on the first clip (6 -> 7 clips);
    5. ripple-deletes: the select tool (V), a click on the first clip, Shift+Delete (the
       sequence gets shorter, the clip count drops by one), Ctrl+Z (undone);
    6. exports the first second: Ctrl+E, "First second", "Export now", waits for
       `AZVIDEOCUT_EXPORTED <key> <bytes> <mp4|y4m|h264>` and checks the file under the data
       root (an MP4 starts with an `ftyp` box);
    7. switches to flora and dark and takes a screenshot.

Usage (from the azul repository, after building libazul with the debug server and AzVideoCut):

    python3 scripts/azvideocut_e2e.py [--bin target/release/AzVideoCut] [--debug-port 8772]
        [--timeout 180] [--width 1280] [--height 800] [--out <dir>] [--keep-data]

`AZVIDEOCUT_BIN` also names the binary. Run it through the capped runner (one app at a time).
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
    print("[azvideocut] %s" % line, flush=True)


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
    if os.environ.get("AZVIDEOCUT_BIN"):
        candidates.append(os.environ["AZVIDEOCUT_BIN"])
    for root in repo_roots():
        for sub in ("release", "debug"):
            candidates.append(os.path.join(root, "target", sub, "AzVideoCut"))
    for c in candidates:
        if c and os.path.isfile(c) and os.access(c, os.X_OK):
            return c
    raise Failure("no AzVideoCut binary; pass --bin or set AZVIDEOCUT_BIN (tried %s)" % candidates)


class App:
    """AzVideoCut under its debug server."""

    def __init__(self, binary, port, env, logs, deadline, data_root):
        self.port = port
        self.deadline = deadline
        self.out_path = os.path.join(logs, "azvideocut.stdout")
        self.err_path = os.path.join(logs, "azvideocut.stderr")
        self.process = subprocess.Popen(
            [binary, "--sample", "--data-dir", data_root], env=env, stdin=subprocess.DEVNULL,
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
        with urllib.request.urlopen(request, timeout=20) as response:
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

    def classes(self):
        out = []
        for d in dicts(self.op("get_node_hierarchy")):
            out.extend(d.get("classes") or [])
        return out

    def texts(self):
        return list(strings(self.op("get_node_hierarchy")))

    def printed(self, key, pattern=r".+"):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            return []
        return re.findall(r"^AZVIDEOCUT_%s (%s)$" % (re.escape(key), pattern), text, re.M)

    def until(self, what, check, interval=0.25):
        last = None
        while time.time() < self.deadline:
            if self.process.poll() is not None:
                raise Failure("AzVideoCut exited (%s) while waiting for %s" % (self.process.returncode, what))
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError, urllib.error.URLError) as e:
                last = e
            time.sleep(interval)
        raise Failure("timed out waiting for %s%s" % (what, " (last error: %s)" % last if last else ""))

    def next_line(self, key, pattern, what):
        """Waits for a new `AZVIDEOCUT_<key>` line after the ones seen so far; returns it."""
        before = len(self.printed(key, pattern))
        return lambda: self.until(what, lambda: self.printed(key, pattern)[before:] or None)[-1]

    def rect(self, selector):
        value = self.value("get_node_layout", selector=selector)
        return value.get("node_id"), value.get("rect") or {}

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

    def click(self, **target):
        self.must("click", **target)
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


def tail(path, lines=40):
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


def edit(app, name, do, what):
    """Runs `do` and returns (clips, frames) of the `AZVIDEOCUT_EDIT <name>` line it caused."""
    wait = app.next_line("EDIT", r"%s \d+ \d+" % re.escape(name), what)
    do()
    line = wait()
    _, clips, frames = line.split(" ")
    log("%s: %s clips, %s frames" % (what, clips, frames))
    return int(clips), int(frames)


def playhead(app, do, what):
    wait = app.next_line("PLAYHEAD", r"-?\d+", what)
    do()
    frame = int(wait())
    log("%s: playhead at %d" % (what, frame))
    return frame


def run(args, logs, out, data_root):
    binary = find_binary(args.bin)
    deadline = time.time() + args.timeout
    env = dict(os.environ)
    env.update({
        "AZ_BACKEND": "headless",
        "AZ_DEBUG": str(args.debug_port),
    })
    app = App(binary, args.debug_port, env, logs, deadline, data_root)
    try:
        # 1. The sample project and its first frame.
        line = app.until("the sample project", lambda: (app.printed("PROJECT", r"\S+ \d+ \d+") or [None])[-1])
        project_id, clips, frames = line.split(" ")
        if (int(clips), int(frames)) != (4, 200):
            raise Failure("the sample has %s clips and %s frames, not 4 and 200" % (clips, frames))
        sample = app.until("the sample's media", lambda: (app.printed("SAMPLE", r"\w+") or [None])[-1])
        log("project %s: %s clips, %s frames, clips %s" % (project_id, clips, frames, sample))
        app.until("the first program frame", lambda: "0" in app.printed("FRAME", r"\d+"))
        app.must("resize", width=args.width, height=args.height)
        app.frame(3)

        # 2. The S3 layout.
        rects = {}
        for slot in ("shell-media", "shell-source", "shell-program", "shell-inspector", "shell-timeline"):
            node, rect = app.rect("#%s" % slot)
            if node is None or not inside(rect, args.width, args.height):
                raise Failure("#%s is missing or outside the window: %s" % (slot, rect))
            rects[slot] = rect
        # The editor fills the window (2026-10-03 LOOK: the body was not
        # stretched, every pane collapsed to 0 px under the menu row).
        if float(rects["shell-program"].get("height", 0)) < 150:
            raise Failure("the program monitor is %s px tall - the editor does not fill the window"
                          % rects["shell-program"].get("height"))
        if float(rects["shell-timeline"].get("height", 0)) < 120:
            raise Failure("the timeline is %s px tall" % rects["shell-timeline"].get("height"))
        timeline_bottom = float(rects["shell-timeline"]["y"]) + float(rects["shell-timeline"]["height"])
        if timeline_bottom < args.height - 80:
            raise Failure("the timeline ends at %.0f of %d px - the window is not filled" % (timeline_bottom, args.height))
        row = ["shell-media", "shell-source", "shell-program", "shell-inspector"]
        for i, a in enumerate(row):
            for b in row[i + 1:]:
                if overlap(rects[a], rects[b]):
                    raise Failure("#%s and #%s overlap" % (a, b))
        classes = app.classes()
        clip_nodes = classes.count("__azul-native-timeline-clip")
        if clip_nodes != 4:
            raise Failure("the timeline shows %d clips, not 4" % clip_nodes)
        for cls in ("__azul-native-timeline-playhead", "__azul-native-timeline-ruler", "__azul-native-timeline-lanes"):
            if cls not in classes:
                raise Failure("the timeline has no %s" % cls)
        app.screenshot(os.path.join(out, "editor.png"))

        # 3. The playhead.
        at = playhead(app, lambda: app.key("right", shift=True), "Shift+Right")
        if at != 25:
            raise Failure("Shift+Right moved the playhead to %d, not 25" % at)
        at = playhead(app, lambda: app.key("right", shift=True), "Shift+Right again")
        if at != 50:
            raise Failure("the playhead is at %d, not 50" % at)

        # 4. Cuts: the razor on every track at the playhead, then the razor tool.
        clips, frames = edit(app, "Razor", lambda: app.key("k", primary=True), "Ctrl+K at frame 50")
        if (clips, frames) != (6, 200):
            raise Failure("after Ctrl+K: %d clips, %d frames (want 6, 200)" % (clips, frames))
        app.key("c")
        clips, _ = edit(app, "Razor", lambda: app.click(selector=".__azul-native-timeline-clip"),
                        "a razor click on the first clip")
        if clips != 7:
            raise Failure("the razor click left %d clips, not 7" % clips)
        app.screenshot(os.path.join(out, "razored.png"))

        # 5. Ripple delete the first clip, then undo it.
        app.key("v")
        app.click(selector=".__azul-native-timeline-clip")
        if "__azul-native-timeline-clip-selected" not in app.classes():
            raise Failure("a click did not select the clip")
        clips, short = edit(app, "Ripple_delete", lambda: app.key("delete", shift=True), "Shift+Delete")
        if clips != 6 or short >= 200:
            raise Failure("the ripple delete left %d clips and %d frames" % (clips, short))
        clips, frames = edit(app, "Undo_Ripple_delete", lambda: app.key("z", primary=True), "Ctrl+Z")
        if (clips, frames) != (7, 200):
            raise Failure("undo left %d clips and %d frames, not 7 and 200" % (clips, frames))

        at = playhead(app, lambda: app.key("home"), "Home")
        if at != 0:
            raise Failure("Home moved the playhead to %d" % at)
        at = playhead(app, lambda: app.key("right"), "Right")
        if at != 1:
            raise Failure("Right moved the playhead to %d" % at)

        # 6. Export the first second.
        app.key("e", primary=True)
        app.until("the export dialog", lambda: any("First second" in t for t in app.texts()))
        app.click(text="First second")
        wait = app.next_line("EXPORTED", r"\S+ \d+ \w+", "the export")
        app.click(text="Export now")
        line = wait()
        key, size, fmt = line.split(" ")
        path = os.path.join(data_root, key)
        if not os.path.isfile(path) or os.path.getsize(path) != int(size) or int(size) == 0:
            raise Failure("the export %s is not a %s-byte file at %s" % (key, size, path))
        with open(path, "rb") as f:
            head = f.read(12)
        if fmt == "mp4" and head[4:8] != b"ftyp":
            raise Failure("the MP4 export does not start with an ftyp box: %r" % head)
        if fmt == "y4m" and not head.startswith(b"YUV4MPEG2"):
            raise Failure("the Y4M export has no header: %r" % head)
        log("exported %s (%s bytes, %s)" % (key, size, fmt))
        app.screenshot(os.path.join(out, "exported.png"))
        app.key("escape")

        # 7. Flora and dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.screenshot(os.path.join(out, "flora-dark.png"))
        log("PASS: sample, layout, playhead, razor, ripple delete, undo, export; screenshots in %s" % out)
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
    parser.add_argument("--debug-port", type=int, default=8772)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--width", type=int, default=1280)
    parser.add_argument("--height", type=int, default=800)
    parser.add_argument("--out")
    parser.add_argument("--keep-data", action="store_true")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azvideocut-e2e-")
    data_root = os.path.join(logs, "data")
    os.makedirs(data_root, exist_ok=True)
    out = args.out or os.path.join(logs, "screenshots")
    os.makedirs(out, exist_ok=True)
    passed = False
    try:
        passed = run(args, logs, out, data_root)
    except Failure:
        passed = False
    finally:
        if passed and not args.keep_data:
            shutil.rmtree(data_root, ignore_errors=True)
        log("logs in %s" % logs)
    sys.exit(0 if passed else 1)


if __name__ == "__main__":
    main()
