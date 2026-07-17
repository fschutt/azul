#!/usr/bin/env python3
"""Verify contour structure from raw font data."""
import freetype, struct

FONT_PATH = "/System/Library/Fonts/Supplemental/Times New Roman.ttf"
face = freetype.Face(FONT_PATH)
face.set_char_size(0, 80 * 64, 72, 72)

# Get glyph index for '8'
gid = face.get_char_index(ord('8'))
print(f"Glyph index for '8': {gid}")

# Load without hinting to see raw outline structure
face.load_char('8', freetype.FT_LOAD_NO_HINTING)
outline = face.glyph.outline
print(f"Number of contours: {outline.n_contours}")
print(f"Number of points: {outline.n_points}")
print(f"Contour ends: {list(outline.contours)}")
print(f"Points: {len(list(outline.points))}")

# Also check with FT_LOAD_NO_SCALE to get raw font-unit coords
face.load_char('8', freetype.FT_LOAD_NO_SCALE)
outline_raw = face.glyph.outline
print(f"\nRaw (no scale):")
print(f"  Contours: {outline_raw.n_contours}, ends: {list(outline_raw.contours)}")
print(f"  Points: {outline_raw.n_points}")
for i, (x, y) in enumerate(outline_raw.points):
    on = "ON " if outline_raw.tags[i] & 1 else "OFF"
    end = " <END>" if i in list(outline_raw.contours) else ""
    print(f"  pt{i:2}: ({x:5},{y:5}) {on}{end}")
