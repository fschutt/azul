#!/usr/bin/env python3
"""AzMaps end to end: the map is the window, over the debug server.

    1. starts AzMaps headless (AZ_BACKEND=headless, the debug server on --debug-port) on a fresh
       data folder (`--data-dir`): it opens on the start view (`AZMAPS_VIEW 37.7749 -122.4194 2.0`)
       and reads an empty pins file (`AZMAPS_PINS_LOADED 0`);
    2. the map fills the WHOLE window - no top bar, no status bar - and the draggable title area
       lies over its top edge;
    3. the controls: zoom in (`AZMAPS_VIEW ... 3.0`); the keys: Right (the longitude grows), Up
       (the latitude grows), `-` (zoom 2.0 again);
    3b. a drag pans the map (`AZMAPS_VIEW` moves) in the map's own view: it renders again
       (`AZ_MAP_RENDER`, `--stats`) while the window is NOT rebuilt for every pointer move
       (`AZMAPS_LAYOUT` - nothing of AzMaps' own is on the map yet);
    4. a click on the map drops a pin (`AZMAPS_PINS 1`), written to maps/pins.json
       (`AZMAPS_PINS_SAVED 1`); the sidebar's recents list it (`#__azmaps_place-0`);
    5. the place's row centres the map on it and opens its card - a popover at its pin, a window
       of its own (`AZMAPS_PLACE 0`); the card's Directions makes it the destination
       (`AZMAPS_TRAVEL car - <lat>,<lon>`) and closes the card;
    6. the travel panel: walking (`AZMAPS_TRAVEL walk ...`), a start typed in
       (`AZMAPS_TRAVEL walk 48.2082,16.3738 ...`) shows the distance, and the route worker answers
       off the UI thread (`AZMAPS_ROUTE walk <km> <minutes> <ms>`); the viewport is kept in
       maps/settings.json once it rests;
    7. the sidebar hides and shows again (`AZMAPS_SIDEBAR closed` / `open`);
    8. the gear opens azul-appkit's settings page, About the standard About box
       (`AZMAPS_ABOUT open`), Escape closes the box, Escape the page;
    9. flora / dark: a screenshot;
   10. a second start opens where the first one was left (the kept `AZMAPS_VIEW`) with its pin
       (`AZMAPS_PINS_LOADED 1`).

Tiles need the network: headless runs see the map's ground, the pins and the chrome (offline, the
tile workers fail fast; `AZ_MAP_TILES` / `AZ_MAP_TILE` lines are logged, not required). AzMaps
runs with `--stats`, so the log carries the counters to measure with: `AZMAPS_LAYOUT <n>` per
window rebuild, `AZ_MAP_RENDER <us> <tiles> <labels>` per render of the map's view. Run ONE app at
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
TITLE = "__azmaps_title"
SIDEBAR = "__azmaps_sidebar"


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


def kept_view(data_dir):
    """The viewport maps/settings.json keeps, the way AzMaps prints a view (`lat lon zoom`), or
    None before it keeps one."""
    try:
        with open(os.path.join(data_dir, "maps", "settings.json"), "r", encoding="utf-8") as f:
            values = json.load(f).get("values") or {}
        lat, lon, zoom = (float(x) for x in values["view"].split(","))
    except (OSError, ValueError, KeyError, AttributeError, TypeError):
        return None
    return "%.4f %.4f %.1f" % (lat, lon, zoom)


def body(args, logs, out):
    binary = e2e.find_binary("AzMaps", args.bin, "AZMAPS_BIN")
    data_dir = os.path.join(logs, "data")
    os.makedirs(data_dir, exist_ok=True)

    def start(name):
        app = e2e.App(name, binary,
                      ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--stats"],
                      args.debug_port, logs, args.timeout)
        app.until("AzMaps' window", lambda: app.has_id(MAP))
        app.frame(2)
        return app

    app = start("first")
    try:
        app.expect_line("AZMAPS_VIEW", "37.7749 -122.4194 2.0", "the start view")
        app.expect_line("AZMAPS_PINS_LOADED", "0", "an empty pins file")

        # The map is the window; the title area lies over its top.
        r = app.rect(MAP)
        if (not r or r.get("y", 99) > 1 or r.get("width", 0) < WIDTH * 0.98
                or r.get("height", 0) < HEIGHT * 0.9):
            raise Failure("the map does not fill the %dx%d window: %s" % (WIDTH, HEIGHT, r))
        t = app.rect(TITLE)
        if not t or t.get("y", 99) > 1 or t.get("height", 0) <= 0 or t.get("width", 0) < WIDTH * 0.9:
            raise Failure("no title area across the top of the map: %s" % t)
        if not app.has_id(SIDEBAR):
            raise Failure("the sidebar is not open on a first start")
        app.screenshot(os.path.join(out, "1-start.png"))

        # The controls and the keys.
        app.click(selector="#__azmaps_zoom-in")
        wait_view(app, "zoom in", lambda lat, lon, zoom: abs(zoom - 3.0) < 0.05)
        lon0 = view(app)[1]
        app.key("right")
        wait_view(app, "Right pans east", lambda lat, lon, zoom: lon > lon0 + 1.0)
        lat0 = view(app)[0]
        app.key("up")
        wait_view(app, "Up pans north", lambda lat, lon, zoom: lat > lat0 + 0.5)
        app.key("minus")
        wait_view(app, "- zooms out", lambda lat, lon, zoom: abs(zoom - 2.0) < 0.05)

        # A drag pans the map in its own view. Nothing of AzMaps' own is on the map yet (no pin,
        # no route, no location), so the window is not rebuilt for the pointer moves - only the
        # map's view renders again. (The kept viewport's settings save ends in one rebuild of
        # its own: let it happen first, and allow one.)
        app.until("the map renders its own view", lambda: app.count("AZ_MAP_RENDER") > 0)
        app.until("the viewport kept before the drag",
                  lambda: kept_view(data_dir) == app.last("AZMAPS_VIEW"))
        app.frame(3)
        layouts, renders, before = app.count("AZMAPS_LAYOUT"), app.count("AZ_MAP_RENDER"), view(app)
        app.drag(WIDTH * 0.65, HEIGHT * 0.55, WIDTH * 0.5, HEIGHT * 0.45, steps=8)
        wait_view(app, "a drag pans the map",
                  lambda lat, lon, zoom: (round(lat, 4), round(lon, 4)) != before[:2])
        rebuilt = app.count("AZMAPS_LAYOUT") - layouts
        rendered = app.count("AZ_MAP_RENDER") - renders
        app.log("drag: %d window rebuild(s), %d render(s) of the map's view, last %s"
                % (rebuilt, rendered, app.last("AZ_MAP_RENDER")))
        if rendered < 1:
            raise Failure("the drag did not render the map's view again")
        if rebuilt > 1:
            raise Failure("a drag over a map with nothing of AzMaps' own on it rebuilt the window "
                          "%d times (once per pointer move?)" % rebuilt)
        tiles = app.last("AZ_MAP_TILES")
        app.log("tiles (ready pending fetching failed drawn): %s" % (tiles or "no tile came back yet"))

        # A pin: dropped, saved, listed in the recents.
        app.click(selector="#" + MAP)
        app.expect_line("AZMAPS_PINS", "1", "a click on the map drops a pin")
        app.expect_line("AZMAPS_PINS_SAVED", "1", "the pin is written")
        with open(os.path.join(data_dir, "maps", "pins.json"), "r", encoding="utf-8") as f:
            pins = json.load(f)
        if len(pins) != 1:
            raise Failure("maps/pins.json holds %s" % pins)
        app.until("the pin in the recents", lambda: app.has_id("__azmaps_place-0"))
        if not app.has_id("__azmaps_place-pin-0"):
            raise Failure("the pin is not drawn on the map")
        app.screenshot(os.path.join(out, "2-pin.png"))

        # Its card: a popover at the pin, opened from the recents.
        app.click(selector="#__azmaps_place-0")
        app.expect_line("AZMAPS_PLACE", "0", "the row opens the place's card")
        app.until("the place's card (a popover window)", lambda: app.popup() is not None)
        app.screenshot(os.path.join(out, "3-card.png"))
        app.click(selector="#__azmaps_place-directions-0", window=app.popup())
        app.until("Directions: the place is the destination",
                  lambda: app.printed("AZMAPS_TRAVEL", r"car - -?\d+\.\d{4},-?\d+\.\d{4}"))
        app.until("the card closes", lambda: app.popup() is None)

        # The travel panel: a mode, a start; the distance as the crow flies.
        app.click(selector="#__azmaps_travel-walk")
        app.until("walking", lambda: app.printed("AZMAPS_TRAVEL", r"walk - .+"))
        app.text_input("#__azmaps_travel-from", "48.2082, 16.3738")
        app.until("the start typed in",
                  lambda: app.printed("AZMAPS_TRAVEL", r"walk 48\.2082,16\.3738 -?\d.+"))
        app.until("the distance", lambda: app.has_id("__azmaps_travel-distance"))
        # Both ends are places: the route worker answers, off the UI thread.
        app.until("the route worker's answer",
                  lambda: app.printed("AZMAPS_ROUTE", r"walk \d+\.\d \d+ \d+\.\d{3}"))
        app.log("route: %s" % app.last("AZMAPS_ROUTE"))
        app.screenshot(os.path.join(out, "4-travel.png"))

        # The viewport is kept once it rests (the row moved it last).
        app.until("the viewport kept", lambda: kept_view(data_dir) == app.last("AZMAPS_VIEW"))
        kept = app.last("AZMAPS_VIEW")

        # The sidebar hides and comes back.
        app.click(selector="#__azmaps_sidebar-toggle")
        app.expect_line("AZMAPS_SIDEBAR", "closed", "the sidebar hides")
        app.until("no sidebar", lambda: not app.has_id(SIDEBAR))
        app.click(selector="#__azmaps_sidebar-toggle")
        app.expect_line("AZMAPS_SIDEBAR", "open", "the sidebar shows")
        app.until("the sidebar again", lambda: app.has_id(SIDEBAR))

        # Settings and About.
        app.click(selector="#__azmaps_settings")
        app.until("the settings page", lambda: app.has_id("appkit-settings"))
        app.click(text="About")
        app.until("the About section", lambda: app.has_id("appkit-about-open"))
        app.click(selector="#appkit-about-open")
        app.expect_line("AZMAPS_ABOUT", "open", "the About box opens")
        app.screenshot(os.path.join(out, "5-about.png"))
        app.key("escape")
        app.expect_line("AZMAPS_ABOUT", "closed", "Escape closes the About box first")
        app.key("escape")
        app.until("the map again", lambda: not app.has_id("appkit-settings") and app.has_id(MAP))

        # Flora / dark.
        app.must("set_theme", theme="flora")
        app.must("set_mode", mode="dark")
        app.frame(3)
        app.screenshot(os.path.join(out, "6-flora-dark.png"))
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
        app.screenshot(os.path.join(out, "7-second-start.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: full-window map, controls, keys, pins, the place's card, travel, sidebar, "
            "settings, About, kept view and pins; screenshots in %s" % out)
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8774)
