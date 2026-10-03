# MONITOR9 progress - AzMonitor (wave 9)

Branch `wt/monitor9` from `e537ddbe2`. Brief: scripts/waves/wave9/PLAN.md "MONITOR9";
planning: ../azul-apps/planning/core/system-monitor.md. House rules: scripts/waves/house_rules.md.

## DONE
- (none yet)

## IN PROGRESS
- reading: AzDashboard (RecordsShell + DataTable + Chart model), appkit, sysinfo 0.38.4 API

## NEXT
- skeleton crate examples/azul-monitor (Cargo.toml, lib.rs, main.rs, ids.rs), registration
- RED: model tests (ring buffer, sampling deltas, sorting, filter)

## Decisions
- sysinfo 0.38 (not 0.39: needs rustc 1.95; toolchain is 1.91). 0.38.4 reuses the locked
  objc2-core-foundation 0.3.2 / objc2-io-kit 0.3.2 / windows 0.62.2; only ntapi 0.4 is new (Windows).
  Read from a crates.io download in /tmp/monitor9 (never compiled here).

## Open questions
- (none)
