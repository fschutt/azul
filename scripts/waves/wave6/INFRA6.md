Task INFRA6 (azul Rust GUI toolkit, PR #476, wave 6). Read first, in this order, and follow them exactly (never compile):
1. scripts/waves/house_rules.md (the house rules - wave-6 version: building blocks, user rulings, API gotchas)
2. scripts/waves/wave6/PLAN.md (the eleven tasks, who owns which files, the contracts between tasks)
Your branch: `wt/infra6` from the base commit named in your launch prompt (`git -C <your worktree> checkout -b wt/infra6 <base>`).
TASK name: `INFRA6`. Report `scripts/INFRA6_2026_10_03.md`, progress `scripts/INFRA6.PROGRESS.md` (commit it after every commit).

GOAL: the shared infrastructure the app agents rely on (contracts in PLAN.md - they code against them in parallel).
1. STORAGE MANIFEST (user ruling 2026-10-02): there is no S3 sync yet; every durable write goes into the data tree, and
   `<data root>/.azlin/cache` records what is there so the later S3/database sync only diffs. Build it INSIDE
   examples/azul-storage: the `LocalDrive` updates the manifest on every put / delete / rename / copy (key -> size,
   mtime, content hash; a format that is cheap to append and to rewrite atomically - document it), `.azlin/` itself
   is never listed as user content, and a `diff(&Manifest, &dyn Drive)` (or equivalent) answers "what changed since
   the manifest" (added / modified / deleted keys). RED tests first (crate tests: plain Rust, no azul). The S3Drive
   needs nothing (it is the sync target); say how the sync will use the manifest in your report.
2. ONE DATA ROOT: azul-appkit's `data::data_root` (--data-dir, $AZLIN_DATA, <OS data dir>/Azlin) is the root of every
   app. Some apps used `azul/` (AzShow, AzVideoCut) or `Azul/` (AzPhoto) under the OS data dir: add a one-time,
   idempotent, non-destructive migration of those legacy folders into `Azlin/` (move what is not there yet, never
   overwrite, leave a note file) and hook it into the startup path the appkit apps already call (AzCalculator /
   AzContacts: find it in examples/azul-appkit/src/ui.rs) WITHOUT changing that function's signature - the app agents
   are moving their apps onto that path in parallel. RED tests on a temp folder.
3. ONE ID MINT: azul-appkit's `data::new_uuid` duplicates `azul_storage::ids::random_seed` + `Uuid::from_seed`
   (DEDUP / PIM report): make appkit delegate to azul_storage::ids (plain Rust: appkit cannot call azul's Uuid outside
   its `ui` feature - give azul_storage::ids a `new_uuid()` that formats a v4-shaped uuid from two random seeds, used by
   appkit; the apps' `Uuid::from_seed(random_seed())` stays valid). One seed source in the repo; delete the other copies
   you find outside the apps (apps are their owners' - list app copies in the report).
4. CLOSE (user ruling 2026-10-02): remove `FullWindowState::close_callback` (never invoked by any backend:
   layout/src/window_state.rs ~125/384/570 + its test; list the api.json removal for the parent's autofix). The DOM
   event `EventFilter::Window(WindowEventFilter::CloseRequested)` is THE place for "document modified, save?": verify
   (RED tests, the headless backend first - dll/src/desktop/shell2/headless) that EVERY backend (macOS, Windows, X11,
   Wayland, headless; the titlebar's close button too - layout/src/widgets/titlebar.rs) dispatches CloseRequested
   before closing and honours `CallbackInfo::prevent_window_close()` (BLOCKS, layout/src/callbacks.rs), and that a
   veto keeps window-state changes the callback made. Make BLOCKS' `CloseGuard` (layout/src/widgets/close_guard.rs)
   listen to that event (check what it does now). Apps adopt it themselves (their owners do) - do not edit apps.
Files: examples/azul-storage, examples/azul-appkit, layout/src/window_state.rs, layout/src/widgets/close_guard.rs,
layout/src/callbacks.rs (close bits only), dll/src/desktop/shell2/*/ close handling (HEADLESS6 owns the rest of the
headless backend and run.rs - keep your edits there to the close path and say so).

Report `scripts/INFRA6_2026_10_03.md` per the house rules (what was built, commits, the api.json list - exact methods,
never `Type.*`; least-sure-to-compile spots; the parent's test commands; what you saw broken and did not fix; what is
left). You are running unattended: decide, note decisions in your progress file, continue; do not stop to ask. Do not
spawn subagents.
