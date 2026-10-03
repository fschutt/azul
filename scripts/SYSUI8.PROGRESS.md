# SYSUI8 progress - `system-ui` is the real system UI font

Branch `wt/sysui8` from `5745afee6` (wave 8 integrated). Brief: scripts/waves/wave8/PLAN.md "SYSUI8".
Report (at the end): scripts/SYSUI8_2026_10_03.md.

## Findings so far (measured, 2026-10-03)

- macOS: `system-ui` AND the widgets' `system:ui` both end at `/System/Library/Fonts/SFNS.ttf`
  ("System Font", `.SFNS-Regular`): ONE variable font, axes wdth 30-150, opsz 17-96 (default 28),
  GRAD, wght 1-1000; it carries an AAT `trak` table (tracking per point size).
- CoreText's UI font (`CTFontCreateUIFontForLanguage(kCTFontUIFontSystem, s)`, also
  `CTFontCreateWithName("System Font", s)`) is SFNS at opsz = clamp(s, 17, 96), plus the `trak`
  tracking at s. "Hello world agenda": 16px CoreText 139.148 (Chrome 154: 139.16), 13px 116.257,
  28px 231.396. azul today 128.29 at 16px = the DEFAULT instance (opsz 28, SF Pro Display
  spacing) with no tracking.
- Model check (fontTools instancer + trak, target/sysui8/probe4.py): opsz instance advances +
  trak(size) per glyph = CoreText to 0.000 at >= 28px, and to -0.2..-0.33px below (a GPOS kern
  pair whose value varies with opsz; allsorts' baked instance keeps the default GPOS values).
- The width only varies with opsz between 17 and 28 for SF; above 28 the advances equal the
  default instance's.
- Only Apple's system faces carry `trak` (SFNS, SFNSItalic, SF Compact/Rounded/Hebrew/...,
  New York, Apple Color Emoji); Helvetica / Helvetica Neue / Menlo have none (CoreText width =
  hmtx sum), so applying `trak` whenever a face has one moves nothing else.

## DONE
- (none yet)

## IN PROGRESS
- Chrome probe for the RED numbers (target/sysui8/chrome_probe.py, not committed).

## NEXT
1. RED test layout/tests/system_ui_is_the_system_font_at_its_optical_size.rs (macOS).
2. GREEN A: AAT `trak` tracking (font.rs parse, text3/default.rs apply).
3. GREEN B: optical sizing (`font-optical-sizing: auto`): FontSelector.optical_size -> FontChainKey,
   collector keys on the size, variable instance baked at (wght, opsz).
4. GREEN C: one OS UI font list for `system-ui` and `system:ui`; `-apple-system` /
   `BlinkMacSystemFont` on Apple; Windows Segoe UI Variable; Linux the desktop's UI font.
5. LOOK at the apps (capped, one at a time), fix widget CSS.
6. Report.

## Decisions
- (see report)
