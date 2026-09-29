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

## DONE
(none yet)

## IN PROGRESS

## NEXT
badge, label, divider, spinner, chip, alert, card, frame, breadcrumb,
accordion, menubar, color_input, date_picker

## Open questions
