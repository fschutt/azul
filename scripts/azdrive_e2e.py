#!/usr/bin/env python3
"""AzDrive end to end, headless, on a temporary Home folder with the sample files.

Walks Explorer's main flows through azul's debug server and asserts on the node tree, the
node layout, AzDrive's stdout markers and the files on disk:

     1. This PC: the drive tiles; Explorer's chrome - the navigation row, the command bar with
        This PC's drive commands and the pane switches; Finder's body - the source list's
        FAVORITES (Quick access, the sample's Documents / Pictures / Music), LOCATIONS (This PC,
        Home, Azlin), CLOUD ("Add S3 drive"), This PC's row selected, the path bar and the
        status line at the foot of the content's leaf;
     1b. the source list: a click on Documents goes there (its row selected, the path bar's
        trail), Down walks to Pictures, Enter opens it, Left climbs to FAVORITES' title, Left
        closes the section, Right opens it again; F6 lands on the pane, Down enters the list at
        the selected row, Tab leaves it for the + button;
     2. open the Home drive (double-click its tile);
     3. every layout (Ctrl+Shift+1..8; the icon layouts are azul's IconGrid), then the
        command bar's Large icons / List / Details;
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
        Navigation pane off / on, the Details pane in the preview pane's place (they share
        the right side) and off, Alt+P: the preview pane again;
    13. Properties (Alt+Enter) in the in-window sheet, OK;
    14. the command bar's Options (the gear): the backstage with the Options, Escape;
    15. flora + dark: a screenshot;
    16. Ctrl+A / Ctrl+C with the content pane focused (the engine handed them to the text
        selection until 2026-10-03; steps 5, 7, 9 and 10 use the command bar's tools for the
        same commands, so they do not depend on it);
    17. the looks: This PC, Documents in Details and in Large icons, in flat and flora, by day
        and at night (17-<theme>-<mode>-<view>.png).

The source list shows the sample's Documents, Pictures and Music too (FAVORITES), and the path bar
the open folder's trail: a folder's ITEM is clicked through its name label (`open_item`,
`select_item`), never by the first node showing the text.

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
    laid out with a size - and its command bar."""

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

    def command(self, name):
        """Clicks the command bar's tool `name` (`#__azdrive_cmd_<name>`, src/ids.rs) - by its
        id, so "Copy" in the list is never the tool's."""
        selector = "#" + I("cmd-" + name)
        self.until('the command bar\'s "%s"' % name, lambda: self.has(selector))
        self.click(selector=selector)

    def screenshot(self, path):
        """Waits for the animations first (the details pane slides its rows in)."""
        self.settle()
        super().screenshot(path)


# Explorer's layout keys: Ctrl+Shift+<digit> (the command bar has three of the views, See more >
# Layout all eight in a menu; the keys reach every layout).
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


# The icon layouts are azul's IconGrid (layout/src/widgets/icon_grid.rs): its items, their
# labels and the boxes of their icons or thumbnails carry the widget's classes.
GRID_ITEM = "__azul-native-icon-grid-item"
GRID_LABEL = "__azul-native-icon-grid-label"
GRID_THUMB = "__azul-native-icon-grid-thumb"


def item_count(app):
    """How many of the folder's items the view shows (hand-built, or the IconGrid's)."""
    return sum(1 for n in app.hierarchy()
               if C("item") in (n.get("classes") or []) or GRID_ITEM in (n.get("classes") or []))


def thumbnail_drawn(app):
    """Whether a picture shows as a thumbnail: a hand-built cell's (`__azdrive_thumbnail`), or
    an image in an IconGrid item's icon box."""
    nodes = app.hierarchy()
    if any(C("thumbnail") in (n.get("classes") or []) for n in nodes):
        return True
    by_index = {n["index"]: n for n in nodes}
    for n in nodes:
        if GRID_THUMB in (n.get("classes") or []):
            for child in n.get("children") or []:
                kind = str(by_index.get(child, {}).get("type") or "").lower()
                if "image" in kind or kind == "img":
                    return True
    return False


def item_node(app, name):
    """The node of the folder's item labelled `name` - its name label, so never a command bar
    tool or a crumb showing the same text ("New folder" is a tool too)."""
    nodes = app.hierarchy()
    by_index = {n["index"]: n for n in nodes}
    for n in nodes:
        classes = n.get("classes") or []
        if C("name") in classes or GRID_LABEL in classes:
            texts = [n.get("text")] + [by_index.get(c, {}).get("text")
                                       for c in n.get("children") or []]
            if name in texts:
                return n["index"]
    return None


def open_item(app, name):
    """Double-clicks the folder's item `name` (on the nearest node from its label up that has a
    box, as App.click_exact does)."""
    node = app.until('the item "%s"' % name, lambda: item_node(app, name))
    app.settle(limit=2.0)
    parents = {n["index"]: n.get("parent") for n in app.hierarchy()}
    while isinstance(node, int) and node >= 0:
        answer = app.op("double_click", node_id=node, button="left")
        if isinstance(answer, dict) and answer.get("status") != "error":
            break
        node = parents.get(node)
    else:
        raise Failure('double_click on the item "%s": no node from its label up has a box' % name)
    app.frame()


def select_item(app, name):
    """Clicks the folder's item `name` (its name label, never the source list's or the path
    bar's row of the same name)."""
    node = app.until('the item "%s"' % name, lambda: item_node(app, name))
    app.settle(limit=2.0)
    parents = {n["index"]: n.get("parent") for n in app.hierarchy()}
    while isinstance(node, int) and node >= 0:
        answer = app.op("click", node_id=node, button="left")
        if isinstance(answer, dict) and answer.get("status") != "error":
            break
        node = parents.get(node)
    else:
        raise Failure('click on the item "%s": no node from its label up has a box' % name)
    app.frame()


def node_by_id(app, dom_id):
    """The node of the window whose DOM id is `dom_id`, or None."""
    for n in app.hierarchy():
        if n.get("id") == dom_id:
            return n
    return None


def classes_of(app, dom_id):
    """The classes of the node whose DOM id is `dom_id` (none when it is not there)."""
    return (node_by_id(app, dom_id) or {}).get("classes") or []


def focused_id(app):
    """The DOM id of the node with the keyboard (the debug server's `get_focus_state`, whose
    selector carries `#<id>`), or None."""
    for d in e2e.dicts(app.op("get_focus_state")):
        node = d.get("focused_node")
        if isinstance(node, dict):
            m = re.search(r"#([A-Za-z0-9_-]+)", node.get("selector") or "")
            return m.group(1) if m else None
    return None


def item_names(app):
    """The names of the folder's items, in the order the view shows them."""
    names = []
    nodes = app.hierarchy()
    by_index = {n["index"]: n for n in nodes}
    for n in nodes:
        classes = n.get("classes") or []
        if C("name") in classes or GRID_LABEL in classes:
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
    # Every setting is a switch (src/args.rs); only the engine's AZ_BACKEND / AZ_DEBUG are
    # variables (the shared driver sets them).
    switches = [
        "--sample", "--screen", "this-pc", "--theme", "flat", "--mode", "light",
        "--home", home,
        "--downloads", os.path.join(logs, "downloads"),
        "--data-dir", os.path.join(logs, "data"),  # the data tree (azul-appkit's data root)
        "--drives", os.path.join(logs, "config", "drives.json"),
        "--dialogs", "inline",
    ]
    app = Drive("azdrive", binary, switches, args.debug_port, logs, args.timeout)
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
        # Explorer's chrome: the navigation row over the command bar (This PC's drive
        # commands, the views, See more; the pane switches and the gear at its right end).
        app.until("the command bar", lambda: app.has("#" + I("command-bar")))
        for tool in ("add-drive", "add-folder", "remove-drive", "sort", "layout-icons",
                     "layout-list", "layout-details", "more", "navigation-pane",
                     "preview-pane", "details-pane", "options"):
            app.until("the command bar's %s" % tool, lambda: app.has("#" + I("cmd-" + tool)))
        if not app.nodes_with_class("__azul-native-address-bar-nav"):
            raise Failure("the navigation row has no Back / Forward / Up")
        app.until("the navigation pane", lambda: app.has("#" + I("nav-pane")))
        # Finder's source list (src/ui_sidebar.rs): the section titles, the places that are
        # always there, the sample's standard folders in Home (FAVORITES), the drives.
        for row in ("favorites", "locations", "cloud", "quick-access", "this-pc", "drive-home",
                    "drive-azlin", "add-drive", "fav-documents", "fav-pictures", "fav-music"):
            app.until("the source list's %s" % row, lambda: app.has("#" + I("side-" + row)))
        for title in ("Favorites", "Locations", "Cloud"):
            app.until("the section title %s" % title, lambda: app.exact(title) is not None)
        if C("side-selected") not in classes_of(app, I("side-this-pc")):
            raise Failure("This PC's row in the source list is not the selected one")
        if app.has("#" + I("side-eject-home")):
            raise Failure("the Home drive has an eject button (it can never be removed)")
        # The content is a leaf on the page, Finder's path bar and status line at its foot.
        app.until("the leaf", lambda: app.has("#" + I("leaf")))
        app.until("the path bar", lambda: app.has("#" + I("path-bar")))
        app.until("the status line", lambda: app.has("#" + I("status-line")))
        app.until("the status line's count of the drives",
                  lambda: any("drives" in t for t in app.texts_within("#" + I("status-line"))))
        app.until("This PC's groups", lambda: app.shows("Devices and drives"))
        if not app.has("#shell-tree") or not app.has("#shell-content"):
            raise Failure("the navigation pane and the content pane are not laid out")
        app.screenshot(os.path.join(out, "01-this-pc.png"))
        log("1. This PC: drive tiles, the navigation row, the command bar, the source list "
            "(FAVORITES, LOCATIONS, CLOUD; This PC selected), the leaf with its path bar and "
            "status line")

        # 1b. The source list: a click goes, the arrows walk the rows, Enter opens one, Left
        # climbs and closes, Right opens; F6 lands on the pane, Down enters the list at the
        # selected row, Tab leaves it for the + button.
        app.after("Documents from the source list", "AZDRIVE_PLACE", r"home Documents/",
                  lambda: app.click(selector="#" + I("side-fav-documents")))
        app.until("Documents' row selected",
                  lambda: C("side-selected") in classes_of(app, I("side-fav-documents")))
        app.until("the path bar's trail to Documents",
                  lambda: "Documents" in app.texts_within("#" + I("path-bar")))
        app.until("the keyboard on Documents' row",
                  lambda: focused_id(app) == I("side-fav-documents"))
        app.key("down")
        app.until("Down: the keyboard on Pictures' row",
                  lambda: focused_id(app) == I("side-fav-pictures"))
        app.after("Enter opens Pictures", "AZDRIVE_PLACE", r"home Pictures/",
                  lambda: app.key("enter"))
        app.until("Pictures' row selected",
                  lambda: C("side-selected") in classes_of(app, I("side-fav-pictures")))
        app.key("left")
        app.until("Left: the keyboard on FAVORITES' title",
                  lambda: focused_id(app) == I("side-favorites"))
        app.key("left")
        app.until("Left: FAVORITES closed", lambda: not app.has("#" + I("side-fav-documents")))
        app.key("right")
        app.until("Right: FAVORITES open", lambda: app.has("#" + I("side-fav-documents")))
        app.after("This PC from the source list", "AZDRIVE_PLACE", r"this-pc",
                  lambda: app.click(selector="#" + I("side-this-pc")))
        # F6 from the source list goes on to the content pane, and round to the source list.
        app.key("f6")
        app.until("F6: the keyboard on the content pane",
                  lambda: focused_id(app) == "shell-content")
        for _ in range(3):
            app.key("f6")
            if focused_id(app) == "shell-tree":
                break
        app.until("F6 round: the keyboard on the navigation pane",
                  lambda: focused_id(app) == "shell-tree")
        app.key("down")
        app.until("Down: the keyboard on the selected row (This PC)",
                  lambda: focused_id(app) == I("side-this-pc"))
        app.key("tab")
        app.until("Tab: the keyboard on the + button", lambda: focused_id(app) == I("side-add"))
        if app.printed("AZDRIVE_SELECTED", r"[1-9].*"):
            raise Failure("a key in the source list selected items of the content")
        log("1b. the source list: a click went to Documents, Down walked to Pictures, Enter "
            "opened it, Left climbed and closed FAVORITES, Right opened it; F6, Down, Tab")

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

        # 3. Every layout (Ctrl+Shift+1..8), then the command bar's three views.
        for digit, name in LAYOUTS:
            app.after("the layout %s" % name, "AZDRIVE_LAYOUT", re.escape(name),
                      lambda: app.key(digit, primary=True, shift=True))
            app.until("the %s view" % name, lambda: C("layout-" + name) in app.classes())
            app.until("the items of %s" % name, lambda: item_count(app) >= 5)
            if name in ("large_icons", "tiles"):
                app.screenshot(os.path.join(out, "03-%s.png" % name))
        app.after("Large icons from the command bar", "AZDRIVE_LAYOUT", r"large_icons",
                  lambda: app.command("layout-icons"))
        app.until("the icon grid (azul's IconGrid)", lambda: app.has("#" + I("icon-grid")))
        app.until("the grid's items", lambda: len(app.nodes_with_class(GRID_ITEM)) >= 5)
        app.after("List from the command bar", "AZDRIVE_LAYOUT", r"list",
                  lambda: app.command("layout-list"))
        app.after("Details from the command bar", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.command("layout-details"))
        log("3. all eight layouts render the folder (the icon layouts on azul's IconGrid); the "
            "command bar's Large icons / List / Details switch them")

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
                  lambda: open_item(app, "Documents"))
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
        app.after("Select all", "AZDRIVE_SELECTED", r"3 .*", lambda: app.command("select-all"))
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
        app.command("undo")
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
        app.after("Copy", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.command("copy"))
        # The item, not the command bar's "New folder" tool (the first text "New folder").
        app.after("into New folder", "AZDRIVE_LISTED", r"home Documents/New folder/ \d+",
                  lambda: open_item(app, "New folder"))
        target = os.path.join(docs, "New folder")
        app.after("the paste", "AZDRIVE_TRANSFER", r"\d+ done 1", lambda: app.command("paste"))
        app.until("the copy on disk", lambda: os.path.isfile(os.path.join(target, "notes.txt")))
        app.after("the conflict", "AZDRIVE_TRANSFER", r"\d+ conflict 1",
                  lambda: app.command("paste"))
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
        app.command("undo")
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

        # 12. The panes: the preview pane and the details pane share the window's right side
        # (Explorer's rule), so the preview pane's switch takes the details pane away.
        app.after("the preview pane", "AZDRIVE_PANES", r"true true false",
                  lambda: app.command("preview-pane"))
        app.until("the preview pane laid out", lambda: app.has("#shell-preview"))
        app.until("no details pane beside it", lambda: not app.has("#shell-details"))
        app.after("a text preview", "AZDRIVE_PREVIEW", r"text Documents/notes\.txt",
                  lambda: app.click_exact("notes.txt"))
        app.until("the text in the preview", lambda: app.has("#" + I("preview-text")))
        app.after("up to Home", "AZDRIVE_PLACE", r"home /", lambda: app.key("backspace"))
        app.after("Pictures", "AZDRIVE_LISTED", r"home Pictures/ \d+",
                  lambda: open_item(app, "Pictures"))
        app.after("an image preview", "AZDRIVE_PREVIEW", r"image Pictures/gradient\.png",
                  lambda: app.click_exact("gradient.png"))
        app.until("the image in the preview", lambda: app.has("#" + I("preview-image")))
        app.screenshot(os.path.join(out, "12-preview.png"))
        # Large icons (the IconGrid) show the picture as a thumbnail.
        app.after("a thumbnail", "AZDRIVE_THUMBNAIL", r"Pictures/gradient\.png",
                  lambda: app.key("2", primary=True, shift=True))
        app.until("the thumbnail drawn", lambda: thumbnail_drawn(app))
        app.screenshot(os.path.join(out, "12-thumbnails.png"))
        app.after("back to Details", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.key("6", primary=True, shift=True))
        app.after("the navigation pane off", "AZDRIVE_PANES", r"false true false",
                  lambda: app.command("navigation-pane"))
        app.until("no tree", lambda: not app.has("#shell-tree"))
        app.after("the navigation pane on", "AZDRIVE_PANES", r"true true false",
                  lambda: app.command("navigation-pane"))
        app.until("the tree back", lambda: app.has("#shell-tree"))
        app.after("the details pane in the preview pane's place", "AZDRIVE_PANES",
                  r"true false true", lambda: app.command("details-pane"))
        app.until("the details pane laid out", lambda: app.has("#shell-details"))
        app.until("no preview pane", lambda: not app.has("#shell-preview"))
        app.after("the details pane off", "AZDRIVE_PANES", r"true false false",
                  lambda: app.command("details-pane"))
        app.until("no details pane", lambda: not app.has("#shell-details"))
        app.after("Alt+P: the preview pane again", "AZDRIVE_PANES", r"true true false",
                  lambda: app.key("p", alt=True))
        app.until("the preview pane back", lambda: app.has("#shell-preview"))
        log("12. Preview pane (text and image), the thumbnail in Large icons, Navigation pane, "
            "the Details pane in the preview's place, Alt+P")

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
                  lambda: open_item(app, "Music"))
        app.after("an audio preview", "AZDRIVE_PREVIEW", r"audio Music/chime\.wav",
                  lambda: app.click_exact("chime.wav"))
        app.until("the sound's preview", lambda: app.has("#" + I("preview-audio")))
        app.until("its Play button", lambda: app.has("#" + I("preview-play")))
        log("13b. chime.wav previews as a sound with Play")

        # 14. The command bar's gear: the backstage and the Options.
        app.command("options")
        app.until("the Options", lambda: app.has("#" + I("settings")))
        app.screenshot(os.path.join(out, "14-options.png"))
        if NAMING["prefixed"]:
            # The Options are azul-appkit's page: Appearance saves the theme into the data
            # tree (drive/settings.json), so it is there on the next start.
            app.click_exact("General")
            app.until("the General options", lambda: app.shows("Theme"))
            app.after("the theme saved", "AZDRIVE_SETTINGS_SAVED", r"drive/settings\.json",
                      lambda: app.click_exact("Flora"))
            saved = os.path.join(logs, "data", "drive", "settings.json")
            app.until("flora in the settings file",
                      lambda: os.path.isfile(saved) and '"flora"' in open(saved).read())
            app.click_exact("Flat")
            app.until("flat in the settings file", lambda: '"flat"' in open(saved).read())
        app.key("escape")
        app.until("the backstage closed", lambda: not app.has("#" + I("settings")))
        log("14. the gear opened the Options (the theme saved into the data tree); Escape closed "
            "them")

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
                  lambda: select_item(app, "Documents"))
        app.after("Ctrl+A", "AZDRIVE_SELECTED", r"[2-9] .*", lambda: app.key("a", primary=True))
        app.after("Ctrl+C", "AZDRIVE_CLIPBOARD", r"copy [2-9]", lambda: app.key("c", primary=True))
        app.key("escape")
        log("16. Ctrl+A and Ctrl+C reach Explorer's keyboard while the content pane has focus")

        # 17. The looks the design is judged by: This PC, Documents in Details and in Large
        # icons - in flat (Office 2010) and flora (the website), by day and at night.
        for theme in ("flat", "flora"):
            for mode in ("light", "dark"):
                app.must("set_theme", theme=theme)
                app.must("set_mode", mode=mode)
                app.frame(4)
                look = "%s-%s" % (theme, mode)
                app.after("This PC (%s)" % look, "AZDRIVE_PLACE", r"this-pc",
                          lambda: app.click(selector="#" + I("side-this-pc")))
                app.until("the drive tiles (%s)" % look, lambda: app.nodes_with_class(C("drive")))
                app.screenshot(os.path.join(out, "17-%s-this-pc.png" % look))
                app.after("Documents (%s)" % look, "AZDRIVE_LISTED", r"home Documents/ \d+",
                          lambda: app.click(selector="#" + I("side-fav-documents")))
                app.after("Details (%s)" % look, "AZDRIVE_LAYOUT", r"details",
                          lambda: app.key("6", primary=True, shift=True))
                app.until("the Details header (%s)" % look,
                          lambda: app.has("#" + I("details-header")))
                app.screenshot(os.path.join(out, "17-%s-details.png" % look))
                app.after("Large icons (%s)" % look, "AZDRIVE_LAYOUT", r"large_icons",
                          lambda: app.key("2", primary=True, shift=True))
                app.until("the icon grid (%s)" % look, lambda: app.has("#" + I("icon-grid")))
                app.screenshot(os.path.join(out, "17-%s-large-icons.png" % look))
        log("17. This PC, Details and Large icons in flat and flora, light and dark: %s" % out)
        log("PASS: AzDrive browsed, laid out, sorted, selected, renamed, created, copied, "
            "resolved a conflict, deleted and undid, walked the history, toggled the panes, "
            "showed Properties and the Options, took the editing keys, walked its source list")
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
