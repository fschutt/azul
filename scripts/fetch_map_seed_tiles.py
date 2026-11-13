#!/usr/bin/env python3
"""Fetch the world tiles AzMaps ships in its binary.

AzMaps embeds the vector tiles of zoom 0 and 1 (five tiles, about half a
megabyte) with include_bytes!, so its map shows the world before the first
download and on a first start without a network. The bytes are the same for
the light and the dark look: a vector tile carries geometry, the look is
applied when it is drawn.

The tiles come from the URL template of the map widget's default layer
(MapTileLayer::default in layout/src/widgets/map.rs), read from that file so
the seed always matches the planet version the widget asks for. Run it again
when that version changes.

    python3 scripts/fetch_map_seed_tiles.py            # writes examples/azul-maps/assets/seed/
    python3 scripts/fetch_map_seed_tiles.py --check    # only checks what is there

The data is OpenStreetMap's (ODbL) in the OpenMapTiles schema (CC BY 4.0),
served by OpenFreeMap; ATTRIBUTION.txt next to the tiles says so, and AzMaps
shows the layer's attribution line as for every downloaded tile.
"""

import argparse
import gzip
import os
import re
import sys
import urllib.request

# A neutral User-Agent: no personal data in any request.
USER_AGENT = "azul-supply-chain-gate (+https://github.com/fschutt/azul)"

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WIDGET = os.path.join(ROOT, "layout", "src", "widgets", "map.rs")
OUT = os.path.join(ROOT, "examples", "azul-maps", "assets", "seed")

# The tiles AzMaps embeds, as (z, x, y); examples/azul-maps/src/lib.rs names
# the same five files.
TILES = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (1, 0, 1), (1, 1, 1)]

ATTRIBUTION = """\
These vector tiles (zoom 0 and 1, {template}) are embedded in AzMaps so its
map shows the world before any download.

Data: (c) OpenStreetMap contributors, Open Database License (ODbL 1.0),
https://www.openstreetmap.org/copyright
Schema and tiles: (c) OpenMapTiles (CC BY 4.0), https://openmaptiles.org
Served by OpenFreeMap, https://openfreemap.org

AzMaps shows the attribution line of its tile layer over the map.
"""


def default_template():
    """The URL template of MapTileLayer::default, read from the widget's source."""
    with open(WIDGET, "r", encoding="utf-8") as f:
        source = f.read()
    found = re.search(r'"(https://tiles\.openfreemap\.org/[^"]*\{z\}/\{x\}/\{y\}\.pbf)"', source)
    if not found:
        sys.exit("no OpenFreeMap URL template found in " + WIDGET)
    return found.group(1)


def file_name(z, x, y):
    return "%d-%d-%d.pbf" % (z, x, y)


def looks_like_mvt(data):
    """A vector tile is a protobuf whose fields are layers (field 3, length-delimited)."""
    return len(data) > 2 and data[0] == 0x1A


def fetch(url):
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(request, timeout=30) as response:
        if response.status != 200:
            sys.exit("%s: HTTP %d" % (url, response.status))
        data = response.read()
    # A server may send the tile gzipped whatever was asked; the map wants it plain.
    if data[:2] == b"\x1f\x8b":
        data = gzip.decompress(data)
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="only check the files")
    parser.add_argument("--template", help="another URL template with {z}/{x}/{y}")
    args = parser.parse_args()

    template = args.template or default_template()
    total = 0
    if not args.check:
        os.makedirs(OUT, exist_ok=True)
    for z, x, y in TILES:
        path = os.path.join(OUT, file_name(z, x, y))
        if args.check:
            if not os.path.exists(path):
                sys.exit("missing: " + path)
            with open(path, "rb") as f:
                data = f.read()
        else:
            url = template.replace("{z}", str(z)).replace("{x}", str(x)).replace("{y}", str(y))
            data = fetch(url)
            with open(path, "wb") as f:
                f.write(data)
        if not looks_like_mvt(data):
            sys.exit("not a vector tile: " + path)
        total += len(data)
        print("%s %d bytes" % (path, len(data)))
    if not args.check:
        with open(os.path.join(OUT, "ATTRIBUTION.txt"), "w", encoding="utf-8") as f:
            f.write(ATTRIBUTION.format(template=template))
    print("%d tiles, %d bytes in all" % (len(TILES), total))


if __name__ == "__main__":
    main()
