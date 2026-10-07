#!/usr/bin/env python3
"""Cut the EB Garamond faces azul bundles for flora's chrome.

Source: doc/fonts/EBGaramond-Variable.woff2 - the website's own latin slice of
the variable font (SIL OFL 1.1, licence next to it). azul's font parser reads
WOFF2 but measures a decompressed table as zero-width, and registering a
variable font bakes one instance per weight bucket per window, so the bundle
is two STATIC instances (400 for running text, 700 for flora's capitals),
brotli-compressed at quality 11 like the Material Icons font.

Output: layout/assets/fonts/ui/EBGaramond-{Regular,Bold}.ttf.br (+ the OFL).
Needs fontTools and brotli (pip install fonttools brotli). Run from the repo
root: python3 scripts/make_ebgaramond_ui_fonts.py
"""

import os
import shutil

import brotli
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

SRC = "doc/fonts/EBGaramond-Variable.woff2"
OUT = "layout/assets/fonts/ui"

os.makedirs(OUT, exist_ok=True)
for weight, style in [(400, "Regular"), (700, "Bold")]:
    font = TTFont(SRC)
    static = instancer.instantiateVariableFont(font, {"wght": weight}, updateFontNames=True)
    static.flavor = None
    tmp = os.path.join(OUT, f"EBGaramond-{style}.ttf")
    static.save(tmp)
    raw = open(tmp, "rb").read()
    os.remove(tmp)
    packed = brotli.compress(raw, quality=11)
    open(tmp + ".br", "wb").write(packed)
    check = TTFont(SRC)
    print(f"{tmp}.br: {len(raw)} B ttf -> {len(packed)} B br, weight {weight}")
shutil.copyfile("doc/fonts/EBGaramond-OFL.txt", os.path.join(OUT, "EBGaramond-OFL.txt"))
