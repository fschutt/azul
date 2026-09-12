# The light/dark theme chain: why it is whack-a-mole (2026-09-12)

Branch `fix/compilation-1009`. State of today's five fixes: 1 and 2 are committed in
`621e7fb8c`, 3 in `faa2f6c6a`; 4 (`paint_defaults_fingerprint` in the DL key) and 5
(compact rebuild on any theme flip) are uncommitted, together with new tests in
`layout/src/window.rs` and `[THEME-DBG]` `eprintln!`s in `event.rs:9568`, `layout.rs:476/1701`
and `macos/system_style.rs:929` that must not ship. Every claim below is `file:line` on this tree.

## 1. The pipeline as it actually is

### 1.1 Three theme types, three sources, two precedence rules

| Type | Lives in | Written by | Default |
|---|---|---|---|
| `css::system::Theme` (`css/src/system.rs:161`) | `SystemStyle.theme` (`system.rs:192`) | shell probes: macOS `macos/system_style.rs:276-287`, Windows `windows/system_style.rs:271`, Linux `linux/system_style.rs:1456,1840`; Android/iOS/headless never write it | `Light` (`system.rs:252`) |
| `WindowTheme` (`core/src/window.rs:1277`) | `FullWindowState.theme` (`layout/src/window_state.rs:135`) | `adopt_probed_theme` (`macos/system_style.rs:928`), `linux/system_style.rs:2883`, `windows/mod.rs:6733-6736`, `android/mod.rs:1093`, `ios/mod.rs:1145`, `headless/mod.rs:1986`; app via `ModifyWindowState` (`event.rs:4701-4707`) | `LightMode` (`window_state.rs:371`) |
| `ThemeCondition` (`css/src/dynamic_selector.rs:813`) | `DynamicSelectorContext.theme` (`dynamic_selector.rs:991`) | the builders in 1.3 | `Light` (`dynamic_selector.rs:1078`) |

`AZ_THEME` is read once (`dynamic_selector.rs:846-857`) but applied twice: inside
`from_system_style` (`:1116-1117`, `AZ_THEME > SystemStyle.theme`) and again in
`LayoutWindow::dynamic_selector_context` (`layout/src/window.rs:5135-5139`,
`AZ_THEME > FullWindowState.theme`, unconditionally overwriting the first answer). The effective
rule is therefore `AZ_THEME > window theme > (nothing)`: `SystemStyle.theme` never reaches the
cascade, yet it still drives three other decisions: whether a style change is a full
regeneration (`window.rs:1901-1903`), the window background (`event.rs:9576-9587`), and the
display list's no-context fallback (`display_list.rs:7627-7630`).

Window creation seeds `ws.theme` from `options.window_state.theme` (`macos/mod.rs:5563`,
`windows/mod.rs:619`, `x11/mod.rs:3873`, `wayland/mod.rs:1972`), i.e. from the `LightMode`
default, never from the probed `SystemStyle`. `WindowCreateOptions.theme`
(`window_state.rs:46`) has no reader anywhere.

### 1.2 From "OS appearance changed" to "glyph painted", in order

1. **Shell probe → window theme.** macOS: KVO `effective_appearance_changed`
   (`macos/mod.rs:985-1003`) or the 2 s poll (`:1426-1439`, throttle `system_style.rs:851`) →
   `adopt_probed_theme` (`system_style.rs:909-936`): early-out when `ws.theme` already equals the
   probe (`:919`), else `update_unsynced_state(|ws| ws.theme = theme)` (`:928`) and a
   re-`discover()`ed `SystemStyle` (`:946-965`). Android is the only shell that requests the
   regeneration itself (`android/mod.rs:1094-1096`); every other shell hands the style to step 2.
2. **`adopt_system_style`** (`dll/src/desktop/shell2/common/event.rs:9552-9624`). Bails when
   the *whole* `SystemStyle` is unchanged (`:9556-9559`). `needs_full` (`:9563-9565`) asks
   `system_style_change_needs_full_regeneration` (`window.rs:1894-1906`): a theme flip is always
   a full rebuild (`:1901-1903`); anything else consults what the layout callback declared it
   read (`callbacks.rs:1182-1216`, recorded by `get_theme` `callbacks.rs:1446`, harvested at
   `layout.rs:496-497`). Full → `request_regeneration(ThemeChange)` (`:9597-9601`); otherwise
   `reset_incremental` (drops the DL cache, `cache.rs:720-723`) + `IncrementalRelayout::Restyle`
   (`:9603-9608` → `layout.rs:1662`, which re-enters the funnel at `:1704-1711`). There is **no
   engine handler for the `ThemeChanged` window event**: `grep ThemeChanged` in `event.rs` finds
   only the app path (`:4848`) and the two `ThemeChange` requests above. So a `ws.theme` write
   whose rediscovered style equals the held one triggers nothing (a dark desktop at startup:
   `discover()` is already dark, `ws.theme` is `LightMode`, the poll writes `DarkMode`, step 2
   returns `false`, and the only follow-up is `request_redraw` at `macos/mod.rs:1435-1439`).
3. **`regenerate_layout`** (`layout.rs:236`). The callback sees `current_window_state.theme`
   (`:449`). The pre-cascade fingerprint skip (`:543-553`) excludes `ThemeChange` (`:545`) but
   its "unchanged" exit (`:630-660`) returns **without** entering the funnel, so a theme that
   changed without the reason being set keeps the retained DOM's old context. The warm exit
   (`:668-680`) and the full path both reach the funnel.
4. **`StyledDom` creation cascade** (`core/src/styled_dom.rs:1322-1365`), with
   `dynamic_context = None` (`prop_cache.rs:2367`): `restyle` gates `@theme`/`@media` blocks on
   the context (`prop_cache.rs:1531-1538` — no context, no conditional rule), `apply_ua_css`
   (`:1343`; themed lookup at `prop_cache.rs:4911-4915` but the context is `None`, so the light
   table; the property list `:4859-4891` contains **no `TextColor`**), `compute_inherited_values`
   (`:1344`; Step 4 applies only *unconditional* inline properties, `prop_cache.rs:5056-5060`),
   the compact build (`:1354`; conditional inline props are evaluated against the context
   `compact.rs:955-957` — `None` → dropped; UA defaults go in through the **unthemed**
   `get_ua_property` `compact.rs:1194-1196`; children copy the parent's text tier `:768`), then
   `prune_compact_normal_props` (`:1365`, `prop_cache.rs:1253-1257` drops Normal compact-encoded
   entries from `cascaded_props`).
5. **The funnel installs the context** (`window.rs:5252`, builder `:5120-5141`).
   `set_dynamic_selector_context` (`styled_dom.rs:2349-2404`): equal → return (`:2361-2364`);
   `theme_changed` (`:2358-2370`); the author cascade is re-run only when the *author* sheet has
   conditional rules (`:2379-2387`; `restyle_retained` is a no-op for an empty sheet
   `:1760-1770`); otherwise `recompute_inheritance_and_compact_cache` (`:2389-2404`) which,
   despite its name, rebuilds the compact cache only (`:1659-1684`) — `apply_ua_css` and
   `compute_inherited_values` are **not** re-run.
6. **Query-time getters.** `get_property` (`prop_cache.rs:2579`) → `get_property_slow`
   (`:2624`): user overrides → inline fold, last match wins, non-pseudo conditions need the
   context (`:2673-2685`, Normal at `:2992-3023`) → `css_props` → global `*` (`:3025`) →
   `cascaded_props` (`:3046`) → `computed_values` for inheritable props (`:3045-3053`) →
   `get_ua_property_themed(node_type, prop, dynamic_context)` (`:3058-3062`; dark twins only for
   `<hr>`/`<button>` borders, `ua_css.rs:961-982`). `get_text_color_or_default` seeds black
   (`:2497-2507`). Normal-state fast paths read the compact cache instead: text colour
   `getters.rs:3134-3157`, border colours `:2262-2280`.
7. **Text layout bakes the colour.** `getters.rs:3164` (`unwrap_or(ColorU::BLACK)`) → `:3424`
   → `text3::StyleProperties.color` (`text3/cache.rs:4568,4575`; hashed at `:4661`; excluded from
   the shaping key `:4698-4736`; re-stamped on a shaping hit `:9035-9036`) → glyph runs
   (`glyphs.rs:275,312,327`) → `CachedInlineLayout.glyph_runs` (`layout_tree.rs:292`, built once
   at `:586-588`, kept across frames).
8. **Display list re-resolves it.** `live_color` (`display_list.rs:7570-7639`): ancestor
   user-override walk (`:7587-7605`), then `cache.get_text_color` on the slow path (`:7617-7619`),
   then `evaluate_ua_root_text_color` with the cascade's stored context or a freshly built
   system-only/default one (`:7626-7634`). The baked run colour is used only for runs without a
   source node (`:7639`). Pushed at `:7666/:7691`.
9. **DL cache.** Key tuple `cache.rs:567-574`; inputs `mod.rs:770-779`; hit condition
   `:802-808`; `dl_input_fingerprint` (`:470-500`) now folds `paint_defaults_fingerprint`
   (`:488-490`; `dynamic_selector.rs:1185-1190` hashes the theme only). A second gate,
   `cascade_ctx_unchanged` (`mod.rs:1520-1566` against `cache.last_dynamic_context`,
   `cache.rs:472`), protects DL patching. The DL-only re-emit path
   (`window.rs:17934`, key at `:17992-18004`, store `:18119-18133`; 14 callers such as
   `event.rs:5302,6236,6664`) never installs a context — it reads whatever the last layout pass left.
10. **Layout hash.** Colour is excluded from relayout by design (`property.rs:1700-1715`,
    `:1807-1808`); `NodeDataFingerprint` hashes inline declarations, never resolved values
    (`diff.rs:2085-2160`, inline at `:2109-2111`); it becomes the subtree hash at
    `cache.rs:1577-1584, 2196-2201, 3612-3617`. Two cascades of one DOM under two themes hash
    identically — the documented consequence is at `cache.rs:1623-1651`.

### 1.3 Every place with its own idea of the theme or the default colour

Context builders: `DynamicSelectorContext::default` (Light, `dynamic_selector.rs:1071-1099`);
`from_system_style` (`:1103-1117`); `LayoutWindow::dynamic_selector_context` (`window.rs:5120`);
the DL fallback (`display_list.rs:7626-7631`); `get_scrollbar_style` (`getters.rs:5609-5611`,
SystemStyle only — the window theme is ignored for scrollbars) and
`ComputedScrollbarStyle::default` (`:5493-5497`, Light); the four shells' own contexts
(`macos/mod.rs:5693-5705`, `windows/mod.rs:775-787`, `x11/mod.rs:3985-3997`,
`wayland/mod.rs:2134-2146`) — never installed on any `StyledDom` (`dll/` contains no
`set_dynamic_selector_context` call).

Default-colour answers: `DEFAULT_TEXT_COLOR` (`prop_cache.rs:2506`, `ua_css.rs:1364/1380`),
`ColorU::BLACK` (`getters.rs:3164`), the static UA table (`ua_css.rs:747, 801-804`), its themed
twins (`:961-982`), `UA_ROOT_TEXT_COLOR_CSS` (`:1342-1363`, consulted only at
`display_list.rs:7634`), the compact builder's unthemed UA pass (`compact.rs:1195`), the window
background (`wr_translate2.rs:319`, `headless/mod.rs:593-596`, `event.rs:9578`), and in the
widgets the Bootstrap palette (`button.rs:146-201`, light only) beside the `LIGHT_*/DARK_*`
tokens (`flat.rs:49-548`).

### 1.4 Caches on the path and what their keys see

| Cache | Key / invalidation | Sees the theme? |
|---|---|---|
| `cascaded_props`, `computed_values` (`prop_cache.rs`) | rebuilt by `restyle()` only (`styled_dom.rs:1772-1830`) | no — built with no context at creation; a theme flip does not rebuild them (`styled_dom.rs:2389-2404`) |
| compact cache | rebuilt on context change or `has_dynamic_conditions` (`styled_dom.rs:2389-2404`) | partly: conditional inline props yes (`compact.rs:955`), UA colours no (`:1195`) |
| `StyleCache` (`getters.rs:2942-2995`) | (node, state, viewport); per-`LayoutContext` | no (lifetime-mitigated) |
| `scrollbar_style_cache` (`getters.rs:5815-5833`) | node id | no |
| text shaping cache (`text3/cache.rs:8963-8971`) | excludes colour; colour re-stamped `:9035` | n/a |
| `CachedInlineLayout` (`layout_tree.rs:257,320`) | includes colour via `inline_content_hash` (`fc.rs:3815-3845`) | indirectly |
| `inline_content_cache` (`fc.rs:3647-3697`) | node fingerprints + tier-1 enums + font hash | no |
| `cache_map`, `float_cache`, subtree hash | geometry / inline declarations | no |
| `cached_display_list` (`cache.rs:567-574`) | subtree hash, viewport, gpu, scroll, input fp | only via fix 4's theme hash |
| pre-cascade `last_dom_fingerprints` (`layout.rs:531-553`) | structure + inline style | no; `ThemeChange` reason excluded (`:545`) |

## 2. Why it is whack-a-mole

**R1 — UA colour defaults are answered at query time, not cascaded.** They never enter a node's
resolved style, so no hash, fingerprint or cache can see them (fixes 4 and 5 exist only because
of this). Worse, there are three answer sites that disagree: the slow path is themed
(`prop_cache.rs:3058`), the cascade-time pass is themed but runs before a context exists
(`:4911` with `:2367`), and the compact builder is unthemed (`compact.rs:1195`). The `<hr>` and
native-button border twins from fix 2 are therefore unreachable in the normal state, which reads
the compact cold tier (`getters.rs:2272`). The default *text* colour is not in the UA property
list at all (`:4859-4891`) and lives in a fourth place consulted only by the display list.

**R2 — The cascade runs before the context exists and is not re-run when it arrives.**
`styled_dom.rs:1343` vs `prop_cache.rs:2367`; then `set_dynamic_selector_context` re-runs the
author cascade only for a conditional *author* sheet (`:2379-2387`) and otherwise rebuilds the
compact cache only. For the common DOM (inline-styled widgets, empty sheet) `cascaded_props` and
`computed_values` are permanently the no-context answers. Fix 5 patches the compact half only.

**R3 — Two readers with different inheritance semantics.** The compact builder evaluates
conditional inline declarations against the context (`compact.rs:955-957`) and inherits by
copying the parent's text tier (`:768`); `compute_inherited_values` applies only unconditional
inline declarations (`prop_cache.rs:5056-5060`). Every `dark_theme(TextColor(..))` on a container
is inherited on one path and not the other. The display list's `live_color` uses the slow path
(`display_list.rs:7617`) and overrides the compact-baked glyph colour (`:7639`) — symptom (a).

**R4 — "Colour is paint-only" is a relayout decision that leaked into cache keys.** Excluding
colour from `can_trigger_relayout` (`property.rs:1700-1715`) is right for layout; but the DL
cache and DL patching serve *painted* output keyed on the same colour-blind subtree hash
(`diff.rs:2109-2111`). Three separate compensations now exist for one missing "cascade
generation" key: `paint_defaults_fingerprint` (`mod.rs:488`), `cascade_ctx_unchanged`
(`mod.rs:1520-1566`) and `reset_incremental` in `adopt_system_style` (`event.rs:9603`). Each was
added when its own path bit.

**R5 — Three theme sources, two precedence implementations, seeding from the default.**
`window.rs:5135` re-implements what `dynamic_selector.rs:1116` already decided; window creation
seeds `LightMode` regardless of the probe (`macos/mod.rs:5563`); `adopt_system_style` keys its
decision on `SystemStyle` equality (`:9556-9559`) although the cascade follows the window theme;
no `ThemeChanged` handler exists. Fix 1 made the window theme the truth for the cascade but left
`SystemStyle.theme` as the *trigger* (`window.rs:1901-1903`) and the background source
(`event.rs:9578`).

**R6 — Widget styling is split across three mechanisms with an unenforced ordering invariant.**
`build_button_container_style` emits light Bootstrap values (`button.rs:204-380`); the theme
function appends `dark_theme` twins and states (`flat.rs:651-678`, `flora.rs:895-941`); style
bundles say `None` = "no opinion" (`button.rs:108-116`, `:440-453`). Resolution is last-match-wins
(`prop_cache.rs:2992-3023`, and the comment at `:3012-3016` says widgets *rely* on it), so a
twin pushed after a type's face silently replaces it. The per-type classification exists in
`button_states` (`flat.rs:2103-2107`, `neutral`) but not in the resting-face code (`:663-671`).
Fix 3 made this live — symptom (b).

**R7 — Dead and duplicate paths.** `Button::dom` bypassed both theme functions until
`faa2f6c6a`; the shells' contexts are dead weight; scrollbars use a fourth builder; the e2e
runner never calls `set_dynamic_selector_context` (`styled_dom.rs:1084-1085`). Nothing pinned
the theme path end to end, so each fix could only be found by looking at the screen.

Root causes vs symptoms: fixes 1 and 5 (and half of 2) treat R2/R5; fix 4 treats R4; fix 2's
twins treat R1's surface; fix 3 exposes R6; symptom (a) is R3; the startup-on-dark hazard is R5.

## 3. Invariants that should exist

- **I1 — One function owns the theme decision.** `LayoutWindow::dynamic_selector_context`
  (`window.rs:5120`) is the only constructor of a context that reaches a `StyledDom`; the DL
  fallback (`display_list.rs:7626`), `get_scrollbar_style` (`getters.rs:5609`) and the shells'
  contexts read `cache.dynamic_context` or go away. Enforce: make `from_system_style`/`default`
  non-`pub` outside `css` + a grep lint in `scripts/check.sh`.
- **I2 — The context exists before the first cascade, and a theme flip is a full restyle.**
  `debug_assert!(dynamic_context.is_some())` in `apply_ua_css` and the compact builders;
  `set_dynamic_selector_context` on `theme_changed` runs the same sequence as `restyle()`
  (`styled_dom.rs:1772-1830`). Test: create under Light, install Dark, assert `cascaded_props`
  and `computed_values` carry the dark UA values.
- **I3 — UA defaults are cascaded like any other property, from one themed table.** One
  `&[CssPropertyWithConditions]` per node type (the shape `UA_SCROLLBAR_CSS` already has,
  `ua_css.rs:1342`), with `TextColor` on the root, consumed by `apply_ua_css` *and*
  `apply_ua_css_to_compact` through one resolver. Then `get_ua_property_themed`'s twin match,
  `UA_ROOT_TEXT_COLOR_CSS`, `display_list.rs:7620-7635` and `getters.rs:3164` are unreachable.
  Test: `ua_css_test.rs:1590` rewritten to assert through a `StyledDom` under both themes.
- **I4 — The two readers agree.** For every node in the normal state, compact == slow for every
  compact-encoded property. Enforce: an `AZ_VALIDATE` cross-check at the end of
  `build_compact_cache_with_inheritance`; make `compute_inherited_values` Step 4 evaluate
  conditional inline declarations against the context with the same last-match rule.
- **I5 — Every cache that can serve painted output keys on the cascade generation.** Add
  `CssPropertyCache::cascade_epoch`, bumped by `restyle`, `set_dynamic_selector_context`, and user
  overrides; put it in the DL key (`mod.rs:770-779`, `window.rs:17992`) and `StyleCache`; retire
  `paint_defaults_fingerprint` and the `last_dynamic_context` equality. Test: the uncommitted
  `a_theme_switch_recolours_the_retained_dom` plus a variant through
  `regenerate_display_list_for_dom`.
- **I6 — The theme is paint-only.** Node rects are identical under both themes and across a
  switch: the uncommitted `the_theme_does_not_change_layout` and
  `a_theme_switch_in_one_window_does_not_change_layout` (window.rs diff) — commit them; add a
  debug assertion that a compact *rebuild* equals a fresh build tier by tier.
- **I7 — One trigger.** A `ws.theme` write decides regeneration vs restyle on the *window* theme
  delta, not on `SystemStyle` equality (`event.rs:9556-9559`); creation seeds `ws.theme` from the
  probe. Test: headless `set_system_theme` (`headless/mod.rs:1978`) on a window whose
  `SystemStyle` already matches must still restyle; `dll/tests/headless_lifecycle.rs:476` is the
  place.
- **I8 — Ordering by construction.** Dark twins are emitted *with* their light value
  (`hover_bg_both` shape, `flat.rs:2160`) and states after resting values; the widget lint
  manifest (`layout/tests/widget_lint_manifest_is_exhaustive.rs`) rejects a `dark_theme(X)` that
  precedes an unconditional `X` of the same property type in any `*_style` vector.
- **I9 — One integration test per path:** fresh DOM; retained DOM through the pre-cascade skip's
  unchanged exit; `Restyle`; full regeneration; DL-only re-emit; startup with a dark
  `SystemStyle` and default window state. Fixtures exist in `window.rs`'s test module.

## 4. Recommended restructuring (ranked by leverage vs risk)

1. **Theme flip = restyle** (high leverage, low risk, ~1 day). In `set_dynamic_selector_context`,
   when `theme_changed` (or the DOM carries any conditional declaration), reset
   `cascaded_props`, re-run `apply_ua_css` → `compute_inherited_values` → compact rebuild (the
   tail of `restyle()`), and have the funnel install the context *before* `StyledDom::create`'s
   cascade (or pass it in). Completes fix 5; removes R2. Fix 5 as written is the patch this replaces.
2. **Inherit conditional inline declarations on the slow path** (high, low, ~1 day).
   `prop_cache.rs:5056-5060` evaluates `conds` against `self.dynamic_context`, last match wins,
   plus the I4 cross-check. Fixes symptom (a) for every widget at once; without it every widget
   needs its own label-colour workaround, which is the whack-a-mole in miniature.
3. **One themed UA table consumed by both cascades** (high, medium, ~2-3 days). Replaces fix 2's
   twin match and `UA_ROOT_TEXT_COLOR_CSS`, deletes the DL fallback and the BLACK/`DEFAULT_TEXT_COLOR`
   seeds (keep them as `debug_assert!`s). After this, fix 4's theme hash is redundant.
4. **`cascade_epoch` in the DL and style-cache keys** (high, low, ~½ day). Replaces fix 4 and the
   `last_dynamic_context` gate with one key that also covers `@media`, restyles and user overrides.
5. **Single theme source and trigger** (medium, low, ~½ day). Seed `ws.theme` from the probe at
   creation in all shells (or make it `Option`, `None` = follow system); key `adopt_system_style`
   on the window-theme delta; delete the shells' dead contexts and `WindowCreateOptions.theme`;
   route scrollbars through the stored context; remove the `[THEME-DBG]` prints.
6. **Button surface classification** (medium, low, ~½ day). `ButtonType::surface()` →
   `Neutral | OwnColour | NoSurface`, used by `flat::button`, `flora::button` and `button_states`
   for face, text and border twins alike. Fixes symptom (b) without touching a light value.
7. **Pair builder + lint for widget styles** (medium, medium, ~1-2 days): I8.

Of today's five fixes, 1 and 3 are the right long-term shape (1 needs I1 to finish); 2, 4 and 5
are patches over R1/R4/R2 and should be retired by items 3, 4 and 1 respectively.

## 5. The two live symptoms (diagnosed from code)

**(a) Widgets demo, Default button label dark-on-dark after the switch.** The "Click me!" button
(`examples/c/widgets.c:217`, `AzButton_create` → `ButtonType::Default`, `button.rs:410`) carries
on its container: `simple(TextColor(rgb(33,37,41)))` (`button.rs:262`, from
`get_button_text_color` `:193-201`), then, because `container_style` is `None` (`flat.rs:605`),
`dark_theme(BackgroundContent(DARK_BG))` and `dark_theme(TextColor(DARK_FG))` (`flat.rs:663-671`),
then `button_states` (`:678`). The label is a `widget_p()` `<p>` (`flat.rs:632-640`,
`widgets/mod.rs:473`) whose style declares no colour (`button.rs:390-403`), with a text child.
Under Dark the container itself resolves `DARK_FG` on both readers (inline fold, last match). For
the *text node* the two readers diverge: the compact builder copies `DARK_FG` down
(`compact.rs:955-957`, `:768`), so the baked glyph colour is right; but `live_color`
(`display_list.rs:7617-7619`) asks the slow path, which finds nothing inline or in `css_props`
on the text node and answers from `computed_values` (`prop_cache.rs:3045-3053`) — the value
inherited through Step 4 of `compute_inherited_values`, which took only the unconditional
`rgb(33,37,41)` (`:5056-5060`). Result: near-black text on `DARK_SUR`. The declarations are correct
under the migration rule (a page-neutral surface takes `DARK_SUR`/`DARK_INK`); the engine loses
the dark text colour in inheritance. Principled fix: item 4.2 (conditional inline declarations
participate in `compute_inherited_values`). Setting the colour on the label `<p>` would hide it
for this widget only and would have to be repeated in every widget with a text child.

**(b) hello-world, Primary button lost its blue face; label wraps.** The container gets
`simple(BackgroundContent(bootstrap_primary))` (`button.rs:307`), `simple(TextColor(WHITE))`
(`:262`), and blue borders (`:312-313`, `border_color = bg_normal`). `flat::button` then appends
`dark_theme(BackgroundContent(DARK_BG))` for *every* type (`flat.rs:663-671`, "we just override
the background and text color for dark mode"), and last-match-wins (`prop_cache.rs:2992-3023`)
makes the face `DARK_SUR` under Dark. The text node keeps `WHITE` (same inheritance gap as (a)),
the borders stay blue, and `button_states(Primary)` keeps the blue hover/active
(`flat.rs:2103-2107`, `neutral == false`), so a dark-grey box with a blue outline flashes blue on
hover. Under the rule "a surface that is its own colour keeps its light colour", the resting-face
twins must be gated the way `button_states` already gates the states: push `DARK_BG`/`DARK_FG`
only for `ButtonType::Default` (and no face for `Link`); `flora::button` classifies the face
per type (`flora.rs:895-913`) but still pushes `dark_theme(TextColor(DARK_INK))` and `DARK_BD`
borders for every type (`:918-932`), which is the same defect one layer down. Item 4.6.

The wrap cannot be derived from the code alone. The DOM is identical to the pre-dispatch tree
(the old `Button::dom` already used `widget_p()`: `faa2f6c6a^:button.rs:679-686` vs
`flat.rs:632-640`), no stylesheet keys on `__azul-theme-flat`, and the theme is paint-only by
construction (`property.rs:1700-1715`) — so a wrap means an invalidation seam treats the colour
flip inconsistently. Two candidate seams: (i) the compact cache is now *rebuilt* on context
install for every button (`has_dynamic_conditions`, `styled_dom.rs:2389-2404`) after
`prune_compact_normal_props` ran (`:1365`), and a rebuild has never been proven equal to a fresh
build; (ii) colour is hashed into `inline_content_hash` (`fc.rs:3815-3845` via
`text3/cache.rs:4661`), so the label is re-shaped on the theme pass while box geometry comes
from caches blind to colour (`cache_map`, subtree hash). The uncommitted
`the_theme_does_not_change_layout` / `a_theme_switch_in_one_window_does_not_change_layout` tests
are the correct pin; run them first (`cargo test --release -p azul-layout the_theme_does_not_change_layout`)
and, if they pass, reproduce with a Primary button in the headless harness through the
`Restyle` path, which the unit fixture does not take.
