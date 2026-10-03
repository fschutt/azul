# EVENTS7 - event plumbing and input (wave 7)
Owns: core/src/events.rs, dll/src/desktop/shell2/common/event.rs, the macOS menu / key code under
dll/src/desktop/shell2/macos, layout/src/e2e/*. Read first: scripts/WRITER6_2026_10_03.md (the shortcut / default
action design), scripts/MEETDRIVE6_2026_10_03.md sec. 7, scripts/HEADLESS6_2026_10_03.md.

1. An event aimed into a VirtualView child DOM never bubbles out to the parent DOM (MEETDRIVE6: a double-click on
   a drive tile's capacity bar - a ProgressBar = VirtualView - does not open the drive; event.rs ~9370). The DOM
   event path must continue from the VirtualView's host node in the parent DOM, like the shadow-DOM retargeting.
2. macOS native: the Edit menu's Undo item applies Cmd+Z before any key handler runs, so the RichTextEditor loses
   Cmd+Z to the engine's text undo. Route the menu's Undo / Redo (and Cut / Copy / Paste / Select All) through the
   same default-action path as the keys (WRITER6: the app's handlers first, `prevent_default` vetoes).
3. The in-crate scenario runner (layout/src/e2e/runner.rs) has no arm for DefaultAction::UndoTextEdit /
   RedoTextEdit (it never applied SystemChange::UndoTextEdit either): add them, test with a scenario.
4. Headless menus (child windows `azul-menu`, `azul-menu-2`) do not close on a click outside them or on Escape.
5. Ctrl+B with no selection is not reported to the app (DEDUP_EDITORS: formats missing from the text-edit report;
   WRITER6 made typed text take the engine's formats - check what is left and close it).

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/EVENTS7.PROGRESS.md exact. Finish with the report
scripts/EVENTS7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.
