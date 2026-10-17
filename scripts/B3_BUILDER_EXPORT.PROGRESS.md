# B3 — AzBuilder quick exports + DOM codegen fixes (PROGRESS)

Branch `wt/b3-builder-export` (worktree `.claude/worktrees/agent-a6baf308dfe230142`), cut from the
local `fix/input-bugs-2026-09-19` tip `1843e1edf` (contains B1's drag and drop). Nothing is
compiled here (house rule); node / headless-Chrome tests are run.

## 1. AUDIT (before) — every Export / Import / codegen entry point

Line numbers are at `1843e1edf`. "UI" = `dll/src/desktop/shell2/common/debugger/`.

| # | Entry point | Where | Verdict | Evidence |
|---|---|---|---|---|
| 1 | Import > Project from JSON | UI debugger.html:16, debugger.js:1447-1470 | WORKS (client only) | restores tests / cssOverrides / snapshots / resolvedSymbols into localStorage. The `htmlTree` and `componentRegistry` that Export > Project writes (debugger.js:1519-1524) are IGNORED on import; the builder document is in neither → a "project" never carries the UI you built. |
| 2 | Import > E2E Tests (append) | debugger.html:21, debugger.js:1472-1498 | WORKS | client only. |
| 3 | Import > Component Library | debugger.html:26, debugger.js:1542-1571 → `import_component_library` full.rs:18991-19086 | BROKEN for converted components | `ExportedComponentDef` (full.rs:771-786) has no template; import always sets `render_fn: user_defined_render_fn`, `compile_fn: user_defined_compile_fn` (full.rs:19031-19034) → a component made by "Convert to component" comes back as a div of its default texts. |
| 4 | Export > Project as JSON | debugger.html:35, debugger.js:1501-1527 | WIP | client only; exports the live hierarchy (read-only metadata) but not the builder document, not the component libraries' definitions. |
| 5 | Export > E2E Tests | debugger.html:40, debugger.js:1529-1535 | WORKS | |
| 6 | Export > Component Library (JSON) | debugger.html:45, debugger.js:1574-1592 → `export_component_library` full.rs:19088-19160 | BROKEN for converted components / WIP | only works when a library is selected in the Components view (else a warn log); built from the registry's `ComponentInfo` (full.rs:539-560), which has no template → the template (`render_fn_source`) is lost (B1 §9 flagged this). |
| 7 | Export > Code > Rust/C/C++/Python | debugger.html:50-72, debugger.js:1594-1671 → `export_code_zip` full.rs:18903-18988 | **BROKEN** | the page `fetch`es `export_code_zip` and looks for a binary `application/zip` body (debugger.js:1608); the server answers JSON `{download_url: data:…zip, filename, …}` (full.rs:18964-18974). The page parses it as `res`, `res.status === 'ok'` skips the fallback, then reads `data.files` (debugger.js:1638) which does not exist → logs "No files generated for rust" (1653). **The user never gets the zip.** |
| 7a | …what the zip contains | full.rs:18903-18988 | BROKEN (does not build) | (1) the live page app `build_live_page_code` (full.rs:12578-12607); (2) `build_exported_code`'s scaffold (full.rs:12615-12720) — its `src/main.rs` / `main.c` / `main.cpp` / `main.py` COLLIDE with the live page's and are dropped by the dedupe (18935-18945), its `Cargo.toml` stays: `azul = "0.0.1"` (full.rs:13269) = an ancient crates.io crate without these APIs → `cargo build` fails; (3) `css/<name>.css` per component, referenced by nothing; (4) WARNINGS.txt. No build file for C / C++ / Python, no README. `library` filter ignored (`library: _lib_filter`, 18905). |
| 7b | …the live page it exports | full.rs:12578-12607, core/src/styled_dom.rs:2680-2785 | BROKEN (styling lost, junk classes) | the live StyledDom is serialised with `get_html_string` → each node's computed style lands in a `style="…"` attribute (core/src/dom.rs debug_print_start), and **no page walker reads the `style` attribute** (core/src/xml.rs `compile_node_to_rust_code_inner` 7707-7879, `compile_node_fluent` 8596-8709, `compile_node_c` 8882-9003 match only `<style>` rules) → exported apps are unstyled; the only rule is get_html_string's `* {margin:0; padding:0}`, which every node then inlines. With the builder document active the live DOM carries B1's `azb-<uid>` marker classes → `.with_class("azb-3")` everywhere. |
| 8 | `export_code` op (fallback, curl, tests/e2e/test_export_code.sh) | full.rs:18873-18901 | WORKS for the page (Rust: after the test script REPLACES Cargo.toml) | same page generator as 7b; merges scaffold files with `or_insert` → the scaffold's broken Cargo.toml ships. |
| 9 | Component-library scaffolds | full.rs:13243-13720 `generate_{rust,c,cpp,python}_scaffold` | BROKEN (never compiled) | Rust: `extern "C" fn layout(data: &mut RefAny, _info: &mut LayoutCallbackInfo)` (13394) and callback stubs `(data: &mut RefAny, info: &mut CallbackInfo)` (13342) — today's callbacks take `RefAny` / `LayoutCallbackInfo` BY VALUE (examples/rust/src/hello-world.rs:7); `NodeType` used by `user_defined_compile_fn` but not imported. C: `AzDom layout(AzRefAny* data, AzLayoutCallbackInfo* info)` (13498) vs `AzLayoutCallbackType` by value (examples/c/hello-world.c:24) — clang ≥16 errors (incompatible-function-pointer-types); render fns wrap the compile_fn output in `return {code};` (13447) but `user_defined_compile_fn`'s C arm is a statement BLOCK (`AzDom root = …; … return root;`, core/src/xml.rs:3245-3290) → `return /* Component */ AzDom root = …` does not parse. C++: same `return {code};` (13558) around a block (xml.rs:3292-3322), `Dom layout(RefAny& data, LayoutCallbackInfo& info)` (13593) is not an `AzLayoutCallbackType`. Python: `App.create(None, …)` (13702). |
| 10 | Components view: "Edit compile_fn ▾" | debugger.js:2420-2434, 2963-3011 → `get_component_source {compile_fn}` full.rs:19658-19727, `update_component_compile_fn` full.rs:19787-19830 | PLACEHOLDER | GET runs the component's `compile_fn` for the language — for a converted (template) component that is `user_defined_compile_fn`, which prints the default texts, not the template. SAVE stores the text in the ONE `compile_fn_source` slot for every language (the `language` field is ignored) and nothing ever reads it back. |
| 11 | Components view: "Edit render_fn" | `update_component_render_fn` full.rs:19729-19785 | WORKS for templates (B1), PLACEHOLDER otherwise | non-template source is stored, never run. |
| 12 | `POST /debug/compile?lang=` (CSS → project zip, curl only) | dll/.../debug_server/platform.rs:366-389, 459-548 | PLACEHOLDER | not reachable from the UI. Rust backend (css/src/codegen/rust.rs:26-68) emits `const CSS: Css = Css { rules: [ … ] }` — an array literal where `Css.rules` is a `CssRuleBlockVec` (css/src/css.rs:30-39) → does not compile; C++ / Python backends are stubs returning `// TODO` (css/src/codegen/cpp.rs:23-25, python.rs:24-26). B2 is rewriting these. |
| 13 | Page DOM codegen `str_to_{rust,c,cpp,python}_code` | core/src/xml.rs:5510-5597, 8807-8866, 9005-9088 | WORKS (API-checked) except: | every emitted constructor was checked against today's generated bindings (target/codegen azul.h / azul20.hpp / dll_api_external.rs / python_api.rs): all 86 per-tag creators and all 86 semantic (a11y tier A-D) creators exist in all four bindings. Defects: (a) `style` attribute ignored (7b); (b) C `main` builds its RefAny with a NULL destructor (xml.rs:9083-9084) → `RefCount::drop` calls it unconditionally (core/src/refany.rs:251-272) → **the exported C app segfaults when it exits**; (c) C++ `render` never adopts the owned `AzRefAny` (leak per frame; examples/cpp/cpp20/hello-world.cpp:17 adopts it). |
| 14 | `builtin_compile_fn` | core/src/xml.rs:2927-2965 | WIP | C arm returns ONLY a text node for an element with text (the element is dropped); C++ / Python arms ignore the text. Only reachable via `get_component_source` (builtins are never exported). |
| 15 | `user_defined_compile_fn` | core/src/xml.rs:3180-3363 | WIP | prints the data model's default values as flat text children of a div (see 9 for the C / C++ block-vs-expression bug). This is what a converted component "compiles" to today. |
| 16 | "Convert HTML tree to component" | Document: `builder_convert_to_component` (B1, builder.rs:832-935); Live DOM: `create_component {render_tree}` (B1) | WORKS for the preview; BROKEN for export | the template lives in `render_fn_source`; `compile_fn` stays `user_defined_compile_fn` (builder.rs:1785-1788) → every code export of a converted component (7, 9, 10) loses its structure. |
| 17 | Quick "CSS classes → language", "subtree → code", "component → code" | — | MISSING | the only CSS → code path is the curl-only zip (12); the only DOM → code path is the whole-page zip (7). |

### What the zip looks like today (Export > Code > Rust, empty user libraries)
```
src/main.rs     live page (compiles once Cargo.toml is fixed; unstyled; azb-* classes)
Cargo.toml      azul = "0.0.1"  → does not build
WARNINGS.txt    "No user-defined component libraries to export. Generated minimal scaffold."
```
…and the browser never downloads it (row 7).

## 2. Plan
1. core: an IR-based fragment compiler (`compile_xml_fragment`, `compile_xml_fragment_app`) —
   subtree / template → a render FUNCTION per target, `{param}` placeholders → string
   parameters, matched rules + the `style` attribute → `with_css`; page walkers read `style`;
   C main gets a real destructor; C++ render adopts its RefAny.
2. layout `e2e/export.rs`: languages, CSS rule listing + CSS compile (existing
   `CodegenBackend` trait, looked up by name), subtree → code, component → code
   (render fn + registration for Rust and C), template `compile_fn`, a zip that builds.
3. Server ops (DebugEvent): `get_codegen_languages`, `get_css_rules`, `compile_css`,
   `export_subtree_code`, `export_component_code`; `export_code(_zip)` use the builder document
   when it is active; component library JSON carries the template.
4. UI `debugger-export.js`: three transient dialogs, fixed Export > Code download.

## DONE
- `f861cf64c` docs(b3): audit
- `0d6318f46` test(xml): page exporters keep styles / free data / Python link; fragment goldens (RED)
- `95afd6be5` fix(xml): `compile_xml_fragment(_app)` + page exporter fixes
- `2086d25fa` test(builder): quick exports + Export > Code scenarios (RED, `export_tests`)
- `b9dd44cff` fix(builder): export.rs + 5 ops + project zip + template compile_fn / JSON
- `c7df70a1d` test(debugger-ui): export dialogs node test + headless smoke (RED)
- `92c92aa6d` feat(debugger): debugger-export.js dialogs + Export > Code download
  (node 10/10, smoke 29/29; B1's dnd 13/13 + 25/25 still green)

## COURSE CHANGE (coordinator, 2026-09-29): no second code generator
B2 is integrated on `fix/input-bugs-2026-09-19` @ 095df1ffb (one construction IR
`css/src/codegen/ir.rs` + 35 printers `css/src/codegen/lang/*`, `ExprSyntax`, blockers /
limitations, `all()` / `backend_for` / `supported_languages()`, goldens in
`css/tests/codegen_goldens/<lang>/`, `AZ_BLESS=1`). My `core/src/xml_fragment_codegen.rs`
(own IR + 4 printers) must go. Steps:
1. merge `fix/input-bugs-2026-09-19` (merge, not rebase).
2. extend B2's IR: `Expr::Method {recv, class, method, args}`, `Expr::Param(name)`,
   `Expr::Concat(Vec<Expr>)`, typed `Item` params + a `Dom` return class; every printer spells
   them per `doc/src/codegen/v2/lang_*` (or a `limitation`).
3. `xml_fragment_codegen.rs` becomes a LOWERING (XmlNode / template → `ir::Module`), printed
   by `azul_css::codegen::lang`; delete its printers.
4. app / project scaffolding via B2's `emit_project_files`; page exporters + old walkers: move
   or list with file:line what remains.
5. export.rs: languages = `all_backends()` (ONE list), CSS via `backend_for`, platform.rs
   "Supported: rust, cpp, python" → `supported_languages()`.
6. DOM goldens through B2's harness pattern; hand-write 3-4 languages as RED anchor, rest "to
   bless".
UI (`debugger-export.js`) stays as is.

- `9d5bf7a36` merge of `fix/input-bugs-2026-09-19` @ 095df1ffb (B2) - clean
- `970013edc` test(css): DOM goldens in B2's harness (RED; rust/c/cpp/python dom_card by hand)
- `18d0b219d` feat(css): Expr::Method/Param/Concat, ItemParam, AppSpec, LibrarySpec,
  Doc::Chain, ExprSyntax::method/param/concat/dom_limitation; rust/c/cpp/python implement
  them (+ apps, registration); every other printer reports the limitation.

- `12ff91c22` docs(b3): progress
- `eb6573974` test(xml): the DOM fragment lowers to B2's codegen IR (RED)
- `a79391ca9` refactor(xml): `xml_fragment_codegen.rs` is a LOWERING only
  (`lower_xml_fragment` / `_app` / `lower_xml_page_app` → `ir::Module`); its own IR and four
  printers are deleted.
- `6518363b8` test(builder): export dialogs use the one language list + B2's printers (RED:
  export_tests, codegen_structure `exports_dom`, node test, smoke mock)
- `6fa397279` refactor(builder): export.rs is assembly only (lower + `emit_module` /
  `emit_project_files`); `CodegenBackend::exports_dom()`; `get_codegen_languages` =
  `all_backends()`; CSS via `backend_for` in all 35 languages; the live-page Export > Code
  goes through `lower_xml_page_app` (no `str_to_*_code` caller left in production);
  platform.rs lists `supported_languages()`; UI reads the one list (non-DOM languages
  disabled in the DOM dialogs) and pages through an app's files.
  node 13/13, export smoke 32/32, B1 dnd smoke 25/25.

- `56dcb9187` docs(b3): progress
- `096ba79b7` test(css): two exported C / C++ headers can be included together (RED; clang
  confirmed `redefinition of az_concat` without a guard)
- `ad9ce1b24` fix(css): C / C++ DOM helpers behind `AZ_CODEGEN_{CONCAT,REGISTRATION,STRING}`
- report `scripts/B3_BUILDER_EXPORT_2026_09_29.md` (audit before/after, commits, API changes,
  remaining old walkers with file:line, least-sure spots, commands).

## IN PROGRESS
- nothing (B3 done, pending the parent's build).

## NEXT
- (parent) build + run: see the report's command list; bless the 31 "to bless" DOM goldens.
- DOM export for more printers (C#, Java, ...): implement `ExprSyntax::method/param/concat`,
  clear `dom_limitation`, set `exports_dom() = true` (codegen_structure checks the pair).
- delete the old page walkers (listed in the report with file:line) once their tests are
  ported to the lowering.

## Open questions / notes for the parent
- target/codegen/azul.h (06:00 today, main checkout) does not compile as C on its own:
  `AzString_fromConstStr` is both a macro (122644) and an inline fn (122786), `AzString_tr`
  static-vs-extern (122799 vs 62135). Every C file fails on those 4 header errors; the goldens
  have no errors of their own (`clang -fsyntax-only -ferror-limit=0`, filtered to the file).
- This Mac's clang++ has no libc++ headers (`<cstdint>` not found): C++ goldens NOT checked.
