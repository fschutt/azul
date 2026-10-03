# B5 — AzBuilder extras (report, 2026-09-29)

Branch `wt/b5-builder-extras`, cut from `d240a1b1d`. **Nothing was compiled here** (house
rule). `rustfmt --check` parsed every edited Rust file (no parse errors; the formatting diffs it
lists are cosmetic, and several files had them before). All node tests and headless-Chrome smokes
were run and are green.

## What was built

| # | Item | Server | Page |
|---|---|---|---|
| 1 | Properties panel | (uses `builder_set_attribute`) | Right-hand side panel in the Inspector (Document mode): text / id / class / style / other attributes; a component instance's **arguments** from its data model (default = placeholder), then class / id / style. Reuses `app.widgets.FieldEditor` / `FieldInput` / `TypeBadge` (the Components view's look, the page's tokens). One `builder_set_attribute` per committed field (Enter, blur, checkbox) = one undo step; empty removes. Callback / slot / list arguments shown read-only. Prompts (menu, F2) stay. |
| 2 | Document stylesheet | `BuilderDocument.stylesheet`; undo snapshots = tree + stylesheet; `builder_get_stylesheet`, `builder_set_stylesheet`; mounted as the LAST `<head><style>` (the parser hangs it on the document root, `Dom.css` of `<html>`, what `with_component_css` does) so it survives remounts and wins over component CSS on ties; saved in `document.json`; exported as the app's stylesheet through B6's `project_files` (named styles) and in Subtree → code / Compile CSS | "Stylesheet" editor under Properties: dirty dot, Apply, Ctrl/Cmd+Enter or +S, Tab indents, parser warnings; follows undo / load but never overwrites unapplied text |
| 3 | Drop onto the canvas | `builder_hit_test {x, y}`: deepest node whose box contains the point and carries an `azb-<uid>` marker (plain `<div>`s have no hit-test area, so layout rects count too); `hit_test`'s loop moved into the shared `nodes_at` | The Inspector shows the window as a picture (`take_screenshot`, mapped through `get_state`'s logical size), refreshed after every edit, foldable. Hover hit-tests (one request in flight) and draws the before / into / after indicator; drop inserts (or moves a dragged row) by the tree's own rule; INTO refused → before / after; outside every node → end of `<body>`; click selects |
| 4 | Hide `azb-<uid>` | `get_node_hierarchy` answers the marker as `builder_uid`, not in `classes` (only `azb-<digits>`; `azb-card` stays); the class stays on the node | the page finds a document node's live node by `builder_uid` (class fallback for older servers) |
| 5 | Duplicate + file | `builder_duplicate {node}`; `builder_save_document`; `builder_load_document {document}` (one undoable edit, fresh uids) | row menu "Duplicate", Ctrl/Cmd+D; Export > Builder document (JSON) (→ `document.json` via debugger.js `_downloadJSON`); Import > Builder document (JSON)… |

**Why a picture and not the OS window itself (item 3):** a browser drag cannot land in the native
window: every shell registers its view for FILE drops only (macOS `NSFilenamesPboardType` in
`macos/mod.rs`, Windows `IDropTarget` / `CF_HDROP`, X11 / Wayland uri lists). The picture is the
window's own CPU rendering, and a point on it is a point in the window; `builder_hit_test` is what
a native drop path would call too.

**Why the stylesheet is in `<head>`, not a `<style>` inside `<body>` (item 2):** both end up as a
component sheet (`Dom.css`), but `collect_css_from_dom` ranks an INNER sheet below the outer one,
so a body-level sheet would lose to the component CSS on equal specificity. The head's `<style>`s
are the document root's own sheet; the document's comes last there.

## Ops (all in OP_POLICY as IDE surface, `doc/src/gene2e.rs`)

| op | args | answer |
|---|---|---|
| `builder_get_stylesheet` | — | `{active, stylesheet, css, rules, warnings}` (`rules` / `warnings` as `get_css_rules`) |
| `builder_set_stylesheet` | `css: String` | the document (`{active, can_undo, can_redo, root, stylesheet}`) + `warnings`; one undo step (same text: none); 4 MiB cap |
| `builder_hit_test` | `x: f32, y: f32` (logical window px) | `{hit: true, x, y, uid, node, rect: {x, y, width, height}, rel_x, rel_y}` or `{hit: false, x, y, uid: null}` |
| `builder_duplicate` | `node: u64` | the document + `inserted` (the copy's uid); refuses the root / an unknown uid |
| `builder_save_document` | — | `{format: "azul-builder-document", version: 1, root, stylesheet}` (no uids) |
| `builder_load_document` | `document: Value` (that file, or a bare node tree) | the document; one undo step; refuses another `format`, a non-string `stylesheet`, a tree `BuilderNode::from_json` refuses |

Changed: every document answer carries `stylesheet`; `get_node_hierarchy` nodes carry
`builder_uid` (skipped when absent) and no marker in `classes`; `document.json` gains
`stylesheet` (older files still read); `get_css_rules` / `compile_css` `document` and `node`
include the document's own sheet.

## Commits (`wt/b5-builder-extras`)

| hash | |
|---|---|
| `bb244292b` | test(debugger-ui): a properties panel edits the selected document node (RED) |
| `68b3a492b` | feat(debugger): a properties panel for the builder document's nodes |
| `c196ff546` | test(builder): the document has a stylesheet of its own (RED) |
| `17bdb34e7` | feat(builder): the document's own stylesheet |
| `ac8b65589` | test(builder): a drop on the window lands on the document node under it (RED) |
| `3fee8515d` | feat(builder): drop onto the window; builder_hit_test through the azb marker |
| `367cf5065` | test(builder): the azb-<uid> markers stay out of the inspector (RED) |
| `a2ec15ebd` | fix(builder): the azb-<uid> markers are builder_uid, not a class, in the inspector |
| `c99da1d4a` | test(builder): duplicate a subtree; the document as a JSON file (RED) |
| `611a1d122` | feat(builder): duplicate a subtree; save / load the document as JSON |
| (this) | docs(b5): report and progress |

RED pass for the parent: each `test(...)` commit's Rust scenarios fail with "Unknown op" against
its parent commit (the ops are JSON, so the RED commits compile); the node tests / smokes fail on
missing functions / elements (checked here for every item).

## api.json

**No api.json change.** Everything is in `azul_layout::e2e` (feature `e2e-server`, not in the C
API): new `DebugEvent` variants, a new field `HierarchyNodeInfo::builder_uid`, and inside the
crate-private `e2e::builder` module: `BuilderDocument::{stylesheet, set_stylesheet, duplicate,
replace_with, to_file_json, from_file_json}`, `DOCUMENT_FORMAT`, `MARKER_PREFIX`, `marker_uid`,
`node_marker`, `BuilderSession::{set_document_stylesheet, duplicate, replace_document}`; renamed
`BuilderSession::stylesheet / set_stylesheet` → `project_stylesheet / set_project_stylesheet`
(they are B4's project `styles/`, not the document's); removed `BuilderSession::root_for_save`
(= `export_document(live)`); `load_document` takes a `BuilderDocument`. `project.rs`'s
`document_file_json` / `document_root` moved into builder.rs as the two `*_file_json` methods.

## Least sure to compile (look here first)

1. `full.rs` `nodes_at`: `.or_else(|| if layout_boxes { callback_info.get_node_rect(dom_node_id) } else { None })` — a closure capturing `callback_info` (shared) and the `Copy` `dom_node_id`.
2. `full.rs` `BuilderGetStylesheet` arm: `guard.builder.export_document(live).stylesheet.clone()` (a field of a `Cow` temporary borrowed from the `MutexGuard`) while `live` borrows the layout window.
3. `full.rs` `BuilderSaveDocument` / `BuilderLoadDocument` arms: `let file = …; file` / `let reply = …; reply` on purpose (edition 2021: a tail temporary borrowing `guard` / `map_guard` would outlive them). Clippy's `let_and_return` should skip these (borrowing locals); if it does not, allow it there.
4. `builder.rs` `node_marker`: `.filter_map(|a| a.as_class()).find_map(marker_uid)` — a fn item as `FnMut(&str) -> Option<u64>`.
5. `builder.rs` `from_file_json`: `match v.get("root").and(v.get("stylesheet")) { None | Some(Value::Null) => …, Some(Value::String(s)) => s.clone(), … }` over `Option<&Value>`.
6. `builder.rs` `duplicate`: `path.split_last()` borrows the local `path` across `self.checkpoint()` / `self.renumber(&mut copy)` / `self.at_mut(parent)`.
7. `builder.rs` `replace_with`: `let BuilderDocument { mut root, stylesheet, .. } = loaded;`.
8. `builder.rs` unit test `only_azb_followed_by_digits_is_a_marker` calls `crate::xml::parse_xml_to_styled_dom` (the FastDom path) and assumes the imported `<div>` keeps its class.

Runtime, least sure: `builder_hit_test` boxes come from `get_node_rect` (layout position + size)
for nodes without a hit-test area; the scenario asserts 40px rows at y 0 / 40 with
`body { margin: 0; padding: 0 }`. A scrolled container's children would be off by the scroll.

## Test commands for the parent

```
cargo test -p azul-layout --features e2e-server --lib builder_tests      # +5 scenarios (B5 section)
cargo test -p azul-layout --features e2e-server --lib e2e::builder::tests # +6 unit tests
cargo test -p azul-layout --features e2e-server --lib export_tests        # +1 (document stylesheet export)
cargo test -p azul-layout --features e2e-server --lib project_tests       # +1 (document.json stylesheet)
cargo test -p azul-doc every_real_op_is_classified no_zombie_is_reachable
cargo test -p azul-doc every_guide_pointer_resolves
node dll/src/desktop/shell2/common/debugger/debugger-dnd-extras.test.js   # 17/17 here
node scripts/debugger-ui/builder-extras-smoke.mjs                         # 40/40 here
# unchanged, re-run green here: builder-dnd-smoke 25, builder-export-smoke 42,
# builder-project-smoke 42, debugger-dnd.test 13, debugger-export.test 14, debugger-project.test 13
```

## Twins found (NO DUPLICATION)

- `scripts/debugger-ui/lib/smoke.mjs` is new shared smoke plumbing (serve the page, headless
  Chrome, checks, a builder-document mock). `builder-dnd-smoke.mjs`, `builder-project-smoke.mjs`
  and `builder-export-smoke.mjs` still carry their own copies of those helpers (and two builder
  mocks); they can move onto the lib without changing a check.
- Download helpers: `debugger.js` `_downloadJSON` (reused here) and `debugger-export.js`
  `download` / `downloadUrl` do the same Blob + anchor click.
- The JS fallback `'azb-' + uid` in `liveNodeOf` mirrors Rust's `MARKER_PREFIX` (cross-language;
  only for servers before this branch).

## What is left

- A drop onto the OS window itself: each shell would have to accept a text drag type and route
  the drop point to `builder_hit_test` + `builder_insert` (macOS, Windows, X11, Wayland).
- The picture is refreshed 250 ms after an edit; a slow remount can show the previous frame until
  the next refresh (the refresh button forces one).
- Export > Code includes the document's own stylesheet but not the project's `styles/` (as B4 left
  it).
- Duplicate keeps every attribute, `id` too (two nodes with one id).
- Properties panel: no "add attribute" row (the context menu / slash command do it), text is a
  single-line field, Option / Vec / callback / slot arguments are read-only.
- Multi-select is still not implemented (B1's list).
