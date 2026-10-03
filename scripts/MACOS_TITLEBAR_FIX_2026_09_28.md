# macOS titlebar and bold system font: fix report (2026-09-28)

Branch `wt/macos-titlebar`, cut from `5414bfa6b` (PR #476, `fix/input-bugs-2026-09-19`).
Nothing was compiled or type-checked. Every expected value below is derived by hand
and has not been measured. The parent compiles and runs the wave.

User report (macOS): the custom titlebar "looks weird": it is almost the Windows
titlebar's height, it doesn't use `system:ui:bold`, it isn't centred, and the
background and border-bottom should be configurable.

Reference: `scripts/NATIVE_WIDGET_LOOK_REFERENCE_2026_09_28.md`, sections 0, 4.1 and
5.3, and P0 items 1-3.

## Commits, in order, with the RED each fix turns green

| Commit | What | RED today → expected |
|---|---|---|
| `080968b40` | **RED fonts** | css `a_bold_system_font_on_macos_is_the_system_font_first`: `UiBold`/`TitleBold` chain[0] on MacOs is `"Helvetica Neue"` today, expected `"System Font"`. css `the_macos_title_is_bold_like_the_titlebarfont`: `TitlebarMetrics::macos().title_font_weight` is `Some(600)` today, expected `Some(700)`. layout `a_bold_request_on_a_variable_font_file_draws_its_700_instance`: `doc/fonts/RedHatMono-VariableFont_wght.ttf` is registered as a FILE and resolved at Bold; the drawn face's weight is `Normal` today, expected `Bold`, and its bytes must equal `bake_weight_instance(file, 0, 700)`. This test runs on every host. layout `the_bold_system_title_face_on_macos_is_the_system_font_at_700` (skips without `/System/Library/Fonts/SFNS.ttf`): the first matched group is `"Helvetica Neue"` today, expected `"System Font"`, with its face at `Bold`. |
| `fe402914c` | **FIX fonts** | Turns the four above green. |
| `9e30ba3a5` | **RED titlebar centring**, new `layout/tests/the_macos_titlebar_lines_up_with_its_traffic_lights.rs` | `the_injected_macos_title_sits_on_the_traffic_lights_line`: the line-box centre y is about 14.7-14.9 today (padding-top 7 plus half of a 15.3-15.9px line), expected 14 ± 0.5. `a_csd_titlebars_title_sits_on_the_line_of_its_controls`: the title is at y = 17.5 and the controls at 14 today (font-independent), expected \|Δ\| ≤ 0.5. Guards that pass today: `the_injected_macos_titlebar_is_28px_tall` and `the_injected_macos_title_is_centred_on_the_window` (text-run centre x = 240 ± 0.5). |
| `00ee8a43b` | **FIX titlebar centring** | Turns both REDs green. |
| `5ea0f49f8` | **RED separator** | `the_macos_titlebar_has_no_fill_and_the_system_separator`: `from_system_style(macOS light/dark).dom()` should declare, unconditionally, a solid border-bottom of #D0D0D0 / #000000 with width > 0 and no BackgroundContent. Today there is no border (colour `None`). |
| `15cc3df99` | **FEAT/FIX background + separator** | Turns it green. Also adds the unit test `the_builders_set_the_background_and_the_line_under_the_bar`. |
| `0afefe5dd` | **RED demo** (same test file; `page_frame()` is now `pub(crate)` and captures the label style) | `the_demo_titlebar_is_28px_tall`: 39 today (38 plus a 1px content-box border), expected 28. `the_demo_title_is_centred_on_the_window`: about 150 today (left-aligned after 82px), expected 240 ± 0.5. `the_demo_title_sits_on_the_traffic_lights_line`: 19 today, expected 14 ± 0.5. |
| `a77a16309` | **FIX demo** | Turns the three green. |
| `6f41cf5a5`, `d9a37780d`, `b831d9868`, `e58354ce9` | progress checkpoints | This commit removes the progress file. |

Where the separator is on, the "after" values are 13.75 for the title centre y: the
0.5px line sits inside the 28px border box. That is within the 0.5 tolerance.

## What changed

### 1. Font: `system:ui:bold` / `system:title:bold` is SF at 700

- **Root cause.** `SFNS.ttf` ("System Font") is one variable font: `wght` runs from
  1 to 1000, with Bold at 700. rust-fontconfig 5.0 indexes the file once, at its
  default instance (400). A bold request therefore drew SF *regular*. That is why
  the macOS bold chain had been routed to Helvetica Neue.
- **`layout/src/solver3/getters.rs`** gains three items:
  - `select_variable_weight_instances(chain, weight, fc_cache)` looks at the best
    face of each CSS group.
  - `variable_weight_instance(fc_cache, face, weight) -> Option<FontMatch>`. If
    `face` is a font FILE (not a memory font) at another weight, and its `wght`
    axis spans the request, it bakes the static instance with the existing
    `font::parsed::bake_weight_instance`. It registers the instance in the cache
    with `with_memory_font_with_id`, under the source's family at the requested
    weight.
  - A private `enum WeightInstance { Baked, Absent, UnknownFace }` and
    `bake_weight_instance_into`.

  The answer for each `(source FontId, weight)` is memoised process-wide. rust-fontconfig
  memoises chains, so the source face keeps coming back. The memo also covers the
  case where rust-fontconfig would de-duplicate a second registration.
- **Call sites.** `resolve_font_chains_with_registry` and `resolve_font_chains_fast`
  call it before `chains.insert`. So does `text3::cache::resolve_chain_on_miss`
  (the shaping-time miss path).
- **`css/src/system.rs`.** The macOS `Ui | UiBold | TitleBold` chains are now all
  `[System Font, Helvetica Neue, Lucida Grande]`. `TitlebarMetrics::macos()
  .title_font_weight` goes from 600 to 700.

### 2. Titlebar widget (`layout/src/widgets/titlebar.rs`)

- **Height.** It was already 28 on macOS (`TitlebarMetrics::macos()` and
  `DEFAULT_TITLEBAR_HEIGHT`). It is now guarded by a laid-out test. The 38 was the
  demo's height.
- **Vertical centring.** The container is flex in both modes:
  - title-only: `flex-direction: column; justify-content: center`. The title block
    keeps the full width through cross-axis stretch, so `text-align: center` still
    lands on the window's middle.
  - CSD: `row; align-items: center`, unchanged.

  The title's `padding-top: (h - fs) / 2` is gone.
- **Horizontal centring.** Title-only mode keeps its symmetric padding, so the
  title is on the window's centre line. This is guarded by a test.
- **Background and separator.**
  - New fields and builders (see API).
  - `build_container_style` emits `box-sizing: border-box` and a solid
    `border-bottom`, so the bar's height includes the line.
  - The default macOS colour gets a dark twin (#000000).
  - An unfocused window uses the `:backdrop` colour.
  - `dom_controls_only` (the `NoTitle` overlay on Linux) draws no line.
- **macOS defaults.** There is no fill: `background_*` stays `None`, so the window
  shows through, as it does behind a transparent native bar. The separator comes
  from the presets: `defaults::macos_modern_light/dark()` set
  `metrics.titlebar.separator_color` to #D0D0D0 / #000000, and
  `TitlebarMetrics::macos().separator_width` is 0.5px. `Titlebar::new` on macOS
  uses the same values, with the dark twin.
- **Generated tests** updated to the new shape:
  - `expected_container` and `expected_title` changed.
  - Three padding-top pins are replaced by
    `build_container_style_centres_the_title_on_the_midline_in_both_modes`,
    `build_title_style_leaves_the_vertical_centring_to_the_bar` and
    `the_title_style_does_not_depend_on_the_bar_height`.
  - The container "unconditional" check allows the dark twin.

### 3. AzWidgets demo (`examples/azul-widgets/src/lib.rs`)

- **Bar.** 28px `border-box`, no fill, `border-bottom: 0.5px solid system:separator`,
  symmetric 78px padding, `position: relative`.
- **Title.** `font-family: system:title:bold`, flex `1 1 0`, `text-align: center`,
  nowrap, clipped.
- **Label.** "custom titlebar" is `position: absolute` at the right with
  `line-height: 28px`, so it cannot push the title off centre.
- **Theme tests.** The literal order that `azul_widgets_demo_follows_the_theme.rs`
  anchors on is unchanged, and every colour is still a `system:` keyword. The
  palette comment is updated.

## Public API changes (api.json NOT edited; run autofix/codegen)

`Titlebar` (repr(C); three fields appended at the end):
- `separator_color: OptionColorU`
- `separator_color_inactive: OptionColorU`
- `separator_width: f32`

New `Titlebar` methods (all `const fn`):
- `set_background(&mut self, OptionColorU)`
- `with_background(self, ColorU) -> Self`
- `with_background_inactive(self, ColorU) -> Self`
- `set_border_bottom(&mut self, f32, OptionColorU)`
- `with_border_bottom(self, f32, ColorU) -> Self`
- `with_border_bottom_inactive(self, ColorU) -> Self`
- `without_border_bottom(self) -> Self`

`Titlebar::dom_controls_only` takes `mut self`. The signature does not change.

`TitlebarMetrics` (repr(C); three fields appended at the end):
- `separator_color: OptionColorU`
- `separator_color_inactive: OptionColorU`
- `separator_width: OptionPixelValue`

Behaviour changes to existing API:
- `TitlebarMetrics::macos()`: `title_font_weight` 700 (was 600); `separator_width`
  `Some(0.5px)`.
- `defaults::macos_modern_light/dark()`: `metrics.titlebar.separator_color` is set.
- `SystemFontType::get_fallback_chain(MacOs)` for `UiBold`/`TitleBold` returns
  `[System Font, Helvetica Neue, Lucida Grande]`.

New `azul-layout` Rust API, not in api.json:
- `solver3::getters::select_variable_weight_instances`
- `solver3::getters::variable_weight_instance`

## Least sure to compile (look here first)

1. **`getters.rs` `variable_weight_instance`.** It uses `static INSTANCES:
   Mutex<BTreeMap<(FontId, u16), Option<FontId>>> = Mutex::new(BTreeMap::new())`.
   This needs `const` `Mutex::new`/`BTreeMap::new`, and `FontId: Send`. It also
   has the closure `as_match`, which borrows `face` and is later moved into
   `.map(as_match)`. The file imports `rust_fontconfig::FontId` at module level,
   around the old line 5392; the new code relies on that import.
2. **`bake_weight_instance_into`** uses these rust-fontconfig 5.0 APIs:
   - `FcFontCache::{is_memory_font, get_metadata_by_id, get_font_by_id,
     get_font_bytes, with_memory_font_with_id, for_each_pattern}`
   - `OwnedFontSource::{Disk, Memory}`
   - `FcFont { bytes, font_index, id }`
   - `FontId::new()`
   - `FcPattern: PartialEq + Clone`

   Also check the `let ... else` blocks, which are split over lines.
3. **Titlebar `const fn` builders** that take `mut self` (the struct holds an
   `AzString`). Card's `with_flex_grow` does the same, but check for E0493.
4. **Glob-resolved names in `titlebar.rs`.** These come through `layout::*` /
   `style::*` / `basic::*`: `LayoutBoxSizing`, `LayoutBorderBottomWidth { inner:
   PixelValue::px(..) }`, `StyleBorderBottomStyle`, `StyleBorderBottomColor` and
   `BorderStyle`. The test module reaches them through `use super::*`.
5. **Test files.**
   - `a_bold_request_...rs`: `rust_fontconfig::FcParseFontBytes`,
     `FcFontCache::insert_fast_pattern` and `FcFontPath { path, font_index,
     bytes_hash }`.
   - `the_macos_titlebar_...rs`: a `static OnceLock<FcFontCache>` (FcFontCache must
     be Sync), `const DEMO_BAR: NodeId = NodeId::new(1)`, and
     `materialized_inline_layout_for_node(..).bounds()`.
   - `crate::azul_widgets_demo_follows_the_theme::page_frame()` is called across
     sibling test modules.
6. **`css/src/system.rs` presets.** They use `TitlebarMetrics { separator_color: ..,
   ..TitlebarMetrics::macos() }` inside `pub mod defaults`.

## Open items

- **Can allsorts bake SFNS?** SFNS has 4 axes, `avar`/`MVAR`/`HVAR`/`trak`, and a
  7 MB `gvar`. If `variations::instance` fails, the SFNS test stays red, and bold
  system text renders SF *regular* (the chain no longer falls to Helvetica Neue).
  In that case either fix the bake, or drop a variable face whose bake failed from
  a bold group.
- **Bake cost.** One bake per (face, weight) per process, on the layout thread, at
  first use. For SFNS it could be about 0.1-0.5 s (estimated, not measured), which
  hits the first frame of a window with a bold system title. Measure it, and
  consider persisting baked instances next to the font cache.
- **Optical size.** The baked instance keeps `opsz` 28 (Display). AppKit uses the
  Text optical size at 13pt, and applies `trak`. Bake `opsz` = clamp(size, 17, ..)
  per size bucket as a follow-up (reference section 5.3).
- **Any disk variable font is now drawn at the requested weight**, on every
  platform. Tests that pinned bold widths measured from a default instance could
  move.
- **Title-only padding** is still `button_area / 2 + padding` per side (47px on
  macOS). A long title can therefore run under the traffic lights (x 8-60).
  Reserving the full button area on both sides would fix it; not changed here.
- **`create_csd_stylesheet`** still hard-codes `border-bottom: 1px
  rgb(200,200,200)` / `(60,60,60)`. The widget's inline separator overrides it when
  set. The stylesheet should read `TitlebarMetrics.separator_*`.
- **Window-level option.** There is none yet for the auto-injected bar
  (`inject_software_titlebar`) to take a background or separator from. Apps that
  want a different edge must build their own `Titlebar`.
- **Titlebar backgrounds.** A `system:titlebar-background` / `-separator` token,
  and runtime traffic-light geometry from `standardWindowButton` frames (macOS 26:
  14pt lights at a 23pt pitch), remain P2 in the reference.
- **The demo's bar is 28px on every OS.** On Linux the software `NoTitle` controls
  overlay (about 35px on GNOME) may overhang it. The demo could pick its height per
  OS, but the theme tests read the style as one literal.
- **`title-only` on Windows.** It is centred everywhere. Windows 11 left-aligns its
  title (`title_align` in the reference). That waits for the later `@theme` /
  native-theme work, as agreed.
