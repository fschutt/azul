#!/usr/bin/env python3
"""Dump ALL glyph data needed to verify our Rust parser: points, contours,
instructions, on-curve flags, and hinted results at various ppem values.

Focus on glyphs that appear in the lorem ipsum test.
"""
from fontTools.ttLib import TTFont
import freetype
import json

FONT_PATH = "/System/Library/Fonts/Supplemental/Times New Roman.ttf"

font = TTFont(FONT_PATH)
glyf_table = font['glyf']
cmap = font.getBestCmap()

face = freetype.Face(FONT_PATH)

# Test at multiple ppem values that matter for the lorem ipsum test
test_ppems = [12, 14, 16, 20, 24, 32, 48]
test_chars = 'Lorem ipsum dolor sit amet.'

# For each unique char, dump raw glyph data
unique_chars = sorted(set(test_chars))
print("=== RAW GLYPH DATA (fonttools) ===\n")

for ch in unique_chars:
    gid_name = cmap.get(ord(ch))
    if gid_name is None:
        continue
    glyph = glyf_table[gid_name]

    if glyph.isComposite() or glyph.numberOfContours <= 0:
        continue

    coords = list(glyph.coordinates)
    flags = list(glyph.flags)
    ends = list(glyph.endPtsOfContours)
    instrs = list(glyph.program.getBytecode()) if hasattr(glyph, 'program') and glyph.program else []

    print(f"'{ch}' n_pts={len(coords)} n_contours={len(ends)} ends={ends} n_instr={len(instrs)}")

# Now check hinted output at each ppem for 'L' (simple, 1 contour)
print("\n=== HINTED COMPARISON AT VARIOUS PPEM ===\n")

for ppem in test_ppems:
    face.set_char_size(0, ppem * 64, 72, 72)

    # Check a few glyphs
    for ch in ['L', 'o', 'e', 'm']:
        gid = face.get_char_index(ord(ch))

        # Unhinted
        face.load_glyph(gid, freetype.FT_LOAD_NO_HINTING)
        pts_uh = list(face.glyph.outline.points)
        n_contours = face.glyph.outline.n_contours
        contour_ends = list(face.glyph.outline.contours)

        # Hinted
        face.load_glyph(gid, freetype.FT_LOAD_TARGET_MONO)
        pts_h = list(face.glyph.outline.points)
        adv_h = face.glyph.advance.x

        # Compute max Y delta
        max_dy = 0
        for (ux, uy), (hx, hy) in zip(pts_uh, pts_h):
            dy = abs(hy - uy)
            if dy > max_dy:
                max_dy = dy

        print(f"ppem={ppem:2} '{ch}' gid={gid:3} contours={n_contours} ends={contour_ends} "
              f"adv={adv_h} max_dy={max_dy} F26Dot6 ({max_dy/64:.2f}px)")

# Specifically check if hinting produces grid-aligned Y for stems
print("\n=== STEM ALIGNMENT CHECK (ppem=16, 'L') ===\n")
face.set_char_size(0, 16 * 64, 72, 72)
gid = face.get_char_index(ord('L'))
face.load_glyph(gid, freetype.FT_LOAD_TARGET_MONO)
pts = list(face.glyph.outline.points)
for i, (x, y) in enumerate(pts):
    grid = "GRID" if y % 64 == 0 else f"off by {y % 64}"
    print(f"  pt{i:2}: ({x:5},{y:5}) Y={y/64:.2f}px {grid}")
