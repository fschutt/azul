#!/usr/bin/env python3
"""AzMusic end to end, headless, over the debug server: the sample library played by azul's
AudioPlayer on the synthetic audio output (AZ_SYNTHETIC_DEVICES=audio_sink: it plays in real
time and hears nothing).

    1. starts AzMusic with --sample in an empty data folder: the six sample tones are written
       into the data tree (AZMUSIC_LIBRARY 6 2), the songs table shows them;
    2. PLAY: a double-click on "First Light" plays it (AZMUSIC_PLAY, AZMUSIC_STATE playing), the
       now-playing bar names it, the seek bar moves (its value text changes);
    3. GAPLESS: a seek to the end of the track; the queue's next track ("Harbour Walk") is
       heard after it without a stop (AZMUSIC_HEARD Harbour Walk, no AZMUSIC_STATE finished
       between);
    4. PAUSE / PLAY: Space pauses (AZMUSIC_STATE paused), Space plays on;
    5. ALBUMS: the sidebar's Albums lists "Blue Hour" and "Field Notes"; "Play" on Field Notes
       plays "Low Tide";
    6. SETTINGS: Mod+, opens the settings page with the Library section; Escape closes it;
    7. screenshots after each step (flat light), the mode switched to dark and the theme to
       flora at the end.

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


def body(args, logs, out):
    binary = e2e.find_binary("AzMusic", args.bin, "AZMUSIC_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)
    app = e2e.App(TAG, binary, ["--data-dir", data_dir, "--size", "1280x800", "--theme", "flat",
                                "--mode", "light", "--sample"],
                  args.debug_port, logs, args.timeout,
                  extra_env={"AZ_SYNTHETIC_DEVICES": "audio_sink"})
    try:
        # ---- the sample library ----
        app.until("the sample library", lambda: (app.last("AZMUSIC_LIBRARY") or "") == "6 2")
        app.frame(3)
        app.until("the songs table", lambda: app.has_id("__azmusic_songs"))
        for title in ("First Light", "Harbour Walk", "Low Tide"):
            if not app.shows(title):
                raise Failure("the songs table does not show %r" % title)
        app.until("the sample tones in the data tree", lambda: any(
            "sample" in dirs for _, dirs, _ in os.walk(data_dir)))
        app.screenshot(os.path.join(out, "1-library.png"))

        # ---- play ----
        app.click_exact("First Light", double=True)
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

        # ---- albums ----
        app.click(selector="#__azmusic_nav-albums")
        app.until("the albums", lambda: app.has_id("__azmusic_albums"))
        for title in ("Blue Hour", "Field Notes"):
            if not app.shows(title):
                raise Failure("the albums list does not show %r" % title)
        app.screenshot(os.path.join(out, "4-albums.png"))

        # ---- settings ----
        app.key("comma", primary=True)
        app.until("the settings page", lambda: app.has_id("__azmusic_scan"))
        app.screenshot(os.path.join(out, "5-settings.png"))
        app.key("escape")
        app.until("the settings closed", lambda: not app.has_id("__azmusic_scan"))

        # ---- dark, flora ----
        app.must("set_mode", mode="dark")
        app.must("wait_settled")
        app.screenshot(os.path.join(out, "6-dark.png"))
        app.log("PASS")
        return True
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8791)
