# RIBBONSCALE14 progress (worktree agent-a4c56fe2d378400f7, base 82e3d1947)

Task: the Ribbon scales a tab that does not fit its width the way Office 2010 does (groups shrink right
to left: large buttons to medium, galleries to fewer cells, buttons to icons, then a group collapses
into one button that opens the whole group in a popup). Found by AzDrive's E2E (View tab, Options at
x = 1378 in a 1280 px window). No compiling (house rule): the lead builds and runs the tests.

## Design (decided)
- Width source: the window the DOM is built for. The engine enters a `WindowSizeScope`
  (core/src/callbacks.rs) around `layout()` (dll shell common/layout.rs, web html_render.rs), the
  twin of `app_theme::ThemeScope`; a widget asks `build_window_width_less_than(px)`, which RECORDS
  the query like `LayoutCallbackInfo::window_width_less_than` - so a resize across one of the
  ribbon's steps re-runs layout() and any other resize re-flows the DOM. No app change.
  Override: `Ribbon.available_width` (0 = the window's) for a ribbon in a narrower box.
- Widths: estimated at build time from the part styles' own metrics (paddings, borders, 32/16 px
  icons, 120 px gallery cells) + Helvetica advances x 1.1 for the labels (errs wide), 4 px slack.
- Steps (all that make the tab narrower, in order): 1 medium (large -> small buttons, stacked three
  to a column) R->L; 2 galleries one cell at a time down to 2 (list galleries to one column) R->L;
  3 small (every button its icon alone, named by its label) R->L; 4 collapse R->L. The first step
  whose total fits is built.
- Collapsed group: one large Menu button (group icon - `RibbonGroup.icon`, else its first button's
  - over its label and the arrow) spanning the group, plus a `<transient-window>` (closed, Bottom,
  Outside) holding the FULL group on a `group_popup_style` panel. The button's click toggles it
  (engine's open state). In the popup: a command (plain button, split main part, gallery cell) is
  noted, the popup closes, and the app runs it in its own window on Dismissed (File menu pattern);
  every other callback (check boxes, combos, menus, custom DOM) runs in the popup with RefreshDom
  widened to RefreshDomAllWindows.
- Top-level rows (RibbonX boxes) stack three to a column like small items (AzDrive's Show/hide).

## DONE
- c57edd063 progress file
- 99e4b7348 RED core: build_window_tests (core/src/callbacks_test.rs)
- 01170ec3b core WindowSizeScope + build_window_width_less_than; shell + web enter the scope
- d4a8ed0c3 RED ribbon: a_row_at_a_groups_top_level_stacks_with_the_small_items
- 41a9f14de fix: rows stack (is_small_item)
- aa9b408d5 RED ribbon scaling: layout/src/widgets/ribbon_scaling_tests.rs (+ mod line) and
  layout/tests/a_ribbon_tab_wider_than_its_window_keeps_every_control_inside_it.rs (all.rs)
- 87d238b8e fix: the scaling, the collapsed group + popup, flora's group_popup_style
- (next) test: a scaled ribbon declares its structure once for every theme
- Python port of the estimate (/tmp, not committed): View tab 1225.5 px as built (fits 1280),
  1000 -> Panes medium + Layout 2 cells, 800 -> + Options / Show/hide / Current view icons,
  600 -> + Panes icons + Show/hide collapsed. AzWriter Home 1210 (fits 1280), 900 -> Styles 2
  cells + Editing / Paragraph icons, 700 -> + Font icons.

## IN PROGRESS
- (none) - report to the lead.

## NEXT
- Only if the lead's build / tests fail: fix in ribbon.rs / ribbon_scaling_tests.rs / core
  callbacks.rs, one commit each. Tuning knobs: TEXT_WIDTH_FACTOR (1.1), FIT_SLACK_PX (4),
  GALLERY_MIN_CELLS (2), CUSTOM_FRAME_PX (8).
