#!/usr/bin/env python3
"""AzMaps' road routing end to end, on real OpenStreetMap data and a plain file server.

    1. an OpenStreetMap extract: `--extract <file.osm.pbf>`, or with `--download` Geofabrik's
       Monaco extract (about half a megabyte), kept in `--cache` for the next run;
    2. azul-routing's `routing-tiles` cuts it into the routing tiles (zoom 8 / 11 / 14,
       `tiles/<z>/<x>/<y>.pbf` next to a `manifest.json`), and `routing-tiles --route` checks a
       route on the folder;
    3. a local Python file server serves the folder - static hosting, nothing else (a missing
       tile is a 404, a tile without roads);
    4. AzMaps starts headless with `--routing http://127.0.0.1:<port>/manifest.json` on a
       fresh data folder; a start and a destination are typed into the travel panel;
    5. the route worker fetches the corridor's tiles off the UI thread, reporting its progress
       (`AZMAPS_ROUTE_PROGRESS`), and answers a road route (`AZMAPS_ROAD car <km> <min>
       <points> <tiles> <attempts>`): longer than the straight line, shorter than three times
       it, along many road points; the map draws it (a screenshot);
    6. walking the same way is another road route; driving again costs no download (the
       session's cache);
    7. Settings > Navigation > Offline areas keeps the area on the map (Monaco, the view the
       data folder's settings open at) under a name: the hosting's tile list plans it, its
       tiles are downloaded and pinned (`AZMAPS_OFFLINE_AREA <tiles> <bytes> <failed> <name>`)
       and it is listed;
    8. a second start with the file server gone routes from the tiles cached in the data folder
       and still lists the offline area.

Network: only step 1 with `--download`, one GET to download.geofabrik.de naming
`azul-supply-chain-gate (+https://github.com/fschutt/azul)` and nothing else. Run ONE app at a
time, through the capped runner:

    scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 300 --log /tmp/azmaps-routing.log -- \\
      python3 scripts/azmaps_routing_e2e.py --bin target/release/AzMaps \\
        --routing-tiles-bin target/release/routing-tiles --download

The debug-server client is the shared one (scripts/azlin_e2e.py).
"""

import functools
import http.server
import math
import os
import re
import subprocess
import sys
import threading
import urllib.request

import azlin_e2e as e2e
from azlin_e2e import Failure

TAG = "azmaps-routing"
WIDTH, HEIGHT = 1100, 720
MAP = "__azmaps_map"
EXTRACT_URL = "https://download.geofabrik.de/europe/monaco-latest.osm.pbf"
# What the download names itself: the project, nothing about whoever runs it.
USER_AGENT = "azul-supply-chain-gate (+https://github.com/fschutt/azul)"
# Fontvieille's harbour to the casino of Monte-Carlo, about 1.7 km as the crow flies.
FROM = (43.7302, 7.4187)
TO = (43.7394, 7.4280)


def crow_km(a, b):
    """The great-circle distance in km."""
    lat1, lat2 = math.radians(a[0]), math.radians(b[0])
    dlat, dlon = lat2 - lat1, math.radians(b[1] - a[1])
    h = math.sin(dlat / 2) ** 2 + math.cos(lat1) * math.cos(lat2) * math.sin(dlon / 2) ** 2
    return 2 * 6371.0 * math.asin(min(1.0, math.sqrt(h)))


def the_extract(args):
    """The extract to cut: the one named, or Monaco's, downloaded once into the cache."""
    if args.extract:
        if not os.path.isfile(args.extract):
            raise Failure("no extract at %s" % args.extract)
        return args.extract
    path = os.path.join(args.cache, "monaco-latest.osm.pbf")
    if os.path.isfile(path) and os.path.getsize(path) > 0:
        return path
    if not args.download:
        raise Failure("no extract: pass --extract <file.osm.pbf>, or --download to fetch %s"
                      % EXTRACT_URL)
    os.makedirs(args.cache, exist_ok=True)
    request = urllib.request.Request(EXTRACT_URL, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=120) as response:
        data = response.read()
    if not data.startswith(b"\0\0"):
        raise Failure("the download is not an extract (%d bytes)" % len(data))
    with open(path + ".part", "wb") as f:
        f.write(data)
    os.replace(path + ".part", path)
    return path


def run_tool(binary, argv):
    """`routing-tiles argv`: its stdout, or a Failure with its stderr."""
    done = subprocess.run([binary] + argv, capture_output=True, text=True, timeout=600)
    if done.returncode != 0:
        raise Failure("routing-tiles %s failed (%d): %s%s"
                      % (" ".join(argv), done.returncode, done.stdout, done.stderr))
    return done.stdout


class Tiles:
    """A plain file server over the tiles folder, counting what it was asked for."""

    def __init__(self, root):
        self.requests = []
        tiles = self

        class Handler(http.server.SimpleHTTPRequestHandler):
            def log_message(self, fmt, *a):
                tiles.requests.append(self.path)

        handler = functools.partial(Handler, directory=root)
        self.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def template(self):
        return "http://127.0.0.1:%d/manifest.json" % self.port

    def stop(self):
        self.server.shutdown()
        self.server.server_close()


def road(app, mode):
    """The road route lines for `mode` as (km, minutes, points, tiles, attempts)."""
    out = []
    # `<mode> <km> <min> <points> <tiles> <attempts> <source> <steps>`.
    for value in app.printed("AZMAPS_ROAD", r"%s \d+\.\d \d+ \d+ \d+ \d+ \w+ \d+" % mode):
        _, km, minutes, points, tiles, attempts = value.split()[:6]
        out.append((float(km), float(minutes), int(points), int(tiles), int(attempts)))
    return out


def ask(app, frm, to):
    """Types the travel panel's ends as coordinates."""
    app.text_input("#__azmaps_travel-from", "%.4f, %.4f" % frm)
    app.text_input("#__azmaps_travel-to", "%.4f, %.4f" % to)


def body(args, logs, out):
    binary = e2e.find_binary("AzMaps", args.bin, "AZMAPS_BIN")
    tool = e2e.find_binary("routing-tiles", args.routing_tiles_bin, "ROUTING_TILES_BIN")
    extract = the_extract(args)

    # The tiles, cut from the extract, and a route on the folder without the app.
    folder = os.path.join(logs, "routing")
    cut = run_tool(tool, [extract, folder])
    print("[%s] %s" % (TAG, cut.strip()), flush=True)
    if not re.search(r"TILES \d+ ways \d+ nodes z8=\d+ z11=\d+ z14=\d+", cut):
        raise Failure("routing-tiles wrote no tiles: %s" % cut)
    check = run_tool(tool, ["--route", folder, "%f,%f" % FROM, "%f,%f" % TO, "car"])
    print("[%s] %s" % (TAG, check.strip()), flush=True)

    tiles = Tiles(folder)
    data_dir = os.path.join(logs, "data")
    os.makedirs(os.path.join(data_dir, "maps"), exist_ok=True)
    # The map opens over Monaco: the offline area is the view on the map.
    with open(os.path.join(data_dir, "maps", "settings.json"), "w", encoding="utf-8") as f:
        f.write('{"values": {"view": "%f,%f,14"}}' % ((FROM[0] + TO[0]) / 2, (FROM[1] + TO[1]) / 2))

    def start(name, template):
        app = e2e.App(name, binary,
                      ["--data-dir", data_dir, "--size", "%dx%d" % (WIDTH, HEIGHT), "--stats",
                       "--language", "en", "--routing", template, "--transit", "off"],
                      args.debug_port, logs, args.timeout)
        app.until("AzMaps' window", lambda: app.has_id(MAP))
        app.expect_line("AZMAPS_ROUTING", template, "the routing tiles named")
        app.frame(2)
        return app

    crow = crow_km(FROM, TO)
    app = start("first", tiles.template())
    try:
        # Both ends typed in: the route worker fetches the corridor and routes on the roads.
        ask(app, FROM, TO)
        found = app.until("the road route by car", lambda: road(app, "car"))
        km, minutes, points, used, attempts = found[-1]
        app.log("by car: %.1f km, %.0f min, %d points, %d tiles, %d attempt(s); %d tile "
                "request(s) to the file server; progress lines %d"
                % (km, minutes, points, used, attempts, len(tiles.requests),
                   app.count("AZMAPS_ROUTE_PROGRESS")))
        if not crow <= km <= crow * 3.0:
            raise Failure("the car route is %.1f km for %.1f km as the crow flies" % (km, crow))
        if points < 10:
            raise Failure("the car route has %d points: it does not follow the roads" % points)
        if not tiles.requests:
            raise Failure("the file server was asked for no tile")
        if app.count("AZMAPS_ROUTE_PROGRESS") < 1:
            raise Failure("the route worker reported no progress")
        if not app.printed("AZMAPS_ROUTE", r"car %.1f \d+ \d+\.\d{3}" % km):
            raise Failure("AZMAPS_ROUTE does not carry the road route's length")
        app.frame(3)
        app.screenshot(os.path.join(out, "1-road-route.png"))

        # Walking the same way: another road route.
        app.click(selector="#__azmaps_travel-walk")
        walk = app.until("the road route on foot", lambda: road(app, "walk"))
        app.log("on foot: %s" % (walk[-1],))
        if walk[-1][1] <= minutes:
            raise Failure("walking (%s min) is no slower than driving (%s min)" % (walk[-1][1], minutes))

        # Driving again: the session's cache has every tile.
        asked = len(tiles.requests)
        cars = len(road(app, "car"))
        app.click(selector="#__azmaps_travel-car")
        app.until("the car route again", lambda: len(road(app, "car")) > cars)
        if len(tiles.requests) != asked:
            raise Failure("the second car route downloaded %d tile(s) again"
                          % (len(tiles.requests) - asked))

        # The area on the map kept for offline use.
        app.click(selector="#__azmaps_settings")
        app.frame(3)
        app.click_exact("Navigation")
        app.until("the offline areas", lambda: app.has_id("__azmaps_offline-name"))
        app.text_input("#__azmaps_offline-name", "Monaco")
        app.click(selector="#__azmaps_offline-download")
        kept = app.until("the area kept",
                         lambda: app.printed("AZMAPS_OFFLINE_AREA", r"\d+ \d+ \d+ Monaco"))
        count, size, failed = (int(v) for v in kept[-1].split()[:3])
        app.log("offline area: %d tiles, %d bytes, %d failed" % (count, size, failed))
        if count < 3 or size <= 0 or failed:
            raise Failure("the offline area kept %d tiles of %d bytes, %d failed" % (count, size, failed))
        app.until("the area listed", lambda: app.has_id("__azmaps_offline-area-0"))
        areas = os.path.join(data_dir, "maps", "routing")
        area_files = [os.path.join(root, f) for root, _dirs, files in os.walk(areas) for f in files
                      if os.path.basename(root) == "areas" and f.endswith(".json")]
        if not area_files:
            raise Failure("no offline area file under %s" % areas)
        app.frame(2)
        app.screenshot(os.path.join(out, "2-offline-area.png"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s"
              % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()

    # The data folder keeps the tiles: with the file server gone, a new start still routes.
    template = tiles.template()
    tiles.stop()
    cached = os.path.join(data_dir, "maps", "routing")
    if not os.path.isdir(cached):
        raise Failure("no routing tiles cached under %s" % cached)
    app = start("second", template)
    try:
        cars = len(road(app, "car"))
        ask(app, FROM, TO)
        app.until("the car route from the cache", lambda: len(road(app, "car")) > cars)
        app.log("from the cache: %s" % (road(app, "car")[-1],))
        app.until("the offline area at the start", lambda: app.printed("AZMAPS_OFFLINE_AREAS", r"1 .*"))
    except Failure:
        print("---- stdout ----\n%s---- stderr ----\n%s"
              % (e2e.tail(app.out_path), e2e.tail(app.err_path)))
        raise
    finally:
        app.stop()
    app.log("PASS: an OpenStreetMap extract cut into routing tiles, served as plain files, routed "
            "by car and on foot off the UI thread with progress, cached for the session and in "
            "the data folder; screenshots in %s" % out)
    return True


def main():
    # The shared runner parses the common switches; these are this script's own.
    own = {"--extract": None, "--cache": os.path.join("/tmp", "azmaps-routing-cache"),
           "--routing-tiles-bin": None}
    flags = {"--download": False}
    rest = []
    argv = iter(sys.argv[1:])
    for arg in argv:
        if arg in own:
            own[arg] = next(argv, None)
        elif arg in flags:
            flags[arg] = True
        else:
            rest.append(arg)

    def with_own(args, logs, out):
        args.extract = own["--extract"]
        args.cache = own["--cache"]
        args.routing_tiles_bin = own["--routing-tiles-bin"]
        args.download = flags["--download"]
        return body(args, logs, out)

    e2e.run(TAG, with_own, argv=rest, default_port=8775)


if __name__ == "__main__":
    main()
