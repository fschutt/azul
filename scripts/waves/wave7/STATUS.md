# Wave 7 resume table (base 2e55eef06, started 2026-10-03 ~09:40)

Resume after an outage: scripts/waves/README.md. A stopped agent: SendMessage its id ("continue from your
progress file"). Worktrees: /Users/fschutt/Development/azul/.claude/worktrees/agent-<id>.

| Task | Agent id | Branch | Worktree | State |
|---|---|---|---|---|
| TEXT7 | a19030a2753308208 | wt/text7 | .claude/worktrees/agent-a19030a2753308208 | MERGED, DONE (21 commits; report scripts/TEXT7_2026_10_03.md; text-indent fixed for greedy + Knuth-Plass, font-load reshape, bolder/lighter in the cascade, dll font-index; items 5/6 were already fixed in wave 5 - pin tests; edits in LAYOUT7 files getters.rs / fc.rs / sizing.rs and core compact.rs / prop_cache.rs, css font.rs) |
| LAYOUT7 | aece3b97049bb1c3a | wt/layout7 | .claude/worktrees/agent-aece3b97049bb1c3a | running |
| PAINT7 | a3f339e65a9f1d3c3 | wt/paint7 | .claude/worktrees/agent-a3f339e65a9f1d3c3 | MERGED, DONE (report scripts/PAINT7_2026_10_03.md; no api.json; paint order step 8 + compositor in-place layers, one transform lookup for hit test, X11 rebuild, released stale animation values, nested slides relative, clip spaces, incremental CPU repaint with transforms; one-line edits in EVENTS7 runner.rs / event.rs, core gpu.rs) |
| EVENTS7 | a083d3b843e66855a | wt/events7 | .claude/worktrees/agent-a083d3b843e66855a | MERGED, DONE (report scripts/EVENTS7_2026_10_03.md; api.json FocusEventFilter::TypingStyleChanged (last variant); VirtualView clicks bubble to the host page; macOS Edit menu through press_shortcut_keys; undo/redo moved into LayoutWindow; headless menus close; least sure: hover_callbacks_along_path lifetimes, undo_redo.rs impl, macOS edit_command) |
| WIDGETS7 | a82f849dedcbbbe5d | wt/widgets7 | .claude/worktrees/agent-a82f849dedcbbbe5d | DONE (report scripts/WIDGETS7_2026_10_03.md; INTEGRATION: api.json CloseGuard.dirty_check + ToDoBar.week_start; run scripts/waves/wave7/widgets7_rename_summary_list.py --api, then gen_codegen_lowering.py, then --apps AFTER merging OFFICE7; edits in core prop_cache.rs (placeholder cascade), css colour printers) |
| OFFICE7 | ae791904cdd96da77 | wt/office7 | .claude/worktrees/agent-ae791904cdd96da77 | DONE, not merged (report scripts/OFFICE7_2026_10_03.md; api: Update.max_self, RichTextEditor.line_height field + set/with_line_height) |
| PIMDRIVE7 | a5305b6b2e2aa001f | wt/pimdrive7 | .claude/worktrees/agent-a5305b6b2e2aa001f | running |
| DATATABLE7 | a5fe1fc603353c662 | wt/datatable7 | .claude/worktrees/agent-a5fe1fc603353c662 | MERGED (report scripts/DATATABLE7_2026_10_03.md; api list in report; AccessibilityState SortedAscending/Descending) |
| CHART7 | a316f8beeb5434fe0 | wt/chart7 | .claude/worktrees/agent-a316f8beeb5434fe0 | MERGED 7ef97bd8c (report scripts/CHART7_2026_10_03.md; dashboard lib.rs wiring still TODO - see report "lib.rs lines"; api list in report) |
| TOOLS7 | ae314b7cb341cd9cb | wt/tools7 | .claude/worktrees/agent-ae314b7cb341cd9cb | MERGED (report scripts/TOOLS7_2026_10_03.md; 185 autofix+patch tests pass; next scan moves TextRasterStyle css -> image) |

2026-10-03 integration round 1 (in progress): merged TEXT7, EVENTS7, PAINT7, TOOLS7, DATATABLE7, CHART7. WIDGETS7 waits for OFFICE7 (the SummaryList rename script runs --api, lowering, then --apps after OFFICE7).
