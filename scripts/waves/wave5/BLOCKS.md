Task BLOCKS (azul Rust GUI toolkit, PR #476, wave 5). Read the house rules first and follow them exactly (never compile):
scripts/waves/wave5/house_rules.md
Your branch: `wt/blocks` from base commit `2e92c759b` (`git -C <your worktree> checkout -b wt/blocks 2e92c759b`). TASK name: `BLOCKS`.
Other wave-5 agents work in parallel on: MAILHTML (mail HTML/CSS parity, AzMail's html.rs), TABLES (table layout),
TEXTENG (line-height, inline-blocks in spans, text-edit formats), APIEXPORT (api exports + ctrl||meta sites),
HYGIENE (macro paths, autofix, helper twins), RTE (shared rich-text editor; AzNotes/AzMail compose), BLOCKS
(selection model, undo stack, switcher merge, preset shells), PIM (azul-pim crate; Calendar/Tasks/Contacts).
Stay in your area; where you must touch another's file keep the edit minimal and list it in your report.

GOAL: the shared building blocks the apps re-implement (DEDUP_OFFICE / DEDUP_WIDGETS_API):
1. One selection model (click / Ctrl-or-Cmd+click / Shift+click ranges, select all, keyboard extend) - AzDrive,
   AzTasks and AzShow (twice) each write their own, MessageList has a helper: a shared one (core or a widget
   module), RED tests, MessageList and the apps adopt it.
2. One undo/redo snapshot stack (AzPhoto, AzVideoCut, AzShow have three diverged ones): shared, tested, adopted.
3. ModuleSwitcher vs ShellNavigationPane's switcher: the same Outlook module buttons, disagreeing on wrapping at
   the ends; only the pane has badges; seven apps use the pane, only the showcase ModuleSwitcher: ONE switcher
   (the pane's, with the wrap rule fixed), ModuleSwitcher becomes it or goes (list the api.json change).
4. The preset shells repeat ~98 setters that OfficeShell already has: delegate instead of copy.
5. DEDUP_OFFICE N14: no app asks "save changes?" before closing a window with unsaved work - a shared close guard
   (the window close request + a standard MessageBox from dialog_kit) apps can opt into, RED test; adoption in the
   apps is wave 6 (list the call sites).
Do not rename MessageList (that rename waits for the user).

Report `scripts/BLOCKS_2026_10_02.md` per the house rules (what was built, commits, api.json list, least-sure-to-compile
spots, the parent's test commands, what is left for wave 6). You are running unattended: decide, note decisions in
your progress file, continue; do not stop to ask. Do not spawn subagents.
