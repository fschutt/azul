#!/usr/bin/env python3
"""AzMusic end to end, headless, over the debug server: the sample library played by azul's
AudioPlayer on the synthetic audio output (AZ_SYNTHETIC_DEVICES=audio_sink: it plays in real
time and hears nothing). The window is Spotify's 2010 player: the page is a VirtualView, so its
rows and cards live in a DOM of their own (`every_dom`); the sidebar, the tool bar and the
now-playing bar are in the window's DOM 0.

    1. starts AzMusic with --sample --screen songs in an empty data folder (no --mode: AzMusic is
       dark until the user picks a mode): the six sample tones are written into the data tree
       (AZMUSIC_LIBRARY 6 2), the songs page lists them;
    2. THE CHROME: no title row - the tool bar is the window's top edge and its drag region
       (`-azul-app-region: drag`; Back, Forward and the search field `no-drag`; on macOS Back
       clears the traffic lights); nothing is selectable text but the search field (the status
       line and a song table's cell - in the page's own DOM - say `user-select: none`, the field
       `text`), and the cell is in the player's sans (`system:ui`), not the engine's serif;
    3. PLAY: the play icon a song shows under the pointer plays it ("Harbour Walk"); a
       double-click on "First Light" plays it (AZMUSIC_PLAY, AZMUSIC_STATE playing), the
       now-playing bar names it, the seek bar moves;
    4. GAPLESS: a seek to the end of the track; the queue's next track ("Harbour Walk") is
       heard after it without a stop (AZMUSIC_HEARD Harbour Walk, no AZMUSIC_STATE finished
       between);
    5. PAUSE / PLAY: Space pauses (AZMUSIC_STATE paused), Space plays on;
    6. ALBUMS: the sidebar's Albums shows the covers of "Blue Hour" and "Field Notes"; a click on
       Field Notes opens its page, its Play plays "Low Tide"; Back returns to the albums;
    7. SEARCH: "tide" in the search field finds one song (AZMUSIC_SEARCH 1), the results page;
    8. SETTINGS: Mod+, opens the settings page with the Library section; Escape closes it;
    9. LIGHT, then FLORA: the songs table in flat light, in flora light and dark, an album's page
       in flora - the table's cell still the player's sans, still not selectable.

Screenshots after each step (flat dark first): 1-library, 2-playing, 3-next-track, 4-albums,
5-album, 6-search, 7-settings, 8-light, 9-songs-light, 10-flora-light, 11-flora-dark,
12-flora-album.

Usage (after building libazul with the debug server and AzMusic; ONE app at a time, through
scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azmusic_e2e.py [--bin target/release/AzMusic] [--debug-port 8791]
        [--timeout 180] [--out <dir>] [--keep]
"""

import os
import sys

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azmusic"

# The width macOS's traffic lights take at the window's top-left (azul's
# `TabsInTitlebar::platform()`): the tool bar's first control starts after it.
TRAFFIC_LIGHTS_W = 78.0


def state(app):
    return app.last("AZMUSIC_STATE")


def page_text(app, text):
    """(dom, text node, its box) of the text node showing exactly `text` in the page's
    VirtualView (a DOM of its own, not the window's DOM 0), or None. The box is the `<p>` a cell
    or a title is drawn in."""
    for dom in app.dom_ids():
        if dom == 0:
            continue
        for n in e2e.dicts(app.op("get_node_hierarchy", dom_id=dom)):
            if "index" in n and n.get("text") == text:
                return dom, n["index"], n.get("parent", n["index"])
    return None


def page_node(app, text):
    """(dom, node) of the box holding exactly `text` in the page, or None."""
    found = page_text(app, text)
    return (found[0], found[2]) if found else None


def double_click_in_page(app, text):
    """Double-clicks the song row showing exactly `text` in the page."""
    dom, node = app.until('the row "%s" in the page' % text, lambda: page_node(app, text))
    app.settle(limit=2.0)
    app.must("double_click", node_id=node, dom_id=dom)
    app.frame(2)


def page_row(app, title):
    """(dom, row) of the song row (`.__azmusic_track`) whose title cell shows exactly `title` in
    the page, or None."""
    for dom in app.dom_ids():
        if dom == 0:
            continue
        nodes = [n for n in e2e.dicts(app.op("get_node_hierarchy", dom_id=dom)) if "index" in n]
        by_index = {n["index"]: n for n in nodes}
        for n in nodes:
            if n.get("text") != title:
                continue
            cell = by_index.get(n.get("parent"))
            row = by_index.get(cell.get("parent")) if cell else None
            if row and "__azmusic_track" in (row.get("classes") or []):
                return dom, row["index"]
    return None


def play_from_row_icon(app, title):
    """Points at the song row `title` - its lead cell, where the number turns into a play icon -
    and clicks that icon: it plays the song. The icon is a box with the callback; an icon node's
    own callback is dropped when the icon resolves to its glyph (it only selected the row)."""
    dom, row = app.until('the row "%s" in the page' % title, lambda: page_row(app, title))
    value = app.value("get_node_layout", node_id=row, dom_id=dom) or {}
    r = value.get("screen_rect") or value.get("rect") or {}
    # The page's side padding (16px), then into the 36px lead cell.
    x = float(r.get("x", 0.0)) + 16.0 + 20.0
    y = float(r.get("y", 0.0)) + float(r.get("height", 0.0)) / 2.0
    app.must("mouse_move", x=x, y=y)
    app.frame(2)
    app.until("the play icon of the row under the pointer",
              lambda: app.has(".__azmusic_row-play", every_dom=True))
    before = app.count("AZMUSIC_PLAY")
    app.click(selector=".__azmusic_row-play", every_dom=True)
    app.until("%s to play from its row's icon" % title,
              lambda: app.count("AZMUSIC_PLAY") > before
              and title in (app.last("AZMUSIC_PLAY") or ""))


def css_of(app, dom=None, **target):
    """A node's computed CSS (`get_node_css_properties`) as {property: value}: what it declares
    and what it inherits (`user-select`, `font-family`). `target` is `selector=` or `node_id=`;
    `dom` the DOM it is in (the page is a VirtualView's DOM of its own)."""
    if dom is not None:
        target["dom_id"] = dom
    value = app.value("get_node_css_properties", **target)
    props = value.get("properties") if isinstance(value, dict) else None
    found = {}
    for line in props or []:
        if isinstance(line, str) and ":" in line:
            key, val = line.split(":", 1)
            found[key.strip()] = val.strip()
    return found


def check_title_bar(app):
    """No title row: the tool bar is the window's top edge and the window's drag region, its
    controls keep their presses (`no-drag`), and on macOS Back clears the traffic lights."""
    # The title row's title (`Titlebar`'s `.csd-title`, which said "AzMusic"). Not the bar's
    # class: Linux's software window controls overlay the frame's corner as a controls-only
    # `.csd-titlebar` of their own.
    if app.has(".csd-title") or app.exact("AzMusic") is not None:
        raise Failure("a title row is back over the tool bar")
    bar = app.rect("__azmusic_toolbar")
    if abs(float(bar.get("y", -1.0))) > 0.5:
        raise Failure("the tool bar is not the window's top edge: %r" % bar)
    region = css_of(app, selector="#__azmusic_toolbar").get("-azul-app-region")
    if region != "drag":
        raise Failure("the tool bar is not the window's drag region (-azul-app-region %r)" % region)
    for control in ("back", "forward", "search"):
        got = css_of(app, selector="#__azmusic_%s" % control).get("-azul-app-region")
        if got != "no-drag":
            raise Failure("a press on #__azmusic_%s would move the window (%r)" % (control, got))
    if sys.platform == "darwin":
        back = app.rect("__azmusic_back")
        if float(back.get("x", 0.0)) < TRAFFIC_LIGHTS_W:
            raise Failure("Back sits under the traffic lights: %r" % back)
    app.log("the tool bar is the title bar: %r" % bar)


def check_not_selectable(app, cell_text):
    """Nothing is selectable text but the search field: the status line (DOM 0) and the cell
    showing `cell_text` (the page's own DOM) say `user-select: none`, the field `text`. The cell
    is in the player's sans: a VirtualView inherits nothing from the window, and the page's
    cells, which set no family, were drawn in the engine's default serif."""
    status = css_of(app, selector="#__azmusic_status").get("user-select")
    if status != "none":
        raise Failure("the status line is selectable text (user-select %r)" % status)
    field = css_of(app, selector="#__azmusic_search").get("user-select")
    if field != "text":
        raise Failure("the search field's text is not selectable (user-select %r)" % field)
    dom, text_node, cell = app.until('the cell "%s" in the page' % cell_text,
                                     lambda: page_text(app, cell_text))
    box = css_of(app, dom=dom, node_id=cell)
    if box.get("user-select") != "none":
        raise Failure("the cell %r is selectable text (user-select %r)"
                      % (cell_text, box.get("user-select")))
    glyphs = css_of(app, dom=dom, node_id=text_node).get("user-select")
    if glyphs not in (None, "none"):
        raise Failure("the text of the cell %r is selectable (user-select %r)" % (cell_text, glyphs))
    family = box.get("font-family")
    if family is None:
        app.log("the cell's font-family is not reported (user-select %r)" % box.get("user-select"))
    elif not family.startswith("system:ui"):
        raise Failure("the cell %r is not in the player's sans: font-family %r" % (cell_text, family))
    app.log("not selectable: the status line, the cell %r (font-family %r)" % (cell_text, family))


def body(args, logs, out):
    binary = e2e.find_binary("AzMusic", args.bin, "AZMUSIC_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG, binary, ["--data-dir", data_dir, "--size", "1280x800", "--theme", "flat",
                                "--screen", "songs", "--sample"],
                  args.debug_port, logs, args.timeout,
                  extra_env={"AZ_SYNTHETIC_DEVICES": "audio_sink"})
    try:
        # ---- the sample library ----
        app.until("the sample library", lambda: (app.last("AZMUSIC_LIBRARY") or "") == "6 2")
        app.frame(3)
        app.until("the songs page", lambda: app.has_id("__azmusic_songs"))
        for title in ("First Light", "Harbour Walk", "Low Tide"):
            app.until("%r in the songs page" % title,
                      lambda: app.shows(title, every_dom=True))
        app.until("the sample tones in the data tree", lambda: any(
            "sample" in dirs for _, dirs, _ in os.walk(data_dir)))
        app.screenshot(os.path.join(out, "1-library.png"))

        # ---- the chrome: the tool bar is the title bar, nothing is selectable text ----
        check_title_bar(app)
        check_not_selectable(app, "Harbour Walk")

        # ---- play: from a row's play icon, then by a double-click ----
        play_from_row_icon(app, "Harbour Walk")
        app.until("playing", lambda: state(app) == "playing")
        double_click_in_page(app, "First Light")
        app.until("First Light to play", lambda: "First Light" in (app.last("AZMUSIC_PLAY") or ""))
        app.until("playing", lambda: state(app) == "playing")
        app.until("the now-playing bar", lambda: app.exact("First Light") is not None)
        app.frame(4)
        app.screenshot(os.path.join(out, "2-playing.png"))

        # ---- gapless into the next track ----
        # A click on the seek bar seeks to the click (and focuses it); End seeks to the end of
        # "First Light": the queue's next track, handed to the player in advance, follows at
        # once - no stop in between.
        before = len(app.printed("AZMUSIC_STATE"))
        # Harbour Walk was heard once already (its row's play icon): only a hearing after the
        # seek counts.
        heard_before = len(app.printed("AZMUSIC_HEARD"))
        app.click(selector="#__azmusic_seek")
        app.key("end")
        app.until("Harbour Walk to be heard", lambda: any(
            line.startswith("Harbour Walk")
            for line in app.printed("AZMUSIC_HEARD")[heard_before:]))
        states = app.printed("AZMUSIC_STATE")[before:]
        if "finished" in states:
            raise Failure("the queue stopped between the tracks: %r" % states)
        app.log("gapless: %s" % app.printed("AZMUSIC_HEARD")[-2:])

        # ---- pause / play ----
        app.click(selector="#__azmusic_now-title")
        app.key("space")
        app.until("paused", lambda: state(app) == "paused")
        app.key("space")
        app.until("playing again", lambda: state(app) == "playing")
        app.screenshot(os.path.join(out, "3-next-track.png"))

        # ---- albums: the covers, an album's page, its Play, Back ----
        app.click(selector="#__azmusic_nav-albums")
        app.until("the albums", lambda: app.has_id("__azmusic_albums"))
        for title in ("Blue Hour", "Field Notes"):
            app.until("the cover of %r" % title, lambda: app.shows(title, every_dom=True))
        app.screenshot(os.path.join(out, "4-albums.png"))
        app.click(text="Field Notes", every_dom=True)
        app.until("the album's page", lambda: app.has(".__azmusic_page-album"))
        app.until("its songs", lambda: app.shows("Salt and Cedar", every_dom=True))
        app.click(selector="#__azmusic_page-play", every_dom=True)
        app.until("Low Tide to play", lambda: "Low Tide" in (app.last("AZMUSIC_PLAY") or ""))
        app.frame(4)
        app.screenshot(os.path.join(out, "5-album.png"))
        app.click(selector="#__azmusic_back")
        app.until("back at the albums", lambda: app.has_id("__azmusic_albums"))

        # ---- search ----
        app.click(selector="#__azmusic_search")
        app.must("text_input", text="tide")
        app.frame(2)
        app.until("the search to find Low Tide", lambda: app.last("AZMUSIC_SEARCH") == "1")
        app.until("the results page", lambda: app.has_id("__azmusic_search-results"))
        app.screenshot(os.path.join(out, "6-search.png"))

        # ---- settings ----
        app.key("comma", primary=True)
        app.until("the settings page", lambda: app.has_id("__azmusic_scan"))
        app.screenshot(os.path.join(out, "7-settings.png"))
        app.key("escape")
        app.until("the settings closed", lambda: not app.has_id("__azmusic_scan"))

        # ---- light ----
        app.must("set_mode", mode="light")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "8-light.png"))

        # ---- the songs table, flat light, then flora light and dark; an album in flora ----
        # Flora sets its scopes in Garamond: the player's hand must still win in the table.
        app.click(selector="#__azmusic_nav-songs")
        app.until("the songs page again", lambda: app.has_id("__azmusic_songs"))
        app.until("its songs", lambda: page_text(app, "Harbour Walk") is not None)
        app.screenshot(os.path.join(out, "9-songs-light.png"))
        app.must("set_theme", theme="flora")
        app.frame(3)
        app.until("the songs page in flora", lambda: page_text(app, "Harbour Walk") is not None)
        check_not_selectable(app, "Harbour Walk")
        app.screenshot(os.path.join(out, "10-flora-light.png"))
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.screenshot(os.path.join(out, "11-flora-dark.png"))
        app.click(selector="#__azmusic_nav-albums")
        app.until("the albums in flora", lambda: app.has_id("__azmusic_albums"))
        app.until("the cover of Field Notes", lambda: app.shows("Field Notes", every_dom=True))
        app.click(text="Field Notes", every_dom=True)
        app.until("the album's page in flora", lambda: app.has(".__azmusic_page-album"))
        app.until("its songs in flora", lambda: page_text(app, "Salt and Cedar") is not None)
        check_not_selectable(app, "Salt and Cedar")
        check_title_bar(app)
        app.screenshot(os.path.join(out, "12-flora-album.png"))
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
