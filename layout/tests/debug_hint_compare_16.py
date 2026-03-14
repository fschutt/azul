#!/usr/bin/env python3
"""Compare hinted 'L' and 'o' at ppem=16 between FreeType and our Rust output.
Run the Rust test first to get our values, then compare here."""
import freetype

FONT_PATH = "/System/Library/Fonts/Supplemental/Times New Roman.ttf"
face = freetype.Face(FONT_PATH)

for ppem in [12, 16]:
    face.set_char_size(0, ppem * 64, 72, 72)
    for ch in ['L', 'o', 'e']:
        gid = face.get_char_index(ord(ch))

        # Unhinted
        face.load_glyph(gid, freetype.FT_LOAD_NO_HINTING)
        pts_uh = list(face.glyph.outline.points)

        # Hinted
        face.load_glyph(gid, freetype.FT_LOAD_TARGET_MONO)
        pts_h = list(face.glyph.outline.points)
        adv = face.glyph.advance.x

        print(f"\nppem={ppem} '{ch}' gid={gid} adv={adv} F26Dot6 ({adv/64:.1f}px)")
        print(f"  FreeType hinted points (F26Dot6):")
        for i, ((ux,uy), (hx,hy)) in enumerate(zip(pts_uh, pts_h)):
            dx, dy = hx-ux, hy-uy
            flag = " <<<" if abs(dy) > 10 else ""
            print(f"    pt{i:2}: ({hx:5},{hy:5}) delta=({dx:+4},{dy:+4}){flag}")
