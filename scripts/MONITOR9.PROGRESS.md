# MONITOR9 progress - AzMonitor (wave 9)

Branch `wt/monitor9` from `e537ddbe2`. Brief: scripts/waves/wave9/PLAN.md "MONITOR9";
planning: ../azul-apps/planning/core/system-monitor.md. House rules: scripts/waves/house_rules.md.

## DONE
- b130fb395 progress file
- 97af982f6 RED: crate examples/azul-monitor (registered: root Cargo.toml, workspace_test_members.txt,
  rust.yml dll_tests) + history.rs / model.rs tests
- a04258f26 GREEN: History ring + Model (rates, rows, sort, filter, selection)

## IN PROGRESS
- sample machine (src/sample.rs) RED -> GREEN

## NEXT
1. src/sample.rs: deterministic sample machine (8 cores, 16 GB, ~40 processes per the planning doc,
   cargo spike), `Source` trait (`read() -> Snapshot`, `end(pid, force) -> Result<String,String>`)
2. src/live.rs: sysinfo Source (System, Disks, Networks, Users)
3. src/sampler.rs: the Thread loop (Arc<Shared>: interval, commands, stop), writeback per reading
4. src/ticks.rs (or in lib): TickPlan = which live views to re-render (RED: a tick never rebuilds the page)
5. UI: ids.rs, lib.rs (start, layout, RecordsShell, ShellThemeScope Slate), ui_processes.rs (cards VV +
   table VV with DataTable), ui_performance.rs (VV: charts, per-core bars TODO(WIDGETS9B) Gauge),
   end-process MessageBox in Modal, settings section (update speed), history CSV export via Drive
6. scripts/azmonitor_e2e.py (--sample; AZMON_* stdout lines; layout count constant across ticks)
7. report scripts/MONITOR9_2026_10_03.md

## Decisions
- sysinfo 0.38 (not 0.39: needs rustc 1.95; toolchain is 1.91). 0.38.4 reuses the locked
  objc2-core-foundation 0.3.2 / objc2-io-kit 0.3.2 / windows 0.62.2; only ntapi 0.4 is new (Windows).
  Read from a crates.io download in /tmp/monitor9 (never compiled here).
- The 1 Hz tick: the sampler Thread's writeback updates the Model and returns Update::DoNothing; it
  re-renders only the live VirtualViews (cards strip, table, performance page) with
  `trigger_virtual_view_rerender` (found by marker) and the marked status-bar segments with
  `StatusBar::update_segment_label`. The first reading is the one RefreshDom (empty state -> table).
  User actions (sort click, filter, selection) return RefreshDom (the proven path).
- VirtualView content does not inherit from the host: each live view wraps its content in its own
  `ShellThemeScope` (accent Slate - the system monitor's family per ShellThemeAccent's doc).
- The app owns the sort and filter (the model), not the DataTable: rows change every second, the
  widget's computed `order` would be stale. On a Sort event the app takes `view.sort` as its keys and
  hands the view back "in app order" (order empty, ordered false, order_serial = query_serial) - the
  header still shows the arrows. The widget's filter row is off; the tool row has the filter field.
  The selection is the PID (model), re-mapped to its row each tick.
- CPU per process = share of the whole machine (Task Manager's rule), not % of one core.
- Bytes and rates through azul's one formatter `DiskSpace::format_bytes` (+ "/s"); network in bytes/s
  too (one formatter, not a bits/s twin).
- `--sample` = the deterministic sample machine (planning doc s1/s6), for screenshots and the E2E;
  "End process" there removes it from the sample (nothing is killed).

## Open questions
- (none)
