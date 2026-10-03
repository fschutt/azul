Task PIM6 (azul Rust GUI toolkit, PR #476, wave 6). Read first, in this order, and follow them exactly (never compile):
1. scripts/waves/house_rules.md (the house rules - wave-6 version: building blocks, user rulings, API gotchas)
2. scripts/waves/wave6/PLAN.md (the eleven tasks, who owns which files, the contracts between tasks)
Your branch: `wt/pim6` from the base commit named in your launch prompt (`git -C <your worktree> checkout -b wt/pim6 <base>`).
TASK name: `PIM6`. Report `scripts/PIM6_2026_10_03.md`, progress `scripts/PIM6.PROGRESS.md` (commit it after every commit).

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

GOAL: AzCalendar, AzTasks, AzContacts (and the azul-pim crate) finished. Read scripts/CAL2_WEEK_2026_09_30.md,
CAL3_2026_10_01.md, TASKS_2026_10_01.md, SMALLAPPS_2026_10_01.md (Contacts), PIM_2026_10_02.md first, then LOOK.
- A RecurrenceEditor widget (layout/src/widgets/recurrence_editor.rs + both themes appended + tests + manifest):
  daily/weekly/monthly/yearly, interval, weekdays, end (never/count/until), producing azul_pim's repeat/RRULE; used by
  AzCalendar's event editor and AzTasks' task detail. List its api.json entries.
- AzCalendar: events, calendars and tasks onto the azul-storage Drive on a Thread (CAL3: still direct atomic writes
  from callbacks), "edit this occurrence" (not only delete), its event editor's close check via CloseRequested +
  prevent_window_close (CAL3: unsaved edits are lost on close), DatePicker week start (APIEXPORT added
  with_week_start; call sites chrome.rs ~236, editor_ui.rs ~262).
- AzTasks: VTODO import/export (azul_pim content_line), planned view / board view if time, theme+mode kept in
  settings (appkit), the TokenInput for tags if cheap.
- AzContacts: photo preview (data: URI -> ImageRef), CSV import with column mapping, birthday as DatePicker.
- azul-pim: ADDITIVE changes only (MAIL6 builds AzMail's To-Do bar on the task store in parallel); the remaining
  twins (TempDir copies, AzNotes search - WRITER6 adopts, AzMeet initials - MEETDRIVE6 adopts) - list them.

Report `scripts/PIM6_2026_10_03.md` per the house rules (what was built, commits, the api.json list - exact methods,
never `Type.*`; least-sure-to-compile spots; the parent's test commands; what you saw broken and did not fix; what is
left). You are running unattended: decide, note decisions in your progress file, continue; do not stop to ask. Do not
spawn subagents.
