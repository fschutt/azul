#!/usr/bin/env python3
"""png_nonblank.py <file.png>: did the window of this screenshot draw anything? Exit 0 when it holds more than one flat colour,
1 when it is flat, 2 when the picture cannot be read.

The boot tests take a screenshot after the wait. A window that never drew is one colour (black, white, the system's default);
one that drew has text, shapes, edges. The system draws its own bars into every screenshot (the status bar with the clock, the
navigation bar or the home indicator), and those have edges and many colour values, so a picture counted whole is "drawn"
because of them: only the band between them counts, the rows from 12 to 88 percent of the height. In that band a flat window has
every sampled pixel the same; a drawn one has some of them, at least 0.05 percent, in another colour than the most common one
(a single line of text is more than that, a stray pixel is not).

Standard library only; the decoder is scripts/ci/png_compare.py's.
"""
import sys

import png_compare

BAND = (0.12, 0.88)
DRAWN_FRACTION = 0.0005


def drawn_fraction(decoded):
    """The share of the band's sampled pixels that are NOT the band's most common colour: 0.0 for a flat window."""
    width, height, bpp, rows = decoded
    channels = min(bpp, 3)      # the alpha channel says nothing about what was drawn
    counts, total = {}, 0
    for y in range(int(height * BAND[0]), int(height * BAND[1]), 2):
        row = rows[y]
        for x in range(0, width * bpp, 2 * bpp):
            key = bytes(row[x:x + channels])
            counts[key] = counts.get(key, 0) + 1
            total += 1
    return 1.0 - max(counts.values()) / total if total else 0.0


def main(argv):
    try:
        decoded = png_compare.decode(argv[0])
    except IndexError:
        print(__doc__, file=sys.stderr)
        return 2
    except Exception as e:  # noqa: BLE001  (a file that is not there, torn off, or a PNG of a kind the decoder does not read)
        print(f"png_nonblank: {argv[0]}: {e}", file=sys.stderr)
        return 2
    if decoded is None:
        print(f"png_nonblank: {argv[0]}: not an 8-bit, non-interlaced PNG", file=sys.stderr)
        return 2
    fraction = drawn_fraction(decoded)
    print(f"{argv[0]}: {fraction * 100:.2f}% of the window's sampled pixels are not its most common colour")
    return 0 if fraction >= DRAWN_FRACTION else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
