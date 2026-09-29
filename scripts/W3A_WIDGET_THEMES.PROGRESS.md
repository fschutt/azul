# W3a widget themes - progress (checkpoint file)

Branch: `wt/w3a-widget-themes` (cut from `fix/input-bugs-2026-09-19` @ 9c6065b05).
Task: flat/flora theme option (light + dark, focus rings) for accordion, alert,
badge, breadcrumb, card, chip, color_input, date_picker, divider, frame, label,
menubar, spinner; Spinner makeover (spokes + ring, animated, reduced motion).
Final report goes to `scripts/W3A_WIDGET_THEMES_2026_09_29.md`.

## Design (decided)
- Each widget: `theme: OptionUiTheme` field, `set_theme` / `with_theme`, and
  `dom()` dispatches to `themes::flat::<widget>` / `themes::flora::<widget>`.
- Flat = the widget's established look (light values never move) plus any
  missing dark twins / focus rings. Flora = flora.css language (paper faces,
  hairline bd rules, small-caps labels, accent stone, brass ink), light + dark.
- Theme code appended at the END of flat.rs / flora.rs under
  `// ==== <widget> ====` banners (W3b appends there too).
- Engine facts: `animation` is transition-only; looping keyframes run through
  `-azul-animation-in` with `infinite`; one in-track per node; CSS transform
  beats the anim transform on the same node (so static rotation goes on a
  wrapper); `CssDuration` is u32 (no negative delay) -> one phase-rotated
  `@keyframes` per spoke; SVG clip paths need an ancestor `SvgNodeData::ViewBox`
  (else the path is window-absolute) and the `cpurender` feature.

## Workflow per widget (3 commits + checkpoint)
1. `refactor(<w>)`: theme field + set/with_theme + dispatch; flat = old look
   byte for byte, flora = flat stub (no visual change, compiles).
2. `test(<w>): RED` - tests in the widget's `mod theme_tests` + one test in
   `layout/tests/flat_and_flora_widgets_follow_the_light_and_dark_theme.rs`
   (contrast + dark-twin order in BOTH looks). Compiles against commit 1.
3. `feat(<w>)`: the flora (and any flat dark/focus) implementation.
Commit messages: write to scratchpad/w3a/msg.txt (the scratchpad root msg.txt
is shared with other agents!). Append to files via Write-to-scratch + `cat >>`.

## DONE
- themes/decl.rs (shared builders) + integration test file: 255d57be2, f68a68f3f
- badge: 255d57be2 (plumbing), f68a68f3f (RED), 4aadcfec4 (flora).
  API: `Badge.theme: OptionUiTheme` appended after `badge_style`;
  `set_theme(&mut self, UiTheme)`, `with_theme(self, UiTheme) -> Badge`.
  Flora stones palette `FloraStone` + STONE_ACCENT/LEAF/CLAY/SLATE/AMBER in flora.rs.

- label: 1e9bf0f55 (plumbing), e4c375e9c (RED), 323a262a1 (flora ink INTRO).
  API: `Label.theme` appended after `label_style`; set_theme / with_theme.

- divider: a3225bdee (plumbing), ab3aad664 (RED), f682418fb (flora SEP hairline).
  API: `Divider.theme` appended after `divider_style`; set_theme / with_theme.

- spinner: e101c8551 (plumbing), 96f0b26bd (RED makeover tests), f666983df
  (impl), 98c5bba05 (cleanup). API: new enum `SpinnerStyle {Auto, Spokes,
  Ring}` (repr C); fields now size, spinner_style, indicator: SpinnerStyle,
  theme: OptionUiTheme, color: OptionColorU, track_color: OptionColorU;
  set_indicator/with_indicator, set_theme/with_theme; default size 32.
  Build in `spinner::build(s, &SpinnerLook)`; flat/flora supply the look.

- chip: 4c6add68b (plumbing: ChipLook + chip::build; decl::shadow single-side,
  decl::focus_halo), 993691435 (RED), 764fb51fe (flat focus halos + flora tag).
  API: `Chip.theme` appended after `container_style`; set_theme / with_theme.
  Focus-ring convention: bordered node -> decl::focus_ring (border colour);
  borderless -> decl::focus_halo (2px spread shadow). Flat colours FIELD_RING /
  flat DARK_ACC; flora LIGHT_ACC / DARK_GLOW.

- alert: dd14ba85c (plumbing: AlertLook + alert::build), 3db7709e5 (RED),
  889aaf031 (flat close halo + flora leaf banner w/ stone thread).
  API: `Alert.theme` appended after `container_style`; set_theme / with_theme.
  flora.rs consts LEAF_SHADOW_LIGHT/DARK (= --fl-shadow-1) live in the alert
  section; reuse for card/frame.

- card: c0616d9bb (plumbing: card::build(card, style, classes)), 7d6ac6427 (RED),
  691f3714f (flora leaf card). API: `Card.theme` appended after `on_click`;
  set_theme / with_theme.

- frame: 939859d31 (plumbing: FrameLook + frame::build; FRAME_*_STYLE exports),
  b5a8cfa9e (RED), 7888c9b46 (flora label + BD rules). API: `Frame.theme`
  appended after `content`; set_theme / with_theme.

- breadcrumb: d4693525e (plumbing: BreadcrumbLook + breadcrumb::build),
  4cef5c643 (RED), e042d5f31 (flat hover underline + halo; flora brass trail,
  chevron separator). API: `Breadcrumb.theme` appended after
  `container_style`; set_theme / with_theme.

- accordion: 3ce4acd09 (plumbing: AccordionLook + accordion::build;
  decl::focus_halo_inset), a48742efe (RED), 7c69e9d8e (flat hover + inset
  ring; flora FAQ list). API: `Accordion.theme` appended after `on_toggle`;
  set_theme / with_theme.

- menubar: bc1e6059e (plumbing: NEW `Menubar { menu, theme }` Rust-only
  struct; build_menubar_dom = Menubar::create(menu).dom(); menubar::build with
  style closures), a85f45fcc (RED), 26d474db7 (flora strip). API: `Menubar`
  (create / set_theme / with_theme / dom) - not in api.json (neither was the
  function).

- color_input: 0482f8bc2 (plumbing: ColorInputLook + color_input::build,
  PANEL/PREVIEW/EYEDROPPER/GRIP_HANDLE_CSS consts), 8e4a9f7a4 (RED; also
  updates 2 existing tests to the focus-ring contract), b5415b72f (flat
  halos + night preview/grip; flora framed swatch + leaf picker).
  API: `ColorInput.theme` appended after `accessibility_name`; set_theme /
  with_theme.

## IN PROGRESS

## NEXT
date_picker, then final report

## Open questions
- ENGINE GAP (spinner): `-azul-animation-in` tracks are started only by
  `LayoutWindow::finish_reconciliation`, which only the E2E runner calls
  (dll desktop shell has its own reconcile, never starts in-tracks); and
  only for mount ROOTS, never on the initial mount (window.rs ~11914).
  So a declared looping spinner does not spin on desktop. Report it.
