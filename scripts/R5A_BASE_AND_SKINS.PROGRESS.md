# R5A_BASE_AND_SKINS - progress checkpoint

Branch `wt/r5A-base-and-skins`, base `d240a1b1d`. Widgets: accordion, alert, avatar, backstage,
badge, breadcrumb, button, card, check_box, chip, color_input.

## DONE
- `1a501bee3` RED: one structure-lint test per widget (`assert_structure_is_shared`, `&[]`).
- `cd89505ca` GREEN accordion: `ACCORDION_{CONTAINER,SECTION,HEADER,TITLE,CHEVRON}_BASE`, laid
  first by `accordion::build`; flat / flora skins only.
- `692fff6ac` GREEN breadcrumb: `BREADCRUMB_ITEM_BASE`, `BREADCRUMB_LABEL_BASE`; two static-reading
  tests now read base + flat skin.
- `501adb32b` GREEN color_input: `PICKER_{PANEL,PREVIEW,EYEDROPPER}_BASE_CSS` sheets before the
  skin sheets.
- `08da0680c` alert / badge / card / chip: structure authored once in the widget file
  (`ALERT_{CONTAINER,MESSAGE,CLOSE}_BASE`, `BADGE_BASE`, `CARD_BASE`, `CHIP_{CONTAINER,REMOVE}_BASE`
  + `CHIP_LABEL_STYLE` as the label's base); 3 incidental tests updated.
- `6060a3b59` `decl::on_base(base, skin)` replaces the four inline base-then-skin twins.

## IN PROGRESS
- Report `scripts/R5A_BASE_AND_SKINS_2026_09_29.md`.

## NEXT
- Nothing after the report.

## Decisions
- avatar: flat::avatar and flora::avatar are identical twins; NOT deduped (the structure is already
  avatar.rs's `build_avatar_style`, and editing the middle of the shared theme files risks conflicts
  with R5-B/C/D). Reported.
- button, check_box, backstage: no code change (one base already; guards only).
- color_input test prunes nested text inputs / labels (other agents' widgets).
- Parent facts (after the power loss): `theme_blocks::stack_parts` exists in main only - not copied,
  backstage's `merged_style` untouched; nothing here stacks merged parts (every base + skin is one
  list before the merge). The lint prints CSS names in main (`flex-direction`), Debug names here.

## Audit notes
- accordion: container OverflowX/OverflowY cross (flat declares them last, flora right after the
  font); header Cursor/UserSelect cross (flat after the padding, flora before it).
- breadcrumb: crumb Cursor crosses UserSelect (flat CUR, US; flora US, ink, CUR).
- color_input: the picker's panel / preview / eyedropper CSS strings are one rule each: a rule is
  never split, so display / flex-direction / position / overflow / align / justify / cursor go into
  the theme blocks with the paint.
- alert, badge, card, chip: structure alike and in the same order (passes), but flora.rs writes its
  own copy -> base in the widget file.
- avatar: flat and flora builders are identical twins (both call avatar.rs's style builders).
- button, check_box: both theme builders start from the widget file's resolved style; no structure
  in the theme files.
- backstage: flora's parts are `chrome_geometry(flat part)` + paint: the structure is the widget
  file's, once.

## Open questions
- (none)
