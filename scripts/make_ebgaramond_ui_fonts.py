#!/usr/bin/env python3
"""Cut the EB Garamond faces azul bundles for flora (its chrome and its running text).

Source: doc/fonts/EBGaramond-Variable.woff2 and EBGaramond-Variable-Italic.woff2 -
the website's own latin slices of the variable font, byte-identical to Google
Fonts' EB Garamond v33 latin slices (name table "Version 1.003"; the SHA-256 of
each is pinned below and checked before anything is cut), SIL OFL 1.1, the
licence next to them. azul's font parser reads WOFF2 but measures a
decompressed table as zero-width, and registering a variable font bakes one
instance per weight bucket per window, so the bundle is STATIC instances - 400
for running text, 700 for flora's capitals and bold, each upright and italic
(running text has emphasis) - brotli-compressed at quality 11 like the Material
Icons font.

Output: layout/assets/fonts/ui/EBGaramond-{Regular,Bold,Italic,BoldItalic}.ttf.br
(+ the OFL). Deterministic: the source's head timestamp is kept, so a re-cut
with the same fontTools writes the same bytes. Faces that already exist are
left alone unless --all is given (the Regular and Bold faces of 2026-10-07 were
cut with a recalculated timestamp; --all re-cuts them, changing only that).

Needs fontTools and brotli (pip install fonttools brotli). Run from the repo
root: python3 scripts/make_ebgaramond_ui_fonts.py [--all] [--measure]
--measure also prints what the VARIABLE fonts would cost instead (the
decision record: static instances are smaller and cost nothing per window).
"""

import hashlib
import os
import shutil
import sys

import brotli
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

FONTS = "doc/fonts"
OUT = "layout/assets/fonts/ui"

# The website's woff2 slices, as served by
# https://fonts.gstatic.com/s/ebgaramond/v33/SlGUmQSNjdsmc35JDF1K5GR1SDk_YAPI.woff2 (upright)
# https://fonts.gstatic.com/s/ebgaramond/v33/SlGWmQSNjdsmc35JDF1K5GRweDs1ZyHKpWg.woff2 (italic)
# (the latin `unicode-range` block of
# https://fonts.googleapis.com/css2?family=EB+Garamond:ital,wght@0,400..800;1,400..800).
PINNED = {
    "EBGaramond-Variable.woff2":
        "68fd4b247dde89984ee835d99953d235e4927be2b1b732ad81173b519869cee8",
    "EBGaramond-Variable-Italic.woff2":
        "d2761541b314509e02df3539cd35fc712954a48a50f4e0ab424dde817c699c2a",
}

FACES = [
    ("EBGaramond-Variable.woff2", 400, "Regular"),
    ("EBGaramond-Variable.woff2", 700, "Bold"),
    ("EBGaramond-Variable-Italic.woff2", 400, "Italic"),
    ("EBGaramond-Variable-Italic.woff2", 700, "BoldItalic"),
]


def check_sources():
    for name, want in PINNED.items():
        got = hashlib.sha256(open(os.path.join(FONTS, name), "rb").read()).hexdigest()
        if got != want:
            sys.exit(f"{name}: sha256 {got}, pinned {want} - not the v33 slice")


def cut(source, weight, style):
    font = TTFont(os.path.join(FONTS, source), recalcTimestamp=False)
    static = instancer.instantiateVariableFont(font, {"wght": weight}, updateFontNames=True)
    static.flavor = None
    tmp = os.path.join(OUT, f"EBGaramond-{style}.ttf")
    static.save(tmp)
    raw = open(tmp, "rb").read()
    os.remove(tmp)
    packed = brotli.compress(raw, quality=11)
    open(tmp + ".br", "wb").write(packed)
    print(f"{tmp}.br: {len(raw)} B ttf -> {len(packed)} B br, weight {weight}")


def measure():
    for source in PINNED:
        font = TTFont(os.path.join(FONTS, source), recalcTimestamp=False)
        font.flavor = None
        tmp = "/tmp/_ebgaramond_variable.ttf"
        font.save(tmp)
        raw = open(tmp, "rb").read()
        os.remove(tmp)
        packed = brotli.compress(raw, quality=11)
        print(f"variable {source}: {len(raw)} B ttf -> {len(packed)} B br")


def main():
    check_sources()
    every = "--all" in sys.argv
    os.makedirs(OUT, exist_ok=True)
    for source, weight, style in FACES:
        if every or not os.path.exists(os.path.join(OUT, f"EBGaramond-{style}.ttf.br")):
            cut(source, weight, style)
    shutil.copyfile(os.path.join(FONTS, "EBGaramond-OFL.txt"), os.path.join(OUT, "EBGaramond-OFL.txt"))
    if "--measure" in sys.argv:
        measure()


main()
