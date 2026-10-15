#!/usr/bin/env python3
"""AzDrive end to end, headless, on a temporary Home folder with the sample files.

Walks Explorer's main flows through azul's debug server and asserts on the node tree, the
node layout, AzDrive's stdout markers and the files on disk:

     1. This PC: the drive tiles; the chrome - the ribbon (no title row: its tab strip is the
        title bar; at This PC the Computer and View tabs; the tab content closed at rest,
        Computer's drive commands once it shows), the window title (the path), the address row
        (round Back / Forward, Up, the breadcrumb box with its location icon, crumbs and
        chevrons, Refresh); Finder's body - the source list's FAVORITES (Quick access, the
        sample's Documents / Pictures / Music), LOCATIONS (This PC, Home, Azlin), CLOUD ("Add
        drive..."), This PC's row selected; the content filling its pane; no status bar (no
        path bar, no status line, no Status bar switch, no Transfers block): the two pies
        (15 x 25) at the right of the navigation pane's foot, + and the gear on its left;
     1c. the tab area's hover reveal: the pointer leaving closes the content, a hover over a
        tab opens it, a double click locks it open;
     1d. the sync pie's popover (a window of its own): nothing syncs, the drive's space; Escape;
     1b. the source list: a click on Documents goes there (its row selected, the address
        row's crumb), Down walks to Pictures, Enter opens it, Left climbs to FAVORITES' title, Left
        closes the section, Right opens it again; F6 lands on the pane, Down enters the list at
        the selected row, Tab leaves it for the + button;
     2. open the Home drive (double-click its tile): the ribbon shows Home, Share, View;
     3. every layout (Ctrl+Shift+1..8; the icon layouts are azul's IconGrid, the others the
        folder's virtual view), then the ribbon's View > Layout gallery (Tiles, Details);
     4. sort: the Name header twice, then Size;
     5. into Documents; select (click, Ctrl+click, Shift+click), Home > Select all, Escape;
     6. type-ahead ("r" selects report.md);
     7. F2: rename notes.txt to todo.txt in place (the file on disk; End, Backspace over the
        whole name, typing), Home > Undo renames it back;
     8. Ctrl+Shift+N: a new folder (on disk), Escape keeps its name;
     9. Home > Copy / Paste into the new folder, again: the conflict dialog, "Keep both files";
    10. Delete: into the trash folder (on disk), Ctrl+Z brings it back;
    11. Backspace (up), Alt+Left (back), Alt+Right (forward);
    12. the panes: the preview pane from the start (the design's default), View > Preview pane
        off and on (a text and an image preview), a thumbnail in the list and in Large icons,
        Navigation pane off / on, the Details pane in the preview pane's place (they share the
        right side) and off, Alt+P: the preview pane again;
    13. Properties (Alt+Enter) in the in-window sheet, OK;
    14. View > Options: the backstage with the Options (its own title row), Escape;
    15. flora + dark: a screenshot;
    16. Ctrl+A / Ctrl+C with the content pane focused;
    17. the looks: This PC, Documents in Details and in Large icons, in flat and flora, by day
        and at night (17-<theme>-<mode>-<view>.png);
    18. the breadcrumb: in Documents a crumb per step (Home, Documents), a click on the
        Home crumb goes there, its chevron drops Home's folders (Documents goes back), a click on
        the box's empty part turns it into the typed path - which has the keyboard - and Escape
        turns it back;
    19. the File menu: a popup under the File tab (New tab, Open new window, Open terminal
        here, Delete history, Keyboard shortcuts, Options, About, Close; Frequent places),
        Delete history > Recent places runs in the window (the menu closes), a frequent place's
        pin pins it, Escape closes the menu;
    20. F5 counts a folder's items again (Counted: 17 files, 6 written meanwhile, 23); a
        folder of 3,000 files opens at once: the listing streams in, the virtual view holds a
        few screens of rows (never the folder), the rows in view get their sizes (the stat of
        the rows in view), End reveals the last file;
    21. the search view (the design's behaviour B, azul-search): "needle" searches the open
        folder's drive (the scope This drive) with the files' contents (on by default) and finds
        a file three folders down - under its folder's header from the drive's root, the term
        marked in its name, the bar's chips and its "1 item found", the Search tab; a click
        selects it by its key -, the bar's File contents off and on searches again, Escape
        clears the box (the view shows "needle" as a recent search), Escape again closes the
        view (the folder's rows are back), "zebra-quartz" finds the file whose line holds it
        (two lines of it, the terms marked), "othr" finds nothing and suggests "other";
    22. the Search tab's "Index this drive" (azul-search-index, in the run's --cache-dir): a
        word only a Word document holds finds nothing by the walk (a zip is binary to it); with
        the Home drive indexed (AZDRIVE_INDEXED, the search bar's "Indexed:") the same search
        finds the document, its line from its text; turned off, the index's folder is gone;
    23. the Search tab's saved searches: Save search keeps the open search ("zebra-quartz" in
        Find, File contents on); from Home, Saved searches runs it again - Find opens, the
        search finds plan.md again -, and "Forget this saved search" drops it;
    23b. location tabs: Ctrl+T opens one on Documents (nothing selected, a note), the second
        goes to Pictures in Large icons; Ctrl+Tab back to Documents with its selection and
        Details, Ctrl+2 to the second; its search comes back with it; Ctrl+Shift+A the overview
        (its rows, their Ctrl digits, New tab), Escape; Ctrl+W closes the tab showing, the last
        tab stays;
    23c. Quick Look (Space: the text of notes.txt, Down steps on, Escape) and the command
        palette (Cmd+K: its commands at rest, "notes" finds the file, Return shows it in Quick
        Look, Escape);
    24. a cloud drive's index (AzDrive restarted with an S3 drive of the mock stack in its drives
        file, its key in the run's keyring file): a word only a file's text holds finds nothing
        by name; "Index files in the cloud" and "Index this drive" download each file within
        the cap (GetObject at the mock), index its text and keep nothing but the index; the same
        search finds the file from the index; turned off, the index is gone. (An encrypted
        drive's search - names from its drive index - is covered by its unit tests; the drives
        AzDrive makes are encrypted as they are made: scripts/azdrive_add_e2e.py.);
    25. the folder sync, in an AzDrive of its own whose drives file has one S3 drive on the mock
        stack's S3 (scripts/azlin_mock_stack.py; its keys in the headless keyring file, the poll
        every 2 s - $AZDRIVE_SYNC_POLL): Share > Sync with a folder pairs it with AzDrive/<name>
        in Home (the pairing sheet, AZDRIVE_SYNC_PAIRED; the folder opens, "Up to date" in the
        sync pie's popover); a file written on disk goes up with the next poll (the drive's index names
        its BLAKE3); a version another device commits (its blob and the index one generation
        on, written into the mock S3's folder - scripts/azlin_blake3.py) comes down; paused,
        both change it, resumed: the question (D52, "Someone changed this file"), Keep both -
        the drive's version under the name, this computer's as "notes (conflict <device>
        <date>).txt", on the drive too; Free up space: the file leaves this computer, its row
        stays (cloud only) and previews as a sentence, opening it downloads it first; the plain
        drive's own listing shows the sync index's files (not only its hidden `.azlin`); a
        cloud-only row deleted asks "Delete from the drive?" and the next pass deletes it there;
        an Azlin drive synced from the start whose token server says it takes no writes says
        "Read-only (payment due)"; 12 of its files turned random at once pause the uploads
        (what the drive changes still comes down) until "These changes are mine"; again with
        other files, "I was hacked..." restores the drive as of before the change, the
        encrypted copies here wait for a choice (D52) and the drive's version brings each back;
        a metered network (the headless network file
        AZ_NETWORK_STATE_FILE: "cellular metered") says "Paused (metered network)", a 26 MB file
        waits while a small one goes up, "Sync anyway on this network" (Options > Drives > Sync,
        kept in view.json) sends it, a Low Data Mode Wi-Fi pauses too, a free Wi-Fi syncs; a
        laptop at 23 % on a phone's hotspot (the headless battery file AZ_BATTERY_STATE_FILE:
        "battery 23 discharging", the network file: "hotspot") has a client health of 35 - one
        transfer at a time, the recompression and the index upkeep held
        (AZDRIVE_CLIENT_HEALTH 35 transfers=1 recompress=held index=held), a small file still
        goes up, and Options > Drives > Sync says so in one line; charged on a free Wi-Fi: 100.
        The drives of steps 24 and 25 are made at the mock directly, with no keys: plaintext
        drives like the ones made before encryption ("we always encrypt" makes every NEW drive
        encrypted). AzDrive is built with its default features - `encryption` among them - and
        keeps opening such drives as they are; that old path is what these steps check.
        `--sync-only` runs step 25 alone.

The source list shows the sample's Documents, Pictures and Music too (FAVORITES), and the path bar
the open folder's trail: a folder's ITEM is found through its name label (`item_node`), in
whichever DOM holds it - the folder's rows are a virtual view, a DOM of its own.

Usage (from the azul repository, after building libazul with the debug server and AzDrive):

    python3 scripts/azdrive_e2e.py [--bin target/release/AzDrive] [--debug-port 8781]
        [--timeout 240] [--out /tmp/azdrive-shots] [--keep-logs]

Run it through the capped runner on a small machine:

    <scratchpad>/run_capped.sh --cap-mb 1500 --seconds 300 --log /tmp/azdrive-e2e.log -- \\
        python3 scripts/azdrive_e2e.py --bin target/release/AzDrive

Every op that changes state is followed by `wait_frame`s and an `until` on what it must cause;
every screenshot waits for the animations to finish (`settle`).
Every key_down has its key_up (the E2E key_up rule).
"""

import argparse
import calendar
import glob
import json
import zipfile
import os
import re
import shutil
import struct
import sys
import tempfile
import time
import zlib

import azlin_blake3
import azlin_e2e as e2e
import azlin_mock_stack
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


def norm(text):
    """A label as it reads: a large ribbon button sets a two-line label with no-break spaces
    between the words of a line."""
    return " ".join((text or "").replace(" ", " ").split())


# The address bar (layout/src/widgets/address_bar.rs) and the File menu
# (layout/src/widgets/ribbon_file_menu.rs) by their widget classes.
BAR = "__azul-native-address-bar"
BAR_NAV = BAR + "-nav"
BAR_BOX = BAR + "-box"
BAR_ICON = BAR + "-icon"
BAR_FIELD = BAR + "-field"
BAR_CRUMB = BAR + "-crumb"
BAR_CHEVRON = BAR + "-chevron"
BAR_EDIT = BAR + "-edit"
BAR_REFRESH = BAR + "-refresh"
FILE_MENU = "__azul-native-ribbon-file-menu"
FILE_MENU_PIN = FILE_MENU + "-pin"
FILE_MENU_PINNED = FILE_MENU + "-pinned"
TITLEBAR = "__azul-native-titlebar"
# The ribbon's tab buttons and its tab content (layout/src/widgets/ribbon.rs): at rest the
# content is closed (height 0); a hover over a tab opens it, a double click locks it open.
RIBBON_TAB = "__azul-native-ribbon-tab"
RIBBON_CONTENT = "__azul-native-ribbon-content"


class Drive(e2e.App):
    """AzDrive under its debug server: the shared driver (`scripts/azlin_e2e.py`) with AzDrive's
    own reading of the window - two frames after an op, the texts of the text nodes, "has" as
    laid out with a size - its ribbon and the folder's rows in their virtual view's DOM."""

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

    def hierarchy_of(self, dom):
        """The nodes of DOM `dom` of the window (a virtual view's rows are a DOM of their own)."""
        answer = self.op("get_node_hierarchy", dom_id=dom)
        return [d for d in e2e.dicts(answer) if "index" in d and "type" in d]

    def doms(self):
        """The window's DOMs, the virtual views' first (their rows are the folder's items)."""
        return sorted(set(self.dom_ids()), key=lambda d: (d == 0, d))

    def press(self, op, dom, node, what):
        """`op` (click, double_click) on `node` of DOM `dom`, or the nearest ancestor of it that
        has a box (an inline label has none)."""
        parents = {n["index"]: n.get("parent") for n in self.hierarchy_of(dom)}
        at = node
        while isinstance(at, int) and at >= 0:
            answer = self.op(op, node_id=at, dom_id=dom, button="left")
            if isinstance(answer, dict) and answer.get("status") != "error":
                self.frame()
                return
            at = parents.get(at)
        raise Failure("%s on %s: no node from it up has a box" % (op, what))

    # ---- the ribbon ----

    def ribbon_node(self, label, prefix=False):
        """The node of the ribbon's control or tab labelled `label` (`prefix`: a label starting
        with it, as the Undo button names what it undoes), or None. Only a node with a box: the
        closed File menu's DOM is in the ribbon too (its Frequent places hold a "Home"), without
        one - a click there climbed to the File button and opened the menu."""
        nodes, inside = self._within("#" + I("ribbon"))
        for n in nodes:
            text = norm(n.get("text"))
            if not text or not inside(n):
                continue
            if text == label or (prefix and text.startswith(label)):
                node = n.get("parent", n["index"])
                rect = (self.value("get_node_layout", node_id=node) or {}).get("rect") or {}
                if rect.get("width"):
                    return node
        return None

    def ribbon(self, label, prefix=False):
        """Clicks the ribbon's control `label` once it shows (its tab must be the active one)."""
        node = self.until('the ribbon\'s "%s"' % label,
                          lambda: self.ribbon_node(label, prefix))
        self.settle(limit=2.0)
        self.press("click", 0, node, 'the ribbon\'s "%s"' % label)

    def tab(self, name):
        """Clicks the ribbon's tab `name` (Home, Share, View, Computer) and waits for the app to
        take it - and for its content to show: the pointer stays on the tab, which opens it; if
        it does not open by itself, a double click on the tab locks it open."""
        before = self.count("AZDRIVE_RIBBON_TAB", re.escape(name))
        self.ribbon(name)
        # A click on the tab the ribbon shows already (a place whose tabs start with it, the
        # content locked open) switches nothing and the app hears nothing: its content counts.
        end = time.time() + 2.0
        while time.time() < end and self.count("AZDRIVE_RIBBON_TAB", re.escape(name)) <= before:
            self.frame(1)
            time.sleep(0.05)
        self.content_open(name)

    def content_shows(self):
        """Whether the ribbon's tab content is open (laid out with a height)."""
        return self.has("." + RIBBON_CONTENT)

    def content_open(self, name):
        """The tab content of tab `name` shows: open already, open after the hover delay, or
        locked open by a double click on the tab."""
        end = time.time() + 1.0
        while time.time() < end:
            if self.content_shows():
                return
            self.frame(1)
            time.sleep(0.05)
        node = self.until("the %s tab" % name, lambda: self.ribbon_node(name))
        self.press("double_click", 0, node, "the %s tab" % name)
        self.until("the %s tab's content locked open" % name, self.content_shows)

    def screenshot(self, path):
        """Waits for the animations first (the details pane slides its rows in)."""
        self.settle()
        super().screenshot(path)


# Explorer's layout keys: Ctrl+Shift+<digit> (the ribbon's View > Layout gallery shows four of
# the eight at a time; the keys reach every layout).
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


def item_labels(app):
    """(dom, node, name) of every item label the window shows: the folder view's rows (its
    virtual view's DOM) or the IconGrid's labels, in document order."""
    found = []
    for dom in app.doms():
        nodes = app.hierarchy_of(dom)
        by_index = {n["index"]: n for n in nodes}
        for n in nodes:
            classes = n.get("classes") or []
            if C("name") in classes or GRID_LABEL in classes:
                # A search's result marks the terms in its name: the name is its spans' texts
                # (each span holds a text node).
                def texts_under(i):
                    node = by_index.get(i, {})
                    if node.get("text"):
                        return [node["text"]]
                    return [t for c in node.get("children") or [] for t in texts_under(c)]

                name = "".join(texts_under(n["index"]))
                if name:
                    found.append((dom, n["index"], name))
    return found


def item_count(app):
    """How many of the folder's items the view holds (its rows, or the IconGrid's)."""
    count = 0
    for dom in app.doms():
        count += sum(1 for n in app.hierarchy_of(dom)
                     if C("item") in (n.get("classes") or [])
                     or GRID_ITEM in (n.get("classes") or []))
    return count


def item_names(app):
    """The names of the folder's items the view holds, in the order it shows them."""
    return [name for _, _, name in item_labels(app)]


def item_node(app, name):
    """(dom, node) of the label of the folder's item `name` - never a ribbon control, a crumb or
    a source list row showing the same text ("New folder" is a ribbon button too)."""
    for dom, node, label in item_labels(app):
        if label == name:
            return (dom, node)
    return None


def thumbnail_drawn(app):
    """Whether a picture shows as a thumbnail: a hand-built cell's (`__azdrive_thumbnail`), or
    an image in an IconGrid item's icon box."""
    for dom in app.doms():
        nodes = app.hierarchy_of(dom)
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


def open_item(app, name):
    """Double-clicks the folder's item `name`."""
    dom, node = app.until('the item "%s"' % name, lambda: item_node(app, name))
    app.settle(limit=2.0)
    app.press("double_click", dom, node, 'the item "%s"' % name)


def select_item(app, name):
    """Clicks the folder's item `name` (its name label, never the source list's or the path
    bar's row of the same name)."""
    dom, node = app.until('the item "%s"' % name, lambda: item_node(app, name))
    app.settle(limit=2.0)
    app.press("click", dom, node, 'the item "%s"' % name)


def texts_in_view(app):
    """Every text of the folder view's virtual view (its rows: names, sizes, dates)."""
    out = []
    for dom in app.doms():
        if dom == 0:
            continue
        out.extend(n.get("text") for n in app.hierarchy_of(dom) if n.get("text"))
    return out


def node_by_id(app, dom_id):
    """The node of the window whose DOM id is `dom_id`, or None."""
    for n in app.hierarchy():
        if n.get("id") == dom_id:
            return n
    return None


def rect_of(app, selector):
    """The laid-out rect ({x, y, width, height}) of the node `selector` names."""
    rect = (app.value("get_node_layout", selector=selector) or {}).get("rect") or {}
    return {k: float(rect.get(k, 0)) for k in ("x", "y", "width", "height")}


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


def focused_selector(app):
    """The selector of the node with the keyboard (classes and id), or ""."""
    for d in e2e.dicts(app.op("get_focus_state")):
        node = d.get("focused_node")
        if isinstance(node, dict):
            return node.get("selector") or ""
    return ""


def focus_search_box(app):
    """Ctrl+F: the address bar's search box takes the keyboard (a click on the box when the key
    did not - said in the log, it is the step's input that matters here)."""
    app.key("f", primary=True)
    for _ in range(8):
        if "text-input" in focused_selector(app):
            return
        app.frame()
    log("Ctrl+F did not focus the search box; clicking it")
    app.click(selector="." + BAR + "-search")
    app.until("the search box has the keyboard", lambda: "text-input" in focused_selector(app))


def sync_words(app):
    """What the sync popover says: a click on the sync pie opens it (a window of its own over the
    navigation pane's foot), Escape closes it."""
    before = len(app.window_ids())
    app.click(selector="#" + I("sync-pie"))
    pop = e2e.modal_window(app)
    pop.until("the sync popover", lambda: pop.has("#" + I("sync-popover")))
    words = " ".join(t for t in pop.texts() if t)
    pop.key("escape")
    app.until("the sync popover closed", lambda: len(app.window_ids()) <= before)
    return words


def close_search_view(app):
    """Escape in the search box clears its text (the view stays), Escape again closes the view;
    the folder's own rows show again."""
    focus_search_box(app)
    app.after("Escape clears the search", "AZDRIVE_SEARCH_CLEARED", r".*",
              lambda: app.key("escape"))
    app.after("Escape closes the search view", "AZDRIVE_SEARCH_CLOSED", r".*",
              lambda: app.key("escape"))


def bar_texts(app, cls):
    """The texts inside the address bar's nodes of class `cls`, in document order."""
    return app.texts_within("." + cls)


def write_png(path, width, height):
    """A `width` x `height` RGB picture (a colour ramp) as a PNG file. The sample's own picture
    is square, and a square picture cannot show whether a thumbnail keeps its proportions."""
    rows = b"".join(
        b"\x00" + bytes(v for x in range(width)
                        for v in (x * 255 // max(width - 1, 1), y * 255 // max(height - 1, 1), 160))
        for y in range(height))

    def chunk(kind, data):
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(rows))
                + chunk(b"IEND", b""))


# The non-square pictures in Home's Pictures folder: (name, width, height).
WIDE_PICTURE = ("panorama.png", 96, 48)
TALL_PICTURE = ("tower.png", 48, 96)


def thumbnail_sizes(app):
    """(width, height) of every picture the rows draw in an icon's place (`__azdrive_thumbnail`),
    in whichever DOM holds the rows."""
    sizes = []
    for dom in app.doms():
        for n in app.hierarchy_of(dom):
            if C("thumbnail") not in (n.get("classes") or []):
                continue
            answer = app.op("get_node_layout", node_id=n["index"], dom_id=dom)
            if not isinstance(answer, dict) or answer.get("status") == "error":
                continue
            rect = ((answer.get("data") or {}).get("value") or {}).get("rect") or {}
            sizes.append((float(rect.get("width", 0)), float(rect.get("height", 0))))
    return sizes


def check_thumbnails_whole(app, icon_px, layout):
    """The wide and the tall picture of the rows fill their `icon_px` icon square on the longer
    side and keep their proportions: the 2:1 picture is twice as wide as tall, the 1:2 one twice
    as tall as wide
    (Chrome's sizes for the same `<img>` under max-width / max-height). A sliver one or two
    pixels wide is the failure this guards."""
    def whole():
        sizes = thumbnail_sizes(app)
        if len(sizes) < 3:
            return None
        return sizes
    sizes = app.until("the %s pictures laid out" % layout, whole)
    for w, h in sizes:
        # No picture outgrows its square; a smaller one (the 16 px picture in List's 18 px
        # square) keeps its own size, as max-width / max-height never scale up.
        if max(w, h) > icon_px + 1.0 or min(w, h) < 2.0:
            raise Failure("%s: a thumbnail drawn %.1fx%.1f in a %.0f px icon square (all: %s)"
                          % (layout, w, h, icon_px, sizes))
    wide = [(w, h) for w, h in sizes if w >= 1.8 * h]
    tall = [(w, h) for w, h in sizes if h >= 1.8 * w]
    if not wide or not tall:
        raise Failure("%s: the wide and the tall picture do not keep their proportions: %s"
                      % (layout, sizes))
    for w, h in wide + tall:
        if abs(max(w, h) - icon_px) > 1.0 or min(w, h) < icon_px / 2.0 - 1.0:
            raise Failure("%s: the %.1fx%.1f picture does not fill its %.0f px icon square on its "
                          "longer side (all: %s)" % (layout, w, h, icon_px, sizes))
    log("%s: the pictures drawn whole: %s" % (layout, sizes))


def run(args, logs):
    binary = e2e.find_binary("AzDrive", args.bin, "AZDRIVE_BIN")
    log("AzDrive: %s" % binary)
    log("logs and data: %s" % logs)
    out = args.out or os.path.join(logs, "shots")
    os.makedirs(out, exist_ok=True)
    if args.sync_only:
        sync_step(args, logs, binary, out)
        log("PASS: the folder sync (step 25)")
        return True

    home = os.path.join(logs, "home")
    os.makedirs(home)
    # Step 20's big folder: 3,000 files of five bytes each.
    big = os.path.join(home, "Big")
    os.makedirs(big)
    for i in range(3000):
        with open(os.path.join(big, "file-%05d.txt" % i), "wb") as f:
            f.write(b"hello")
    # Step 20's recount: a folder of 17 files (6 more are written while Home shows).
    os.makedirs(os.path.join(home, "Counted"))
    for i in range(17):
        with open(os.path.join(home, "Counted", "c-%02d.txt" % i), "wb") as f:
            f.write(b"x")
    # Step 21's search: a name three folders down, a word on the third line of another file.
    os.makedirs(os.path.join(home, "Find", "deep", "er"))
    with open(os.path.join(home, "Find", "deep", "er", "needle-report.txt"), "wb") as f:
        f.write(b"nothing to see here\n")
    with open(os.path.join(home, "Find", "plan.md"), "wb") as f:
        f.write(b"# Plan\n\nthe zebra-quartz line\n")
    with open(os.path.join(home, "Find", "other.txt"), "wb") as f:
        f.write(b"nothing either\n")
    # Step 12's thumbnails: a wide and a tall picture beside the sample's square one.
    os.makedirs(os.path.join(home, "Pictures"))
    for name, width, height in (WIDE_PICTURE, TALL_PICTURE):
        write_png(os.path.join(home, "Pictures", name), width, height)
    # Step 22's index: a Word document (a zip of XML, deflated: binary to the walk) holding a
    # word no other file holds.
    with zipfile.ZipFile(os.path.join(home, "Find", "Minutes.docx"), "w",
                         zipfile.ZIP_DEFLATED) as docx:
        docx.writestr("[Content_Types].xml", "<Types/>")
        docx.writestr(
            "word/document.xml",
            '<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.'
            'openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Narwhal '
            'tusk minutes</w:t></w:r></w:p></w:body></w:document>')
    cache = os.path.join(logs, "cache")
    # Every setting is a switch (src/args.rs); only the engine's AZ_BACKEND / AZ_DEBUG are
    # variables (the shared driver sets them).
    switches = [
        "--sample", "--screen", "this-pc", "--theme", "flat", "--mode", "light",
        "--language", "en",  # the clicks read English words (the system may be German)
        "--home", home,
        "--downloads", os.path.join(logs, "downloads"),
        "--data-dir", os.path.join(logs, "data"),  # the data tree (azul-appkit's data root)
        "--drives", os.path.join(logs, "config", "drives.json"),
        "--dialogs", "inline",
        "--cache-dir", cache,  # a cloud drive's last listing, the drives' indexes
    ]
    app = Drive("azdrive", binary, switches, args.debug_port, logs, args.timeout)
    docs = os.path.join(home, "Documents")
    stack = None
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
        # Windows 8's chrome: the ribbon, its tab strip the title bar - no title row.
        app.until("the ribbon", lambda: app.has("#" + I("ribbon")))
        for tab in ("Computer", "View"):
            app.until("the ribbon's %s tab" % tab, lambda: app.ribbon_node(tab) is not None)
        if app.ribbon_node("Home") is not None or app.ribbon_node("Share") is not None:
            raise Failure("This PC shows Home / Share: Windows 8 shows Computer and View there")
        # The design's tab area: at rest the tab content is closed - the window is the title
        # bar, the golden line, the address row and the panes.
        app.settle(limit=2.0)
        if app.content_shows():
            raise Failure("the tab content shows at rest (it opens on a hover)")
        app.tab("Computer")
        for tool in ("Add drive", "Add folder as drive", "Remove drive", "Properties",
                     "Refresh"):
            app.until("Computer's %s" % tool, lambda: app.ribbon_node(tool) is not None)
        if app.nodes_with_class(TITLEBAR):
            raise Failure("a title row shows over the ribbon (its tabs are the title bar)")
        app.until("the window title is the path",
                  lambda: app.printed("AZDRIVE_TITLE", re.escape("This PC - AzDrive")))
        # The address row: Back, Forward, Up (the design has no Recent locations button); the
        # breadcrumb box with its icon, the crumb This PC and its chevron; Refresh between the
        # box and the search box.
        app.until("the address row's arrows", lambda: len(app.nodes_with_class(BAR_NAV)) >= 3)
        if len(app.nodes_with_class(BAR_NAV)) != 3:
            raise Failure("the address row has %d arrows, not Back, Forward and Up"
                          % len(app.nodes_with_class(BAR_NAV)))
        app.until("the breadcrumb box", lambda: app.nodes_with_class(BAR_BOX))
        app.until("the location icon", lambda: app.nodes_with_class(BAR_ICON))
        app.until("the This PC crumb", lambda: "This PC" in bar_texts(app, BAR_CRUMB))
        if not app.nodes_with_class(BAR_CHEVRON):
            raise Failure("the crumb has no chevron")
        if not app.nodes_with_class(BAR_REFRESH):
            raise Failure("no Refresh in the address row")
        # Refresh after the trail and before the search box: flat ends the breadcrumb box with it
        # (Explorer), flora stands it apart as a round button (SPEC 2.6).
        def node_rect(node):
            r = (app.value("get_node_layout", node_id=node) or {}).get("rect") or {}
            return {k: float(r.get(k, 0)) for k in ("x", "y", "width", "height")}
        crumb_box = node_rect(app.nodes_with_class(BAR_BOX)[0])
        refresh = node_rect(app.nodes_with_class(BAR_REFRESH)[0])
        search_box = node_rect(app.nodes_with_class(BAR + "-search")[0])
        box_end = crumb_box["x"] + crumb_box["width"]
        if not (refresh["x"] + 0.5 >= box_end - refresh["width"] - 2.0
                and refresh["x"] + refresh["width"] <= search_box["x"] + 0.5):
            raise Failure("Refresh is not at the end of the trail before the search box: box %s, "
                          "refresh %s, search %s" % (crumb_box, refresh, search_box))
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
        # The content fills its pane edge to edge, no page around it.
        app.until("the leaf", lambda: app.has("#" + I("leaf")))
        app.until("This PC's groups", lambda: app.shows("Devices and drives"))
        if not app.has("#shell-tree") or not app.has("#shell-content"):
            raise Failure("the navigation pane and the content pane are not laid out")
        pane, leaf = rect_of(app, "#shell-content"), rect_of(app, "#" + I("leaf"))
        if any(abs(pane[k] - leaf[k]) > 1.0 for k in ("x", "y", "width", "height")):
            raise Failure("the content does not fill its pane: pane %s, content %s" % (pane, leaf))
        # No status bar: no path bar or status line at the content's foot, no switch for one
        # in the View tab, no Transfers block in the navigation pane. The sync and the space
        # are the two pies at the right of the navigation pane's foot, beside + and the gear.
        if app.has("#" + I("path-bar")) or app.has("#" + I("status-line")):
            raise Failure("a status bar shows at the foot of the content (there is none)")
        if app.has("#" + I("side-activity")):
            raise Failure("the navigation pane shows a Transfers block (the pies replace it)")
        app.tab("View")
        if app.ribbon_node("Status bar") is not None:
            raise Failure("the View tab has a Status bar switch (there is no status bar)")
        for pie in ("sync-pie", "space-pie"):
            app.until("the %s" % pie, lambda: app.has("#" + I(pie)))
            box = rect_of(app, "#" + I(pie))
            if abs(box["width"] - 15.0) > 1.0 or abs(box["height"] - 25.0) > 1.0:
                raise Failure("the %s's hit area is %s, not 15 x 25" % (pie, box))
        add, gear, sync = (rect_of(app, "#" + I(n)) for n in ("side-add", "side-actions",
                                                              "sync-pie"))
        if not (add["x"] < gear["x"] < sync["x"]):
            raise Failure("the foot is not + and the gear on the left, the pies on the right")
        app.tab("Computer")
        app.screenshot(os.path.join(out, "01-this-pc.png"))
        log("1. This PC: drive tiles, the ribbon (Computer, View; no title row; the tab content "
            "closed at rest), the window title, the address row (arrows, breadcrumb box, "
            "Refresh), the source list, the content filling its pane, no status bar, the pies "
            "in the navigation pane's foot")

        # 1c. The tab area's hover reveal: the pointer leaving the tab area closes the content
        # after the leave delay; on a tab it opens again after the hover delay; a double click
        # locks it open, and a double click on the locked tab closes it again.
        leaf = rect_of(app, "#" + I("leaf"))
        app.must("mouse_move", x=leaf["x"] + leaf["width"] / 2.0, y=leaf["y"] + leaf["height"] / 2.0)
        time.sleep(0.6)
        app.frame(4)
        app.until("the content closed after the pointer left", lambda: not app.content_shows())
        tab_node = app.until("the Computer tab", lambda: app.ribbon_node("Computer"))
        tab_box = app.value("get_node_layout", node_id=tab_node)["rect"]
        app.must("mouse_move", x=tab_box["x"] + tab_box["width"] / 2.0,
                 y=tab_box["y"] + tab_box["height"] / 2.0)
        time.sleep(0.3)
        app.frame(4)
        app.until("the content open on the hover", app.content_shows)
        app.screenshot(os.path.join(out, "01c-hover-open.png"))
        app.press("double_click", 0, tab_node, "the Computer tab")
        app.must("mouse_move", x=leaf["x"] + leaf["width"] / 2.0, y=leaf["y"] + leaf["height"] / 2.0)
        time.sleep(0.6)
        app.frame(4)
        if not app.content_shows():
            raise Failure("a double click did not lock the tab content open")
        log("1c. the tab content closed when the pointer left, opened on a hover over a tab, "
            "and a double click locked it open")

        # 1d. The pies' popover: a click on the sync pie opens it above the pie (a window of
        # its own) with the sync's state and the drive's space; Escape closes it.
        windows_before = len(app.window_ids())
        app.click(selector="#" + I("sync-pie"))
        pop = e2e.modal_window(app)
        pop.until("the sync popover", lambda: pop.has("#" + I("sync-popover")))
        pop.until("its words: nothing syncs here",
                  lambda: any("Nothing syncs here" in t for t in pop.texts()))
        pop.until("the drive's space", lambda: any(" used of " in t for t in pop.texts()))
        popover_box, pie_box = pop.box("#" + I("sync-popover")), rect_of(app, "#" + I("sync-pie"))
        log("the sync popover at %s, the pie at %s" % (popover_box, pie_box))
        pop.key("escape")
        app.until("the popover closed", lambda: len(app.window_ids()) <= windows_before)
        log("1d. the sync pie opened its popover (nothing syncs, the drive's space); Escape "
            "closed it")

        # 1b. The source list: a click goes, the arrows walk the rows, Enter opens one, Left
        # climbs and closes, Right opens; F6 lands on the pane, Down enters the list at the
        # selected row, Tab leaves it for the + button.
        app.after("Documents from the source list", "AZDRIVE_PLACE", r"home Documents/",
                  lambda: app.click(selector="#" + I("side-fav-documents")))
        app.until("Documents' row selected",
                  lambda: C("side-selected") in classes_of(app, I("side-fav-documents")))
        app.until("the address row's crumb of Documents",
                  lambda: "Documents" in bar_texts(app, BAR_CRUMB))
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
        for tab in ("Home", "Share", "View"):
            app.until("the ribbon's %s tab" % tab, lambda: app.ribbon_node(tab) is not None)
        app.until("the window title is Home's path",
                  lambda: app.printed("AZDRIVE_TITLE", re.escape("Home - AzDrive")))
        app.until("Documents in the view", lambda: "Documents" in item_names(app))
        if ".hidden-settings" in item_names(app):
            raise Failure("a hidden item shows while Hidden items is off")

        # 3. Every layout (Ctrl+Shift+1..8), then the ribbon's Layout gallery.
        for digit, name in LAYOUTS:
            app.after("the layout %s" % name, "AZDRIVE_LAYOUT", re.escape(name),
                      lambda: app.key(digit, primary=True, shift=True))
            app.until("the %s view" % name, lambda: C("layout-" + name) in app.classes())
            app.until("the items of %s" % name, lambda: item_count(app) >= 5)
            if name in ("large_icons", "tiles"):
                app.screenshot(os.path.join(out, "03-%s.png" % name))
        app.tab("View")
        # The gallery shows the row of four holding the current layout (Details: List, Details,
        # Tiles, Content).
        app.after("Tiles from the Layout gallery", "AZDRIVE_LAYOUT", r"tiles",
                  lambda: app.ribbon("Tiles"))
        app.after("Details from the Layout gallery", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.ribbon("Details"))
        app.after("Large icons (Ctrl+Shift+2)", "AZDRIVE_LAYOUT", r"large_icons",
                  lambda: app.key("2", primary=True, shift=True))
        app.until("the icon grid (azul's IconGrid)", lambda: app.has("#" + I("icon-grid")))
        app.until("the grid's items", lambda: len(app.nodes_with_class(GRID_ITEM)) >= 5)
        app.after("back to Details", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.key("6", primary=True, shift=True))
        app.until("the folder's rows (a virtual view)", lambda: app.has("#" + I("folder-rows")))
        log("3. all eight layouts render the folder (the icon layouts on azul's IconGrid, the "
            "others a virtual view); View > Layout switches them")

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
        # The group is non-capturing: `printed` is re.findall, which answers tuples for a
        # pattern with groups of its own, so "desc" could never have matched.
        app.after("sort by name again", "AZDRIVE_SORT", r"Name (?:asc|desc)",
                  lambda: app.click_exact("Name"))
        if app.printed("AZDRIVE_SORT", r"Name (?:asc|desc)")[-1] == "Name desc":
            app.after("sort by name, ascending", "AZDRIVE_SORT", r"Name asc",
                      lambda: app.click_exact("Name"))
        log("4. the Name and Size headers sort (a second click reverses)")

        # 5. Documents; selection.
        app.after("the Documents listing", "AZDRIVE_LISTED", r"home Documents/ \d+",
                  lambda: open_item(app, "Documents"))
        app.until("notes.txt", lambda: "notes.txt" in item_names(app))
        app.after("one selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: select_item(app, "notes.txt"))
        app.after("Ctrl+click", "AZDRIVE_SELECTED", r"2 .*", lambda: (
            app.op("key_down", key=PRIMARY, modifiers={PRIMARY: True}),
            select_item(app, "report.md"),
            app.op("key_up", key=PRIMARY, modifiers={PRIMARY: False})))
        app.after("Shift+click", "AZDRIVE_SELECTED", r"3 .*", lambda: (
            app.op("key_down", key="shift", modifiers={"shift": True}),
            select_item(app, "data.csv"),
            app.op("key_up", key="shift", modifiers={"shift": False})))
        app.after("Escape", "AZDRIVE_SELECTED", r"0 -", lambda: app.key("escape"))
        app.tab("Home")
        app.after("Home > Select all", "AZDRIVE_SELECTED", r"3 .*",
                  lambda: app.ribbon("Select all"))
        app.after("Escape", "AZDRIVE_SELECTED", r"0 -", lambda: app.key("escape"))
        log("5. click, Ctrl+click, Shift+click, Home > Select all and Escape select as Explorer "
            "does")

        # 6. Type-ahead.
        app.after("type-ahead r", "AZDRIVE_SELECTED", r"1 Documents/report\.md",
                  lambda: app.key("r"))
        log("6. typing r selects report.md")

        # 7. F2: rename in place; Home > Undo.
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: select_item(app, "notes.txt"))
        app.after("the rename field", "AZDRIVE_RENAMING", r"Documents/notes\.txt",
                  lambda: app.key("f2"))
        app.until("the rename field laid out",
                  lambda: app.has("#" + I("rename-field"), every_dom=True))
        # The field is in its row, in the virtual view's DOM (a tuple: DOM 0 is falsy).
        def rename_dom():
            for d in app.doms():
                if app._has_in("#" + I("rename-field"), d):
                    return (d,)
            return None
        (field_dom,) = app.until("the rename field's DOM", rename_dom)
        app.must("focus_node", selector="#" + I("rename-field"), dom_id=field_dom)
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
        app.ribbon("Undo rename", prefix=True)
        app.until("notes.txt back on disk (Undo)",
                  lambda: os.path.isfile(os.path.join(docs, "notes.txt")))
        log("7. F2 renamed notes.txt to todo.txt on disk; Home > Undo renamed it back")

        # 8. A new folder.
        app.after("a new folder", "AZDRIVE_DONE", r"created Documents/New folder/",
                  lambda: app.key("n", primary=True, shift=True))
        app.until("the new folder on disk", lambda: os.path.isdir(os.path.join(docs, "New folder")))
        app.until("its rename field", lambda: app.has("#" + I("rename-field"), every_dom=True))
        app.key("escape")
        app.until("the field gone", lambda: not app.has("#" + I("rename-field"), every_dom=True))
        log("8. Ctrl+Shift+N made New folder on disk; Escape kept its name")

        # 9. Copy / paste; a conflict; Keep both.
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: select_item(app, "notes.txt"))
        app.after("Home > Copy", "AZDRIVE_CLIPBOARD", r"copy 1", lambda: app.ribbon("Copy"))
        # The item, not the ribbon's "New folder" button.
        app.after("into New folder", "AZDRIVE_LISTED", r"home Documents/New folder/ \d+",
                  lambda: open_item(app, "New folder"))
        target = os.path.join(docs, "New folder")
        app.after("Home > Paste", "AZDRIVE_TRANSFER", r"\d+ done 1",
                  lambda: app.ribbon("Paste"))
        app.until("the copy on disk", lambda: os.path.isfile(os.path.join(target, "notes.txt")))
        app.after("the conflict", "AZDRIVE_TRANSFER", r"\d+ conflict 1",
                  lambda: app.ribbon("Paste"))
        app.until("the conflict dialog", lambda: app.has("#" + I("conflict")))
        app.screenshot(os.path.join(out, "09-conflict.png"))
        app.after("keep both", "AZDRIVE_TRANSFER", r"\d+ done 1",
                  lambda: (app.must("click", selector="#" + I("conflict-keep-both")), app.frame()))
        app.until("notes (2).txt on disk", lambda: os.path.isfile(os.path.join(target, "notes (2).txt")))
        log("9. Home > Copy / Paste copied into New folder; a second paste asked, Keep both made "
            "notes (2).txt")

        # 10. Delete into the trash; Ctrl+Z.
        app.after("notes (2).txt selected", "AZDRIVE_SELECTED", r"1 .*notes \(2\)\.txt",
                  lambda: select_item(app, "notes (2).txt"))
        app.after("the delete", "AZDRIVE_DELETED", r"1", lambda: app.key("delete"))
        app.until("notes (2).txt gone", lambda: not os.path.exists(os.path.join(target, "notes (2).txt")))
        trashed = glob.glob(os.path.join(home, ".azdrive-trash", "*", "Documents", "New folder", "notes (2).txt"))
        if not trashed:
            raise Failure("the deleted file is not in the trash folder")
        app.key("z", primary=True)
        app.until("notes (2).txt back (Ctrl+Z)",
                  lambda: os.path.isfile(os.path.join(target, "notes (2).txt")))
        log("10. Delete moved the file into .azdrive-trash; Ctrl+Z brought it back")

        # 11. Up, Back, Forward.
        app.after("Backspace (up)", "AZDRIVE_PLACE", r"home Documents/", lambda: app.key("backspace"))
        app.after("Alt+Left (back)", "AZDRIVE_PLACE", r"home Documents/New folder/",
                  lambda: app.key("left", alt=True))
        app.after("Alt+Right (forward)", "AZDRIVE_PLACE", r"home Documents/",
                  lambda: app.key("right", alt=True))
        log("11. Backspace went up, Alt+Left back, Alt+Right forward")

        # 12. The panes (View tab): the preview pane and the details pane share the window's right
        # side (Explorer's rule), so the preview pane's switch takes the details pane away.
        # The preview pane shows from the start (the design's default), the details pane not.
        app.until("the preview pane laid out", lambda: app.has("#shell-preview"))
        app.until("no details pane beside it", lambda: not app.has("#shell-details"))
        app.tab("View")
        app.after("the preview pane off", "AZDRIVE_PANES", r"true false false",
                  lambda: app.ribbon("Preview pane"))
        app.until("no preview pane", lambda: not app.has("#shell-preview"))
        app.after("the preview pane on again", "AZDRIVE_PANES", r"true true false",
                  lambda: app.ribbon("Preview pane"))
        app.until("the preview pane laid out again", lambda: app.has("#shell-preview"))
        app.after("a text preview", "AZDRIVE_PREVIEW", r"text Documents/notes\.txt",
                  lambda: select_item(app, "notes.txt"))
        app.until("the text in the preview", lambda: app.has("#" + I("preview-text")))
        app.after("up to Home", "AZDRIVE_PLACE", r"home /", lambda: app.key("backspace"))
        app.after("Pictures", "AZDRIVE_LISTED", r"home Pictures/ \d+",
                  lambda: open_item(app, "Pictures"))
        # The list shows its pictures as thumbnails in their icons' place: the pictures in view
        # get one as soon as the folder lists them.
        app.until("a thumbnail in the list",
                  lambda: app.count("AZDRIVE_THUMBNAIL", r"Pictures/gradient\.png") > 0)
        app.until("the thumbnail drawn in the list", lambda: thumbnail_drawn(app))
        for name, _, _ in (WIDE_PICTURE, TALL_PICTURE):
            app.until("a thumbnail of %s" % name,
                      lambda: app.count("AZDRIVE_THUMBNAIL", r"Pictures/" + re.escape(name)) > 0)
        check_thumbnails_whole(app, 16.0, "Details")
        app.after("List", "AZDRIVE_LAYOUT", r"list", lambda: app.key("5", primary=True, shift=True))
        check_thumbnails_whole(app, 18.0, "List")
        app.screenshot(os.path.join(out, "12-list-thumbnails.png"))
        app.after("Details again", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.key("6", primary=True, shift=True))
        app.after("an image preview", "AZDRIVE_PREVIEW", r"image Pictures/gradient\.png",
                  lambda: select_item(app, "gradient.png"))
        app.until("the image in the preview", lambda: app.has("#" + I("preview-image")))
        app.screenshot(os.path.join(out, "12-preview.png"))
        # Large icons (the IconGrid) show the same thumbnail at once: it is made once.
        app.after("Large icons", "AZDRIVE_LAYOUT", r"large_icons",
                  lambda: app.key("2", primary=True, shift=True))
        app.until("the thumbnail drawn", lambda: thumbnail_drawn(app))
        app.screenshot(os.path.join(out, "12-thumbnails.png"))
        app.after("back to Details", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.key("6", primary=True, shift=True))
        app.after("the navigation pane off", "AZDRIVE_PANES", r"false true false",
                  lambda: app.ribbon("Navigation pane"))
        app.until("no tree", lambda: not app.has("#shell-tree"))
        app.after("the navigation pane on", "AZDRIVE_PANES", r"true true false",
                  lambda: app.ribbon("Navigation pane"))
        app.until("the tree back", lambda: app.has("#shell-tree"))
        app.after("the details pane in the preview pane's place", "AZDRIVE_PANES",
                  r"true false true", lambda: app.ribbon("Details pane"))
        app.until("the details pane laid out", lambda: app.has("#shell-details"))
        app.until("no preview pane", lambda: not app.has("#shell-preview"))
        app.after("the details pane off", "AZDRIVE_PANES", r"true false false",
                  lambda: app.ribbon("Details pane"))
        app.until("no details pane", lambda: not app.has("#shell-details"))
        app.after("Alt+P: the preview pane again", "AZDRIVE_PANES", r"true true false",
                  lambda: app.key("p", alt=True))
        app.until("the preview pane back", lambda: app.has("#shell-preview"))
        log("12. View: Preview pane (text and image), the thumbnail in Large icons, Navigation "
            "pane, the Details pane in the preview's place, Alt+P")

        # 13. Properties.
        app.after("gradient.png selected", "AZDRIVE_SELECTED", r"1 Pictures/gradient\.png",
                  lambda: select_item(app, "gradient.png"))
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
                  lambda: select_item(app, "chime.wav"))
        app.until("the sound's preview", lambda: app.has("#" + I("preview-audio")))
        app.until("its Play button", lambda: app.has("#" + I("preview-play")))
        log("13b. chime.wav previews as a sound with Play")

        # 14. View > Options: the backstage and the Options, with a title row of their own (the
        # ribbon, whose tabs are the title bar, is not there).
        app.tab("View")
        app.ribbon("Options")
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
        app.until("the ribbon back", lambda: app.has("#" + I("ribbon")))
        log("14. View > Options opened the Options (the theme saved into the data tree); Escape "
            "closed them")

        # 15. Flora, dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(4)
        app.screenshot(os.path.join(out, "15-flora-dark.png"))
        log("15. flora + dark")

        # 16. The editing keys with the content pane focused (a click focuses it): Ctrl+A,
        # Ctrl+C reach Explorer's keyboard.
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
        app.must("set_theme", theme="flat")
        app.must("set_mode", mode="light")
        app.after("Details", "AZDRIVE_LAYOUT", r"details",
                  lambda: app.key("6", primary=True, shift=True))
        log("17. This PC, Details and Large icons in flat and flora, light and dark: %s" % out)

        # 18. The breadcrumb (Documents is open): a crumb per step, the Home crumb goes there,
        # its chevron drops Home's folders, the box's empty part turns into the typed path.
        app.until("the crumbs Home, Documents",
                  lambda: [t for t in bar_texts(app, BAR_CRUMB)
                           if t in ("This PC", "Home", "Documents")] == ["Home", "Documents"])
        crumbs = app.nodes_with_class(BAR_CRUMB)
        chevrons = app.nodes_with_class(BAR_CHEVRON)
        if len(chevrons) < len(crumbs):
            raise Failure("%d crumbs but %d chevrons: every crumb has its chevron"
                          % (len(crumbs), len(chevrons)))
        app.after("the Home crumb", "AZDRIVE_PLACE", r"home /",
                  lambda: app.press("click", 0, app.nodes_with_class(BAR_CRUMB)[0], "Home's crumb"))
        windows_before = len(app.window_ids())
        app.press("click", 0, app.nodes_with_class(BAR_CHEVRON)[0], "Home's chevron")
        menu = app.until("the chevron's menu of Home's folders",
                         lambda: app.popup() if len(app.window_ids()) > windows_before else None)
        app.until("Documents in the menu",
                  lambda: any(n.get("text") == "Documents" for n in app.hierarchy(menu)))
        app.after("Documents from the chevron's menu", "AZDRIVE_PLACE", r"home Documents/",
                  lambda: app.click_exact("Documents", window=menu))
        app.until("the menu closed", lambda: len(app.window_ids()) <= windows_before)
        app.press("click", 0, app.nodes_with_class(BAR_FIELD)[0], "the box's empty part")
        app.until("the typed path", lambda: app.nodes_with_class(BAR_EDIT))
        app.until("the typed path has the keyboard",
                  lambda: "text-input" in focused_selector(app))
        app.key("escape")
        app.until("the crumbs back", lambda: not app.nodes_with_class(BAR_EDIT)
                  and app.nodes_with_class(BAR_CRUMB))
        log("18. the breadcrumb: a crumb per step, the Home crumb went there, its chevron "
            "dropped Home's folders, the empty part opened the typed path with the keyboard, "
            "Escape closed it")

        # 19. The File menu: a popup under the File tab.
        windows_before = len(app.window_ids())
        app.ribbon("File")
        menu = app.until("the File menu", lambda: app.popup()
                         if len(app.window_ids()) > windows_before else None)
        for text in ("New tab", "Open new window", "Open terminal here", "Delete history",
                     "Keyboard shortcuts", "Options", "About AzDrive", "Close",
                     "Frequent places"):
            app.until('"%s" in the File menu' % text,
                      lambda: any(norm(n.get("text")) == text for n in app.hierarchy(menu)))
        if not any(FILE_MENU in (n.get("classes") or []) for n in app.hierarchy(menu)):
            raise Failure("the File menu is not the RibbonFileMenu widget")
        app.click_exact("Delete history", window=menu)
        app.until("Delete history's choices",
                  lambda: any(norm(n.get("text")) == "Recent places" for n in app.hierarchy(menu)))
        app.after("Delete history > Recent places", "AZDRIVE_DONE", r"history true false",
                  lambda: app.click_exact("Recent places", window=menu))
        app.until("the File menu closed", lambda: len(app.window_ids()) <= windows_before)
        app.until("the pick ran in the window",
                  lambda: app.printed("AZDRIVE_FILE_MENU", r"ClearHistory.*"))
        # A frequent place's pin: Documents (the place visited last) pins to Quick access.
        app.ribbon("File")
        menu = app.until("the File menu again", lambda: app.popup()
                         if len(app.window_ids()) > windows_before else None)
        pins = app.until("a frequent place's pin",
                         lambda: [n["index"] for n in app.hierarchy(menu)
                                  if FILE_MENU_PIN in (n.get("classes") or [])])
        app.after("the pin", "AZDRIVE_DONE", r"pinned [1-9]",
                  lambda: (app.must("click", node_id=pins[0], window_id=menu), app.frame()))
        app.key("escape")
        app.until("the File menu closed by Escape", lambda: len(app.window_ids()) <= windows_before)
        log("19. the File menu: its commands (New tab first, Keyboard shortcuts, Options, About) "
            "and Frequent places, Delete history > Recent places ran in the window, a pin pinned "
            "the place, Escape closed it")

        # 20. A folder of 3,000 files opens at once: the listing streams in, the virtual view
        # holds a few screens of rows, the rows in view get their sizes, End reveals the last.
        app.after("up to Home", "AZDRIVE_PLACE", r"home /", lambda: app.key("backspace"))
        # F5 reads the folder again, the item counts of its folders with it (a count is asked
        # for once per listing, not once per window): Counted's 17 files and 6 more read 23.
        app.until("Counted's count", lambda: "17 items" in texts_in_view(app))
        for i in range(17, 23):
            with open(os.path.join(home, "Counted", "c-%02d.txt" % i), "wb") as f:
                f.write(b"x")
        app.after("F5", "AZDRIVE_LISTED", r"home / \d+", lambda: app.key("f5"))
        app.until("Counted counted again (F5)", lambda: "23 items" in texts_in_view(app))
        app.after("Big (3,000 files)", "AZDRIVE_LISTED", r"home Big/ 3000",
                  lambda: open_item(app, "Big"))
        app.until("the first rows", lambda: "file-00000.txt" in item_names(app))
        built = item_count(app)
        if built >= 600:
            raise Failure("the virtual view built %d rows of 3,000 (a few screens are enough)"
                          % built)
        app.until("the sizes of the rows in view (their stat)",
                  lambda: "5 B" in texts_in_view(app))
        app.after("the first file", "AZDRIVE_SELECTED", r"1 Big/file-00000\.txt",
                  lambda: select_item(app, "file-00000.txt"))
        app.after("End: the last file", "AZDRIVE_SELECTED", r"1 Big/file-02999\.txt",
                  lambda: app.key("end"))
        app.until("the last file revealed", lambda: "file-02999.txt" in item_names(app))
        log("20. F5 counted Counted again (23 items); a folder of 3,000 files: %d rows built, "
            "all 3,000 listed, the rows in view got their sizes, End revealed the last" % built)

        # 21. The search view (the design's behaviour B): the search box searches the open
        # folder's drive from its root (the scope "This drive"), names and - File contents is on
        # by default - the files' text; the results are grouped by their folder under a header
        # from the drive's root, each with its name and two lines of its text, the terms marked;
        # the bar counts them. Escape clears the box (the view stays on the recent searches),
        # Escape again closes the view.
        app.after("up to Home", "AZDRIVE_PLACE", r"home /", lambda: app.key("backspace"))
        app.after("Find", "AZDRIVE_LISTED", r"home Find/ \d+", lambda: open_item(app, "Find"))
        app.until("Find's rows", lambda: "plan.md" in item_names(app))

        def status():
            return " ".join(app.texts_within("#" + I("find-status")))

        focus_search_box(app)
        app.after("the search", "AZDRIVE_SEARCHED", r"1 contents needle",
                  lambda: (app.must("text_input", text="needle"), app.frame(2)))
        app.until("the search's bar", lambda: app.has("#" + I("find-bar")))
        for chip in ("This folder", "This drive", "All drives", "Kind", "Date modified", "Size",
                     "File contents", "Save search"):
            app.until("the bar's %s" % chip,
                      lambda: chip in [norm(t) for t in app.texts_within("#" + I("find-bar"))])
        app.until("the file three folders down",
                  lambda: "needle-report.txt" in item_names(app))
        if "plan.md" in item_names(app):
            raise Failure("the search's results still show the folder's own rows")
        app.until("its folder's header from the drive's root",
                  lambda: "Home \u203a Find \u203a deep \u203a er" in texts_in_view(app))
        app.until("the term marked in its name", lambda: "needle" in texts_in_view(app)
                  and "-report.txt" in texts_in_view(app))
        app.until("the bar's count", lambda: "1 item found" in status())
        app.until("the Search tab", lambda: app.ribbon_node("Search") is not None)
        app.screenshot(os.path.join(out, "21-search-names.png"))
        app.after("a result selected by its key", "AZDRIVE_SELECTED",
                  r"1 Find/deep/er/needle-report\.txt",
                  lambda: select_item(app, "needle-report.txt"))
        app.after("File contents off: names only", "AZDRIVE_SEARCHED", r"1 names needle",
                  lambda: app.click_within("#" + I("find-bar"), "File contents"))
        app.after("File contents on again", "AZDRIVE_SEARCHED", r"1 contents needle",
                  lambda: app.click_within("#" + I("find-bar"), "File contents"))
        focus_search_box(app)
        app.after("Escape in the box clears it", "AZDRIVE_SEARCH_CLEARED", r".*",
                  lambda: app.key("escape"))
        app.until("the recent searches", lambda: app.has("#" + I("find-start")))
        app.until("\"needle\" among them",
                  lambda: "needle" in [norm(t) for t in app.texts_within("#" + I("find-start"))])
        app.after("Escape again closes the search view", "AZDRIVE_SEARCH_CLOSED", r".*",
                  lambda: app.key("escape"))
        app.until("the folder's rows back", lambda: "plan.md" in item_names(app))
        focus_search_box(app)
        app.after("the contents search", "AZDRIVE_SEARCHED", r"1 contents zebra-quartz",
                  lambda: (app.must("text_input", text="zebra-quartz"), app.frame(2)))
        app.until("the file whose line holds it", lambda: "plan.md" in item_names(app))
        app.until("its line, the terms marked",
                  lambda: "zebra" in texts_in_view(app) and "quartz" in texts_in_view(app)
                  and any("line" in t for t in texts_in_view(app)))
        if "other.txt" in item_names(app):
            raise Failure("a file without the text is a result")
        app.screenshot(os.path.join(out, "21-search-contents.png"))
        close_search_view(app)
        app.until("Find's rows again", lambda: "other.txt" in item_names(app))
        # Nothing found: a spelling the window knows within two edits, the other scopes.
        focus_search_box(app)
        app.after("a misspelt name", "AZDRIVE_SEARCHED", r"0 contents othr",
                  lambda: (app.must("text_input", text="othr"), app.frame(2)))
        app.until("the suggestions", lambda: app.has("#" + I("find-suggestions")))
        app.until("did you mean \"other\"",
                  lambda: any("other" in t for t in app.texts_within("#" + I("find-suggestions"))))
        app.after("the suggestion searched", "AZDRIVE_SEARCHED", r"\d+ contents other",
                  lambda: app.click_within("#" + I("find-suggestions"),
                                           "Did you mean \u201cother\u201d?"))
        app.until("other.txt found", lambda: "other.txt" in item_names(app))
        close_search_view(app)
        log("21. the search view: \"needle\" found Find/deep/er/needle-report.txt under its "
            "folder's header, the term marked, \"1 item found\", the bar's chips; File "
            "contents off and on; Escape cleared the box (\"needle\" a recent search), Escape "
            "closed the view; \"zebra-quartz\" found plan.md with its line marked; \"othr\" "
            "suggested \"other\"")

        # 22. "Index this drive": the Home drive's full-text index reads what the walk cannot.
        focus_search_box(app)
        app.after("a word only a Word document holds, without an index", "AZDRIVE_SEARCHED",
                  r"0 contents narwhal",
                  lambda: (app.must("text_input", text="narwhal"), app.frame(2)))
        app.until("nothing found by the walk", lambda: "No items match" in status())
        app.tab("Search")
        app.after("Index this drive", "AZDRIVE_INDEXED", r"home \d+ \d+ \d+",
                  lambda: app.ribbon("Index this drive"))
        app.until("the search's bar names the index", lambda: "Indexed:" in status())
        focus_search_box(app)
        app.after("Escape clears the box", "AZDRIVE_SEARCH_CLEARED", r".*",
                  lambda: app.key("escape"))
        app.after("the same word, the index asked first", "AZDRIVE_SEARCHED",
                  r"1 contents narwhal",
                  lambda: (app.must("text_input", text="narwhal"), app.frame(2)))
        app.until("the Word document", lambda: "Minutes.docx" in item_names(app))
        app.until("its line from its text", lambda: "Narwhal" in texts_in_view(app))
        app.screenshot(os.path.join(out, "22-search-index.png"))
        app.tab("Search")
        app.after("Index this drive off", "AZDRIVE_INDEX_REMOVED", r"home",
                  lambda: app.ribbon("Index this drive"))
        index_dir = os.path.join(cache, "index")
        if os.path.isdir(index_dir) and os.listdir(index_dir):
            raise Failure("the index's folder stays after Index this drive was turned off: %s"
                          % os.listdir(index_dir))
        close_search_view(app)
        log("22. Index this drive: \"narwhal\" (in Find/Minutes.docx only) found nothing by the "
            "walk; the Home drive indexed (\"Indexed:\" in the search's bar), the same search "
            "found the Word document with its line; turned off, the index's folder went")

        # 23. Saved searches: Save search keeps the open search, Saved searches runs it again.
        def ribbon_menu(button, item, key, pattern, what):
            """Opens the ribbon's menu `button` and clicks its entry `item`, waiting for `key`."""
            windows_before = len(app.window_ids())
            app.ribbon(button)
            menu = app.until("the %s menu" % button,
                             lambda: app.popup() if len(app.window_ids()) > windows_before
                             else None)
            app.until('"%s" in the menu' % item,
                      lambda: any(norm(n.get("text")) == item for n in app.hierarchy(menu)))
            app.after(what, key, pattern, lambda: app.click_exact(item, window=menu))
            app.until("the menu closed", lambda: len(app.window_ids()) <= windows_before)

        focus_search_box(app)
        app.after("the search to keep", "AZDRIVE_SEARCHED", r"1 contents zebra-quartz",
                  lambda: (app.must("text_input", text="zebra-quartz"), app.frame(2)))
        app.tab("Search")
        app.after("Save search", "AZDRIVE_SEARCH_SAVED", r"zebra-quartz",
                  lambda: app.ribbon("Save search"))
        close_search_view(app)
        # The keys go back to the listing (Escape leaves the search box focused, where
        # Backspace edits the text): a row selected, then Backspace goes up.
        app.after("up to Home", "AZDRIVE_PLACE", r"home /",
                  lambda: (select_item(app, "plan.md"), app.key("backspace")))
        focus_search_box(app)
        app.after("another search in Home", "AZDRIVE_SEARCHED", r"\d+ contents needle",
                  lambda: (app.must("text_input", text="needle"), app.frame(2)))
        app.tab("Search")
        app.after("the saved search's results", "AZDRIVE_SEARCHED", r"1 contents zebra-quartz",
                  lambda: ribbon_menu("Saved searches", "zebra-quartz",
                                      "AZDRIVE_SAVED_SEARCH_RUN", r"zebra-quartz",
                                      "the saved search run again"))
        app.until("back in Find: its result", lambda: "plan.md" in item_names(app))
        app.until("its folder: Find", lambda: any("Find" in t for t in texts_in_view(app)))
        app.screenshot(os.path.join(out, "23-saved-search.png"))
        app.tab("Search")
        ribbon_menu("Saved searches", "Forget this saved search",
                    "AZDRIVE_SAVED_SEARCH_FORGOTTEN", r"zebra-quartz", "Forget")
        close_search_view(app)
        log("23. saved searches: \"zebra-quartz\" (Find, File contents) saved; from Home, Saved "
            "searches ran it again (Find, plan.md); Forget dropped it")

        # 23b. Location tabs: Ctrl+T opens one on the folder in view (Finder's habit), after the
        # tab showing; each keeps its own place, selection, layout and search; Ctrl+Tab and
        # Ctrl+2 switch, Ctrl+Shift+A shows the overview (Escape closes it), Ctrl+W closes the
        # tab showing, and the last tab stays. The tab keys are the Control key on every
        # platform.
        def overview_texts():
            texts = app.texts(every_dom=True)
            popup = app.popup()
            if popup:
                texts += list(e2e.strings(app.op("get_node_hierarchy", window_id=popup)))
            return [norm(t) for t in texts if t]

        app.after("Documents", "AZDRIVE_PLACE", r"home Documents/",
                  lambda: app.click(selector="#" + I("side-fav-documents")))
        app.until("Documents' rows", lambda: "notes.txt" in item_names(app))
        app.after("notes.txt selected in the first tab", "AZDRIVE_SELECTED",
                  r"1 Documents/notes\.txt", lambda: select_item(app, "notes.txt"))
        app.after("Ctrl+T: a second tab, showing", "AZDRIVE_TABS", r"2 1",
                  lambda: app.key("t", ctrl=True))
        app.until("it opened on Documents", lambda: app.last("AZDRIVE_PLACE") == "home Documents/")
        app.until("the note says where", lambda: app.shows("New tab: Documents"))
        app.until("nothing selected in it", lambda: app.last("AZDRIVE_SELECTED") == "0 -")
        app.after("Pictures in the second tab", "AZDRIVE_PLACE", r"home Pictures/",
                  lambda: app.click(selector="#" + I("side-fav-pictures")))
        app.after("Large icons in the second tab", "AZDRIVE_LAYOUT", r"large_icons",
                  lambda: app.key("2", primary=True, shift=True))
        app.after("Ctrl+Tab: the first tab again", "AZDRIVE_TABS", r"2 0",
                  lambda: app.key("tab", ctrl=True))
        app.until("its place", lambda: app.last("AZDRIVE_PLACE") == "home Documents/")
        app.until("its layout", lambda: app.last("AZDRIVE_LAYOUT") == "details")
        app.until("its selection", lambda: app.last("AZDRIVE_SELECTED") == "1 Documents/notes.txt")
        app.after("Ctrl+2: the second tab", "AZDRIVE_TABS", r"2 1",
                  lambda: app.key("2", ctrl=True))
        app.until("its place", lambda: app.last("AZDRIVE_PLACE") == "home Pictures/")
        app.until("its layout", lambda: app.last("AZDRIVE_LAYOUT") == "large_icons")
        # A search in the second tab comes back with it.
        focus_search_box(app)
        app.after("a search in the second tab", "AZDRIVE_SEARCHED", r"\d+ \w+ gradient",
                  lambda: (app.must("text_input", text="gradient"), app.frame(2)))
        app.after("Ctrl+Tab: the first tab, no search", "AZDRIVE_TABS", r"2 0",
                  lambda: app.key("tab", ctrl=True))
        app.until("the first tab's own rows", lambda: "notes.txt" in item_names(app))
        app.after("Ctrl+Shift+Tab: the second tab, its search run again", "AZDRIVE_SEARCHING",
                  r"gradient", lambda: app.key("tab", ctrl=True, shift=True))
        app.until("its result", lambda: "gradient.png" in item_names(app))
        app.after("Ctrl+Shift+A: the tab overview", "AZDRIVE_TAB_OVERVIEW", r"open",
                  lambda: app.key("a", ctrl=True, shift=True))
        app.until("the overview's rows", lambda: "Documents" in overview_texts()
                  and "Search: gradient" in overview_texts() and "New tab" in overview_texts())
        app.until("the Ctrl digits", lambda: "\u23032" in overview_texts())
        app.screenshot(os.path.join(out, "23b-tab-overview.png"))
        app.after("Escape closes the overview", "AZDRIVE_TAB_OVERVIEW", r"closed",
                  lambda: app.key("escape"))
        app.after("Ctrl+W closes the second tab", "AZDRIVE_TABS", r"1 0",
                  lambda: app.key("w", ctrl=True))
        app.until("the first tab shows", lambda: app.last("AZDRIVE_PLACE") == "home Documents/")
        app.key("w", ctrl=True)
        app.until("the last tab stays", lambda: app.shows("This is the only tab."))
        if app.last("AZDRIVE_TABS") != "1 0":
            raise Failure("Ctrl+W closed the last tab: %s" % app.last("AZDRIVE_TABS"))
        log("23b. location tabs: Ctrl+T on Documents (empty selection, a note), Pictures and "
            "Large icons in the second, Ctrl+Tab back to Documents with notes.txt selected in "
            "Details, Ctrl+2, the second tab's search run again, Ctrl+Shift+A the overview, "
            "Ctrl+W closed the second, the last tab stayed")

        # 23c. Quick Look (Space) and the command palette (Cmd+K), over the window on a scrim.
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 Documents/notes\.txt",
                  lambda: select_item(app, "notes.txt"))
        app.after("Space: Quick Look", "AZDRIVE_QUICK_LOOK", r"open Documents/notes\.txt",
                  lambda: app.key("space"))
        app.until("Quick Look laid out", lambda: app.has("#" + I("quick-look")))
        app.until("the text in it", lambda: app.has("#" + I("preview-text")))
        app.screenshot(os.path.join(out, "23c-quick-look.png"))
        before = app.last("AZDRIVE_SELECTED")
        app.key("down")
        app.until("Down: the next item in Quick Look",
                  lambda: app.last("AZDRIVE_SELECTED") != before)
        app.after("Escape closes Quick Look", "AZDRIVE_QUICK_LOOK", r"closed",
                  lambda: app.key("escape"))
        app.until("Quick Look gone", lambda: not app.has("#" + I("quick-look")))
        app.after("Cmd+K: the palette", "AZDRIVE_PALETTE", r"open",
                  lambda: app.key("k", primary=True))
        app.until("the palette laid out", lambda: app.has("#" + I("palette")))
        app.until("its commands at rest",
                  lambda: "Commands" in [norm(t) for t in app.texts_within("#" + I("palette"))])
        app.must("text_input", text="notes")
        app.frame(3)
        app.until("the file it finds",
                  lambda: "Files" in [norm(t) for t in app.texts_within("#" + I("palette"))])
        app.after("Return: the file in Quick Look", "AZDRIVE_QUICK_LOOK",
                  r"open Documents/notes\.txt", lambda: app.key("enter"))
        app.until("the palette closed", lambda: app.printed("AZDRIVE_PALETTE", r"closed"))
        app.after("Escape closes Quick Look", "AZDRIVE_QUICK_LOOK", r"closed",
                  lambda: app.key("escape"))
        app.after("Cmd+K again", "AZDRIVE_PALETTE", r"open", lambda: app.key("k", primary=True))
        app.after("Escape closes the palette", "AZDRIVE_PALETTE", r"closed",
                  lambda: app.key("escape"))
        log("23c. Space opened Quick Look on notes.txt (its text), Down stepped on, Escape "
            "closed it; Cmd+K opened the palette (its commands), \"notes\" found the file, "
            "Return showed it in Quick Look; Escape closed both")

        # 24. A cloud drive's index: the mock stack's S3 bucket as a drive of AzDrive (restarted
        # with it in the drives file and its key in the run's keyring file).
        stack = azlin_mock_stack.start(os.path.join(logs, "s3"))
        bucket = os.path.join(logs, "s3", "e2e-cloud", "Notes")
        os.makedirs(bucket)
        with open(os.path.join(bucket, "minutes.txt"), "wb") as f:
            f.write(b"the tusk ledger of the harbour\n")
        with open(os.path.join(bucket, "other.txt"), "wb") as f:
            f.write(b"nothing to see\n")
        drives_file = os.path.join(logs, "config", "drives.json")
        os.makedirs(os.path.dirname(drives_file), exist_ok=True)
        try:
            with open(drives_file, "r", encoding="utf-8") as f:
                drives = json.load(f)
        except (OSError, ValueError):
            drives = {"format": "azul-storage.drives", "version": 1, "drives": []}
        drives["drives"] = [d for d in drives.get("drives", []) if d.get("id") != "e2e-cloud"]
        drives["drives"].append({
            "id": "e2e-cloud",
            "name": "E2E Cloud",
            "location": {"kind": "s3", "endpoint": stack.s3_url,
                         "region": azlin_mock_stack.REGION, "bucket": "e2e-cloud",
                         "path_style": True},
        })
        with open(drives_file, "w", encoding="utf-8") as f:
            json.dump(drives, f)
        keyring_file = os.path.join(logs, "keyring.json")
        with open(keyring_file, "w", encoding="utf-8") as f:
            json.dump({"azul-storage/s3/e2e-cloud": json.dumps({
                "access_key_id": azlin_mock_stack.ACCESS_KEY,
                "secret_access_key": azlin_mock_stack.SECRET_KEY,
                "session_token": None})}, f)
        app.stop()
        app = Drive("azdrive-cloud", binary, switches + ["--open", "E2E Cloud/Notes"],
                    args.debug_port, logs, args.timeout,
                    extra_env={"AZ_KEYRING_FILE": keyring_file})
        app.until("the debug server", lambda: app.op("get_dom_tree"))
        app.must("resize", width=1280.0, height=800.0)
        app.until("the cloud drive's folder", lambda: app.printed(
            "AZDRIVE_LISTED", r"e2e-cloud Notes/ 2"))
        app.until("its files", lambda: "minutes.txt" in item_names(app))
        focus_search_box(app)
        app.after("a word only a file's text holds, without an index", "AZDRIVE_SEARCHED",
                  r"0 names ledger",
                  lambda: (app.must("text_input", text="ledger"), app.frame(2)))
        app.tab("Search")
        app.after("Index files in the cloud", "AZDRIVE_INDEX_CLOUD_FILES", r"true",
                  lambda: app.ribbon("Index files in the cloud"))
        stack.s3.clear_log()
        app.after("Index this drive", "AZDRIVE_INDEXED", r"e2e-cloud 2 2 0",
                  lambda: app.ribbon("Index this drive"))
        gets = sorted(stack.s3.object_gets() or [])
        if gets != ["Notes/minutes.txt", "Notes/other.txt"]:
            raise Failure("the index downloaded %r, not each file once" % gets)
        kept = [name for _, _, names in os.walk(os.path.join(logs, "cache")) for name in names
                if name in ("minutes.txt", "other.txt")]
        if kept:
            raise Failure("a downloaded file stayed on disk: %r" % kept)
        focus_search_box(app)
        app.after("Escape clears the box", "AZDRIVE_SEARCH_CLEARED", r".*",
                  lambda: app.key("escape"))
        app.after("the same word, from the drive's index", "AZDRIVE_SEARCHED",
                  r"1 contents ledger",
                  lambda: (app.must("text_input", text="ledger"), app.frame(2)))
        app.until("the file whose text holds it", lambda: "minutes.txt" in item_names(app))
        if "other.txt" in item_names(app):
            raise Failure("a file without the word is a result")
        app.screenshot(os.path.join(out, "24-cloud-index.png"))
        app.tab("Search")
        app.after("Index this drive off", "AZDRIVE_INDEX_REMOVED", r"e2e-cloud",
                  lambda: app.ribbon("Index this drive"))
        app.after("Index files in the cloud off", "AZDRIVE_INDEX_CLOUD_FILES", r"false",
                  lambda: app.ribbon("Index files in the cloud"))
        close_search_view(app)
        log("24. a cloud drive (the mock S3): \"ledger\" found nothing by name; Index files in "
            "the cloud + Index this drive downloaded each file once (nothing kept), and the same "
            "search found Notes/minutes.txt from the index; both turned off again")

        # 25. The folder sync, in an AzDrive of its own (one at a time on the debug port).
        app.stop()
        sync_step(args, logs, binary, out)

        log("PASS: AzDrive browsed, laid out, sorted, selected, renamed, created, copied, "
            "resolved a conflict, deleted and undid, walked the history, toggled the panes, "
            "showed Properties and the Options, took the editing keys, walked its source list, "
            "its breadcrumb and its File menu, opened 3,000 files at once, searched a folder "
            "and every folder below it by name and by contents, indexed a drive, kept a saved "
            "search, indexed a cloud drive's files, and synced a cloud drive with a folder")
        return True
    except Failure:
        for name, path in (("stdout", app.out_path), ("stderr", app.err_path)):
            print("\n----- azdrive %s (tail) -----\n%s" % (name, e2e.tail(path)))
        raise
    finally:
        app.stop()
        if stack is not None:
            stack.stop()


# ==== 25. The folder sync ====

SYNC_DRIVE = "e2e-sync"
SYNC_BUCKET = "e2e-sync"
SYNC_NAME = "Sync drive"
# This computer's name in conflict copies (azcloud-kit's device_name: $AZCLOUD_DEVICE).
SYNC_DEVICE = "e2e-laptop"
# The Azlin drive unpaid past its grace (the mock token server's development sign-up).
PAID_NAME = "Unpaid drive"


def sync_drives_file(path, s3_url, extra=()):
    """A drives file with one S3 drive on the mock stack's S3 (its keys in the keyring file),
    and the drives-file entries `extra`."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    entry = {
        "id": SYNC_DRIVE,
        "name": SYNC_NAME,
        "location": {
            "kind": "s3",
            "endpoint": s3_url,
            "region": azlin_mock_stack.REGION,
            "bucket": SYNC_BUCKET,
            "path_style": True,
            "auth": {"type": "keyring"},
        },
    }
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"format": "azul-storage.drives", "version": 1,
                   "drives": [entry] + list(extra)}, f)


def sync_keyring_file(path, extra=None):
    """The headless keyring (AZ_KEYRING_FILE) holding the drive's keys (and `extra` entries)."""
    secret = json.dumps({"access_key_id": azlin_mock_stack.ACCESS_KEY,
                         "secret_access_key": azlin_mock_stack.SECRET_KEY})
    entries = {"azul-storage/s3/" + SYNC_DRIVE: secret}
    entries.update(extra or {})
    with open(path, "w", encoding="utf-8") as f:
        json.dump(entries, f)


def azlin_session(bundle):
    """The keyring text of an Azlin drive's session (azcloud-kit's AzlinSession) from a sign-up's
    bundle."""
    creds = bundle["credentials"]
    expires = calendar.timegm(time.strptime(creds["expires_at"], "%Y-%m-%dT%H:%M:%SZ"))
    return json.dumps({
        "drive_id": bundle["drive"]["id"],
        "drive_token": bundle["drive_token"],
        "access_key_id": creds["access_key_id"],
        "secret_access_key": creds["secret_access_key"],
        "session_token": creds["session_token"],
        "expires_at": expires,
    })


def sync_meta(s3_root, bucket=SYNC_BUCKET):
    """The bucket's folder of the sync's bookkeeping (the whole drive syncs: prefix "")."""
    return os.path.join(s3_root, bucket, ".azlin")


def sync_index(s3_root, bucket=SYNC_BUCKET):
    with open(os.path.join(sync_meta(s3_root, bucket), "index.json"), "r",
              encoding="utf-8") as f:
        return json.load(f)


def other_device_writes(s3_root, key, data, device="e2e-desktop", bucket=SYNC_BUCKET):
    """Another device's commit of `key` = `data`: its blob (named by its BLAKE3) and the index
    one generation on, written into the mock S3's folder as that device's sync would."""
    meta = sync_meta(s3_root, bucket)
    digest = azlin_blake3.hex_digest(data)
    blob = os.path.join(meta, "blobs", digest[:2], digest)
    os.makedirs(os.path.dirname(blob), exist_ok=True)
    with open(blob, "wb") as f:
        f.write(data)
    index = sync_index(s3_root, bucket)
    now = int(time.time())
    index["generation"] += 1
    index["files"][key] = {"hash": digest, "size": len(data), "mtime": now,
                           "gen": index["generation"], "device": device}
    index["updated_at"] = now
    index["updated_by"] = device
    path = os.path.join(meta, "index.json")
    tmp = path + ".e2e-tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(json.dumps(index, indent=2) + "\n")
    os.replace(tmp, path)


def write_network(path, words):
    """The headless network azul reads (AZ_NETWORK_STATE_FILE, at every query): words such as
    "cellular metered", "wifi constrained" or "wifi"."""
    tmp = path + ".e2e-tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(words + "\n")
    os.replace(tmp, path)


def write_battery(path, words):
    """The headless battery azul reads (AZ_BATTERY_STATE_FILE, at every query; the headless
    power follows it): words such as "battery 23 discharging", "charging 90" or
    "battery 8 discharging low-power serious"."""
    tmp = path + ".e2e-tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(words + "\n")
    os.replace(tmp, path)


def last_status(app, drive_id):
    """What drive `drive_id`'s status line said last (AZDRIVE_SYNC_STATUS), or None."""
    said = app.printed("AZDRIVE_SYNC_STATUS", re.escape(drive_id) + r" .*")
    return said[-1].split(" ", 1)[1] if said else None


def settled(status):
    """Whether a status line (its key form: azdrive-sync-status-...) says the drive synced: no
    pause for the network, no pass running."""
    return bool(status) and "metered" not in status and "syncing" not in status


def synced_setting(view, drive_id, name):
    """A synced drive's setting `name` as the view settings file keeps it (None: not there)."""
    try:
        with open(view, "r", encoding="utf-8") as f:
            settings = json.load(f)
    except (OSError, ValueError):
        return None
    for setup in settings.get("synced", []):
        if setup.get("drive_id") == drive_id:
            return setup.get(name)
    return None


def press_or_explain(app, out, selector, key, pattern, what, shot, question=None,
                     under=None, missed=None):
    """Clicks `selector` once it stands still and waits a few seconds for a new `<key>
    <pattern>` line. When none comes, the window is examined: a screenshot before
    (`<shot>-before.png`) and after (`<shot>-after.png`) the click, whether `question` (the
    dialog) and the node still show, the node's rect, what each DOM has at its centre, the
    scrollbars of `under` (the node the dialog lies over) - and whether a click at the node's
    left edge, or Enter on it, does what the click did not: that tells a click that missed the
    node from a node whose callback did nothing. When one of them did it, the notes go to
    `missed` and the step goes on - it still fails at its end with them, after the steps
    behind this one ran; otherwise it fails here. A click that missed before (the same `shot`)
    is only noted again, so a dozen misses do not use up the step's time."""
    tag = "[%s] " % shot
    again = missed is not None and any(m.startswith(tag) for m in missed)
    if not again:
        app.screenshot(os.path.join(out, shot + "-before.png"))
    before = app.count(key, pattern)
    came = lambda: app.count(key, pattern) > before
    app.click(selector=selector)
    end = time.time() + 4.0
    while time.time() < end:
        if came():
            return
        time.sleep(0.25)
    notes = [tag + "%s: no %s line after a click at the centre of %s" % (what, key, selector)]
    if not again:
        app.screenshot(os.path.join(out, shot + "-after.png"))
    if question:
        notes.append("%s shows: %s" % (question, app.has(question)))
    notes.append("%s shows: %s" % (selector, app.has(selector)))
    layout = app.op("get_node_layout", selector=selector)
    value = (layout.get("data") or {}).get("value") if isinstance(layout, dict) else None
    rect = (value or {}).get("rect") or {}
    notes.append("its rect: %s" % json.dumps(rect))
    if under and not again:
        bars = app.op("get_scrollbar_info", selector=under, orientation="both")
        notes.append("the scrollbars of %s: %s" % (
            under, json.dumps((bars or {}).get("data"))[:400]))
    if rect.get("width"):
        cx = rect["x"] + rect["width"] / 2.0
        cy = rect["y"] + rect["height"] / 2.0
        for dom in ([] if again else app.dom_ids()):
            hit = app.op("hit_test", x=cx, y=cy, dom_id=dom)
            notes.append("hit_test dom %s at (%.0f, %.0f): %s" % (
                dom, cx, cy, json.dumps((hit or {}).get("data"))[:160]))
        app.must("click", x=rect["x"] + 6.0, y=cy)
        app.frame(2)
        time.sleep(1.5)
        notes.append("a click at its left edge: %s" % ("did it" if came() else "nothing"))
    if not came():
        app.must("focus_node", selector=selector)
        app.frame(2)
        app.key("enter")
        time.sleep(1.5)
        notes.append("Enter on it: %s" % ("did it" if came() else "nothing"))
    if came() and missed is not None:
        log("NOTE: " + "; ".join(notes))
        missed.append("; ".join(notes))
        return
    raise Failure("; ".join(notes))


def read_file(path):
    try:
        with open(path, "rb") as f:
            return f.read()
    except OSError:
        return None


def sync_step(args, logs, binary, out):
    """25. A cloud drive syncs with a folder: paired (the folder under Home), a file written on
    disk goes up (the poll timer), a version another device committed comes down, a conflict
    asks (D52) and "Keep both" keeps both, "Free up space" leaves the file in the cloud only - a
    row of its folder still - and opening it brings it back."""
    base = os.path.join(logs, "sync")
    home = os.path.join(base, "home")
    os.makedirs(home)
    s3_root = os.path.join(base, "s3")
    stack = azlin_mock_stack.start(s3_root)
    os.makedirs(os.path.join(s3_root, SYNC_BUCKET), exist_ok=True)
    # An Azlin drive (a development sign-up at the mock token server) unpaid past its grace:
    # synced from the start (the view settings name it), its token server says it takes no
    # writes.
    paid = stack.token.state.signup({"name": PAID_NAME})
    paid_id = paid["drive"]["id"]
    stack.token.state.set_read_only(paid_id, True)
    drives = os.path.join(base, "config", "drives.json")
    sync_drives_file(drives, stack.s3_url, [paid["drive"]])
    keyring = os.path.join(base, "keyring.json")
    sync_keyring_file(keyring, {"azul-storage/s3/" + paid_id: azlin_session(paid)})
    view = os.path.join(base, "data", "drive", "view.json")
    os.makedirs(os.path.dirname(view), exist_ok=True)
    with open(view, "w", encoding="utf-8") as f:
        json.dump({"synced": [{"drive_id": paid_id, "prefix": "",
                               "folder": os.path.join(home, "AzDrive", PAID_NAME)}]}, f)
    switches = [
        "--screen", "this-pc", "--theme", "flat", "--mode", "light",
        "--language", "en",  # the clicks read English words (the system may be German)
        "--home", home,
        "--downloads", os.path.join(base, "downloads"),
        "--data-dir", os.path.join(base, "data"),
        "--drives", drives,
        "--dialogs", "inline",
        "--cache-dir", os.path.join(base, "cache"),
    ]
    # The headless network and battery azul reads at every query (no files yet: wired and free,
    # no battery - a desktop on mains).
    network_file = os.path.join(base, "network.txt")
    battery_file = os.path.join(base, "battery.txt")
    env = {"AZ_KEYRING_FILE": keyring, "AZDRIVE_SYNC_POLL": "2", "AZCLOUD_DEVICE": SYNC_DEVICE,
           "AZ_NETWORK_STATE_FILE": network_file, "AZ_BATTERY_STATE_FILE": battery_file}
    app = Drive("azdrive-sync", binary, switches, args.debug_port, logs, args.timeout,
                extra_env=env)
    folder = os.path.join(home, "AzDrive", SYNC_NAME)
    notes = os.path.join(folder, "notes.txt")
    status_key = "AZDRIVE_SYNC_STATUS"

    def status():
        # No status bar: the sync's words are its pie's popover's.
        return sync_words(app)

    try:
        app.until("This PC", lambda: app.printed("AZDRIVE_PLACE", r"this-pc"))
        app.until("the debug server", lambda: app.op("get_dom_tree"))
        app.must("resize", width=1280.0, height=800.0)
        app.frame(3)
        row = "#__azdrive_side_drive_" + SYNC_DRIVE
        app.until("the drive in CLOUD", lambda: app.has(row))
        app.after("the drive opens", "AZDRIVE_LISTED", re.escape(SYNC_DRIVE) + r" / \d+",
                  lambda: app.click(selector=row))
        # Share > Sync with a folder: the pairing sheet, its folder AzDrive/<name> in Home.
        app.tab("Share")
        app.ribbon("Sync with a folder")
        app.until("the pairing sheet", lambda: app.has("#" + I("sync-pair")))
        app.screenshot(os.path.join(out, "25-sync-pair.png"))
        app.after("Sync", "AZDRIVE_SYNC_PAIRED", re.escape(SYNC_DRIVE) + r" .*",
                  lambda: (app.must("click", selector="#" + I("sync-pair-ok")), app.frame()))
        paired = app.last("AZDRIVE_SYNC_PAIRED").split(" ", 1)[1]
        if os.path.realpath(paired) != os.path.realpath(folder):
            raise Failure("paired with %s, not %s" % (paired, folder))
        app.until("the synced folder opens",
                  lambda: app.printed("AZDRIVE_PLACE", r"home AzDrive/%s/" % re.escape(SYNC_NAME)))
        app.until("the first pass", lambda: app.printed("AZDRIVE_SYNC_DONE",
                                                         re.escape(SYNC_DRIVE) + r" .*"))
        app.until("Up to date", lambda: app.printed(status_key, re.escape(SYNC_DRIVE) +
                                                    r" azdrive-sync-status-up-to-date"))
        app.until("the sync popover says it", lambda: "Up to date" in status())
        app.until("the unpaid drive is read-only by its token server's word", lambda: app.printed(
            status_key, re.escape(paid_id) + r" azdrive-sync-status-payment-due"))
        log("25a. paired: %s, the first pass, \"Up to date\" on the status line; the unpaid "
            "Azlin drive: \"Read-only (payment due)\"" % folder)

        # A file written on disk goes up with the next poll.
        with open(notes, "wb") as f:
            f.write(b"first version\n")
        app.until("notes.txt uploaded", lambda: app.printed(
            "AZDRIVE_SYNC_FILE", re.escape(SYNC_DRIVE) + r" on-device notes\.txt"))
        index = app.until("the drive's index names it",
                          lambda: "notes.txt" in sync_index(s3_root).get("files", {})
                          and sync_index(s3_root))
        if index["files"]["notes.txt"]["hash"] != azlin_blake3.hex_digest(b"first version\n"):
            raise Failure("the index names another content: %s" % index["files"]["notes.txt"])
        app.until("notes.txt listed", lambda: "notes.txt" in item_names(app))
        app.screenshot(os.path.join(out, "25-sync-uploaded.png"))
        # The drive's own listing shows the sync index's files, not only its hidden `.azlin`.
        app.after("the drive's own listing", "AZDRIVE_LISTED", re.escape(SYNC_DRIVE) + r" / \d+",
                  lambda: app.click(selector=row))
        app.until("notes.txt in the drive's own listing", lambda: "notes.txt" in item_names(app))
        app.screenshot(os.path.join(out, "25-sync-drive-listing.png"))
        app.after("back to the synced folder", "AZDRIVE_PLACE",
                  r"home AzDrive/%s/" % re.escape(SYNC_NAME), lambda: app.key("left", alt=True))
        log("25b. a file written on disk went up with the poll (the index names its BLAKE3); the "
            "drive's own listing shows it")

        # A version another device committed comes down.
        before = app.count("AZDRIVE_SYNC_DONE")
        other_device_writes(s3_root, "notes.txt", b"second version, from the desktop\n")
        app.until("the other device's version here",
                  lambda: read_file(notes) == b"second version, from the desktop\n")
        app.until("a pass that brought it", lambda: any(
            "down=1" in line for line in app.printed("AZDRIVE_SYNC_DONE")[before:]))
        log("25c. a version committed by another device came down")

        # Both change it: paused, both edits, resumed - the question (D52), Keep both.
        app.tab("Share")
        app.after("Pause syncing", "AZDRIVE_SYNC_PAUSED", re.escape(SYNC_DRIVE),
                  lambda: app.ribbon("Pause syncing"))
        app.until("Paused", lambda: app.printed(status_key, re.escape(SYNC_DRIVE) +
                                                r" azdrive-sync-status-paused"))
        other_device_writes(s3_root, "notes.txt", b"third version, from the desktop\n")
        with open(notes, "wb") as f:
            f.write(b"third version, from the laptop!\n")
        app.after("Resume syncing", "AZDRIVE_SYNC_CONFLICT", re.escape(SYNC_DRIVE) + r" notes\.txt",
                  lambda: app.ribbon("Resume syncing"))
        app.until("the question", lambda: app.has("#" + I("sync-conflict")))
        app.until("it says who changed it", lambda: app.shows("Someone changed this file"))
        if read_file(notes) != b"third version, from the laptop!\n":
            raise Failure("mine changed before the question was answered")
        app.screenshot(os.path.join(out, "25-sync-conflict.png"))
        app.after("Keep both", "AZDRIVE_SYNC_RESOLVED",
                  re.escape(SYNC_DRIVE) + r" both notes\.txt",
                  lambda: (app.must("click", selector="#" + I("sync-keep-both")), app.frame()))
        copies = os.path.join(folder, "notes (conflict %s *).txt" % SYNC_DEVICE)
        app.until("both versions here", lambda: glob.glob(copies)
                  and read_file(notes) == b"third version, from the desktop\n")
        copy = glob.glob(copies)[0]
        if read_file(copy) != b"third version, from the laptop!\n":
            raise Failure("the conflict copy holds %r" % read_file(copy))
        app.until("the copy on the drive too", lambda: any(
            k.startswith("notes (conflict %s " % SYNC_DEVICE)
            for k in sync_index(s3_root).get("files", {})))
        log("25d. a conflict asked (D52); Keep both: the drive's version under the name, this "
            "computer's as %s, on the drive too" % os.path.basename(copy))

        # Free up space: the file leaves this computer, its row stays (cloud only); opening it
        # brings it back.
        app.until("notes.txt listed", lambda: "notes.txt" in item_names(app))
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 .*notes\.txt",
                  lambda: select_item(app, "notes.txt"))
        app.tab("Share")
        app.after("Free up space", "AZDRIVE_SYNC_FREED", re.escape(SYNC_DRIVE),
                  lambda: app.ribbon("Free up space"))
        app.until("cloud only", lambda: app.printed(
            "AZDRIVE_SYNC_FILE", re.escape(SYNC_DRIVE) + r" cloud-only notes\.txt"))
        if read_file(notes) is not None:
            raise Failure("the freed file is still on this computer")
        app.until("its row stays", lambda: "notes.txt" in item_names(app))
        app.screenshot(os.path.join(out, "25-sync-cloud-only.png"))
        # Its preview is a sentence: its bytes are not here. (The preview pane is on by
        # default; Alt+P shows it where it was switched off.)
        panes = app.last("AZDRIVE_PANES")
        if panes is not None and panes.split()[1:2] != ["true"]:
            app.after("the preview pane", "AZDRIVE_PANES", r"\w+ true \w+",
                      lambda: app.key("p", alt=True))
        app.until("a cloud-only preview", lambda: app.printed(
            "AZDRIVE_PREVIEW", r"synced .*notes\.txt"))
        app.after("opening it downloads it first", "AZDRIVE_SYNC_OPENED",
                  re.escape(SYNC_DRIVE) + r" notes\.txt", lambda: open_item(app, "notes.txt"))
        app.until("back on this computer",
                  lambda: read_file(notes) == b"third version, from the desktop\n")
        app.until("on this device", lambda: app.printed(
            "AZDRIVE_SYNC_FILE", re.escape(SYNC_DRIVE) + r" on-device notes\.txt"))
        log("25e. Free up space: notes.txt cloud only (its row stays, its preview a sentence); "
            "opened: downloaded first")

        # A cloud-only row deleted: asked, then the next pass deletes it on the drive.
        draft = os.path.join(folder, "draft.txt")
        with open(draft, "wb") as f:
            f.write(b"a draft\n")
        app.until("draft.txt uploaded", lambda: app.printed(
            "AZDRIVE_SYNC_FILE", re.escape(SYNC_DRIVE) + r" on-device draft\.txt"))
        app.until("draft.txt listed", lambda: "draft.txt" in item_names(app))
        app.after("draft.txt selected", "AZDRIVE_SELECTED", r"1 .*draft\.txt",
                  lambda: select_item(app, "draft.txt"))
        app.tab("Share")
        app.after("Free up space", "AZDRIVE_SYNC_FREED", re.escape(SYNC_DRIVE),
                  lambda: app.ribbon("Free up space"))
        app.until("draft.txt cloud only", lambda: read_file(draft) is None)
        app.until("draft.txt listed", lambda: "draft.txt" in item_names(app))
        app.after("draft.txt selected", "AZDRIVE_SELECTED", r"1 .*draft\.txt",
                  lambda: select_item(app, "draft.txt"))
        app.key("delete")
        app.until("Delete from the drive?", lambda: app.has("#" + I("sync-delete")))
        app.after("Delete", "AZDRIVE_SYNC_DELETED", re.escape(SYNC_DRIVE),
                  lambda: (app.must("click", selector="#" + I("sync-delete-ok")), app.frame()))
        app.until("gone from the drive's index",
                  lambda: "draft.txt" not in sync_index(s3_root).get("files", {}))
        app.until("its row gone", lambda: "draft.txt" not in item_names(app))
        log("25f. a cloud-only row deleted: asked, then gone from the drive")

        # 25g. The plain drive's own listing renames and downloads through the sync.
        app.after("the drive's own listing", "AZDRIVE_LISTED", re.escape(SYNC_DRIVE) + r" / \d+",
                  lambda: app.click(selector=row))
        app.until("notes.txt in the drive's listing", lambda: "notes.txt" in item_names(app))
        app.after("notes.txt selected", "AZDRIVE_SELECTED", r"1 notes\.txt",
                  lambda: select_item(app, "notes.txt"))
        app.after("the rename field", "AZDRIVE_RENAMING", r"notes\.txt", lambda: app.key("f2"))

        def rename_dom():
            for d in app.doms():
                if app._has_in("#" + I("rename-field"), d):
                    return (d,)
            return None
        (field_dom,) = app.until("the rename field's DOM", rename_dom)
        app.must("focus_node", selector="#" + I("rename-field"), dom_id=field_dom)
        app.frame()
        app.key("end")
        for _ in range(len("notes.txt")):
            app.key("backspace")
        app.must("text_input", text="renamed.txt")
        app.frame()
        app.after("renamed through the sync", "AZDRIVE_SYNC_RENAMED",
                  re.escape(SYNC_DRIVE) + r" renamed\.txt", lambda: app.key("enter"))
        renamed = os.path.join(folder, "renamed.txt")
        app.until("the synced copy renamed", lambda: os.path.isfile(renamed)
                  and not os.path.exists(notes))
        app.until("renamed on the drive by the next pass", lambda: "renamed.txt" in sync_index(
            s3_root).get("files", {}) and "notes.txt" not in sync_index(s3_root)["files"])
        # A download of a cloud-only file of the listing: it comes down first.
        app.until("renamed.txt listed", lambda: "renamed.txt" in item_names(app))
        app.after("renamed.txt selected", "AZDRIVE_SELECTED", r"1 renamed\.txt",
                  lambda: select_item(app, "renamed.txt"))
        app.tab("Share")
        app.after("Free up space", "AZDRIVE_SYNC_FREED", re.escape(SYNC_DRIVE),
                  lambda: app.ribbon("Free up space"))
        app.until("renamed.txt cloud only", lambda: not os.path.exists(renamed))
        app.after("renamed.txt selected", "AZDRIVE_SELECTED", r"1 renamed\.txt",
                  lambda: select_item(app, "renamed.txt"))
        app.after("the download waits for it", "AZDRIVE_SYNC_FETCHED", re.escape(SYNC_DRIVE),
                  lambda: app.ribbon("Download"))
        downloaded = os.path.join(base, "downloads", "renamed.txt")
        app.until("downloaded", lambda: read_file(downloaded) == read_file(renamed)
                  and read_file(downloaded) is not None)
        log("25g. the plain drive's own listing renamed notes.txt to renamed.txt through the "
            "synced folder, and downloaded it (cloud only: it came down first)")

        # 25h. A folder emptied here: the mass delete asks, in AzDrive's words; Keep them.
        app.after("back to the synced folder", "AZDRIVE_PLACE",
                  r"home AzDrive/%s/" % re.escape(SYNC_NAME), lambda: app.key("left", alt=True))
        bulk = os.path.join(folder, "bulk")
        os.makedirs(bulk, exist_ok=True)
        for i in range(12):
            with open(os.path.join(bulk, "%d.txt" % i), "wb") as f:
                f.write(b"bulk file %d\n" % i)
        app.until("bulk uploaded", lambda: all(
            "bulk/%d.txt" % i in sync_index(s3_root).get("files", {}) for i in range(12)))
        app.until("bulk on this device", lambda: app.printed(
            "AZDRIVE_SYNC_FILE", re.escape(SYNC_DRIVE) + r" on-device bulk/11\.txt"))
        for i in range(12):
            os.remove(os.path.join(bulk, "%d.txt" % i))
        app.until("the mass delete asked", lambda: app.printed(
            "AZDRIVE_SYNC_MASS_DELETE", re.escape(SYNC_DRIVE) + r" there 12"))
        app.until("the question", lambda: app.has("#" + I("sync-mass")))
        if any("--allow" in (t or "") for t in app.texts()):
            raise Failure("the question names the command line's switch")
        app.screenshot(os.path.join(out, "25-sync-mass-delete.png"))
        app.after("Keep them", "AZDRIVE_SYNC_ANSWERED",
                  re.escape(SYNC_DRIVE) + r" mass-delete keep",
                  lambda: (app.must("click", selector="#" + I("sync-mass-keep")), app.frame()))
        app.until("kept on the drive, cloud only here", lambda: app.printed(
            "AZDRIVE_SYNC_FILE", re.escape(SYNC_DRIVE) + r" cloud-only bulk/0\.txt"))
        if "bulk/0.txt" not in sync_index(s3_root).get("files", {}):
            raise Failure("Keep them deleted the files on the drive")
        log("25h. 12 files gone from the folder: asked in AzDrive's words; Keep them kept them on "
            "the drive (cloud only here)")

        # 25i. Files turned random at once on the Azlin drive: uploads pause, downloads go on;
        # the question waits (Decide later), Sync now asks again, "These changes are mine" sends
        # them.
        paid_bucket = paid["drive"]["location"]["bucket"]
        paid_folder = os.path.join(home, "AzDrive", PAID_NAME)
        os.makedirs(paid_folder, exist_ok=True)
        prose = (b"the quarterly report says the numbers look fine for now " * 40)[:2048]
        paid_row = "#__azdrive_side_drive_" + re.sub(r"[^A-Za-z0-9_-]", "_", paid_id).lower()
        # Clicks that missed a button the window showed (press_or_explain): the step fails
        # with them once the rest ran.
        missed = []

        def text_files(stem):
            """12 text files `<stem><i>.txt` in the Azlin drive's folder, up on the drive, and
            one more pass (the burst guard learns they are text-like)."""
            for i in range(12):
                with open(os.path.join(paid_folder, "%s%d.txt" % (stem, i)), "wb") as f:
                    f.write(prose[:2040] + b"%08d" % i)
            app.until("the Azlin drive's %s files uploaded" % stem, lambda: all(
                "%s%d.txt" % (stem, i) in sync_index(s3_root, paid_bucket).get("files", {})
                for i in range(12)))
            done = app.count("AZDRIVE_SYNC_DONE", re.escape(paid_id) + r" .*")
            app.until("one more pass (the guard learns them)", lambda: app.count(
                "AZDRIVE_SYNC_DONE", re.escape(paid_id) + r" .*") > done)

        def prose_of(i):
            """The BLAKE3 of the i-th text file's words (what the drive's index names)."""
            return azlin_blake3.hex_digest(prose[:2040] + b"%08d" % i)

        def turn_random(stem):
            """The 12 files `<stem><i>.txt` rewritten as random bytes at once (ransomware's
            work): uploads pause, the question shows."""
            bursts = app.count("AZDRIVE_SYNC_BURST", re.escape(paid_id) + r" encryption \d+")
            for i in range(12):
                with open(os.path.join(paid_folder, "%s%d.txt" % (stem, i)), "wb") as f:
                    f.write(os.urandom(2048))
            app.until("uploads paused", lambda: app.count(
                "AZDRIVE_SYNC_BURST", re.escape(paid_id) + r" encryption \d+") > bursts)
            app.until("the guard's question", lambda: app.has("#" + I("sync-burst")))

        text_files("p")
        turn_random("p")
        app.screenshot(os.path.join(out, "25-sync-burst.png"))
        before = sync_index(s3_root, paid_bucket)["files"]["p0.txt"]["hash"]
        other_device_writes(s3_root, "from-desktop.txt", b"while paused\n", bucket=paid_bucket)
        app.until("downloads go on while paused", lambda: read_file(
            os.path.join(paid_folder, "from-desktop.txt")) == b"while paused\n")
        if sync_index(s3_root, paid_bucket)["files"]["p0.txt"]["hash"] != before:
            raise Failure("a paused folder sent its changes")
        app.click(text="Decide later")
        app.until("Decide later", lambda: not app.has("#" + I("sync-burst")))
        app.after("the Azlin drive", "AZDRIVE_PLACE", re.escape(paid_id) + r" .*",
                  lambda: app.click(selector=paid_row))
        app.tab("Share")
        app.after("Sync now asks again", "AZDRIVE_SYNC_QUESTION", re.escape(paid_id) + r" burst",
                  lambda: app.ribbon("Sync now"))
        app.until("the guard's question", lambda: app.has("#" + I("sync-burst")))
        app.after("These changes are mine", "AZDRIVE_SYNC_ANSWERED",
                  re.escape(paid_id) + r" burst mine",
                  lambda: (app.must("click", selector="#" + I("sync-burst-mine")), app.frame()))
        mine = azlin_blake3.hex_digest(read_file(os.path.join(paid_folder, "p0.txt")))
        app.until("sent with the next pass", lambda: sync_index(
            s3_root, paid_bucket).get("files", {}).get("p0.txt", {}).get("hash") == mine)
        log("25i. 12 files of the Azlin drive turned random: uploads paused (a download came on); "
            "the question waited, Sync now asked again, These changes are mine sent them")

        # 25j. Again with other files, and this time it was ransomware - on the other computer
        # too, which sent its encrypted q0.txt: I was hacked... -> Restore as of before the
        # change. The drive has q0.txt back; the encrypted copies here never went up, and against
        # the restored drive (its index older than this folder's last pass) each waits for a
        # choice (D52) instead of overwriting either side; "Take the drive's version" brings
        # each restored file back here.
        text_files("q")
        time.sleep(1.2)
        as_of = int(time.time())
        time.sleep(1.2)
        turn_random("q")
        other_device_writes(s3_root, "q0.txt", os.urandom(2048), bucket=paid_bucket)
        app.until("a pass saw the other computer's q0.txt", lambda: app.printed(
            "AZDRIVE_SYNC_FILE", re.escape(paid_id) + r" conflict q0\.txt"))
        # That pass held q0.txt as a conflict and told the open question its new list (the 11
        # files left to send): the dialog changed its height and its buttons slide to their new
        # place. `app.click` waits until they stand still - a click mid-slide hits nothing.
        app.until("the guard's question still open", lambda: app.has("#" + I("sync-burst")))
        press_or_explain(app, out, "#" + I("sync-burst-hacked"), "AZDRIVE_SYNC_HACKED",
                         re.escape(paid_id), "I was hacked...", "25-sync-hacked",
                         question="#" + I("sync-burst"), under="#" + I("folder-rows"),
                         missed=missed)
        app.until("lock down / restore", lambda: app.has("#" + I("sync-hacked")))
        app.click(selector="#" + I("sync-hacked-restore"))
        app.until("the restore's time field", lambda: app.has("#__azdrive_restore_time"))
        app.must("focus_node", selector="#__azdrive_restore_time")
        app.frame(2)
        # The time typed over what the field opens with, whatever its words.
        app.key("a", primary=True)
        app.key("backspace")
        app.must("text_input", text=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(as_of)))
        app.frame(2)
        held = app.count("AZDRIVE_SYNC_FILE", re.escape(paid_id) + r" conflict q1\.txt")
        app.after("restored as of before the change", "AZDRIVE_RESTORED",
                  r"%s \S+ objects \d+" % re.escape(paid_id),
                  lambda: app.click(selector="#__azdrive_restore_go"))
        if app.has("#__azdrive_restore"):
            app.key("escape")
        app.until("the drive has q0.txt back", lambda: sync_index(
            s3_root, paid_bucket).get("files", {}).get("q0.txt", {}).get("hash") == prose_of(0))
        app.until("the encrypted copies here wait for a choice", lambda: app.count(
            "AZDRIVE_SYNC_FILE", re.escape(paid_id) + r" conflict q1\.txt") > held)
        files = sync_index(s3_root, paid_bucket)["files"]
        if any(files.get("q%d.txt" % i, {}).get("hash") != prose_of(i) for i in range(12)):
            raise Failure("an encrypted copy went up over the restored drive")
        # Each question in turn (the next comes with the pass after an answer): the drive's
        # restored version over the encrypted copy here. With nothing left to send the pause
        # ends by itself, and no question is left open for the next steps.
        for n in range(12):
            app.until("choice %d of 12" % (n + 1), lambda: app.has("#" + I("sync-conflict")))
            press_or_explain(app, out, "#" + I("sync-take-theirs"), "AZDRIVE_SYNC_RESOLVED",
                             re.escape(paid_id) + r" theirs q\d+\.txt",
                             "Take the drive's version (%d of 12)" % (n + 1),
                             "25-sync-take-theirs", question="#" + I("sync-conflict"),
                             under="#" + I("folder-rows"), missed=missed)
        for i in range(12):
            path = os.path.join(paid_folder, "q%d.txt" % i)
            app.until("q%d.txt restored here" % i, lambda: read_file(path)
                      == prose[:2040] + b"%08d" % i)
        app.until("no question left open", lambda: not app.has("#" + I("sync-conflict")))
        log("25j. again with other files, and it was ransomware (on the other computer too): I "
            "was hacked... restored the drive as of before the change; the encrypted copies "
            "here waited for a choice (D52), and the drive's version brought each file back")

        # 25k. A metered network (azul's NetworkState, switched through the headless network
        # file AZ_NETWORK_STATE_FILE, read at every query): "Paused (metered network)"; a file
        # over the drive's auto-download size (25 MB) waits here while a small one goes up;
        # "Sync anyway on this network" (Options > Drives > Sync, kept in the view settings)
        # sends it; a Low Data Mode network pauses too; a free network again: up to date.
        app.after("the sync drive's own listing", "AZDRIVE_LISTED",
                  re.escape(SYNC_DRIVE) + r" / \d+", lambda: app.click(selector=row))
        write_network(network_file, "cellular metered")
        app.until("the metered network read", lambda: app.printed(
            "AZDRIVE_NETWORK",
            r"Cellular connected=true metered=true constrained=false hotspot=false"))
        app.until("Paused (metered network)", lambda: last_status(app, SYNC_DRIVE)
                  == "azdrive-sync-status-metered")
        app.until("the sync popover says it", lambda: "Paused (metered network)" in status())
        app.until("the drive's row has its glyph", lambda: app.has(
            "#__azdrive_side_sync_" + SYNC_DRIVE))
        app.screenshot(os.path.join(out, "25-sync-metered.png"))
        film = os.path.join(folder, "film.bin")
        frame = b"a long film shot on a metered network, frame by frame. "
        with open(film, "wb") as f:
            f.write(frame * (26 * 1024 * 1024 // len(frame) + 1))
        memo = os.path.join(folder, "memo.txt")
        with open(memo, "wb") as f:
            f.write(b"a small memo, sent on a metered network\n")
        app.until("the small file went up", lambda: "memo.txt" in sync_index(s3_root).get(
            "files", {}))
        app.until("a pass held the big one back", lambda: any(
            "held=1" in line for line in app.printed(
                "AZDRIVE_SYNC_DONE", re.escape(SYNC_DRIVE) + r" .*")))
        if "film.bin" in sync_index(s3_root).get("files", {}):
            raise Failure("the big file went up on a metered network")
        if last_status(app, SYNC_DRIVE) != "azdrive-sync-status-metered":
            raise Failure("the status line says %r" % last_status(app, SYNC_DRIVE))
        log("25k. a metered network: \"Paused (metered network)\"; memo.txt went up, film.bin "
            "(26 MB, over the auto-download size) waited")

        # Sync anyway on this network: the big file goes up now; the setting is kept.
        app.tab("View")
        app.ribbon("Options")
        app.until("the Options", lambda: app.has("#" + I("settings")))
        app.click_exact("Drives")
        anyway = "#__azdrive_sync_metered_" + SYNC_DRIVE
        app.until("Sync anyway on this network", lambda: app.has(anyway))
        app.op("scroll_into_view", selector=anyway, block="center", behavior="instant")
        app.frame(2)
        app.screenshot(os.path.join(out, "25-sync-anyway.png"))
        app.after("Sync anyway on this network", "AZDRIVE_SYNC_SETTING",
                  re.escape(SYNC_DRIVE) + r" sync_on_metered true",
                  lambda: app.click(selector=anyway))
        app.until("kept with the sync settings",
                  lambda: synced_setting(view, SYNC_DRIVE, "sync_on_metered") is True)
        app.until("the big file went up", lambda: "film.bin" in sync_index(s3_root).get(
            "files", {}))
        app.until("no longer paused", lambda: settled(last_status(app, SYNC_DRIVE)))
        # Unticked: the metered network holds big files back again.
        app.after("Sync anyway off again", "AZDRIVE_SYNC_SETTING",
                  re.escape(SYNC_DRIVE) + r" sync_on_metered false",
                  lambda: app.click(selector=anyway))
        app.until("paused again", lambda: last_status(app, SYNC_DRIVE)
                  == "azdrive-sync-status-metered")
        app.until("kept off", lambda: synced_setting(view, SYNC_DRIVE, "sync_on_metered")
                  is False)
        app.key("escape")
        app.until("the Options closed", lambda: not app.has("#" + I("settings")))
        log("25l. Sync anyway on this network: film.bin went up at once (the setting kept in "
            "view.json); unticked, the drive pauses its big files again")

        # A Wi-Fi in Low Data Mode pauses too; a free network: up to date.
        write_network(network_file, "wifi constrained")
        app.until("the low-data network read", lambda: app.printed(
            "AZDRIVE_NETWORK",
            r"WiFi connected=true metered=false constrained=true hotspot=false"))
        app.until("still paused", lambda: last_status(app, SYNC_DRIVE)
                  == "azdrive-sync-status-metered")
        write_network(network_file, "wifi")
        app.until("the free network read", lambda: app.printed(
            "AZDRIVE_NETWORK",
            r"WiFi connected=true metered=false constrained=false hotspot=false"))
        app.until("synced on a free network", lambda: settled(last_status(app, SYNC_DRIVE)))
        app.until("the sync popover says it", lambda: "metered" not in status())
        log("25m. a Low Data Mode Wi-Fi paused the big files too; on a free Wi-Fi the drive is "
            "up to date")

        # 25n. A laptop at 23 % on a phone's hotspot (azul's BatteryState and NetworkState,
        # through the headless battery and network files): AzDrive's client health falls to 35
        # - one transfer at a time, the recompression pass and the drive index's upkeep held,
        # the search index's catch-up too; a small file still goes up; the one line of Options >
        # Drives > Sync says why. Charged on a free Wi-Fi again: 100, full speed.
        healthy = r"100 transfers=4 recompress=allowed index=allowed"
        app.until("a desktop on a free network at first", lambda: app.printed(
            "AZDRIVE_CLIENT_HEALTH", healthy))
        write_battery(battery_file, "battery 23 discharging")
        write_network(network_file, "hotspot")
        app.until("the phone's hotspot read", lambda: app.printed(
            "AZDRIVE_NETWORK",
            r"WiFi connected=true metered=true constrained=false hotspot=true"))
        app.until("a drained laptop on a hotspot: fewer transfers, the recompression held",
                  lambda: app.printed("AZDRIVE_CLIENT_HEALTH",
                                      r"35 transfers=1 recompress=held index=held"))
        app.until("paused for the hotspot", lambda: last_status(app, SYNC_DRIVE)
                  == "azdrive-sync-status-metered")
        jot = os.path.join(folder, "jot.txt")
        with open(jot, "wb") as f:
            f.write(b"a jot, sent from a laptop at 23 percent on a phone's hotspot\n")
        app.until("a small file still went up", lambda: "jot.txt" in sync_index(s3_root).get(
            "files", {}))
        app.tab("View")
        app.ribbon("Options")
        app.until("the Options", lambda: app.has("#" + I("settings")))
        app.click_exact("Drives")
        computer = "#" + I("sync-health")
        said = "This computer: on battery, 23%, on a phone's hotspot - syncing slowly, big files wait"
        app.until("the computer's line", lambda: app.has(computer))
        app.until("it says the battery, the hotspot and the pace",
                  lambda: " ".join(app.texts_within(computer)) == said)
        app.op("scroll_into_view", selector=computer, block="center", behavior="instant")
        app.frame(2)
        app.screenshot(os.path.join(out, "25-sync-health.png"))
        app.key("escape")
        app.until("the Options closed", lambda: not app.has("#" + I("settings")))
        before = app.count("AZDRIVE_CLIENT_HEALTH", healthy)
        write_battery(battery_file, "charging 90")
        write_network(network_file, "wifi")
        app.until("charged on a free Wi-Fi: full speed again", lambda: app.count(
            "AZDRIVE_CLIENT_HEALTH", healthy) > before)
        app.until("synced at full speed", lambda: settled(last_status(app, SYNC_DRIVE)))
        log("25n. a laptop at 23 %% on a phone's hotspot: client health 35 - one transfer at a "
            "time, recompression and index upkeep held, jot.txt went up, the Options said "
            "\"%s\"; charged on a free Wi-Fi: 100" % said)
        if missed:
            raise Failure("the sync steps ran, but a click missed what the window showed: "
                          + " | ".join(missed))
        return True
    except Failure:
        for name, path in (("stdout", app.out_path), ("stderr", app.err_path)):
            print("\n----- azdrive-sync %s (tail) -----\n%s" % (name, e2e.tail(path)))
        raise
    finally:
        app.stop()
        stack.stop()


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--bin")
    parser.add_argument("--debug-port", type=int, default=8781)
    parser.add_argument("--timeout", type=float, default=240)
    parser.add_argument("--out")
    parser.add_argument("--keep-logs", action="store_true")
    parser.add_argument("--sync-only", action="store_true",
                        help="run step 25 (the folder sync) alone")
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
