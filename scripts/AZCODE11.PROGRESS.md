# AZCODE11 progress (AzCode: VS Code's workbench behaves)

Worktree: .claude/worktrees/agent-a352efd78f2c2b04e (from fix/input-bugs-2026-09-19 @ 86c0e821e).

## Causes found
- "Selecting a folder does nothing" (macOS): tfd 0.1.2 `select_folder_dialog` runs `choose folder`,
  then a SECOND osascript `POSIX path of alias Macintosh HD:Users:...:` - unquoted, an AppleScript
  syntax error (-2740) -> `None` -> FileDialog::open_directory resolves as cancelled -> AzCode's
  on_folder_picked returns DoNothing. Reproduced with osacompile (no dialog).
- Missing dirty dot: the class sat on `Dom::create_icon("circle")`; icon resolution
  (core/src/icon.rs resolve_icons_in_dom_inner, `*dom = replacement`) replaces the whole node, its
  ids and classes are dropped (engine bug, reported, not fixed here). The dot is now a plain div.

## DONE
(none yet)

## IN PROGRESS
- RED: layout/src/desktop/dialogs.rs the_macos_folder_picker_asks_for_the_posix_path_in_the_same_script

## NEXT
- GREEN dialog fix; dirty dot; args (--folder, --shell); markers; branch; Mod+K Mod+O;
  virtualized explorer; termkit crate + terminal panel; folder search; command palette + menu;
  E2E.
