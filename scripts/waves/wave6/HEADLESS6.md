Task HEADLESS6 (azul Rust GUI toolkit, PR #476, wave 6). Read first, in this order, and follow them exactly (never compile):
1. scripts/waves/house_rules.md (the house rules - wave-6 version: building blocks, user rulings, API gotchas)
2. scripts/waves/wave6/PLAN.md (the eleven tasks, who owns which files, the contracts between tasks)
Your branch: `wt/headless6` from the base commit named in your launch prompt (`git -C <your worktree> checkout -b wt/headless6 <base>`).
TASK name: `HEADLESS6`. Report `scripts/HEADLESS6_2026_10_03.md`, progress `scripts/HEADLESS6.PROGRESS.md` (commit it after every commit).

GOAL: the headless backend and the /e2e corpus healthy. Read scripts/IDLE_CPU_2026_09_30.md, LIFECYCLE_2026_10_01.md,
the e2e docs in layout/src/e2e (runner.rs, full.rs headers) first.
1. The /e2e corpus (e2e/*.json, 62 scenarios: `AZ_BACKEND=headless AZ_E2E=<repo>/e2e target/release/AzPaint`, capped):
   22 failed at 2e92c759b (e.g. noninterference-tab-focus-does-not-move-scroll, op-image-cache-id-repaints,
   op-resize-grow-exposed-strip, op-resize-grow-reflow, op-resize-shrink-stays-full). Run it, triage EVERY failure:
   wrong test (fix the scenario, say why) or engine bug (RED + root cause; solver3/text3 bugs go to MAILENG6's
   report instead). Goal: 62/62 or each remaining failure explained.
2. The AZ_E2E host segfaults at exit (seen with an instrumented build; exit 139): std::process::exit while threads
   still run touching torn-down state. Check whether normal builds crash at exit too (invisible today); stop/join the
   threads (timers, workers, the debug server) before exit. dll/src/desktop/shell2/run.rs exits now go through
   `exit_dumping_profile` (PGO) - keep that.
3. Menus in headless: `show_menu_from_callback` is a no-op in the headless backend, so E2E cannot drive context menus
   or ribbon drop-downs (DRIVE2). Make headless menus real (a menu window or an in-window popup the debug server can
   query and click), with debug ops if needed (list them).
4. A window id for layout callbacks: `LayoutCallbackInfo` has no window id, so an app cannot run several editor
   windows of one kind (CAL3). Add it (core/layout), RED tests; list the api.json entry.
5. Child windows in headless E2E: routing debug requests by `window_id` (CAL3/MAIL2) - verify it works for a second
   window opened from a callback; fix what does not.
Files: layout/src/e2e, dll/src/desktop/shell2/headless, dll/src/desktop/shell2/run.rs, the menu/window plumbing in
dll/src/desktop/shell2/common (not the close path - INFRA6), core/src/callbacks / layout callbacks for item 4.

Report `scripts/HEADLESS6_2026_10_03.md` per the house rules (what was built, commits, the api.json list - exact methods,
never `Type.*`; least-sure-to-compile spots; the parent's test commands; what you saw broken and did not fix; what is
left). You are running unattended: decide, note decisions in your progress file, continue; do not stop to ask. Do not
spawn subagents.
