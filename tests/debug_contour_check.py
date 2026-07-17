#!/usr/bin/env python3
"""Check if the '8' glyph is simple or composite."""
import freetype

FONT_PATH = "/System/Library/Fonts/Supplemental/Times New Roman.ttf"
face = freetype.Face(FONT_PATH)

gid = face.get_char_index(ord('8'))
print(f"Glyph index for '8': {gid}")

# Read raw glyf table to check if simple or composite
face.load_glyph(gid, freetype.FT_LOAD_NO_SCALE | freetype.FT_LOAD_NO_RECURSE)
fmt = face.glyph.format
print(f"Format: {fmt} (1=composite, 4=outline/simple)")
print(f"Outline: {face.glyph.outline.n_contours} contours, {face.glyph.outline.n_points} points")
print(f"Contour ends: {list(face.glyph.outline.contours)}")

# Also check via num_subglyphs for composite
try:
    print(f"Subglyphs: {face.glyph.num_subglyphs}")
except:
    print("No subglyphs (simple glyph)")
