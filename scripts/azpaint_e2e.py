#!/usr/bin/env python3
"""AzPaint end to end over the debug server (headless).

    1. starts AzPaint on a fresh data folder (--data-dir, --size 900x640) and waits for
       its first canvas raster (`AZPAINT_RASTER <rev>`);
    2. draws a stroke with the mouse. The stroke must be ON SCREEN while it is drawn:
       the canvas is rasterised again before the release (`AZPAINT_RASTER` lines) and a
       screenshot taken before the release has dark pixels along the path. (2026-10-03:
       "1 strokes" in the header over an empty canvas - the app listened to MouseOver,
       which fires on ENTRY since the W3C split, and poked the node AFTER the canvas.)
       After the release `AZPAINT_STROKES 1` and the stroke is still on screen;
    3. undo / redo (Mod+Z, Mod+Shift+Z: azul-appkit's UndoHistory): 0 strokes and a clean
       canvas, then 1 stroke and the stroke back - on screen, not only in the counter;
    4. exports a PNG (Mod+S) and an SVG (Mod+Shift+S) INTO the data tree through the
       Drive (`AZPAINT_EXPORTED paint/exports/<name> <bytes>`) and checks both files;
    5. takes screenshots (flat light, flora dark).

Usage: python3 scripts/azpaint_e2e.py [--bin target/release/AzPaint] [--debug-port 8794]
Run it through scripts/waves/tools/run_capped.sh on a small machine.
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import azlin_e2e as e2e  # noqa: E402

PNG_MAGIC = b"\x89PNG\r\n\x1a\n"
WINDOW_W = 900


def body(args, logs, out):
    binary = e2e.find_binary("AzPaint", args.bin, "AZPAINT_BIN")
    data = os.path.join(logs, "data")
    os.makedirs(data, exist_ok=True)
    app = e2e.App("AzPaint", binary, ["--data-dir", data, "--size", "%dx640" % WINDOW_W],
                  args.debug_port, logs, args.timeout)

    def shot(name):
        path = os.path.join(out, name)
        app.screenshot(path)
        width, _, _ = e2e.read_png(path)
        return path, width / float(WINDOW_W)

    try:
        # 1. The window and the first raster of the canvas.
        app.until("the first canvas raster", lambda: app.printed("AZPAINT_RASTER", r"\d+"))
        app.frame(2)
        canvas = app.rect("__azpaint_canvas")
        x0 = float(canvas["x"]) + 120.0
        y0 = float(canvas["y"]) + 120.0
        # The box the stroke crosses (logical px).
        path_box = (x0 - 12.0, y0 - 12.0, 8 * 20.0 + 24.0, 8 * 6.0 + 24.0)
        path, scale = shot("azpaint-empty.png")
        if e2e.dark_pixels(path, path_box, scale):
            raise e2e.Failure("the empty canvas already has ink where the stroke will go")

        # 2. A stroke shows WHILE it is drawn.
        rasters = len(app.printed("AZPAINT_RASTER", r"\d+"))
        app.must("mouse_move", x=x0, y=y0)
        app.must("mouse_down", x=x0, y=y0)
        app.frame()
        for i in range(1, 9):
            app.must("mouse_move", x=x0 + i * 20.0, y=y0 + i * 6.0)
            app.frame()
        live = len(app.printed("AZPAINT_RASTER", r"\d+")) - rasters
        if live <= 1:
            raise e2e.Failure("the canvas was rasterised %d time(s) while the stroke was drawn; "
                              "every move should redraw it" % live)
        path, scale = shot("azpaint-drawing.png")
        ink = e2e.dark_pixels(path, path_box, scale)
        if ink < 200:
            raise e2e.Failure("only %d dark pixels along the stroke BEFORE the release - "
                              "the canvas does not show the stroke while it is drawn" % ink)
        app.must("mouse_up", x=x0 + 160.0, y=y0 + 48.0)
        app.frame(2)
        app.expect_line("AZPAINT_STROKES", "1", "one stroke")
        path, scale = shot("azpaint-stroke.png")
        if e2e.dark_pixels(path, path_box, scale) < ink:
            raise e2e.Failure("the stroke faded after the release")
        app.log("stroke: %d live rasters before the release, %d dark pixels" % (live, ink))

        # 3. Undo / redo, on screen.
        app.key("z", primary=True)
        app.expect_line("AZPAINT_STROKES", "0", "undo")
        app.frame(2)
        path, scale = shot("azpaint-undone.png")
        if e2e.dark_pixels(path, path_box, scale):
            raise e2e.Failure("undo left the stroke on the canvas")
        app.key("z", primary=True, shift=True)
        app.expect_line("AZPAINT_STROKES", "1", "redo")
        app.frame(2)
        path, scale = shot("azpaint-redone.png")
        if e2e.dark_pixels(path, path_box, scale) < ink:
            raise e2e.Failure("redo did not bring the stroke back on the canvas")

        # 4. Exports into the data tree.
        for shift, magic in ((False, PNG_MAGIC), (True, b"<svg")):
            before = len(app.printed("AZPAINT_EXPORTED", r"\S+ \d+"))
            app.key("s", primary=True, shift=shift)
            app.until("an export", lambda: len(app.printed("AZPAINT_EXPORTED", r"\S+ \d+")) > before)
            name, size = app.printed("AZPAINT_EXPORTED", r"\S+ \d+")[-1].split(" ")
            if not name.startswith("paint/exports/"):
                raise e2e.Failure("the export is not in the data tree: %s" % name)
            path = os.path.join(data, *name.split("/"))
            with open(path, "rb") as f:
                head = f.read(len(magic))
            if head != magic or int(size) < 32:
                raise e2e.Failure("%s is not the export it should be (%r, %s bytes)" % (path, head, size))
            app.log("exported %s (%s bytes)" % (name, size))

        # 5. The look.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        shot("azpaint-flora-dark.png")
        app.log("PASS: stroke on screen while drawn, undo / redo on screen, PNG + SVG exports in the data tree")
        return True
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run("azpaint", body, default_port=8794)
