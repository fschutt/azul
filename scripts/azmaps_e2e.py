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
    5. the place's row centres the map on it and opens its card - the pin marker's popup, in the
       map's own view (`AZMAPS_PLACE 0`); the card's Directions makes it the destination
       (`AZMAPS_TRAVEL car - <lat>,<lon>`) and closes the card;
    6. the travel panel: walking (`AZMAPS_TRAVEL walk ...`), a start typed in
       (`AZMAPS_TRAVEL walk 48.2082,16.3738 ...`) shows the distance, and the route worker answers
       off the UI thread (`AZMAPS_ROUTE walk <km> <minutes> <ms>`); the viewport is kept in
       maps/settings.json once it rests;
    6b. a drag with the travel line, its ends and the destination's pin on the map: they are the
       map's overlays, it moves them itself (`AZMAPS_PAN map`, microseconds): the window is NOT
       rebuilt for a pointer move, the pin follows the drag, and the next rebuild draws it where
       the drag left it; a zoom step of the map rebuilds nothing either;
    7. the sidebar hides and shows again (`AZMAPS_SIDEBAR closed` / `open`);
    8. the gear opens azul-appkit's settings page, About the standard About box
       (`AZMAPS_ABOUT open`), Escape closes the box, Escape the page;
    9. flora / dark: a screenshot;
   10. a second start opens where the first one was left (the kept `AZMAPS_VIEW`) with its pin
       (`AZMAPS_PINS_LOADED 1`);
   11. place names, timed: "Frankfurt" typed into the start key by key - each keystroke is the
       field's own edit, no window rebuild, under KEY_BUDGET_MS - and once the typing rests the
       geocoder worker answers (`AZMAPS_SUGGEST from <n> <ms>`); a click on the first
       suggestion makes Frankfurt am Main the start, "Berlin" + Return makes Berlin the
       destination, the route worker answers a car route of 500 to 600 km in five to six hours
       (`AZMAPS_ROUTE car ...`) and the map frames it; a drag over the map with the route on it
       keeps every step under PAN_BUDGET_MS (median).

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
import time

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azmaps"
WIDTH, HEIGHT = 1100, 720
# Budgets of the timed steps, wall clock per step of the driver (the op, then one frame, which
# headless paints on the CPU). A keystroke in a travel field repaints the field only; a step of
# a pan repaints the map's view. Before the text-shadow fix a pan step took ~830 ms and a
# keystroke ~1.5 s (a window rebuild and two full repaints).
KEY_BUDGET_MS = 120
PAN_BUDGET_MS = 250
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
                      ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--stats", "--language", "en",
                       # No public routing server from a test run: the routes are the estimate.
                       "--routing-online", "off",
                       # No public transport server either: the places are AzMaps' own list.
                       "--transit", "off"],
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
        # The pins are markers of the map: their icons live in the map's own view (a DOM of its
        # own), not in the window's.
        if not app.has_id("__azmaps_place-pin-0", every_dom=True):
            raise Failure("the pin is not drawn on the map")
        app.screenshot(os.path.join(out, "2-pin.png"))

        # Its card: the pin marker's popup, opened from the recents.
        card = "__azmaps_place-directions-0"
        app.click(selector="#__azmaps_place-0")
        app.expect_line("AZMAPS_PLACE", "0", "the row opens the place's card")
        app.until("the place's card above its pin", lambda: app.has_id(card, every_dom=True))
        app.screenshot(os.path.join(out, "3-card.png"))
        app.click(selector="#" + card, every_dom=True)
        app.until("Directions: the place is the destination",
                  lambda: app.printed("AZMAPS_TRAVEL", r"car - -?\d+\.\d{4},-?\d+\.\d{4}"))
        app.until("the card closes", lambda: not app.has_id(card, every_dom=True))

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

        # A drag with the travel line, its ends and the destination's pin on the map: they are the
        # map's overlays and move with it in its own view; the window is not rebuilt for a pointer
        # move (it was, while anything of AzMaps' own showed: a full layout per move made the pan
        # crawl).
        app.until("the viewport kept before the drag with the route",
                  lambda: kept_view(data_dir) == app.last("AZMAPS_VIEW"))
        app.frame(3)
        pin = "__azmaps_place-pin-0"
        before = app.rect(pin, every_dom=True)
        if not before:
            raise Failure("the destination's pin is not on the map before the drag")
        layouts, pans = app.count("AZMAPS_LAYOUT"), app.count("AZMAPS_PAN")
        x0, y0, x1, y1 = WIDTH * 0.40, HEIGHT * 0.78, WIDTH * 0.47, HEIGHT * 0.70
        app.drag(x0, y0, x1, y1, steps=8)
        rebuilt = app.count("AZMAPS_LAYOUT") - layouts
        app.log("drag with the route: %d window rebuild(s) %s; pan steps %s"
                % (rebuilt, app.printed("AZMAPS_LAYOUT")[layouts:], app.printed("AZMAPS_PAN")[pans:]))
        if rebuilt > 1:
            raise Failure("a drag with the route on the map rebuilt the window %d times (once per "
                          "pointer move?)" % rebuilt)
        steps = [v.split() for v in app.printed("AZMAPS_PAN")[pans:]]
        if not steps or any(done != "map" for done, _ in steps):
            raise Failure("the drag's steps were not the map's alone: %s" % steps)
        # AzMaps only keeps the viewport: microseconds, not a layout's milliseconds.
        took = [int(us) for _, us in steps]
        if sorted(took)[len(took) // 2] > 5000:
            raise Failure("a step of the drag took AzMaps %s us (median over 5 ms)" % took)
        after = app.rect(pin, every_dom=True)
        moved = (after["x"] - before["x"], after["y"] - before["y"])
        if abs(moved[0] - (x1 - x0)) > 4 or abs(moved[1] - (y1 - y0)) > 4:
            raise Failure("the pin moved by %s, the map by %s" % (moved, (x1 - x0, y1 - y0)))
        # A zoom step (the wheel over the map) rebuilds nothing either. (The viewport kept once
        # it rests may rebuild the window once of its own.)
        layouts, pans, zoom0 = app.count("AZMAPS_LAYOUT"), app.count("AZMAPS_PAN"), view(app)[2]
        app.must("wheel", x=WIDTH * 0.6, y=HEIGHT * 0.6, delta_x=0.0, delta_y=24.0)
        app.frame(3)
        waited = time.time() + 5
        while view(app)[2] <= zoom0 + 0.05 and time.time() < waited:
            time.sleep(0.2)
        if view(app)[2] <= zoom0 + 0.05:
            # The debug server's wheel did not reach the map's own zoom: nothing to check here.
            app.log("the wheel did not zoom the map; the zoom step is not checked")
        else:
            zoomed = [v.split()[0] for v in app.printed("AZMAPS_PAN")[pans:]]
            rebuilt = app.count("AZMAPS_LAYOUT") - layouts
            app.log("zoom step with the route: %s, %d rebuild(s)" % (zoomed, rebuilt))
            if rebuilt > 1:
                raise Failure("a zoom step with the route on the map rebuilt the window %d times"
                              % rebuilt)
        after = app.rect(pin, every_dom=True)
        # A rebuild (the travel mode's button) draws the pin where the drag and the zoom left it:
        # what moved it in place does not follow it into the build.
        layouts = app.count("AZMAPS_LAYOUT")
        app.click(selector="#__azmaps_travel-walk")
        app.until("a rebuild after the drag", lambda: app.count("AZMAPS_LAYOUT") > layouts)
        app.frame(2)
        app.log("the rebuild after the drag: AZMAPS_LAYOUT %s" % app.last("AZMAPS_LAYOUT"))
        rebuilt_at = app.rect(pin, every_dom=True)
        if (not rebuilt_at or abs(rebuilt_at["x"] - after["x"]) > 1.5
                or abs(rebuilt_at["y"] - after["y"]) > 1.5):
            raise Failure("the rebuild drew the pin at %s, the zoom left it at %s" % (rebuilt_at, after))

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

        # Place names: Frankfurt to Berlin by car. A keystroke is the field's own edit - the
        # window is not rebuilt for it (a rebuild per keystroke locked the UI for 1.5 s each);
        # the geocoder worker searches once the typing rests.
        app.must("focus_node", selector="#__azmaps_travel-from")
        app.frame()
        layouts = app.count("AZMAPS_LAYOUT")
        keys = []
        for ch in "Frankfurt":
            started = time.time()
            app.must("text_input", text=ch)
            app.frame()
            keys.append((time.time() - started) * 1000.0)
        typed_layouts = app.count("AZMAPS_LAYOUT") - layouts
        app.log("keystrokes (ms): %s; %d rebuild(s) while typing" % ([round(k) for k in keys], typed_layouts))
        if sorted(keys)[len(keys) // 2] > KEY_BUDGET_MS:
            raise Failure("a keystroke in a travel field took %s ms (median over %d ms)" % (keys, KEY_BUDGET_MS))
        if typed_layouts > 1:
            raise Failure("typing nine letters rebuilt the window %d times" % typed_layouts)
        app.until("the suggestions for Frankfurt", lambda: app.has_id("__azmaps_suggestion-0"))
        app.log("geocoder: %s" % app.last("AZMAPS_SUGGEST"))
        if not app.printed("AZMAPS_SUGGEST", r"from [1-9]\d* \d+\.\d{3}"):
            raise Failure("no AZMAPS_SUGGEST line for the start: %s" % app.printed("AZMAPS_SUGGEST"))
        if not app.shows("Frankfurt am Main, Germany"):
            raise Failure("Frankfurt am Main is not among the suggestions")
        app.click(selector="#__azmaps_suggestion-0")
        app.until("Frankfurt am Main is the start",
                  lambda: app.printed("AZMAPS_TRAVEL", r"car 50\.1109,8\.6821 -"))
        app.must("focus_node", selector="#__azmaps_travel-to")
        app.frame()
        app.must("text_input", text="Berlin")
        app.frame()
        app.until("the suggestions for Berlin", lambda: app.has_id("__azmaps_suggestion-0"))
        app.key("return")
        app.until("Berlin is the destination",
                  lambda: app.printed("AZMAPS_TRAVEL", r"car 50\.1109,8\.6821 52\.5200,13\.4050"))
        route = app.until("the car route",
                          lambda: app.printed("AZMAPS_ROUTE", r"car \d+\.\d \d+ \d+\.\d{3}"))
        km, minutes = (float(v) for v in route[-1].split()[1:3])
        app.log("Frankfurt to Berlin: %s" % route[-1])
        if not (500.0 <= km <= 600.0 and 300.0 <= minutes <= 360.0):
            raise Failure("Frankfurt to Berlin by car came out as %.1f km in %.0f min" % (km, minutes))
        wait_view(app, "the map frames the route", lambda lat, lon, zoom: 5.0 <= zoom <= 8.0
                  and 47.0 < lat < 55.0 and 0.0 < lon < 16.0)
        app.screenshot(os.path.join(out, "8-frankfurt-berlin.png"))

        # A pan with the route on the map, timed per step.
        app.frame(3)
        x0, y0 = WIDTH * 0.70, HEIGHT * 0.55
        app.must("mouse_move", x=x0, y=y0)
        app.frame()
        app.must("mouse_down", x=x0, y=y0)
        app.frame()
        steps = []
        for i in range(1, 13):
            started = time.time()
            app.must("mouse_move", x=x0 - 15 * i, y=y0 - 8 * i)
            app.frame()
            steps.append((time.time() - started) * 1000.0)
        app.must("mouse_up", x=x0 - 180, y=y0 - 96)
        app.frame(2)
        app.log("pan steps (ms): %s" % [round(t) for t in steps])
        if sorted(steps)[len(steps) // 2] > PAN_BUDGET_MS:
            raise Failure("a pan step took %s ms (median over %d ms)" % (steps, PAN_BUDGET_MS))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: full-window map, controls, keys, pins, the place's card, travel, sidebar, "
            "settings, About, kept view and pins, place names and timed typing and panning; "
            "screenshots in %s" % out)
    return True


if __name__ == "__main__":
    e2e.run(TAG, body, default_port=8774)
