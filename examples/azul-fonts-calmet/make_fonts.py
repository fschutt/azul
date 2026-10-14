#!/usr/bin/env python3
"""Pack the Calmet UI faces of this crate (the typeface of AzDrive's design).

Calmet UI is traced from the 1732 print of Augustin Calmet's "Commentaire
litteral" and made fit for a UI (composed umlauts, the long s mapped, a few
re-spaced punctuation marks, liga / dlig / ss01 / tnum). The two TrueType faces
are the design's own build; this script only checks they are the pinned build
(SHA-256 below) and brotli-compresses them at quality 11 into fonts/, like the
EB Garamond faces azul bundles. Their coverage needs no table: the engine reads
each registered face's cmap.

Needs brotli (pip install brotli). Run from the repo root:
python3 examples/azul-fonts-calmet/make_fonts.py <dir with CalmetUI-Regular.ttf and
CalmetUI-Italic.ttf>
"""

import hashlib
import os
import sys

import brotli

OUT = "examples/azul-fonts-calmet/fonts"

PINNED = {
    "CalmetUI-Regular.ttf": "f0b2073d03df53aa30b099c5d55b3ec5bee1792c30ae33dae6ab059eba6eadac",
    "CalmetUI-Italic.ttf": "2d5579369275e7b56e4500a2973a68613f4b95975fd4ab200388cde6e8be2529",
}


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    source = sys.argv[1]
    for name, want in PINNED.items():
        raw = open(os.path.join(source, name), "rb").read()
        got = hashlib.sha256(raw).hexdigest()
        if got != want:
            sys.exit(f"{name}: sha256 {got}, pinned {want} - a new build: pin it")
        packed = brotli.compress(raw, quality=11)
        out = os.path.join(OUT, name + ".br")
        open(out, "wb").write(packed)
        print(f"{out}: {len(raw)} B ttf -> {len(packed)} B br")


main()
