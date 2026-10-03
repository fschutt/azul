#!/usr/bin/env python3
"""AzMaps end to end: the map on the browser shell, over the debug server.

    1. starts AzMaps headless (AZ_BACKEND=headless, the debug server on --debug-port) on a fresh
       data folder (`--data-dir`): it opens on the start view (`AZMAPS_VIEW 37.7749 -122.4194 2.0`)
       and reads an empty pins file (`AZMAPS_PINS_LOADED 0`);
    2. the map fills the shell's content pane (the window is filled: no UA body margin);
    3. the toolbar: zoom in (`AZMAPS_VIEW ... 3.0`), pan east (the longitude grows); the keys:
       Up (the latitude grows), `-` (zoom 2.0 again);
    4. a click on the map drops a pin (`AZMAPS_PINS 1`), written to maps/pins.json
       (`AZMAPS_PINS_SAVED 1`); the pins pane lists it; the viewport is kept in
       maps/settings.json once it rests (`AZMAPS_SETTINGS_SAVED`);
    5. the gear opens azul-appkit's settings page, About the standard About box
       (`AZMAPS_ABOUT open`), Escape closes the box, Escape the page;
    6. flora / dark: a screenshot;
    7. a second start opens where the first one was left (the kept `AZMAPS_VIEW`) with its pin
       (`AZMAPS_PINS_LOADED 1`).

Tiles need the network: headless runs see the map's ground, the pins and the chrome. Run ONE app at
a time, through the capped runner:

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 240 --log /tmp/azmaps.log -- \\
      python3 scripts/azmaps_e2e.py --bin target/release/AzMaps --out /tmp/azmaps-shots

The debug-server client is the shared one (scripts/azlin_e2e.py).
"""

import json
import os

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azmaps"
WIDTH, HEIGHT = 1100, 720
MAP = "__azmaps_map"


def view(app):
    """The last `AZMAPS_VIEW` as (lat, lon, zoom)."""
    line = app.last("AZMAPS_VIEW")
    if not line:
        return None
    lat, lon, zoom = (float(x) for x in line.split())
    return lat, lon, zoom


def wait_view(app, what, check):
    """Waits until the last view satisfies `check(lat, lon, zoom)`."""
    app.until(what, lambda: view(app) is not None and check(*view(app)))
    app.log("%s: %s" % (what, app.last("AZMAPS_VIEW")))


def body(args, logs, out):
    binary = e2e.find_binary("AzMaps", args.bin, "AZMAPS_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)

    def start(name):
        app = e2e.App(name, binary, ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT)],
                      args.debug_port, logs, args.timeout)
        app.until("AzMaps' window", lambda: app.has_id(MAP))
        app.frame(2)
        return app

    app = start("first")
    try:
        app.expect_line("AZMAPS_VIEW", "37.7749 -122.4194 2.0", "the start view")
        app.expect_line("AZMAPS_PINS_LOADED", "0", "an empty pins file")

        r = app.rect(MAP)
        if not r or r.get("height", 0) < 200 or r.get("y", 0) + r.get("height", 0) > HEIGHT + 0.5:
            raise Failure("the map does not fill the content pane of the %dx%d window: %s" % (WIDTH, HEIGHT, r))
        app.screenshot(os.path.join(out, "1-start.png"))

        # The toolbar.
        app.click(selector="#__azmaps_zoom-in")
        wait_view(app, "zoom in", lambda lat, lon, zoom: abs(zoom - 3.0) < 0.05)
        lon0 = view(app)[1]
        app.click(selector="#__azmaps_pan-right")
        wait_view(app, "pan east", lambda lat, lon, zoom: lon > lon0 + 1.0)

        # The keys.
        lat0 = view(app)[0]
        app.key("up")
        wait_view(app, "Up pans north", lambda lat, lon, zoom: lat > lat0 + 0.5)
        app.key("minus")
        wait_view(app, "- zooms out", lambda lat, lon, zoom: abs(zoom - 2.0) < 0.05)

        # A pin: dropped, saved, listed.
        app.click(selector="#" + MAP)
        app.expect_line("AZMAPS_PINS", "1", "a click on the map drops a pin")
        app.expect_line("AZMAPS_PINS_SAVED", "1", "the pin is written")
        with open(os.path.join(data_dir, "maps", "pins.json"), "r", encoding="utf-8") as f:
            pins = json.load(f)
        if len(pins) != 1:
            raise Failure("maps/pins.json holds %s" % pins)
        if not app.shows("1 pin"):
            raise Failure("the status bar does not count the pin")
        app.screenshot(os.path.join(out, "2-pin.png"))

        # The viewport is kept once it rests.
        app.until("the viewport kept", lambda: app.printed("AZMAPS_SETTINGS_SAVED", r".+"))
        kept = app.last("AZMAPS_VIEW")
        with open(os.path.join(data_dir, "maps", "settings.json"), "r", encoding="utf-8") as f:
            if '"view"' not in f.read():
                raise Failure("maps/settings.json does not keep the view")

        # Settings and About.
        app.click(selector="#__azmaps_settings")
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        app.click(text="About")
        app.until("the About section", lambda: app.has_id("appkit-about-open"))
        app.click(selector="#appkit-about-open")
        app.expect_line("AZMAPS_ABOUT", "open", "the About box opens")
        app.screenshot(os.path.join(out, "3-about.png"))
        app.key("escape")
        app.expect_line("AZMAPS_ABOUT", "closed", "Escape closes the About box first")
        app.key("escape")
        app.until("the map again", lambda: not app.has_id("appkit-settings") and app.has_id(MAP))

        # Flora / dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.screenshot(os.path.join(out, "4-flora-dark.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # The second start: where the first was left, with its pin.
    app = start("second")
    try:
        first = app.printed("AZMAPS_VIEW")[:1]
        if first != [kept]:
            raise Failure("the second start opened on %s, not the kept %s" % (first, kept))
        app.expect_line("AZMAPS_PINS_LOADED", "1", "the pin is read back")
        app.screenshot(os.path.join(out, "5-second-start.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: toolbar, keys, pins, settings, About, kept view and pins; screenshots in %s" % out)
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8774)
