Task MEETDRIVE6 (azul Rust GUI toolkit, PR #476, wave 6). Read first, in this order, and follow them exactly (never compile):
1. scripts/waves/house_rules.md (the house rules - wave-6 version: building blocks, user rulings, API gotchas)
2. scripts/waves/wave6/PLAN.md (the eleven tasks, who owns which files, the contracts between tasks)
Your branch: `wt/meetdrive6` from the base commit named in your launch prompt (`git -C <your worktree> checkout -b wt/meetdrive6 <base>`).
TASK name: `MEETDRIVE6`. Report `scripts/MEETDRIVE6_2026_10_03.md`, progress `scripts/MEETDRIVE6.PROGRESS.md` (commit it after every commit).

FIRST, LOOK (the apps were written blind - "the look is written, not seen" in every report): run your app(s) headless
from the prebuilt binaries (/Users/fschutt/Development/azul/target/release/<App>, built from aa59b2d84) through
scripts/waves/tools/run_capped.sh (it holds a machine-wide lock - one app at a time on this 8 GB Mac), drive them over
the debug server (scripts/azlin_e2e.py is the Python helper; ops in layout/src/e2e/full.rs; `screenshot` writes a PNG
you can read), in flat + flora and light + dark, through the main flows. Write what is broken into your progress
file (with screenshots under your worktree's target/ - not committed), THEN fix it: engine bugs RED first in the
engine where they live (if the engine area belongs to another task, write it into your report for its owner instead),
app bugs RED first in the app. Your app's existing E2E script must cover the flows you fix.
APP FINISH CHECKLIST (each app you own): built on its shell, flat+flora x light+dark correct; `__az<app>_` class/id
prefix constants defined once; on azul-appkit (args, data root, settings remembered across restarts, About via the
standard AboutDialog, shortcuts table); durable data + exports through the Drive into the data tree (no direct file
writes from callbacks; jobs on a Thread); a document app asks "save changes?" via CloseRequested + prevent_window_close
or the CloseGuard widget; ids via Uuid::from_seed(azul_storage::ids::random_seed()); no `ctrl || meta`; no duplicated
helpers (DEDUP reports); sample data + empty state; its E2E script passes against the prebuilt binary where the
behaviour does not depend on your new code.

GOAL: AzMeet and AzDrive fully working. Read scripts/MEET2_2026_10_01.md, DRIVE1_AZDRIVE_2026_09_30.md,
DRIVE2_2026_10_01.md, FB2_AZDRIVE_2026_09_30.md, DEDUP_OFFICE_2026_10_02.md (Drive / Meet items) first, then LOOK.
AzDrive: an Explorer-style file manager over the azul-storage Drive (local now, S3 later): every ribbon / context-menu
command really works (rename, move/copy to, delete to a trash folder in the tree, new folder/item, sort/group/columns,
the layouts, details pane, recent locations, search), selection via ListSelection, drag and drop, a progress dialog for
long jobs (standard ProgressDialog), the `.azlin/` folder hidden. Context menus are not shown by the headless backend
(DRIVE2) - HEADLESS6 is fixing that; until then reach commands by keyboard / ribbon in the E2E.
AzMeet: a meeting app (camera, screen share, participants, chat) - finish what MEET2 left (read its report's open
list), make every flow work headless with the mock camera/screen sources, the meeting folder in the data tree (user
ruling: meeting data = files in a per-meeting folder), initials via azul_pim (DEDUP: AzMeet keeps its own copy).

Report `scripts/MEETDRIVE6_2026_10_03.md` per the house rules (what was built, commits, the api.json list - exact methods,
never `Type.*`; least-sure-to-compile spots; the parent's test commands; what you saw broken and did not fix; what is
left). You are running unattended: decide, note decisions in your progress file, continue; do not stop to ask. Do not
spawn subagents.
