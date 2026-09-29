# B1 — AzBuilder drag and drop (progress)

Branch `wt/b1-builder-dnd` (cut from `fix/input-bugs-2026-09-19`), worktree
`.claude/worktrees/agent-a7be8bc20e7ff5fba`. No compiling in this worktree: the
parent compiles once and runs the suites.

## 1. Audit (2026-09-29, before any fix)

JS = `dll/src/desktop/shell2/common/debugger/debugger.js` (4828 lines, not
6.7k), HTML = `debugger.html`, server = `layout/src/e2e/full.rs` (the op
dispatcher moved from the DLL into `azul-layout`; the DLL only serves the
assets, `debug_server/platform.rs`), shell = `dll/src/desktop/shell2/common/event.rs`.

| UI action | JS (file:line) | message sent | server (file:line) | verdict |
|---|---|---|---|---|
| Inspector palette: list components | JS:1736 `_loadPaletteComponents` | `get_component_registry` | full.rs:11934 `build_component_registry` | WORKS as a list; PLACEHOLDER visuals: a generic `widgets` icon per item (JS:1756), no preview |
| Palette item dragstart | JS:1764 | (payload only) `{library, component}` | — | BROKEN: no `type:'component'` (every drop handler in the file checks `data.type === 'component'`, JS:2737), no drag image |
| Drop a palette item onto the DOM tree | JS:656-792 `_renderTreeNode` | — | — | BROKEN: the tree rows have NO `dragover`/`drop` listeners at all, so the browser shows "not allowed" and `drop` never fires. The chain breaks at link 2 of 5 |
| Context menu "Insert child div/p/…" | JS:1917 `ctxInsertChild` | `insert_node {parent_id, node_type}` | full.rs:18195 → event.rs:7150 | WIP: refused unless the parent is on the DOM's rightmost spine (full.rs:18225-18262); `position` is ignored (event.rs:7254 `let _ = position`); a parent with no children leaves the node at the root (event.rs:7242-7251). In AzBuilder (`layout()` returns an empty body, `examples/azul-builder/src/lib.rs:3`) the next `RefreshDom` rebuilds from `layout()` and the node is gone |
| Context menu "Delete node" | JS:1930 `ctxDeleteNode` | `delete_node {node_id}` | full.rs:18296 → event.rs:7285 | BROKEN for a first child: the flat DFS hierarchy derives first child = id+1, the parent is never re-pointed (event.rs:7309-7314, the code says so), so the "deleted" node stays as an empty `<div>` and its former children dangle |
| Move a tree node | — | — | — | MISSING: no UI, no op |
| Undo / redo of tree edits | — | — | — | MISSING (only the E2E-only `undo_app_state` for app state, full.rs:11137) |
| "Create component from subtree" (the convert feature) | JS:1943 `ctxCreateComponentFromSubtree` | `create_component {library, name, display_name, description, render_tree}` | full.rs:2917 / 19060 | BROKEN twice: (1) `CreateComponent` has no `description`/`render_tree` fields and `DebugEvent` has no `deny_unknown_fields` (full.rs:1984-1986), so serde DROPS the subtree silently and an EMPTY component is created (renders as an empty `<div>` via `user_defined_render_fn`, core/src/xml.rs:2993); (2) then `app.ui.showView('components')` (JS:2024) throws — `app.ui` only has `switchView` (JS:581) — and the success is logged as "Create component failed: app.ui.showView is not a function". There is no template concept at all: `ComponentDef` has no body, user components render their data model as a flat list of divs |
| Components view: list item dragstart | JS:2219-2220 | payload `{type:'component', library, component}` | — | WORKS (source side) |
| Component detail: preview | JS:2590 `_loadComponentPreview` | `get_component_preview` | full.rs:19230 → cpurender/raster.rs:5200 | WORKS (CPU render → PNG data URI). A component with no visible content returns an EMPTY `data:` URI (raster.rs:5365-5374), the UI shows a broken image |
| Component detail: "Render Output" mini tree | JS:2653 `_loadMiniTree` | `get_component_render_tree` | full.rs:19367 | WORKS read-only |
| Mini tree: drop a component into it (before / into / after) | JS:4121-4239 + JS:2736 `_handleTreeDrop` | `update_component {library, name, render_tree}` (JS:2883) | full.rs:2934 / 19149 | PLACEHOLDER: edits a local JSON copy; `UpdateComponent` has no `render_tree` field → dropped silently, answers `ok`, the preview reloads unchanged, the next `_loadMiniTree` shows the old tree again |
| Mini tree context menu: insert / duplicate / move up / move down / delete | JS:2778-2881 | same `update_component {render_tree}` | same | PLACEHOLDER (same reason) |
| Data-model slot drop zone ("Drop component here") | JS:3691-3724 | none (local `onChange`) | — | PLACEHOLDER: `onFieldChange` (JS:2326) only logs and re-requests the preview WITHOUT `args`, so the value never reaches the server |
| Edit render_fn | JS:2907 | `update_component_render_fn` | full.rs:19494 | PLACEHOLDER: stores the text, "hot-replacement not yet supported" — rendering never changes |
| Palette thumbnails (Qt Creator style) | — | — | — | MISSING |

### One drag, traced

1. `dragstart` on `.palette-item` (JS:1764) — sets `text/plain` = `{"library":"builtin","component":"p"}`. No `type`, no `setDragImage`.
2. `dragover` on a `#dom-tree-container .tree-row` — **no listener**, so nothing calls `preventDefault()`; the browser refuses the drop. **The chain breaks here.**
3. (would-be) `drop` → there is no handler; and even with one:
4. (would-be) message: the only insert op is `insert_node`, which cannot insert at a position (event.rs:7254) and refuses any parent off the rightmost spine (full.rs:18251).
5. (would-be) native window: `CallbackChange::InsertChildNode` mutates the live `StyledDom` in place; the next `Update::RefreshDom` re-runs AzBuilder's `layout()` (an empty body) and the node disappears.

### Root causes

* There is no document model. Every edit is a surgical mutation of the live, flat-DFS `StyledDom`, which cannot represent an insert at an arbitrary position, a move, or an undo, and is thrown away by the next DOM refresh.
* The component system has no template: a user component cannot own a DOM subtree, so "convert subtree to component" has nothing to store it in, and every tree editor for components is client-side only.
* The browser UI grew against server shapes that were never implemented (`render_tree`), and serde drops unknown fields without a word.
* Also found: the HTTP handler reads a request with ONE `read()` into a 16 KiB buffer (platform.rs:204-208) — a larger body (an import, a big `render_tree`) or a body split across TCP segments is truncated.

### The fix (design)

* A per-window **builder document** (`layout/src/e2e/builder.rs`): a real tree of nodes with stable `uid`s (elements, text, component instances with attributes), with insert-at-index / move / delete / set-attribute / snapshot undo-redo. Every edit re-serialises the document to XML and remounts it through the existing `mount` pipeline (`CallbackChange::RemountDom` → `LayoutWindow::e2e_mount` → `regenerate_layout`), so the native window shows exactly the document and a `RefreshDom` can no longer lose it. Component instances are EXPANDED into plain XML before mounting (a `lib:name` tag would be dropped by `element_draws_nothing`, core/src/xml.rs:6502).
* **Template components**: the XML template lives in `ComponentDef::render_fn_source` behind a marker comment and renders through a new `builder_template_render_fn` (no `repr(C)` / api.json change). Parameters are inferred from the subtree (text → `text`, `text_2`…; `href`/`src`/`placeholder`/… → same-named String fields) and written as `{name}` placeholders. `get_component_preview`, `get_component_render_tree` and the thumbnails therefore work for converted components unchanged.
* New ops: `builder_get_document`, `builder_insert`, `builder_move`, `builder_delete`, `builder_set_attribute`, `builder_undo`, `builder_redo`, `builder_reset`, `builder_convert_to_component`, `get_component_thumbnail` (cached per component fingerprint). `create_component` / `update_component` now accept the `render_tree` the UI already sends.
* Browser: `debugger-dnd.js` (new) — Document/Live-DOM tree switch, Qt-Creator-style palette with native thumbnails, row drop zones (before / into / after with an indicator), drag-to-move, Delete key, Cmd/Ctrl+Z / Shift+Cmd/Ctrl+Z, context menu convert.

## DONE
- (this audit)

## IN PROGRESS
- RED tests for the builder ops

## NEXT
1. RED: `layout/src/e2e/builder_tests.rs` (run through `run_e2e_test`, fails today with "Unknown op").
2. Fix: `builder.rs` + `DebugEvent` variants + dispatch arms; `render_tree` on create/update component.
3. Browser: `debugger-dnd.js`, served + loaded; minimal hooks in `debugger.js`.
4. Node test for the pure JS (drop-zone math, payloads).
5. Report `scripts/B1_BUILDER_DND_2026_09_29.md`.

## Open questions
- Template components are not carried by `export_component_library` (no field for them in `ExportedComponentDef`); export is B3's area.
