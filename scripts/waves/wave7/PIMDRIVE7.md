# PIMDRIVE7 - Calendar / Tasks / Contacts / Drive / Meet leftovers (wave 7)
Owns: examples/azul-{calendar,tasks,contacts,drive,meet} and examples/azul-pim. Engine bugs: RED test + note for the
owner, never an app workaround. Read first: scripts/{PIM6,MEETDRIVE6}_2026_10_03.md.

1. PREFIXES (ruling; PIM6 skipped it): every id / class of AzCalendar, AzTasks, AzContacts carries `__azcal_`,
   `__aztasks_`, `__azcontacts_`, each a `const AzString` (AzString::from_const_str) defined once in an `ids`
   module; ~300 ids, plus the 5 E2E scripts' selectors. One app per commit series; keep the scripts in step.
2. AzTasks: a click on a task title in the "All" view blanks list and detail - reproduce on the prebuilt binary,
   root-cause (app or engine).
3. AzTasks: the planned / board view and tags as a TokenInput (if no TokenInput widget exists, list it for
   WIDGETS7 and use the closest existing piece).
4. AzCalendar: the start still reads events / calendars / settings with std::fs and `--sample` writes with
   event::save - move both through the azul-storage Drive (like the rest of its writes).
5. AzContacts: never looked at after PIM6 (photo avatars, birthday picker, CSV mapping table) - LOOK, fix the
   app's bugs.
6. AzDrive: the hand-built Details table -> an existing widget (list / grid); AzMeet: read a meeting's chat
   (chat.jsonl) back on rejoin; azdrive_e2e.py / azmeet_e2e.py onto the shared scripts/azlin_e2e.py driver.
7. LOOK at all five apps (flat / flora x light / dark, capped, one at a time, wait_settled before screenshots).

Rules: scripts/waves/house_rules.md (read it fully first). Plan + who owns what: scripts/waves/wave7/PLAN.md.
Every behaviour change: a RED test commit first (test names are sentences), then the fix. Root causes, no
workarounds. Never compile. Commit after every unit; keep scripts/PIMDRIVE7.PROGRESS.md exact. Finish with the report
scripts/PIMDRIVE7_<YYYY_MM_DD>.md (built, commits, api.json list in api.json terms, least-sure-to-compile spots, test
commands, what is left) and commit it.
