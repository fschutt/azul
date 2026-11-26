#!/usr/bin/env python3
"""demo_icon.py: a launcher icon for a demo app, drawn here (the apps have none of their own).

    demo_icon.py android <AppName> <res-dir>     <res-dir>/mipmap-<density>/ic_launcher.png, 48..192 px
    demo_icon.py ios <AppName> <app-dir>         <app-dir>/AppIcon60x60@2x.png (120), @3x (180), AppIcon76x76@2x.png (152),
                                                 AppIcon83.5x83.5@2x.png (167)

A rounded square in a colour taken from the app's name (so the apps of one phone differ) with a white "A" of the Az family
drawn as strokes: no font, no image library, only the standard library (a PNG is a zlib stream and three checksums).
The icons are meant to be replaced by an app's own; a manifest field for that belongs to the day an app has one.
"""
import colorsys
import hashlib
import math
import pathlib
import struct
import sys
import zlib

ANDROID = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
IOS = {"AppIcon60x60@2x.png": 120, "AppIcon60x60@3x.png": 180, "AppIcon76x76@2x.png": 152, "AppIcon83.5x83.5@2x.png": 167}
FIXED = {"AzCompanion": (0.585, 0.78, 0.69)}   # the phone remote's own blue (hue, saturation, value)


def colour(name):
    h, s, v = FIXED.get(name) or (hashlib.sha256(name.encode()).digest()[0] / 255.0, 0.72, 0.62)
    return colorsys.hsv_to_rgb(h, s, v)


def seg_distance(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    return math.hypot(px - (ax + t * dx), py - (ay + t * dy))


# the Az family's letter: two legs and a bar, in the unit square
STROKES = [((0.26, 0.78), (0.50, 0.22)), ((0.50, 0.22), (0.74, 0.78)), ((0.35, 0.60), (0.65, 0.60))]
HALF_WIDTH = 0.045


def pixel(x, y, rgb, rounded):
    """RGBA of the point (x, y) in the unit square."""
    if rounded:
        # a rounded square: the corner circles of radius 0.22
        cx = min(max(x, 0.22), 0.78)
        cy = min(max(y, 0.22), 0.78)
        if math.hypot(x - cx, y - cy) > 0.22:
            return (0, 0, 0, 0)
    top = 1.0 - 0.28 * y   # a little lighter at the top: a gradient, so the square is not a flat swatch
    r, g, b = (min(1.0, c * top + 0.12 * (1 - y)) for c in rgb)
    if any(seg_distance(x, y, *a, *b2) <= HALF_WIDTH for a, b2 in STROKES):
        return (255, 255, 255, 255)
    return (int(r * 255), int(g * 255), int(b * 255), 255)


def render(name, size, rounded):
    """RGBA rows from the top, 3x3 supersampled (colours weighted by their alpha, alpha averaged)."""
    rgb = colour(name)
    sub = [(i + 0.5) / 3 for i in range(3)]
    rows = []
    for py in range(size):
        row = bytearray()
        for px in range(size):
            premul = [0.0, 0.0, 0.0]
            alpha = 0.0
            for sy in sub:
                for sx in sub:
                    r, g, b, a = pixel((px + sx) / size, (py + sy) / size, rgb, rounded)
                    premul[0] += r * a
                    premul[1] += g * a
                    premul[2] += b * a
                    alpha += a
            if alpha == 0:
                row += b"\x00\x00\x00\x00"
            else:
                row += bytes(int(c / alpha) for c in premul) + bytes((int(alpha / 9),))
        rows.append(bytes(row))
    return rows


def png(rows, size, rgba=True):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    if rgba:
        raw = b"".join(b"\x00" + r for r in rows)
        colour_type = 6
    else:
        raw = b"".join(b"\x00" + b"".join(r[i:i + 3] for i in range(0, len(r), 4)) for r in rows)
        colour_type = 2
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, colour_type, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def main(argv):
    if len(argv) != 3 or argv[0] not in ("android", "ios"):
        print(__doc__, file=sys.stderr)
        return 2
    kind, name, out = argv
    out = pathlib.Path(out)
    if kind == "android":
        for density, size in ANDROID.items():
            d = out / f"mipmap-{density}"
            d.mkdir(parents=True, exist_ok=True)
            (d / "ic_launcher.png").write_bytes(png(render(name, size, True), size, rgba=True))
    else:
        out.mkdir(parents=True, exist_ok=True)
        for file, size in IOS.items():
            (out / file).write_bytes(png(render(name, size, False), size, rgba=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
