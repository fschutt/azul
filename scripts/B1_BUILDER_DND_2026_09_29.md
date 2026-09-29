# B1 — AzBuilder drag and drop, component previews, convert-to-component (2026-09-29)

Branch `wt/b1-builder-dnd` (worktree `.claude/worktrees/agent-a7be8bc20e7ff5fba`),
cut from `fix/input-bugs-2026-09-19` at `b4ae363d5`. Nothing was compiled here.
Upstream moved since (only `layout/src/e2e/runner.rs`, untouched by this branch),
so every commit cherry-picks cleanly.

## TL;DR

The drag and drop was not "flaky", it was never wired: the DOM Explorer rows had
**no `dragover`/`drop` listeners at all**, and behind them there was nothing an
edit could land in. Every edit mutated the window's live flat-DFS `StyledDom`
(which cannot take an insert at a position, a move or an undo) and the next DOM
refresh rebuilt it from AzBuilder's empty `layout()`. "Convert to component" sent
its subtree in a field serde silently dropped, then crashed on a missing
`app.ui.showView` and reported the success as a failure.

Now the builder edits a **document** (a tree with stable uids, snapshot undo),
the server re-mounts it over the native window after every edit through the
existing `mount` pipeline, the palette shows **native CPU-rendered thumbnails**
of every component (the drag image, Qt Creator style), rows show before / into /
after indicators, rows drag to move, Delete / Ctrl+Z work, and a converted
subtree becomes a **template component** with inferred parameters that shows up
in the palette and drops again.

## 1. Audit (before) — see `scripts/B1_BUILDER_DND.PROGRESS.md` §1 for file:line evidence

| UI action | message | before | after |
|---|---|---|---|
| Inspector palette | `get_component_registry` | list with a generic icon, no preview | Qt-style cards, native thumbnails (`get_component_thumbnail`), filter, non-visual builtins hidden |
| Palette dragstart | payload | BROKEN: no `type`, no drag image | typed payload, the thumbnail is the drag image |
| Drop on the DOM tree | — | BROKEN: rows had no drop listeners | Document view: before / into / after indicators → `builder_insert` |
| Insert child (menu) | `insert_node` | WIP: rightmost spine only, position ignored, lost on refresh | Document menu → `builder_insert` (Live DOM menu unchanged) |
| Delete node | `delete_node` | BROKEN for a first child (left an empty div) | `builder_delete` (Delete key, toolbar, menu) |
| Move node | — | MISSING | drag a row → `builder_move`; menu Move up / down |
| Undo / redo | — | MISSING | `builder_undo` / `builder_redo` (Ctrl/Cmd+Z, Shift+…, Ctrl+Y, toolbar) |
| Convert subtree → component | `create_component {render_tree}` | BROKEN: `render_tree` + `description` dropped by serde → empty component; then `app.ui.showView` TypeError | Document: `builder_convert_to_component` (template + inferred params, instance replaces the subtree). Live DOM path: `create_component {render_tree}` now stores the template; `showView` fixed |
| Component detail preview | `get_component_preview` | WORKS for builtins | also renders converted/template components |
| Mini tree ("Render Output") edits | `update_component {render_tree}` | PLACEHOLDER: dropped by serde, answered `ok` | `render_tree` replaces the template; `get_component_render_tree` answers the template itself, so edits round-trip |
| Edit render_fn | `update_component_render_fn` | PLACEHOLDER | a builder template edited here becomes live (render fn switched) |
| First launch | — | the default E2E test switched the page to the Testing view, hiding explorer + palette | stays in the Inspector |
| HTTP transport | every POST | one `read()` of 16 KiB: bigger / split bodies truncated | reads the whole request (Content-Length) |

### One drag, traced (before)
`dragstart` (debugger.js:1764) put `{library, component}` in text/plain → `dragover`
on a `.tree-row`: no listener, nobody calls `preventDefault`, the browser refuses
the drop — **the chain broke at link 2**. Behind it: `insert_node` ignored
`position` (event.rs:7254) and refused parents off the rightmost spine
(full.rs:18251); `InsertChildNode` mutated the live `StyledDom` in place, and the
next `Update::RefreshDom` re-ran `layout()` (an empty body) and erased it.

## 2. Root causes

1. **No document model.** The builder edited the renderer's flat DFS arena; an
   insert at a position, a move and an undo are not representable there, and a
   DOM refresh discards it.
2. **No template concept.** `ComponentDef` has no body; a user component renders
   its data model as a flat list of divs. "Convert" had nowhere to store the
   subtree; every component tree editor was client-side only.
3. **Silent schema drift.** The UI sent `render_tree` / `description` that the
   server never declared; `DebugEvent` has no `deny_unknown_fields`, so they
   vanished and the ops answered `ok`.
4. **UI never finished**: no drop targets on the tree, a missing function
   (`showView`), a first-launch view switch that hid the explorer.

## 3. What was built

### Server (`layout/src/e2e/builder.rs`, new; arms in `full.rs`)
* `BuilderDocument`: root `<body>` = uid 0; every insert takes one new uid
  (never reused, not even after undo); element text lives in the `text`
  attribute. Edits validate first, then snapshot (undo cap 100). Refusals with
  reasons: unknown uid, the root, own subtree, leaves (text / void / instance),
  index past the end, and HTML auto-close nesting (`<div>` in `<p>`, `li` in
  `li`, …) — which the parser would otherwise mount as siblings.
* Mounting: the document is serialised to one XML document and pushed through
  `CallbackChange::RemountDom` (the `mount` op's pipeline): the window shows
  exactly the document, and a later `RefreshDom` keeps it. Component instances
  are expanded first (a `lib:name` tag is "draws nothing" to the parser); their
  CSS goes into the `<style>` once; a missing component renders a visible
  placeholder. Every element carries `azb-<uid>` so the UI finds its live node.
* The first edit imports what the window shows (DFS uids from `<body>`,
  engine-internal `__azul-*` subtrees skipped), so an app's UI is not wiped.
* Template components: template XML in `ComponentDef::render_fn_source` behind
  `<!-- azul-builder-template -->`, `render_fn = builder_template_render_fn`
  (no `repr(C)` / api.json change). `{name}` placeholders (`{{`/`}}` literal),
  values XML-escaped; nested instances expand recursively (depth cap 16, plus a
  thread-local re-entrancy cap). Inference: text → `text`, `text_2`, …;
  attributes other than class / id / style → same-named String params.
* Thumbnails: `get_component_thumbnail` renders the default instance with the
  CPU renderer (`render_component_preview`, width 140, dpi 2 from the UI) and
  caches it per window by a fingerprint of css + template + data model + render
  fn; `cached: true` on a hit; `data: null, empty: true` for invisible ones.

New ops (Rust `DebugEvent` variants, NOT in api.json): `builder_get_document`,
`builder_insert`, `builder_move`, `builder_delete`, `builder_set_attribute`,
`builder_undo`, `builder_redo`, `builder_reset`, `builder_convert_to_component`,
`get_component_thumbnail`. Changed: `create_component` (+`description`,
+`render_tree`, refuses a duplicate name), `update_component` (+`render_tree`),
`get_component_render_tree` (a template answers with itself),
`update_component_render_fn` (marker source → live template), component edits
re-mount an active document. `E2eScratch` gained a private `builder` field.
`doc/src/gene2e.rs` OP_POLICY classifies the new ops as IDE surface (its
`every_real_op_is_classified` test would fail otherwise).

### Browser (`debugger-dnd.js`, new, served at `/debugger-dnd.js`)
Document / Live DOM switch; Qt-style palette with lazily loaded native
thumbnails (two requests at a time as cards scroll in); drop indicators (line
indented to the target depth / highlighted row / tree end); drag-to-move;
Delete, Ctrl/Cmd+Z, Shift+Ctrl/Cmd+Z, Ctrl+Y, arrows, F2 / Enter / double-click
to edit text; context menu (insert inside, edit text / classes / id, move up /
down, Convert to component…, Delete); toolbar (undo, redo, convert, delete,
reset); clicking a row shows its live node in the detail panel; slash commands
for the new ops. `debugger.js` keeps two delimited one-spot fixes (first-launch
view, `showView`). Mode defaults to Document for AzBuilder (empty window or an
active document), Live DOM for any other app; the choice persists.

### Debug server transport (`debug_server/platform.rs`)
`read_http_request` reads the head, then exactly `Content-Length` bytes (64 MiB
cap, stops on close / the 5 s timeout).

## 4. Tests

| suite | command | status |
|---|---|---|
| server messages, end to end on a headless window (9 scenarios: insert at before / into / after, move incl. the same-parent slot rule, delete of a first child + undo / redo, refusals with reasons, convert → palette → drop again, `create_component {render_tree}`, `update_component {render_tree}`, thumbnail + cache hit / miss, reset) | `cargo test -p azul-layout --features e2e-server --lib builder_tests` | RED at `90c194056` ("Unknown op"), expected GREEN after `01faf30e9` — **not run (no compiling here)** |
| document model unit tests (18) | `cargo test -p azul-layout --features e2e-server --lib e2e::builder::tests` | not run |
| gene2e policy gate | `cargo test -p azul-doc every_real_op_is_classified` (and `no_zombie_is_reachable`) | not run |
| drag-and-drop logic (node, 13) | `node dll/src/desktop/shell2/common/debugger/debugger-dnd.test.js` | **13/13 green** |
| the real page in headless Chrome, mock server (25 checks) | `node scripts/debugger-ui/builder-dnd-smoke.mjs` (`--screenshot out.png`, `AZUL_ROOT=<checkout with target/codegen>` for the icon font, `CHROME=` to pick a browser) | **25/25 green**; it found the first-launch-in-Testing bug |

Note: the e2e module only compiles with `--features e2e-server`; the default
`layout --lib` suite does not run these.

## 5. Manual test (real AzBuilder, after the parent's build)

1. `cargo build --release -p azul-dll --features build-dll,debug-server` (the
   builder needs the server), stage the dylib, build and run `AzBuilder`. The
   browser opens `http://localhost:8080`.
2. Fresh profile or not: the page shows the **Inspector** with **Document**
   active in the DOM Explorer toolbar, a `body` row and the hint "Drag a
   component…". The Components palette shows cards with rendered thumbnails
   (Paragraph shows "Paragraph text", Heading 1 a heading, Button a button; no
   html/head/script cards).
3. Drag **Paragraph** onto `body`: the row highlights (INTO); on drop a `p
   "Paragraph text"` row appears selected and the native window shows the text.
4. Drag **Div** onto the top edge of the `p` row: a blue line above it (BEFORE);
   drop → `div` above `p`, in the window too.
5. Drag **Div** onto the middle of the `p` row: no indicator (a div cannot go in
   a p). Drag **Span** there: INTO works; the span renders inside the p.
6. Drag the `p` row onto the middle of the `div` row → it nests; drag it onto the
   bottom edge of the div → it comes back out after it. Try dragging the div onto
   its own child: no indicator.
7. Select the span, press **Delete** → gone (window too), the p is selected.
   **Ctrl/Cmd+Z** → back. **Shift+Ctrl/Cmd+Z** → gone again. Undo/redo toolbar
   buttons enable/disable accordingly.
8. Double-click the p row → edit its text → the window updates.
9. Right-click the div → **Convert to component…** → name `my-card` → the div's
   subtree becomes a `user:my-card` row; a **USER** section with a `my-card`
   card (with its thumbnail) appears in the palette; the window looks the same.
   Terminal logs the inferred parameters (`text`, …).
10. Drag the `my-card` card into `body` → a second instance renders. Select it,
    F2 → type a new text → only that instance changes (the `text` argument).
11. Open the Components view (third activity icon) → library `user` → `my-card`:
    the preview renders the template; "Render Output" shows the template with
    `{text}`; drop a palette item into it → the preview and the native window
    (both instances) update.
12. Click a Document row → the detail panel shows that node's CSS / layout.
13. Toolbar reset (↻) → confirm → the window goes back to AzBuilder's empty body.
14. Switch to **Live DOM** → the raw inspector tree; its context menu "Create
    component from subtree" now creates a template component and opens the
    Components view (it used to report failure).

## 6. Commits (branch `wt/b1-builder-dnd`)

| hash | |
|---|---|
| `7403fddb5` | docs(b1): audit |
| `90c194056` | test(builder): server messages reach the native window (RED) |
| `4d2c4c9ac` | progress |
| `01faf30e9` | fix(builder): a document model behind AzBuilder's drag and drop |
| `e2d5b397e` | progress |
| `6a8304229` | fix(builder): a template never bakes in the builder's azb markers |
| `9635a5f49` | test(debugger-ui): node logic test + headless smoke (RED) |
| `47dbf79b5` | feat(debugger): AzBuilder drag and drop with native component previews |
| `35b1432fb` | progress |
| `c49c048eb` | fix(debug-server): read the whole HTTP request |
| `759b7bed3` | test(builder): a component instance takes no children either |
| (this report + progress) | docs(b1) |

RED pass for the parent: `git apply -R` of `01faf30e9` (+ `6a8304229`) → the
`builder_tests` go red; of `47dbf79b5` → both JS tests go red (the node test
cannot load `debugger-dnd.js`, the smoke test's page never gets ready).

## 7. API changes

* No api.json change. `ComponentDef` / `ComponentLibrary` untouched.
* `azul_layout::e2e::DebugEvent`: 10 new variants, `CreateComponent` +2 fields,
  `UpdateComponent` +1 field (all `#[serde(default)]`, old JSON still parses).
* `azul_layout::e2e::E2eScratch`: private field `builder`.
* New crate-private module `azul_layout::e2e::builder` (pub items inside a
  private module; `builder_template_render_fn` is the render fn stored in
  converted components).
* HTTP: new asset route `GET /debugger-dnd.js`.

## 8. Least sure to compile (please look here first)

1. `layout/src/e2e/full.rs` `GetComponentThumbnail` arm:
   `scratch(callback_info).builder.thumbnail(callback_info, …)` — two shared
   reborrows of the `&mut CallbackInfo` in one call (the guard temporary and the
   argument). Fallback: collect the font manager / system style into locals and
   pass those instead of `callback_info`.
2. `builder.rs` `thumbnail_key`: `(def.render_fn as usize)` — fn-pointer (higher-
   ranked) to integer cast. Fallback: drop it from the key (the template + CSS +
   data model are already in it).
3. `builder.rs` `node_from_styled`: the `take_uid` closure borrows `uids`
   mutably and the recursion below reuses `uids` (relies on NLL ending the
   closure's borrow at its last use).
4. `builder.rs` `why_not_inside`: `let (A { .. }, B { .. }) = (parent, child)
   else { … }` over a tuple of references.
5. `builder.rs` `write_template`: `match inf.as_deref_mut() { Some(i) if … }`
   (binding a `&mut` in a guarded arm) inside a loop.
6. `BuilderSession::undo`: `self.edit(None, map, BuilderDocument::undo)` passes
   a method path as `impl FnOnce(&mut BuilderDocument) -> Result<R, String>`.
7. `full.rs` UpdateComponent arm: `install_template(comp, tree)` then the
   existing `lib.components = …from_vec(comps)` restore while `comp` is still in
   scope (the same NLL pattern the arm already uses for `fields`).
8. `crate::cpurender::render_component_preview` / `ComponentPreviewOptions` paths
   from inside the e2e module (full.rs spells them `azul_layout::cpurender::…`).

## 9. What is left

* **Properties panel for document nodes**: text / classes / id / component
  arguments are edited through prompts (context menu, F2); a proper side panel
  (the data-model editor bound to `builder_set_attribute`) would be nicer.
* **Document CSS**: the document has no stylesheet of its own; styling comes
  from component CSS (Components view → Scoped CSS). The Inspector's CSS
  override (`set_node_css_override`) edits the live node and is lost on the next
  remount.
* **Export / import**: template components are not in `ExportedComponentDef`
  (export_component_library / export code) — B3's area; the template is in
  `render_fn_source`, `compile_fn` is still the data-model one.
* **Canvas drops**: dropping onto the native window itself (hit-test → document
  node via the `azb-<uid>` class) is not wired; drops go to the tree.
* The Live DOM tree still shows the `azb-<uid>` marker classes on mounted nodes.
* Duplicate (copy) of a subtree, multi-select, and a document save / load
  format are not implemented.
* `insert_node` / `delete_node` (Live DOM menu) still have their old limits; the
  Document view is the supported way to edit.
