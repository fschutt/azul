# MONITOR9 progress - AzMonitor (wave 9)

Branch `wt/monitor9` from `e537ddbe2`. Brief: scripts/waves/wave9/PLAN.md "MONITOR9";
planning: ../azul-apps/planning/core/system-monitor.md. House rules: scripts/waves/house_rules.md.

## DONE
- b130fb395 progress file
- 97af982f6 RED: crate examples/azul-monitor (registered: root Cargo.toml, workspace_test_members.txt,
  rust.yml dll_tests) + history.rs / model.rs tests
- a04258f26 GREEN: History ring + Model (rates, rows, sort, filter, selection)
- 455de3477 / a05424d58 sample machine RED / GREEN (Source trait in model.rs)
- ccf6a294b / bbf0597a4 live machine (sysinfo) RED / GREEN
- 1702c6656 / 0244d89f0 sampler (Thread, Shared, Command, due, run_commands) + ticks (TickPlan) RED / GREEN
- c3be3af98 / 8e3a554de history CSV RED / GREEN + ids.rs
- f58f6a66c table.rs RED; 5e81882b4 user RED; 2f0555dc7 user + table GREEN

- bf65398dc lib.rs written (RED for helpers speed_from_setting / speed_index / speed_text / sort_text: todo!())

- 7d9ebb8ee ui.rs written (RED chart helpers); 62b793938 GREEN lib/ui helpers - NO todo!() left

## IN PROGRESS
- scripts/azmonitor_e2e.py (model: scripts/azdashboard_e2e.py + scripts/azlin_e2e.py helper)
- then report scripts/MONITOR9_2026_10_03.md

## (done) the ui.rs contract, kept for reference:
  `ui::tools(&Monitor, &RefAny) -> Dom` (TabHeader Processes/Performance, filter TextInput with
  `.with_text(model.filter())`, "End process" Button disabled without a selected_row),
  `ui::waiting(&Monitor) -> Dom` (ShellEmptyState, id ids::WAITING),
  `ui::live_view(&RefAny, LiveView) -> Dom` (Dom::create_virtual_view(RefAny::new(Live{app,view}),
  render_live) + with_marker(OptionString::Some(marker)) + with_id + css: Cards height 132px
  flex-shrink 0 width 100%; Table/Performance flex-grow 1 min-height 0 width 100%),
  `ui::marker_of(LiveView) -> AzString` (ids::CARDS / TABLE_VIEW / PERFORMANCE),
  `ui::status_bar(&Monitor) -> Dom` (StatusBarSegment with_marker for STATUS_PROCESSES / CPU /
  MEMORY / NOTICE / SPEED), `ui::status_labels(&Monitor) -> Vec<(AzString, String)>`,
  `ui::confirm_dom(&Confirm, &RefAny) -> Dom` (MessageBox Question in Modal, buttons
  End process / Kill / Cancel, default 2), `ui::ask_to_end(&mut Monitor) -> bool` (opens the
  question for selected_row, prints AZMON_ASK), `ui::settings_sections(&Monitor, &RefAny) ->
  Vec<kit::AppSection>` (category 0 "Monitor": Segmented SPEEDS -> lib::set_speed; Export button ->
  kit::spawn_file_jobs FileJob::Put key kit.key("history/<chrono stamp>.csv") bytes
  model.history_csv(s.seconds_per_reading()), on done AZMON_EXPORTED + s.notice).
  render_live: downcast Live, size = info.bounds.get_logical_size(); content wrapped in a div of
  explicit px size + ShellThemeScope::create(..).with_accent(Slate).dom() (VV content inherits
  nothing); VirtualViewReturn::with_dom(dom, rect, rect). Table: clone view+rows from Monitor, DROP
  the guard, then table::table(app, view, rows, w, h).dom(). Cards: 4 cards (CPU, Memory, Disk,
  Network) headline + Chart Area sparkline (no legend/grid). Performance: CPU chart + per-core
  ProgressBars (TODO(WIDGETS9B) Gauge) + memory/disk/network charts + stats line.
  Chart x = -(seconds ago); a helper `chart_points(&[f64], secs) -> Vec<ChartPoint>` (test it).

## NEXT
1. write ui.rs (above), commit; then GREEN the four lib.rs helpers (tests in lib.rs tests mod)
2. scripts/azmonitor_e2e.py (--sample; AZMON_* stdout lines; layout count constant across ticks)
3. report scripts/MONITOR9_2026_10_03.md (api.json: none expected - app only; sysinfo 0.38 new crate)

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
