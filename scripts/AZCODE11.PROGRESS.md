# AZCODE11 progress (AzCode: VS Code's workbench behaves)

Worktree: .claude/worktrees/agent-a352efd78f2c2b04e (from fix/input-bugs-2026-09-19 @ 86c0e821e).

## Causes found
- "Selecting a folder does nothing" (macOS): tfd 0.1.2 `select_folder_dialog` runs `choose folder`,
  then a SECOND osascript `POSIX path of alias Macintosh HD:Users:...:` - unquoted, an AppleScript
  syntax error (-2740) -> `None` -> FileDialog::open_directory resolves as cancelled -> AzCode's
  on_folder_picked returned DoNothing. Reproduced with osacompile (no dialog).
- Missing dirty dot: the class sat on `Dom::create_icon("circle")`; icon resolution
  (core/src/icon.rs resolve_icons_in_dom_inner, `*dom = replacement`) replaces the whole node, its
  ids and classes are dropped (engine bug, reported, not fixed here). The dot is now a plain div.
  (Before CODE10 the tab said "main.rs *" as text and the E2E passed.)

## DONE
- 67caaa0bb RED dialogs: the_macos_folder_picker_asks_for_the_posix_path_in_the_same_script (+ _is_valid_applescript, macOS)
- 1d125ee17 GREEN dialogs: open_directory on macOS runs our own one-script osascript
- 3f55086d9 termkit: session.rs + vt.rs git-mv'd from AzTerm to examples/azul-termkit (+ pane.rs glue); AzTerm re-exports
- bed366a44 azcode: dirty dot is a div
- d0d8ce720 termkit: pane::setup_env
- 4fa8aae8c azcode: workbench (args, markers, branch, virtualized explorer + search, terminal panel, palette, menu)
- (next) E2E workbench run

## NEXT
- independent compile review of the new code (no cargo); fix what it finds.
- report.
