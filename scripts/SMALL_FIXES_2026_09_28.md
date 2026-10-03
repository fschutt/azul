# Small fixes, 2026-09-28

Branch `wt/small-fixes`, based on 5414bfa6b (PR #476, `fix/input-bugs-2026-09-19`).

Nothing here was compiled or type-checked (no cargo, rustc or LSP). The one
check that did run is `python3 scripts/preflight_contracts.py`: it exits 1 on
the RED commit ca680f505 and 0 on the fix 4950308a4.

Each bug has a RED commit followed by its fix. "Today" is the value on the RED
commit, "expected" the value after the fix.

## 1. CSS values split at whitespace inside parentheses

The crate already had a tokenizer that respects parentheses,
`basic::parse::split_string_respect_whitespace` (used by `transform`). Four
parsers called `str::split_whitespace` instead. Those four are every parser
that splits on whitespace and has a component that can be a function with
inner spaces, such as a colour.

| Commit | What |
|---|---|
| 498370420 RED | New `css/tests/spaces_inside_parentheses_stay_in_one_value.rs`, one test per parser and entry point (see the next table). |
| dfac82e4a fix | `parse_style_box_shadow`, `parse_border_side`, `parse_style_border_color` and `parse_style_scrollbar_color` use the shared tokenizer. |

The REDs:

| Parser | Input | Today | Expected |
|---|---|---|---|
| box-shadow | `0 2px 8px rgba(16, 24, 40, 0.1)` | `Err(TooManyOrTooFewComponents)` | Ok, with the rgba colour |
| box-shadow, whole stylesheet | same value | 0 `BoxShadow*` declarations | 4 |
| text-shadow | spaced rgba | Err | same as the compact spelling |
| `drop-shadow()` | spaced rgba | Err | same as the compact spelling |
| border shorthand | `1px solid rgba(0, 0, 0, 0.1)` | `Err(InvalidDeclaration)` | Ok |
| border-color | two spaced functions | Err | top/bottom and left/right set |
| scrollbar-color | `rgb(255, 0, 0) rgba(0, 0, 0, 0.1)` | `Err(InvalidValue)` | Custom |

The fix changes these existing pins on purpose:

- `scrollbar_color_rejects_functional_colors_containing_spaces` pinned the bug. It is now `..._accepts_...`.
- box-shadow `10px\u{a0}5px` is now an error. The shared tokenizer splits on CSS whitespace (space, tab, LF, CR), and a no-break space is not CSS whitespace.

Covered by the same fix: the four `-azul-box-shadow-*` longhands, `border-top/right/bottom/left` and `column-rule`.

## 2. A bare `auto` was accepted on every property

**Blast radius.** A bare `auto` stays generic on 46 longhands. On every other
property it now goes to that property's own parser.

- **Kept generic (46 longhands):** every longhand whose grammar has `auto`, apart from the `has_typed_auto` ones. Among them are width, height, min-*, the insets, margin-*, z-index, flex-basis, align-self and justify-self, the grid-* ones, column-count/width/fill, break-*, table-layout, cursor, caret-color, background-size and the scrollbar ones.
- **Everything else:** the property's own parser decides. For a grammar without `auto` that means reject, and the declaration is dropped. A custom-ident grammar, such as `font-family: auto`, reads it as a name, as browsers do.
- **Repo usage checked:** `git grep` over rs/css/html/xml/c/py/json finds no `auto` on a longhand whose grammar lacks it. The exceptions are the W3C reftest `doc/xhtml1/c412-blockw-000.xht` (`padding-left: auto`, which exists to check it is ignored) and `image-rendering` (not an azul property).

| Commit | What |
|---|---|
| c0e2cd23e RED | New `css/tests/a_bare_auto_is_only_valid_where_the_grammar_has_it.rs`. `spatial-navigation-function: auto`: today `Ok(CssProperty::auto)`, expected Err. A stylesheet with `grid;` then `auto;`: today `["grid", "auto"]`, expected `["grid"]`. 13 auto-less longhands: today Ok, expected Err. A 25-property control that stays `CssProperty::auto`. |
| 0d73a38fc fix | New `grammar_accepts_bare_auto(key)` allowlist, checked in `parse_css_property`. The pin from 446f70725 in `spatial_nav.rs` now asserts `is_err()`. |

## 3. AppConfig padding

| Commit | What |
|---|---|
| b674fcede RED | `core/src/resources_test.rs` `app_config_has_no_padding_between_its_fields`: `size_of::<AppConfig>()` must equal the sum of its 21 field sizes. Today the difference is 8 bytes (1 + 3 + 4 bytes of padding); expected 0. |
| 209759f00 fix | Fields reordered: the 8-aligned fields in their old relative order, then `log_level`, `natural_scroll`, `termination_behavior`, then `remote_control` (align 2, size 8), then the four bools. |

Nothing depends on the field order:
- `AppConfig::create()` and every other construction use named fields.
- No size or offset is pinned for AppConfig.
- No C host initialises it positionally.

## 4. Windows accent

| Commit | What |
|---|---|
| 752a87e1b RED | New `azul_css::system::windows_accent` in `css/src/system.rs`: text parsers over `reg query` output, so they are testable on every platform. They start with today's logic, moved verbatim. |
| 8fffb732f fix | Parse by value NAME, and write only `colors.accent`. `windows/system_style.rs` drops the `Dwmapi` / `DwmGetColorizationColor` loader, which returned the frame colour. It now queries `HKCU\...\Explorer\Accent` first; that one `reg` call lists `AccentColorMenu` and `AccentPalette`. `HKCU\...\DWM /v AccentColor` is the fallback. |

The REDs:

| Test | Today | Expected |
|---|---|---|
| Explorer listing | `#005FB8` (`StartColorMenu`, the first DWORD in the listing) | `#0078D4` (`AccentColorMenu`) |
| DWM listing | `rgba(0x1c,0,0,0)` (garbage from `Composition 0x1`) | `#0078D4` (`AccentColor`, never `ColorizationColor`) |
| Palette only | None | `#0078D4` (palette entry 3) |
| `AccentPalette` | None | the 7 shades, Light3 to Dark3 |
| Applying the accent | `selection_background` becomes the accent | the selection is unchanged |

A malformed-palette test is a green control.

## 5. AzWidgets demo accessibility

| Commit | What |
|---|---|
| 877c4906b RED | Date picker, `every_day_cell_is_a_button_named_by_its_full_date`: role today `ComboBox`, expected `PushButton`; name today None, expected "Tuesday, 23 June 2026" and "Monday, 1 June 2026". `the_month_navigation_buttons_are_named`: ‹ / › today unnamed, expected "Previous month" / "Next month". Chip, `the_remove_button_is_named_after_the_chip_it_removes`: × today unnamed, expected "Remove Rust". |
| a2da2c573 fix | The day cell's role was `ComboBox`, copied from the field; it is now `PushButton`. `build_grid` adds the name through new `weekday_name` and `day_accessibility_name`; the signature of `build_day_cell` is kept for its tests. The nav buttons are named, and the chip × is named "Remove <label>". |
| ae63b001c RED | Time picker ▲ / ▼: today unnamed, expected "Increase/Decrease hour/minute". ProgressBar: the API was added so the test compiles; today the role node is unnamed in Flat and Flora, expected "Upload". Video `frame_image`: today no accessibility, expected role `Nothing`. `dom_lint`: a node with only lifecycle callbacks (today "[azul][a11y] ... has a callback but no accessible name", expected silent); a named `<button>` (today "role is Unknown", expected silent); a node with only window callbacks (today "role is Unknown", expected silent); a clickable div as a green control. |
| 83c9d22ae fix | `build_spinner` takes a unit; its six test calls pass `"hour"`. `progressbar_render_bar_impl` in Flat and Flora forwards the name. `frame_image` is decorative. `dom_lint`: "interactive" means Hover/Focus callbacks, and an element with a role of its own (button, a, input, textarea, select, option, menu item, progress, meter) is not "role Unknown", matching both accessibility trees. |
| ca680f505 RED | `scripts/preflight_contracts.py` `check_demo_accessibility`. It flags an unnamed Slider, Switch, CheckBox, RadioGroup or ProgressBar builder, and any `create_div_with_text` in the demo. Run today: exit 1, with 5 unnamed widgets and 14 divs. |
| 4950308a4 fix | Demo, see the list below. `preflight_contracts.py` then exits 0. |

The demo fix:
- Section titles are `<h2>`, the page title is `<h1>` and the subtitle is a `<p>`.
- The titlebar texts are `<span>`s, and the bodies (Card, Modal, Accordion, Popover, SplitPane, video note) are `<p>` with `margin: 0`, so the look is unchanged.
- The five widgets are named on their builders.
- New `captioned()`, the caption without renaming. It replaces `labelled()` where `labelled()` overwrote a real name ("Accent colour" became "ColorInput", "Register Cmd+Shift+K" became "Registration") and for the Tooltip's hover-only wrapper.
- The seek bar is `AccessibilityInfo::named("Seek", Slider)`, with the time as its value.

How the reported findings map to fixes:

| Finding | Fix |
|---|---|
| div-as-text | the demo |
| "icon-only control, node 318" | The VideoWidget root: two lifecycle listeners, no text. Fixed in the lint. |
| Slider / Switch / CheckBox / RadioGroup | named at the call site |
| Date-picker PushButton nodes | The actual PushButtons were the ‹ / › buttons; the day cells were `ComboBox`. Both fixed. |
| Progress | new builder name, forwarded into the VirtualView |
| Image | the video frame, now decorative |
| Interactive node with role Unknown | The drop zone (window listeners) and the video overlay/toggle (named `<button>`s) are fixed in the lint; the Tooltip wrapper and the seek bar are fixed in the demo. |

## API and layout changes (for autofix; `api.json` was not edited)

1. **`AppConfig`: repr(C) field order changed and the struct is 8 bytes smaller.** `api.json` `struct_fields` must be re-synced to the new order.
2. **`ProgressBar`: new field `accessibility_name: OptionString`**, placed between `container_background` and `theme`, which changes its size.
   - New builder `ProgressBar::with_accessibility_name<S: Into<AzString>>(self, S) -> Self`.
   - The dll's transmutes and memtest will fail until `api.json` is synced.
   - The demo calls `ProgressBar::with_accessibility_name`, so it will not compile until autofix and `codegen all` have run.
3. **New public module `azul_css::system::windows_accent`**: `EXPLORER_ACCENT_KEY`, `DWM_KEY`, `AccentPalette` (not repr(C)), `accent_from_reg_query`, `accent_palette_from_reg_query` and `apply_accent`. It is a Rust-side helper for the Windows shell, and I would keep it out of `api.json`.
4. No other public signature changed. `build_spinner`, `frame_image`, `day_accessibility_name`, `weekday_name`, `grammar_accepts_bare_auto` and the new `dom_lint` helpers are all private.

## Least sure to compile

- `css/src/system.rs`, `windows_accent::apply_accent`: a `const fn` that assigns through `&mut SystemStyle`, and `SystemStyle` implements `Drop`. It needs `const_mut_refs`, stable since 1.83; only a Copy field is assigned, so nothing is dropped. If it is refused, make it a plain `fn`; clippy nursery would then ask for const.
- `examples/azul-widgets/src/video.rs`: `AccessibilityInfo::named("Seek", AccessibilityRole::Slider).with_value(time_text(&status))` passes a std `String` into the generated `with_value`. `create_span_with_text` takes the same `String` argument type in this file, so it should be accepted. The new import is `azul::dom::{AccessibilityInfo, AccessibilityRole}`, the path `examples/rust/src/async.rs` uses.
- `examples/azul-widgets`: `ProgressBar::with_accessibility_name` needs regenerated bindings (API change 2 above).
- `layout/src/widgets/themes/{flat,flora}.rs`: `accessibility_name: this.accessibility_name.clone()` inside `progressbar_render_bar_impl`. `this.bar_background` is moved later, in the children, which is a disjoint field.
- `css/tests/*.rs`, the two new integration test files: every type is used through public paths (`azul_css::props::style::{border, box_shadow, scrollbar}`, `props::basic::color::parse_css_color`, `css::{Css, CssDeclaration}`), but the paths are unverified.
- `layout/src/dom_lint.rs`: `const fn element_implies_a_role(&NodeType)` matches on a reference with unit-variant patterns.

## Open items

- **Shorthands still intercept a bare `auto`** (`parse_combined_css_property`): `padding: auto`, `border: auto` and `gap: auto` are accepted. `parse_layout_padding` also accepts `auto` per side; that divergence is pinned in `spacing.rs`. Two more problems sit there:
  - `flex: auto` becomes the generic `auto` on grow, shrink and basis instead of `1 1 auto`.
  - `flex: none` never reaches its own arm (the generic `none` intercept runs first).
- **`unset` / `revert`** are not handled as CSS-wide keywords at all; they fall through to the property parsers and are rejected.
- **The Windows accent palette** is parsed but not stored: `SystemColors` has no slot for it. The reference doc's `system:accent-dark-1` / `system:accent-light-2` need new fields and new `SystemColorRef` tokens (an API change). `UISettings` itself (WinRT) is not called; the registry holds the same values.
- **Widget builder names that are never read**, found by the survey:
  - `flat::text_input` takes its name only from the placeholder, so `TextInput` / `NumberInput` `::with_accessibility_name` do nothing. ColorInput's hex and R/G/B/A fields are unnamed as a result.
  - The `accessibility_name` fields of `ComboBox` and `TimePicker` are never read in `dom()`.
- **The demo's document tabs** have callbacks and text but no role or `tab_index`. The lints are silent about them, but they cannot be reached from the keyboard.
- **The box-shadow comma list** (several shadows) is still unsupported, and so is form feed as CSS whitespace in the shared tokenizer.
- **The other `split_whitespace` sites** (flex, gap, the insets, overflow, background-position/size, text-*, filter internals, shapes) take only keywords and plain lengths, so they cannot contain a spaced function today. They would need the shared tokenizer if `calc()` or `var()` ever reach them.
