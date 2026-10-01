# MAILWIDGETS progress

Branch `wt/mail-widgets` from `wt/fb2-azdrive-explorer` (`82f1d74c9`). Report:
`scripts/MAILWIDGETS_2026_09_30.md`. Task: the reusable widgets for the Outlook-2010-style AzMail
rewrite (MessageList, ReadingPane + InfoBar, ToDoBar, ModuleSwitcher, StatusBar sync, WizardLayout),
flat + flora, light + dark, keyboard, a11y, tests, showcase cards; engine gaps found by reading fixed
RED-first (date_picker inline mode + today, list_view lazy-load hook never wired).

## DONE
- `822b65c94` progress checkpoint.
- `028879ee7` RED A: date_picker inline + today tests, list_view lazy-load wiring test, statusbar
  sync tests, info_bar (stub dom); manifest entries "info_bar", "date_picker (inline)",
  "statusbar (sync)".
- `c06c8bdca` GREEN A: date_picker `inline` / `today` (+ `DatePickerLook::day_today`,
  `CellFaces::today`, `DatePickerData::inline`), list_view `scroll_settled_hook` /
  `scroll_window_of` + `on_lazy_load_scroll` wired, statusbar `sync_dom`, info_bar build + flat /
  flora looks (`// ==== info_bar ====` appended in both theme files; flora `statusbar_style` and the
  two `date_picker` theme fns edited in place - 4 lines each).
- `4f643014e` RED C: message_list, reading_pane, todo_bar, module_switcher, wizard_layout (full
  API + bases + `build` + tests, `dom()` stubbed), `OptionInfoBar`, manifest entries
  "message_list", "reading_pane", "todo_bar", "module_switcher", "module_switcher (collapsed)",
  "wizard_layout" in their contrast groups.
- `d8525a81b` GREEN D: real `dom()` in the five files; `// ==== <widget> ====` looks appended to
  flat.rs / flora.rs (+ shared strokes `flat_strip_below/above`, `flat_sheet`,
  `flora_strip_below/above`, `flora_leaf`, `flora_label`); `decl::border_top`.
- (next hash) showcase "Mail" section (`examples/azul-widgets/src/mail.rs`, wired in lib.rs) and
  the report `scripts/MAILWIDGETS_2026_09_30.md`.

## IN PROGRESS
- nothing: the task is complete; the parent compiles, runs autofix for the api.json list in the
  report (section 6), runs the test commands (section 8) and builds the showcase.

## NEXT
- (if resumed) anything the parent's compile reports: the least-sure spots are in the report,
  section 7.

## Open questions
- No sibling worktree has `layout/src/widgets/shells/` or a `NavigationPane`: the ModuleSwitcher is
  built standalone here (said so in the report).
- The wheel-ownership lint (`widgets/mod.rs::wheel_ownership`) forbids `Scroll` hooks on manifest
  widgets; the virtualised list reports its window on `ScrollEnd` (a settled gesture) instead.
