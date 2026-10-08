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
  icons, 120 px gallery cells) + Helvetica advances x 1.1 for the labels (errs wide).
- Steps (all that make the tab narrower, in order): 1 medium (large -> small buttons, stacked three
  to a column) R->L; 2 galleries one cell at a time down to 2 (list galleries to one column) R->L;
  3 small (every button its icon alone, named by its label) R->L; 4 collapse R->L. The first step
  whose total fits (with 8 px slack) is built.
- Collapsed group: one large Menu button (group icon - `RibbonGroup.icon`, else its first button's
  - over its label and the arrow) spanning the group, plus a `<transient-window>` (closed, Bottom,
  Outside) holding the FULL group on a `group_popup_style` panel. The button's click toggles it
  (engine's open state). In the popup: a command (plain button, split main part, gallery cell) is
  noted, the popup closes, and the app runs it in its own window on Dismissed (File menu pattern);
  every other callback (check boxes, combos, menus, custom DOM) runs in the popup with RefreshDom
  widened to RefreshDomAllWindows.
- Top-level rows (RibbonX boxes) stack three to a column like small items (AzDrive's Show/hide).

## DONE
- (none yet)

## IN PROGRESS
- core scope RED test + fix

## NEXT
- rows stacking RED + fix; ribbon scaling RED (unit + layout test) + fix; flora group_popup_style;
  report (api.json entries, tests to run, what to look at).
