# Wave 7 - every open bug from wave 6, plus the "recommendable" widgets (2026-10-03)

Source of truth: the ledger's "WAVE 7" section (user 2026-10-03: "update the ledger for wave 7 - all bugs"),
which harvests the wave-6 reports (`scripts/<TASK6>_2026_10_03.md`, "Seen broken" / "Left") and the pdfocr
agent's text-indent report. Base: the wave-6 integration that COMPILES (the commit your prompt names); the
prebuilt binaries in target/release are that build.

Ten tasks, each owning disjoint files. Shared files are append-only (layout/tests/all.rs, layout/src/widgets/mod.rs
lists, the theme files flat.rs / flora.rs under a `// ==== <widget> ====` banner); the parent merges those.

| Task | Owns | Items (ledger WAVE 7) |
|---|---|---|
| TEXT7 | layout/src/text3/*, layout/src/font* (font loading / metrics), the one line dll/src/desktop/shell2/common/layout.rs:1555 | B: text-indent narrows the first line (pdfocr's 3 RED tests), a run shaped before its font loads, bolder / lighter relative, the font-index off-by-one in dll layout.rs:1555, line-height 19px -> 19.55, line-height rem / vw / vh |
| LAYOUT7 | layout/src/solver3/* EXCEPT display_list.rs and page_breaks.rs; css/src/props for `zoom` | B: block inside an inline dropped, abspos child treated as in-flow (+ ::marker with list-style none), inline-block min-width:100% = 524, fit-content = 100%, display:table drops a child's margins, CSS `zoom`, AzMail wizard page 2 (RED cc7040ae5), inline-block in an inline span sized from max-content, restore a_narrow_table_wraps_its_cells_to_fit, a block taller than a page (pagination, NOT page_breaks.rs) |
| PAINT7 | layout/src/solver3/display_list.rs, layout/src/cpurender/*, the hit tester, the incremental-relayout / display-list cache paths in layout/src/window.rs, dll compositor / X11 image paths | C: paint order (positioned vs earlier transformed sibling), hit test ignores animation transforms, X11 CSS-id image, cpurender sheet under a scroll layer + backstage leftovers after a theme switch, stale / doubled geometry in headless shots, incremental relayout garbles (PIM6 display lists), an overflow:hidden span clipping glyphs at 5 px |
| EVENTS7 | core/src/events.rs, dll/src/desktop/shell2/common/event.rs, the macOS menu code, layout/src/e2e/* | D: an event into a VirtualView child never reaches the parent DOM, macOS Edit menu eats Cmd+Z before key handlers, the scenario runner's UndoTextEdit / RedoTextEdit arms, headless menus close on outside click / Escape; Ctrl+B with no selection reported to the app |
| WIDGETS7 | layout/src/widgets/* (existing widgets) + theme appends | E: CloseGuard asks the app at close time, TextInput placeholder ink, the backstage TextInput / DropDown caret, DatePicker wider than its pane, ToDoBar week start, flora-dark zoom slider, check box colour, ModuleSwitcher vs ShellNavigationPane merge (DEDUP), MessageList -> SummaryList naming (DEDUP) |
| OFFICE7 | examples/azul-{writer,notes,mail,sheets,show,review} | A + F: WRITER6 ids -> const AzString, AzReview MouseMove, AzWriter import off the UI thread + Update::max_self export (no twin), AzNotes title field, AzMail mail files on the Drive (one Drive, per-account scope), AzShow (rail drop indicator, tables in place, presenter monitor), AzSheets (Replace in the grid, pickers, Format Cells = one undo step) |
| PIMDRIVE7 | examples/azul-{calendar,tasks,contacts,drive,meet}, examples/azul-pim | A + F: the `__azcal_` / `__aztasks_` / `__azcontacts_` prefixes (~300 ids + 5 E2E scripts), AzTasks board / planned view + TokenInput tags + the blank-on-click bug, AzCalendar start through the Drive, AzContacts first look, AzDrive Details table on a widget, AzMeet chat on rejoin, azdrive / azmeet E2E onto azlin_e2e.py |
| DATATABLE7 | NEW layout/src/widgets/data_table.rs (+ theme appends, tests), NEW examples/azul-dashboard (shared with CHART7: DATATABLE7 creates it) | I: a virtualized DataTable - sort / filter / edit built in, 500k x 25 rows smooth, keyboard, a11y; the dashboard example's table half |
| CHART7 | NEW layout/src/widgets/chart.rs (+ theme appends, tests), the chart half of examples/azul-dashboard | I: a Chart widget - line / bar / scatter / pie, axes, legend, hover, both themes x modes; the dashboard's chart half + the tutorial doc |
| TOOLS7 | doc/src/autofix/*, doc/src/codegen/*, css/src/macros.rs (the macros only) | G: `autofix add` for a static fn whose body calls a free fn, the module choice for new types, AUTOFIX6's list (&T returns, Option<T> args, &[T] -> VecSlice, the suffix splice, ClassPatch::is_empty, remove + add in one round), impl_option! / impl_result! with `$crate::` (leave the 77 imports alone), module_map's "is a Vec" rule once (VecSlice) |

Parent (not agents): the PGO + order-file chain (scripts/pgo/chain.sh), BOLT for the Linux .so in CI, the
allocator-slack measurement, the suites / E2E / mail corpus, the verification list (ledger A).

Contracts between tasks:
- WIDGETS7 owns existing widgets; DATATABLE7 / CHART7 only add new files. A theme change goes in the theme files
  as an APPEND.
- LAYOUT7 owns layout; PAINT7 owns what is painted and hit. A bug that turns out to be the other's: write the RED
  test, commit it, note it in your report for the other task, do not fix it in their files.
- EVENTS7 owns the event plumbing; WIDGETS7 the widgets' handlers.
- OFFICE7 / PIMDRIVE7 never fix an engine bug in the app: RED test in layout/tests + a note for its owner.
- examples/azul-dashboard: DATATABLE7 creates the crate (Cargo.toml, registration, main.rs, lib.rs with a `table`
  module); CHART7 adds a `chart` module and the tutorial, and edits lib.rs only to add `mod chart;` and the
  chart's place in the layout (one marked block).
