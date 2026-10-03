Task APIEXPORT (azul Rust GUI toolkit, PR #476, wave 5). Read the house rules first and follow them exactly (never compile):
scripts/waves/wave5/house_rules.md
Your branch: `wt/apiexport` from base commit `2e92c759b` (`git -C <your worktree> checkout -b wt/apiexport 2e92c759b`). TASK name: `APIEXPORT`.
Other wave-5 agents work in parallel on: MAILHTML (mail HTML/CSS parity, AzMail's html.rs), TABLES (table layout),
TEXTENG (line-height, inline-blocks in spans, text-edit formats), APIEXPORT (api exports + ctrl||meta sites),
HYGIENE (macro paths, autofix, helper twins), RTE (shared rich-text editor; AzNotes/AzMail compose), BLOCKS
(selection model, undo stack, switcher merge, preset shells), PIM (azul-pim crate; Calendar/Tasks/Contacts).
Stay in your area; where you must touch another's file keep the edit minimal and list it in your report.

GOAL: the API gaps the DEDUP reviews found - each one exported (you write the Rust method/type in its crate,
docs, tests; list the api.json entry in your report for the parent's autofix) and its app workarounds removed:
1. `KeyboardState::primary_down` (core) is not exported, so 21 app files and 13 widget sites test `ctrl || meta`
   (wrong on macOS: Ctrl+S saves too; Win acts as Ctrl elsewhere). Export it (and its CallbackInfo convenience if
   one fits), replace every `ctrl || meta` shortcut test in layout/src/widgets and examples/azul-* (grep), RED
   tests for a widget shortcut on both conventions.
2. `ShellThemeAccent::colors` + `ShellThemeAccentColors` (AzShow's themes.rs copies all 20 flora colours): export,
   delete AzShow's copy.
3. Constructors: `ButtonOnClick` (Sheets/Show/Writer build the struct by hand), `StatusBarZoom` (+ setters), Ribbon
   `with_items`. The status-bar zoom slider is fixed at 10-190 % while Show/Sheets buttons go to 400 % (a drag snaps
   back - DEDUP_OFFICE D28): give the zoom a configurable range, RED first.
4. `ColorU::to_hex` / `parse_hex` - and `ColorU::from_str` returns black on bad input (apps cannot tell black from
   invalid): a parse that reports failure. `RawImage::create_rgba8` / `resized`. One `format_bytes` (4 copies:
   find them) exported and used. `Button` disabled and toggled states (with both themes). `NodeData` attribute
   getter (AzMail keeps link hrefs in a CSS class - move them to the attribute). A text-node accessor that replaces
   the unsafe `box_str` helpers in AzWriter/AzNotes/AzMail. `TextAreaState.get_text`. `DatePicker.with_week_start`
   (AzCalendar's weeks start Monday, the picker's Sunday - its week rows split). `GlobalHotkey.matches`.
5. `reborrow_info` copies in AzWriter/AzNotes/AzSheets: delete them, `CallbackInfo` is Copy (`*info`).
Read DEDUP_WIDGETS_API, DEDUP_OFFICE "api.json exports" and DEDUP_EDITORS "api.json" sections for the exact sites.
RTE owns the editors' internals (keep your AzNotes/AzWriter/AzMail edits to the accessor swap); PIM owns
Calendar's model files (your DatePicker change is the widget; the Calendar call site is a one-line edit).

Report `scripts/APIEXPORT_2026_10_02.md` per the house rules (what was built, commits, api.json list, least-sure-to-compile
spots, the parent's test commands, what is left for wave 6). You are running unattended: decide, note decisions in
your progress file, continue; do not stop to ask. Do not spawn subagents.
