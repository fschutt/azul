#!/usr/bin/env python3
"""Check FreeType phantom points and vertical metrics for '8'."""
import freetype

FONT_PATH = "/System/Library/Fonts/Supplemental/Times New Roman.ttf"
face = freetype.Face(FONT_PATH)
face.set_char_size(0, 80 * 64, 72, 72)

gid = face.get_char_index(ord('8'))

# Check vertical metrics
print(f"has_vertical: {bool(face.has_vertical)}")
print(f"ascender: {face.ascender} (F26Dot6 = {face.ascender})")
print(f"descender: {face.descender}")
print(f"height: {face.height}")
print(f"units_per_EM: {face.units_per_EM}")

# Load with NO_SCALE to get raw metrics
face.load_glyph(gid, freetype.FT_LOAD_NO_SCALE)
metrics = face.glyph.metrics
print(f"\nRaw glyph metrics (FUnits):")
print(f"  width: {metrics.width}")
print(f"  height: {metrics.height}")
print(f"  horiBearingX: {metrics.horiBearingX}")
print(f"  horiBearingY: {metrics.horiBearingY}")
print(f"  horiAdvance: {metrics.horiAdvance}")
print(f"  vertBearingX: {metrics.vertBearingX}")
print(f"  vertBearingY: {metrics.vertBearingY}")
print(f"  vertAdvance: {metrics.vertAdvance}")

# Compute expected phantom points
print(f"\nPhantom points (should be):")
print(f"  phantom[0] origin: (0, 0)")
print(f"  phantom[1] advance: ({metrics.horiAdvance}, 0)")

# FreeType computes phantom[2].y from vertBearingY
# and phantom[3].y from vertBearingY - height
# These are in FUnits if loaded with NO_SCALE
vby = metrics.vertBearingY
h = metrics.height
print(f"  phantom[2] top: (0, {vby}) [vertBearingY]")
print(f"  phantom[3] bottom: (0, {vby - h}) [vertBearingY - height]")

# Now compute F26Dot6 values at ppem=80
scale = 80 * 64 * 65536 // face.units_per_EM
def from_funits(v):
    a = abs(v)
    b = abs(scale)
    c = (a * b + 0x8000) >> 16
    return c if v >= 0 else -c

print(f"\nPhantom points F26Dot6 at ppem=80:")
print(f"  phantom[2].y = {from_funits(vby)} (from {vby} FUnits)")
print(f"  phantom[3].y = {from_funits(vby - h)} (from {vby - h} FUnits)")

# Compare: load hinted and check if point count differs
face.set_char_size(0, 80 * 64, 72, 72)
face.load_char('8', freetype.FT_LOAD_TARGET_MONO)
print(f"\nHinted outline: {face.glyph.outline.n_points} points")
print(f"Hinted advance: {face.glyph.advance.x} F26Dot6")
