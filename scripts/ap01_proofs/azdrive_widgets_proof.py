#!/usr/bin/env python3
"""AP-01 proofs in AzDrive, headless (a multi-step walk: never in a real window, which would hold
the screen and the focus for minutes): the tab overview takes the keyboard (RUNTIME23 2), the
search box's Cmd+F hint shows on an empty box and goes with the focus, toasts leave by
themselves and at most three show, F2 on an icon tile edits the name in the grid's own field
(WIDGETS23), and a screenshot in flat / flora x light / dark.
Usage: azdrive_widgets_proof.py --bin BIN --out DIR [--port N]"""
import argparse, os, sys, tempfile, time
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import azlin_e2e as e2e
import azdrive_e2e as d
from azlin_e2e import Failure

p = argparse.ArgumentParser()
p.add_argument("--bin", required=True)
p.add_argument("--out", required=True)
p.add_argument("--port", type=int, default=8823)
a = p.parse_args()
os.makedirs(a.out, exist_ok=True)
tag = "azdrive"
logs = tempfile.mkdtemp(prefix="azdrive-proof-")
home = os.path.join(logs, "home")
os.makedirs(home)
switches = ["--sample", "--screen", "this-pc", "--theme", "flat", "--mode", "light", "--language", "en",
            "--home", home, "--downloads", os.path.join(logs, "downloads"),
            "--data-dir", os.path.join(logs, "data"), "--drives", os.path.join(logs, "config", "drives.json"),
            "--dialogs", "inline", "--cache-dir", os.path.join(logs, "cache")]
app = d.Drive(tag, a.bin, switches, a.port, logs, 240)
I = d.I
results = []
def ok(what):
    results.append("PASS " + what); print("PASS", what, flush=True)
def section(name, fn):
    app.deadline = time.time() + 40
    try:
        fn()
    except Failure as err:
        results.append("FAIL %s: %s" % (name, err)); print("FAIL", name, err, flush=True)
    app.deadline = time.time() + 180
def shot(name):
    app.screenshot(os.path.join(a.out, "%s-%s.png" % (tag, name)))
def hint_shown():
    sel = ".__azul-native-address-bar-search-hint"
    return app.has(sel) and app.box(sel)["width"] > 0
def toasts():
    return len(app.nodes_with_class("__azul-native-toast"))
try:
    app.until("the This PC view", lambda: app.printed("AZDRIVE_PLACE", r"this-pc"))
    app.until("the debug server", lambda: app.op("get_dom_tree"))
    app.must("resize", width=1280.0, height=800.0)
    app.frame(3)
    app.after("Documents", "AZDRIVE_PLACE", r"home Documents/",
              lambda: app.click(selector="#" + I("side-fav-documents")))
    app.until("Documents' rows", lambda: "notes.txt" in d.item_names(app))
    def hint_section():
      app.until("the Cmd+F hint while the box is empty", hint_shown)
      shot("1-search-hint")
      d.focus_search_box(app)
      app.until("the hint gone while the box has the keyboard", lambda: not hint_shown())
      app.must("text_input", text="a"); app.frame(2)
      if hint_shown():
          raise Failure("the hint shows over typed text")
      shot("2-search-typed")
      d.close_search_view(app)
      app.until("Escape: the hint is back", hint_shown)
      ok("search hint: shows empty, hides with focus and text, comes back")
    section("search hint", hint_section)
    if "text-input" in d.focused_selector(app):
        app.key("escape"); app.frame(2)
    # The tab overview takes the keyboard.
    app.after("Ctrl+T: a second tab", "AZDRIVE_TABS", r"2 1", lambda: app.key("t", ctrl=True))
    app.until("the toast of the new tab", lambda: toasts() >= 1 and app.shows("New tab: Documents"))
    shot("3-toast")
    t0 = time.time()
    app.until("the toast leaves by itself", lambda: toasts() == 0, interval=0.1)
    ok("toast: shows on Ctrl+T and leaves after %.1f s" % (time.time() - t0))
    d.focus_search_box(app)
    app.after("Ctrl+Shift+A: the tab overview", "AZDRIVE_TAB_OVERVIEW", r"open",
              lambda: app.key("a", ctrl=True, shift=True))
    app.until("the overview has the keyboard", lambda: "tab-overview-list" in d.focused_selector(app))
    shot("4-overview")
    app.key("down")
    shot("5-overview-down")
    app.after("Return opens the highlighted first tab", "AZDRIVE_TABS", r"2 0", lambda: app.key("return"))
    ok("tab overview: took the keyboard from the search box, Down + Return opened the first tab")
    # Four copies at once (a navigation clears AzDrive's notices, so not four new tabs): at
    # most three toasts, newest lowest; x closes one at once.
    app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
              lambda: d.select_item(app, "notes.txt"))
    for _ in range(4):
        app.key("c", primary=True, frames=1)
    try:
        app.until("the toasts", lambda: toasts() >= 3)
    except Failure:
        print("tabs lines:", app.printed("AZDRIVE_TABS"), "toasts:", toasts(), "focus:", d.focused_selector(app))
        shot("6-toasts-missing")
        raise
    if toasts() > 3:
        raise Failure("%d toasts show (at most 3)" % toasts())
    shot("6-three-toasts")
    before = toasts()
    app.click(selector=".__azul-native-toast-close")
    app.until("x closes one at once", lambda: toasts() == before - 1)
    ok("toasts: at most 3 of 4, x closed one at once")
    # F2 on an icon tile: the name field takes the keyboard with a caret, Enter renames.
    for _ in range(4):
        app.key("w", ctrl=True)
    app.after("Large icons", "AZDRIVE_LAYOUT", r"large_icons", lambda: app.key("2", primary=True, shift=True))
    app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
              lambda: d.select_item(app, "notes.txt"))
    app.after("F2: the rename field", "AZDRIVE_RENAMING", r"Documents/notes\.txt", lambda: app.key("f2"))
    app.until("the field has the keyboard", lambda: "rename" in d.focused_selector(app))
    def caret():
        v = app.value("get_cursor_state")
        return isinstance(v, dict) and v.get("has_cursor")
    app.until("a caret in the field", caret)
    shot("7-icon-rename")
    app.key("escape")
    try:
        app.until("Escape: the name unchanged", lambda: "notes.txt" in d.item_names(app)
                  and "rename" not in d.focused_selector(app))
    except Failure:
        print("names:", d.item_names(app), "focus:", d.focused_selector(app))
        shot("7b-after-escape")
        raise
    ok("icon view F2: the field took the keyboard with a caret; Escape left the name")
    for theme in ("flat", "flora"):
        for mode in ("light", "dark"):
            app.must("set_theme", theme=theme); app.must("set_mode", mode=mode); app.frame(4)
            shot("8-%s-%s" % (theme, mode))
    print("\n".join(results), flush=True)
    if any(r.startswith("FAIL") for r in results):
        sys.exit(1)
except Failure as err:
    print("FAIL:", err, flush=True)
    print("\n".join(results))
    print(e2e.tail(app.out_path))
    sys.exit(1)
finally:
    app.stop()
