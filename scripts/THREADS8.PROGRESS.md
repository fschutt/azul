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
- ff44c1340 RED video: the test-pattern and replay workers stop when told to terminate
- 0e68e6d7e GREEN video: both poll capture_common::terminate_requested each frame

## IN PROGRESS
- Gap 2: a closing window tells its workers to stop one at a time (BTreeMap drop -> each Thread destructor
  sends Terminate then waits up to 2 s) -> serial stop latencies; N misbehaving workers = N x 2 s UI hang.
  Plan: LayoutWindow::stop_all_threads (tell all first, wait on ONE shared grace, detach the rest) +
  `impl Drop for LayoutWindow` calling it; headless shutdown_threads uses it.

## NEXT
- Gap 3 (decide): timers a node's lifecycle callback started (map's 250 ms sweep timer) never stop.
- Report scripts/THREADS8_2026_10_03.md.

## Decisions / open questions
- The core mechanism exists; no rewrite. Gaps only.
