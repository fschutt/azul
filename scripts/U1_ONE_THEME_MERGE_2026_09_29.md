# U1 - one theme merge, and the loose ends of the `@theme` migration (2026-09-29)

Branch `wt/u1-one-theme-merge`, cut from `fix/input-bugs-2026-09-19` @ `0a326afe5`.
Nothing was compiled or run (house rule). The spots I'm least sure of are in section 5.

## 1. What was built

### 1.1 One merge (`layout/src/widgets/themes/theme_blocks.rs`)

T2's `every_theme_dom` / `every_theme_dom_for` / `every_theme_css` and T3's
`flat::follow_props` / `follow_dom` / `follow_app_theme` / `follow_node` / `follow_alike` are
gone. One module keeps T3's names:

| fn | for |
|---|---|
| `follow_app_theme(widget, flat_fn, flora_fn)` | a two-builder widget's `None` arm |
| `follow_dom(structure, flat, flora)` | two DOMs the caller built (card / form / datetime_local / progressbar) |
| `follow_props(flat, flora)` | one part of a skin / look widget |
| `follow_css(structure, flat, flora)` | one sheet: rules plus `@keyframes` |

W5a / W5b's paths exist: `crate::widgets::themes::theme_blocks::{follow_app_theme, follow_dom,
follow_props}`, all `pub(crate)`.

**The semantic.** I kept T3's per-PROPERTY sharing: a property both looks declare alike (the
same declarations, conditions included, in the same order) is declared once, unconditionally.
It is emitted ORDER-PRESERVING, which is T2's guarantee:

- The shared declarations are anchors. Between two anchors come flat's other declarations in
  flat's order, then flora's in flora's order.
- So under app theme T, the live declarations ARE pinned T's, in order. They are equal, not
  merely equivalent.
- This makes both proofs hold at once:
  - T2's ordered contract (`theme_blocks::checks::assert_follows_the_app_theme`);
  - T3's per-property resolution check (`layout/tests/widgets_follow_the_app_theme.rs`, and
    the moved `follow_tests`).

Two edge rules keep it exact:

- **The two orders cross** (flat A..B, flora B..A, both alike). The property whose twin is
  further away gives way and is written per theme.
- **A rule is never split.** T3 split multi-declaration rules; this merge does not. A rule that
  declares an alike property and a differing one goes whole into each block, and takes the
  alike property's other declarations with it. A property's declarations are all shared, or
  none are, so a dark twin always sits with its light half.

**Folded in from T2:**

- the `flat == flora` short-circuit, per part, per sheet and per node style. The O(n*depth)
  whole-subtree compare at every level is gone; each node is compared once;
- the component-sheet merge: pairwise, or a sheet only one look attaches goes whole into its
  block;
- the keyframes merge: the structure theme's tracks first, the other theme's only under new
  names.

T3's `// ==== follow the app theme (T3) ====` section is removed from the end of `flat.rs`.
Its `follow_tests` moved unchanged into `theme_blocks.rs`. A part declaration goes into its
block through the css crate's existing `CssPropertyWithConditions::in_theme`.

**Call sites switched:**

- **T2's 18 widgets.**
  - 15 call `follow_app_theme`.
  - card calls `follow_app_theme` around its placeholder.
  - datetime_local and form call `follow_dom(UiTheme::current(), ..)`.
  - frame now uses the skin merge (1.5).
- **T3's sites and docs.** `flat::follow_*` became `theme_blocks::follow_*`.

### 1.2 One probe rule (`themes/theme_checks.rs`)

Three readers decided on their own which `@theme` block is live:

- `theme_checks::applies`;
- the copy in `widgets::theme_probe::live`;
- `checks::live_rules`.

Now there is one rule:

- `theme_checks::probe_theme()` is the app theme the test built for. It is entered
  explicitly with `checks::under(theme, ..)` / `ThemeScope`, and is flat outside one.
- `theme_checks::live_conditions(conds, theme)` uses the cascade's own matcher.

Everything goes through it, and `theme_probe::live` is now `checks::live_inline`. This is T2's
reading, which the merged branch already ran. No test changed.

T3's "probes ignore app-theme conditions" reading no longer existed on the merged branch. Two
tests need T2's reading:

- `text_input::dom_gives_the_container_and_the_label_a_dark_mode_twin`;
- `slider::dom_gives_the_track_and_the_thumb_a_dark_mode_fill`.

Both probe an unpinned widget; under the shared-only reading they would find no dark fill.

### 1.3 The twin build is silent (T3 decision 3)

- `widgets::style_only_build(f)` sets a thread-local flag, restored by a drop guard. It is
  under `cfg(std)`.
- `warn_widget_needs_a_name` checks the flag.
- `follow_app_theme` builds the structure theme's look normally and the other theme's look
  inside the flag.
- Result: an unnamed slider, switch or check box warns once per build.

### 1.4 Embedders (T2 note 4)

Audit:

- **Pinned embedders already pass their pin:**
  - `ColorInput` passes it to the hex TextInput, the R/G/B/A NumberInputs and their Labels;
  - `DateTimeLocalPicker` passes it to its parts;
  - `FileInput` passes it to its Button.
- **dialog / modal / popover embed no widget.** The close button is their own node, so there is
  nothing to pass.
- **ribbon, statusbar, quick_access, backstage and node_graph have no theme option.** They are
  single-look widgets (flat), and the widgets they build followed the app theme.
  - Under flora, a flat ribbon carried flora-marked Buttons.
  - The flat status bar carried a flora zoom Slider with `@theme` blocks.
  - The node graph's fields likewise.
  - Now `UiTheme::SINGLE_LOOK` (`pub(crate) const`, = Flat) pins these parts:
    - ribbon: its Button;
    - quick_access: 2 Buttons;
    - statusbar: 2 Buttons and the zoom Slider;
    - backstage: the back Button;
    - node_graph: its 5 fields.

### 1.5 Performance (T2 note)

- **frame** is built ONCE from a merged `FrameLook`:
  - `flat::frame_look()` / `flora::frame_look()` were split out of `flat::frame` /
    `flora::frame`, which now build with them;
  - `follow_props` merges each part, the look takes the structure theme's marker, and one
    `build` follows;
  - the caller's content is no longer cloned.
- **accordion** still clones its sections' content (see section 6).

## 2. Commits

| hash | what |
|---|---|
| `d08f7bfbf` | RED: the one merge's contract (alike once wherever it sits, each theme its own order, crossing orders, rules never split, alike dark twin shared) |
| `0233bf13a` | the one merge; T3 section out of `flat.rs`; `every_theme_*` deleted; all call sites; T2 tests renamed to `follow_css` / `follow_dom` (no assertion changed); `follow_tests` moved |
| `3f9390e33` | one probe rule (`probe_theme` / `live_conditions`) |
| `8da8a8a20` | RED: an unnamed follower warns once (test-only thread-local counter; the diagnostics ring is shared across parallel tests) |
| `ea3ef9457` | `style_only_build` + the flag in `warn_widget_needs_a_name` |
| `bb3ddfcdc` | RED: a widget without a theme option pins what it embeds (new `layout/tests/a_widget_without_a_theme_option_pins_what_it_embeds.rs` + `all.rs`; node_graph unit test) |
| `6820d6bd8` | `UiTheme::SINGLE_LOOK` pins |
| `9edbd35ce` | frame built once from the merged look |
| `6b28c0738` | style: the flag's drop guard as a plain item |
| `38f1d7046` | parts go into their block through `CssPropertyWithConditions::in_theme` (no copy of it) |
| + `chore(u1)` progress commits and this report | |

## 3. api.json

**No changes.** Everything added or removed is `pub(crate)` or test-only:

- `UiTheme::SINGLE_LOOK`;
- `theme_blocks::{follow_*, theme_condition}`;
- `widgets::style_only_build`;
- `flat::frame_look` / `flora::frame_look`.

`flat::frame` / `flora::frame` keep their signatures.

## 4. Tests for the parent

```
cargo test --release -p azul-layout --lib widgets::
cargo test --release -p azul-layout --test all widgets_follow_the_app_theme
cargo test --release -p azul-layout --test all a_widget_without_a_theme_option_pins_what_it_embeds
cargo test --release -p azul-layout --test all      # the rest (accordion_animation, menubar_item_clip, ...)
```

Named ones inside `widgets::`:

- `themes::theme_blocks::tests` (T2's and the U1 cases);
- `themes::theme_blocks::follow_tests` (T3's);
- `a11y_warning_per_build`;
- `node_graph::autotest_generated::a_nodes_fields_render_the_same_under_every_app_theme`;
- every widget's `app_theme_tests`;
- `frame::app_theme_tests`.

## 5. Least sure (compile, then behaviour)

1. **`theme_blocks.rs`, the generic engine.**
   - `plan(&flat_rules, &flora_rules)` passes `&Vec<CssRuleBlock>` into `fn plan<T: Unit>(&[T],
     &[T])`, relying on deref coercion with an inferred `T`.
   - `Iterator::eq` over `&T` items.
   - `let _twin = flora.next();`.
2. **`follow_node(built: &mut Dom, mut other: Dom, ..)`.**
   - `built.children.as_mut()` as `&mut [Dom]`; T3 used the same.
   - `core::mem::take` on `built.root.style` and `built.css`; T2 used the same.
3. **`widgets/mod.rs`, the test code.**
   - `fn unnamed() -> [(&str, fn() -> Dom); 3]` with closures coerced to `fn` pointers.
   - `A11Y_WARNINGS_ON_THIS_THREAD.with(core::cell::Cell::get)`.
4. **`frame.rs::follow_look`.**
   - a closure `both(&[P], &[P])` returning `into_library_owned_vec()`;
   - `FrameLook` built from `flat::frame_look()`.
5. **Behaviour: T2's contract, check 5** ("both themes' blocks present where the looks differ").
   - Under per-property sharing, a theme's block is absent if every declaration of that look is
     one the other look declares alike. For example, flora adds new properties only, on every
     node.
   - T2's prefix merge always wrote both blocks after the first difference.
   - I kept the check strict. The risk looks low: a flora look recolours, so it re-declares
     what flat declares. I did not verify this widget by widget.
   - If one fails there, the widget still renders right under both themes (checks 1-4). Relax
     check 5 to "carries a theme block".
6. **Behaviour: the single-look pin is a judgment call.**
   - It restores the pre-migration look of those parts.
   - The alternative is to let a single-look widget's parts follow the app theme. That is one
     line per site, and the new test would go.

## 6. What is left / integration notes

- **W5a / W5b (ribbon, quick_access, statusbar).**
  - When those widgets gain `theme`, each `UiTheme::SINGLE_LOOK` pin in their files becomes the
    widget's own theme: `if let Some(pin) = self.theme.into_option() { b.set_theme(pin) }`,
    and `None` leaves the Button following.
  - The widget moves from `a_widget_without_a_theme_option_pins_what_it_embeds.rs` (and T3's
    single-look guard) to an `assert_follows_the_app_theme` check.
  - Sites:
    - `ribbon.rs::styled_button`;
    - `quick_access.rs::{action_button, window_button}`;
    - `statusbar.rs::{styled_button, segment_dom, zoom_dom}` (the Slider).
  - My only edits in those three files are these one-line pins.
  - I made no edits in tabs, titlebar or tree_view.
- **V1 (accordion, spinner, segmented, stepper, pagination, date_picker).**
  - My edits there are the call-site / path switch only.
  - The accordion still clones its sections' content for the twin build.
  - Once V1's chevron lands, do the frame's move: `flat::accordion_look()` /
    `flora::accordion_look()`, `follow_props` per `AccordionLook` part, one `build`.
- **Twins found, left in place:**
  - The two contract checks: T2's `theme_blocks::checks::assert_follows_the_app_theme` (unit,
    ordered) and T3's `assert_follows_the_app_theme` in
    `layout/tests/widgets_follow_the_app_theme.rs` (integration, per property).
    - The integration crate cannot reach the crate-private one.
    - One fix is to move T3's widget checks into unit tests on `checks::`. The ordered check is
      the stronger one now.
  - The integration tests' local `nodes` / `theme_names` helpers are copies of `theme_checks::*`
    for the same reason.
- `all.rs`: I appended one `#[path]` + `mod` pair. The main checkout's uncommitted `all.rs` also
  appends at the end, so a trivial conflict is expected.
