Task WRITER6 (azul Rust GUI toolkit, PR #476, wave 6). Read first, in this order, and follow them exactly (never compile):
1. scripts/waves/house_rules.md (the house rules - wave-6 version: building blocks, user rulings, API gotchas)
2. scripts/waves/wave6/PLAN.md (the eleven tasks, who owns which files, the contracts between tasks)
Your branch: `wt/writer6` from the base commit named in your launch prompt (`git -C <your worktree> checkout -b wt/writer6 <base>`).
TASK name: `WRITER6`. Report `scripts/WRITER6_2026_10_03.md`, progress `scripts/WRITER6.PROGRESS.md` (commit it after every commit).

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

GOAL: AzWriter on the shared RichTextEditor, and the editor itself finished. Read scripts/RTE_2026_10_02.md (section
8 = AzWriter's adoption plan), TEXTENG_2026_10_02.md (DocumentTextEdit.runs, CallbackInfo::get_typing_formats),
EDITOR_2026_09_30.md, NOTES_2026_10_01.md first, then LOOK at AzWriter and AzNotes.
1. AzWriter: replace its own editor/IR with the RichTextEditor + RichTextDoc (tables, alignment, page breaks are in the
   shared model already), keep its page view (A4 pages), DOCX import, its ribbon; one undo history.
2. The editor takes formats from the engine: wire `DocumentTextEdit.runs` and `get_typing_formats` into
   layout/src/widgets/rich_text* so Ctrl+B with no selection (pending format) and a formatted paste come from the
   engine, not mirrored widget-side (RTE's open gap).
3. Ctrl/Cmd+Z / Shift+Ctrl+Z never reach an app that owns its history: core/src/events.rs's shortcut block claims
   Undo/Redo (AddAndSkip) for the engine's text undo. An app (the RichTextEditor) must be able to own undo: design it
   (e.g. a focused node that declares it handles undo gets the key; RED tests in core + layout), no app workaround.
4. AzNotes: polish after the RTE adoption (LOOK), its close check via CloseRequested + prevent_window_close
   (jobs.rs ~167-169 clears the flag by hand - DEDUP), its search / tag cleaning onto azul_pim (DEDUP twin), prefixes,
   appkit, ids already random.

Report `scripts/WRITER6_2026_10_03.md` per the house rules (what was built, commits, the api.json list - exact methods,
never `Type.*`; least-sure-to-compile spots; the parent's test commands; what you saw broken and did not fix; what is
left). You are running unattended: decide, note decisions in your progress file, continue; do not stop to ask. Do not
spawn subagents.
