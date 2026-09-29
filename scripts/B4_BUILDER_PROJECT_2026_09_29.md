# B4 — AzBuilder projects: a Qt-Creator-like file viewer, the site, the guide (2026-09-29)

Branch `wt/b4-builder-project` (worktree `.claude/worktrees/agent-a4bc637dbe6f77ca8`),
cut from `fix/input-bugs-2026-09-19` at `1843e1edf` (B1's builder document model
included). **Nothing was compiled here** (no cargo, no LSP); `rustfmt +nightly
--check` was used as a parse check on every edited Rust file. The JS suites were
run here and are green.

## TL;DR

AzBuilder had no project at all: the document, the converted components and
their CSS lived in the window and died with it, "Project as JSON" round-tripped
only tests and snapshots, and the App State camera button threw a TypeError.
Now a **project is a folder on disk** that the server opens and confines every
path to; the browser shows it as a **Qt-Creator-style tree + editor** (a Project
activity and a compact section under the Inspector's palette), **saving a live
file applies it** (a stylesheet re-styles the native window, a component file
re-registers the component and its instances, `document.json` loads the
document), and **selection syncs** component file ↔ palette card ↔ document
instance. Save / Load Project carries the document, every user component, the
E2E tests and the snapshots; ZIP export / import sits on top. The site links
AzBuilder in the /ui hero, gives it a section right under it, and opens the
release page's Demos with it; the guide describes the builder as it works now.

## 1. Audit (before) — details with file:line in `scripts/B4_BUILDER_PROJECT.PROGRESS.md` §1

| Feature | Status | Evidence (short) |
|---|---|---|
| Project on disk | MISSING | nothing touched the file system but `open_file`; state in the window + `localStorage` |
| Save / load the builder document | MISSING | B1 §9; `BuilderSession` had no serialisation |
| Document stylesheet | MISSING | `to_mount_xml` wrote component CSS only |
| Import > Project from JSON | WIP (lossy) | restores tests / overrides / snapshots / symbols only (debugger.js:1450) |
| Export > Project as JSON | WIP (lossy) | registry + hierarchy dumped read-only, never re-imported (debugger.js:1501) |
| Import / Export E2E tests | WORKS | debugger.js:1472, 1529 |
| Import / Export Component Library | WIP (lossy) | a template component exports without its template (full.rs:19148) — B3's area |
| Export > Code (ZIP) | BROKEN | page expects binary / `data.files`, server answers `{download_url}` (debugger.js:1594, full.rs:18903) — B3's area, left alone |
| App State camera button | BROKEN | calls `app.handlers._saveSnapshot`, which does not exist (debugger.html:106, debugger.js:1395) — **fixed** |
| Snapshot list restore / rename / delete | WORKS | browser-only |
| E2E test persistence | WIP | `localStorage` only |
| File tree / editor | MISSING | |

## 2. Design

**On disk.**

```
<root>/azul-project.json            {format: "azul-project", version, name}
<root>/document.json                {format: "azul-builder-document", version, root}  (lossless tree, no uids)
<root>/components/<lib>/<name>.json {format: "azul-component", library, name, display_name,
                                     description, fields[{name,type,default,description}], css, template?}
<root>/styles/**/*.css              mounted after the component CSS, in path order
<root>/tests/<name>.json            one E2E test per file (AZ_E2E=<root>/tests runs them)
<root>/snapshots/<alias>.json       {alias, state}
<root>/export/<language>/…
```

**Server** (`layout/src/e2e/project.rs`, new; `E2eScratch.project`; 13 thin arms in
`full.rs` through `run_project_op` → `project::handle` → `finish_builder_op`):
`project_info`, `project_open {path, create}`, `project_close`, `project_list`,
`project_read_file`, `project_write_file {path, content, encoding?}`,
`project_create {path, directory?, content?}`, `project_rename {from, to}`,
`project_delete`, `project_save`, `project_load`, `project_export_zip`,
`project_import_zip {data}`.

* **Confinement**: every path is relative; `..`, absolute paths (`/…`, `C:…`,
  `\\server`), NUL are refused by spelling (`\` is a separator everywhere);
  then the longest EXISTING prefix of the joined path is canonicalised and must
  stay under the canonical root (a symlink that leads out is refused, a
  dangling one too). Listing and zipping never follow symlinks. A zip is
  validated entry by entry before the first write (all or nothing); a single
  top folder holding the manifest is stripped. Create / rename never overwrite.
* **Live files**: `project_write_file` applies `styles/**.css` (re-read all,
  `BuilderSession::set_stylesheet` → remount), `components/<lib>/<name>.json`
  (parse → template or data-model `ComponentDef` → upsert → remount) and
  `document.json` (`BuilderNode::from_json` → `load_document` → remount). A file
  that does not apply is still written; the answer carries `apply_error`.
  Open / create / rename / delete / import re-read the stylesheets too.
* **Save** writes the manifest if missing, `document.json` (the document, or
  the imported live DOM before the first edit), one file per component of
  every modifiable library. **Load**: components, then stylesheets, then the
  document (history cleared, window taken over).
* builder.rs additions: `BuilderNode::from_json`, `BuilderDocument::from_root`
  (DFS uids), `BuilderSession::{stylesheet, set_stylesheet, root_for_save,
  load_document}`, `to_mount_xml_with` (project CSS after component CSS).
* full.rs: `field_type_to_string`, `default_value_to_opt_string`,
  `validate_exported_fields` became `pub(super)`. gene2e: the 13 ops are
  classified as IDE surface.

**Browser** (`debugger-project.js`, new, served at `/debugger-project.js`, loaded
after debugger.js; `debugger.js` untouched):

* Project activity (folder icon): toolbar, welcome form (path, Open / Create,
  Recent, the app's cwd), tree (folders first, icons by role, component badges,
  unsaved dots, context menu: open, select / insert a component, new file with a
  fitting template, new folder, rename F2, delete, copy path; keyboard; drag a
  file onto a folder to move it).
* Editor: tabs, line gutter, a highlight layer under a transparent textarea
  (CSS, JSON, XML/HTML with `{placeholders}`, Rust, C/C++, JS, Python,
  Markdown), every character escaped; Ctrl/Cmd+S; the status bar says what was
  applied or why not; image preview for binary images.
* Inspector: a compact Project section under the palette. Sync: file →
  palette card (`azp-linked`) + first instance selected in the Document;
  instance / palette card → file selected; a component file drags into the
  Document tree like a card.
* Project menu: Open / New, Save Project (dirty tabs, `project_save`, tests,
  snapshots), Load Project (+ tests merged by name, snapshots), Export / Import
  ZIP, Close. After a reload the page adopts the server's open project, else
  re-opens the last one — and loads it only into a fresh window (inactive
  document, empty body). An explicit Close forgets it (it stays in Recent).
* The camera button: `app.handlers._saveSnapshot` now exists (prompts a name,
  saves, writes `snapshots/<slug>.json` into an open project).
* `debugger-dnd.js` (two marked lines): `select()` dispatches `azb:select` and
  is exported on `azDnd`.

**Site**: `/ui` (`doc/templates/index.template.html` + `ui-landing.css`): a hero
link and an AzBuilder section right under the hero (description, the guide's
`debugger-initial.png`, Download → release `#demos`, Guide); inventory test
extended (`.ui-builder {`, `.ui-builder-grid {`). Release page (`deploy.rs` +
`docs-release.css`): the Demos section opens with an AzBuilder block
(screenshot, description, guide link); AzBuilder joins `DEMO_APPS`, desktop
only (`DESKTOP_ONLY`: no iOS/Android rows, no Docker line — it has no
Dockerfile). Checked in headless Chrome at 1400 px and 390 px (no horizontal
scroll).

**Guide**: `doc/guide/en/architecture/gui-builder.md` rewritten: Document vs
Live DOM, drag and drop with previews, undo, convert to component, projects
(layout, tree, editor, live files, sync, save / load, ZIP, re-open, confinement),
Export in general terms (B3's dialogs), slash commands, E2E tests into `tests/`,
`## More methods` (the server messages by group), cross-references. Removed
passages about a "Properties Panel" and "WebSockets" (neither exists). The
screenshot links were `../../images/…` → `doc/guide/images`, which does not
exist (`every_guide_pointer_resolves` would flag them); now `../images/…`. New
`images/builder-project.png` (the Project view from the headless smoke — mock
data, real page).

## 3. Tests

| suite | command | status |
|---|---|---|
| project scenarios through the real dispatcher on a headless window (9): files CRUD, create/rename never overwrite, traversal refusals (`..`, absolute, NUL, `\`, root) with the outside checked on disk, symlink escape (unix), save → document.json + one file per component, load → components + stylesheet + document in the native window (`assert_layout` width 123), stylesheet save re-applies (50 → 123 → 200 → delete → 50), component file save re-registers (h1 → h2; a broken file saved but not applied), zip export (a real zip) / import (all or nothing) | `cargo test -p azul-layout --features e2e-server --lib project_tests` | RED at `dd0ce93b0` (unknown op), expected GREEN at `e06aea928` — **not run** |
| project.rs unit tests (8: path spelling, segments, component paths, base64, zip prefix, component file round trip, library from path) | `cargo test -p azul-layout --features e2e-server --lib e2e::project::tests` | not run |
| builder.rs unit tests (+3: document JSON round trip with fresh uids, refusals with reasons, stylesheet after component CSS) | `cargo test -p azul-layout --features e2e-server --lib e2e::builder::tests` | not run |
| gene2e policy gate | `cargo test -p azul-doc every_real_op_is_classified` (+ `no_zombie_is_reachable`) | not run |
| site: stylesheet inventory, guide links | `cargo test -p azul-doc stylesheets_keep_their_sections every_guide_pointer_resolves` | not run (links checked by hand: all resolve) |
| project logic (node, 13) | `node dll/src/desktop/shell2/common/debugger/debugger-project.test.js` | **13/13 green** |
| the real page in headless Chrome vs a mock server (42 checks) | `node scripts/debugger-ui/builder-project-smoke.mjs` (`--screenshot out.png` writes the Project view and the Inspector; `AZUL_ROOT=<checkout with target/codegen>` for the icon font) | **42/42 green** |
| B1's drag-and-drop smoke, re-run with the new script loaded | `node scripts/debugger-ui/builder-dnd-smoke.mjs` | **25/25 green** |

RED pass for the parent: `git apply -R` of `e06aea928` → `project_tests` red
(and `every_real_op_is_classified` stays green: the rows go with the ops); of
`245e936b1` → the node test cannot load `debugger-project.js`, the smoke's page
never gets ready.

## 4. Commits (`wt/b4-builder-project`)

| hash | |
|---|---|
| `5e3ba363d` | docs(b4): audit |
| `dd0ce93b0` | test(builder): project scenarios (RED) |
| `e06aea928` | feat(builder): AzBuilder projects are folders on disk |
| `0d53eea05` | test(debugger-ui): node logic + headless smoke (RED) |
| `245e936b1` | feat(debugger): project tree and editor, synced with the builder |
| `f69bce04a` | docs(site): AzBuilder front and center on /ui and under the release Demos |
| `d790aae09` | docs(guide): the GUI builder as it works now |
| `0155f96a5` | fix(builder): from_root decides the `<body>` wrap before moving the node |
| `30c2ac296`, `13e424008`, `eb6f53b18`, `d4a124102`, (this) | docs(b4): progress / report |

Cherry-pick notes: B3 edits `debugger.html`, `build.rs` and `platform.rs` too.
This branch's hunks sit at different lines on purpose (the script tag right
after `debugger.js`, the asset first in the `build.rs` list, the route before
the "Compressed debugger assets" block) — a clean 3-way merge is expected;
if one conflicts, keep both sides.

## 5. Manual test (real AzBuilder, after the parent's build)

1. Build the dylib with `build-dll,debug-server`, run `AzBuilder`; the browser opens.
2. Click the folder icon (second in the activity bar): the welcome form shows
   `<cwd>/AzBuilderProject`. Type `~/azb-demo`, **Create**: the tree shows
   components / snapshots / styles / tests / azul-project.json.
3. Inspector: drag Div into body, right-click it → Set classes… → `card`, drag
   Paragraph into the div; right-click the div → Convert to component… → `card`. **Project > Save Project**: the Inspector's
   Project section shows `components/user/card.json`, `document.json`,
   `tests/test-1.json`.
4. Click `card.json` in that section: the `card` palette card gets a purple
   outline, the instance row is selected. Click the body row, then the instance
   row: the file is selected again.
5. Drag `card.json` onto the body row: a second instance appears in the window.
6. Right-click `styles` → New file… → `app.css`; type
   `.card { background: #ffd; padding: 12px; }`, Ctrl/Cmd+S: the status bar
   says "stylesheet applied to the window", both cards turn yellow.
7. Open `card.json`, change the template's `<p>` to `<h2>`, save: "component
   user:card re-registered"; both instances show a heading; the palette
   thumbnail updates. Put a syntax error in, save: saved, "not applied: not
   JSON…", the window keeps the last good version.
8. Quit AzBuilder, start it again: the page re-opens `~/azb-demo` and loads it —
   the two cards and the stylesheet are back.
9. Try escaping: Terminal `/project_read_file path ../../etc/passwd` → refused
   ("contains '..'"); `ln -s / ~/azb-demo/root` then
   `/project_read_file path root/etc/hosts` → refused ("resolves outside").
10. Project > Export Project as ZIP downloads `azb-demo.zip`; Import ZIP into
    Project… with it writes the same files back.
11. App State: refresh, camera → name → the snapshot appears in the list and in
    `snapshots/`.

## 6. Least sure to compile (please look here first)

1. `full.rs` `run_project_op`: the block ends in
   `super::project::handle(op, &mut s.project, &mut s.builder, &mut map_guard, live)`
   with `live` borrowing `callback_info` and `guard = scratch(callback_info)`
   both shared, `s = &mut *guard` (split field borrows through the guard).
2. `full.rs` arms: `ProjectOp::Open { path, create: *create }` — `path: &String`
   into a `&'a str` field (deref coercion at a struct-expression field).
3. `project.rs` `upsert_component`: `match libs.iter_mut().find(..) { Some(lib)
   if !lib.modifiable => …, Some(lib) => …, None => libs.push(..) }` (NLL).
4. `project.rs` `handle`: the non-capturing closure `keep` used by value in
   several arms (`.map(keep)`; relies on it being `Copy`).
5. `project.rs` `apply_written` / `load`: `map: &mut ComponentMap` captured by
   an `and_then` closure (`apply_component(map, …)`), then reused.
6. `builder.rs` `from_json_at`: `let children: &[Value] = match … { None |
   Some(Value::Null) => &[], Some(Value::Array(cs)) => cs.as_slice(), … }`.
7. `builder.rs` `to_mount_xml_with`: `let mut css = w.css;` (a field moved out
   of the `XmlWriter` local).
8. `deploy.rs`: `|c| !DESKTOP_ONLY.contains(&c)` coerced to `fn(&str) -> bool`
   in `os_groups`; `.filter(|(crate_name, _, _)| !DESKTOP_ONLY.contains(crate_name))`
   over `&&(&str, &str, &str)`.
9. `platform.rs`: a `static … = include_bytes!(…"/debugger-project.js.br")`
   inside the route's `if` block (build.rs compresses it first in the list).

Runtime, least sure: the scenario tests assume `RemountDom` from a step lands
by the next `wait_frame` (as B1's do); the project CSS goes into the mount's
`<style>` XML-escaped, as the component CSS already did.

## 7. What is left

* Deleting a component file does not unregister the component (it stays in
  the palette until a restart); renaming one does not rename the component
  (the JSON's `library` / `name` win).
* Save Project writes tests / snapshots by name; a renamed or deleted test
  leaves its old file behind.
* No file watching: files changed outside AzBuilder show after Refresh / on
  Load; they are applied only when saved from the editor or loaded.
* The project stylesheet styles the native window only, not the palette
  thumbnails or the Components-view preview.
* The editor has no find / replace or multi-cursor (it is a textarea).
* Export > Code still does not download (JSON vs binary) — B3's dialogs.
* A real screenshot for the guide (the Project view against the real server;
  `builder-project.png` is from the mock).
