#!/usr/bin/env python3
"""Generate the TEST-ONLY "stress-test" fonts under `layout/tests/fonts/`.

These fonts exist to exercise the text3 text-layout / shaping engine with
hand-chosen, ROUND metrics so future Rust tests become exact arithmetic rather
than "roughly this wide" fuzzy assertions. Every glyph outline is a single
filled rectangle (a black box) -- nothing about the SHAPE matters, only the
METRICS and the presence/behaviour of OpenType layout tables (GSUB/GPOS/vmtx).
A CPU rasterizer will later draw these boxes as visible coloured rectangles.

They are loaded at runtime via `std::fs::read` (NOT `include_bytes!`), so they
live in an excluded test dir and never ship in the published crate. This is the
deliberate counterpart to `scripts/gen_mock_fonts.py` (which ships two committed
mono/wide fonts); this script is fontTools-based and stays entirely separate.

Design rules (shared unless a font overrides them):
  * unitsPerEm = 1000, ascent = 800, descent = -200  (matches gen_mock_fonts)
  * every rectangle glyph = one closed contour (50, 0)-(advance-50, 700)
  * blank glyphs (space) carry an advance but no contour
  * output is deterministic: head.created/modified are pinned to 0 and no
    timestamps/UUIDs are embedded, so re-running reproduces identical bytes.

Re-run:  python3 scripts/gen_stress_fonts.py
Then update layout/tests/fonts/README.md if you change any metric.

Requires fontTools (v4.63 tested).  Pure Python -- runs no `cargo`.
"""

import os

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

# --------------------------------------------------------------------------
# Shared constants / paths
# --------------------------------------------------------------------------

_ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT_DIR = os.path.join(_ROOT, "layout", "tests", "fonts")

UPEM = 1000
ASCENT = 800
DESCENT = -200  # hhea/typo descent is negative
BOX_Y0, BOX_Y1 = 0, 700  # every rectangle spans y in [0, 700]
BOX_INSET = 50  # rectangle is inset 50 units on each side horizontally

ASCII = list(range(0x20, 0x7F))  # 0x20..0x7E inclusive (printable ASCII)


# --------------------------------------------------------------------------
# Glyph outline helpers (shape is irrelevant; these are just filled boxes)
# --------------------------------------------------------------------------

def _box_glyph(advance):
    """A single filled rectangle inset `BOX_INSET` on each side, y in [0,700]."""
    pen = TTGlyphPen(None)
    x0 = BOX_INSET
    x1 = max(advance - BOX_INSET, x0 + 1)
    pen.moveTo((x0, BOX_Y0))
    pen.lineTo((x1, BOX_Y0))
    pen.lineTo((x1, BOX_Y1))
    pen.lineTo((x0, BOX_Y1))
    pen.closePath()
    return pen.glyph()


def _blank_glyph():
    """No contour at all (used for space / .notdef when we want it invisible)."""
    return TTGlyphPen(None).glyph()


# --------------------------------------------------------------------------
# Core builder
# --------------------------------------------------------------------------

def build_font(
    family,
    glyph_specs,
    cmap,
    fea=None,
    vertical=None,
    keep_names=True,
    style="Regular",
    upem=UPEM,
    ascent=ASCENT,
    descent=DESCENT,
):
    """Build one TTF.

    glyph_specs : list of (glyph_name, h_advance, has_box) -- order defines
                  glyph ids (a leading ".notdef" is prepended automatically, so
                  the first entry here becomes glyph id 1).
    cmap        : {codepoint: glyph_name}
    fea         : optional feaLib feature source string (GSUB/GPOS)
    vertical    : optional dict describing vertical metrics:
                    {"advance": int, "ascent": int, "descent": int,
                     "tsb": {glyph_name: topSideBearing}}
    keep_names  : keep post format 2.0 (glyph names) -- set False for the huge
                  CJK font to keep `post` tiny (format 3.0).
    """
    # .notdef is glyph id 0 -- give it a visible box at the font's first advance
    # so a missing-glyph fallback is visually obvious.
    default_adv = glyph_specs[0][1]
    specs = [(".notdef", default_adv, True)] + list(glyph_specs)

    glyph_order = [name for (name, _adv, _box) in specs]

    glyphs = {}
    hmetrics = {}
    for name, adv, has_box in specs:
        glyphs[name] = _box_glyph(adv) if has_box else _blank_glyph()
        # left side bearing = BOX_INSET for boxes, 0 for blanks
        lsb = BOX_INSET if has_box else 0
        hmetrics[name] = (adv, lsb)

    fb = FontBuilder(unitsPerEm=upem, isTTF=True)
    fb.setupGlyphOrder(glyph_order)
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics(hmetrics)

    advances = [adv for (_n, adv, _b) in specs]
    fb.setupHorizontalHeader(
        ascent=ascent,
        descent=descent,
        lineGap=0,
        advanceWidthMax=max(advances),
    )

    fb.setupNameTable(
        {
            "familyName": family,
            "styleName": style,
            "uniqueFontIdentifier": f"AzulStress;{family};1.000",
            "fullName": f"{family} {style}",
            "version": "Version 1.000",
            "psName": family.replace(" ", "") + "-" + style,
        }
    )

    lo = min(cmap) if cmap else 0
    hi = max(cmap) if cmap else 0
    fb.setupOS2(
        version=4,
        sTypoAscender=ascent,
        sTypoDescender=descent,
        sTypoLineGap=0,
        usWinAscent=ascent,
        usWinDescent=-descent,
        achVendID="AZUL",
        fsSelection=0x0040,  # REGULAR
        usWeightClass=400,
        usWidthClass=5,
        usFirstCharIndex=min(lo, 0xFFFF),
        usLastCharIndex=min(hi, 0xFFFF),
    )
    fb.setupPost(keepGlyphNames=keep_names)

    # ---- vertical tables (vmtx + vhea) -------------------------------------
    if vertical is not None:
        vadv = vertical["advance"]
        tsb = vertical.get("tsb", {})
        vmetrics = {name: (vadv, tsb.get(name, 0)) for name in glyph_order}
        fb.setupVerticalMetrics(vmetrics)
        fb.setupVerticalHeader(
            ascent=vertical["ascent"],
            descent=vertical["descent"],
            lineGap=0,
            advanceHeightMax=vadv,
            numberOfVMetrics=len(glyph_order),
        )

    # ---- OpenType layout features ------------------------------------------
    if fea:
        addOpenTypeFeaturesFromString(fb.font, fea)

    _finalize(fb)
    return fb.font


def _finalize(fb):
    """Pin timestamps and recompute head bbox so output is deterministic."""
    font = fb.font
    head = font["head"]
    head.created = 0
    head.modified = 0
    head.fontRevision = 1.0
    # union bbox over all glyphs (blank glyphs contribute nothing)
    glyf = font["glyf"]
    xmins, ymins, xmaxs, ymaxs = [], [], [], []
    for name in font.getGlyphOrder():
        g = glyf[name]
        if getattr(g, "numberOfContours", 0) != 0:
            g.recalcBounds(glyf)
            xmins.append(g.xMin)
            ymins.append(g.yMin)
            xmaxs.append(g.xMax)
            ymaxs.append(g.yMax)
    if xmins:
        head.xMin, head.yMin = min(xmins), min(ymins)
        head.xMax, head.yMax = max(xmaxs), max(ymaxs)


# --------------------------------------------------------------------------
# Per-font advance rules
# --------------------------------------------------------------------------

def _ascii_specs(advance_fn):
    """Build (name, advance, has_box) for printable ASCII using advance_fn(cp)."""
    specs = []
    cmap = {}
    for cp in ASCII:
        name = "uni%04X" % cp
        specs.append((name, advance_fn(cp), cp != 0x20))  # space has no box
        cmap[cp] = name
    return specs, cmap


# ===== 1. azul-mock-liga.ttf ==============================================

def build_liga():
    specs, cmap = _ascii_specs(lambda cp: 500)  # monospace 500
    # extra ligature glyphs (not in cmap; reached only via GSUB)
    specs += [
        ("f_i", 700, True),
        ("f_f", 700, True),
        ("f_f_i", 900, True),
    ]
    # ffi listed first so the 3-component ligature wins over ff/fi (the shaper
    # takes the first match within a ligature set keyed by the first glyph 'f').
    fea = """
languagesystem DFLT dflt;
languagesystem latn dflt;

feature liga {
    sub uni0066 uni0066 uni0069 by f_f_i;
    sub uni0066 uni0069 by f_i;
    sub uni0066 uni0066 by f_f;
} liga;
"""
    return build_font("Azul Mock Liga", specs, cmap, fea=fea)


# ===== 2. azul-mock-kern.ttf ==============================================

def build_kern():
    specs, cmap = _ascii_specs(lambda cp: 500)  # monospace 500
    # GPOS pair adjustment: A V kerned by -200 (shrinks A's advance in context)
    fea = """
languagesystem DFLT dflt;
languagesystem latn dflt;

feature kern {
    pos uni0041 uni0056 -200;
} kern;
"""
    return build_font("Azul Mock Kern", specs, cmap, fea=fea)


# ===== 3. azul-mock-arabic.ttf ============================================

# Real Arabic codepoints -- the shaper derives joining from Unicode props.
BEH, TEH, LAM, ALEF, MEEM, SPACE = 0x0628, 0x062A, 0x0644, 0x0627, 0x0645, 0x0020
# beh/teh/lam/meem are DUAL-joining; alef is RIGHT-joining (no init/medi forms).
_DUAL = [("beh", BEH), ("teh", TEH), ("lam", LAM), ("meem", MEEM)]


def build_arabic():
    specs = []
    cmap = {}
    # nominal (cmap-target) glyphs
    for base, cp in _DUAL:
        specs.append((base, 500, True))
        cmap[cp] = base
    specs.append(("alef", 500, True))
    cmap[ALEF] = "alef"
    specs.append(("space", 500, False))
    cmap[SPACE] = "space"
    # positional-form glyphs for the dual-joining letters
    for base, _cp in _DUAL:
        for form in ("isol", "init", "medi", "fina"):
            specs.append((f"{base}.{form}", 500, True))
    # alef: only isolated + final forms exist (right-joining)
    specs.append(("alef.isol", 500, True))
    specs.append(("alef.fina", 500, True))
    # required lam-alef ligature (one explicit glyph)
    specs.append(("lam_alef", 500, True))

    base_class = " ".join([n for (n, _a, _b) in specs if n != "lam_alef"])

    fea = f"""
languagesystem DFLT dflt;
languagesystem arab dflt;

feature isol {{
    sub beh by beh.isol;
    sub teh by teh.isol;
    sub lam by lam.isol;
    sub meem by meem.isol;
    sub alef by alef.isol;
}} isol;

feature init {{
    sub beh by beh.init;
    sub teh by teh.init;
    sub lam by lam.init;
    sub meem by meem.init;
}} init;

feature medi {{
    sub beh by beh.medi;
    sub teh by teh.medi;
    sub lam by lam.medi;
    sub meem by meem.medi;
}} medi;

feature fina {{
    sub beh by beh.fina;
    sub teh by teh.fina;
    sub lam by lam.fina;
    sub meem by meem.fina;
    sub alef by alef.fina;
}} fina;

feature rlig {{
    sub lam.init alef.fina by lam_alef;
    sub lam.medi alef.fina by lam_alef;
    sub lam alef by lam_alef;
}} rlig;

table GDEF {{
    GlyphClassDef [{base_class}], [lam_alef], , ;
}} GDEF;
"""
    return build_font("Azul Mock Arabic", specs, cmap, fea=fea)


# ===== 4. azul-mock-vertical.ttf ==========================================

VERT_CPS = list(range(0x4E00, 0x4E10))  # U+4E00..U+4E0F, 16 ideographs


def build_vertical():
    specs = []
    cmap = {}
    for cp in VERT_CPS:
        name = "uni%04X" % cp
        specs.append((name, 1000, True))  # square: h-advance 1000
        cmap[cp] = name
    specs.append(("space", 1000, False))
    cmap[0x20] = "space"
    # vertical origin sits at the horizontal ascent (800); box tops out at 700,
    # so topSideBearing = 800 - 700 = 100 for boxes, 0 for the blank space.
    tsb = {name: (100 if has_box else 0) for (name, _a, has_box) in specs}
    tsb[".notdef"] = 100
    vertical = {"advance": 1000, "ascent": 500, "descent": -500, "tsb": tsb}
    return build_font("Azul Mock Vertical", specs, cmap, vertical=vertical)


# ===== 5. azul-mock-cjk-massive.ttf =======================================

CJK_LO, CJK_HI = 0x4E00, 0x9FFF  # full CJK Unified Ideographs block


def build_cjk_massive():
    specs = []
    cmap = {}
    for cp in range(CJK_LO, CJK_HI + 1):
        name = "uni%04X" % cp
        specs.append((name, 1000, True))
        cmap[cp] = name
    specs.append(("space", 1000, False))
    cmap[0x20] = "space"
    # keep_names=False -> post format 3.0, no per-glyph name strings (~21k names
    # would bloat the file for zero test value).
    return build_font("Azul Mock CJK Massive", specs, cmap, keep_names=False)


# ===== 6. azul-mock-prop.ttf ==============================================

_PROP_NARROW = {0x69, 0x6C, 0x2E, 0x2C}  # i l . ,   -> 200
_PROP_WIDE = {0x6D, 0x77, 0x4D, 0x57}    # m w M W   -> 900


def _prop_advance(cp):
    if cp == 0x20:
        return 250  # space
    if cp in _PROP_NARROW:
        return 200
    if cp in _PROP_WIDE:
        return 900
    return 500  # all other printable ASCII


def build_prop():
    specs, cmap = _ascii_specs(_prop_advance)
    return build_font("Azul Mock Prop", specs, cmap)


# --------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------

BUILDERS = [
    ("azul-mock-liga.ttf", build_liga),
    ("azul-mock-kern.ttf", build_kern),
    ("azul-mock-arabic.ttf", build_arabic),
    ("azul-mock-vertical.ttf", build_vertical),
    ("azul-mock-cjk-massive.ttf", build_cjk_massive),
    ("azul-mock-prop.ttf", build_prop),
]


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    for fname, builder in BUILDERS:
        font = builder()
        path = os.path.join(OUT_DIR, fname)
        font.save(path)
        size = os.path.getsize(path)
        print(f"wrote {path} ({font['maxp'].numGlyphs} glyphs, {size} bytes)")


if __name__ == "__main__":
    main()
