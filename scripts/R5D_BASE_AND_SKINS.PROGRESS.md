# R5D_BASE_AND_SKINS - progress checkpoint

Branch `wt/r5d-base-and-skins` from `d240a1b1d`. Widgets: statusbar, stepper, switch, tabs, text_area,
text_input, time_picker, titlebar, toast, tooltip, tree_view, video.

## DONE

- c19e2ef1d RED: one `structure_tests` test per widget (`a_<widget>_declares_its_structure_once_for_every_theme`).

## IN PROGRESS

- GREEN stepper.

## NEXT

1. GREEN stepper: `CIRCLE_BASE` / connector / label bases in stepper.rs; flora uses them; circle box-sizing
   unified to border-box.
2. GREEN tabs: `TAB_BASE` (box-sizing content-box, align-items center, cursor pointer), header / after /
   panel bases; allow header align-items (flora end) and before-tabs flex-grow (flat 1, flora 0).
3. GREEN text_input: search row base + clear-button base (display flex/none, justify/align center);
   live show writes `flex`.
4. GREEN (already shared, flora restates): time_picker, toast, tooltip, tree_view bases.
5. Report `scripts/R5D_BASE_AND_SKINS_2026_09_29.md`.

## Audit (read from the builders)

- Already shared, no code change: statusbar (flora repaints flat geometry via `chrome_part`), switch (one
  look), text_area (both builders start from the widget's resolvers), titlebar (`container_style_painted`
  / `title_style_painted` are the widget's), video (no structure in the poster).

## Open questions

- none yet
