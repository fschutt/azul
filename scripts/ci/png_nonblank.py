#!/usr/bin/env python3
"""png_nonblank.py <file.png>: exit 0 when the picture holds more than a flat colour, 1 when it is flat or no PNG.

The boot tests of the phone apps take a screenshot after the wait: a window that never drew is black (or one colour), a
window that drew has text, shapes, edges. The PNG's filtered rows of a flat picture are all zero bytes (Sub/Up of
equal pixels), so the number of DISTINCT bytes in a sample of the inflated image tells the two apart without decoding
the picture (standard library only; the files are ~10 MB inflated and zlib does that in milliseconds).
"""
import struct
import sys
import zlib

DISTINCT_FOR_A_PICTURE = 12


def distinct_bytes(data):
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        return 0
    pos, idat = 8, []
    while pos + 8 <= len(data):
        length, kind = struct.unpack(">I4s", data[pos:pos + 8])
        if kind == b"IDAT":
            idat.append(data[pos + 8:pos + 8 + length])
        if kind == b"IEND":
            break
        pos += 12 + length
    try:
        raw = zlib.decompress(b"".join(idat))
    except zlib.error:
        return 0
    return len(set(raw[::7]))


def main(argv):
    try:
        data = open(argv[0], "rb").read()
    except (IndexError, OSError):
        return 1
    n = distinct_bytes(data)
    print(f"{argv[0]}: {n} distinct byte values in the sample")
    return 0 if n >= DISTINCT_FOR_A_PICTURE else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
