Task HYGIENE (azul Rust GUI toolkit, PR #476, wave 5). Read the house rules first and follow them exactly (never compile):
scripts/waves/wave5/house_rules.md
Your branch: `wt/hygiene` from base commit `2e92c759b` (`git -C <your worktree> checkout -b wt/hygiene 2e92c759b`). TASK name: `HYGIENE`.
Other wave-5 agents work in parallel on: MAILHTML (mail HTML/CSS parity, AzMail's html.rs), TABLES (table layout),
TEXTENG (line-height, inline-blocks in spans, text-edit formats), APIEXPORT (api exports + ctrl||meta sites),
HYGIENE (macro paths, autofix, helper twins), RTE (shared rich-text editor; AzNotes/AzMail compose), BLOCKS
(selection model, undo stack, switcher merge, preset shells), PIM (azul-pim crate; Calendar/Tasks/Contacts).
Stay in your area; where you must touch another's file keep the edit minimal and list it in your report.

GOAL: the mechanical duplication / hygiene items (DEDUP_WIDGETS_API):
1. `impl_option!` / `impl_result!` call `impl_option_inner!` / `impl_result_inner!` without `$crate::`
   (css/src/macros.rs ~1101,1124,1147 / ~1211,1229,1248): qualify them, qualify `RefAny` in `impl_widget_callback!`,
   then delete the hand imports of `impl_option_inner` (77 files) and `impl_result_inner` (9 files) - EXCEPT
   `layout/src/solver3/page_breaks.rs` (another session's file: leave its import).
2. The autofix bug: `doc/src/autofix/module_map.rs` writes the "is a Vec type" rule three times and only one copy
   includes `vecslice`, so 14 widget `*VecSlice` types sit in wrong api.json modules (CellGridRangeVecSlice in css).
   One rule (one function), a RED unit test in doc/src/autofix, and list the 14 types: the parent re-runs autofix
   (never edit api.json).
3. Theme helpers: `layout/src/widgets/themes/decl.rs` and `style_kit.rs` are parallel files where `fill` means a
   background in one and `width/height: 100%` in the other: one module, one name per meaning, every caller moved.
   `timeline.rs` and `cell_grid.rs` own private single-property helpers (display_flex, px_width, px_bottom, ...):
   use the shared ones. Nine widget files carry a private callback-builder `hook()`: one shared helper.
4. Core: eight HTML escapers and two character-reference decoders - one of each (find them: grep escape / &amp;),
   every caller moved, RED tests for the escaping rules first.
5. micromail 0.1 (layout's crash-mail feature) and 0.2 (AzMail) are both in Cargo.lock: move layout to 0.2 (its
   API changed: read ~/Development/micromail's CHANGELOG / src; keep crash-mail working, RED test if one exists).
These touch many files other agents also touch (imports): keep each edit to the exact lines, commit per item.

Report `scripts/HYGIENE_2026_10_02.md` per the house rules (what was built, commits, api.json list, least-sure-to-compile
spots, the parent's test commands, what is left for wave 6). You are running unattended: decide, note decisions in
your progress file, continue; do not stop to ask. Do not spawn subagents.
