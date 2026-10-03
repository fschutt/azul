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
- 339d58966 RED thread_owner.rs tests::a_window_that_closes (3 workers x 300 ms stop together < 600 ms;
  3 stuck workers cost one ORPHAN_GRACE, not three)
- a742a0149 GREEN part 1: `managers::thread_owner::stop_all(&mut BTreeMap<ThreadId, Thread>)` (tell all,
  poll together via poll_orphan against one grace, clear). poll_orphan's message now covers both paths.
- ce9e4735f GREEN part 2: `impl Drop for LayoutWindow` -> stop_all (window.rs after the struct)
- 8aa774c5b headless shutdown_threads -> stop_all + reset thread_owners

## IN PROGRESS / NEXT (exact)
1. DONE - layout/src/window.rs: right after the `pub struct LayoutWindow { .. }` closing brace (line ~2008), add
   `impl Drop for LayoutWindow { fn drop(&mut self) { #[cfg(feature = "std")]
   crate::managers::thread_owner::stop_all(&mut self.threads); } }` with a doc comment. (Checked: no
   by-value destructure / field move of LayoutWindow anywhere in layout/, dll/, layout/tests - only `ref`
   destructures, so Drop does not trip E0509.) Commit.
2. DONE - dll/src/desktop/shell2/headless/mod.rs `fn shutdown_threads` (~line 2438): replace `lw.threads.clear()`
   with `azul_layout::managers::thread_owner::stop_all(&mut lw.threads)` and update its doc. Commit.
3. Gap 3 (decide): timers a node's lifecycle callback started never stop (map.rs map_on_after_mount adds a
   250 ms sweep timer, TerminateTimer::Continue forever; a remount adds a second). Option: bind timers in the
   same ThreadOwnerManager (owner on CallbackChange::AddTimer, set in event.rs dispatch_events_propagated next
   to AddThread; remap orphans -> remove from lw.timers + a drain the dll turns into stop_timer). Risk: app
   timers started in AfterMount (AzCalendar on_app_mounted -> start_syncing; its root has an id so it always
   matches). DECIDED: do it (same rule, same manager). Plan:
   a) thread_owner.rs: ThreadOwnerManager gains timer_owners: BTreeMap<TimerId, DomNodeId> + timers_to_stop:
      Vec<TimerId>; bind_timer / timer_owner / forget_timer / take_timers_to_stop; remap_node_ids remaps timer
      owners too and queues orphaned timers in timers_to_stop (return type unchanged: Vec<ThreadId>).
   b) window.rs LayoutWindow::remap_node_ids: `timers: _` -> `timers`, remove the orphaned timers at once.
   c) event.rs dispatch_events_propagated: collect (timer_id, hit_node) for AddTimer when binds_threads; bind
      after the changes are applied. apply_user_change AddTimer/RemoveTimer -> forget_timer. A provided
      fn stop_timers_of_unmounted_nodes() (drain -> self.stop_timer) at the start of
      dispatch_pending_lifecycle_events and of invoke_expired_timers.
   d) RED first: thread_owner.rs unit tests + a headless test next to the thread one (headless/mod.rs ~12390).
4. Report scripts/THREADS8_2026_10_03.md.

## Decisions / open questions
- The core mechanism exists; no rewrite. Gaps only.
- Window teardown is fixed in `Drop for LayoutWindow` (one place, every shell's close path) rather than in
  each shell's close code.
