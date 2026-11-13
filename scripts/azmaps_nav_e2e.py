#!/usr/bin/env python3
"""AzMaps as a navigator, end to end, headless: public transport, a guided drive, an NMEA
receiver, the cache without a network, a Google Takeout import.

    transit   Berlin Hbf to Leipzig Hbf by public transport: both ends found by name with the
              online search (each suggestion credits its source), the journeys of Transitous
              listed (AZMAPS_JOURNEYS, AZMAPS_LEG: a train among the legs), the picked one's legs
              shown and drawn, the provider credited with a link to its sources.
    simulate  a car route (FOSSGIS' Valhalla) navigated by a simulated car (--simulate-route,
              --navigate): the banner and the bar show, the prompts are said (AZMAPS_SAY, and
              azul's headless speech log AZ_SPEECH), the car takes its wrong turn, one new route
              is asked for and followed (AZMAPS_REROUTE, AZMAPS_REROUTED 1), the car arrives;
              no position update rebuilds the window (AZMAPS_LAYOUT between the AZMAPS_NAV
              lines, AZMAPS_STATS=1) but the one with a new maneuver: the map follows the car
              and the banner counts down in place.
    replay    the same kind of route driven by an NMEA replay (--gps replay:<fixture>
              --replay-speed 10): fixes (AZMAPS_GPS) with their quality, the guidance counting
              down, arrival.
    offline   a car route and a place search asked with the network, then asked again in a
              second run with --offline: the route comes from the cache, marked offline
              (AZMAPS_ROUTE_ORIGIN car offline), the places too (AZMAPS_SUGGEST_SOURCE to <n>
              offline).
    takeout   a synthetic Google Takeout folder imported (--import): its four lists in the
              sidebar, the place without coordinates looked up and waiting in Settings > Places'
              review.

Network: AzMaps itself asks Transitous (api.transitous.org: journeys, place search) and
FOSSGIS' Valhalla (valhalla1.openstreetmap.de) under its own User-Agent
`AzMaps/<version> (+https://github.com/fschutt/azul)`, sending the places, the time and the
typed text only. This script sends nothing. `takeout` needs no network (without it the
looked-up place waits in the review unfound).

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 600 --log /tmp/azmaps-nav.log -- \\
      python3 scripts/azmaps_nav_e2e.py --bin target/release/AzMaps [--only transit,simulate,...]

The debug-server client is the shared one (scripts/azlin_e2e.py).
"""

import os
import re
import sys

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azmaps-nav"
WIDTH, HEIGHT = 1100, 760
MAP = "__azmaps_map"
HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.join(HERE, "..", "examples", "azul-maps", "tests", "fixtures")
TAKEOUT = os.path.join(FIXTURES, "takeout")
NMEA = os.path.join(FIXTURES, "drive-berlin.nmea")
# Berlin Hbf (Europaplatz) to Alexanderplatz: the replay fixture's drive.
DRIVE_FROM = (52.5246, 13.3685)
DRIVE_TO = (52.5220, 13.4111)
SCENARIOS = ["takeout", "transit", "simulate", "replay", "offline"]


def start(args, logs, name, data_dir, extra, env=None):
    binary = e2e.find_binary("AzMaps", args.bin, "AZMAPS_BIN")
    app = e2e.App(name, binary,
                  ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--language", "en"]
                  + extra,
                  args.debug_port, logs, args.timeout, extra_env=env)
    app.until("AzMaps' window", lambda: app.has_id(MAP))
    app.frame(2)
    return app


def pick(app, field, text, end_key):
    """Types `text` into the travel field `field`, waits for the online suggestions and picks
    the first; the field's end becomes a place."""
    app.must("focus_node", selector="#__azmaps_travel-%s" % field)
    app.frame()
    app.must("text_input", text=text)
    app.frame()
    app.until("the suggestions for %s" % text, lambda: app.has_id("__azmaps_suggestion-0"))
    app.until("the online suggestions for %s" % text,
              lambda: app.shows("Transitous · © OpenStreetMap contributors"))
    before = app.count("AZMAPS_TRAVEL")
    app.click(selector="#__azmaps_suggestion-0")
    app.until("%s picked" % text, lambda: app.count("AZMAPS_TRAVEL") > before)
    return app.last("AZMAPS_TRAVEL")


def type_ends(app, frm, to):
    app.text_input("#__azmaps_travel-from", "%.4f, %.4f" % frm)
    app.text_input("#__azmaps_travel-to", "%.4f, %.4f" % to)


def transit(args, logs, out):
    app = start(args, logs, "transit", os.path.join(logs, "transit-data"), [])
    try:
        app.click(selector="#__azmaps_travel-transit")
        pick(app, "from", "Berlin Hbf", "from")
        travel = pick(app, "to", "Leipzig Hbf", "to")
        app.log("travel: %s" % travel)
        if not re.match(r"transit 52\.52\d*,13\.3\d* 51\.34\d*,12\.3\d*", travel or ""):
            raise Failure("Berlin Hbf to Leipzig Hbf did not become the ends: %s" % travel)
        app.screenshot(os.path.join(out, "transit-1-search-credits.png"))
        journeys = app.until("the journeys", lambda: app.printed("AZMAPS_JOURNEYS", r"[1-9]\d* .*"))
        app.log("journeys: %s; legs: %s" % (journeys[-1], app.printed("AZMAPS_LEG")))
        legs = app.printed("AZMAPS_LEG")
        if not any(re.match(r"(highspeed|longdistance|regional) ", leg) for leg in legs):
            raise Failure("no train among the first journey's legs: %s" % legs)
        for node in ["__azmaps_journeys", "__azmaps_journey-0", "__azmaps_journey-legs", "__azmaps_transit-credit"]:
            app.until(node, lambda node=node: app.has_id(node))
        if not app.shows("Transitous"):
            raise Failure("the journeys do not credit Transitous")
        app.screenshot(os.path.join(out, "transit-2-journeys.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


# What may rebuild the window while navigating: a new route asked for or followed, arrival.
NAV_EVENTS = ("AZMAPS_REROUTE", "AZMAPS_REROUTED", "AZMAPS_REROUTE_FAILED", "AZMAPS_ARRIVED")


def rebuilds_per_position(out_path):
    """The window rebuilds (AZMAPS_LAYOUT, printed with AZMAPS_STATS) after each position update
    (AZMAPS_NAV) up to the next one, for the updates that came with no navigation event: a list
    of (update number, rebuilds). The first update is left out, it follows the start's rebuild.
    A new maneuver is an event too: the banner's arrow is an icon node, drawn anew."""
    with open(out_path, "r", encoding="utf-8", errors="replace") as f:
        lines = f.read().splitlines()
    spans = []
    current = None
    step = None
    for line in lines:
        key = line.split(" ", 1)[0]
        if key == "AZMAPS_NAV":
            if current is not None:
                spans.append(current)
            fields = line.split()
            new_step = fields[1] if len(fields) > 1 else None
            current = {"n": len(spans) + 1, "layouts": 0, "event": new_step != step}
            step = new_step
        elif current is not None and key == "AZMAPS_LAYOUT":
            current["layouts"] += 1
        elif current is not None and key in NAV_EVENTS:
            current["event"] = True
    return [(s["n"], s["layouts"]) for s in spans[1:] if not s["event"]]


def simulate(args, logs, out):
    speech_log = os.path.join(logs, "speech.log")
    app = start(args, logs, "simulate", os.path.join(logs, "simulate-data"),
                ["--simulate-route", "--simulate-speed", "250", "--navigate"],
                env={"AZ_SPEECH_LOG": speech_log, "AZMAPS_STATS": "1"})
    try:
        type_ends(app, DRIVE_FROM, DRIVE_TO)
        started = app.until("the navigation", lambda: app.printed("AZMAPS_NAV_START", r"\d+ \d+ simulated .*"))
        app.log("navigation: %s" % started[-1])
        app.until("the banner", lambda: app.has_id("__azmaps_nav-banner") and app.has_id("__azmaps_nav-bar"))
        app.until("a distance in the banner", lambda: app.has_id("__azmaps_nav-distance"))
        app.screenshot(os.path.join(out, "simulate-1-banner.png"))
        app.until("a new route asked for after the wrong turn", lambda: app.printed("AZMAPS_REROUTE"))
        app.until("the new route followed", lambda: app.printed("AZMAPS_REROUTED", r"1 \d+ \w+"))
        app.screenshot(os.path.join(out, "simulate-2-rerouted.png"))
        app.until("arrival", lambda: app.printed("AZMAPS_ARRIVED"))
        said = app.printed("AZMAPS_SAY")
        app.log("said: %s" % said)
        if len(said) < 3 or "Recalculating the route." not in said or "You have arrived." not in said:
            raise Failure("the prompts were not said: %s" % said)
        spoken = open(speech_log, encoding="utf-8").read() if os.path.isfile(speech_log) else ""
        if "AZ_SPEECH en-US You have arrived." not in spoken:
            raise Failure("azul's speech did not log the prompts: %r" % spoken[:400])
        app.screenshot(os.path.join(out, "simulate-3-arrived.png"))
        # The map follows the car and the banner counts down in place: a position update
        # rebuilds nothing.
        updates = rebuilds_per_position(app.out_path)
        rebuilt = [(n, layouts) for n, layouts in updates if layouts > 0]
        app.log("position updates: %d, with a window rebuild: %s" % (len(updates), rebuilt))
        if len(updates) < 5:
            raise Failure("too few position updates to judge: %d" % len(updates))
        if rebuilt:
            raise Failure("position updates rebuilt the window: %s" % rebuilt)
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


def replay(args, logs, out):
    app = start(args, logs, "replay", os.path.join(logs, "replay-data"),
                ["--gps", "replay:%s" % os.path.abspath(NMEA), "--replay-speed", "10", "--navigate"])
    try:
        app.until("the receiver's fixes", lambda: app.printed("AZMAPS_GPS", r"\d+\.\d+,\d+\.\d+ 3D fix .*"))
        type_ends(app, DRIVE_FROM, DRIVE_TO)
        started = app.until("the navigation", lambda: app.printed("AZMAPS_NAV_START", r"\d+ \d+ replay .*"))
        app.log("navigation: %s" % started[-1])
        app.until("the indicator", lambda: app.has_id("__azmaps_gps-indicator"))
        app.screenshot(os.path.join(out, "replay-1-driving.png"))
        app.until("arrival", lambda: app.printed("AZMAPS_ARRIVED"))
        remaining = [float(line.split()[2]) for line in app.printed("AZMAPS_NAV")]
        if len(remaining) < 10 or remaining[-1] > remaining[0] / 4:
            raise Failure("the distance left did not count down: %s" % remaining[:5] + remaining[-5:])
        if app.printed("AZMAPS_REROUTED"):
            app.log("note: the replay left its route %d time(s)" % len(app.printed("AZMAPS_REROUTED")))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


def offline(args, logs, out):
    data_dir = os.path.join(logs, "offline-data")
    app = start(args, logs, "online", data_dir, [])
    try:
        type_ends(app, DRIVE_FROM, DRIVE_TO)
        app.until("the car route", lambda: app.printed("AZMAPS_ROUTE_ORIGIN", r"car online"))
        pick(app, "to", "Leipzig Hbf", "to")
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app = start(args, logs, "offline", data_dir, ["--offline"])
    try:
        type_ends(app, DRIVE_FROM, DRIVE_TO)
        app.until("the car route from the cache", lambda: app.printed("AZMAPS_ROUTE_ORIGIN", r"car offline"))
        app.until("the route on the panel", lambda: app.printed("AZMAPS_ROAD", r"car .*"))
        app.must("focus_node", selector="#__azmaps_travel-to")
        app.frame()
        app.must("text_input", text="Leipzig Hbf")
        app.frame()
        app.until("the places from the cache", lambda: app.printed("AZMAPS_SUGGEST_SOURCE", r"to [1-9]\d* offline"))
        app.screenshot(os.path.join(out, "offline-1-cached.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


def takeout(args, logs, out):
    app = start(args, logs, "takeout", os.path.join(logs, "takeout-data"),
                ["--import", os.path.abspath(TAKEOUT)])
    try:
        imported = app.until("the import", lambda: app.printed("AZMAPS_IMPORTED", r"\d+ \d+ \d+ \d+"))
        lists, places, placed, review = (int(v) for v in imported[-1].split())
        app.log("imported: %d lists, %d places, %d looked up and placed, %d to review"
                % (lists, places, placed, review))
        if lists != 4 or places != 8:
            raise Failure("the Takeout's four lists and eight places did not come in: %s" % imported[-1])
        if placed + review < 2:
            raise Failure("the two places without coordinates were neither placed nor reviewed")
        app.until("the saved lists in the sidebar", lambda: app.has_id("__azmaps_saved"))
        for name in ["Saved Places", "Labeled places", "Berlin sights", "Want to go"]:
            if not app.shows(name, every_dom=True) and not any(name in t for t in app.texts()):
                raise Failure("the list %s is not in the sidebar" % name)
        app.screenshot(os.path.join(out, "takeout-1-lists.png"))
        app.click(selector="#__azmaps_settings")
        app.frame(3)
        if review:
            app.click_exact("Places")
            app.until("the review", lambda: app.has_id("__azmaps_import-review"))
            app.screenshot(os.path.join(out, "takeout-2-review.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s" % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()


def main():
    only = None
    rest = []
    argv = iter(sys.argv[1:])
    for arg in argv:
        if arg == "--only":
            only = next(argv, "")
        elif arg.startswith("--only="):
            only = arg.split("=", 1)[1]
        else:
            rest.append(arg)
    chosen = [s for s in SCENARIOS if not only or s in only.split(",")]

    def body(args, logs, out):
        for name in chosen:
            print("[%s] ---- %s ----" % (TAG, name), flush=True)
            globals()[name](args, logs, out)
        print("[%s] PASS: %s; screenshots in %s" % (TAG, ", ".join(chosen), out), flush=True)
        return True

    e2e.run(TAG, body, argv=rest, default_port=8779)


if __name__ == "__main__":
    main()
