#!/usr/bin/env python3
"""Compare raw font data (fonttools) with what our Rust parser should extract.

Dumps: glyph points, contour ends, instructions, on-curve flags
for several glyphs at different complexity levels.
"""
from fontTools.ttLib import TTFont
import sys

FONT_PATH = "/System/Library/Fonts/Supplemental/Times New Roman.ttf"

font = TTFont(FONT_PATH)
glyf_table = font['glyf']
cmap = font.getBestCmap()
head = font['head']
maxp = font['maxp']

print(f"Font: {FONT_PATH}")
print(f"units_per_em: {head.unitsPerEm}")
print(f"numGlyphs: {maxp.numGlyphs}")

# Check fpgm, prep, cvt sizes
for tag in ['fpgm', 'prep', 'cvt ']:
    if tag in font:
        data = font.getTableData(tag)
        print(f"{tag.strip()}: {len(data)} bytes")

print()

# Test glyphs: simple ones + the problematic "8"
test_chars = ['L', 'o', 'r', 'e', 'm', '8', 'T', 'i']

for ch in test_chars:
    gid = cmap.get(ord(ch))
    if gid is None:
        print(f"'{ch}': not in cmap")
        continue

    glyph = glyf_table[gid]

    if glyph.isComposite():
        print(f"'{ch}' (gid={gid}): COMPOSITE ({glyph.numberOfContours} components)")
        for comp in glyph.components:
            print(f"  component: glyph={comp.glyphName} flags=0x{comp.flags:04X}")
        continue

    if not hasattr(glyph, 'coordinates') or glyph.numberOfContours <= 0:
        print(f"'{ch}' (gid={gid}): empty/no contours")
        continue

    coords = list(glyph.coordinates)
    flags = list(glyph.flags)
    ends = list(glyph.endPtsOfContours)
    instrs = list(glyph.program.getBytecode()) if hasattr(glyph, 'program') and glyph.program else []

    print(f"'{ch}' (gid={gid}): {len(coords)} points, {len(ends)} contours, ends={ends}, {len(instrs)} instr bytes")

    # Print first few and last few points
    for i, ((x, y), f) in enumerate(zip(coords, flags)):
        on = "ON " if f & 1 else "OFF"
        end_mark = " <END>" if i in ends else ""
        print(f"  pt{i:2}: ({x:5},{y:5}) {on}{end_mark}")

    # Print instruction bytes (hex) - first 40 bytes
    if instrs:
        hex_str = ' '.join(f'{b:02X}' for b in instrs[:60])
        print(f"  instructions[0..{min(60,len(instrs))}]: {hex_str}")
        if len(instrs) > 60:
            print(f"  ... ({len(instrs)} total bytes)")

    print()

# Also dump CVT table values
if 'cvt ' in font:
    cvt_data = font.getTableData('cvt ')
    import struct
    n_cvt = len(cvt_data) // 2
    cvt_values = struct.unpack(f'>{n_cvt}h', cvt_data)
    print(f"CVT: {n_cvt} entries")
    # Print entries that might matter (indices 5, 13, and around 262)
    for idx in [0, 1, 2, 3, 4, 5, 6, 13, 25, 26, 262]:
        if idx < n_cvt:
            print(f"  CVT[{idx}] = {cvt_values[idx]} FUnits")
    print()

# Dump prep bytecode (first 100 bytes)
if 'prep' in font:
    prep_data = font.getTableData('prep')
    hex_str = ' '.join(f'{b:02X}' for b in prep_data[:100])
    print(f"prep[0..{min(100,len(prep_data))}]: {hex_str}")
    if len(prep_data) > 100:
        print(f"  ... ({len(prep_data)} total bytes)")
