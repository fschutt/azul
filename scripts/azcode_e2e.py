#!/usr/bin/env python3
"""AzCode end to end, headless, over the debug server: azul's CodeView over the
sample workspace (written on the first run into the data folder, code/sample/), and
VSCode's shape around it (the activity bar, the explorer, the tabs, the welcome page).

    1. starts AzCode --sample (AZ_BACKEND=headless, AZ_DEBUG=--debug-port), waits for the
       workspace (AZCODE_READY, the sample written, AZCODE_LISTED / with its entries); no
       file is open yet, so the editor shows the welcome page;
    2. EXPLORER: a click on "src" lists the folder (AZCODE_LISTED src/), a click on
       "main.rs" opens it in a tab (AZCODE_OPENED src/main.rs <lines>): the code view is in
       the tree, the text shows, a keyword wears the keyword class (syntect's colours);
    3. EDIT: a click into the code view, Ctrl/Cmd+Home, typed text - it shows, the tab
       wears the dot of unsaved changes; SAVE: Ctrl/Cmd+S writes it through the drive
       (AZCODE_SAVED), the dot goes;
    4. FIND / REPLACE: Ctrl/Cmd+F, "counts" typed (AZCODE_FOUND >= 3), Ctrl/Cmd+H, "tally",
       Replace all (AZCODE_REPLACED), "tally" shows; UNDO in the code view brings
       "counts" back (one undo step);
    5. A HUNDRED THOUSAND LINES: huge.rs opens (AZCODE_OPENED huge.rs 100001), Ctrl/Cmd+G
       90003 jumps there (the line shows; only the lines in view are in the tree), the
       colours arrive from the background walk; Ctrl/Cmd+End shows the last lines;
    6. a screenshot after each step, flat light; the mode switched to dark at the end.
    7. THE EMPTY START, a second run without --sample on a fresh data folder and without
       --mode (AzCode is dark by default, as VSCode is): the explorer says "You have not yet
       opened a folder." with an Open Folder button, the editor shows the welcome page (Open
       Folder..., Open File..., the keyboard shortcuts);
    8. OPEN FOLDER: the folder dialog answered by the debug server's mock store
       (`file_open`), Mod+O picks a project folder (AZCODE_FOLDER <dir>), the explorer lists
       it (AZCODE_LISTED / 3) and shows its entries, the empty state goes, the welcome page
       stays (no file is open);
    9. TABS, OPEN FILE AND THE SIDE BAR: a click on Cargo.toml opens it in a tab, the tab's
       close button closes it (the welcome page is back); the welcome page's Open File... (the
       mocked dialog again) opens a file outside the folder on its own (AZCODE_FILE), typed
       text is saved in that file's own folder; Mod+B hides the side bar, Mod+B shows it
       again;
    10. QUICK OPEN: Mod+P lists the folder's files (AZCODE_INDEXED), "lib" leaves src/lib.rs,
        Enter opens it.

Usage (after building libazul with the debug server and AzCode; ONE app at a time,
through scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azcode_e2e.py [--bin target/release/AzCode]
        [--debug-port 8791] [--timeout 240] [--out <dir>] [--keep]
"""

import os

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azcode"
EDITOR = "#__azcode_editor"
DIRTY = "__azcode_tab-dirty"
KEYWORD = "__azul-native-code-view-keyword"
LINE = "__azul-native-code-view-line"


def code_shows(app, text):
    """Whether the code view's lines show `text` (the find and replace fields hold the
    searched and the replacing word too, so `shows` alone proves nothing there)."""
    return any(text in t for t in app.texts_within(LINE))


def listed(app, folder):
    """The entry count of the last AZCODE_LISTED line for `folder`."""
    for line in reversed(app.printed("AZCODE_LISTED")):
        name, _, n = line.rpartition(" ")
        if name == folder:
            return int(n)
    return None


def body(args, logs, out):
    binary = e2e.find_binary("AzCode", args.bin, "AZCODE_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG, binary, ["--sample", "--data-dir", data_dir, "--size", "1280x800",
                                "--theme", "flat", "--mode", "light"],
                  args.debug_port, logs, args.timeout)
    try:
        app.until("the window", lambda: app.printed("AZCODE_READY", r".*"))
        app.until("the sample workspace", lambda: (listed(app, "/") or 0) >= 4)
        app.frame(3)
        for name in ("src", "Cargo.toml", "README.md", "huge.rs"):
            if not app.shows(name):
                raise Failure("the explorer does not show %s" % name)
        if not app.has_id("__azcode_welcome"):
            raise Failure("no file is open, but the editor shows no welcome page")
        app.screenshot(os.path.join(out, "1-workspace.png"))

        # ---- the explorer: a folder, a file ----
        app.click(text="src")
        app.until("src listed", lambda: listed(app, "src/") == 2)
        app.click(text="main.rs")
        app.until("main.rs opened", lambda: app.printed("AZCODE_OPENED", r"src/main\.rs \d+"))
        app.frame(3)
        if not app.has_id("__azcode_editor"):
            raise Failure("the code view (#__azcode_editor) is not in the tree")
        if not app.has_id("__azcode_tab-0") or app.has_id("__azcode_welcome"):
            raise Failure("main.rs is not in a tab in front of the welcome page")
        if not app.shows("word_counts"):
            raise Failure("main.rs's text is not shown")
        if not app.nodes_with_class(KEYWORD):
            raise Failure("no run of main.rs wears the keyword colour")
        app.screenshot(os.path.join(out, "2-main-rs.png"))

        # ---- edit and save ----
        app.click(selector=EDITOR)
        app.key("home", primary=True)
        app.must("text_input", text="// azcode was here")
        app.frame(2)
        app.key("return")
        if not app.shows("// azcode was here"):
            raise Failure("the typed text is not shown")
        if not app.nodes_with_class(DIRTY):
            raise Failure("the tab does not wear the dot of unsaved changes")
        app.key("s", primary=True)
        app.until("main.rs saved", lambda: app.printed("AZCODE_SAVED", r"src/main\.rs"))
        app.frame(2)
        if app.nodes_with_class(DIRTY):
            raise Failure("the tab still wears the dot of unsaved changes after the save")
        saved = os.path.join(data_dir, "code", "sample", "src", "main.rs")
        with open(saved, "r", encoding="utf-8") as f:
            if not f.read().startswith("// azcode was here\n"):
                raise Failure("the file on disk does not start with the typed line")
        app.screenshot(os.path.join(out, "3-saved.png"))

        # ---- find, replace all, undo ----
        app.key("f", primary=True)
        app.text_input("#__azcode_find-input", "counts")
        found = int(app.last("AZCODE_FOUND") or "0")
        if found < 3:
            raise Failure("'counts' found %d times in main.rs, expected at least 3" % found)
        app.key("h", primary=True)
        app.text_input("#__azcode_replace-input", "tally")
        app.click(selector="#__azcode_replace-all")
        replaced = int(app.last("AZCODE_REPLACED") or "0")
        if replaced != found:
            raise Failure("replace all replaced %d of %d" % (replaced, found))
        if not code_shows(app, "tally") or code_shows(app, "counts"):
            raise Failure("the replacement is not shown in the code")
        app.screenshot(os.path.join(out, "4-replaced.png"))
        app.click(selector=EDITOR)
        app.key("z", primary=True)
        if code_shows(app, "tally") or not code_shows(app, "counts"):
            raise Failure("one undo did not take back every replacement")
        app.key("escape")

        # ---- a hundred thousand lines ----
        app.click(text="huge.rs")
        app.until("huge.rs opened", lambda: app.printed("AZCODE_OPENED", r"huge\.rs 100001"))
        app.frame(3)
        lines = len(app.nodes_with_class(LINE))
        if not (10 <= lines <= 80):
            raise Failure("%d lines of huge.rs are in the tree - only the lines in view should be" % lines)
        app.key("g", primary=True)
        app.text_input("#__azcode_goto-input", "90003")
        app.key("return")
        app.frame(3)
        if not app.shows("pub fn f9000(x: u64)"):
            raise Failure("go to line 90003 does not show it")
        app.until("the colours of the far lines", lambda: app.nodes_with_class(KEYWORD), interval=0.5)
        app.screenshot(os.path.join(out, "5-line-90003.png"))
        app.click(selector=EDITOR)
        app.key("end", primary=True)
        app.frame(3)
        if not app.shows("line 99996"):
            raise Failure("Ctrl/Cmd+End does not show the last lines")
        app.screenshot(os.path.join(out, "6-the-end.png"))

        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "7-dark.png"))
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


# The project folder of step 7: a file of each kind and a folder (3 entries at the top).
PROJECT = {
    "Cargo.toml": "[package]\nname = \"picked\"\nversion = \"0.1.0\"\n",
    "notes.md": "# Picked\n\nOpened with Mod+O.\n",
    os.path.join("src", "lib.rs"): "pub fn picked() -> u32 {\n    7\n}\n",
}


def folder_run(args, logs, out):
    """7-10: the empty start, Mod+O opens the folder the (mocked) folder dialog answers, a tab
    and its close button, Open File... (a file outside the folder), Mod+B, quick open
    (Mod+P)."""
    binary = e2e.find_binary("AzCode", args.bin, "AZCODE_BIN")
    data_dir = os.path.join(logs, "data-folder")
    os.makedirs(data_dir, exist_ok=True)
    project = os.path.join(logs, "picked-project")
    for name, text in PROJECT.items():
        path = os.path.join(project, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
    # No --mode: the empty start shows AzCode's own default, dark.
    app = e2e.App(TAG + "-folder", binary, ["--data-dir", data_dir, "--size", "1280x800",
                                            "--theme", "flat"],
                  args.debug_port, logs, args.timeout)
    try:
        app.until("the window", lambda: app.printed("AZCODE_READY", r".*"))

        # ---- the empty start: VSCode's empty explorer and its welcome page ----
        app.until("the empty explorer", lambda: app.has_id("__azcode_no-folder"))
        app.frame(2)
        if not app.shows("You have not yet opened a folder."):
            raise Failure("the empty explorer does not say that no folder is open")
        for node in ("__azcode_open-folder", "__azcode_welcome", "__azcode_welcome-open-folder",
                     "__azcode_welcome-open-file", "__azcode_activity-explorer",
                     "__azcode_activity-search", "__azcode_activity-settings"):
            if not app.has_id(node):
                raise Failure("the empty start has no #%s" % node)
        if not app.shows("Keyboard shortcuts"):
            raise Failure("the welcome page lists no keyboard shortcuts")
        app.screenshot(os.path.join(out, "8-empty.png"))

        # ---- Mod+O: the folder the mocked dialog answers ----
        app.must("mock", set={"file_open": {"path": project}})
        app.key("o", primary=True)
        app.until("the picked folder", lambda: project in app.printed("AZCODE_FOLDER"))
        app.until("the picked folder listed", lambda: listed(app, "/") == len(PROJECT))
        app.frame(3)
        for name in ("src", "Cargo.toml", "notes.md"):
            if not app.shows(name):
                raise Failure("the explorer does not show the picked folder's %s" % name)
        if app.has_id("__azcode_no-folder"):
            raise Failure("the empty explorer is still shown after the folder opened")
        if not app.has_id("__azcode_welcome"):
            raise Failure("no file is open, but the welcome page went with the empty explorer")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "9-picked-folder.png"))

        # ---- a file in a tab; the tab's close button ----
        app.click(text="Cargo.toml")
        app.until("Cargo.toml opened", lambda: app.printed("AZCODE_OPENED", r"Cargo\.toml \d+"))
        app.frame(3)
        if not app.has_id("__azcode_tab-0") or app.has_id("__azcode_welcome"):
            raise Failure("Cargo.toml is not in a tab in front of the welcome page")
        if not code_shows(app, "picked"):
            raise Failure("Cargo.toml's text is not shown")
        app.screenshot(os.path.join(out, "10-tab.png"))
        app.click(selector="#__azcode_tab-close-0")
        app.until("the tab closed", lambda: not app.has_id("__azcode_tab-0"))
        if not app.has_id("__azcode_welcome"):
            raise Failure("the welcome page is not back after the last tab closed")

        # ---- Open File...: a file outside the folder opens on its own and saves there ----
        outside = os.path.join(logs, "outside", "scratch.txt")
        os.makedirs(os.path.dirname(outside), exist_ok=True)
        with open(outside, "w", encoding="utf-8") as f:
            f.write("scratch\n")
        app.must("mock", set={"file_open": {"path": outside}})
        app.click(selector="#__azcode_welcome-open-file")
        app.until("the picked file", lambda: outside in app.printed("AZCODE_FILE"))
        app.until("scratch.txt opened", lambda: app.printed("AZCODE_OPENED", r"scratch\.txt \d+"))
        app.frame(3)
        app.click(selector=EDITOR)
        app.key("home", primary=True)
        app.must("text_input", text="more ")
        app.frame(2)
        app.key("s", primary=True)
        app.until("scratch.txt saved", lambda: app.printed("AZCODE_SAVED", r"scratch\.txt"))
        with open(outside, "r", encoding="utf-8") as f:
            if not f.read().startswith("more scratch"):
                raise Failure("the file opened on its own was not saved in its own folder")
        app.click(selector="#__azcode_tab-close-0")
        app.until("its tab closed", lambda: not app.has_id("__azcode_tab-0"))

        # ---- Mod+B hides the side bar and shows it again ----
        app.key("b", primary=True)
        app.until("the side bar hidden", lambda: not app.has_id("__azcode_explorer"))
        app.key("b", primary=True)
        app.until("the side bar back", lambda: app.has_id("__azcode_explorer"))

        # ---- Mod+P: quick open finds src/lib.rs by its letters ----
        app.key("p", primary=True)
        app.until("quick open", lambda: app.has_id("__azcode_quick-open"))
        app.until("the folder indexed", lambda: app.printed("AZCODE_INDEXED", r"\d+"))
        app.text_input("#__azcode_quick-open", "lib")
        app.until("src/lib.rs listed", lambda: app.shows("src/lib.rs"))
        app.screenshot(os.path.join(out, "11-quick-open.png"))
        app.key("return")
        app.until("src/lib.rs opened", lambda: app.printed("AZCODE_OPENED", r"src/lib\.rs \d+"))
        app.frame(3)
        if app.has_id("__azcode_quick-open"):
            raise Failure("quick open is still shown after the file opened")
        if not code_shows(app, "picked"):
            raise Failure("src/lib.rs's text is not shown")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "12-lib-rs.png"))
        app.log("PASS (empty start, open folder, tabs, side bar, quick open)")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


def all_runs(args, logs, out):
    return body(args, logs, out) and folder_run(args, logs, out)


if __name__ == "__main__":
    e2e.run(TAG, all_runs, default_port=8791)
