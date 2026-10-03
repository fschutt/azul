# W5c - a flora look and a theme option for the backstage (2026-09-29)

Branch `wt/w5c-flora-backstage`, cut from `5fba6d3b9` (the integrated W5a / W5b / U1 work).
Nothing was compiled (house rule). api.json was NOT touched: the list is in section 5.

## 1. What an app gets

`Backstage` has a flora look, light and dark, next to its flat (Office 2013) look. It also has
a `theme: OptionUiTheme` option.

- **`theme: None` (the default):** the backstage FOLLOWS the app theme. `dom()` merges every
  part of the flat look with the same part of the flora look through the one merge helper,
  `theme_blocks::follow_props` (`backstage::follow_style`, part by part, like W5b's
  `TreeViewLook::of`).
  - The DOM is built ONCE, so the caller's title strip and pane content are never cloned or
    walked twice. This is why it is not `follow_app_theme`, which builds the widget twice.
  - What both looks declare alike ends up unconditional: the column's layout, the flex boxes,
    the font, the back button's circle. Everything else sits in its theme's block.
- **`with_theme(t)`:** exactly that look, with no `@theme` block anywhere.
- **The back Button** takes the backstage's own `theme`. The `UiTheme::SINGLE_LOOK` pin is
  gone.
  - Pinned: the button is built in that look.
  - `None`: the button follows the app theme with the backstage. Its injected parts already
    carry both blocks, so the button's own merge returns them as they are.
- **The root** carries the marker of the structure theme (`__azul-theme-flat` /
  `__azul-theme-flora`). This is the W5a chrome convention.
- **A part the caller set** (`Some(..)` in `BackstageStyle`) is the caller's in both looks.
- **The flat look** is the Office palette's parts, declaration for declaration. A test pins
  this.

## 2. The flora look, and what it takes from the flyout menu

The source is flora.css's drop panel, `.mobile-menu` (lines ~2897-2952): the drawer that
`body.nav-open` slides in when the socket is pressed. The hover and pressed states come from
`.nav-links a:hover` / `a:active` (~846-858), and the collar and focus ring from the socket,
`.fl-orb` (~680-790). Every colour is a flora token, taken through the style kit.

| part | flyout source | flora backstage (light / dark) |
|---|---|---|
| nav column | `.mobile-menu`: `background: var(--fl-sur)`, `border-left: 1px solid var(--fl-bd2)` | `LIGHT_SUR` / `DARK_SUR` leaf; 1px `LIGHT_BD2` / `DARK_BD2` hairline on the edge facing the page (right) |
| page: root, right side, pane host | the page the drawer lies on (`--color-bg` = `--fl-pg`), text `--color-text` = `--fl-ink` | `LIGHT_PG` / `DARK_PG`; the root inks `LIGHT_INK` / `DARK_INK`, so app text in the pane reads in both modes |
| nav item | `.mobile-menu a`: `--color-text`, `padding: 7px 12px`, `border: 1px solid transparent`, `border-radius: var(--fl-r)`; drawer `padding: 14px 12px; gap: 2px` | W5a's toolbar key (`chrome_key`): bare, transparent ring, 3px radius, ink `--fl-ink` |
| nav item :hover | `.mobile-menu a:hover`: `linear-gradient(--fl-hT, --fl-hB)`, `border-color: --fl-bd` | `HOVER_FACE_LIGHT` / `_DARK` in a `LIGHT_BD` / `DARK_BD` hairline |
| nav item :active | `.nav-links a:active`: `linear-gradient(--fl-pT, --fl-pB)` | `PRESSED_FACE_LIGHT` / `_DARK` |
| selected item | `.mobile-menu a.active`: `--fl-gem-sunken` + leaf (brass) border + inset shadow, `color: --fl-on-acc` | `selected_stone()` (gem-sunken under the sunken rig) in `LIGHT_ON_ACC`, with a `TAB_METAL` edge (`--fl-metal-turn`, the brass the leaf is cut from). It stays the stone under the pointer and while held. It is the same in both modes (a stone is its own colour). |
| back button | the socket that opens and closes the drawer: `.fl-orb-collar` (the brass ring), `:hover` brighter, `:active` settles in, `:focus-visible` a 2px focus-colour ring following the circle | the ribbon FILE button's raised accent stone (`stone_face(LIGHT_ACC, STONE_STREAK)`) in a 2px `TAB_METAL` collar. Its arrow is `LIGHT_ON_ACC`. On hover the streak brightens (`STONE_STREAK_HOVER`); held, it sinks (`sunken_stone_face(LIGHT_DEEP)`); on focus it gets a halo (`LIGHT_ACC` by day, `DARK_GLOW` by night). |

**Metrics.** The column (126px), the back button's 38px circle, the margins and the fonts are
flat's (`chrome_geometry`).

Only the keys are re-measured, to the drawer's inset. This is the one place the flyout differs
in a way that matters: rounded, ringed keys need room around them.

- **Horizontal:** 12px in from each side, a 1px ring, and 11px padding. The label stays at the
  flat look's 24px indent.
- **Vertical:** a 36px key with 1px above and below. The items keep the flat 38px pitch, with
  the drawer's 2px gap between keys.
- **Result:** a theme switch moves no word. The one exception is the item after the gap, which
  sits 1px lower because the gap is flat's `margin-top: 22`.

The column's hairline is a 1px `border-right`. The column is `border-box`, so it stays 126px
wide.

## 3. Design guesses (decide)

1. **Page vs leaf.** The pane is `--fl-pg` (the ground) and the column is `--fl-sur` (a leaf),
   as the drawer lies on the page. The alternative is pane `--fl-sur` and column `--fl-desk`.
2. **Back button.** I used the FILE button's accent stone, so the stone that opened the
   backstage also closes it, seated in the socket's brass collar. I did not copy the socket
   itself (a logo stone in a well, 60px): the circle keeps flat's 38px.
3. **Brass as one colour.** The selected item's edge and the collar are `TAB_METAL`
   (#C6B279). The CSS draws them as radial or conic brass gradients, and azul borders take one
   colour.
4. **Keys re-measured** as in section 2. Everything else keeps flat's numbers.
5. **Font.** The chrome keeps `system:ui` 13px. I did not use the drawer's EB Garamond small
   caps (18px, 700), as W5a / W5b kept the system face.
6. **Not drawn:**
   - the drawer's shadow (`-12px 0 42px`): the sibling pane would paint over it;
   - the overlay;
   - the sheen / shaft animations.
7. **Custom palettes.** A caller's `BackstageTheme` (e.g. `from_system`, as AzWriter's
   palette.rs uses it) is the FLAT look's palette, and flora ignores it, as W5a's chrome does.
   Pin `with_theme(UiTheme::Flat)` to keep a custom palette under a flora app theme.
8. **Marker on both looks' roots** (W5a's chrome convention; W5b gives flat none).
9. **Dead focus rules.** The nav items carry `chrome_key`'s focus ring (and the selected item a
   glow ring), but no nav item is a Tab stop in either look (see section 7).

## 4. Tests

RED commit e8484d34c, green after f40f52798.

**Unit tests:** `backstage.rs` `mod flora_tests` (12 tests):

- theme option default and `set_theme` / `with_theme`;
- drawer and page colours in both modes;
- nav key colours at rest, on hover and pressed, in both modes;
- the selected item's stone and brass edge at rest, on hover and pressed, in both modes;
- the back button's stone, collar, arrow and focus ring in both modes;
- key metrics (inset, gap, and the label's indent and pitch against flat);
- `theme_checks::assert_theme_invariants` with the selected item plain and after the gap;
- pinned builds the button in its theme with no blocks, under the other app theme;
- `theme_blocks::checks::assert_follows_the_app_theme`;
- shared generic properties are declared once outside the blocks, under both app themes;
- a caller's part wins (None / Flat / Flora);
- the flat look equals the Office parts.

**Integration:**

- `layout/tests/widgets_follow_the_app_theme.rs`:
  - `backstages_follow_the_app_theme`: items 2 and 9 active, back and nav handlers, a title
    strip and pane content;
  - `the_backstage_has_a_flora_look_of_its_own`.
- `layout/tests/flat_and_flora_widgets_follow_the_light_and_dark_theme.rs`:
  `backstages_read_in_both_themes_in_both_looks` (flat and flora, two selections each, plus
  flora with pane text).
- `layout/tests/a_widget_without_a_theme_option_pins_what_it_embeds.rs`: the backstage test is
  deleted. See section 7 for what is left there.

**Commands for the parent:**

```
cargo test -p azul-layout --lib widgets::backstage
cargo test -p azul-layout --lib -- widgets::theme_pairs widgets::theme_contrast widgets::label_convention
cargo test -p azul-layout --lib widgets::themes::theme_blocks
cargo test -p azul-layout --test all widgets_follow_the_app_theme
cargo test -p azul-layout --test all flat_and_flora_widgets_follow_the_light_and_dark_theme
cargo test -p azul-layout --test all a_widget_without_a_theme_option_pins_what_it_embeds
python3 scripts/preflight_contracts.py   # ran here: OK
```

## 5. api.json list (autofix; every doc is ASCII)

| class | item | spelling |
|---|---|---|
| Backstage | struct field, LAST (after `style`) | `theme: OptionUiTheme`, doc "The widget theme, or `None` to follow the app theme (`AppConfig::with_theme`). Flat is the Office look [`Self::style`] describes; flora lays the flyout drawer of flora.css - a leaf of paper, inset keys, the selected item a sunken stone - on the same column. A part the caller set in [`Self::style`] wins in either theme." |
| Backstage | fn `set_theme` | `[{"self": "refmut"}, {"theme": "UiTheme"}]`, body `object.set_theme(theme)`, doc "Pick the widget theme: the backstage and its back button keep this look whatever the app theme is. Unset (`None`), the backstage follows the app theme (`AppConfig::with_theme`, flat by default)." |
| Backstage | fn `with_theme` | `[{"self": "value"}, {"theme": "UiTheme"}]`, returns `Backstage`, body `object.with_theme(theme)`, doc "[`Self::set_theme`] for the builder chain." |
| Backstage | fn `dom` (doc only) | "Renders the backstage in its theme: a pinned theme is that look; no theme follows the app theme (every part carries both looks, each inside its `@theme(<name>)` block, in the structure of the theme the DOM is built for). The tree is built ONCE either way, so the caller's title strip and pane content are never cloned." |

**Placement.** `theme` goes LAST, after `style`.

- `OptionUiTheme` is 8 bytes and 4-aligned.
- `BackstageStyle` is 8-aligned and a multiple of 8 in size: `BackstageTheme` is 24 bytes,
  then 9 option vecs.
- So `theme` lands on an 8-aligned offset. The struct grows by exactly 8 bytes with no new
  padding, and every existing offset stays.
- The 7 bytes of padding after `behavior: BackstageBehavior` (one bool) before `style` were
  already there.

**Rust-only:** `pub(crate) themes::flora::backstage_style`, and the private consts
`BACKSTAGE_KEY_{INSET,GAP_HALF,H,PAD}` in `flora.rs`.

## 6. Least sure to compile

1. `flora_tests` names `LayoutPaddingLeft`, `LayoutHeight`, `LayoutMargin{Left,Right,Top,Bottom}`,
   `LayoutBorderLeftWidth`, `LayoutWidth` and `StyleBackgroundContent` through `use super::*`.
   backstage.rs brings them in with `props::{layout::*, style::*}` globs, so the test relies on
   the glob-of-a-glob re-import.
2. `let shared: [(&str, &Dom, &[CssPropertyType]); 6] = [..]`: `&[T::A, ..]` literals of
   different lengths, coerced to slices by the annotation.
3. flora.rs `chrome_part(slot, &e, |v| { v.retain(..); .. })`, and `chrome_part(.., |_| {})`
   for the gap part (closures against `impl FnOnce(&mut Vec<P>)`).
4. flora.rs `for (slot, e) in [(&mut s.right_style, right), (&mut s.content_style, content)]`
   (W5a's pattern).
5. `assert_eq!(face(..).last(), Some(&flora::STONE_STREAK_HOVER))`: a borrowed temporary of a
   const that has drop glue, used within the statement.

Least sure in behaviour:

- The integration follow check resolves the merged parts under the chain `[flora, flat]` as
  well. This is the same property of the matcher every other widget relies on.
- The contrast probe averages the sunken stone's gradient stops. I estimated on-acc on the
  stone at about 10:1.

## 7. Found, not fixed

- **The base did not compile.** At 5fba6d3b9, `layout/tests/widgets_follow_the_app_theme.rs`
  had lost the closing `}` of
  `the_ribbon_quick_access_bar_and_status_bar_each_have_a_flora_look_of_their_own` in the
  W5a/W5b merge. I added it in the RED commit, since my tests append after it. If the parent's
  uncommitted copy fixes the same line, the merge is identical.
- **Stale single-look guards.** At 5fba6d3b9, `a_widget_without_a_theme_option_pins_what_it_embeds.rs`
  still holds single-look tests for the ribbon, the status bar and the quick-access bar. W5a
  gave those three widgets a theme option, so unpinned they now carry theme blocks and those
  three tests must fail. There is no node_graph test in that file on this base.
  - I deleted only the backstage test, as asked.
  - The parent's uncommitted copy of this file probably already reworks it. Expect a conflict
    at the tail.
- **Stale doc.** `UiTheme::SINGLE_LOOK`'s doc (`themes/mod.rs`) still lists the ribbon, the
  status bar, the quick-access bar and the backstage. Only the node graph (and the leftover
  status bar / ribbon `set_theme(SINGLE_LOOK)` lines that are overridden right after) still use
  it.
- **Nav items are not keyboard stops in either look.** They are clickable divs with no tab
  index. The back button (and Escape) are the only keyboard route out; there is none to the
  panes.
- **Flat look, dark mode.** The flat back button has no focus ring. The flat pane stays white
  at night: `BackstageTheme::office_2013` has no night value, and app text there takes the UA's
  light night ink. This is the Office look as it was; `from_system` covers it.
- **Twins** (already reported by W5a / W5b): `themes::decl` and `themes::style_kit`. I used
  style_kit wherever it has the helper, and decl only for what style_kit lacks:
  - paint-only border colours (`border_colors`), as W5a's chrome helpers do;
  - `margin`;
  - ring widths without colour (`border`).

## 8. Commits

- a62f1c0cb: plumbing (theme option, follow_style, marker, back button theme, flora stub, guard
  test removed)
- e8484d34c: RED
- f40f52798: the flora look
- the commit with this report (and the checkpoint file `scripts/W5C_FLORA_BACKSTAGE.PROGRESS.md`)

## 9. Left

- Compile and run section 4.
- Autofix api.json from section 5.
- Merge the tails of the shared test files and `flora.rs`: this work appends after W5b's
  titlebar section and tests. Keep both sides.
- Optional:
  - make the nav items Tab stops (a roving group, like the tree view's);
  - drop `SINGLE_LOOK` from the backstage's mention in `themes/mod.rs`.
