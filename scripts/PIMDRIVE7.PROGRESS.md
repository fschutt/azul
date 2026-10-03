# PIMDRIVE7 progress (wave 7, branch wt/pimdrive7 from 2e55eef06)

Brief: scripts/waves/wave7/PIMDRIVE7.md. Owns examples/azul-{calendar,tasks,contacts,drive,meet}, examples/azul-pim.
Scratch helpers (not committed): /tmp/pimdrive7/rep.py (exact replacements from a JSON list),
/tmp/pimdrive7/pysel.py (E2E "#x" -> FN("x")), /tmp/pimdrive7/parse.sh (rustfmt parse check),
/tmp/pimdrive7/look_tasks*.py (LOOK drivers on azlin_e2e).

## DONE
- b4ea5d988 progress file
- 3d46aa748 AzCalendar src/ids.rs (`__azcal_` + old name, names! macro like AzCalculator's), all UI files use it
- c0f72572e azcalendar_e2e.py / week_interactions.py / offline_links.py: `wi.sel(stem)`, `detect_naming`
- d53b050f0 mint-and-join.mjs: `cal(stem)`, `detectNaming`
- 46f77dc56 AzTasks src/ids.rs wired (is_task_row, focus_id takes AzString); d641667ff aztasks_e2e.py sel/detect
- 24591336b AzContacts src/ids.rs wired (indexed! macro for form rows; ui::section_id -> ids::section)
- b93148da3 azlin_e2e.App detect_naming / name / sel; azcontacts_e2e.py on it
- 54aa054bd item 2: not reproduced on the wave-6 build (4 LOOK runs); E2E step "All: a click on a title
  selects it alone" (passes on the prebuilt); key taps release their modifiers (azlin_e2e, aztasks_e2e,
  azcalendar_e2e)

- 99d9054fd / c49643a68 azul-pim RED / GREEN: dates::month_grid, Task.started (+ JSON, spawn resets)
- 1a3ca545a / 80dc8d4e8 AzTasks RED / GREEN: views::planned_month, Column, board; Tasks::move_to_column,
  reschedule (reanchor moved to state.rs); vtodo IN-PROCESS
- c48355955 state planned_month / month / board, take_dropped (nav uses it); ids
- 6fa8710f0 layouts.rs: the planned month + the board, the header switch (list.rs hooks)
- b4b8b2bb1 coordinator note (WIDGETS7): AzCalendar editor on CloseGuard.with_dirty_check
  (CloseGuardDocumentState, CloseGuardDirtyCheckCallbackType), ToDoBar.with_week_start in AzCalendar
  (Monday) and AzTasks (setting) - needs WIDGETS7's api.json entries
- 1573ae018 aztasks_e2e.py on azlin_e2e (App.has / box / drag added there); stage planned_and_board

- 1573ae018 E2E run on the prebuilt: PASS (planned/board stage BLOCKED on the old build, as designed)
- 50775f491 / 41b2ce569 RED / GREEN tag suggestions (views::tag_suggestions, chips under the tag field)
- affe9544d AzCalendar Month days = azul_pim::dates::month_grid (month_grid_start gone)

- 4778a86fc / a05db19a7 item 4 RED / GREEN: AzCalendar start reads events / calendars / settings through
  the data folder's Drive, --sample writes through it; std::fs helpers gone; the meeting server line is
  AzCalendar's own (rooms.rs drops encode_settings / decode_settings / MAX_SETTINGS_BYTES)

## IN PROGRESS
- 5. AzContacts LOOK (prebuilt, capped): list, card + photo avatar, edit form + birthday picker, CSV import
  mapping table; fix the app's bugs (RED first). Driver: /tmp/pimdrive7/look_contacts.py on azlin_e2e.

## NEXT
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
- D5 Item 2 not reproduced. LOOK notes for others: AzTasks settings page - the Default list DropDown is a black
  box and a strip covers the "Reminders" category (WIDGETS7: backstage DropDown caret); after the E2E's runtime
  flora + dark switch the nav's search field is not painted (PAINT7, incremental relayout); the To-Do bar's
  DatePicker cuts its Saturday column (WIDGETS7).

## Open questions
