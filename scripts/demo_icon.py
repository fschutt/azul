#!/usr/bin/env python3
"""demo_icon.py: a launcher icon for a demo app, drawn here (the apps have none of their own).

    demo_icon.py android <AppName> <res-dir>     <res-dir>/mipmap-<density>/ic_launcher.png, 48..192 px
    demo_icon.py ios <AppName> <app-dir>         <app-dir>/AppIcon60x60@2x.png (120), @3x (180), AppIcon76x76@2x.png (152),
                                                 AppIcon83.5x83.5@2x.png (167)

A rounded square in a colour taken from the app's name (so the apps of one phone differ) with the first letters of the name in
white: "WI" for AzWidgets, "IM" for AzInstallManager (the first letters of its words), "RC" for AzReview Companion. The letters are
a 5 by 7 dot font of this file (A to Z, 0 to 9): no font file, no image library, no art of anybody's, only the standard library
(a PNG is a zlib stream and three checksums). The icons are meant to be replaced by an app's own; a manifest field for that
belongs to the day an app has one.
"""
import colorsys
import hashlib
import math
import pathlib
import re
import struct
import sys
import zlib

ANDROID = {"mdpi": 48, "hdpi": 72, "xhdpi": 96, "xxhdpi": 144, "xxxhdpi": 192}
IOS = {"AppIcon60x60@2x.png": 120, "AppIcon60x60@3x.png": 180, "AppIcon76x76@2x.png": 152, "AppIcon83.5x83.5@2x.png": 167}
FIXED = {"AzCompanion": (0.585, 0.78, 0.69), "AzReview Companion": (0.585, 0.78, 0.69)}   # the phone remote's own blue (hue, saturation, value)


def colour(name):
    h, s, v = FIXED.get(name) or (hashlib.sha256(name.encode()).digest()[0] / 255.0, 0.72, 0.62)
    return colorsys.hsv_to_rgb(h, s, v)


# the letters of an app whose name does not give them: the phone remote of AzReview
LETTERS = {"AzCompanion": "RC", "AzReview Companion": "RC"}

# a dot font: 5 columns by 7 rows, '#' is a lit dot
FONT = {
    "A": [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
    "B": ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####."],
    "C": [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###."],
    "D": ["####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####."],
    "E": ["#####", "#....", "#....", "####.", "#....", "#....", "#####"],
    "F": ["#####", "#....", "#....", "####.", "#....", "#....", "#...."],
    "G": [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####"],
    "H": ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#"],
    "I": [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###."],
    "J": ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."],
    "K": ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#"],
    "L": ["#....", "#....", "#....", "#....", "#....", "#....", "#####"],
    "M": ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#"],
    "N": ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#"],
    "O": [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "P": ["####.", "#...#", "#...#", "####.", "#....", "#....", "#...."],
    "Q": [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#"],
    "R": ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#"],
    "S": [".####", "#....", "#....", ".###.", "....#", "....#", "####."],
    "T": ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#.."],
    "U": ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###."],
    "V": ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#.."],
    "W": ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#"],
    "X": ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#"],
    "Y": ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#.."],
    "Z": ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####"],
    "0": [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###."],
    "1": ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###."],
    "2": [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####"],
    "3": ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###."],
    "4": ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#."],
    "5": ["#####", "#....", "####.", "....#", "....#", "#...#", ".###."],
    "6": ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###."],
    "7": ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#..."],
    "8": [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###."],
    "9": [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##.."],
    "?": [".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#.."],
}
CELL = 0.062   # one dot of the font, in the unit square of the icon: two letters are 0.68 wide, 0.43 high
GAP = 0.06     # of a dot, on every side: a hint of a grid at large sizes, nothing at small ones


def letters(name):
    """The letters of the icon of `name`: after the family's "Az" the first two ("WI" for AzWidgets, "PD" for AzPdf), or the first
    letters of its words when it has two ("IM" for AzInstallManager, "RC" for AzReview Companion)."""
    if name in LETTERS:
        return LETTERS[name]
    base = name[2:] if name.startswith("Az") and len(name) > 2 else name
    base = "".join(c for c in base if c.isascii() and c.isalnum())
    humps = re.findall(r"[A-Z][a-z0-9]*|[0-9]+|[a-z]+", base)
    if len(humps) >= 2 and not base.isupper():
        text = humps[0][0] + humps[1][0]
    else:
        text = base[:2]
    return text.upper() or "?"


def text_box(text):
    """x0, y0, x1, y1 of the letters in the unit square, centred."""
    w, h = (6 * len(text) - 1) * CELL, 7 * CELL
    return 0.5 - w / 2, 0.5 - h / 2, 0.5 + w / 2, 0.5 + h / 2


def cell_centre(text, letter_index, col, row):
    x0, y0, _, _ = text_box(text)
    return x0 + (letter_index * 6 + col + 0.5) * CELL, y0 + (row + 0.5) * CELL


def lit(text, x, y):
    """Is the point (x, y) of the unit square inside a lit dot of the letters?"""
    x0, y0, x1, y1 = text_box(text)
    if not (x0 <= x < x1 and y0 <= y < y1):
        return False
    cx, cy = (x - x0) / CELL, (y - y0) / CELL
    col, row = int(cx), int(cy)
    letter, c = divmod(col, 6)
    if c == 5:                       # the space between two letters
        return False
    if not (GAP <= cx - col < 1 - GAP and GAP <= cy - row < 1 - GAP):
        return False
    return FONT[text[letter]][row][c] == "#"


def pixel(x, y, rgb, rounded, text):
    """RGBA of the point (x, y) in the unit square."""
    if rounded:
        # a rounded square: the corner circles of radius 0.22
        cx = min(max(x, 0.22), 0.78)
        cy = min(max(y, 0.22), 0.78)
        if math.hypot(x - cx, y - cy) > 0.22:
            return (0, 0, 0, 0)
    top = 1.0 - 0.28 * y   # a little lighter at the top: a gradient, so the square is not a flat swatch
    r, g, b = (min(1.0, c * top + 0.12 * (1 - y)) for c in rgb)
    if lit(text, x, y):
        return (255, 255, 255, 255)
    return (int(r * 255), int(g * 255), int(b * 255), 255)


def render(name, size, rounded):
    """RGBA rows from the top, 3x3 supersampled (colours weighted by their alpha, alpha averaged)."""
    rgb = colour(name)
    text = letters(name)
    sub = [(i + 0.5) / 3 for i in range(3)]
    rows = []
    for py in range(size):
        row = bytearray()
        for px in range(size):
            premul = [0.0, 0.0, 0.0]
            alpha = 0.0
            for sy in sub:
                for sx in sub:
                    r, g, b, a = pixel((px + sx) / size, (py + sy) / size, rgb, rounded, text)
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
