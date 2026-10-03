# WIDGETS7 - widget bugs (wave 7)
Owns: the EXISTING widgets in layout/src/widgets/* and their theme code (appends in flat.rs / flora.rs). New
widgets DataTable / Chart belong to DATATABLE7 / CHART7. Read first: scripts/PIM6_2026_10_03.md ("Seen broken"),
scripts/WRITER6_2026_10_03.md, scripts/DEDUP_WIDGETS_API_2026_10_02.md.

1. CloseGuard reads `dirty` when the DOM is built, so an app that saves and then closes in the same callback is
   vetoed by its own guard (AzCalendar works around it with with_dirty(false) + its own veto). Make the guard ask
   the app at close time (a callback returning dirty / a RefAny the app updates) so the decision is made when
   CloseRequested fires. Keep with_dirty for static cases. List the API change.
2. TextInput placeholders are drawn in the full ink colour (should be the placeholder / secondary ink).
3. AzCalendar backstage, Calendars page: the first row's TextInput garbled and the colour DropDown's caret drawn
   inside the text field - reproduce on the prebuilt AzCalendar, root-cause in the widgets.
4. The navigator's / To-Do bar's DatePicker is wider than its pane (it must fit / shrink).
5. ToDoBar has no week start (its calendar is Sunday-first): with_week_start like DatePicker, passed through.
6. Visuals (WRITER6): the flora-dark zoom slider, the check box colour that reads as transparent in the HTML dump.
7. DEDUP: ModuleSwitcher vs ShellNavigationPane's switcher (the same Outlook buttons; they disagree on wrapping;
   only the pane has badges; 7 apps use the pane) - one implementation. MessageList -> a generic name
   (SummaryList) since AzNotes uses it as a generic list: only if the rename is mechanical; list api.json renames.

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/WIDGETS7.PROGRESS.md exact. Finish with the report
scripts/WIDGETS7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.
