# MONITOR9 progress - AzMonitor (wave 9)

Branch `wt/monitor9` from `e537ddbe2`. Brief: scripts/waves/wave9/PLAN.md "MONITOR9";
planning: ../azul-apps/planning/core/system-monitor.md. House rules: scripts/waves/house_rules.md.

## STATE: DONE (report scripts/MONITOR9_2026_10_03.md)

## DONE
- b130fb395 progress file
- 97af982f6 / a04258f26 model + history RED / GREEN (crate registered: root Cargo.toml,
  workspace_test_members.txt, rust.yml dll_tests)
- 455de3477 / a05424d58 sample machine RED / GREEN (Source trait in model.rs)
- ccf6a294b / bbf0597a4 live machine (sysinfo 0.38) RED / GREEN
- 1702c6656 / 0244d89f0 sampler (Thread, Shared, Command, due, run_commands) + ticks (TickPlan)
- c3be3af98 / 8e3a554de history CSV + ids.rs
- f58f6a66c / 5e81882b4 / 2f0555dc7 table glue + "who runs the monitor"
- bf65398dc / 7d9ebb8ee / 62b793938 lib.rs + ui.rs (window, live views, question, settings) + helpers
- 9afcda4a0 scripts/azmonitor_e2e.py
- report scripts/MONITOR9_2026_10_03.md

## NEXT (if resumed)
- Nothing required. After the parent's build: run `cargo test -p AzMonitor --lib` and the E2E; fix
  what the compiler / the E2E finds (the least-sure spots are listed in the report, section 6).
- Optional "Left" items in the report, section 8.

## Decisions
- sysinfo 0.38 (not 0.39: needs rustc 1.95; toolchain is 1.91). 0.38.4 reuses the locked
  objc2-core-foundation 0.3.2 / objc2-io-kit 0.3.2 / windows 0.62.2; only ntapi 0.4 is new (Windows).
- The 1 Hz tick: the sampler Thread's writeback updates the Model and returns Update::DoNothing; it
  re-renders only the live VirtualViews (cards strip, table, performance page) by marker and the
  marked status-bar segments. The first reading is the one RefreshDom. Sort / select / tab /
  question return RefreshDom; scroll / filter keystrokes re-render the table view only.
- VirtualView content inherits nothing: each live view wraps its content in its own ShellThemeScope
  (accent Slate).
- The app owns sort and filter (rows change every second); the view goes back "in app order";
  the DataTable's filter row is off; the selection is a PID re-mapped each reading.
- CPU per process = share of the whole machine. Bytes / rates through DiskSpace::format_bytes.
- `--sample` = the deterministic sample machine; ending there removes the process (root refused).
- Delete ends the selected process only when the focus is not in the page's own DOM (DomId 0: the
  filter field keeps its Delete).

## Open questions
- (none)
