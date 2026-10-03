#!/usr/bin/env python3
"""AzPaint end to end over the debug server (headless).

    1. starts AzPaint on a fresh data folder (--data-dir) and waits for its first canvas
       raster (`AZPAINT_RASTER <rev>`);
    2. draws a stroke with the mouse: the canvas is rasterised again WHILE the stroke is
       drawn (the canvas marker's image callback is poked on every move - before the
       release, `AZPAINT_RASTER` lines with a newer revision), `AZPAINT_STROKES 1` after it;
    3. undo / redo (Mod+Z, Mod+Shift+Z: azul-appkit's UndoHistory): 0, then 1 stroke;
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


def body(args, logs, out):
    binary = e2e.find_binary("AzPaint", args.bin, "AZPAINT_BIN")
    data = os.path.join(logs, "data")
    os.makedirs(data, exist_ok=True)
    app = e2e.App("AzPaint", binary, ["--data-dir", data, "--size", "900x640"], args.debug_port, logs, args.timeout)
    try:
        # 1. The window and the first raster of the canvas.
        app.until("the first canvas raster", lambda: app.printed("AZPAINT_RASTER", r"\d+"))
        app.frame(2)
        canvas = app.rect("__azpaint_canvas")
        x0 = float(canvas["x"]) + 120.0
        y0 = float(canvas["y"]) + 120.0
        app.screenshot(os.path.join(out, "azpaint-empty.png"))

        # 2. A stroke shows WHILE it is drawn.
        rasters = len(app.printed("AZPAINT_RASTER", r"\d+"))
        app.must("mouse_move", x=x0, y=y0)
        app.must("mouse_down", x=x0, y=y0)
        app.frame()
        for i in range(1, 9):
            app.must("mouse_move", x=x0 + i * 20.0, y=y0 + i * 6.0)
            app.frame()
        live = len(app.printed("AZPAINT_RASTER", r"\d+")) - rasters
        if live <= 0:
            raise e2e.Failure("the canvas was not rasterised while the stroke was drawn "
                              "(the canvas marker's image callback was never poked)")
        app.must("mouse_up", x=x0 + 160.0, y=y0 + 48.0)
        app.frame(2)
        app.expect_line("AZPAINT_STROKES", "1", "one stroke")
        app.log("stroke: %d live rasters before the release" % live)
        app.screenshot(os.path.join(out, "azpaint-stroke.png"))

        # 3. Undo / redo.
        app.key("z", primary=True)
        app.expect_line("AZPAINT_STROKES", "0", "undo")
        app.key("z", primary=True, shift=True)
        app.expect_line("AZPAINT_STROKES", "1", "redo")

        # 4. Exports into the data tree.
        for key, shift, magic in (("s", False, PNG_MAGIC), ("s", True, b"<svg")):
            before = len(app.printed("AZPAINT_EXPORTED", r"\S+ \d+"))
            app.key(key, primary=True, shift=shift)
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
        app.screenshot(os.path.join(out, "azpaint-flora-dark.png"))
        app.log("PASS: stroke shown while drawn, undo / redo, PNG + SVG exports in the data tree")
        return True
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run("azpaint", body, default_port=8794)
