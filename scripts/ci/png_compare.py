#!/usr/bin/env python3
"""png_compare.py <before.png> <after.png>: how much of the picture is the same? Prints the fraction of the sampled pixels
that are equal (0.0 .. 1.0) and exits 0 when the pictures DIFFER (the app is on the screen), 1 when they are (nearly) the same
(the launcher or the home screen is still there), 2 when a file cannot be read.

The boot tests take a screenshot of the home screen BEFORE they start an app and one after the wait. An app that crashed, was
killed or never came to the front leaves the same home screen behind: its picture is what a "not black" test calls fine
(that test said the iOS home screen of 2026-11-26 was a running app). The clock of the status bar changes between two shots,
so "nearly the same" is 98.5 percent of the sampled pixels.

Standard library only: a minimal decoder of 8-bit, non-interlaced greyscale/RGB/RGBA PNGs (what screencap and simctl write).
Every 4th row and every 4th pixel is compared; the rows are still all unfiltered, which costs a few seconds for a phone screen.
"""
import struct
import sys
import zlib

SAME = 0.985


def decode(path):
    """-> (width, height, bytes per pixel, rows) with `rows` the unfiltered rows (bytes), or None."""
    data = open(path, "rb").read()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return None
    pos, idat, ihdr = 8, [], None
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + length]
        if kind == b"IHDR":
            ihdr = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat.append(body)
        elif kind == b"IEND":
            break
        pos += 12 + length
    if ihdr is None:
        return None
    width, height, depth, ctype, _, _, interlace = ihdr
    if depth != 8 or interlace != 0 or ctype not in (0, 2, 6):
        return None
    bpp = {0: 1, 2: 3, 6: 4}[ctype]
    raw = zlib.decompress(b"".join(idat))
    stride = width * bpp
    rows, prev = [], bytearray(stride)
    pos = 0
    for _ in range(height):
        ftype = raw[pos]
        cur = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += 1 + stride
        if ftype == 1:      # Sub
            for i in range(bpp, stride):
                cur[i] = (cur[i] + cur[i - bpp]) & 255
        elif ftype == 2:    # Up
            cur = bytearray((c + p) & 255 for c, p in zip(cur, prev))
        elif ftype == 3:    # Average
            for i in range(stride):
                left = cur[i - bpp] if i >= bpp else 0
                cur[i] = (cur[i] + ((left + prev[i]) >> 1)) & 255
        elif ftype == 4:    # Paeth
            for i in range(stride):
                a = cur[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                pred = a if (pa <= pb and pa <= pc) else (b if pb <= pc else c)
                cur[i] = (cur[i] + pred) & 255
        rows.append(cur)
        prev = cur
    return width, height, bpp, rows


def same_fraction(a, b):
    if a is None or b is None or a[:3] != b[:3]:
        return 0.0
    width, height, bpp, ra = a
    rb = b[3]
    equal = total = 0
    for y in range(0, height, 4):
        x_a, x_b = ra[y], rb[y]
        for x in range(0, width * bpp, 4 * bpp):
            total += 1
            if x_a[x:x + bpp] == x_b[x:x + bpp]:
                equal += 1
    return equal / total if total else 0.0


def main(argv):
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    try:
        fraction = same_fraction(decode(argv[0]), decode(argv[1]))
    except (OSError, zlib.error, ValueError) as e:
        print(f"png_compare: {e}", file=sys.stderr)
        return 2
    print(f"{fraction:.4f} of the sampled pixels are the same")
    return 1 if fraction >= SAME else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
