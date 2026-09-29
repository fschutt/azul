# W3a - flat / flora themes for 13 widgets, and the native spinner (2026-09-29)

Branch `wt/w3a-widget-themes`, cut from `fix/input-bugs-2026-09-19` @ 9c6065b05.
52 commits (13 x plumbing / RED / impl, checkpoints, 3 cleanups). Nothing was
compiled (house rule) - see "Least sure to compile" before merging.

## Shape of the change

Every widget now has `theme: OptionUiTheme` (+ `set_theme` / `with_theme`),
and `dom()` dispatches to `themes::flat::<widget>` or `themes::flora::<widget>`.
The widget file keeps structure, behaviour and accessibility in one
`build(widget, &<Widget>Look)` (or `build(widget, style, classes)` for the
simple ones); the two theme modules only supply styles. Both theme modules
got their code APPENDED at the end under `// ==== <widget> ====` banners;
nothing existing was reordered. `flora::label` (which existed already) was
changed in place.

- **Flat** = the widget's established look, light values byte-for-byte
  unchanged, plus whatever was missing: dark twins and a visible focus ring.
- **Flora** = doc/templates/flora.css: paper faces (`--fl-rT -> --fl-rB`
  gradients), `--fl-bd` hairlines, the 3px / 5px house radii, small-caps
  labels, brass (`--fl-qt`) link ink, the accent stone, each with its night
  value. Every flora root carries the `__azul-theme-flora` class.

Shared helpers: NEW `layout/src/widgets/themes/decl.rs` (pub(crate)) - fill /
ink / layers, their themed (light + dark twin) forms, borders, radius,
padding, margin, single-side `shadow`, `focus_ring` (bordered nodes: border
colour on :focus), `focus_halo` (borderless nodes: 2px outset spread shadow
on :focus), `focus_halo_inset` (inside an overflow-hidden parent),
`hover_*` / `active_*`, `hover_underline`, bold / semibold, letter spacing.
One line added to `themes/mod.rs` (`pub(crate) mod decl;`).

Flora stones (shared by every flora widget with a kind), in flora.rs:
`pub struct FloraStone { stone, deep, soft, glow }` and `STONE_ACCENT`,
`STONE_LEAF` (#44684F), `STONE_CLAY` (#7E4A42), `STONE_SLATE` (#4A5C6B),
`STONE_AMBER` (#8A5A1E, for warnings - a brass fill would break flora's
"metal on borders, never a field").

Focus rings, all four combinations: flat light `#4286F4` (FIELD_RING), flat
dark = flat's night accent; flora light `--fl-acc`, flora dark `--fl-glow`.
Accessibility (roles, names, tab stops, click handlers) is unchanged in
every widget; the tests assert it where a Look could have touched it.

## Per widget

| Widget | Flat | Flora | Plumbing | RED | Impl |
|---|---|---|---|---|---|
| badge | established pill | flora `.pill`: raised paper in a `--fl-bd2` hairline, bold tracked `--fl-soft1` ink, 3px radius; a coloured kind is `.pill-live`, a stone with `--fl-on-acc` ink | 255d57be2 | f68a68f3f | 4aadcfec4 |
| label | established | `--fl-intro` ink (#4E4C45), night #BCBCBC as the twin | 1e9bf0f55 | e4c375e9c | 323a262a1 |
| divider | established 1px rule | flora `hr`: 1px `--fl-sep` (#D8D5CE / night #383838), 8px air | a3225bdee | ab3aad664 | f682418fb |
| spinner | Windows 11 ring in `system:accent` (spokes on request: black / white) | macOS spokes in `--fl-ink` (ring on request: `--fl-acc`, `--fl-glow` at night) | e101c8551 | 96f0b26bd | f666983df, 98c5bba05 |
| chip | established tag + focus halo on the remove "x" and clickable label (was invisible) | the badge pill cut for content, `--fl-ink2`, stones for kinds; the "x" inherits the ink, washes on hover, rings on focus | 4c6add68b | 993691435 | 764fb51fe |
| alert | established Bootstrap banner + focus halo on the close button | a leaf: faint stone wash, `--fl-bd` hairline with a 3px stone thread down the left, `--fl-shadow-1`; at night neutral surface, the thread lifted to the stone's glow | dd14ba85c | 3db7709e5 | 889aaf031 |
| card | established | `--fl-sur` leaf in `--fl-bd`, 5px radius, 14px padding, near half of `--fl-shadow-2` | c0616d9bb | 7d6ac6427 | 691f3714f |
| frame | established group box | title is flora's `.fl-label` (bold, 0.12em, `--fl-soft1`), every rule `--fl-bd` | 939859d31 | b5a8cfa9e | 7888c9b46 |
| breadcrumb | established + hover underline (with night twin) + focus halo | brass crumbs (`--fl-qt`, `--fl-qt2` + underline on hover), current page semibold `--fl-ink`, U+203A chevron separator in `--fl-soft2` | d4693525e | 4cef5c643 | e042d5f31 |
| accordion | established panel + header hover grey (night hover face) + INSET focus ring (the rounded panel clips outer halos) | flora FAQ list: `--fl-sur` leaf, `--fl-bd` section rules, raised-paper headers that go brass on hover and press in, semibold titles, inset `--fl-acc` / `--fl-glow` ring | 3ce4acd09 | a48742efe | 7c69e9d8e |
| menubar | established bar (via the new `Menubar` struct) | flora toolbar strip: `--fl-strip`, `--fl-ink`, 1px `--fl-bd` foot, 28px; items lift to the hover face, sink to the pressed face | bc1e6059e | a85f45fcc | 26d474db7 |
| color_input | established swatch / picker + focus halo on the swatch and the plane / hue / alpha bars; preview frame and grip take the desktop separator at night | swatch framed like a sample (`--fl-bd2`, 3px, `--fl-bd3` hover, accent ring); picker is a leaf with a framed preview, raised-paper eyedropper, `--fl-bd` grip, bars ring in the accent | 0482f8bc2 | 8e4a9f7a4 | b5415b72f |
| date_picker (REDONE on branch `wt/w3a-date-picker`, see below) | established field + calendar in every mode; ring on the field's border, halos on the header buttons and every grid cell (days, whole-week days, months) | paper field (`--fl-fld`, `--fl-bd2`), leaf calendar, brass header buttons, `--fl-soft2` weekday names, the pick (day / week / month) as the accent stone; a pick repaints with the same palette (carried in the day and month payloads) | a6d6310ba | 0b39967be | 7376e28b9 |

Checkpoint commits sit between widgets. Cleanups after the pass: 18604cd4b
(drop the unused `decl::border_top`). (Hashes in the table above are from
`wt/w3a-widget-themes`; the integrated tree carries them re-applied.)

### Follow-up branch `wt/w3a-date-picker` (from integrated b9cc36bee)

The first date-picker commits (88af3dad1, 3f2ab820a, ac5b49d0f, 4e6e9ea25)
conflicted with W1's new modes and were not integrated. Redone on W1's
date_picker.rs:

- a6d6310ba refactor: `theme` appended LAST (after W1's `name`, `mode`);
  `DatePickerLook` threads through every builder - month header, weekday
  row, day grid (Sunday-first), the week grid (Monday-first, whole row
  lit), the year header and the month grid (month cells are the look's day
  faces, three cells wide). `DayPalette` rides in `DayCellData` AND
  `MonthCellData`; `restyle_days` / `restyle_week` / `restyle_grid` take
  it. `flat::date_picker` / `flora::date_picker` appended at the ends of
  flat.rs / flora.rs (closing braces intact). The day-grid header now
  builds its buttons with W1's `header_nav_button` (same output as the
  closure it had).
- 0b39967be RED: `theme_tests` for all three modes in both looks, with
  explicit per-state probes (`at_rest` / `on_hover` / `on_focus`), plus
  the three modes in the both-looks integration test.
- 7376e28b9 feat: flat rings; flora paper calendar (as before, now also on
  the month and week grids).

Fixes for the 4 tests that failed after integration (one commit each):

| Test | Wrong side | Commit |
|---|---|---|
| accordion `a_flora_header_is_raised_paper_that_lifts_under_the_pointer` | TEST: `last(theme_probe::dark(h))` picked the `:active` night twin (`--fl-pT/--fl-pB` #1F1F1F/#262626); the impl's resting face is `--fl-rT/--fl-rB` #333333/#292929 per flora.css | de82e9746 |
| breadcrumb `a_flora_crumb_is_written_in_brass_ink` | TEST: picked the `:hover` night twin `--fl-qt2` #DED3B4; the resting night ink is `--fl-qt` #C4B58E per flora.css | 87ec428d6 |
| color_input `a_flora_swatch_is_framed_in_a_hairline_and_rings_in_the_accent` | TEST: picked the `:focus` night twin `--fl-glow` #7A93C6; the resting night rule is `--fl-bd2` #4A4A4A per flora.css | 49811cb9c |
| spinner `the_ring_spins_clockwise_at_450_degrees_a_second` | ENGINE: `compile_keyframes_track` read rotate stops with `to_degrees()` (folds into [0, 360)), so `rotate(360deg)` compiled to 0 and the ring's track stood still; now `to_degrees_raw()`. The test now samples the engine's compiled `rotate_deg` channel | 7aaab07ec |

Root cause of the three test bugs: `theme_probe::dark` returns dark twins of
EVERY pseudo-state, and the state pairs are declared after the resting
pair. New tests use per-state probes.

### Spinner makeover (scripts/NATIVE_WIDGET_LOOK_REFERENCE_2026_09_28.md)

- Container: `size` x `size`, `position: relative`, no grow / shrink, carries
  `SvgNodeData::ViewBox { 0, 0, D, D }` so clip paths are in its user space,
  and a component `Css` holding the keyframes. Fades in / out through
  `-azul-animation-in` / `-out` (flat 150 ms, flora 420 ms).
- Spokes (flora default): 8 full-size nodes, each clipped
  (`Dom::with_svg_clip_path`) to a capsule D/8 wide from 13/64 D to the rim,
  one per 45 degrees; static opacity ramp 0.55, -0.07 per spoke, 0.06 at the
  tail. Each spoke runs its own phase-rotated `@keyframes
  __azul-spinner-spoke-k` (800 ms, linear, infinite) because `CssDuration`
  is u32 and cannot hold the negative delays CSS would use.
- Ring (flat default): a round-capped 135-degree arc (centre-line 0.4375 D,
  stroke 0.09375 D) under `@keyframes __azul-spinner-spin` 0 -> 360 deg per
  800 ms (450 deg/s); an annulus track below it only when `track_color` is
  set.
- Every animation declaration is gated on
  `PrefersReducedMotion(False)`; with reduced motion the same picture is
  held still (the ramp and the arc are static geometry, not animation).
- Tests (spinner.rs `api_tests` + `makeover_tests`, 28 in total): default
  size, style per theme, `indicator` override, the viewBox, capsule and arc
  geometry, shapes inside the box, track only on request, the opacity ramp,
  one phase per spoke per 800 ms and a continuous wave across the cycle
  boundary, 450 deg/s clockwise, fade in / out, nothing animated under
  reduced motion, inks per theme and mode, caller colour in both modes,
  caller `spinner_style` replaces the container CSS, not a tab stop.

## Tests

- Each widget's `#[cfg(test)] mod theme_tests` (spinner: `makeover_tests`):
  5-12 tests per widget - flat light values unchanged, flora values, dark
  twin after its light half in the same pseudo-state, a visible focus ring in
  all four combinations, structure / a11y / click targets unchanged.
- NEW `layout/tests/flat_and_flora_widgets_follow_the_light_and_dark_theme.rs`
  (appended to `layout/tests/all.rs`): renders every widget in BOTH looks
  under the macOS light and dark presets through the public API only;
  CONTRAST (>= 2:1, no light island at night) and PAIRS (every dark twin has
  its light half earlier, same pseudo-state). One test per widget plus a
  guard `the_pair_walk_reports_a_missing_half_and_a_reversed_pair`.
- Test contracts that CHANGED (review these):
  - color_input: two existing tests updated - the swatch's "extra"
    declaration count is now +2 (the focus halo pair), and "no property
    twice" is checked on unconditional declarations only (the halo adds a
    :focus shadow).
  - spinner: the old test module described the static blue-top-border square
    and is replaced wholesale by `api_tests` + `makeover_tests`.

## API changes (api.json NOT edited - apply via autofix)

All docs are ASCII. `OptionUiTheme` is `repr(C, u8)` over a 4-byte `repr(C)`
enum: size 8, align 4, so appending it after an 8-aligned field adds no
padding. (Badge, Divider, Chip, Alert, Card and Frame already had interior
padding from older small fields; left as they were.)

`theme: OptionUiTheme` APPENDED LAST, plus `set_theme(&mut self, UiTheme)`
and `with_theme(self, UiTheme) -> Self`, on:

| Struct | theme goes after |
|---|---|
| Badge | `badge_style` |
| Label (not in api.json today) | `label_style` |
| Divider | `divider_style` |
| Chip | `container_style` |
| Alert | `container_style` |
| Card | `on_click` |
| Frame | `content` |
| Breadcrumb | `container_style` |
| Accordion | `on_toggle` |
| ColorInput | `accessibility_name` |
| DatePicker | `mode` (W1's `name`, `mode` stay before it; `mode` is a 4-byte enum, so no padding) |

Spinner (breaking):
- NEW `#[repr(C)] enum SpinnerStyle { Auto, Spokes, Ring }` (default Auto).
- Fields reordered to `size: isize, spinner_style:
  OptionCssPropertyWithConditionsVec, indicator: SpinnerStyle, theme:
  OptionUiTheme, color: OptionColorU, track_color: OptionColorU` (align
  8, 8, 4, 4, 1, 1). api.json today: size, color: ColorU, track_color:
  ColorU, spinner_style.
- `color` / `track_color` are now `OptionColorU` (None = native ink / no
  track); `set_color` / `with_color` / `set_track_color` / `with_track_color`
  keep their `ColorU` signatures and store `Some`.
- NEW `set_indicator` / `with_indicator`, `set_theme` / `with_theme`.
- Default size 32 (was 24).

Menubar: NEW Rust-only `#[repr(C)] pub struct Menubar { menu: Menu, theme:
OptionUiTheme }` with `create`, `set_theme`, `with_theme`, `dom`;
`build_menubar_dom(menu)` is now `Menubar::create(menu.clone()).dom()`. Not in
api.json (neither was the function) - add it if the menubar should be public.

## Least sure to compile

1. `spinner.rs`: the import lists (`azul_core` / `azul_css` paths for
   `SvgNodeData`, `SvgMultiPolygon`, `Keyframes`, the animation property
   types), `#[allow(clippy::...)]` attributes on `let` statements inside
   `const fn spoke_opacity_milli`, and the test-only `IdOrClass` import.
2. `themes/decl.rs`: the `azul_css` import paths for the box-shadow /
   border / radius types, and the const-ness of the array-returning builders
   (`border`, `border_bottom`, `border_left`, `radius`, `padding`, `margin`).
3. `chip.rs`: `ChipLook.container` is a plain `fn(ChipKind) -> Vec<..>`
   pointer; the theme functions pass named fns - check coercion.
4. `menubar.rs`: `build(menu, marker, style_bar: impl Fn(Dom) -> Dom,
   style_item: impl Fn(Dom) -> Dom)` - closures passed by reference into the
   recursive `build_item` may need `&dyn Fn` if the recursion trips
   inference.
5. `date_picker.rs` (redo): `build` moves `picker.state` into the shared
   data after cloning `container_style` / `accessibility_name` / `name`;
   the `#[cfg(test)]` wrappers that keep the old builder names
   (`build_header`, `build_weekday_row`, `build_grid`, `build_blank_cell`,
   `build_day_cell`); the tests' `pair` probe compares
   `Vec<PseudoStateType>` with `&[PseudoStateType]`, and `palette_of`
   downcasts a cell payload twice (day, then month); `DayPalette` compared
   with `assert_eq!` against `day_cell_colours` tuples (needs `PartialEq` /
   `Debug` on `StyleBackgroundContentVec`).
5b. spinner test: `crate::window::compile_keyframes_track` is only built
   with the `text_layout` feature (as are the date picker's own harness
   tests).
6. The integration test's `azul_layout::solver3::getters` path and the
   `azul_css::system::{defaults, SystemStyle, Theme}` path.
7. Each RED commit was written to compile against its plumbing commit; the
   spinner RED commit (96f0b26bd) references the most new API, so it is the
   likeliest to need a touch-up if the suite is bisected.

## Left / follow-ups

1. ENGINE GAP (spinner does not move on desktop): `-azul-animation-in`
   tracks are started only by `LayoutWindow::finish_reconciliation`, which
   only the E2E runner calls; the dll desktop shell reconciles on its own and
   never starts in-tracks. Even there, only the ROOT of a newly mounted
   subtree gets a track, and nothing starts on the initial mount
   (`window.rs` ~11914). Fix: start infinite in-tracks for every mounted
   node that declares one, including on the first mount, and have the
   desktop shell call the same begin / finish reconciliation path. The
   widget side is declarative and tested, so it spins as soon as the engine
   does.
2. Ring grow / shrink: Windows 11 animates the arc length; here a fixed
   135-degree arc spins (no width-of-arc track exists to drive a clip path).
3. Box-shadow storage: shadows are stored per side and the painter draws
   every side's slot as a full shadow, so `decl::shadow` emits a single
   `BoxShadowBottom` declaration. A shorthand that fills all four sides would
   compound four shadows - worth fixing in the painter or the parser.
4. api.json: apply the list above via autofix; decide whether `Menubar`
   and `Label` should be exported.
5. The AzWidgets demo (`examples/azul-widgets/`) was not touched; it shows
   no theme switch for these widgets yet.
6. Spinner a11y role unchanged (it is decoration to the keyboard, as
   before); a `progressbar` / busy role is a separate decision.
7. Flora fonts and small caps are approximated (bold + letter spacing; no
   font-variant support used).
8. Accordion: the animated disclosure chevron is still the pre-existing TODO2.
8b. `DateTimeLocalPicker` has its own `theme` but does not forward it to
   the `DatePicker` / `TimePicker` it composes (`datetime_local.rs`
   `dom()`), so a flora datetime-local row holds flat parts. A two-line
   `.with_theme(theme)` on each part; left to W1 (the file is W1's and
   was being edited elsewhere).
9. Merge: W3b appends to the ends of `themes/flat.rs` and `themes/flora.rs`
   in parallel - expect end-of-file conflicts there; keep both sides. The
   only other shared-file touch is the one-line `pub(crate) mod decl;` in
   `themes/mod.rs` and the appended `#[path]` / `mod` pair in
   `layout/tests/all.rs`.
