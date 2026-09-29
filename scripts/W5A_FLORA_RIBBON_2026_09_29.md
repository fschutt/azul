# W5a - flora looks and a theme option for the ribbon, the quick-access band and the status bar (2026-09-29)

Branch `wt/w5a-flora-ribbon`, cut from `fix/input-bugs-2026-09-19` @ 0a326afe5. Nothing was
compiled (house rule). **The build waits for U1**: the widgets call
`crate::widgets::themes::theme_blocks::{follow_app_theme, follow_dom}` (T3's helpers, which U1 is
moving from `themes/flat.rs` into `theme_blocks.rs`); on this base they still live in `flat.rs`.

## 1. What an app gets

`Ribbon`, `QuickAccessBar` and `StatusBar` each have a flora look (light and dark) next to their
established flat (Office) look, and a `theme: OptionUiTheme` option:

- `theme: None` (the default): the widget FOLLOWS the app theme. `dom()` (and the ribbon's
  `dom_desktop` / `dom_mobile`) builds the widget in both looks and merges them through the one
  merge helper - `theme_blocks::follow_app_theme(self, Self::flat_look, Self::flora_look)` for the
  bar and the band, `theme_blocks::follow_dom(UiTheme::current(), flat, flora)` for the ribbon
  (its builder also takes the chrome mode). No merge of my own.
- `with_theme(t)`: exactly that look, no `@theme` block anywhere.
- Every build pins what the widget builds itself to the look being built - every internal
  `Button`, the status bar's zoom `Slider` - and every embedded ribbon `ComboBox` / `DropDown` /
  `CheckBox` the caller left unpinned (a caller's pin on an embedded widget is kept). **U1 planned
  this Button pin pass-down too; it would collide with the rewrite of the same builders, so it is
  done here** (U1 can drop its ribbon/statusbar/quick_access part).
- The root carries the theme marker class (`__azul-theme-flat` / `__azul-theme-flora`).

## 2. Design

**Flat look** = the palette parts already in the widget files (`theme_*(&RibbonTheme)` & co.),
untouched byte for byte.

**Flora look** = `themes::flora::{ribbon_style, statusbar_style, quick_access_style}(style)`:
every part the caller left `None` in the widget's style bundle becomes the established part's
GEOMETRY plus flora's paint (`// ==== chrome (ribbon, quick_access, statusbar) ====` in flora.rs):

- `chrome_geometry(part)` = the flat part minus paint (`is_chrome_paint`: background, text colour,
  border colours, box shadows), minus dark twins and state rules; order kept, viewport conditions
  kept. So flora repaints the chrome and never re-measures it: the 68px item row, the 26px tab
  strip, the 23px bar, the zoom thumb's travel keep their numbers (a test pins every metric,
  `flora::chrome_metric_findings`). It also means every layout property is declared ALIKE by both
  looks, so the follow merge keeps all of them unconditional.
- `chrome_part(slot, established, paint)`: a caller's `Some(..)` part wins in both looks.
- Shared paint: `chrome_key` (bare at rest, `chrome_lift` = hover face in a `--fl-bd` hairline,
  pressed face, focus ring accent / glow), `chrome_key_states` (re-appended after a resting face a
  part lays over a key, so no state is shadowed), `chrome_leaf` (popover leaf).
- Colours: flora's tokens only (`LIGHT_*` / `DARK_*`, the raised / hover / pressed faces, the
  stones, the existing popover shadow and field-well consts). No new colour literal.

## 3. The flora looks

- **Ribbon**: flora's toolbar strip (`--fl-strip`, closed at its foot by a `--fl-bd` rule) over a
  leaf (`--fl-sur`) holding the groups, each ruled off by a `--fl-sep` hairline and captioned in
  `--fl-soft1`. Tabs are flora's nav tabs: soft ink, square-shouldered (4px top radius), lifting to
  the hover face and `--fl-ink` under the pointer; the selected tab is the sunken accent stone
  (`--fl-gem-sunken` + sunken rig, `--fl-deep` edge) in `--fl-on-acc`. The application button is
  the raised accent stone (primary command, streak brightens on hover, sinks when held). Every
  command, the launcher and the zoom keys are toolbar keys (3px radius); a toggled one is
  pushed-in paper in a `--fl-bd3` hairline. The gallery is a well of field paper (`--fl-fld`,
  `--fl-bd2`, `--fl-well` inset); its pick is the accent's soft wash rimmed in the accent; the
  More panel and the touch tab picker are popover leaves; spinner buttons ring with an inset halo.
  Touch chrome: accent text (`--fl-acc` / `--fl-glow`) on the strip, the picked group is the
  sunken stone.
- **Status bar**: the toolbar strip closed along its TOP by a `--fl-bd` hairline (an inset shadow,
  so no height), status in `--fl-soft1`, glyphs `--fl-icon`; segments, views and zoom buttons are
  toolbar keys; the active view is pushed-in paper; a `--fl-bd3` rail and 100% tick under a
  raised-paper thumb (`--fl-rT` -> `--fl-rB`, `--fl-bd2`, 2px radius); the slider rings with an
  inset halo.
- **Quick-access band**: flora's recessed desk band (`--fl-desk`), title in `--fl-intro`, glyphs
  `--fl-icon`, chevron `--fl-soft2`; every action / window control a toolbar key; the close key
  warms to clay under the pointer (`STONE_CLAY.soft` by day, `.deep` by night, `.glow` rim, deeper
  while held).

## 4. Guessed (decide)

1. **Status bar = paper, not a band.** The Office bar is an accent strip; flora.css has a navy
   `--fl-band`, but it is a hero band with no token in flora.rs, and flora keeps its accent for
   stones and rings. I made the bar flora's toolbar strip under a top hairline (the menubar's
   strip turned over). If the user wants a navy bar, it is one part (`bar_style`) plus ink.
2. **Title band = `--fl-desk`**, one step deeper than the ribbon's strip under it (window chrome
   recessed behind the leaves). Could be `--fl-pg`.
3. **Close key = clay wash** (flora's danger stone), not Windows red and not round (flat's close
   hover is a round red fill; flora keeps its 3px square-shouldered keys).
4. **Selected tab = sunken accent stone** (flora.css `.nav-links a.active`), without the metal
   surround / coved shoulders (`.fl-tab-*` needs pseudo-elements; W5b's tabs may choose
   differently - worth aligning).
5. **Gallery pick at night = `DARK_HT` + glow rim**: `--fl-soft` has no night value in flora.css
   (it stays #E0E4EE, a light island on a dark window).
6. **A caller's custom palette** (`RibbonTheme` / `StatusBarTheme` / `QuickAccessTheme`, e.g.
   `from_system`) is the FLAT look's palette. The flora look ignores it (flora has its own
   tokens); pin `with_theme(UiTheme::Flat)` to keep a custom palette under a flora app theme.
   Parts the caller set as `Some(..)` stay the caller's in both looks (so AzWriter's patched
   `container_style` stays Office-white under flora - its own choice).
7. `RibbonStyle::styled_combo_box` still returns the flat (Office) field parts; under flora the
   caller's combo keeps them (caller parts win). A flora variant would need a theme argument.

## 5. Tests

RED commit 4637a0edc, green after 0d39eebd4:

- `ribbon.rs` `flora_tests` (9), `statusbar.rs` `flora_tests` (7), `quick_access.rs`
  `flora_tests` (6): the looks above in both modes, the theme invariants
  (`theme_checks::assert_theme_invariants`: pairs, no shadowed state, every Tab stop ringed by day
  and night) - ribbon in all three chromes, every metric kept, caller part wins, pinned widget
  builds its buttons / slider / unpinned embedded widgets in its theme and carries no theme block.
- `layout/tests/widgets_follow_the_app_theme.rs`: `ribbons_follow_the_app_theme_in_every_chrome`,
  `status_bars_follow_the_app_theme`, `quick_access_bars_follow_the_app_theme`
  (`assert_follows_the_app_theme`: unpinned under T resolves like `with_theme(T)` on every node,
  light/dark, rest/hover/active/focus; both blocks; pinned ignores the app theme; a11y tree the
  same), and `the_ribbon_quick_access_bar_and_status_bar_each_have_a_flora_look_of_their_own`
  (else the follow check would pass trivially). The three left the single-look guard (tabs,
  titlebar, tree_view stay for W5b).
- `layout/tests/flat_and_flora_widgets_follow_the_light_and_dark_theme.rs`:
  `ribbons_title_bands_and_status_bars_read_in_both_themes_in_both_looks` (contrast >= 2:1, no
  light island at night, dark twins after their light half) - flat AND flora.
- The workspace lint `widgets::theme_pairs` walks the manifest's unpinned (followed) widgets, so
  both looks' pairs are checked there (per `@theme` block).
- Existing unit tests that read the flat look's exact declarations are pinned to `UiTheme::Flat`
  (ribbon: 8 constructions incl. `render_item`; statusbar: 1).

Commands for the parent (after U1 lands):

```
cargo test -p azul-layout --lib widgets::ribbon
cargo test -p azul-layout --lib widgets::statusbar
cargo test -p azul-layout --lib widgets::quick_access
cargo test -p azul-layout --lib widgets::theme_pairs
cargo test -p azul-layout --lib widgets::theme_contrast
cargo test -p azul-layout --test all widgets_follow_the_app_theme
cargo test -p azul-layout --test all flat_and_flora_widgets_follow_the_light_and_dark_theme
cargo test -p azul-layout --test all ribbon statusbar window_control_click flex_intrinsic_text
```

## 6. Commits

380e204f4 plumbing (theme option, pin pass-down, marker, flat pins) - 4637a0edc RED -
0d39eebd4 flora looks - + checkpoint / report commits.

## 7. api.json (autofix; docs ASCII)

| class | item | spelling |
|---|---|---|
| Ribbon | struct field, LAST (after `behavior`) | `theme: OptionUiTheme`, doc "The widget theme, or `None` to follow the app theme (`AppConfig::with_theme`). Flat is the Office look `style` describes; flora lays flora's paper, hairlines and stones on the same metrics. A part the caller set in `style` wins in either theme." |
| Ribbon | fn `set_theme` | `[{"self": "refmut"}, {"theme": "UiTheme"}]`, body `object.set_theme(theme)`, doc "Pick the widget theme: the ribbon, its buttons and every embedded widget without a theme of its own keep this look whatever the app theme is. Unset (`None`), the ribbon follows the app theme (`AppConfig::with_theme`, flat by default)." |
| Ribbon | fn `with_theme` | `[{"self": "value"}, {"theme": "UiTheme"}]` -> `Ribbon`, body `object.with_theme(theme)`, doc "[`Self::set_theme`] for the builder chain." |
| StatusBar | struct field, LAST (after `style`) | `theme: OptionUiTheme`, doc as Ribbon's with "flora's toolbar strip and paper keys" |
| StatusBar | fns `set_theme` / `with_theme` | as Ribbon's (returns `StatusBar`); set_theme doc "... the status bar, its buttons and its zoom slider keep this look ..." |
| QuickAccessBar | struct field, LAST (after `top_inset`) | `theme: OptionUiTheme`, doc as Ribbon's with "flora's recessed band and paper keys" |
| QuickAccessBar | fns `set_theme` / `with_theme` | as Ribbon's (returns `QuickAccessBar`); set_theme doc "... the band and its buttons keep this look ..." |

Docs changed: `Ribbon::dom` / `StatusBar::dom` / `QuickAccessBar::dom` now render "in its theme"
(pinned = that look, none = follows the app theme).

Layout: `OptionUiTheme` is 8 bytes, 4-aligned. `StatusBar` and `QuickAccessBar` are 8-aligned with
sizes that are multiples of 8 and end on an 8-aligned field / `f32` pair, so each grows by 8 with
no padding. **`Ribbon` ends with `behavior: RibbonBehavior` (5 bools)**: appending `theme` LAST (as
asked) keeps every existing offset but moves the struct's 3 tail-padding bytes between `behavior`
and `theme` (size +8). Putting `theme` before `behavior` would be padding-free inside but move
`behavior`'s offset. Decide which the FFI generator prefers.

Rust-only (`pub(crate)`): `themes::flora::{ribbon_style, statusbar_style, quick_access_style}`,
`themes::decl::themed_border_right_color`; `#[cfg(test)] themes::flora::{CHROME_METRICS,
chrome_metric_findings}`.

## 8. Least sure to compile

1. `theme_blocks::follow_app_theme` / `follow_dom` paths and signatures (U1). Assumed T3's:
   `follow_app_theme<W: Clone>(W, fn(W) -> Dom, fn(W) -> Dom)`, `follow_dom(UiTheme, Dom, Dom)`.
2. `follow_app_theme(self, Self::flat_look, Self::flora_look)` - method fn items coerced to
   `fn(StatusBar) -> Dom`.
3. flora.rs `for (slot, e) in [(&mut s.a, x), (&mut s.b, y)]` - two disjoint `&mut` field
   borrows in one array (the resolved parts are computed before it, so no `&s` overlaps).
4. `decl::border_colors(c).map(P::simple)` with `type P = CssPropertyWithConditions` (array
   `map`, assoc fn through a type alias).
5. Test modules: `&&&Dom` -> `&Dom` deref coercion in `tabs.iter().find(|t| tc::has_class(t, ..))`;
   `Ribbon::with_theme` (const fn) coerced to `fn(Ribbon, UiTheme) -> Ribbon` in `pinned(..)`.

Behaviour, least sure:
1. `the_flora_*_keeps_every_theme_invariant*`: relies on every Tab stop being a Button with a
   chrome key (border ring) or the spinner / slider halo; an embedded widget I did not foresee
   would need its own ring.
2. The status bar's zoom `Slider` appends its own dark paint to injected styles
   (`flora::slider`: `DARK_TRACK` track, accent-orb thumb at night; `flat::slider`: the desktop
   field / accent). So a flora bar's thumb is raised paper by day and an accent orb by night - the
   slider's policy, not the bar's; the tests only pin the day thumb.
3. Live restyles (gallery pick, touch group pick) write only the unconditional (light) half of a
   part at runtime - pre-existing in flat, same in flora. The touch group pick is the sunken stone
   (mode-neutral) so it survives that; the gallery pick at night does not (flat has the same gap).
4. An unpinned status bar's slider without a name warns (`warn_widget_needs_a_name`) once per
   look, i.e. twice per follow build (T3's known doubled warning).

## 9. Twins found (NO DUPLICATION ruling)

- `themes/decl.rs` (W3a) and `themes/style_kit.rs` (W3b) are two helper kits for the same
  concern (themed fills / inks / borders / focus rings / hover and active pairs). I used
  style_kit and, where it has no paint-only border colour, decl (added the missing
  `themed_border_right_color` there rather than a third copy). They should be merged.
- `theme_blocks::every_theme_dom` (T2) and `flat::follow_dom` (T3) - the two merges U1 is
  deduplicating.

## 10. Files touched that other tasks own (kept minimal)

- `themes/decl.rs`: one appended sibling fn (`themed_border_right_color`).
- `layout/tests/widgets_follow_the_app_theme.rs`: the single-look guard lost its three entries and
  their imports (W5b edits the same guard for tabs / titlebar / tree_view - expect a trivial
  conflict); my tests appended at the end.
- `layout/tests/flat_and_flora_widgets_follow_the_light_and_dark_theme.rs`: one appended test.
- No change to `theme_blocks.rs` / `theme_checks.rs` (U1's) or `all.rs`.

## 11. Left

- U1's merge move (build blocker, see top).
- Optional: `styled_combo_box` in the flora look (section 4.7); merge decl / style_kit.
