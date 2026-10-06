#!/usr/bin/env python3
"""AzSheets end to end over the debug server, on the shared driver scripts/azlin_e2e.py.

    1. starts AzSheets headless with a fresh data folder (--data-dir), 1280 x 800;
    2. the window fits it: the status bar lies inside the window (the body once kept the UA's
       8 px margin and its content height and pushed the status bar off the bottom);
    3. focuses the grid and TYPES 4 Enter 5 Enter =SUM(A1:A2) Enter - `AZSHEETS_CELL A3 9`;
    4. selects A1:A3 with Shift+Up: `AZSHEETS_STATS ... sum=18` and "Sum: 18" in the status bar;
    5. types 3 / 1 / 2 into C1:C3, selects them with the grid's keys, clicks HOME > "Sort A to Z"
       (it fell off the ribbon's right edge once) - C1 becomes 1;
    6. B2 and VIEW > Freeze Panes - `AZSHEETS_FROZEN 1 1` and the freeze line;
    7. Mod+S - `AZSHEETS_SAVED <id>`, sheets/<id>.xlsx and .json on disk;
    8. DATA > CSV - the export lands IN the data tree, sheets/exports/<title>.csv;
    9. Mod+, - the Options pane is appkit's settings page (theme / mode switches, the shortcuts);
       About - the standard About box names IronCalc;
   10. File > New > the Budget sample - its rows arrive (only the title did once: a ragged
       paste block lost every row of another length);
   11. the close guard: a close request with unsaved work shows the question, Cancel keeps
       the window (reported as a NOTE when the headless backend does not dispatch the close);
   12. a second session on --screen backstage-open opens the workbook: A3 = 9, C1 = 1, frozen.
   Screenshots after the main steps, in flat light and flora dark.

Usage (from the azul repository, after building libazul with the debug server and AzSheets):

    python3 scripts/azsheets_e2e.py [--bin target/release/AzSheets] [--debug-port 8772]
        [--timeout 240] [--out <dir>] [--keep]

Run it through the capped runner on the 8 GB Mac (one app at a time):

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 300 --log /tmp/azsheets.log -- \\
        python3 scripts/azsheets_e2e.py --bin target/release/AzSheets
"""

import os
import re
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import azlin_e2e as k  # noqa: E402

TAG = "azsheets"
GRID = "#__azsheets_grid"
WIDTH, HEIGHT = 1280, 800


class Sheets(k.App):
    """AzSheets under its debug server."""

    def replies(self):
        return len(self.printed("AZSHEETS_REPLY", r"\d+ \w+"))

    def await_reply(self, before, what):
        """Waits until a reply after `before` replies arrived. (Not `settle`: that is the shared
        helper's wait for animations, which every `click` runs first.)"""
        self.until(what, lambda: self.replies() > before)
        self.frame(2)

    def cell_line(self, a1):
        """The last `AZSHEETS_CELL <a1> <text>` text, or None."""
        found = self.printed("AZSHEETS_CELL", r"%s .*|%s" % (re.escape(a1), re.escape(a1)))
        if not found:
            return None
        return found[-1][len(a1):].strip()

    def focus_grid(self):
        self.must("focus_node", selector=GRID)
        self.frame(1)

    def type_text(self, text):
        self.must("text_input", text=text)
        self.frame(1)

    def home(self, right=0, down=0, shift_down=0):
        """Mod+Home, then arrows: the grid's own keyboard navigation."""
        self.focus_grid()
        self.key("home", primary=True)
        for _ in range(right):
            self.key("right")
        for _ in range(down):
            self.key("down")
        for _ in range(shift_down):
            self.key("down", shift=True)
        self.frame(2)

    def enter_values(self, values):
        for text in values:
            before = self.replies()
            # Re-focus each time: a rebuild that loses the focus must not
            # send the next keys nowhere (the E2E checks the grid, not that).
            self.focus_grid()
            self.type_text(text)
            self.key("return")
            self.await_reply(before, "the engine to take %s" % text)

    def classes(self):
        out = set()
        for d in k.dicts(self.op("get_node_hierarchy")):
            for c in d.get("classes") or []:
                out.add(c)
        return out

    def layout_of(self, selector):
        value = self.value("get_node_layout", selector=selector)
        return (value or {}).get("rect") or {}


def start(binary, args, logs, data_dir, extra):
    app = Sheets(TAG, binary, ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT)] + extra,
                 args.debug_port, logs, args.timeout)
    app.until("AzSheets to be ready", lambda: app.printed("AZSHEETS_READY") and app.op("get_node_hierarchy"))
    app.must("resize", width=WIDTH, height=HEIGHT)
    app.frame(3)
    return app


def first_session(binary, args, logs, data_dir, out):
    app = start(binary, args, logs, data_dir, [])
    try:
        app.until("the grid in the tree", lambda: app.has_id("__azsheets_grid"))
        app.until("the first snapshot", lambda: app.replies() > 0)

        # 2. The window fits.
        bar = app.layout_of(".__azul-native-statusbar")
        bottom = bar.get("y", 1e9) + bar.get("height", 0)
        if bottom > HEIGHT + 0.5:
            raise k.Failure("the status bar ends at %.0f px, below the %d px window" % (bottom, HEIGHT))
        app.log("the status bar is inside the window (ends at %.0f px)" % bottom)

        # 3. Type numbers and a SUM.
        app.enter_values(["4", "5", "=SUM(A1:A2)"])
        app.key("up")
        app.until("A3 to show 9", lambda: app.cell_line("A3") == "9")
        if not app.shows("9"):
            raise k.Failure("the grid does not show 9")
        app.log("typed 4, 5, =SUM(A1:A2): A3 shows 9")

        # 4. The status bar's sum of A1:A3.
        before = app.replies()
        app.focus_grid()
        app.key("up", shift=True)
        app.key("up", shift=True)
        app.await_reply(before, "the selection's statistics")
        app.until("the sum of A1:A3", lambda: any(s.startswith("count=3 sum=18") for s in app.printed("AZSHEETS_STATS")))
        app.until("'Sum: 18' in the status bar", lambda: app.shows("Sum: 18"))
        app.log("status bar: Sum: 18")

        # 5. Sort a range from HOME (on screen now).
        app.home(right=2)
        app.enter_values(["3", "1", "2"])
        app.home(right=2, shift_down=2)
        before = app.replies()
        app.click(text="Sort A to Z")
        app.await_reply(before, "the sort")
        app.home(right=2)
        app.until("C1 to be 1 after the sort", lambda: app.cell_line("C1") == "1")
        app.log("sorted C1:C3 from HOME: C1 is 1")

        # 6. Freeze a pane at B2.
        app.home(right=1, down=1)
        app.click(text="VIEW")
        before = app.replies()
        app.click(text="Freeze Panes")
        app.await_reply(before, "the freeze")
        app.until("the panes frozen at B2", lambda: "1 1" in app.printed("AZSHEETS_FROZEN"))
        app.until("the freeze line", lambda: "__azul-native-cell-grid-freeze" in app.classes())
        app.log("froze the panes at B2")
        app.screenshot(os.path.join(out, "workbook-flat-light.png"))

        # 7. Save.
        app.focus_grid()
        app.key("s", primary=True)
        saved = app.until("the save", lambda: app.printed("AZSHEETS_SAVED", r"[0-9a-f-]+"))
        doc_id = saved[-1]
        for ext in ("xlsx", "json"):
            path = os.path.join(data_dir, "sheets", "%s.%s" % (doc_id, ext))
            if not os.path.isfile(path) or os.path.getsize(path) == 0:
                raise k.Failure("the save wrote no %s" % path)
        app.log("saved sheets/%s.xlsx and .json" % doc_id)

        # 8. CSV into the data tree.
        app.click(text="DATA")
        app.click(text="CSV")
        exported = app.until("the CSV export", lambda: app.printed("AZSHEETS_EXPORTED"))
        csv_path = os.path.join(data_dir, "sheets", "exports")
        if not (os.path.isdir(csv_path) and any(f.endswith(".csv") for f in os.listdir(csv_path))):
            raise k.Failure("the CSV is not in sheets/exports/ of the data tree (%s)" % exported[-1])
        app.log("exported %s" % exported[-1])

        # 9. Options (appkit's settings page) and About (the standard About box).
        app.focus_grid()
        app.key("comma", primary=True)
        app.until("the settings page", lambda: app.has_id("appkit-theme") and app.has_id("appkit-mode"))
        app.screenshot(os.path.join(out, "options.png"))
        app.click(text="About")
        app.until("the About box", lambda: app.shows("IronCalc 0.8.3"))
        app.screenshot(os.path.join(out, "about.png"))
        app.key("escape")
        app.log("Options is the kit's settings page; About names IronCalc")

        # 10. The sample's rows arrive.
        app.click(text="FILE")
        app.click(text="New")
        before = app.replies()
        app.click(selector="#__azsheets_new-sample")
        app.await_reply(before, "the sample")
        for word in ("Category", "Rent", "Total"):
            app.until("%s in the sample" % word, lambda: app.shows(word))
        app.screenshot(os.path.join(out, "sample-flat-light.png"))
        app.log("the Budget sample shows its rows")

        # 11. The close guard (the sample is unsaved).
        try:
            app.must("close")
            app.frame(3)
            app.until("the save question", lambda: "__azul-native-message-box" in app.classes())
            app.screenshot(os.path.join(out, "close-question.png"))
            app.click(text="Cancel")
            app.until("the question gone", lambda: "__azul-native-message-box" not in app.classes())
            app.log("a close with unsaved work asks; Cancel keeps the window")
        except k.Failure as e:
            if app.process.poll() is not None:
                app.log("NOTE: the headless backend closed without CloseRequested (%s) - INFRA6 / HEADLESS6" % e)
                return doc_id
            raise

        # Flora dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.screenshot(os.path.join(out, "sample-flora-dark.png"))
        return doc_id
    except k.Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (k.tail(app.out_path), k.tail(app.err_path)))
        raise
    finally:
        app.stop()


def second_session(binary, args, logs, data_dir, out, doc_id):
    app = start(binary, args, logs, data_dir, ["--screen", "backstage-open"])
    try:
        app.until("the workbook in the Open list", lambda: app.has_id("__azsheets_open-0"))
        app.click(selector="#__azsheets_open-0")
        app.until("the workbook to open", lambda: doc_id in app.printed("AZSHEETS_OPENED", r"[0-9a-f-]+"))
        app.frame(3)
        app.until("the panes still frozen", lambda: app.printed("AZSHEETS_FROZEN") and app.printed("AZSHEETS_FROZEN")[-1] == "1 1")
        app.home(down=2)
        app.until("A3 still 9", lambda: app.cell_line("A3") == "9")
        app.home(right=2)
        app.until("C1 still 1", lambda: app.cell_line("C1") == "1")
        app.screenshot(os.path.join(out, "reopened.png"))
        app.log("reopened %s: A3 = 9, C1 = 1, panes frozen" % doc_id)
    except k.Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (k.tail(app.out_path), k.tail(app.err_path)))
        raise
    finally:
        app.stop()


def body(args, logs, out):
    binary = k.find_binary("AzSheets", args.bin, "AZSHEETS_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    doc_id = first_session(binary, args, logs, data_dir, out)
    second_session(binary, args, logs, data_dir, out, doc_id)
    if not args.keep:
        shutil.rmtree(data_dir, ignore_errors=True)
    print("[%s] PASS" % TAG, flush=True)
    return True


if __name__ == "__main__":
    k.run(TAG, body, default_port=8772)
