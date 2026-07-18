#!/usr/bin/env python3
"""Generate the committed MOCK TEST FONTS in `assets/fonts/test/`.

Why: real system fonts have metrics we do not control, so a text test can
only ever assert "roughly this wide". A mock font with hand-chosen metrics
turns text layout into ARITHMETIC: if every glyph advances exactly 0.5em and
the ascent/descent are exactly 0.8em/0.2em, then a 5-character string at
font-size 20px occupies exactly 50px and its line box is exactly 20px tall.
Caret offsets, selection rects, line-break positions and bidi run widths all
become exact integers you can write down in a test.

The glyphs are deliberately dumb: every printable ASCII codepoint maps to its
own glyph id whose outline is one filled rectangle. Nothing about the SHAPE
matters; everything about the METRICS does.

Generating a NEW mock font is mechanical — add an entry to FONTS below:
    family      : the CSS font-family name the tests will ask for
    advance     : per-glyph advance width in font units
    upem        : units per em (keep 1000 so `advance/upem` reads as a fraction of em)
    ascent/descent
    codepoints  : which characters get a glyph (everything else = .notdef → fallback)
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


def _glyf_rect(xmin, ymin, xmax, ymax) -> bytes:
    """One closed contour, 4 on-curve points: a filled rectangle."""
    out = struct.pack(">hhhhh", 1, xmin, ymin, xmax, ymax)  # numContours + bbox
    out += struct.pack(">H", 3)  # endPtsOfContours[0] = 3 (4 points)
    out += struct.pack(">H", 0)  # instructionLength
    # flags: on-curve (0x01) for all 4 points
    out += bytes([0x01, 0x01, 0x01, 0x01])
    # x coords as int16 deltas (flag bit 0x02/0x10 not set → SHORT off, so int16)
    out += struct.pack(">hhhh", xmin, xmax - xmin, 0, xmin - xmax)
    out += struct.pack(">hhhh", ymin, 0, ymax - ymin, 0)
    return _pad4(out)


def build_font(family, upem, advance, ascent, descent, codepoints, box):
    subfamily = "Regular"
    n_glyphs = 1 + len(codepoints)  # .notdef + one per codepoint

    # ---- glyf / loca ----
    glyf = b""
    loca = [0]
    empty_glyph = b""  # .notdef: no contours (blank), still advances
    glyf += empty_glyph
    loca.append(len(glyf))
    rect = _glyf_rect(*box)
    for cp in codepoints:
        if cp == 0x20:  # space: blank, but same advance
            pass
        else:
            glyf += rect
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
    maxp = struct.pack(">IHHHHHHHHHHHHHH", 0x00010000, n_glyphs, 4, 1, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0)
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
