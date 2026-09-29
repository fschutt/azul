# I1 - icon metadata, the capability-aware resolver, SVG recolouring, rank, remap rules (report)

Branch `wt/i1-icon-metadata`, cut from `fix/input-bugs-2026-09-19` @ `0a326afe5`.
Design: `scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md` sections 8, 8.1, 8.2, decided (h)
and (i), 9.1 pitfalls 5, 8 and 10. Nothing was compiled (house rule); section 4 lists the spots I am
least sure of, section 5 the exact commands.

## 1. What was built

### 1.1 Metadata and the capability-aware default resolver
- `core/src/icon.rs`: `IconMeta { variants, recolor, designed_for, monochrome }` with
  `IconDesignedFor { Any, Light, Dark }` (a MODE, never a theme), `IconVariants { light, dark,
  high_contrast }` (icon SPECS), `IconRecolor { CurrentColor, Mask, Palette(IconColorMappingVec),
  Fixed(IconModeColors), None }`, `IconColorMapping { from, to }`, `IconModeColors { light, dark }`.
  `Fixed` is an extension of the design's four: it is the remap format's explicit colour
  (`recolor: "#e6e6e6"` / `{light, dark}`), which pitfall 10 says beats the CSS `color`.
  Defaults: `IconMeta::for_font()` = CurrentColor + monochrome, `for_image()` = None,
  `for_mask()` = Mask + monochrome.
- The metadata lives ON the registered data (`ImageIconData::meta`, `FontIconData::meta`,
  `SvgIconData::meta`), so a custom resolver reads it where it reads the artwork.
- `layout/src/icon.rs` default resolver, request x capability (never guessing from the kind):
  - the variant for the mode first (high contrast wins when asked for): the resolver returns
    `Dom::create_icon(variant_spec)` and the existing icon-to-icon loop resolves it with ITS
    metadata; a variant naming the icon itself is not followed;
  - `ink_for`: tint on CurrentColor -> that colour; on Mask -> flood; on Palette/None -> ignored;
    Fixed beats tint and CSS colour; Mask drawn for the other mode (or `inherit_text_color`)
    takes the text colour;
  - font: the ink is a `color` pushed after the call site's declarations (so Fixed beats an inline
    `color`, CurrentColor keeps it);
  - raster/SVG: only alpha-mask artwork (`monochrome` or `Mask`) is flooded, always as
    `flood(c) composite(in)`; `currentColor` floods with the new `CURRENT_COLOR_TOKEN`.
- The mode: read from `SystemStyle.theme` (`is_dark()` in layout icon.rs). The core resolution
  entry point hands the resolver a style in the WINDOW's mode (`style_in_window_mode`, from the
  context's `theme`), so an app pinned dark on a light desktop gets dark variants.
  **R0 note:** when `SystemStyle.theme` / `Theme` are renamed to a mode, update `is_dark()`
  (layout/src/icon.rs), `style_in_window_mode()` (core/src/icon.rs) and the test helpers that set
  `s.theme = Theme::Dark` (layout icon.rs `dark_style`, the SVG integration test, core
  `mode_recording_resolver`).

### 1.2 The raster tint bug (ledger E15) - confirmed, and worse than suspected
- WebRender's `Flood` REPLACES its input (`brush_blend.glsl`: `color = offset.rgb; alpha =
  offset.a`), so the bare flood painted a filled square - where it painted at all: `filter` did
  not make a stacking context and the display list only emits `PushFilter` around one, so a
  filter on an ordinary box (an icon in a button) was silently dropped. The CPU renderer ignored
  Flood/Composite entirely.
- Fix: `filter != none` establishes a stacking context (display_list.rs, spec-correct);
  `flood(CURRENT_COLOR_TOKEN)` resolves to the node's cascaded `color` in the display list;
  CPU: `apply_layer_filters` implements `flood()` and `composite(op)` (all Porter-Duff ops +
  arithmetic, against the source graphic), and the direct render path isolates a filter group
  that holds a flood/composite (`MaskEntry::Filter(FilterGroup)`: set the backdrop aside, paint
  onto transparent, filter, composite back); WebRender: `fold_flood_in` turns the pair into the
  one colour matrix it is (`StyleColorMatrix::flood_in`).
- Found on the way: `translate_style_filters_to_wr` passed azul's ROW-major 4x5 matrix to WR,
  which takes input-channel COLUMNS then offsets (`picture.rs` uploads `m[4i..4i+4]` as vec4 i,
  `mat4(c0..c3)`). Every colour-matrix filter (the icon grayscale included) was scrambled. Now
  `StyleColorMatrix::to_column_major()`.

### 1.3 SVG
- `layout/src/cpurender/svg.rs`: `SvgPaintContext { current_color, palette }` threads through
  the rasteriser. `currentColor` (fill/stroke, attribute or style, any case) paints the inherited
  SVG `color` property: the document's own first, then the host's, black without either. A palette
  swaps listed literal paints (including the implicit black fill), never currentColor.
  `render_svg_to_imageref_painted`, `svg_natural_size`, `svg_uses_only_current_color`.
  The three copies of "parse, find `<svg>`, set up the viewBox" became `with_svg_root` /
  `rasterize_svg`, and `render_svg_to_png_over(None)` now really renders onto transparent
  (`AzulPixmap::new` starts opaque WHITE, so its "zeroed" backdrop was white).
- `layout/src/icon.rs`: `SvgIconData`, `register_svg_icon(provider, pack, name, svg_bytes, meta)
  -> bool` (refuses non-SVG and > 1 MiB), `default_svg_icon_meta(svg)`. The resolver rasterises at
  2x; monochrome ink is drawn as a black mask and flooded with the ink, so it follows the node's
  CASCADED colour (including one inherited from a container); non-monochrome documents bake
  currentColor from the explicit ink, the icon's inline colour, or the mode's `system:text`.
  Palette `system:` targets resolve for the mode.

### 1.4 Lookup order: rank, then registration order
- Today's lookup walked a `BTreeMap`: pack NAME order (the guide claimed registration order).
  `IconProviderInner { pack_order, pack_ranks }`, `packs_in_lookup_order`, `insert_icon`,
  `remove_pack`; `IconProviderHandle::set_pack_rank(pack, rank)`; unranked packs
  (`ICON_PACK_UNRANKED`) after every ranked one.

### 1.5 Remap rules and the loader
- `core/src/icon.rs`: `IconRemapRule { theme, conditions, target }`, `IconRuleCondition {
  Selector(DynamicSelector), App, Never }`, `parse_icon_apply_if`: `theme=` (chain membership via
  `DynamicSelector::Theme(Custom)`, the cascade's own matcher; `theme=light|dark` = the mode),
  `mode=`, `os=` (`parse_os_at_rule_content`, exactly `@os(...)`), `contrast=`, `app=`; comma =
  AND; unknown terms never match. `lookup_spec_in_context`: remap first (the verbatim spec, then
  every entry in spec order), then the spec's fallback list. Rules rank by their theme's index in
  the live chain (global last, themes not in the chain contribute nothing), then insertion order.
- Evaluated at LOOKUP: `resolve_icons_in_dom_with_context(dom, provider, style, Option<&ctx>)`;
  `styled_dom_resolving_icons_with_context` (the window path) uses it. The resolution cache now
  validates the style AND a rule context that holds only what rules read (mode, chain, OS,
  desktop, contrast), so a light -> dark switch re-evaluates while a resize does not flush.
- `layout/src/icon_remap.rs` (new, std; JSON half behind `json`): `load_user_icon_rules(provider,
  root, app_name) -> IconRulesReport`, `walk_theme_dirs(root) -> Vec<ThemeDir>`, `user_azul_root`,
  `user_icons_root`, `current_app_name`. `file:` is traversal-checked (plain relative components
  only, canonical target inside the canonical directory, symlinks included), tables and files are
  size-capped, theme directories named light/dark are refused (pitfall 5). Every theme directory is
  loaded (inert unless its theme is in the chain), so a runtime `set_theme` finds its rules.
  Injectable root; the tests never touch `$HOME`.
- `dll/src/desktop/app.rs`: `App::create` loads `~/.azul/icons` after the built-in packs unless
  `AZ_RICING=off`, printing what was refused.
- **R4 unification:** `azul_layout::icon_remap::walk_theme_dirs` (and `user_azul_root`) is the
  small reusable walk of a `.azul` theme tree (`xyz/pink/` -> `xyz:pink`, parents first, bounded,
  dot-dirs and symlinked dirs skipped). R4's `~/.azul/css/<theme>/` walk should use it (or the
  parent merges the two into one).
- **R3:** the chain is read from `DynamicSelectorContext.theme_chain`; prefix semantics come from
  R3's chain expansion (`[xyz:pink, xyz]`), membership is exact like `has_app_theme`.

## 2. Commits
- `3a2c83b43` plan checkpoint
- `8e1d021ac` RED metadata / variants / request x capability - `b4fa0dc75` impl
- `35cb194ff` RED E15 (pixel test + unit tests) - `4452689d9` fix
- `e2ca23f73` RED SVG - `0d61a4568` impl
- `b691e355f` RED rank order - `29b5b631b` impl
- `044a79ec8` RED remap rules / apply-if at lookup / window mode - `063c8c62a` impl
- `5f169baf9` RED loader - `775790a2a` impl
- `308e0893d` guide (also carries two tiny review fixes: a Copy field no longer `.clone()`d in
  `icon_rule_context`, `SVG_ICON_OVERSAMPLE` allowed dead without `cpurender`)
- `e972f00af` fix: `Some(theme.as_str())` in the loader
- `93f158913` this report
- `5a1a0d688` fix(svg): an absent viewBox is `0 0 width height` when rasterising (an icon written
  `width="16" height="16"` without a viewBox drew into a quarter of its 2x image)
- `4e442414c` test fix: borrow the palette in a let-else instead of partially moving it
- `d7cc63b35` fix: three `br#"..."#` SVG literals containing `fill="#..."` closed early; now
  `br##"..."##` (one sat in a `cfg(test)` module but is still parsed, so it broke every layout build)
- progress checkpoints: `af875194a`, `bdae4f114`, `577ff7b48`, `7c7e0234c`, `240b87e0a`, `99bc00405`

## 3. api.json (for the autofix; nothing edited by hand)

New types (all `repr(C)`, external `azul_core::icon::*`; fields in decreasing alignment):
- `IconDesignedFor` enum `{ Any, Light, Dark }`, derive Debug Clone Copy PartialEq Eq PartialOrd
  Ord Hash Default.
- `IconVariants` struct `{ light: OptionString, dark: OptionString, high_contrast: OptionString }`,
  derive Debug Clone PartialEq Eq PartialOrd Ord Hash Default.
- `IconColorMapping` struct `{ from: ColorU, to: ColorU }` (Copy); `OptionIconColorMapping`;
  `IconColorMappingVec` + `IconColorMappingVecDestructor`, `IconColorMappingVecDestructorType`,
  `IconColorMappingVecSlice`.
- `IconModeColors` struct `{ light: ColorU, dark: ColorU }` (Copy).
- `IconRecolor` enum `repr(C, u8)` `{ CurrentColor, Mask, Palette(IconColorMappingVec),
  Fixed(IconModeColors), None }`, derive Debug Clone PartialEq.
- `IconMeta` struct `{ variants: IconVariants, recolor: IconRecolor, designed_for:
  IconDesignedFor, monochrome: bool }`, derive Debug Clone PartialEq; custom Default (=
  `for_image`).

`IconMeta` methods:
- constructors `create_for_font()` -> `azul_core::icon::IconMeta::for_font()`,
  `create_for_image()` -> `...::for_image()`, `create_for_mask()` -> `...::for_mask()`,
  `create_for_svg(svg_bytes: U8VecRef)` -> `azul_layout::icon::default_svg_icon_meta(svg_bytes.as_slice())`;
- `with_light_variant` / `with_dark_variant` / `with_high_contrast_variant` (`self: value`,
  `spec: String`) -> `IconMeta`, body `object.with_dark_variant(spec)` etc.;
  `with_variants(variants: IconVariants)`, `with_recolor(recolor: IconRecolor)`,
  `with_designed_for(designed_for: IconDesignedFor)`, `with_monochrome(monochrome: bool)` (all
  `self: value` -> `IconMeta`); `is_mask_artwork(self: ref) -> bool`.
- `IconModeColors`: constructor `create_same(color: ColorU)` -> `IconModeColors::same(color)`;
  `for_mode(self: ref, dark: bool) -> ColorU`.
- `IconVariants`: `pick(self: ref, dark: bool, high_contrast: bool)` returns a reference -
  suggest NOT exporting it (C users read the fields).

`IconProviderHandle` new functions (self `refmut`; free-function bodies take `iconproviderhandle`
like the existing `register_image_icon`):
- `register_image_icon_with_meta(pack_name: String, icon_name: String, image: ImageRef, meta:
  IconMeta)`, body `azul_layout::icon::register_image_icon_with_meta(iconproviderhandle,
  pack_name.as_str(), icon_name.as_str(), image, meta)`;
- `register_svg_icon(pack_name: String, icon_name: String, svg_bytes: U8VecRef, meta: IconMeta)
  -> bool`, body `azul_layout::icon::register_svg_icon(iconproviderhandle, pack_name.as_str(),
  icon_name.as_str(), svg_bytes.as_slice(), meta)`;
- `set_pack_rank(pack_name: String, rank: u32)`, body `object.set_pack_rank(pack_name.as_str(), rank)`;
- `add_icon_remap_rule(icon_name: String, apply_if: String, target_spec: String)`, body
  `object.add_icon_remap_rule(icon_name.as_str(), apply_if.as_str(), target_spec.as_str())`;
- `add_theme_icon_remap_rule(theme: String, icon_name: String, apply_if: String, target_spec:
  String)`, same shape;
- `set_app_name(app_name: String)`.
- Doc updates: `lookup` / `has_icon` docs say "rank, then registration order" instead of "first
  match wins".
- Not proposed for the C API: `IconRemapRule` / `IconRuleCondition` (Rust `Vec`s, `DynamicSelector`
  inside), `load_user_icon_rules` (App::create calls it), `SvgPaintContext`.

## 4. Least sure to compile
1. `core/src/icon.rs`: the `impl_option!` / `impl_vec!` / `impl_vec_*` invocations for
   `IconColorMapping` (copied from the ColorU / JsonVec shapes).
2. `ImageIconData::with_meta` is a `const fn` moving Drop-carrying params into a struct (the same
   shape as `DomIconData::new`, which compiles).
3. `raster.rs` PushFilter arm: `.map(|r| FilterGroup::begin(pixmap, &r, filters))` reborrows the
   `&mut AzulPixmap` inside a closure; `super::compositor::apply_layer_filters` (made `pub(crate)`).
4. `cpurender/svg.rs`: `with_svg_root(.., |svg_node| -> Result<AzulPixmap, String> {..})?` and the
   `svg_natural_size` closure with `?` on `Option`.
5. `icon_remap.rs`: serde_json type inference in `load_table`'s `and_then` chain; the Copy closures
   `colour` / `for_mode` in `parse_recolor`.
6. `display_list.rs`: two closures borrowing `self` (system colours map, then the currentColor map).
7. `core/src/icon.rs parse_icon_apply_if`: the `never` closure passed by value to `map_or_else`
   (relies on it being Copy - it captures only `&&str`).
8. Test pixel expectations: the E15 and SVG tests assume the CPU pixmap is straight RGBA when a
   group paints onto transparent (the ink is opaque, tolerances 8/12).
A read-only review agent went over the whole diff for compile errors, checking every item against
its definition (macros, derives, AsRef ambiguity, borrows, cfg combinations, call sites outside the
diff). It found the three broken raw strings, fixed in `d7cc63b35`; everything else checked out.

## 5. Test commands (parent)
- `cargo test -p azul-core --lib icon` (autotest_generated, icon_cache_tests, remap_rules_tests)
- `cargo test -p azul-css --lib flood_in_matrix_tests`
- `cargo test -p azul-layout --lib icon::` and `cargo test -p azul-layout --lib tray_icon`
- `cargo test -p azul-layout --lib cpurender::svg` and `cargo test -p azul-layout --lib filter_`
  (compositor `apply_layer_filters` tests)
- `cargo test -p azul-layout --features json --test all -- a_tinted_raster_icon an_svg_icon_follows user_icon_rules`
  (`json` is not a default layout feature; the loader tests are gated on it)
- `cargo check -p azul-dll --features build-dll` (compositor2.rs, app.rs)
- Regression suites worth running because behaviour changed: the whole `azul-layout --test all`
  (filters now create stacking contexts), the reftests that use `filter:`, and the core icon tests.

## 6. Behaviour changes the parent should know
1. Pack lookup is registration order (was pack-name order). On Linux, `system` is registered before
   `material-icons`, so for a name in both packs the desktop icon now wins (it lost before).
2. `filter` establishes a stacking context: paint order changes for filtered boxes, and filters
   that were silently dropped (grayscale etc.) now render.
3. WebRender colour matrices are transposed correctly now - every colour-matrix filter changes on
   screen (it was scrambled).
4. Images registered without metadata no longer take `tint_color` (it painted a square before);
   `register_image_icon_with_meta(.., IconMeta::for_mask())` opts in.
5. `render_svg_to_png_over(.., None)` is transparent (was white).
6. The resolver's `SystemStyle` is in the window's mode; `IconProviderInner` gained pub fields.

## 7. Left / open
- From the review (not errors): `FilterGroup` (CPU direct path) isolates by the scroll-adjusted
  bounds only, so a tinted icon inside a TRANSFORMED element is isolated at the wrong place, and a
  bare `flood()` without a composite ignores the active clip. `find_in_packs` sorts the pack list
  per lookup (hidden by the resolution cache). With `AZ_THEME` set in the test environment,
  `a_light_to_dark_switch_swaps_the_artwork_through_the_provider` (layout icon.rs) follows the pin
  - run the suites without it. clippy may flag `let id = ..; id` (kept on purpose: it ends the
  `downcast_ref` borrow before `data` drops).
- Not done: section 8.2 "zip and directory packs read an optional remap.json next to the files";
  attribution in About (8 step 5); watching `~/.azul` for changes (pitfall 6).
- A `recolor` on an `icon:` redirect rule is ignored (the target keeps its own metadata).
- CPU compositor edge case: a filter list with blur AND flood promotes a layer whose pixbuf starts
  opaque white, so `composite(in)` floods the whole layer; blur-free tints take the isolated path.
- `backdrop-filter` still does not establish a stacking context (only `filter` was changed).
- The client-side decorations (`dll/src/desktop/csd.rs`) resolve icons without a window context,
  so they follow the desktop's mode, not a pinned one.
- `DomIconData` has no metadata (no variants for DOM icons).
- Twins found: the executable-stem computation exists 8 times (dll linux/macos/windows
  system_style, x11, wayland, notifications linux/windows, telemetry `app_key`);
  `icon_remap::current_app_name` is a 9th - one helper should serve all. The SVG root lookup in
  cpurender/svg.rs had two copies, now one (`with_svg_root`).
- The WebRender half of E15 is covered by the pure fold/transposition unit tests only; check a
  tinted icon on screen once.
