#!/usr/bin/env python3
"""AzShow end to end, headless over the debug server, on the shared driver scripts/azlin_e2e.py.

    1. starts AzShow headless on File > New with an empty data folder (--data-dir), 1280 x 800;
    2. creates a deck from the default theme (Create) - AZSHOW_DECK;
    3. adds a slide with HOME > New Slide and one with INSERT > the "Two Content" layout cell
       (AZSHOW_SLIDES 2 2, then 3 3), checks the status bar says SLIDE 3 OF 3;
    4. double-clicks the title placeholder, types a title, leaves the text with Escape, checks it;
    5. inserts a rectangle (INSERT > Rectangle), drags it 150 px right and checks the committed frame
       moved by 150 / scale slide units (AZSHOW_FRAME);
    6. VIEW > Slide Sorter, drags slide 1 onto slide 3 - AZSHOW_ORDER (when no drop arrives
       headlessly: a NOTE, and Mod+Down on the focused thumbnail instead);
    7. Mod+S - show/<id>/deck.json holds three slides and the title;
    8. File > Export > Create PDF - the PDF lands IN the data tree, show/exports/<title>.pdf
       (it went through a save dialog to a path outside the tree once);
    9. File > Options is appkit's settings page; File > About the standard About box;
   10. F5 steps through the show to its end, Escape closes it;
   11. the close guard: a close request with unsaved work shows the question, Cancel keeps the
       window (a NOTE when the headless backend does not dispatch the close);
   12. screenshots per step, and flora + dark.

Usage (from the azul repository, after building libazul with the debug server and AzShow):

    python3 scripts/azshow_e2e.py [--bin target/release/AzShow] [--debug-port 8781]
        [--timeout 240] [--out <dir>] [--keep]

AZSHOW_E2E_PRESENTER=1 also opens the presenter window. Run it through the capped runner:

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 300 --log /tmp/azshow.log -- \\
        python3 scripts/azshow_e2e.py --bin target/release/AzShow
"""

import json
import os
import shutil
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as k  # noqa: E402

TAG = "azshow"
WIDTH, HEIGHT = 1280, 800


class Show(k.App):
    """AzShow under its debug server."""

    def stdout(self):
        try:
            with open(self.out_path, "r", encoding="utf-8", errors="replace") as f:
                return f.read()
        except OSError:
            return ""

    def has_line(self, name):
        """Whether the bare line `name` (no value) was printed."""
        return any(line.strip() == name for line in self.stdout().splitlines())

    def soon(self, what, check, seconds=6.0):
        """`until` with a short deadline: None instead of a failure."""
        end = time.time() + seconds
        while time.time() < end:
            try:
                value = check()
                if value:
                    return value
            except (OSError, ValueError):
                pass
            time.sleep(0.25)
        return None

    def box(self, selector):
        value = self.value("get_node_layout", selector=selector)
        r = (value or {}).get("rect") or {}
        return {key: float(r.get(key, 0)) for key in ("x", "y", "width", "height")}

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

    def classes(self):
        out = set()
        for d in k.dicts(self.op("get_node_hierarchy")):
            for c in d.get("classes") or []:
                out.add(c)
        return out


def slide_box(app):
    """The slide's rectangle on screen and its scale (px per slide unit)."""
    r = app.box("#__azshow_slide")
    if r["width"] <= 0:
        raise k.Failure("the slide (#__azshow_slide) is not laid out: %s" % r)
    return r, r["width"] / 1920.0


def body(args, logs, out):
    binary = k.find_binary("AzShow", args.bin, "AZSHOW_BIN")
    data_root = os.path.join(logs, "data")
    os.makedirs(data_root, exist_ok=True)
    argv = ["--data-dir", data_root, "--size", "%dx%d" % (WIDTH, HEIGHT), "--screen", "backstage-new"]
    if os.environ.get("AZSHOW_E2E_PRESENTER") != "1":
        argv.append("--no-presenter")
    app = Show(TAG, binary, argv, args.debug_port, logs, args.timeout)
    shot = lambda name: app.screenshot(os.path.join(out, name + ".png"))
    try:
        app.until("AzShow's window", lambda: app.has_line("AZSHOW_READY") and app.shows("Create"))
        app.must("resize", width=WIDTH, height=HEIGHT)
        app.frame(2)
        shot("01-new")

        # ---- a new deck ----
        app.click(text="Create")
        deck_id = app.until("the new deck", lambda: app.printed("AZSHOW_DECK", r"\S+"))[-1]
        app.log("deck %s" % deck_id)
        app.until("the normal view", lambda: app.shows("SLIDE 1 OF 1"))

        # ---- slides with layouts ----
        before = len(app.printed("AZSHOW_SLIDES", r"\d+ \d+"))
        app.click(text="New Slide")
        app.until("a second slide", lambda: app.printed("AZSHOW_SLIDES", r"\d+ \d+")[before:])
        app.click(text="INSERT")
        app.click(text="Two Content")
        app.until("a third slide", lambda: [l for l in app.printed("AZSHOW_SLIDES", r"\d+ \d+") if l.startswith("3 ")])
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
        app.log("typed the title")
        shot("03-title")

        # ---- insert a rectangle and move it ----
        app.click(text="INSERT")
        app.click(text="Rectangle")
        box, scale = slide_box(app)
        cx = box["x"] + (760 + 200) * scale
        cy = box["y"] + (390 + 150) * scale
        before = len(app.printed("AZSHOW_FRAME", r".+"))
        app.drag(cx, cy, cx + 150, cy)
        frames = app.until("the committed move", lambda: app.printed("AZSHOW_FRAME", r".+")[before:])
        moved_x = float(frames[-1].split()[1])
        want = 760 + 150 / scale
        if abs(moved_x - want) > 12:
            raise k.Failure("the rectangle went to x=%s, expected about %.0f (scale %.3f)" % (moved_x, want, scale))
        app.log("moved the rectangle to x=%s" % moved_x)
        shot("04-moved")

        # ---- reorder in the slide sorter ----
        app.click(text="VIEW")
        app.click(text="Slide Sorter")
        app.until("the sorter", lambda: app.printed("AZSHOW_VIEW", r"Slide Sorter"))
        shot("05-sorter")
        thumbs = []
        for d in k.dicts(app.op("get_all_nodes_layout")):
            if "__azul-native-thumbnail-strip-item" in (d.get("classes") or []) and isinstance(d.get("rect"), dict):
                thumbs.append({key: float(d["rect"].get(key, 0)) for key in ("x", "y", "width", "height")})
        if len(thumbs) < 3:
            raise k.Failure("the sorter shows %d thumbnails, expected 3" % len(thumbs))
        center = lambda r: (r["x"] + r["width"] / 2, r["y"] + r["height"] / 2)
        before = len(app.printed("AZSHOW_ORDER", r".+"))
        (x1, y1), (x3, y3) = center(thumbs[0]), center(thumbs[2])
        app.drag(x1, y1, x3, y3, steps=12)
        order = app.soon("the drop", lambda: app.printed("AZSHOW_ORDER", r".+")[before:])
        if order:
            app.log("drag and drop reordered the slides: %s" % order[-1])
        else:
            app.log("NOTE: no drop arrived headlessly; reordering with Mod+Down on the thumbnail")
            app.must("click", x=x1, y=y1)
            app.frame(2)
            app.key("down", primary=True)
            order = app.until("the keyboard move", lambda: app.printed("AZSHOW_ORDER", r".+")[before:])
            app.log("Mod+Down reordered the slides: %s" % order[-1])
        shot("06-reordered")
        app.click(text="VIEW")
        app.click(text="Normal")

        # ---- save ----
        before = len(app.printed("AZSHOW_SAVED", r"\S+"))
        app.key("s", primary=True)
        app.until("the save", lambda: app.printed("AZSHOW_SAVED", r"\S+")[before:])
        path = os.path.join(data_root, "show", deck_id, "deck.json")
        with open(path, "r", encoding="utf-8") as f:
            deck = json.load(f)
        if deck.get("format") != "azshow.deck" or len(deck.get("slides", [])) != 3:
            raise k.Failure("deck.json is not the three-slide deck: %s" % json.dumps(deck)[:300])
        if "Quarterly plan" not in json.dumps(deck):
            raise k.Failure("deck.json does not hold the typed title")
        app.log("saved %s (%d slides)" % (path, len(deck["slides"])))

        # ---- export into the data tree ----
        app.click(text="FILE")
        app.click(text="Export")
        app.click(text="Create PDF")
        exported = app.until("the PDF export", lambda: app.printed("AZSHOW_EXPORTED", r"\S.*"))
        key = exported[-1]
        if not key.startswith("show/exports/") or not os.path.isfile(os.path.join(data_root, key)):
            raise k.Failure("the PDF is not in show/exports/ of the data tree: %s" % key)
        app.log("exported %s" % key)

        # ---- Options and About ----
        app.click(text="Options")
        app.until("the settings page", lambda: app.has_id("appkit-theme") and app.has_id("appkit-mode"))
        shot("07-options")
        # OK returns to the backstage; About is its own page there.
        app.click(selector="#appkit-settings-ok")
        app.until("the backstage again", lambda: not app.has_id("appkit-settings"))
        app.click(text="About")
        app.until("the About box", lambda: "__azul-native-about-dialog" in app.classes())
        shot("08-about")
        app.key("escape")
        app.until("back in the editor", lambda: app.shows("SLIDE"))

        # ---- the show ----
        before = len(app.printed("AZSHOW_SHOW", r"\d+ \d+"))
        app.key("f5")
        app.until("the show", lambda: app.printed("AZSHOW_SHOW", r"\d+ \d+")[before:])
        shot("09-show")
        for key_name in ("space", "right", "space"):
            app.key(key_name)
        app.until("the end of the show", lambda: app.has_line("AZSHOW_SHOW_ENDED"))
        steps = app.printed("AZSHOW_SHOW", r"\d+ \d+")[before:]
        if len(steps) < 3:
            raise k.Failure("the show did not step through the slides: %s" % steps)
        app.key("escape")
        app.until("the show to close", lambda: app.has_line("AZSHOW_SHOW_CLOSED"))
        app.log("the show stepped: %s" % steps)

        # ---- the close guard (an unsaved change first) ----
        app.click(text="INSERT")
        app.click(text="Rectangle")
        try:
            app.must("close")
            app.frame(3)
            app.until("the save question", lambda: "__azul-native-message-box" in app.classes())
            shot("10-close-question")
            # The question is a Modal: a window of its own.
            app.click(text="Cancel", window="azul-transient")
            app.until("the question gone", lambda: "__azul-native-message-box" not in app.classes())
            app.log("a close with unsaved work asks; Cancel keeps the window")
        except k.Failure as e:
            if app.process.poll() is not None:
                app.log("NOTE: the headless backend closed without CloseRequested (%s) - INFRA6 / HEADLESS6" % e)
                return True
            raise

        # ---- the other theme and mode ----
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        shot("11-flora-dark")
        if not args.keep:
            shutil.rmtree(data_root, ignore_errors=True)
        print("[%s] PASS" % TAG, flush=True)
        return True
    except k.Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (k.tail(app.out_path), k.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    k.run(TAG, body, default_port=8781)
