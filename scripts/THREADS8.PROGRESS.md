# THREADS8 progress (wave 8) - widget threads that never stop

Branch: wt/threads8 (base 45c6bf98b)

## Finding (read first)
The backlog item ("threads of widgets removed from the DOM are never terminated") was already fixed on
2026-10-01 by 7c1c12389 (layout/src/managers/thread_owner.rs: a thread a node's AfterMount / NodeResized /
Updated callback added belongs to the node; LayoutWindow::remap_node_ids orphans it and sends TerminateThread;
run_all_threads retires it once finished or detaches it after ORPHAN_GRACE; headless RED/GREEN test
`the_worker_of_a_node_that_unmounts_is_told_to_stop_and_is_gone_after_a_frame`). The ledger was stale.
THREADS8 closes the gaps left around it.

## DONE
- a2346a221 progress file
- ff44c1340 RED / 0e68e6d7e GREEN video: test-pattern + replay workers poll capture_common::terminate_requested
- 339d58966 RED thread_owner.rs tests::a_window_that_closes (workers stop together; one grace, not N)
- a742a0149 thread_owner::stop_all; ce9e4735f `impl Drop for LayoutWindow` -> stop_all;
  8aa774c5b headless shutdown_threads -> stop_all
- 5c7e4e4fa RED timers: thread_owner unit tests (4) + headless
  `the_timer_a_node_started_on_mount_stops_when_the_node_unmounts`
- 9e3b63583 manager: timer_owners + stop list, shared `follow` rule
- bec416fdb window.rs remap_node_ids drops stopped timers from lw.timers
- e98718289 event.rs dispatcher binds AddTimer of lifecycle callbacks (after apply); apply AddTimer/RemoveTimer forget
- 688ad6f6f event.rs provided stop_timers_of_unmounted_nodes, called at the start of
  dispatch_pending_lifecycle_events and invoke_expired_timers
- b9f948d31 module docs

## NEXT (exact)
1. Write the report scripts/THREADS8_2026_10_03.md (what was built, commits, api.json: none, least-sure spots,
   test commands, what is left) and commit it. Then update this file to "finished".

## Decisions / open questions
- The core mechanism exists; no rewrite. Gaps only.
- Window teardown is fixed in `Drop for LayoutWindow` (one place, every shell's close path).
- Timers: same rule as threads (lifecycle-started timer belongs to the node). Bound through a list the
  dispatcher collects (no new field on CallbackChange::AddTimer, so timer.rs / e2e runner patterns untouched).
