# PIM6 progress (wave 6, 2026-10-03)

Branch `wt/pim6` from `25d78e309`. Brief: scripts/waves/wave6/PIM6.md.
Scratch (shared scratchpad dir - use the `pim6/` subfolder only, other agents overwrite the rest):
`/private/tmp/claude-501/-Users-fschutt-Development-azul/344f2a1f-485e-4b53-8631-15c97f8eeca1/scratchpad/pim6/`
(look.py = headless LOOK driver: `look.py <App> <steps.py> -- <app args>`, through run_capped.sh).
Screenshots: target/pim6-shots (not committed).

## DONE
- 6bb6fcff7 progress file
- 74b3fa634 test(widgets): RecurrenceEditor RED (layout/src/widgets/recurrence_editor.rs + mod.rs entry)
- (this commit) RecurrenceEditor GREEN part 1: RecurrenceRule::create / to_rrule / from_rrule
  (parse_rrule, parse_basic_date), apply(Part), whole(); date_picker.rs `weekday` and
  `days_in_month` made pub(crate) (reused, not copied).

- 78f4e7c60 GREEN part 2 (editor create / on_change / dom / build / handlers / fixtures)
- 5e93d320b GREEN part 3 (flat + flora looks appended)
- f0f9898cd manifest entry + INPUTS group
=> RecurrenceEditor DONE (uncompiled). api.json list: see the report draft below.

## IN PROGRESS
- AzCalendar adopts the RecurrenceEditor (editor_ui.rs repeat rows; editor.rs form keeps a
  `Rule`), then DatePicker week start at editor_ui.rs date_picker().

## (old notes, done) RecurrenceEditor GREEN part 2 steps, in layout/src/widgets/recurrence_editor.rs:
  1. replace the 4 remaining `todo!()`: `RecurrenceEditor::create` (week_start Monday default,
     completion_option false), `set_on_change`, `dom()` (match theme: flat/flora::recurrence_editor,
     None -> theme_blocks::follow_app_theme), `build()`.
  2. build(): base statics RECURRENCE_COLUMN_BASE / ROW_BASE (flex row, wrap, align center) /
     LABEL_BASE (no grow/shrink, user-select none) / FIXED_BASE / WEEKDAYS_BASE; rows: "Repeats"
     + Segmented(FREQUENCY_LABELS); if not Never: "Every" + NumberInput(min 1, max 999) + unit
     ("day"/"days".."year"/"years"); Weekly: "On" + 7 toggle Buttons in week_start order
     (WeekdayData{day, shared}); Monthly: "On" + Segmented["On day N", "On the <ordinal|last>
     <Weekday>"]; "Ends" + Segmented(END_LABELS) + NumberInput+"time(s)" | DatePicker(until,
     week_start, "Last date"); completion_option: label("") + CheckBox + COMPLETION_LABEL.
     Root: classes EDITOR_CLASS (+look.marker), Grouping a11y named "Repeat" (value = to_rrule).
     Part handlers on_*_part -> change(&mut shared, info, Part::..) -> apply + invoke app cb.
  3. themes: APPEND `// ==== recurrence_editor ====` at END of themes/flat.rs and flora.rs:
     `recurrence_editor_look()` (decl::font_size(13), SYSTEM_UI_FAMILY, themed_ink(LIGHT_INK,
     DARK_INK); row margin + ColumnGap; label px_width(72) + themed_ink(SOFT1); unit SOFT1;
     number px_width(64); weekdays ColumnGap 4; flora marker style_kit::FLORA_CLASS) and
     `pub fn recurrence_editor(e) -> Dom { build(e, &look()) }`.
  4. manifest: `fixtures::sample()` in recurrence_editor.rs (#[cfg(test)]); widgets/mod.rs
     `all.push(("recurrence_editor", ..))` after rich_text_editor's push, and "recurrence_editor"
     appended to the INPUTS list of the theme-contrast groups.

## NEXT (after the widget)
- AzCalendar: editor repeat rows -> RecurrenceEditor (editor.rs keeps Rule; convert via
  to_rrule/Rule::parse); DatePicker week start editor_ui.rs ~262; events/calendars/tasks on the
  Drive via a Thread (azul_appkit::files jobs); edit this occurrence (RECURRENCE-ID style: an
  exception + a detached event); close check: CloseRequested + prevent_window_close when the
  form is dirty; `__azcal_` prefix constants.
- AzTasks: detail repeat -> RecurrenceEditor (completion option); VTODO import/export through
  azul_pim::content_line; theme+mode kept in settings; DatePicker week_start from settings
  (detail.rs:347/471); body margin 0 (LOOK); pane widths (LOOK).
- AzContacts: photo preview (data: URI -> ImageRef), CSV import with column mapping, birthday
  as DatePicker.
- E2E scripts cover the fixed flows; report scripts/PIM6_2026_10_03.md.

## Seen broken (LOOK, prebuilt aa59b2d84, headless) - screenshots in target/pim6-shots
AzCalendar:
- 15-minute events in Week: title and time stacked into a 12 px block, each line squeezed to
  6 px (flex items with overflow:hidden shrink to 0 min-height) - APP bug (timegrid.rs
  event_block: spans need flex-shrink: 0; short blocks one line "title, time").
- "Lunch with Ana 12:30 - 13:30": the time line's glyphs clipped after ~5 px though its box is
  14 px - looks like an ENGINE clip bug (display list clip of an overflow:hidden span) -> owner
  MAILENG6 (solver3) - to confirm with get_display_list.
- Navigator DatePicker (nav pane) and the To-Do bar's DatePicker are wider than their panes:
  the last column is cut. To-Do bar calendar is Sunday-first while the navigator is
  Monday-first (ToDoBar has no week start).
- Backstage: the nav item before the gap ("Calendars") is drawn displaced (x=126 overlapping the
  page on Info, x+6 on other pages) - ENGINE (layout / incremental relayout) -> MAILENG6/HEADLESS6.
- Backstage Calendars page: first row's TextInput garbled/taller, the colour DropDown's caret
  drawn inside the text field, "Name" placeholder garbled.
- Text inputs: placeholders drawn in the full ink (look like typed values) - widget/text_input.
- Editor window (headless now runs it): form OK; Repeat row = Segmented only for Never.
AzTasks:
- body has an 8 px margin (left/bottom white strip) - APP (BODY css lacks margin: 0).
- nav pane content clipped at its right edge; list column narrow (reminder banner wraps, "Dismiss"
  cut), detail pane far too wide.
- After Cmd+2 (Upcoming) the list renders with overlapping/duplicated bold text (stale glyph
  positions); backstage settings: drop-down's "Work" drawn outside its box, search box text
  garbled - ENGINE incremental relayout / display list (HEADLESS6 or MAILENG6); captured
  display lists target/pim6-shots/tasks_dl_*.json for analysis.
- Clicking a task title in "All" view: list and detail went blank (tasks_detail_flat_light.png).
AzContacts: not looked at yet.

## Decisions
- D1 RecurrenceEditor speaks RRULE text both ways (to_rrule / from_rrule of the subset it shows);
  apps parse/write the full rule with azul_pim::rrule. The widget cannot depend on azul_pim
  (layout crate); the partial RRULE parser in the widget is the one twin, reported.
- D2 weekday bits Monday-first (bit 0 = MO), 0 = the start's weekday; the default week start of
  the editor is Monday (RFC 5545 WKST default).
- D3 Disk was full at ~02:40 (ENOSPC) and the Mac is on battery: no more long headless runs until
  told; commit after every small unit.
