# B4 — AzBuilder project / file viewer, site, guide (PROGRESS)

Branch `wt/b4-builder-project` (worktree `.claude/worktrees/agent-a4bc637dbe6f77ca8`),
cut from `fix/input-bugs-2026-09-19` at `1843e1edf` (contains B1's builder document
model). Nothing is compiled here; the parent builds. Node / headless-Chrome tests
are run here.

## 1. Audit (before) — project save/load, import/export, snapshots, E2E panel, file-ish UI

Legend: WORKS / BROKEN / WIP (partial, lossy) / PLACEHOLDER / MISSING.

| Feature | Where | Status | Evidence |
|---|---|---|---|
| A project on disk (folder, files) | — | MISSING | No op touches the file system except `open_file` (full.rs `OpenFile`, "open in the user's editor") and the E2E shot dir. All builder state lives in the window (`E2eScratch.builder`, full.rs:3667) and the browser's `localStorage` (`azul_debugger`, debugger.js:201, 3121). |
| Save / load the builder document | — | MISSING | B1 report §9: "a document save / load format are not implemented". `BuilderSession` (builder.rs:660) has no serialisation; a server restart loses every edit. |
| Document stylesheet | — | MISSING | B1 §9: "the document has no stylesheet of its own"; `to_mount_xml` (builder.rs:617) only writes component CSS into the `<style>`. The Inspector CSS override (`set_node_css_override`, debugger.js:1821-1850) edits the live node and is lost on the next remount. |
| Import > Project from JSON | debugger.html:14, debugger.js:1450-1470 | WIP (lossy) | Restores `tests`, `cssOverrides`, `snapshots`, `resolvedSymbols` only. The export's `componentRegistry` and `htmlTree` are never read back; the builder document and user components are not in the file at all. |
| Export > Project as JSON | debugger.html:35, debugger.js:1501-1527 | WIP (lossy) | Writes tests / overrides / snapshots / symbols + a read-only dump of the live hierarchy and the registry. Re-importing does not bring back the UI or a single component. |
| Import > E2E Tests (append) | debugger.js:1472-1499 | WORKS | Appends; accepts one test or an array. |
| Export > E2E Tests (CLI format) | debugger.js:1529-1535 | WORKS | One array file (the DLL loader `run.rs:234 load_e2e_tests` accepts arrays; `layout::e2e::report::load_e2e_tests` wants one test per file). |
| Import / Export > Component Library | debugger.js:1537-1592, full.rs:18991 / 19092 | WIP (lossy) | Works for data-model components. A template component (B1 "convert to component") exports WITHOUT its template: `ExportedComponentDef` (full.rs:771) has no body, the export (full.rs:19148-19156) writes name / fields / css only, the import (full.rs:19016-19031) installs `user_defined_render_fn`. (B3's area.) |
| Export > Code > Rust / C / C++ / Python | debugger.js:1594-1671, full.rs:18903-18986 | BROKEN | The UI posts `export_code_zip` and only handles a binary `application/zip` body or `data.files`; the server answers JSON `{download_url: "data:application/zip;base64,…", filename, …}`. Result: "No files generated for rust" and no download. (B3 owns the export dialogs.) |
| `POST /debug/compile?lang=` (CSS → project ZIP) | platform.rs:377-398 | WORKS (curl only) | Not reachable from the UI. |
| App State "Save Snapshot" (camera icon) | debugger.html:106, debugger.js:1395 | BROKEN | The icon calls `app.handlers._saveSnapshot()`, which does not exist (`_saveSnapshot` lives on `app`, and returns at once without an alias): a TypeError on every click. Snapshots can only come from a project import. |
| Snapshot list: restore / rename / delete | debugger.js:1008-1080, 1403-1440 | WORKS | Browser-only (`localStorage`), never on disk. |
| `restore_snapshot` step, `run_e2e_tests {snapshots}` | debugger.js:4605-4612, 510 | WORKS | |
| E2E test list persistence | debugger.js:201-210, 3121 | WIP | `localStorage` only: tests die with the browser profile / a different port or browser. |
| "Open source file" (backtrace links) | debugger.js:1910-1920, full.rs `OpenFile` | WORKS | Best effort, absolute paths, the user's editor. |
| Components view: render_fn / compile_fn source popups | debugger.js:4399-4540 | PLACEHOLDER for builtins, WORKS for templates (B1) | `update_component_compile_fn` stores text that nothing compiles. |
| A file tree / editor in the UI | — | MISSING | |

### One save, traced (before)
Build something in the Document view → Export > Project as JSON → the file has the
tests, the snapshots and a *dump* of the live node hierarchy → restart AzBuilder →
Import > Project → the tests come back, the window stays empty: the document, the
converted components and their CSS are gone. There is nowhere to put them.

## 2. Design (to build)

**A project is a folder on disk**, opened through the server (a path the user
types; the server suggests `<cwd>/AzBuilderProject`):

```
<root>/
  azul-project.json            manifest {format, version, name}
  document.json                the builder document (lossless JSON tree, no uids)
  components/<library>/<name>.json   one file per user component (fields, css, template)
  styles/*.css                 project stylesheets, applied to the document in path order
  tests/<name>.json            E2E tests, one per file (AZ_E2E=<root>/tests runs them)
  snapshots/<alias>.json       app-state snapshots
  export/<language>/…          exported code
```

Server (`layout/src/e2e/project.rs`, arms in `full.rs`, `E2eScratch.project`):
`project_info`, `project_open {path, create}`, `project_close`, `project_list`,
`project_read_file`, `project_write_file`, `project_create`, `project_rename`,
`project_delete`, `project_save`, `project_load`, `project_export_zip`,
`project_import_zip`. Every path is RELATIVE to the root; `..`, absolute paths,
NUL and symlinks leading out are refused (checked against the canonical root).
Writing a live file applies it: `styles/**/*.css` → the builder's stylesheet
(remount), `components/**.json` → re-register the component (remount),
`document.json` → load the document (remount).

Browser (`debugger-project.js`, served at `/debugger-project.js`): a Project
activity (tree + editor tabs), a compact Project section in the Inspector
sidebar, a Project menu, selection sync (component file ↔ palette card ↔
document instance), drag a component file onto the Document tree.

## 3. DONE

| hash | what |
|---|---|
| `5e3ba363d` | docs(b4): audit |
| `dd0ce93b0` | test(builder): project scenarios, RED (`layout/src/e2e/project_tests.rs`, 9 tests) |
| `e06aea928` | feat(builder): project.rs + builder.rs stylesheet/load + full.rs arms + gene2e rows (+ 8 project.rs unit tests, 3 builder.rs unit tests; rustfmt applied to the two new files) |

## 4. IN PROGRESS

- RED JS tests: `debugger-project.test.js` (node) + `scripts/debugger-ui/builder-project-smoke.mjs`.

## 5. NEXT

1. RED server tests → commit.
2. `project.rs` + builder.rs (stylesheet, load) + full.rs arms + gene2e rows → commit.
3. RED JS tests (node logic + headless smoke) → commit.
4. `debugger-project.js` + html/build.rs/platform.rs wiring → commit.
5. Site: /ui landing section + release "Demos" AzBuilder entry → commit.
6. Guide refresh → commit.
7. Report `scripts/B4_BUILDER_PROJECT_2026_09_29.md` → commit.

## 6. Open questions

- B3 adds `debugger-export.js` + export dialogs; this branch leaves Export > Code
  alone (BROKEN above, reported) and only adds project-level zip export / import.
