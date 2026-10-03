Task MEDIA6 (azul Rust GUI toolkit, PR #476, wave 6). Read first, in this order, and follow them exactly (never compile):
1. scripts/waves/house_rules.md (the house rules - wave-6 version: building blocks, user rulings, API gotchas)
2. scripts/waves/wave6/PLAN.md (the eleven tasks, who owns which files, the contracts between tasks)
Your branch: `wt/media6` from the base commit named in your launch prompt (`git -C <your worktree> checkout -b wt/media6 <base>`).
TASK name: `MEDIA6`. Report `scripts/MEDIA6_2026_10_03.md`, progress `scripts/MEDIA6.PROGRESS.md` (commit it after every commit).

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

GOAL: AzPhoto (Photoshop-like), AzVideoCut (Premiere-like) and AzPaint finished. Read scripts/PHOTO_2026_10_01.md,
VIDEOCUT_2026_10_01.md, VIDEO_PATH_2026_09_30.md, VIDEO_REVIEW_2026_09_30.md, DEDUP_OFFICE_2026_10_02.md first, then LOOK.
Engine API you own: azul has no way to rasterise text into pixels (PHOTO's text tool is a stub) - add one (e.g.
`RawImage::draw_text(text, font, size_px, color, x, y)` or a `TextRaster` helper over the existing cpurender glyph
raster; RED tests on pixel output; list the api.json entries). AzPhoto: the tool rail with Button disabled/toggled
states, the text tool on that API, live move-tool preview, dialog_kit/standard dialogs, exports into the data tree.
AzVideoCut: its own `fit_within` -> `RawImage::resized` (DEDUP), exports into the data tree through the Drive, the
standard ProgressDialog for the export, the B-frame/pts limits documented in its report stay as they are unless
cheap. AzPaint: appkit, prefixes, the canvas markers via the RenderImageCallback (already fixed engine-side).
Undo everywhere via azul_appkit::UndoHistory (BLOCKS moved Photo/VideoCut onto it - verify).

Report `scripts/MEDIA6_2026_10_03.md` per the house rules (what was built, commits, the api.json list - exact methods,
never `Type.*`; least-sure-to-compile spots; the parent's test commands; what you saw broken and did not fix; what is
left). You are running unattended: decide, note decisions in your progress file, continue; do not stop to ask. Do not
spawn subagents.
