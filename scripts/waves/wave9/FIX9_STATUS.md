# FIX9 - the wave-9 small-fix wave (started 2026-10-05, base b454da215)

Plan: scripts/waves/wave9/SMALL_FIXES.md (86 items in 6 packages, no shared files; + the suite failures).
Resume: SendMessage the agent id ("continue from your progress file"); in a NEW conversation start a new agent per task in
the EXISTING worktree (MONDAY_RESUME_2026_10_05.md "How to resume"). Worktrees: .claude/worktrees/agent-<id>.

| Task | Package | Agent id | Branch | Progress / report | State |
|---|---|---|---|---|---|
| FIX9-LAYOUT | PKG 1 ENGINE-LAYOUT-TEXT | aed040f92949d6540 | wt/fix9-layout | scripts/FIX9_LAYOUT.PROGRESS.md / _2026_10_05.md | running |
| FIX9-PAINT | PKG 2 ENGINE-PAINT-FRAME-A11Y | a88eb9585f405aa19 | wt/fix9-paint | scripts/FIX9_PAINT.PROGRESS.md | running |
| FIX9-INPUT | PKG 3 INPUT-IO + DLL + TOOLING | a99cddf42c4359914 | wt/fix9-input | scripts/FIX9_INPUT.PROGRESS.md | running |
| FIX9-WIDGETS | PKG 4 WIDGETS | a82bcf5fe8377717a | wt/fix9-widgets | scripts/FIX9_WIDGETS_2026_10_05.md | DONE (17/18; 4.17 dialog-button disabled model needs a user decision; api: ReferencePickerEventKind::Clear; the 4 widget suite failures fixed) |
| FIX9-APPSA | PKG 5 APPS-A | acbf919509a8261aa | wt/fix9-appsa | scripts/FIX9_APPSA.PROGRESS.md | running |
| FIX9-APPSB | PKG 6 APPS-B | a8beb815bbe185f30 | wt/fix9-appsb | scripts/FIX9_APPSB_2026_10_05.md | DONE (9/12; skipped AzReader IconGrid + links (files outside), ERP filter bar; integration: Toolbar items need their id as DOM id (R-1, toolbar.rs tool()), azerp_e2e.py:83 selector + "2,400.00" (R-2)) |

Integration: merge_one.sh per branch, then the PARENT list in SMALL_FIXES.md (api.json from the reports, register new
test files in all.rs, the 9 external path fixes, the AudioSink doc refresh), codegen, dylib + 35 apps, suites.
