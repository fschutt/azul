# OFFICE7 - the Office apps' leftovers (wave 7)
Owns: examples/azul-{writer,notes,mail,sheets,show,review}. Engine bugs: RED test in layout/tests + a note for the
owning task (PLAN.md), never an app workaround. Read first: scripts/{WRITER6,MAIL6,SHEETSHOW6,SMALL6,MEDIA6}_2026_10_03.md.

1. AzWriter / AzNotes ids: WRITER6 wrote them as `&str` consts; make them `const AzString` via
   `AzString::from_const_str` (PREFIXES ruling), every use site adjusted.
2. AzReview examples/azul-review/src/ui.rs:547 tracks pointer movement on HoverEventFilter::MouseOver (fires only
   on entry since 2a7712f66) -> MouseMove (MEDIA6 did the same for AzPaint / AzPhoto).
3. AzWriter: the import reads the picked file on the UI thread -> a Thread through the Drive pattern; and
   `Update::max_self` (core) is not exported, so commands::merge re-implements it - list `Update.max_self` for
   api.json and use it (no twin).
4. AzNotes: the title field is too small.
5. AzMail: the mail files onto the azul-storage Drive - ONE Drive at the data root with a per-account scope (a
   per-account LocalDrive would make a second .azlin/cache), account.rs / send.rs writes too (design in
   scripts/MAIL6_2026_10_03.md sec. 8). Once LAYOUT7's CSS `zoom` lands the reading pane uses it - leave a TODO
   naming LAYOUT7 if it is not in your base.
6. AzShow: the drop indicator in the slide rail, tables edited in place, the presenter window on a chosen monitor.
7. AzSheets: Replace inside the grid's own edit; colour / font / border pickers beyond the dialog's presets; Format
   Cells OK as ONE undo step (not one per property). Merges that shift with inserted rows: only if IronCalc's model
   allows it - else document why not.
8. LOOK at each of your apps on the prebuilt binaries (flat / flora x light / dark, capped, one at a time,
   wait_settled before each screenshot) and fix what is the app's; engine findings -> report.

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/OFFICE7.PROGRESS.md exact. Finish with the report
scripts/OFFICE7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.
