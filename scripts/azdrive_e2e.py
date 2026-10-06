#!/usr/bin/env python3
"""AzDrive end to end, headless, on a temporary Home folder with the sample files.

Walks Explorer's main flows through azul's debug server and asserts on the node tree, the
node layout, AzDrive's stdout markers and the files on disk:

     1. This PC: the drive tiles, the ribbon's FILE / HOME / SHARE / VIEW / DRIVE;
     2. open the Home drive (double-click its tile);
     3. every layout of VIEW > Layout (the gallery), back to Details;
     4. sort: the Name header twice, then Size;
     5. into Documents; select (click, Ctrl+click, Shift+click), Select all, Escape;
     6. type-ahead ("r" selects report.md);
     7. F2: rename notes.txt to todo.txt in place (the file on disk; End, Backspace over the
        whole name, typing), Undo renames it back;
     8. Ctrl+Shift+N: a new folder (on disk), Escape keeps its name;
     9. Copy / Paste into the new folder, again: the conflict dialog, "Keep both files";
    10. Delete: into the trash folder (on disk), Undo brings it back;
    11. Backspace (up), Alt+Left (back), Alt+Right (forward);
    12. the panes: Preview pane (a text and an image preview), a thumbnail in Large icons,
        Navigation pane off / on, Details pane off / on;
    13. Properties (Alt+Enter) in the in-window sheet, OK;
    14. FILE: the backstage with the Options, Escape;
    15. flora + dark: a screenshot;
    16. Ctrl+A / Ctrl+C with the content pane focused (the engine handed them to the text
        selection until 2026-10-03; steps 5, 7, 9 and 10 use the ribbon's buttons for the
        same commands, so they do not depend on it).

Usage (from the azul repository, after building libazul with the debug server and AzDrive):

    python3 scripts/azdrive_e2e.py [--bin target/release/AzDrive] [--debug-port 8781]
        [--timeout 180] [--out /tmp/azdrive-shots] [--keep-logs]

Run it through the capped runner on a small machine:

    <scratchpad>/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/azdrive-e2e.log -- \\
        python3 scripts/azdrive_e2e.py --bin target/release/AzDrive

Every op that changes state is followed by `wait_frame`s and an `until` on what it must cause;
every screenshot waits for the animations to finish (`settle`).
Every key_down has its key_up (the E2E key_up rule).
"""

import argparse
import glob
import os
import re
import shutil
import sys
import tempfile

import azlin_e2e as e2e
from azlin_e2e import Failure

# The key of the platform's shortcut modifier (KeyModifiers::primary_down):
# Cmd on macOS, Ctrl elsewhere.
PRIMARY = "meta" if sys.platform == "darwin" else "ctrl"

# AzDrive names its ids and classes with the app prefix `__azdrive_` (src/ids.rs, the wave-6
# prefix ruling); a binary built before that used `azdrive-<class>` and bare ids. The script
# finds out once which naming the window uses (NAMING) and asks C / I for every name.
NAMING = {"prefixed": True}


def C(name):
    """A class by its short name: `drive` -> `__azdrive_drive` (`azdrive-drive` before)."""
    if NAMING["prefixed"]:
        return "__azdrive_" + name.replace("-", "_")
    return "azdrive-" + name


def I(name):
    """An id by its short name: `rename-field` -> `__azdrive_rename_field` (bare before)."""
    if NAMING["prefixed"]:
        return "__azdrive_" + name.replace("-", "_")
    return name


def log(line):
    print("[azdrive-e2e] %s" % line, flush=True)


class Drive(e2e.App):
    """AzDrive under its debug server: the shared driver (`scripts/azlin_e2e.py`) with AzDrive's
    own reading of the window - two frames after an op, the texts of the text nodes, "has" as
    laid out with a size - and its ribbon."""

    def frame(self, n=2):
        super().frame(n)

    def texts(self, every_dom=False):
        if every_dom:
            return super().texts(every_dom)
        return [n.get("text") for n in self.hierarchy() if n.get("text")]

    def has(self, selector, every_dom=False):
        if every_dom:
            return super().has(selector, every_dom)
        answer = self.op("get_node_layout", selector=selector)
        if not isinstance(answer, dict) or answer.get("status") == "error":
            return False
        value = (answer.get("data") or {}).get("value") or {}
        rect = value.get("rect") or {}
        return rect.get("width", 0) > 0 and rect.get("height", 0) > 0

    def ribbon(self, label):
        """Clicks the HOME tab's button whose label starts with `label`, then shows VIEW again
        (where the walk keeps the ribbon, so "New folder" in the list is not the ribbon's)."""
        def found():
            for n in self.hierarchy():
                if (n.get("text") or "").startswith(label):
                    return n.get("parent", n["index"])
            return None
        self.click_exact("HOME")
        node = self.until('the ribbon\'s "%s"' % label, found)
        # The HOME tab's groups slide in: a click lands where the button is painted (the
        # settle of App.click, 05ef3a8f4).
        self.settle(limit=2.0)
        self.must("click", node_id=node, button="left")
        self.frame()
        self.click_exact("VIEW")

    def screenshot(self, path):
        """Waits for the animations first (the details pane slides its rows in)."""
        self.settle()
        super().screenshot(path)


# Explorer's layout keys: Ctrl+Shift+<digit>. (The ribbon's Layout gallery shows a strip that
# clips its later cells; the keys reach every layout.)
LAYOUTS = [
    ("1", "extra_large_icons"),
    ("2", "large_icons"),
    ("3", "medium_icons"),
    ("4", "small_icons"),
    ("5", "list"),
    ("7", "tiles"),
    ("8", "content"),
    ("6", "details"),
]


def item_names(app):
    """The names of the folder's items, in the order the view shows them."""
    names = []
    nodes = app.hierarchy()
    by_index = {n["index"]: n for n in nodes}
    for n in nodes:
        if C("name") in (n.get("classes") or []):
            if n.get("text"):
                names.append(n["text"])
                continue
            for child in n.get("children") or []:
                text = by_index.get(child, {}).get("text")
                if text:
                    names.append(text)
    return names


def run(args, logs):
    binary = e2e.find_binary("AzDrive", args.bin, "AZDRIVE_BIN")
    log("AzDrive: %s" % binary)
    log("logs and data: %s" % logs)
    out = args.out or os.path.join(logs, "shots")
    os.makedirs(out, exist_ok=True)

    home = os.path.join(logs, "home")
    os.makedirs(home)
    env = {
        "AZDRIVE_HOME": home,
        "AZDRIVE_DOWNLOADS": os.path.join(logs, "downloads"),
        "AZDRIVE_SETTINGS": os.path.join(logs, "settings"),  # an older build's settings folder
        "AZLIN_DATA": os.path.join(logs, "data"),  # the data tree (azul-appkit's data root)
        "AZUL_DRIVES": os.path.join(logs, "config", "drives.json"),
        "AZDRIVE_DIALOGS": "inline",
    }
    app = Drive("azdrive", binary, ["--sample", "--screen", "this-pc", "--theme", "flat", "--mode", "light"],
                args.debug_port, logs, args.timeout, extra_env=env)
    docs = os.path.join(home, "Documents")
    try:
        # 1. This PC.
        app.until("the This PC view", lambda: app.printed("AZDRIVE_PLACE", r"this-pc"))
        app.until("the debug server", lambda: app.op("get_dom_tree"))
        app.must("resize", width=1280.0, height=800.0)
        app.frame(3)
        app.until("the drive tiles", lambda: app.nodes_with_class("__azdrive_drive") or
                  app.nodes_with_class("azdrive-drive"))
        NAMING["prefixed"] = bool(app.nodes_with_class("__azdrive_drive"))
        log("names: %s" % ("__azdrive_ prefixed" if NAMING["prefixed"] else "unprefixed (older build)"))
        for tab in ("FILE", "HOME", "SHARE", "VIEW", "DRIVE"):
            app.until("the ribbon tab %s" % tab, lambda: app.exact(tab) is not None)
        app.until("This PC's groups", lambda: app.shows("Devices and drives"))
        if not app.has("#shell-tree") or not app.has("#shell-content"):
            raise Failure("the navigation pane and the content pane are not laid out")
        app.screenshot(os.path.join(out, "01-this-pc.png"))
        log("1. This PC: drive tiles, the five ribbon tabs, the navigation and content panes")

        # 2. The Home drive.
        # On the tile's icon: its centre is the capacity bar, a ProgressBar, which is a
        # VirtualView of its own DOM - and a Hover event aimed into a child DOM does not
        # bubble out to the tile in the parent DOM (engine, reported to HEADLESS6).
        tile = app.nodes_with_class(C("drive"))[0]
        r = app.value("get_node_layout", node_id=tile)["rect"]
        listed = app.after("the Home drive's listing", "AZDRIVE_LISTED", r"home / \d+",
                           lambda: (app.must("double_click", x=r["x"] + 24.0,
                                             y=r["y"] + r["height"] / 2.0), app.frame()))
        log("2. opened the Home drive (%s)" % listed)
        app.until("Documents in the view", lambda: "Documents" in item_names(app))
        if ".hidden-settings" in item_names(app):
            raise Failure("a hidden item shows while Hidden items is off")

        # 3. Every layout (Ctrl+Shift+1..8), and one through the ribbon's gallery.
        for digit, name in LAYOUTS:
            app.after("the layout %s" % name, "AZDRIVE_LAYOUT", re.escape(name),
                      lambda: app.key(digit, primary=True, shift=True))
            app.until("the %s view" % name, lambda: C("layout-" + name) in app.classes())
            app.until("the items of %s" % name, lambda: len(app.nodes_with_class(C("item"))) >= 5)
            if name in ("large_icons", "tiles"):
                app.screenshot(os.path.join(out, "03-%s.png" % name))
        app.click_exact("VIEW")
        app.after("Large icons from the gallery", "AZDRIVE_LAYOUT", r"large_icons",
                  lambda: app.click_exact("Large icons"))
        app.after("Details from the status bar's switch", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.key("6", primary=True, shift=True))
        log("3. all eight layouts render the folder (keys and the ribbon's gallery)")

        # 4. Sort by the Name header (twice: descending), then by Size.
        app.until("the Details header", lambda: app.has("#" + I("details-header")))
        before = item_names(app)
        app.after("sort by name, descending", "AZDRIVE_SORT", r"Name desc",
                  lambda: app.click_exact("Name"))
        app.until("the reversed order",
                  lambda: [n for n in item_names(app)] == list(reversed(before)) or
                  item_names(app)[0] == sorted(before, key=str.lower)[-1])
        app.after("sort by name, ascending", "AZDRIVE_SORT", r"Name asc",
                  lambda: app.click_exact("Name"))
        app.after("sort by size", "AZDRIVE_SORT", r"Size asc", lambda: app.click_exact("Size"))
        app.after("sort by name again", "AZDRIVE_SORT", r"Name (asc|desc)",
                  lambda: app.click_exact("Name"))
        if app.printed("AZDRIVE_SORT", r"Name (asc|desc)")[-1] == "desc":
            app.after("sort by name, ascending", "AZDRIVE_SORT", r"Name asc",
                      lambda: app.click_exact("Name"))
        log("4. the Name and Size headers sort (a second click reverses)")

        # 5. Documents; selection.
        app.after("the Documents listing", "AZDRIVE_LISTED", r"home Documents/ \d+",
                  lambda: app.click_exact("Documents", double=True))
        app.until("notes.txt", lambda: "notes.txt" in item_names(app))
        app.after("one selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: app.click_exact("notes.txt"))
        app.after("Ctrl+click", "AZDRIVE_SELECTED", r"2 .*", lambda: (
            app.op("key_down", key=PRIMARY, modifiers={PRIMARY: True}),
            app.click_exact("report.md"),
            app.op("key_up", key=PRIMARY, modifiers={PRIMARY: False})))
        app.after("Shift+click", "AZDRIVE_SELECTED", r"3 .*", lambda: (
            app.op("key_down", key="shift", modifiers={"shift": True}),
            app.click_exact("data.csv"),
            app.op("key_up", key="shift", modifiers={"shift": False})))
        app.after("Escape", "AZDRIVE_SELECTED", r"0 -", lambda: app.key("escape"))
        app.after("Select all", "AZDRIVE_SELECTED", r"3 .*", lambda: app.ribbon("Select all"))
        app.after("Escape", "AZDRIVE_SELECTED", r"0 -", lambda: app.key("escape"))
        log("5. click, Ctrl+click, Shift+click, Select all and Escape select as Explorer does")

        # 6. Type-ahead.
        app.after("type-ahead r", "AZDRIVE_SELECTED", r"1 Documents/report\.md",
                  lambda: app.key("r"))
        log("6. typing r selects report.md")

        # 7. F2: rename in place; Ctrl+Z.
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: app.click_exact("notes.txt"))
        app.after("the rename field", "AZDRIVE_RENAMING", r"Documents/notes\.txt",
                  lambda: app.key("f2"))
        app.until("the rename field laid out", lambda: app.has("#" + I("rename-field")))
        app.must("focus_node", selector="#" + I("rename-field"))
        app.frame()
        app.key("end")
        for _ in range(len("notes.txt")):
            app.key("backspace")
        app.must("text_input", text="todo.txt")
        app.frame()
        app.after("the rename", "AZDRIVE_DONE", r"renamed Documents/todo\.txt",
                  lambda: app.key("enter"))
        app.until("todo.txt on disk", lambda: os.path.isfile(os.path.join(docs, "todo.txt")))
        if os.path.exists(os.path.join(docs, "notes.txt")):
            raise Failure("notes.txt is still there after the rename")
        app.ribbon("Undo")
        app.until("notes.txt back on disk (Undo)",
                  lambda: os.path.isfile(os.path.join(docs, "notes.txt")))
        log("7. F2 renamed notes.txt to todo.txt on disk; Undo renamed it back")

        # 8. A new folder.
        app.after("a new folder", "AZDRIVE_DONE", r"created Documents/New folder/",
                  lambda: app.key("n", primary=True, shift=True))
        app.until("the new folder on disk", lambda: os.path.isdir(os.path.join(docs, "New folder")))
        app.until("its rename field", lambda: app.has("#" + I("rename-field")))
        app.key("escape")
        app.until("the field gone", lambda: not app.has("#" + I("rename-field")))
        log("8. Ctrl+Shift+N made New folder on disk; Escape kept its name")

        # 9. Copy / paste; a conflict; Keep both.
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: app.click_exact("notes.txt"))
        app.after("Copy", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.ribbon("Copy"))
        app.after("into New folder", "AZDRIVE_LISTED", r"home Documents/New folder/ \d+",
                  lambda: app.click_exact("New folder", double=True))
        target = os.path.join(docs, "New folder")
        app.after("the paste", "AZDRIVE_TRANSFER", r"\d+ done 1", lambda: app.ribbon("Paste"))
        app.until("the copy on disk", lambda: os.path.isfile(os.path.join(target, "notes.txt")))
        app.after("the conflict", "AZDRIVE_TRANSFER", r"\d+ conflict 1",
                  lambda: app.ribbon("Paste"))
        app.until("the conflict dialog", lambda: app.has("#" + I("conflict")))
        app.screenshot(os.path.join(out, "09-conflict.png"))
        app.after("keep both", "AZDRIVE_TRANSFER", r"\d+ done 1",
                  lambda: (app.must("click", selector="#" + I("conflict-keep-both")), app.frame()))
        app.until("notes (2).txt on disk", lambda: os.path.isfile(os.path.join(target, "notes (2).txt")))
        log("9. Copy / Paste copied into New folder; a second paste asked, Keep both made notes (2).txt")

        # 10. Delete into the trash; Ctrl+Z.
        app.after("notes (2).txt selected", "AZDRIVE_SELECTED", r"1 .*notes \(2\)\.txt",
                  lambda: app.click_exact("notes (2).txt"))
        app.after("the delete", "AZDRIVE_DELETED", r"1", lambda: app.key("delete"))
        app.until("notes (2).txt gone", lambda: not os.path.exists(os.path.join(target, "notes (2).txt")))
        trashed = glob.glob(os.path.join(home, ".azdrive-trash", "*", "Documents", "New folder", "notes (2).txt"))
        if not trashed:
            raise Failure("the deleted file is not in the trash folder")
        app.ribbon("Undo")
        app.until("notes (2).txt back (Undo)",
                  lambda: os.path.isfile(os.path.join(target, "notes (2).txt")))
        log("10. Delete moved the file into .azdrive-trash; Undo brought it back")

        # 11. Up, Back, Forward.
        app.after("Backspace (up)", "AZDRIVE_PLACE", r"home Documents/", lambda: app.key("backspace"))
        app.after("Alt+Left (back)", "AZDRIVE_PLACE", r"home Documents/New folder/",
                  lambda: app.key("left", alt=True))
        app.after("Alt+Right (forward)", "AZDRIVE_PLACE", r"home Documents/",
                  lambda: app.key("right", alt=True))
        log("11. Backspace went up, Alt+Left back, Alt+Right forward")

        # 12. The panes.
        app.click_exact("VIEW")
        app.after("the preview pane", "AZDRIVE_PANES", r"true true true",
                  lambda: app.click_exact("Preview pane"))
        app.until("the preview pane laid out", lambda: app.has("#shell-preview"))
        app.after("a text preview", "AZDRIVE_PREVIEW", r"text Documents/notes\.txt",
                  lambda: app.click_exact("notes.txt"))
        app.until("the text in the preview", lambda: app.has("#" + I("preview-text")))
        app.after("up to Home", "AZDRIVE_PLACE", r"home /", lambda: app.key("backspace"))
        app.after("Pictures", "AZDRIVE_LISTED", r"home Pictures/ \d+",
                  lambda: app.click_exact("Pictures", double=True))
        app.after("an image preview", "AZDRIVE_PREVIEW", r"image Pictures/gradient\.png",
                  lambda: app.click_exact("gradient.png"))
        app.until("the image in the preview", lambda: app.has("#" + I("preview-image")))
        app.screenshot(os.path.join(out, "12-preview.png"))
        # Large icons show the picture as a thumbnail.
        app.after("a thumbnail", "AZDRIVE_THUMBNAIL", r"Pictures/gradient\.png",
                  lambda: app.key("2", primary=True, shift=True))
        app.until("the thumbnail drawn", lambda: C("thumbnail") in app.classes())
        app.screenshot(os.path.join(out, "12-thumbnails.png"))
        app.after("back to Details", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.key("6", primary=True, shift=True))
        app.after("the navigation pane off", "AZDRIVE_PANES", r"false true true",
                  lambda: app.click_exact("Navigation pane"))
        app.until("no tree", lambda: not app.has("#shell-tree"))
        app.after("the navigation pane on", "AZDRIVE_PANES", r"true true true",
                  lambda: app.click_exact("Navigation pane"))
        app.until("the tree back", lambda: app.has("#shell-tree"))
        app.after("the details pane off", "AZDRIVE_PANES", r"true true false",
                  lambda: app.click_exact("Details pane"))
        app.until("no details pane", lambda: not app.has("#shell-details"))
        app.after("the details pane on", "AZDRIVE_PANES", r"true true true",
                  lambda: app.click_exact("Details pane"))
        app.until("the details pane back", lambda: app.has("#shell-details"))
        log("12. Preview pane (text and image), Navigation pane and Details pane toggle")

        # 13. Properties.
        app.after("gradient.png selected", "AZDRIVE_SELECTED", r"1 Pictures/gradient\.png",
                  lambda: app.click_exact("gradient.png"))
        app.after("Properties", "AZDRIVE_DONE", r"properties 1", lambda: app.key("enter", alt=True))
        app.until("the Properties sheet", lambda: app.has("#" + I("properties")))
        app.until("its title", lambda: app.shows("gradient.png Properties"))
        app.screenshot(os.path.join(out, "13-properties.png"))
        app.click_exact("OK")
        app.until("the sheet closed", lambda: not app.has("#" + I("properties")))
        log("13. Alt+Enter opened Properties; OK closed it")

        # 13b. A WAV previews (and could play through azul's AudioSink).
        app.after("up to Home", "AZDRIVE_PLACE", r"home /", lambda: app.key("backspace"))
        app.after("Music", "AZDRIVE_LISTED", r"home Music/ \d+",
                  lambda: app.click_exact("Music", double=True))
        app.after("an audio preview", "AZDRIVE_PREVIEW", r"audio Music/chime\.wav",
                  lambda: app.click_exact("chime.wav"))
        app.until("the sound's preview", lambda: app.has("#" + I("preview-audio")))
        app.until("its Play button", lambda: app.has("#" + I("preview-play")))
        log("13b. chime.wav previews as a sound with Play")

        # 14. FILE: the backstage and the Options.
        app.click_exact("FILE")
        app.until("the Options", lambda: app.has("#" + I("settings")))
        app.screenshot(os.path.join(out, "14-options.png"))
        if NAMING["prefixed"]:
            # The Options are azul-appkit's page: Appearance saves the theme into the data
            # tree (drive/settings.json), so it is there on the next start.
            app.click_exact("Appearance")
            app.until("the Appearance section", lambda: app.shows("Theme"))
            app.after("the theme saved", "AZDRIVE_SETTINGS_SAVED", r"drive/settings\.json",
                      lambda: app.click_exact("Flora"))
            saved = os.path.join(logs, "data", "drive", "settings.json")
            app.until("flora in the settings file",
                      lambda: os.path.isfile(saved) and '"flora"' in open(saved).read())
            app.click_exact("Flat")
            app.until("flat in the settings file", lambda: '"flat"' in open(saved).read())
        app.key("escape")
        app.until("the backstage closed", lambda: not app.has("#" + I("settings")))
        log("14. FILE opened the Options (the theme saved into the data tree); Escape closed them")

        # 15. Flora, dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(4)
        app.screenshot(os.path.join(out, "15-flora-dark.png"))
        log("15. flora + dark")

        # 16. The editing keys with the content pane focused (a click focuses it): Ctrl+A,
        # Ctrl+C, Ctrl+Z reach Explorer's keyboard. (Engine, 2026-10-03: with ANY node
        # focused, core's handle_key_down took Copy / Cut / Paste / Select all for the text
        # selection and skipped the callbacks - the window's VirtualKeyDown never saw them.)
        app.after("up to Home", "AZDRIVE_PLACE", r"home /", lambda: app.key("backspace"))
        app.after("an item selected", "AZDRIVE_SELECTED", r"1 Documents/",
                  lambda: app.click_exact("Documents"))
        app.after("Ctrl+A", "AZDRIVE_SELECTED", r"[2-9] .*", lambda: app.key("a", primary=True))
        app.after("Ctrl+C", "AZDRIVE_CLIPBOARD", r"copy [2-9]", lambda: app.key("c", primary=True))
        app.key("escape")
        log("16. Ctrl+A and Ctrl+C reach Explorer's keyboard while the content pane has focus")
        log("PASS: AzDrive browsed, laid out, sorted, selected, renamed, created, copied, "
            "resolved a conflict, deleted and undid, walked the history, toggled the panes, "
            "showed Properties and the Options, took the editing keys")
        return True
    except Failure:
        for name, path in (("stdout", app.out_path), ("stderr", app.err_path)):
            print("\n----- azdrive %s (tail) -----\n%s" % (name, e2e.tail(path)))
        raise
    finally:
        app.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8781)
    parser.add_argument("--timeout", type=float, default=180)
    parser.add_argument("--out")
    parser.add_argument("--keep-logs", action="store_true")
    args = parser.parse_args()
    logs = tempfile.mkdtemp(prefix="azdrive-e2e-")
    ok = False
    try:
        ok = run(args, logs)
    except Failure as e:
        log("FAIL: %s" % e)
    finally:
        if ok and not args.keep_logs and not args.out:
            shutil.rmtree(logs, ignore_errors=True)
        else:
            log("kept %s" % logs)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
