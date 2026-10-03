# PIMDRIVE7 progress (wave 7, branch wt/pimdrive7 from 2e55eef06)

Brief: scripts/waves/wave7/PIMDRIVE7.md. Owns examples/azul-{calendar,tasks,contacts,drive,meet}, examples/azul-pim.
Scratch helpers (not committed): /tmp/pimdrive7/rep.py (exact replacements from a JSON list),
/tmp/pimdrive7/pysel.py (E2E "#x" -> FN("x")), /tmp/pimdrive7/parse.sh (rustfmt parse check).

## DONE
- b4ea5d988 progress file
- 3d46aa748 AzCalendar src/ids.rs (`__azcal_` + old name, names! macro like AzCalculator's), all UI files use it
- c0f72572e azcalendar_e2e.py / week_interactions.py / offline_links.py: `wi.sel(stem)`, `detect_naming`
- d53b050f0 mint-and-join.mjs: `cal(stem)`, `detectNaming`
- 46f77dc56 AzTasks src/ids.rs wired (is_task_row, focus_id takes AzString); d641667ff aztasks_e2e.py sel/detect
- 24591336b AzContacts src/ids.rs wired (indexed! macro for form rows; ui::section_id -> ids::section)
- b93148da3 azlin_e2e.App detect_naming / name / sel; azcontacts_e2e.py on it

## IN PROGRESS
- 2. AzTasks blank-on-click in "All": reproduce on the prebuilt binary (capped, headless), root-cause.

## NEXT
- 3. AzTasks planned / board view + tags as TokenInput.
- 4. AzCalendar start through the Drive.
- 5. AzContacts LOOK + fixes.
- 6. AzDrive Details on a widget; AzMeet chat on rejoin; azdrive / azmeet E2E onto azlin_e2e.py.
- 7. LOOK at all five apps.

## Decisions
- D1 Prefix naming: `__az<app>_` + the old kebab name unchanged (AzCalculator / AzSheets style), so a script's
  selector is the prefix + the old stem; scripts detect an older build's bare names (to run on the prebuilt).
- D2 The OfficeShell's own ids (`shell-backstage`, `shell-ribbon`, ...) are the widget's, not prefixed; the
  ShellPane ids the APP chooses are (`__azcal_shell-navigation`).
- D3 AzCalendar's DOM root id `azcalendar` -> `__azcal_app`; window ids (`azcalendar`, `azcalendar-editor`)
  are window names, unchanged.
- D4 AzTasks `is_task_row` excludes `task-list` / `task-pane` (task ids are UUIDs).

## Open questions
