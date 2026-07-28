#!/usr/bin/env python3
"""Generate the committed MOCK TEST FONTS in `assets/fonts/test/`.

Why: real system fonts have metrics we do not control, so a text test can
only ever assert "roughly this wide". A mock font with hand-chosen metrics
turns text layout into ARITHMETIC: if every glyph advances exactly 0.5em and
the ascent/descent are exactly 0.8em/0.2em, then a 5-character string at
font-size 20px occupies exactly 50px and its line box is exactly 20px tall.
Caret offsets, selection rects, line-break positions and bidi run widths all
become exact integers you can write down in a test.

The glyphs are deliberately dumb, but they are NOT interchangeable: every
printable ASCII codepoint maps to its own glyph id whose outline is the SAME
rectangle with a codepoint-derived bite taken out of its interior (see
`_glyf_cell`). Nothing about the SHAPE matters; everything about the METRICS
does — but the shapes must still DIFFER from each other, because a font whose
glyphs all rasterise identically makes "the text changed" invisible to a pixel
diff, and every `assert_changed` over an equal-length text edit then asserts
something that cannot hold.

Generating a NEW mock font is mechanical — add an entry to FONTS below:
    family      : the CSS font-family name the tests will ask for
    advance     : per-glyph advance width in font units
    upem        : units per em (keep 1000 so `advance/upem` reads as a fraction of em)
    ascent/descent
    codepoints  : which characters get a glyph (everything else = .notdef → fallback)
                  (must be contiguous; the per-glyph ink pattern is keyed off
                  the offset from the first codepoint)
Then run:  python3 scripts/gen_mock_fonts.py
and commit the .ttf. E.g. an RTL mock = same call with Hebrew/Arabic codepoints;
a "missing glyph" mock = drop half the ASCII range; a proportional mock = pass a
per-codepoint advance map.

No third-party deps on purpose (the repo cannot assume fonttools).
"""

import os
import struct

_ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
# The canonical copy lives in the repo-root asset tree; `layout/assets/...` is a
# vendored copy so the *published* azul-layout crate ships the fonts inside its
# own crate root (include_bytes! cannot reach outside it). Write both so they
# never drift.
OUT_DIRS = [
    os.path.join(_ROOT, "assets", "fonts", "test"),
    os.path.join(_ROOT, "layout", "assets", "fonts", "test"),
]


def _pad4(b: bytes) -> bytes:
    return b + b"\0" * ((4 - len(b) % 4) % 4)


def _checksum(b: bytes) -> int:
    b = _pad4(b)
    total = 0
    for i in range(0, len(b), 4):
        total = (total + struct.unpack(">I", b[i : i + 4])[0]) & 0xFFFFFFFF
    return total


def _glyf_rects(bbox, rects) -> bytes:
    """A simple glyph made of `rects`, each one closed 4-point contour.

    The declared bbox is passed in rather than computed so it stays CONSTANT
    across every glyph of a font — see `_glyf_cell`, which guarantees the ink
    really does touch all four sides, so the declared box is also the tight one.

    All contours are wound the same way (CCW in font units) and are pairwise
    DISJOINT — they may share an edge but never overlap or nest. That makes the
    fill rule irrelevant: non-zero and even-odd rasterizers agree on the result,
    and abutting edges cancel exactly because the coordinates are integers
    scaled by one common factor, so tiled cells leave no seam.
    """
    pts = []
    for x0, y0, x1, y1 in rects:
        pts += [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
    n_contours = len(rects)

    out = struct.pack(">h", n_contours) + struct.pack(">hhhh", *bbox)
    # endPtsOfContours: each rect closes after its 4th point
    out += b"".join(struct.pack(">H", 4 * i + 3) for i in range(n_contours))
    out += struct.pack(">H", 0)  # instructionLength
    # flags: on-curve (0x01) for every point, run-length encoded via REPEAT
    # (0x08) so the largest glyph here (9 contours / 36 points) spends 2 bytes
    # on flags instead of 36.
    out += _repeat_flags(0x01, len(pts))
    # Coordinates are int16 deltas from the PREVIOUS point across the whole
    # glyph — the point array is global, so the chain does not restart per
    # contour. (flag bits 0x02/0x10 and 0x04/0x20 clear → 16-bit signed delta.)
    for axis in (0, 1):
        prev = 0
        for p in pts:
            out += struct.pack(">h", p[axis] - prev)
            prev = p[axis]
    return _pad4(out)


def _repeat_flags(flag: int, count: int) -> bytes:
    """`count` copies of `flag`, using the glyf REPEAT bit (0x08)."""
    out = b""
    while count > 0:
        run = min(count, 256)  # one flag byte + a repeat count of up to 255
        if run == 1:
            out += bytes([flag])
        else:
            out += bytes([flag | 0x08, run - 1])
        count -= run
    return out


def _glyf_cell(bbox, index):
    """The rectangles of glyph `index`: a constant frame + a variable interior.

    Why not just one filled rectangle (what this generator used to emit): if
    every glyph is the same ink, two equal-length strings rasterise to
    IDENTICAL pixels. A text edit like "tick 1" → "tick 2" then produces damage
    but no pixel change, so `assert_changed` / `assert_damage_covers_changes` /
    `assert_damage_sound` cannot pass — not because the engine is wrong, but
    because the font cannot express the difference.

    So the ink varies while every METRIC stays bit-identical:

      * the frame (four disjoint bars tiling the ring) is the same for every
        glyph and touches all four sides of `bbox`, so the glyph's tight bbox —
        and therefore `head`'s global bbox, `hhea.xMaxExtent` and any ink
        extent a rasterizer computes — is exactly what it was when the whole
        box was filled;
      * the interior is a 3x3 grid of cells, and cell k is KEPT iff bit k of
        `index` is CLEAR. `index` is `codepoint - first_codepoint`, so distinct
        codepoints get distinct bit patterns and therefore distinct ink. 95
        printable ASCII codepoints only reach index 94, which needs 7 of the 9
        bits, so cells 7 and 8 happen to be kept in every glyph — the grid has
        room for a wider codepoint range without losing distinctness.

    Deriving the pattern straight from the codepoint keeps this deterministic
    across runs, platforms and Python versions — no hashing, no iteration
    order, no randomness.

    Nothing in `hmtx`, `hhea`, `head`, `cmap`, `OS/2` or `post` is touched by
    any of this: the advance width, ascent, descent, line gap and upem are the
    same numbers regardless of which cells a glyph keeps.
    """
    xmin, ymin, xmax, ymax = bbox
    fx = (xmax - xmin) // 8  # frame bar thickness, x
    fy = (ymax - ymin) // 8  # frame bar thickness, y
    ix0, ix1 = xmin + fx, xmax - fx  # interior span, x
    iy0, iy1 = ymin + fy, ymax - fy  # interior span, y

    # The frame ring as four disjoint bars (corners belong to the side bars).
    rects = [
        (xmin, ymin, xmax, iy0),  # bottom
        (xmin, iy1, xmax, ymax),  # top
        (xmin, iy0, ix0, iy1),  # left
        (ix1, iy0, xmax, iy1),  # right
    ]

    # 3x3 interior grid, integer boundaries, exactly tiling [ix0,ix1]x[iy0,iy1].
    xs = [ix0 + (ix1 - ix0) * c // 3 for c in range(4)]
    ys = [iy0 + (iy1 - iy0) * r // 3 for r in range(4)]
    for r in range(3):
        # Merge horizontally adjacent kept cells into one rectangle: fewer
        # contours, fewer bytes, and no interior seams to reason about.
        run_start = None
        for c in range(4):
            keep = c < 3 and not (index >> (r * 3 + c)) & 1
            if keep and run_start is None:
                run_start = c
            elif not keep and run_start is not None:
                rects.append((xs[run_start], ys[r], xs[c], ys[r + 1]))
                run_start = None
    return rects


def build_font(family, upem, advance, ascent, descent, codepoints, box):
    subfamily = "Regular"
    n_glyphs = 1 + len(codepoints)  # .notdef + one per codepoint

    # ---- glyf / loca ----
    glyf = b""
    loca = [0]
    empty_glyph = b""  # .notdef: no contours (blank), still advances
    glyf += empty_glyph
    loca.append(len(glyf))
    max_contours, max_points = 0, 0
    for cp in codepoints:
        if cp == 0x20:  # space: blank, but same advance
            pass
        else:
            # Each codepoint gets its OWN ink (same box, same advance) so that
            # two equal-length strings can never rasterise identically.
            rects = _glyf_cell(box, cp - codepoints[0])
            glyf += _glyf_rects(box, rects)
            max_contours = max(max_contours, len(rects))
            max_points = max(max_points, 4 * len(rects))
        loca.append(len(glyf))
    loca_tbl = b"".join(struct.pack(">I", o) for o in loca)  # long format

    # ---- hmtx: every glyph the same advance ----
    hmtx = b"".join(struct.pack(">Hh", advance, 0) for _ in range(n_glyphs))

    # ---- cmap (format 4, one contiguous segment) ----
    lo, hi = codepoints[0], codepoints[-1]
    assert codepoints == list(range(lo, hi + 1)), "generator supports one contiguous range"
    id_delta = (1 - lo) & 0xFFFF  # glyph id of `lo` is 1
    seg_count = 2
    sub = struct.pack(
        ">HHHHHH", 4, 16 + 8 * seg_count, 0, seg_count * 2, 2 * (2 ** 1), 1
    )
    sub = struct.pack(">HHH", 4, 16 + 8 * seg_count, 0)  # format, length, language
    sub += struct.pack(">HHHH", seg_count * 2, 2, 1, 0)  # segCountX2, searchRange, entrySel, rangeShift
    sub += struct.pack(">HH", hi, 0xFFFF)  # endCode[]
    sub += struct.pack(">H", 0)  # reservedPad
    sub += struct.pack(">HH", lo, 0xFFFF)  # startCode[]
    sub += struct.pack(">HH", id_delta, 1)  # idDelta[]
    sub += struct.pack(">HH", 0, 0)  # idRangeOffset[]
    cmap = struct.pack(">HH", 0, 2)  # version, numTables
    off = 4 + 8 * 2
    cmap += struct.pack(">HHI", 3, 1, off)  # windows BMP
    cmap += struct.pack(">HHI", 0, 3, off)  # unicode BMP
    cmap += sub

    # ---- name ----
    names = [
        (1, family),
        (2, subfamily),
        (3, f"AzulMock;{family}"),
        (4, f"{family} {subfamily}"),
        (5, "Version 1.000"),
        (6, family.replace(" ", "")),
    ]
    records, strings = b"", b""
    entries = []
    for name_id, value in names:
        entries.append((3, 1, 0x409, name_id, value.encode("utf-16-be")))
        entries.append((1, 0, 0, name_id, value.encode("mac-roman")))
    for plat, enc, lang, name_id, data in entries:
        records += struct.pack(">HHHHHH", plat, enc, lang, name_id, len(data), len(strings))
        strings += data
    name = struct.pack(">HHH", 0, len(entries), 6 + 12 * len(entries)) + records + strings

    # ---- head / hhea / maxp / OS2 / post ----
    head = struct.pack(
        ">IIIIHHQQhhhhHHhhh",
        0x00010000,  # version
        0x00010000,  # fontRevision
        0,  # checkSumAdjustment (patched later)
        0x5F0F3CF5,  # magic
        0b11,  # flags
        upem,
        0,  # created
        0,  # modified
        box[0], box[1], box[2], box[3],
        0,  # macStyle: regular
        8,  # lowestRecPPEM
        2,  # fontDirectionHint
        1,  # indexToLocFormat: long
        0,  # glyphDataFormat
    )
    hhea = struct.pack(
        ">IhhhHhhhhhhhhhhhH",
        0x00010000,
        ascent,
        -descent,
        0,  # lineGap
        advance,  # advanceWidthMax
        0, 0,  # minLeft/RightSideBearing
        advance,  # xMaxExtent
        1, 1, 0,  # caretSlopeRise/Run/Offset
        0, 0, 0, 0,  # reserved
        0,  # metricDataFormat
        n_glyphs,  # numberOfHMetrics: all glyphs have an entry
    )
    # maxPoints/maxContours must cover the LARGEST glyph: rasterizers (FreeType
    # among them) size their point buffers from these, so understating them
    # truncates or rejects the glyph.
    maxp = struct.pack(
        ">IHHHHHHHHHHHHHH",
        0x00010000, n_glyphs, max_points, max_contours, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0,
    )
    os2 = struct.pack(
        ">HhHHHhhhhhhhhhh",
        4,  # version
        advance,  # xAvgCharWidth
        400,  # usWeightClass
        5,  # usWidthClass
        0,  # fsType
        advance // 2, ascent // 2, advance // 2, ascent // 2,  # subscript
        advance // 2, ascent // 2,  # superscript X/Y size
        0, ascent // 2,  # superscript offsets
        ascent // 20, ascent // 2,  # strikeout size/pos
    )
    os2 += struct.pack(">h", 0)  # sFamilyClass
    os2 += bytes([2, 11, 6, 9, 2, 2, 2, 2, 2, 4])  # PANOSE (monospaced-ish)
    os2 += struct.pack(">IIII", 1, 0, 0, 0)  # ulUnicodeRange1-4 (basic latin)
    os2 += b"AZUL"  # achVendID
    os2 += struct.pack(">H", 0x0040)  # fsSelection = REGULAR
    os2 += struct.pack(">HH", lo, hi)  # usFirst/LastCharIndex
    os2 += struct.pack(">hhhhh", ascent, -descent, 0, ascent, descent)  # typo + win asc/desc
    os2 += struct.pack(">II", 0, 0)  # ulCodePageRange1/2
    os2 += struct.pack(">hhhHHH", ascent // 2, int(ascent * 0.9), 0, 0, 0, 2)  # sxHeight..usMaxContext
    post = struct.pack(">IIhhIIIII", 0x00030000, 0, 0, 0, 1, 0, 0, 0, 0)

    # ---- minimal OpenType Layout tables ----
    # These carry NO shaping behaviour (empty script/feature/lookup lists); they
    # exist only so the mock font can stand in as a positive control for code
    # that inspects GSUB/GPOS/GDEF presence — real fonts ship them and our tests
    # assert we extract their bytes. They do not touch glyf/hmtx/cmap, so every
    # advance-width metric the other tests rely on is unchanged.
    #
    # GSUB/GPOS share the same header (v1.0 + three offsets) followed by three
    # empty lists: ScriptList(count=0), FeatureList(count=0), LookupList(count=0).
    _empty_layout = struct.pack(">HHHHH", 1, 0, 10, 12, 14) + struct.pack(">HHH", 0, 0, 0)
    gsub = _empty_layout
    gpos = _empty_layout
    # GDEF v1.0 with all four subtable offsets NULL (no glyph-class def, attach
    # list, ligature-caret list, or mark-attach class def).
    gdef = struct.pack(">HHHHHH", 1, 0, 0, 0, 0, 0)

    tables = {
        b"GDEF": gdef,
        b"GPOS": gpos,
        b"GSUB": gsub,
        b"OS/2": os2,
        b"cmap": cmap,
        b"glyf": glyf,
        b"head": head,
        b"hhea": hhea,
        b"hmtx": hmtx,
        b"loca": loca_tbl,
        b"maxp": maxp,
        b"name": name,
        b"post": post,
    }

    # ---- assemble sfnt ----
    tags = sorted(tables)
    num = len(tags)
    entry_selector = max(0, (num.bit_length() - 1))
    search_range = (2 ** entry_selector) * 16
    header = struct.pack(">IHHHH", 0x00010000, num, search_range, entry_selector, num * 16 - search_range)
    offset = 12 + 16 * num
    dir_entries, body = b"", b""
    for tag in tags:
        data = tables[tag]
        dir_entries += tag + struct.pack(">III", _checksum(data), offset, len(data))
        padded = _pad4(data)
        body += padded
        offset += len(padded)
    font = header + dir_entries + body

    # head.checkSumAdjustment
    head_off = 12 + 16 * tags.index(b"head")
    head_data_off = struct.unpack(">I", font[head_off + 8 : head_off + 12])[0]
    adj = (0xB1B0AFBA - _checksum(font)) & 0xFFFFFFFF
    font = (
        font[: head_data_off + 8] + struct.pack(">I", adj) + font[head_data_off + 12 :]
    )
    return font


ASCII = list(range(0x20, 0x7F))

FONTS = [
    # family, upem, advance, ascent, descent, codepoints, glyph box
    ("Azul Mock Mono", 1000, 500, 800, 200, ASCII, (50, 0, 450, 700)),
    ("Azul Mock Wide", 1000, 1000, 800, 200, ASCII, (50, 0, 950, 700)),
]

if __name__ == "__main__":
    for family, upem, advance, ascent, descent, cps, box in FONTS:
        data = build_font(family, upem, advance, ascent, descent, cps, box)
        fname = family.lower().replace(" ", "-") + ".ttf"
        for out_dir in OUT_DIRS:
            os.makedirs(out_dir, exist_ok=True)
            path = os.path.join(out_dir, fname)
            with open(path, "wb") as f:
                f.write(data)
            print(f"wrote {path} ({len(data)} bytes, advance={advance}/{upem} em)")
