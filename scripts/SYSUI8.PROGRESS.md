# SYSUI8 progress - `system-ui` is the real system UI font

Branch `wt/sysui8` from `5745afee6` (wave 8 integrated). Brief: scripts/waves/wave8/PLAN.md "SYSUI8".
Report (at the end): scripts/SYSUI8_2026_10_03.md. Probes (not committed): target/sysui8/*.py.

## Findings (measured, 2026-10-03)

- macOS: `system-ui` AND the widgets' `system:ui` both end at `/System/Library/Fonts/SFNS.ttf`
  ("System Font", `.SFNS-Regular`): ONE variable font, axes wdth 30-150, opsz 17-96 (default 28),
  GRAD, wght 1-1000; it carries an AAT `trak` table (tracking per point size).
- CoreText's UI font (`CTFontCreateUIFontForLanguage(kCTFontUIFontSystem, s)`, also
  `CTFontCreateWithName("System Font", s)`) is SFNS at opsz = clamp(s, 17, 96), plus the `trak`
  tracking at s on every glyph advance (positions: no half offset). "Hello world agenda": 16px
  CoreText 139.148, Chrome 154 139.1562; azul 128.29 = the DEFAULT instance (opsz 28), untracked.
- Chrome 154 (target/sysui8/chrome_probe.py): system-ui 11/13/16/20/28/40px = 100.70 / 116.27 /
  139.16 / 168.02 / 231.41 / 327.41; bold 16px 148.63; BlinkMacSystemFont = system-ui;
  `-apple-system` is NOT known to Chrome 154 (a lone one falls to Times).
- Model check (fontTools instancer + trak, probe4/6): opsz instance advances + trak(size) per glyph
  = CoreText to 0.000 at >= 28px, -0.2..-0.35px below: SF kerns "wo" -40 units at opsz 17 through a
  GPOS variation delta (allsorts' baked instance keeps the default GPOS value).
- SF's widths only vary with opsz between 17 and 28; above 28 the advances equal the default's.
- Only Apple's system faces carry `trak` (SFNS, SFNSItalic, SF Compact/Rounded/Hebrew/...,
  New York, Apple Color Emoji); Helvetica / Helvetica Neue / Menlo have none.
- The window's skip-font-resolution signature hashed only per-node FAMILY hashes: a relayout that
  only changed sizes or weights kept the old chains (latent for weights, fatal for opsz keys).

## DONE (commits)
- cae68bbcc progress
- 65bdc86b6 RED test layout/tests/system_ui_is_the_system_font_at_its_optical_size.rs (3 tests)
- 9a5866637 GREEN A: `Tracking` (font.rs, `trak` normal track), applied in text3/default.rs shaping
- 89c5d7ae1 refactor: `bake_instance` / `read_variation_axis` (one bake, one axis reader)
- 6c5a6451a GREEN B1: FontSelector/FontChainKey.optical_size, `optical_size_for`,
  select_variable_instances / variable_instance (weight + opsz, unique instance names)
- 847c01db5 GREEN B2: `at_optical_size` in get_style_properties + collector keyed on size
- be0e68ea3 RED: text that only changes size is drawn at the new size
- 28dc312fe GREEN: window.rs font-stack signature mixes weight/style/raw font size
- a87af32f1 RED unit test + c4ebf6301 GREEN: unloaded covering face -> next loaded face + deficit

## NEXT
1. GREEN C: `-apple-system` / `BlinkMacSystemFont` = system-ui on Apple (build_font_selector_stack);
   ONE OS UI font list for CSS `system-ui` and `system:ui` (browser_generic_families sets
   SystemUi = SystemFontType::Ui's chain); Linux: the desktop's detected UI font first (SystemStyle);
   Windows: Segoe UI Variable (already in the Ui chain).
2. LOOK at the apps (capped, one at a time) - only possible on a NEW build; the prebuilt binaries
   are the old engine. Decide: note expected width changes (SF Text is ~8% wider than the Display
   instance at 13px) and check widget CSS that hard-codes widths for text.
3. Report scripts/SYSUI8_2026_10_03.md.

## Decisions
- `-apple-system` follows Safari (system font) though Chrome 154 dropped it: the user named both;
  real stacks list both.
- opsz bucket = round(font-size px), clamped to the face's axis at bake time; memo by the baked
  coordinates (11-17px share SF's opsz-17 instance).
- Instances are registered under their own name ("System Font @ wght 700 opsz 17") so a family
  query never returns an instance in place of the variable file.
- GPOS variation kerning of the baked instance is not applied (<= 0.35px per kern pair at 16px);
  test tolerance 0.5px.
