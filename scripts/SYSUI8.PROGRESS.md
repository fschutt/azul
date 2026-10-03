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
- ece48a76b RED + 4f77dc395 GREEN C: `system-ui` generic = SystemFontType::Ui chain (font.rs
  browser_generic_families), prefer_system_ui_font / use_system_ui_font (Linux desktop font, dll
  shell2/common/layout.rs), `system:ui` + Apple aliases build the one `system-ui` selector
- 19bfd7f51 RED + fd7e59d41 GREEN: style attribute split_once(':') (core/src/xml_attributes.rs):
  `font-family: system:ui` in markup was cut to `system` (prebuilt AzPaint: Helvetica 136.98)

## Prebuilt AzPaint probe (target/sysui8/app_probe.py, 16px "Hello world agenda")
system:ui (markup) 136.98 (the colon bug -> Helvetica), system-ui 128.30, 'System Font' 128.30,
'Helvetica Neue' 138.36, Helvetica 136.98, BlinkMacSystemFont / -apple-system 136.98 (Helvetica).
The widgets' const `System("system:ui")` is not parsed, so production widgets draw SF Display.

## NEXT (exact)
1. Baseline LOOK on the prebuilt (old-engine) apps with scripts/sysui8_look.py (committed):
   `AZUL_ROOT=/Users/fschutt/Development/azul scripts/waves/tools/run_capped.sh --cap-mb 1500
   --seconds 1200 --log /tmp/sysui8_look.log -- python3 scripts/sysui8_look.py record
   <worktree>/target/sysui8/look/before` (screenshots + text-node boxes per app, in target/, not
   committed). After the parent's build: same with `.../after`, then `python3 scripts/sysui8_look.py
   compare .../before .../after` lists text that newly overflows / wraps -> fix that widget's CSS.
   (Cannot be done in this worktree: no compiling; the prebuilt binaries are the OLD engine.)
2. Write scripts/SYSUI8_2026_10_03.md (what was built, commits, api.json: none, least-sure spots,
   test commands, left) and commit it.

## Expected width change (SF, "Hello world agenda", new/old)
11px x1.14, 12px x1.13, 13px x1.12, 14px x1.10, 16px x1.09, 20px x1.05, 24px x1.03, 28px+ x1.02-1.03.
