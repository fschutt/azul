# PAINT7 - paint order, compositor, hit test, incremental relayout (wave 7)
Owns: layout/src/solver3/display_list.rs, layout/src/cpurender/*, the hit tester (layout/src/headless.rs hit
test, layout/src/hit_test*), the display-list / relayout cache paths in layout/src/window.rs, the dll compositor
and X11 image registration. Read first: scripts/HEADLESS6_2026_10_03.md, scripts/MEETDRIVE6_2026_10_03.md sec. 7,
scripts/SHEETSHOW6_2026_10_03.md, scripts/PIM6_2026_10_03.md ("Seen broken"), scripts/MEDIA6_2026_10_03.md.

1. A positioned box is painted before an earlier transformed sibling - the reverse of CSS paint order (CSS 2.2
   Appendix E: positioned descendants and transformed elements, which create stacking contexts, paint in tree
   order at z-index:auto / 0).
2. The hit tester ignores the animation transform channel: a node mid-slide is hit at its old place.
3. A CSS-id image registration may stay invisible on the X11 GPU path (HEADLESS6 fixed it for headless).
4. CPU compositor (MEETDRIVE6): an absolutely positioned sheet is painted UNDER an earlier scroll layer; after
   closing AzDrive's Options and switching theme + mode, pieces of the backstage stay painted over the ribbon.
5. Headless screenshots paint stale / doubled geometry while the hierarchy dump is right (SHEETSHOW6: ghost labels,
   overlapping sorter thumbnails, a slide's text drawn twice; shots in /Users/fschutt/Development/azul-work/evidence/
   sheetshow6-shots/). First check with `wait_settled` (HEADLESS6 found theme switches
   slide ~300 ms) - if it persists after settling, root-cause it.
6. Incremental relayout garbles (PIM6): AzCalendar's backstage nav item before the gap drawn displaced; AzTasks
   after Cmd+2: duplicated bold text, the settings drop-down's "Work" outside its box, the search box garbled.
   Reproduce on the prebuilt AzTasks / AzCalendar (capped runner), compare display lists before / after (PIM6 saved
   its display lists in /Users/fschutt/Development/azul-work/evidence/pim6-shots/tasks_dl_*.json).
7. An overflow:hidden span clips its glyphs after ~5 px though its box is 14 px (AzCalendar: "Lunch with Ana
   12:30 - 13:30", the time line).

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/PAINT7.PROGRESS.md exact. Finish with the report
scripts/PAINT7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.
