#!/usr/bin/env python3
"""Real-app proofs of the engine fixes, for the final test round.

    python3 scripts/engine_proofs.py <proof> [--bin DIR] [--logs DIR]

Each proof starts its app over the debug server, does what the fix is about and exits 0 when the
app shows the fixed behaviour, 1 when it does not (the reason on stderr). `--bin` is the folder
of the release binaries (default target/release), `--logs` where the apps write their logs
(default target/engine-proofs). Every proof runs headless except `widgets-gpu-dark`, which opens
a real window for a few seconds per backend.

    maps-idle        AzMaps at rest renders its map 0 times a second once the tiles are in: two
                     labels of one name took turns, each fade asking for the next render.
    maps-mode        AzMaps pinned light draws light tiles whatever the desktop shows, and a switch
                     to dark re-renders the map in dark.
    widgets-click    A click on AzWidgets' radio buttons rebuilds the page comparing a handful of
                     nodes (AZ_RECONCILE_DEBUG's `compared`), not the whole page.
    widgets-gpu-dark AzWidgets in dark mode paints the Dialog row's "Delete file..." button with
                     the same face on the GPU as on the CPU.
    native-colour    A native screenshot of AzWidgets shows the "Delete file..." button in the
                     colour the CPU frame has: the capture is converted from the display's colour
                     space to sRGB (a Display P3 Mac moved every colour before).
"""
import argparse
import base64
import json
import os
import re
import struct
import sys
import time
import zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as e2e  # noqa: E402


def fail(why):
    print("FAIL: " + why, file=sys.stderr)
    sys.exit(1)


def started(app, seconds=40):
    deadline = time.time() + seconds
    while time.time() < deadline:
        try:
            app.op("wait_frame")
            return
        except Exception:
            time.sleep(0.3)
    fail("%s never answered its debug server" % app.tag)


def maps_idle(args):
    data = os.path.join(args.logs, "maps-idle-data")
    app = e2e.App("azmaps-idle", os.path.join(args.bin, "AzMaps"),
                  ["--stats", "--data-dir", data], 18961, args.logs, 180)
    renders = lambda: open(app.out_path, errors="replace").read().count("AZ_MAP_RENDER")
    tiles = lambda: open(app.out_path, errors="replace").read().count("AZ_MAP_TILE ")
    try:
        started(app)
        # The tiles of the start view arrive first; the map is at rest once none came for 3 s.
        quiet_since, last = time.time(), tiles()
        deadline = time.time() + 90
        while time.time() - quiet_since < 3:
            if time.time() > deadline:
                fail("tiles kept arriving for 90 s")
            time.sleep(0.5)
            now = tiles()
            if now != last:
                quiet_since, last = time.time(), now
        before = renders()
        time.sleep(5)
        idle = renders() - before
        print("renders in 5 s at rest: %d" % idle)
        if idle != 0:
            fail("the map rendered %d times in 5 s at rest (0 expected)" % idle)
    finally:
        app.stop()


def maps_mode(args):
    data = os.path.join(args.logs, "maps-mode-data")
    app = e2e.App("azmaps-mode", os.path.join(args.bin, "AzMaps"),
                  ["--stats", "--data-dir", data, "--mode", "light"], 18962, args.logs, 120,
                  extra_env={"AZ_MAP_DEBUG": "1"})
    looks = lambda text: re.findall(r"spawn_pending: scheme=(\S+) look=(\S+)", text)
    try:
        started(app)
        time.sleep(4)
        pinned = looks(open(app.err_path, errors="replace").read())
        if not pinned or any("Dark" in look for _, look in pinned[-3:]):
            fail("pinned light, the map fetched %s" % pinned[-3:])
        offset = len(open(app.err_path, errors="replace").read())
        app.must("set_mode", mode="dark")
        time.sleep(4)
        after = looks(open(app.err_path, errors="replace").read()[offset:])
        print("light: %s, after the switch: %s" % (pinned[-1:], after[-1:]))
        if not after or not any("Dark" in look for _, look in after):
            fail("the switch to dark fetched no dark tiles: %s" % after[-3:])
    finally:
        app.stop()


def widgets_click(args):
    app = e2e.App("azwidgets-click", os.path.join(args.bin, "AzWidgets"), [], 18963, args.logs,
                  120, extra_env={"AZ_RECONCILE_DEBUG": "1"})
    compared = lambda: [int(n) for n in re.findall(r"\[reconcile\] (\d+) compared",
                                                   open(app.err_path, errors="replace").read())]
    try:
        started(app)
        app.frame(3)
        seen = len(compared())
        for i in range(4):
            app.must("click", text="Option B" if i % 2 == 0 else "Option C")
            app.frame()
        counts = compared()[seen:]
        print("nodes compared per rebuild: %s" % counts)
        if not counts:
            fail("no rebuild reported what it compared (AZ_RECONCILE_DEBUG)")
        if max(counts) > 100:
            fail("a click compared %d nodes; the radio group's own nodes are expected" % max(counts))
    finally:
        app.stop()


def png_pixels(data):
    """(width, height, rows, channels) of an 8-bit RGB / RGBA, non-interlaced PNG."""
    pos, idat, ctype = 8, b"", 6
    while pos < len(data):
        n = struct.unpack(">I", data[pos:pos + 4])[0]
        kind, chunk = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + n]
        pos += 12 + n
        if kind == b"IHDR":
            w, h, _depth, ctype = struct.unpack(">IIBB", chunk[:10])
        elif kind == b"IDAT":
            idat += chunk
    ch = {6: 4, 2: 3}[ctype]
    raw, stride, rows, prev, i = zlib.decompress(idat), w * ch, [], bytearray(w * ch), 0
    for _ in range(h):
        f, line = raw[i], bytearray(raw[i + 1:i + 1 + stride])
        i += 1 + stride
        for x in range(stride):
            a = line[x - ch] if x >= ch else 0
            b = prev[x]
            c = prev[x - ch] if x >= ch else 0
            if f == 1:
                line[x] = (line[x] + a) & 255
            elif f == 2:
                line[x] = (line[x] + b) & 255
            elif f == 3:
                line[x] = (line[x] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(bytes(line))
        prev = line
    return w, h, rows, ch


def face_of(backend, port, args):
    """The colour at the middle of the "Delete file..." button, in a real dark window."""
    app = e2e.App("azwidgets-" + backend, os.path.join(args.bin, "AzWidgets"), [], port,
                  args.logs, 60, extra_env={"AZ_BACKEND": backend})
    try:
        started(app)
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.must("scroll_into_view", text="Delete file…")
        app.frame(3)
        layout = app.value("get_node_layout", text="Delete file…")
        shot = app.value("take_native_screenshot")
        b64 = shot.get("png_base64") if isinstance(shot, dict) else shot
        png = base64.b64decode(b64)
        open(os.path.join(args.logs, "delete-file-%s.png" % backend), "wb").write(png)
    finally:
        # A real window stays up only for the proof itself.
        app.stop()
    w, h, rows, ch = png_pixels(png)
    rect = layout.get("rect", layout) if isinstance(layout, dict) else {}
    scale = float(shot.get("scale", 1.0)) if isinstance(shot, dict) else 1.0
    x = int((rect["x"] + rect["width"] / 2) * scale)
    y = int((rect["y"] + rect["height"] / 2) * scale)
    if not (0 <= x < w and 0 <= y < h):
        fail("%s: the button's middle %s lies outside the %dx%d capture" % (backend, (x, y), w, h))
    return tuple(rows[y][x * ch:x * ch + 3])


def pixel_at(png, rect, scale):
    w, h, rows, ch = png_pixels(png)
    x = int((rect["x"] + rect["width"] / 2) * scale)
    y = int((rect["y"] + rect["height"] / 2) * scale)
    if not (0 <= x < w and 0 <= y < h):
        fail("the button's middle %s lies outside the %dx%d capture" % ((x, y), w, h))
    return tuple(rows[y][x * ch:x * ch + 3])


def native_colour(args):
    app = e2e.App("azwidgets-native-colour", os.path.join(args.bin, "AzWidgets"), [], 18966,
                  args.logs, 60, extra_env={"AZ_BACKEND": "cpu"})
    try:
        started(app)
        app.must("scroll_into_view", text="Delete file…")
        app.frame(3)
        layout = app.value("get_node_layout", text="Delete file…")
        cpu = app.value("take_screenshot")
        native = app.value("take_native_screenshot")
    finally:
        # A real window stays up only for the proof itself.
        app.stop()
    rect = layout.get("rect", layout) if isinstance(layout, dict) else {}
    as_png = lambda shot: base64.b64decode(shot.get("png_base64") if isinstance(shot, dict) else shot)
    scale_of = lambda shot: float(shot.get("scale", 1.0)) if isinstance(shot, dict) else 1.0
    in_cpu = pixel_at(as_png(cpu), rect, scale_of(cpu))
    in_native = pixel_at(as_png(native), rect, scale_of(native))
    print("Delete file... face: cpu frame %s, native capture %s" % (in_cpu, in_native))
    if max(abs(a - b) for a, b in zip(in_cpu, in_native)) > 6:
        fail("the native capture shows %s where the frame has %s" % (in_native, in_cpu))


def widgets_gpu_dark(args):
    cpu = face_of("cpu", 18964, args)
    gpu = face_of("gpu", 18965, args)
    print("Delete file... face: cpu %s gpu %s" % (cpu, gpu))
    if max(abs(a - b) for a, b in zip(cpu, gpu)) > 8:
        fail("the GPU paints the button's face %s, the CPU %s" % (gpu, cpu))


PROOFS = {
    "maps-idle": maps_idle,
    "maps-mode": maps_mode,
    "widgets-click": widgets_click,
    "widgets-gpu-dark": widgets_gpu_dark,
    "native-colour": native_colour,
}


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("proof", choices=sorted(PROOFS))
    parser.add_argument("--bin", default=os.path.join(root, "target", "release"))
    parser.add_argument("--logs", default=os.path.join(root, "target", "engine-proofs"))
    args = parser.parse_args()
    os.makedirs(args.logs, exist_ok=True)
    PROOFS[args.proof](args)
    print("PASS " + args.proof)


if __name__ == "__main__":
    main()
