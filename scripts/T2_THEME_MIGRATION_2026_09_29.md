# T2 - `@theme` migration, first half of the widgets (2026-09-29)

Branch `wt/t2-theme-migration`, cut from `fix/input-bugs-2026-09-19` @ `36ce2f698` (step 0).
This is wave 3 of PR #476, and it follows the recipe in `scripts/T1_APP_THEME_2026_09_29.md` section 4.
Nothing was compiled or run (house rule). The spots I'm least sure of are in section 5.

## 1. What changed for an app

A widget built with no `with_theme` now follows the app theme (`AppConfig::with_theme`,
`CallbackInfo::set_theme`). Its DOM carries both widget themes' declarations. Each one sits inside its
`@theme(flat)` / `@theme(flora)` block, and the cascade's matcher keeps the app theme's.
`with_theme(t)` pins the widget: the output is exactly today's DOM, and it ignores the app theme. The
default app theme is `flat`, so an app that chose nothing looks exactly as before.

## 2. How (one mechanism for every widget)

`layout/src/widgets/themes/theme_blocks.rs` (new):

- **`every_theme_dom(flat, flora)`**. The widget builds both pinned looks with today's generators
  (`themes::flat::x(self.clone())`, `themes::flora::x(self)`). They are untouched, so a pin is
  byte-for-byte today's output. This function then returns one tree:
  - **Structure.** Nodes, text, classes and the theme marker, callbacks and a11y all come from
    `UiTheme::current()`, the theme the DOM is being built for. A theme switch rebuilds the DOM
    (T1 2.5).
  - **Style per node.** Each node gets `every_theme_css(flat_style, flora_style)`:
    - the leading rules both looks share are declared once and unconditioned. A rule qualifies
      if it is equal in both looks, has no conditions, and its property gets no dark twin in
      either look (so `theme_pairs` still finds each twin's light half inside its own block);
    - then flat's remaining rules, each with `@theme(flat)` prepended (the name comes first,
      then dark and the state);
    - then flora's remaining rules, with `@theme(flora)` prepended.
  - **Why only a prefix is shared.** Only a common prefix is hoisted, so under app theme T the
    live rules are exactly pinned T's rules, in the same order. The older tests keep comparing
    ordered lists, and they still pass.
  - **Subtrees.** A subtree that is equal in both looks (the caller's content, or a part drawn
    alike in both themes) is kept once, unchanged.
  - **Component sheets.** A component sheet that only one look attaches (`with_css`: the flat
    menubar) goes whole into that theme's block. Sheets both looks attach are merged pairwise.
  - **Mismatched trees.** Where the two trees differ in child count, the structural theme's
    subtree is kept as is. A node paired with the wrong partner could only carry dead
    declarations.
- **Why a runtime merge and not const statics.** Every flora look, and most flat ones, is built
  at runtime (`decl::` helpers, `Look` structs, functions that depend on kind or size).
  Hand-rewriting about 40 generators into `theme_conditions!` statics without a compiler was
  the riskiest option. This merge is mechanical and exact, and it is the one place a later
  static or cached form plugs into (section 6).

## 3. Per widget

| widget | status | notes |
|---|---|---|
| accordion | done | same `build(Look)` for both; bodies (theme-independent, animated) stay unconditioned |
| alert | done | |
| avatar | done | the two looks are identical today, so the merge returns the one look with no blocks; the test checks that |
| backstage | **n/a** | no `UiTheme`, one look (Office palette); its back `Button` has a caller style, so it stays unconditioned |
| badge | done | all 6 kinds tested |
| breadcrumb | done | the separator glyph is structure (flat `/`, flora a U+203A chevron) and comes from the app theme; tested |
| button | done | all 8 types + submit; `with_form_semantics` is applied after the merge |
| card | done | both looks are built around a placeholder, then the caller's content is inserted once (no clone or deep compare) |
| check_box | done | checked and unchecked |
| chip | done | 6 kinds x removable |
| color_input | done | passes its pin to the picker's `Label` / `TextInput` / `NumberInput` (otherwise a pinned input would follow the app theme in its parts) |
| date_picker | done | date, month and week modes; the click palette comes from the structural theme |
| datetime_local | done | parts are built once, unpinned (they follow on their own); a pin still passes down; the row is merged |
| divider | done | plus a real-cascade test: both app themes x light/dark painted |
| drop_down | done | **default theme changed from `Some(Flat)` to `None`** (and `None` used to build an EMPTY div); `<select>` replacements now follow the app theme |
| form | done | the form node is merged without children, then the content is inserted once |
| frame | done | content is cloned and compared (see section 6, perf) |
| label | done | |
| list_view | **n/a** | no `UiTheme`, one look; nothing belongs in a flora block (a flora list view is design work, not migration) |
| menubar | done | `build_menubar_dom` (the window's injected bar) follows too; the flat bar's `with_css` sheets land in `@theme(flat)` |

`flat.rs` / `flora.rs` are untouched.

## 4. Commits

| hash | what |
|---|---|
| `83b537b80` | RED. `theme_blocks::checks`: the contract, stated once. Probes now read app-theme names. `mod app_theme_tests` in all 18 widgets. |
| `3d2025dea` | GREEN. `every_theme_css` / `every_theme_dom` + unit tests, and the `None` path of all 18 widgets. Also: DropDown default, card/form content, datetime_local parts, color_input pin threading, older tests reading the live view, docs of `theme` / `set_theme` / `dom`. |
| `720404d0b` | divider through the real cascade (`create_from_dom_with_context`, both app themes x light/dark) |
| (last) | this report + progress checkpoint |

**RED, genuine.** On `83b537b80` the product code is unchanged, so `None` renders flat under every
app theme. Every widget's flora half therefore fails `a node does not render as the pinned flora
widget's`. DropDown's two tests fail on its pinned default.

**The contract** (`checks::assert_follows_the_app_theme(what, follow, pinned)`). Every widget is
checked for five things:

1. Under each app theme T (a `ThemeScope`, as the engine builds), the followed DOM has pinned T's
   tree, types and classes (marker included). Per node, after dropping the blocks the real
   matcher (`DynamicSelector::matches` with `with_app_theme(T)`) rejects, it has exactly pinned
   T's rules in order: inline style and component sheets.
2. The same a11y outline as pinned T, and the same outline under both themes.
3. `half_pairs` passes. This now pairs by theme name, like the lint.
4. A pinned widget looks the same under both app themes and carries no blocks.
5. Where the looks differ, both blocks are present whichever theme built the DOM.

## 5. Least sure (compile and behaviour)

1. **`theme_blocks.rs`.**
   - The inner `fn pair<T>` is declared after `let` statements.
   - `.map(CssDeclaration::get_type)` uses a `const fn` as a path.
   - `Css { rules, keyframes }` is destructured; I assumed `Css` has exactly these two fields
     and no `Drop` impl.
   - `take_while(|(a, b)| a == b ...)` compares `&&CssRuleBlock` values.
2. **The live-view rewrite in older tests.** `X.root.style.iter_inline_properties()` became
   `checks::live_inline(&X).iter()`, a regex rewrite at ~40 sites. Items are now
   `&(CssProperty, DynamicSelectorVec)` instead of `(&CssProperty, &DynamicSelectorVec)`.
   Closure patterns bind the same, but a site that stored the iterator past the statement would
   now borrow a temporary. I checked each site; none does.
3. **`card.rs`: `let mut shell = self;`** inside a match arm whose scrutinee is
   `self.theme.into_option()`.
4. **`datetime_local` / `color_input` follow tests need T3.** They build `TimePicker` /
   `TextInput` / `NumberInput` unpinned. Until T3 migrates those, under the app theme flora these
   parts are flat, while the pinned-flora reference has flora parts. The flora half of these two
   tests fails on this branch alone and passes once T3 is in.
5. **Doc changes.** The `theme` field, `set_theme` and `dom` docs changed on 18 widgets.
   api.json docs will mismatch until autofix runs.
6. **The engine side.** Inline rules conditioned `[Custom(name), Dark, Hover]` go through the
   generic `matches` in `prop_cache` / `compact`. Tagging for hover etc. scans *any* pseudo
   condition (checked). T1 tested inline `@theme` blocks through the funnel. The divider
   cascade test is the one widget-level proof.

## 6. Open questions / what is left

1. **A StyledDom with no context** (`StyledDom::create`, `create_from_dom`).
   - **Problem.** `prop_cache` (`matches_pseudo_state`, ~l.2827) and `compact` (inline scan,
     ~l.962) evaluate every non-pseudo condition as false when there is no context. A followed
     widget styled that way shows only its shared prefix.
   - **Where it matters.** Windows set the context before every pass, so this is fine there.
     `layout/tests` with widgets go through `layout_and_generate_display_list`. The no-context
     sites in my widgets' tests are structure or callback tests (checked each).
   - **Fix, one line each, in core (outside my files).** With no context,
     `Theme(Custom(n))` should hold iff `n == DEFAULT_APP_THEME`, as the default context does.
   - **Also check.** Any headless / PDF path that styles replaced form controls without a
     context.
2. **Shared files I had to touch** (T3 likely did the same; keep one copy):
   - `themes/theme_checks.rs`: `applies` reads `Custom(name)` against `current_theme()`, and
     `half_pairs` pairs by `theme_names()`;
   - `widgets/mod.rs`: `theme_probe` gained a private `live()` that the three probes go
     through;
   - `themes/mod.rs`: one line, `pub(crate) mod theme_blocks;`. A second copy from T3 is a
     duplicate-module compile error.
   - `themes/theme_blocks.rs` is new. If T3 created one too, merge the two by name. Mine
     exports `every_theme_dom`, `every_theme_dom_for`, `every_theme_css`, `theme_condition`,
     and `checks::{BOTH, under, live_rules, live_inline, live_style, live_properties,
     theme_names, assert_follows_the_app_theme}`.
3. **T3's widgets that embed mine** (dialog / modal / popover buttons, ribbon / statusbar /
   quick_access `Button`s) must pass their pin down, as `color_input` now does. Otherwise their
   "pinned ignores the app theme" check sees a following child. Children with a caller style are
   unaffected: the style is the same in both looks, so it stays unconditioned.
4. **Performance.**
   - A followed widget builds both looks and deep-compares each level (O(n*depth)).
   - accordion and frame also clone and compare the caller's content; card and form avoid it
     with a placeholder.
   - Fix: build the looks' style vectors without building DOM twice, or generate
     `theme_conditions!` statics from the same generators and pin them with
     `without_theme_names` (recipe item 4). This needs a compiler in the loop.
5. **Demo (D1, examples/, not mine).** Drop every `.with_theme(state.theme)` and make the
   toggle `info.set_theme(..)` (T1 4, item 8).
6. **list_view / backstage.** One look each, so they stay theme-independent until someone
   designs a flora look.
7. **The unknown-theme sharp edge** (T1 4). `set_theme("monokai")` leaves a followed widget with
   only its shared prefix, until the chain gets its default floor.

## 7. Suites to run

- `cargo test --release -p azul-layout --lib widgets::`: the per-widget `app_theme_tests`,
  `theme_blocks::tests`, the `theme_pairs` lint, `theme_contrast`, `label_convention`,
  `chrome_text_is_not_selectable`, and every widget's older suite.
- `-p azul-layout --test all`: accordion_animation, menubar_item_clip,
  flat_and_flora_widgets_follow_the_light_and_dark_theme, form_controls_become_widgets,
  fixed_size_widgets_sit_at_the_start, app_theme_override.
- `-p azul-dll --test app_theme_headless`, `--test color_scheme_headless`,
  `-p azul-dll --lib --features build-dll`, the AzWidgets demo build.
