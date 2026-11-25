#!/usr/bin/env python3
"""Generate the TEST-ONLY multi-axis variable font `layout/tests/fonts/azul-mock-variable.ttf`.

The font exists so a test can read WHICH axis of a variable face moved, and by
how much, from the advance of ONE glyph: every axis has a glyph of its own
whose advance is linear in that axis only (one font unit per user unit, or the
slope given below), and every other glyph is constant.

    axis   min / default / max    glyph   advance (font units)
    wght   300 / 400 / 700        W       500 + 1  * (wght - 400)
    wdth    75 / 100 / 125        D       500 + 4  * (wdth - 100)
    slnt   -15 /   0 /   0        S       500 + 10 * slnt
    ital     0 /   0 /   1        I       500 + 100 * ital
    opsz     8 /  14 / 144        O       500 + 2  * (opsz - 14)

Every other printable ASCII character (and the space) is a constant 500 units.
unitsPerEm is 1000, ascent 800, descent -200, every outline one filled box
(50, 0)-(500 - 50, 700): only the metrics matter. The advances vary through the
phantom points of the glyphs' `gvar` data (tuples with explicit deltas, no
interpolation), the way `allsorts::variations::instance` (rust-fontconfig's
`FcCreateVariableInstance`) applies them.

Deterministic: `head.created` / `modified` are pinned to 0 and nothing embeds a
timestamp or a UUID, so running the script again reproduces identical bytes.

Re-run:  python3 scripts/gen_variable_test_font.py        (fontTools >= 4.40)
Then update layout/tests/fonts/README.md if you change a metric.
Pure Python: runs no `cargo`.
"""

import os

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables.TupleVariation import TupleVariation

_ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(_ROOT, "layout", "tests", "fonts", "azul-mock-variable.ttf")

UPEM = 1000
ASCENT = 800
DESCENT = -200
BASE = 500  # the advance of every glyph at the default of every axis
BOX_Y0, BOX_Y1 = 0, 700
BOX_INSET = 50

ASCII = list(range(0x20, 0x7F))

# (tag, min, default, max, name) in the order of the fvar table.
AXES = [
    ("wght", 300, 400, 700, "Weight"),
    ("wdth", 75, 100, 125, "Width"),
    ("slnt", -15, 0, 0, "Slant"),
    ("ital", 0, 0, 1, "Italic"),
    ("opsz", 8, 14, 144, "Optical Size"),
]

# glyph name, character, axis tag, font units per user unit
VARYING = [
    ("W", "W", "wght", 1),
    ("D", "D", "wdth", 4),
    ("S", "S", "slnt", 10),
    ("I", "I", "ital", 100),
    ("O", "O", "opsz", 2),
]


def _box(advance):
    pen = TTGlyphPen(None)
    x0 = BOX_INSET
    x1 = advance - BOX_INSET
    pen.moveTo((x0, BOX_Y0))
    pen.lineTo((x1, BOX_Y0))
    pen.lineTo((x1, BOX_Y1))
    pen.lineTo((x0, BOX_Y1))
    pen.closePath()
    return pen.glyph()


def _blank():
    return TTGlyphPen(None).glyph()


def _axis(tag):
    return next(a for a in AXES if a[0] == tag)


def _tuples(tag, slope):
    """The gvar tuples that make the advance of a box glyph `BASE + slope * (v - default)`.

    A box has 4 points, then the 4 phantom points: (lsb origin, advance, top, bottom).
    The advance is the x distance of the first two, so the delta of the second
    phantom point is the change of the advance. One tuple per side of the default
    that the axis has: the delta at the end of the range is slope * (end - default).
    """
    _, lo, default, hi, _ = _axis(tag)
    out = []
    for end, peak in ((lo, (-1.0, -1.0, 0.0)), (hi, (0.0, 1.0, 1.0))):
        if end == default:
            continue  # no range on this side of the default
        delta = slope * (end - default)
        coordinates = [(0, 0)] * 4  # the box's own points do not move
        coordinates += [(0, 0), (delta, 0), (0, 0), (0, 0)]  # phantom points
        out.append(TupleVariation({tag: peak}, coordinates))
    return out


def build():
    glyph_order = [".notdef", "space"]
    glyphs = {".notdef": _box(BASE), "space": _blank()}
    metrics = {".notdef": (BASE, BOX_INSET), "space": (BASE, 0)}
    cmap = {0x20: "space"}
    varying_chars = {char: name for name, char, _, _ in VARYING}

    for cp in ASCII:
        if cp == 0x20:
            continue
        char = chr(cp)
        name = varying_chars.get(char, "uni%04X" % cp)
        if name in glyphs:
            continue
        glyph_order.append(name)
        glyphs[name] = _box(BASE)
        metrics[name] = (BASE, BOX_INSET)
    for cp in ASCII:
        if cp != 0x20:
            char = chr(cp)
            cmap[cp] = varying_chars.get(char, "uni%04X" % cp)

    fb = FontBuilder(unitsPerEm=UPEM, isTTF=True)
    fb.setupGlyphOrder(glyph_order)
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=ASCENT, descent=DESCENT, lineGap=0, advanceWidthMax=BASE + 300)
    fb.setupNameTable(
        {
            "familyName": "Azul Mock Variable",
            "styleName": "Regular",
            "uniqueFontIdentifier": "AzulStress;Azul Mock Variable;1.000",
            "fullName": "Azul Mock Variable Regular",
            "version": "Version 1.000",
            "psName": "AzulMockVariable-Regular",
        }
    )
    fb.setupOS2(
        version=4,
        sTypoAscender=ASCENT,
        sTypoDescender=DESCENT,
        sTypoLineGap=0,
        usWinAscent=ASCENT,
        usWinDescent=-DESCENT,
        achVendID="AZUL",
        fsSelection=0x0040,  # REGULAR
        usWeightClass=400,
        usWidthClass=5,
        usFirstCharIndex=0x20,
        usLastCharIndex=0x7E,
    )
    fb.setupPost(keepGlyphNames=True)
    fb.setupFvar([(tag, lo, default, hi, name) for tag, lo, default, hi, name in AXES], [])
    fb.setupGvar({name: _tuples(tag, slope) for name, _char, tag, slope in VARYING})

    font = fb.font
    head = font["head"]
    head.created = 0
    head.modified = 0
    head.fontRevision = 1.0
    glyf = font["glyf"]
    boxes = []
    for name in font.getGlyphOrder():
        g = glyf[name]
        if getattr(g, "numberOfContours", 0) != 0:
            g.recalcBounds(glyf)
            boxes.append((g.xMin, g.yMin, g.xMax, g.yMax))
    head.xMin = min(b[0] for b in boxes)
    head.yMin = min(b[1] for b in boxes)
    head.xMax = max(b[2] for b in boxes)
    head.yMax = max(b[3] for b in boxes)
    return font


if __name__ == "__main__":
    build().save(OUT)
    print("wrote", os.path.normpath(OUT))
