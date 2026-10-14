#!/usr/bin/env python3
"""Pack the Calmet UI faces azul bundles (the typeface of AzDrive's design).

Calmet UI is traced from the 1732 print of Augustin Calmet's "Commentaire
litteral" and made fit for a UI (composed umlauts, the long s mapped, a few
re-spaced punctuation marks, liga / dlig / ss01 / tnum). The two TrueType faces
are the design's own build; this script only checks they are the pinned build
(SHA-256 below) and brotli-compresses them at quality 11, like the EB Garamond
faces next to them.

It also prints each face's cmap as the range table `layout/src/text3/ui_fonts.rs`
registers the face with, so a new build of the faces comes with its exact
coverage (a stale table sends a letter the face has to the next family, or
claims one it lacks).

Needs fontTools and brotli (pip install fonttools brotli). Run from the repo
root: python3 scripts/make_calmet_ui_fonts.py <dir with CalmetUI-Regular.ttf and
CalmetUI-Italic.ttf>
"""

import hashlib
import io
import os
import sys

import brotli
from fontTools.ttLib import TTFont

OUT = "layout/assets/fonts/ui"

PINNED = {
    "CalmetUI-Regular.ttf": "f0b2073d03df53aa30b099c5d55b3ec5bee1792c30ae33dae6ab059eba6eadac",
    "CalmetUI-Italic.ttf": "2d5579369275e7b56e4500a2973a68613f4b95975fd4ab200388cde6e8be2529",
}


def ranges_of(raw):
    cmap = TTFont(io.BytesIO(raw)).getBestCmap()
    ranges = []
    for cp in sorted(c for c in cmap if c >= 0x20):
        if ranges and ranges[-1][1] == cp - 1:
            ranges[-1][1] = cp
        else:
            ranges.append([cp, cp])
    return ranges


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    source = sys.argv[1]
    for name, want in PINNED.items():
        raw = open(os.path.join(source, name), "rb").read()
        got = hashlib.sha256(raw).hexdigest()
        if got != want:
            sys.exit(f"{name}: sha256 {got}, pinned {want} - a new build: pin it and update the ranges")
        packed = brotli.compress(raw, quality=11)
        out = os.path.join(OUT, name + ".br")
        open(out, "wb").write(packed)
        print(f"{out}: {len(raw)} B ttf -> {len(packed)} B br")
        for start, end in ranges_of(raw):
            print(f"        (0x{start:04X}, 0x{end:04X}),")


main()
