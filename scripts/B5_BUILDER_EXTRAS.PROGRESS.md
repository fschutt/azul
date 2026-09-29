# B5 — AzBuilder extras: progress

Branch `wt/b5-builder-extras`, base `d240a1b1d`. Nothing is compiled here (house rule).

## Plan (in order, RED test commit then the item's commit)

1. Properties panel for document nodes (debugger-dnd.js; reuses `app.widgets.FieldEditor`).
2. The document's own stylesheet (`BuilderDocument.stylesheet`, `builder_get_stylesheet` /
   `builder_set_stylesheet`, undo, mount, `document.json`, Export > Code's app stylesheet, panel).
3. Drop onto the canvas (`builder_hit_test` → document node through the `azb-<uid>` marker;
   a live picture of the native window in the Inspector is the drop target).
4. Hide the `azb-<uid>` markers (`get_node_hierarchy` answers them as `builder_uid`).
5. Duplicate a subtree (`builder_duplicate`) + `builder_save_document` / `builder_load_document`.

## DONE

- 1 RED `bb244292b` (node test + smoke + lib/smoke.mjs)
- 1 GREEN `68b3a492b` properties panel (node 7/7, smoke 17/17, other smokes unchanged)
- 2 RED `c196ff546` (builder_tests / export_tests / project_tests scenarios, node, smoke)
- 2 GREEN `17bdb34e7` document stylesheet (node 9/9, smoke 24/24; Rust not run)
- 3 RED `ac8b65589` (builder_hit_test scenario, node, smoke)

## IN PROGRESS

3. Canvas drops: implementation commit (this one).

## NEXT

3 (implementation) → 5, then the report `scripts/B5_BUILDER_EXTRAS_2026_09_29.md`.

## Open questions

- A browser drag cannot land in the OS window itself (every shell registers for FILE drops
  only); the canvas is the window's own CPU rendering inside the page. See the report.
