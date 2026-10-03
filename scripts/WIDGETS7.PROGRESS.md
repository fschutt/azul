# WIDGETS7 progress (wave 7, 2026-10-03)

Branch `wt/widgets7` from `2e55eef06` (worktree .claude/worktrees/agent-a82f849dedcbbbe5d).
Brief: scripts/waves/wave7/WIDGETS7.md. Rules: scripts/waves/house_rules.md.
Scratch: target/widgets7 in the worktree (look.py = headless look driver, screenshots - not committed).
Commit messages: written to target/widgets7/msg, `git -C <wt> commit -F target/widgets7/msg`.
Look run (one at a time, capped): run_capped.sh --cap-mb 1500 --seconds 90 --log <wt>/target/widgets7/runN.log --
  python3 <wt>/target/widgets7/look.py <wt>/target/widgets7/<prefix> <port> <steps.json> -- <binary> <args>

## DONE
- 645fbec30 progress file
- item 1 CloseGuard: c62a6ae72 RED (dirty_check API stub + 3 tests), b14047275 GREEN (on_close_requested
  asks the check; guide windowing.md)
- item 2 placeholder ink: LOOK prebuilt AzCalendar --screen backstage-calendars --mode light: "Name" prompt
  #4c4c4cff in the display list. Root cause = cascade (prop_cache get_property_slow placeholder tier
  admitted theme/mode-only declarations). cd26cf886 RED (core prop_cache_test + text_input
  placeholder_ink_tests), 1a78b80f4 GREEN (prop_cache.rs closure; flat.rs FIELD_PLACEHOLDER_DARK doc)

- item 3: the garbled first-row TextInput / DropDown caret NOT reproduced on the wave-6 build (flat + flora,
  light + dark, started on the page and navigated FILE > Calendars, typed into the field, opened the
  drop-down: all rows clean). Found instead: flat backstage page WHITE in dark mode (white ink on white).
  26a6a4711 RED (the_flat_backstage_page_is_dark_in_the_dark_mode), 3192837e3 GREEN (page_bg pair).
  For PAINT7: the headless screenshot drops the selected nav label "Calendars" in DARK mode (flat and
  flora) though the display list has it (#ffffff / #f4f2ea text at 24,225 over the item) - cpurender.

- item 4: LOOK prebuilt AzCalendar week: navigator + To-Do bar calendars lose their last column.
  d416c5d78 RED (layout/tests/an_inline_date_picker_fits_its_pane.rs + all.rs pair), fae2d1084 GREEN
  (CONTAINER_STYLE box-sizing border-box + max-width 100%).
- item 5: 525f635a3 RED (ToDoBar.week_start field + setters stub, test), 85fe5221f GREEN (passed on).
  AzCalendar (PIMDRIVE7's) should call ToDoBar.with_week_start(Monday) - note in the report.
- Seen for others: AzCalendar week view (light, --sample): three 1-hour event blocks show NO title
  (Thu 16:00, Tue 18:00, Fri 14:00 - only colour), "Lunch with Ana"'s time line clipped (known, PAINT7).

## IN PROGRESS
- item 6: flora-dark zoom slider; check box colour

## NEXT
6. flora-dark zoom slider; check box colour transparent in the HTML dump
7. DEDUP F2 ModuleSwitcher vs ShellNavigationPane; F20 MessageList -> SummaryList (only if mechanical)

## Decisions
- D1 CloseGuard: a callback (CloseGuardDirtyCheckCallbackType = fn(RefAny, CallbackInfo) ->
  CloseGuardDocumentState {Saved, Unsaved}) - a repr(C) enum, not bool (bool has no HostOut impl and a
  core edit for it is out of my area); a host that does not answer = Saved (never traps the window).
- D2 placeholder: fixed in the cascade (pseudo-element semantics only for Placeholder; other tiers keep the
  loose match the widgets' "states appended last" convention relies on).

## Open questions
