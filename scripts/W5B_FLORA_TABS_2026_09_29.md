# W5b - flora looks and a theme option for tabs, titlebar, tree_view (2026-09-29)

Branch `wt/w5b-flora-tabs`, cut from `fix/input-bugs-2026-09-19` @ 0a326afe5. Nothing
was compiled (house rule). Every widget went plumbing -> RED -> flora, with a
checkpoint between widgets. api.json was NOT touched - the list is in section 4.

**The build waits for U1.** Every unpinned look is merged with
`crate::widgets::themes::theme_blocks::follow_props`, called by that exact path as
the task asked. On 0a326afe5 the helper is still `themes::flat::follow_props`
(T3); U1 moves it. Expected signature (T3's):
`follow_props(&[CssPropertyWithConditions], &[CssPropertyWithConditions]) -> CssPropertyWithConditionsVec`.

## 1. Shape

Each widget keeps ONE structure builder. It keeps the nodes, classes,
datasets, callbacks, roving Tab stop, `on_node_toggle`, the arrow keys and the
accessibility info. The builder takes a `<Widget>Look` (pub(crate)): one style
per part plus an `Option<&'static str>` marker class. Flat has no marker, as in
W3a's widgets; flora carries `__azul-theme-flora`.

- `themes::flat::<x>_look()` is the established look, byte-for-byte. It is
  built from the widget's existing const styles or builders, which became
  `pub(crate)`.
- `themes::flora::<x>_look()` is the new look. Both are appended under
  `// ==== <widget> ====` banners.
- With `theme: None`, `<Look>::of()` merges the flat and flora looks part by
  part with `follow_props`. It takes the marker of `UiTheme::current()` (the
  structure theme). This is T3's "skin merge" shape: the DOM is built ONCE, and
  the caller's content (the tab panel's body, the tree's labels) is never
  cloned or built twice.
- Pinned: that theme's look, with no `@theme` condition.

| widget | Look | parts |
|---|---|---|
| tree_view | `TreeViewLook` | container, row, row_selected, children, icon, icon_selected, leaf_spacer, label, label_selected |
| tabs | `TabHeaderLook` / `TabContentLook` | header, before, after, active, before_active, after_active, inactive / padded, unpadded |
| titlebar | `TitlebarLook` | container, title, button, close |

Titlebar metrics: `Titlebar::container_style_painted` / `title_style_painted`
build the platform metrics once for every look:

- display and direction, centring, height;
- whether there is a line, its box sizing, width and style;
- cursor, drag region, user-select, padding;
- the title's `system:title:bold` font at the platform size, min-width, alignment, nowrap and clip.

The look only passes its paint: background, ink, line colour, title ink.

`build_container_style` / `build_title_style` are those builders with the
flat (native) paint, so their output is unchanged, and the existing autotests
still call them. `build_button_container` (flat controls) is `#[cfg(test)]`
now; the bar builds its controls through `button_container_painted(&look)`.

## 2. The flora looks

- **tree_view**
  - The tree is a sheet of field paper (`--fl-fld`, night field) in a `--fl-bd2` hairline, 3px radius, 3px inset.
  - Text is flora ink; the chevrons are `--fl-icon`.
  - Rows wash to `--fl-hov` under the pointer (the radio row's wash constants are reused) and sink to the pressed face.
  - The selected row is the sunken accent stone (`selected_stone()`). Its label and chevron are in `--fl-on-acc`, the same colour in both modes.
  - An open parent's children hang from a 1px `--fl-sep` guide rule under the chevron. The indent stays 16px.
  - Every row gets an inset focus ring: accent by day, glow by night and on the stone.
  - The leaf spacer is the flat const, so the 16px icon column is shared.
- **tabs**, after flora.css's navigation strip:
  - The header is raised chrome (`--fl-rT` over `--fl-rB`, with its night face), system:ui at 13px. Tabs sit on its foot, 8px in.
  - A 2px rule of `--fl-metal-turn` (#C6B279, new `flora::TAB_METAL`) runs under both spacers and every unselected tab.
  - Unselected tabs: `--fl-soft1` ink, 4px shoulders and an invisible 1px edge on three sides.
    - Hover: the hover face, a `--fl-bd` edge and `--fl-ink` ink.
    - Pressed: the pressed face with a `--fl-bd3` edge.
    - The rule at the tab's foot never changes.
  - The selected tab is the sunken stone in `--fl-on-acc`, with a 2px metal surround on three sides and 6px shoulders. It has no foot, so it breaks the rule.
  - Labels share a baseline: 4px plus the 2px rule under an unselected label, 6px under the selected one.
  - There are no seams, so the tabs beside the selected one use the unselected style.
  - Every tab gets an inset focus ring, because the arrow keys can focus any of them.
  - The panel is a `--fl-sur` leaf in a `--fl-bd` hairline on three sides, open at the top, with 3px corners at its foot. Padded, it has 10px of padding.
- **titlebar**, flora's window chrome (docs-guide.css `.azul-titlebar`):
  - The band runs `--fl-ct` -> `--fl-cb` (warm slate by day, #383838 -> #262626 by night), with the chrome ink #F2F2F2 in both modes.
  - A `--fl-bd5` line is drawn wherever the platform bar has a line.
  - The title dims to #B9B5AB under `:backdrop`.
  - Minimize and maximize wash in a 15% chrome-ink tint and sink to the band's foot when pressed.
  - Close turns to the clay stone (its deep face when pressed), with the glyph in `--fl-on-acc`.
  - The metrics stay the platform's; see section 1.

## 3. Commits

| widget | plumbing | RED | flora |
|---|---|---|---|
| tree_view | 068155ead | ecd7c8b92 | cb2fcd9c1 |
| tabs | 79e0dc64e | e48d6d8bc | e53a85308 |
| titlebar | 8f23902c4 | 715aa42ad | da220f323 |

Checkpoints: 38de49d26, fc925501f, plus the commit with this report.

## 4. api.json list (for autofix)

Every `theme` field is `OptionUiTheme` (8 bytes, 4-aligned), appended LAST.

| class | item | spelling |
|---|---|---|
| TreeView | struct field (LAST, after `on_node_toggle`) | `theme: OptionUiTheme`, doc "The widget theme this tree is PINNED to (`with_theme`), or `None` to follow the app theme (`AppConfig::with_theme`, `CallbackInfo::set_theme`; flat unless the app chose another)." |
| TreeView | fn `set_theme` | `[{"self": "refmut"}, {"theme": "UiTheme"}]`, body `object.set_theme(theme)`, doc "Pin the widget theme: the tree keeps this look whatever the app theme is. Unset (`None`), it follows the app theme." |
| TreeView | fn `with_theme` | `[{"self": "value"}, {"theme": "UiTheme"}]` -> `TreeView`, body `object.with_theme(theme)`, doc "[`Self::set_theme`] for the builder chain." |
| TabHeader | field (LAST, after `on_click`) + `set_theme` / `with_theme` | as TreeView; returns `TabHeader`; field doc says "tab bar" |
| TabContent | field (LAST, after `has_padding`) + `set_theme` / `with_theme` | as TreeView; returns `TabContent`; doc says "panel" |
| Titlebar | field (LAST, after `separator_width`) + `set_theme` / `with_theme` | as TreeView; returns `Titlebar`. The field doc adds: "A theme decides the bar's paint - its fill, ink, line colour and the controls' hover faces - never its metrics: height, font, padding, centring, the line's width and the drag region are the platform's in every theme. The colour fields above are the FLAT look's (the native one, filled from the desktop by `from_system_style`); flora draws its own window chrome." |

How each struct grows:

- **TreeView and TabHeader**: every existing field is 8-aligned, so each grows by exactly 8 bytes with no padding.
- **Titlebar**: the new field lands at the old tail padding plus 4. It adds 8 bytes and no new interior padding. The 1 pad byte after the seven `OptionColorU`s was already there.
- **TabContent**: with `theme` LAST after `has_padding: bool`, there are 3 bytes of interior padding. That breaks the "decreasing alignment" rule, but the task said LAST. Moving `theme` before `has_padding` gives the same size (+8) and no interior padding, but it moves `has_padding`'s offset. Your call.

Docs changed (autofix syncs them): `TreeView::dom` and the Titlebar struct doc.
`TabContent::new` stays `const`.

Rust-only (not FFI):

- `pub(crate)`:
  - the Look structs and their `of()`;
  - `tree_view::{TREE_CONTAINER_STYLE, ROW_STYLE, ROW_SELECTED_STYLE, CHILDREN_STYLE, ICON_STYLE, LEAF_SPACER_STYLE, LABEL_STYLE}`;
  - nine `tabs::CSS_MATCH_*` vec consts;
  - `Titlebar::{build_container_style, build_title_style, container_style_painted, title_style_painted}`;
  - `titlebar::flat_control_hover`;
  - `flat::` / `flora::{tree_view_look, tab_header_look, tab_content_look, titlebar_look}`.
- `pub const` in `themes::flora`, flora palette tokens like the existing `LIGHT_*` ones: `TAB_METAL`, `LIGHT_CT`, `LIGHT_CB`, `DARK_CT`, `DARK_CB`, `CHROME_INK`, `CHROME_INK_DIM`, `CHROME_HOVER`.

## 5. Tests

**Unit tests** (`#[cfg(test)] mod theme_tests` in each widget file):

- a fresh widget follows (`None`), and `set_theme` / `with_theme` agree;
- the flora look's specific colours, in both modes and per state;
- `theme_checks::assert_theme_invariants` on flora: whole pairs, no shadowed states, every focusable node ringed in both modes;
- both looks build the same nodes, classes, handlers, datasets and a11y outline;
- titlebar only: `a_flora_titlebar_keeps_every_platform_metric`. It compares 13 bar metrics and 6 title metrics between flat and flora, for 5 bars (macOS, GNOME CSD, Windows, `new`, `new` with a line), in all 5 shapes, in both modes.

**Integration**, `layout/tests/widgets_follow_the_app_theme.rs` (T3's `assert_follows_the_app_theme`):

- `tree_views_follow_the_app_theme`
- `tab_bars_and_their_panels_follow_the_app_theme`
- `titlebars_follow_the_app_theme_in_every_shape`

My three widgets left the single-look guard. It now lists ribbon, quick_access and statusbar (W5a's), and its doc says so.

**Integration**, `flat_and_flora_widgets_follow_the_light_and_dark_theme.rs` (pair lint and contrast):

- `flora_tree_views_read_in_both_themes`
- `tab_bars_and_their_panels_read_in_both_themes_in_both_looks`
- `titlebars_read_in_both_themes_in_both_looks`

The flat tree's labels keep their parked half-pairs (`widgets::theme_pairs::KNOWN_HALF_PAIRS`, paths `root/0/1` and `root/1/0/1`). Those entries still match, now inside `@theme(flat)`, so the flat tree is not walked in this file.

**Pinned to flat** (they compare rendered styles with the flat consts):

- tree_view: 5 autotests, through a `flat_dom` helper;
- tabs: 5 autotests;
- titlebar: 2 autotests.

`the_macos_titlebar_has_no_fill_and_the_system_separator` (a layout test outside my files, minimal edit) now reads the injected bar at rest under the default app theme: unconditional declarations plus `@theme(flat)`. Before, it read only the unconditional ones.

Commands for the parent (after U1 is merged):

```
cargo test -p azul-layout --lib widgets::tree_view widgets::tabs widgets::titlebar
cargo test -p azul-layout --lib widgets::theme_pairs widgets::theme_contrast widgets::label_convention
cargo test -p azul-layout --test all widgets_follow_the_app_theme
cargo test -p azul-layout --test all flat_and_flora_widgets_follow_the_light_and_dark_theme
cargo test -p azul-layout --test all the_macos_titlebar_lines_up_with_its_traffic_lights
cargo test -p azul-dll --test headless_decoration_flip
```

## 6. Least sure to compile

1. `theme_blocks::follow_props`: the path and signature are U1's. Four `of()` constructors call it as `follow_props(x.as_ref(), y.as_ref())`.
2. `TabContent::dom`: after moving `look.padded` / `look.unpadded` in an if/else, it reads `look.marker` (a Copy field of a partially moved local).
3. `let part = CssPropertyWithConditionsVec::from_const_slice;` (flat tree) and `::from_vec` (flora): impl_vec associated fns used as local fn values.
4. The integration titlebar fixtures, `[(&str, fn() -> Titlebar); 3] = [("create", create), ...]`: fn items coerced inside tuples in a typed array.
5. `P::with_single_condition(kit::ink(..), &[DynamicSelector::PseudoState(PseudoStateType::Backdrop)])` in flora; titlebar.rs already does the same.
6. `with_on_node_click(RefAny::new(()), pick as TreeViewOnNodeClickCallbackType)` and the TabHeader twin, relying on `From<fn>` for the callback wrappers.
7. `tc::background(..).as_ref().and_then(tc::bg_color)`: a fn item passed to `and_then` on `Option<&CssProperty>`.

## 7. Guesses

- **Titlebar colour fields.** Flora ignores the desktop's colour fields (`title_color`, `background_color`, the `*_inactive` colours, the hover colours). It draws flora's chrome instead. The fields stay the flat (native) look's. Flora does keep the line's presence (`separator_color.is_some()`) and its width.
- **Titlebar band.** I used `--fl-ct` / `--fl-cb` (flora's "window chrome" tokens, used by the docs' mock titlebar), not the light toolbar strip the flora menubar uses. The ink `#F2F2F2`, the backdrop dim `#B9B5AB` and the 15% hover wash are my values. Only #F2F2F2 is in the CSS.
- **Titlebar controls.** Close turns to `STONE_CLAY` (the mock's close light); minimize and maximize keep the platform glyphs. The mock's traffic-light gems are not drawn, because on macOS the OS draws the lights.
- **Tabs rule.** The 2px rule uses one colour, `--fl-metal-turn`. flora.css's rolled gradient and its corner assembly (flare, cove, run-out: radial masks) are not drawn.
- **Tab placement.** Flora tabs are left-aligned (the before-spacer is 8px wide). Flat's spacers both grow, so flat centres its tabs. Flora tab heights follow the label instead of flat's fixed 21 / 23px.
- **Tab panel.** Flora's panel is a `--fl-sur` leaf, not flora.css's dark "band": app content with dark ink has to stay readable on it. The unpadded panel has no hairline, parallel to flat's unpadded panel.
- **Tree.** The guide rule under open parents, the 3px row radius and the reuse of the radio-row `--fl-hov` constants (`RADIO_GROUP_HOVER_*`) are my choices.
- **Marker class.** Flat gets no marker class (W3a's convention, e.g. menubar), so the flat class lists the autotests pin stay exact.

## 8. Found, not fixed

- **Twins:**
  - `themes::decl` and `themes::style_kit` are two builder kits for the same concerns: padding, radius, themed fill/ink/layers, hover/active layers, borders, focus halo, shadows. I used `style_kit` only.
  - T2's `theme_blocks::every_theme_dom` and T3's `flat::follow_*` are the pair U1 is deduping.
- **Flat title `:backdrop` colour.** It is pushed BEFORE the resting title colour (`flat_title_ink`), so under last-match-wins the dimmed title never applies. The comment claims the reverse. The container's `:backdrop` line colour is ordered correctly. I left flat byte-for-byte.
- **Flat focus rings.** Flat tabs and flat tree rows are keyboard stops with no focus ring. Flora rings them. Flat would need the W3a treatment (a ring appended to the const styles, and the counting autotests updated).
- **C widgets demo.** `examples/c/widgets.c` pins its other widgets with `withTheme(theme)` but not the tree view. After autofix, `AzTreeView_withTheme(tv, theme)` would make it follow the demo's switch.

## 9. Left

- Build and run the suites once U1's `theme_blocks::follow_props` is in.
- Autofix api.json from section 4, and decide the TabContent field order.
- Merge with W5a:
  - both edit the single-look guard in `widgets_follow_the_app_theme.rs`; after both, the guard is empty and can go;
  - both append at the ends of `flat.rs`, `flora.rs` and the two test files; keep both sides.
