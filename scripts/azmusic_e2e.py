#!/usr/bin/env python3
"""AzMusic end to end, headless, over the debug server: the sample library played by azul's
AudioPlayer on the synthetic audio output (AZ_SYNTHETIC_DEVICES=audio_sink: it plays in real
time and hears nothing). The window is Spotify's 2010 player: the page is a VirtualView, so its
rows and cards live in a DOM of their own (`every_dom`); the sidebar, the tool bar and the
now-playing bar are in the window's DOM 0.

    1. starts AzMusic with --sample --screen songs in an empty data folder (no --mode: AzMusic is
       dark until the user picks a mode): the six sample tones are written into the data tree
       (AZMUSIC_LIBRARY 6 2), the songs page lists them;
    2. PLAY: a double-click on "First Light" plays it (AZMUSIC_PLAY, AZMUSIC_STATE playing), the
       now-playing bar names it, the seek bar moves;
    3. GAPLESS: a seek to the end of the track; the queue's next track ("Harbour Walk") is
       heard after it without a stop (AZMUSIC_HEARD Harbour Walk, no AZMUSIC_STATE finished
       between);
    4. PAUSE / PLAY: Space pauses (AZMUSIC_STATE paused), Space plays on;
    5. ALBUMS: the sidebar's Albums shows the covers of "Blue Hour" and "Field Notes"; a click on
       Field Notes opens its page, its Play plays "Low Tide"; Back returns to the albums;
    6. SEARCH: "tide" in the search field finds one song (AZMUSIC_SEARCH 1), the results page;
    7. SETTINGS: Mod+, opens the settings page with the Library section; Escape closes it;
    8. screenshots after each step (dark), the mode switched to light at the end.

Usage (after building libazul with the debug server and AzMusic; ONE app at a time, through
scripts/waves/tools/run_capped.sh on the 8 GB Mac):

    python3 scripts/azmusic_e2e.py [--bin target/release/AzMusic] [--debug-port 8791]
        [--timeout 180] [--out <dir>] [--keep]
"""

import os

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azmusic"


def state(app):
    return app.last("AZMUSIC_STATE")


def page_node(app, text):
    """(dom, node) of the box holding exactly `text` in the page's VirtualView (a DOM of its
    own, not the window's DOM 0), or None."""
    for dom in app.dom_ids():
        if dom == 0:
            continue
        for n in e2e.dicts(app.op("get_node_hierarchy", dom_id=dom)):
            if "index" in n and n.get("text") == text:
                return dom, n.get("parent", n["index"])
    return None


def double_click_in_page(app, text):
    """Double-clicks the song row showing exactly `text` in the page."""
    dom, node = app.until('the row "%s" in the page' % text, lambda: page_node(app, text))
    app.settle(limit=2.0)
    app.must("double_click", node_id=node, dom_id=dom)
    app.frame(2)


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

        # ---- play ----
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
        app.click(selector="#__azmusic_seek")
        app.key("end")
        app.until("Harbour Walk to be heard", lambda: any(
            line.startswith("Harbour Walk") for line in app.printed("AZMUSIC_HEARD")))
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
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
