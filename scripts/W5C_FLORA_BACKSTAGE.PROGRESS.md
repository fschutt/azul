# W5C_FLORA_BACKSTAGE - progress (branch wt/w5c-flora-backstage, from 5fba6d3b9)

Task: flora look (light + dark, after flora.css's flyout drawer `.mobile-menu`)
and a theme option for the Backstage; unpinned follows the app theme through
`theme_blocks::follow_props` (part by part, the DOM built once, so the
caller's title strip / pane content is never cloned).

## Design (decided)
- Flat look = the established palette parts in backstage.rs, untouched.
- Flora look = `themes::flora::backstage_style(style)`: every part the caller
  left `None` becomes the flat part's GEOMETRY (W5a's `chrome_geometry` /
  `chrome_part`) + flora paint. Caller's `Some(..)` parts win in both looks.
- `theme: OptionUiTheme` LAST (after `style`), `set_theme` / `with_theme`.
  Pinned: that look; `None`: `follow_style` merges every part with
  `follow_props`; the root carries the structure theme's marker; the back
  Button takes the backstage's `theme` (the SINGLE_LOOK pin is gone).

## DONE
- (plumbing) theme option, follow_style, marker, back button theme,
  flora stub, backstage test removed from the single-look guard

## IN PROGRESS
- RED tests

## NEXT
- RED: backstage.rs `flora_tests`, follow test + own-look guard in
  layout/tests/widgets_follow_the_app_theme.rs, light/dark entry
- flora look in flora.rs `// ==== backstage ====`
- report scripts/W5C_FLORA_BACKSTAGE_2026_09_29.md

## Open questions
- (see report)
