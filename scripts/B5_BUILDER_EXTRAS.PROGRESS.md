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

## IN PROGRESS

1. Properties panel: implementation commit (this one).

## NEXT

2 → 5, then the report `scripts/B5_BUILDER_EXTRAS_2026_09_29.md`.

## Open questions

- A browser drag cannot land in the OS window itself (every shell registers for FILE drops
  only); the canvas is the window's own CPU rendering inside the page. See the report.
