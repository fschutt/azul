# B3: AzBuilder quick exports and DOM code generation (report, 2026-09-29)

Branch `wt/b3-builder-export`, worktree `.claude/worktrees/agent-a6baf308dfe230142`. It was cut
from `fix/input-bugs-2026-09-19` @ `1843e1edf` and has B2 merged in (`9d5bf7a36`, merged from
095df1ffb). **Nothing here has been compiled** (house rule). What did run: the node unit test,
the two headless-Chrome smokes, `clang -fsyntax-only` on the hand-written C goldens and
`python3 -m py_compile` on the Python golden.

## 1. The result in one paragraph

AzBuilder has three quick, transient dialogs: **Compile CSS to…**, **Subtree → code** and
**Component → code**. They send text and get text back, with no zip. Export > Code downloads a
project that builds. **There is one code generator.** Markup is *lowered* to B2's
language-neutral IR (`azul_core::xml::lower_xml_fragment` / `_app` / `lower_xml_page_app` →
`azul_css::codegen::ir::Module`) and *printed* by B2's per-language printers: `emit_module` for
functions and component libraries, `emit_project_files` for apps, and `emit_css` for
stylesheets. The dialogs get ONE language list, `all_backends()`. CSS compiles in all 35
languages. DOM export prints in Rust, C, C++ and Python. The other 31 printers answer with the
reason (a `limitation`) instead of a UI. The builder side
(`layout/src/e2e/export.rs`) only picks the markup, the stylesheet and the parameters, then
assembles the files.

```
builder document / component template / live page (get_html_string)
        │  builder::export_node_xml / component_export_xml
        ▼
XmlNodeChild ──lower_xml_fragment(_app) / lower_xml_page_app──▶ ir::Module {items, app, library}
                                                                  │
               azul_css::codegen::backend_for(lang) ◀─────────────┘
                 emit_module        → function / component library (+ registration)
                 emit_project_files → app project (build file, ui module, main)
                 emit_css           → named styles (Compile CSS to…)
```

## 2. Audit: every Export / Import / codegen entry point, before → after

"Before" line numbers are at `1843e1edf`. The full evidence is in
`scripts/B3_BUILDER_EXPORT.PROGRESS.md` §1.

| # | Entry point | Before | After |
|---|---|---|---|
| 1 | Import > Project from JSON (debugger.js) | works, client only; ignores htmlTree / componentRegistry | unchanged (out of scope, listed) |
| 2 | Import > E2E Tests | works | unchanged |
| 3 | Import > Component Library → `import_component_library` | BROKEN for converted components: no template, so it comes back as a div of defaults | the JSON carries `template`; import calls `builder::set_template` (b9dd44cff). The round trip is tested in `export_tests::a_converted_component_survives_a_library_export_and_an_import_as_a_template` |
| 4 | Export > Project as JSON | WIP (client only) | unchanged |
| 5 | Export > E2E Tests | works | unchanged |
| 6 | Export > Component Library (JSON) | loses the template | `ExportedComponentDef.template` (b9dd44cff) |
| 7 | Export > Code > Rust/C/C++/Python (debugger.js) | **the zip never downloads** (the page wants a binary body; the server answers JSON with a data URI) | `debugger-export.js` `exportCode` downloads the data URI (92c92aa6d) |
| 7a | …what the zip contains | `azul = "0.0.1"`, colliding scaffolds, no build files for C / C++ / Python | `export::project_files`: the printer's app project (Rust: Cargo.toml with the azul registry, .cargo/config.toml, src/ui.rs, src/main.rs; C: ui.h, main.c, Makefile; C++: ui.hpp, main.cpp, Makefile; Python: ui.py, main.py), one file per exportable component library (Rust `src/components/<lib>.rs` plus mod.rs plus `mod components;`; others `components/<lib>.<ext>`), README with build commands |
| 7b | …the live page it exports | `style` attribute ignored (unstyled), `azb-*` classes, C NULL destructor segfault, C++ leak | the live page goes through `lower_xml_page_app` (full.rs:12736). The `<style>` rules and the `style` attribute both become `with_css`. The C app reflects its data (AZ_REFLECT). With the builder active, the builder document (no `azb-*`) is exported instead |
| 8 | `export_code` op (curl, tests/e2e/test_export_code.sh) | the scaffold's broken Cargo.toml ships | same files as 7a, as a path→contents map |
| 9 | component scaffolds `generate_{rust,c,cpp,python}_scaffold` (full.rs) | never compiled (pre-by-value callbacks, `return {block};`) | **deleted** (b9dd44cff). The component libraries are printed by the printers' registration (`Module::library`) |
| 10 | Components view "Edit compile_fn" | a converted component shows its default texts | template components' `compile_fn` = `export::builder_template_compile_fn` = the shared printers. Builtins and non-template components still go through their own `compile_fn` (see §6) |
| 11 | Components view "Edit render_fn" | works for templates | unchanged |
| 12 | `POST /debug/compile?lang=` (platform.rs) | placeholder; "Supported: rust, cpp, python" | B2's `backend_for` / `emit_project` (B2). The unknown-language answer lists `supported_languages()` (6fa397279) |
| 13 | page walkers `str_to_{rust,c,cpp,python}_code` | style attr ignored, C segfault, C++ leak | fixed (95afd6be5), and then **no production caller is left** (§6) |
| 14/15 | `builtin_compile_fn` / `user_defined_compile_fn` | WIP | unchanged; only the Components view and the old walkers call them (§6) |
| 16 | "Convert HTML tree to component" | the export loses the structure | the template is lowered with `{placeholders}` as parameters. The Rust / C / C++ / Python output rebuilds the subtree and registers the component again (`export_tests::a_converted_component_exports_to_code_that_recreates_its_subtree`) |
| 17 | quick CSS → language, subtree → code, component → code | missing | the three dialogs plus 5 ops (`get_codegen_languages`, `get_css_rules`, `compile_css`, `export_subtree_code`, `export_component_code`) |

## 3. What works now (as far as tests that were not run can say)

- **Compile CSS to…**: the source is the selected node's style (the rules its last compound
  selector matches, plus its `style` attribute as a rule), the document stylesheet, a component's
  CSS, or pasted text. It lists the rules, lets you tick some of them, and compiles them with any
  of the 35 printers (`backend_for`, `emit_css`). The file name is `styles.<extension()>`.
- **Subtree → code**: a function (named `render_<id|class|tag>` or your own name), or a runnable
  app. For an app the answer is the whole project in `files`; the dialog shows the main file
  first and has a file picker (Copy / Download take the file on screen).
- **Component → code**: the render function (template placeholders become parameters with their
  defaults) plus the library registration (Rust, C and C++; Python prints a note because its
  bindings cannot build a native callback). A component without a template exports its default
  rendering and says so in a warning.
- **Export > Code**: a project that builds (7a above). One library that fails is left out with a
  warning instead of costing you the app.
- **UI**: the DOM dialogs show the same one list, with the 31 non-DOM languages disabled and
  labelled "(no DOM export yet)". The last language used is remembered per dialog kind.
- Tests that ran here: node `debugger-export.test.js` 13/13, `builder-export-smoke.mjs`
  32/32, and B1's `builder-dnd-smoke.mjs` 25/25.

## 4. Commits (first-parent, oldest first)

| hash | what |
|---|---|
| f861cf64c | docs(b3): audit |
| 0d6318f46 | test(xml): page exporters keep styles / free data / Python link (RED) |
| 95afd6be5 | fix(xml): fragment compiler + page walker fixes (the fragment compiler is superseded by a79391ca9) |
| 5a76913bc | docs(b3): progress |
| 2086d25fa | test(builder): quick exports + Export > Code scenarios (RED) |
| b9dd44cff | fix(builder): export.rs, 5 ops, project zip, template compile_fn / JSON; deleted the old scaffolds |
| e72a2ed94 | docs(b3): progress |
| c7df70a1d | test(debugger-ui): dialogs node test + headless smoke (RED) |
| 92c92aa6d | feat(debugger): debugger-export.js dialogs + Export > Code download |
| 13303a656 | docs(b3): progress |
| 9d5bf7a36 | merge of fix/input-bugs-2026-09-19 @ 095df1ffb (B2), clean |
| 970013edc | test(css): DOM goldens in B2's harness (RED; rust/c/cpp/python `dom_card` by hand) |
| 18d0b219d | feat(css): DOM construction in the IR (Method/Param/Concat, ItemParam, AppSpec, LibrarySpec), 4 printers implement it, 31 report the limitation |
| 12ff91c22 | docs(b3): progress |
| eb6573974 | test(xml): the DOM fragment lowers to B2's IR (RED) |
| a79391ca9 | refactor(xml): `xml_fragment_codegen.rs` is a lowering only; its IR and 4 printers are deleted |
| 6518363b8 | test(builder): one language list + B2's printers in the dialogs (RED) |
| 6fa397279 | refactor(builder): export.rs is assembly only; `exports_dom()`; one list; live page through the lowering; platform.rs `supported_languages()`; UI reads the one list |
| 56dcb9187 | docs(b3): progress |
| 096ba79b7 | test(css): two exported C / C++ headers can be included together (RED) |
| ad9ce1b24 | fix(css): the C / C++ DOM helpers are behind `AZ_CODEGEN_*` guards |
| (this) | docs(b3): report |

## 5. API changes

**azul-css (`codegen` feature)**
- `ir::Expr::{Method {recv, class, method, args}, Param(Ident), Concat(Vec<Expr>)}`, plus
  `Expr::method / param / concat / is_dom_node`.
- `ir::Item.params: Vec<ItemParam>`, `ItemParam {name, ty, default}` + `ItemParam::string`,
  `default_text()`.
- `ir::Module {items, app: Option<AppSpec>, library: Option<LibrarySpec>}` (derives `Default`),
  `is_dom()`, `item(&Ident)`. `AppSpec {title, root, is_body}`,
  `LibrarySpec {name, version, components}` and
  `ComponentSpec {item, name, display_name, description, data_model, data_model_description,
  field_descriptions}`.
- `doc::Doc::Chain` + `Doc::chained`, `is_multiline()`.
- `lang::ExprSyntax::{dom_limitation, method, param, concat}` (defaults: the limitation),
  `MethodLayout`, `ConcatPart`, `item_dom_blocker`, and `lang::dom` (C helpers, the C / C++
  registration).
- `CodegenBackend::exports_dom() -> bool` (default `false`).
- `lower_types.rs` regenerated with `Dom` and `SmallAriaInfo`
  (`css/tools/gen_codegen_lowering.py`, `DOM_EXPORT_TYPES`).

**azul-core**
- `xml::{lower_xml_fragment, lower_xml_fragment_app, lower_xml_page_app, FragmentParam,
  compile_xml_fragment (→ String), compile_xml_fragment_app (→ Vec<GeneratedFile>)}`.
  `compile_xml_fragment` now takes a language *name*, not a `CompileTarget`.
- Page walker fixes (95afd6be5): `node_inline_css`, the C AZ_REFLECT data, C++ `RefAny adopted`,
  the Python link.

**azul-layout (`e2e-server`)**
- `e2e::export`: `languages_json`, `backend`, `CssSource`, `resolve_css`, `css_rules_json`,
  `compile_css`, `SubtreeMode`, `subtree_code`, `library_module`, `component_code`,
  `library_code`, `builder_template_compile_fn`, `document_app`, `live_page_app`,
  `project_files`, `CodeExport {language, file_name, code, files, warnings}` and
  `CodeFile {path, contents}` (+ `From<GeneratedFile>`).
- Debug ops: `get_codegen_languages` → `{languages: [{id, label, ext, dom}]}`,
  `get_css_rules {source, css, node, library, name}`,
  `compile_css {language, source, css, node, library, name, rules}`,
  `export_subtree_code {node, language, mode, function_name}` and
  `export_component_code {library, name, language}`. The component library JSON carries
  `template`. `ComponentInfo.template`.
- `api.json`: untouched.

## 6. What still exists besides the shared path (file:line on this branch)

**Old page walkers.** They have NO production caller any more: `export_code(_zip)` of the live
page went through `str_to_*_code` until 6fa397279. Only tests call them. They are a second
implementation of the same export, kept (not deleted) until their tests are ported:

- `core/src/xml.rs:5510` `str_to_rust_code`, `:5599` `compile_components`,
  `:5640` `format_component_args`, `:5652` `compile_component`,
  `:6866` `set_stringified_attributes`, `:7472` `compile_body_node_to_rust_code`,
  `:7663` `compile_and_format_dynamic_items`, `:7713` `format_args_for_rust_code`,
  `:7725` `compile_node_to_rust_code_inner`.
- `core/src/xml.rs:8450-8489` `impl CtorArg { render_rust/_c/_cpp/_python }` and
  `:8491-8578` `impl NodeCtor { render_rust/_c/render_fluent }`. The *enums* `CtorArg` /
  `NodeCtor` (`:8021`, `:8035`) and `analyze_node_ctor` (`:8209`) are SHARED: the lowering uses
  them.
- `core/src/xml.rs:8580` `FluentSyntax`, `:8596` `CPP_SYNTAX`, `:8611` `PYTHON_SYNTAX`,
  `:8630` `compile_node_fluent`, `:8743` `compile_body_fluent`,
  `:8808` `parse_page_style_and_body`, `:8824` `body_matcher`, `:8837` `str_to_cpp_code`,
  `:8867` `str_to_python_code`, `:8913` `compile_node_c`, `:9034` `str_to_c_code`.
- `format.rs` uses by those walkers: `core/src/xml.rs:30` (`codegen::format::VecContents`),
  `:5527`, `:7475`, `:7534` (`codegen::format::GetHash`) and `:7729`. Unrelated old-path imports
  of `GetHash` (B2's note): `core/src/dom.rs:20`, `core/src/resources.rs:22`,
  `layout/src/solver3/layout_tree.rs:106`.
- Their tests: `core/src/xml_test.rs` (42 references), `dll/tests/xml_to_rust_compilation.rs`
  (18), `dll/tests/kitchen_sink_integration.rs:10,29`, `tests/src/xml.rs:3`, and
  `core/tests/export_page_codegen.rs` (my 95afd6be5 fixes; these now test dead code).
- **Shared with the lowering (they stay):** `head_style_text` `:5149`, `get_html_node` `:5182`,
  `get_body_node` `:5272`, `element_draws_nothing` `:6502`, `CssMatcher` `:7273`,
  `css_blocks_to_inline_string` `:7586`, `node_inline_css` `:7629`, `get_css_blocks` `:7643`,
  `safe_container_tag` `:7986`, `camel_to_snake` `:8066` and `analyze_node_ctor` `:8209`.

**`CompileTarget` arms (the `ComponentDef::compile_fn` ABI, in api.json; they stay):**
- `core/src/xml.rs:2927` `builtin_compile_fn`, `:3180` `user_defined_compile_fn`, `:4301`
  `builtin_if_compile_fn`, `:4387` `builtin_for_compile_fn` and `:4477`
  `builtin_map_compile_fn` each carry per-target string templates. **This is a second live
  implementation**, but only for the Components view's "compile_fn" display of builtins and
  non-template user components: `layout/src/e2e/full.rs:19049-19067` maps the language to a
  `CompileTarget` and calls `def.compile_fn`. Template components' `compile_fn` is
  `export::builder_template_compile_fn` (the shared printers). Every EXPORT of any component
  goes through the shared path; for a component without a template that means its default
  rendering, lowered.

## 7. DOM export per printer

- **Implemented** (`exports_dom() == true`): `rust`, `c`, `cpp`, `python`. They print functions
  with typed parameters, builder chains / nested C calls, string joins (Rust `format!`, C
  `az_concat`, C++ `std::string +`, Python f-string), apps (`emit_project_files` with
  `Module::app`) and registration (Rust / C / C++; Python prints why it cannot register).
- **Limitation** (they print the reason item by item; the export warns): `csharp`, `java`,
  `kotlin`, `go`, `swift`, `node`, `ruby`, `php`, `lua`, `zig`, `nim`, `d`, `ocaml`, `haskell`,
  `julia`, `pascal`, `ada`, `algol68`, `cobol`, `crystal`, `fortran`, `freebasic`, `lisp`,
  `odin`, `perl`, `powershell`, `racket`, `red`, `smalltalk`, `v`, `vb6`. Every node kind exists
  for all of them; implementing one means `ExprSyntax::method/param/concat`, `dom_limitation →
  None`, `exports_dom → true` and an app in `emit_project_files`.
  `codegen_structure::exports_dom_is_true_exactly_for_the_printers_that_build_the_dom` checks
  that the flag and the output agree.
- Goldens: `dom_card` was written by hand for rust / c / cpp / python (the RED anchor; the C
  golden passes `clang -fsyntax-only` except azul.h's own errors, and the Python golden parses).
  `dom_library.<ext>`, `dom_app/<files>` for all 35, and `dom_card` for the other 31 are **to
  bless** (missing files fail until blessed).

## 8. Commands (for the parent: build, run, bless)

```sh
# css: IR + printers
AZ_BLESS=1 cargo test -p azul-css --features codegen,parser --test codegen_goldens   # review the new files, then:
cargo test -p azul-css --features codegen,parser --test codegen_goldens
cargo test -p azul-css --features codegen,parser --test codegen_structure
# core: the lowering (+ the old walker fix tests)
cargo test -p azul-core --test dom_fragment_codegen
cargo test -p azul-core --test export_page_codegen
# layout: export.rs unit tests + scenarios through the real dispatcher
cargo test -p azul-layout --features e2e-server --lib e2e::export::tests
cargo test -p azul-layout --features e2e-server --lib export_tests
# the op table
cargo test -p azul-doc every_real_op_is_classified
# UI
node dll/src/desktop/shell2/common/debugger/debugger-export.test.js
node scripts/debugger-ui/builder-export-smoke.mjs
node scripts/debugger-ui/builder-dnd-smoke.mjs
# exported code against the real bindings
clang -fsyntax-only -I target/codegen css/tests/codegen_goldens/c/dom_card.h
clang++ -std=c++17 -fsyntax-only -I target/codegen css/tests/codegen_goldens/cpp/dom_card.hpp
python3 -m py_compile css/tests/codegen_goldens/python/dom_card.py
tests/e2e/test_export_code.sh    # live page → export_code → cargo check / clang / py_compile
```

## 9. Least-sure spots

1. **Nothing was compiled.** The riskiest spots are `layout/src/e2e/export.rs` (a rewrite: the
   `Box<dyn CodegenBackend>` derefs, `template_params`' `unzip` into a tuple of Vecs,
   `is_template.then_some(params.as_slice())`) and the full.rs `build_app_code` /
   `build_live_page_code` signature change (both now return `Vec<CodeFile>`).
2. **The exact printer output the scenario tests expect** (`export_tests.rs`). The expectations
   were derived by reading the printers, e.g. whether Rust keeps
   `Dom::create_a(azul::str::String::from("https://azul.rs"), azul::str::String::from("Docs"),
   SmallAriaInfo::label(..))` on one line, and C `AzString_copyFromBytes((const uint8_t*)"Hello",
   0, 5)`. If an assertion fails on spacing or line breaks, the golden (blessed) output is the
   truth.
3. **`builder_convert_to_component`'s field names and order** (`text`, `href`, `text_2`) are
   taken from builder.rs:1736 and its unit test. The Rust registration reads fields by
   `Ident::snake()` of the parameter name. A field name that is not snake-case-stable would read
   its fallback.
4. **The Rust registration** (`azul::component`, `azul::error`, `azul::option`, `azul::vec`,
   `azul::prelude::StyledDom`): every imported name exists in `target/codegen/reexports.rs`
   (checked by grep), and `StyledDom::create_from_dom` / `AzComponentId_create` exist. The
   bodies were never compiled.
5. **C header shape.** `register_<lib>_library` is a non-static function defined in a `.h`: two
   TUs that include the same library header would collide at link time. The helpers are guarded
   (ad9ce1b24), but the C headers have no `#pragma once` (adding it would churn all of B2's C
   CSS goldens).
6. **A non-DOM language's app** answers only `ui.<ext>`, the module that says why
   (`export::app_files`), not the printer's CSS harness project. This rule lives in the builder
   module, not in the printers.
7. **The live page export's title** is fixed to "Azul app" and the builder document's to
   "AzBuilder app".
8. **`CssSource::Node`** matches only the last compound selector (ancestors are not checked), so
   it can list rules that do not actually apply.

## 10. What is left

- The parent should build and run §8, then bless the DOM goldens for the 31 limitation printers
  and `dom_library` / `dom_app` for all 35.
- Port the old-walker tests (§6) to the lowering and delete `str_to_*_code` and its walkers
  (about 1.5k lines of `core/src/xml.rs` plus the `format.rs` imports).
- Decide whether the Components view's "compile_fn" for builtins / non-template components
  should show `export::component_code` (the shared printers, any of the 35 languages) instead of
  the `CompileTarget` templates. Its SAVE still stores one `compile_fn_source` for every language
  (audit row 10).
- DOM export for more printers: C# / Java / Kotlin / Swift / Go first, per
  `doc/src/codegen/v2/lang_*`.
- Import > Project from JSON still ignores the builder document and the component libraries
  (audit row 1).
- azul.h's 4 own C errors (`AzString_fromConstStr` macro vs inline fn, `AzString_tr`
  static vs extern) are recorded by the coordinator; left alone here.
