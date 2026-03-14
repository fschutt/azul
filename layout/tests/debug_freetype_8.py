#!/usr/bin/env python3
"""Debug FreeType internals for Times New Roman '8' at ppem=80.

Compares hinted vs unhinted coordinates to find which points
FreeType moves on Y-axis, and by how much.

Usage: python3 debug_freetype_8.py
Requires: pip install freetype-py
"""
import sys
try:
    import freetype
except ImportError:
    print("pip install freetype-py")
    sys.exit(1)

FONT_PATH = "/System/Library/Fonts/Supplemental/Times New Roman.ttf"
PPEM = 80

face = freetype.Face(FONT_PATH)
face.set_char_size(0, PPEM * 64, 72, 72)

# Load UNHINTED first (FT_LOAD_NO_HINTING gives raw scaled coords)
face.load_char('8', freetype.FT_LOAD_NO_HINTING)
outline_unhinted = face.glyph.outline
pts_unhinted = list(outline_unhinted.points)

# Load HINTED (FT_LOAD_TARGET_MONO = v35 full X+Y hinting)
face.load_char('8', freetype.FT_LOAD_TARGET_MONO)
outline_hinted = face.glyph.outline
pts_hinted = list(outline_hinted.points)

print(f"Times New Roman '8' at ppem={PPEM}")
print(f"  {len(pts_unhinted)} unhinted points, {len(pts_hinted)} hinted points")
print(f"  contour ends: {list(outline_hinted.contours)}")
print()

print(f"{'pt':>3} {'uh_x':>8} {'uh_y':>8} {'h_x':>8} {'h_y':>8} {'dx':>6} {'dy':>6}")
for i in range(len(pts_hinted)):
    ux, uy = pts_unhinted[i]
    hx, hy = pts_hinted[i]
    dx = hx - ux
    dy = hy - uy
    flag = " <<<" if abs(dy) > 1 else ""
    print(f"{i:3} {ux:8} {uy:8} {hx:8} {hy:8} {dx:6} {dy:6}{flag}")

print()
print("Key points for IP reference:")
for idx in [16, 23]:
    ux, uy = pts_unhinted[idx]
    hx, hy = pts_hinted[idx]
    print(f"  pt{idx}: unhinted=({ux},{uy}) hinted=({hx},{hy}) delta=({hx-ux},{hy-uy})")

print()
print("Y-axis hinting deltas (sorted by magnitude):")
y_deltas = [(i, pts_hinted[i][1] - pts_unhinted[i][1]) for i in range(len(pts_hinted))]
y_deltas.sort(key=lambda x: abs(x[1]), reverse=True)
for i, dy in y_deltas[:20]:
    if dy != 0:
        print(f"  pt{i}: dy={dy} F26Dot6 = {dy/64:.4f} px")

# Also try FT_LOAD_DEFAULT (v40) to confirm Y values are identical
face.load_char('8', freetype.FT_LOAD_DEFAULT)
pts_default = list(face.glyph.outline.points)
print()
print("v35 vs v40 comparison (should differ only on X):")
for i in range(len(pts_hinted)):
    hx, hy = pts_hinted[i]
    dx, dy = pts_default[i]
    if hy != dy:
        print(f"  pt{i}: v35 Y={hy}, v40 Y={dy}, diff={dy-hy} <<<")
if all(pts_hinted[i][1] == pts_default[i][1] for i in range(len(pts_hinted))):
    print("  All Y values identical between v35 and v40 ✓")
