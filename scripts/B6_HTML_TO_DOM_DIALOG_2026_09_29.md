# B6: "HTML → DOM (code)" and a component-aware code generator in azul-core (report, 2026-09-29)

Branch `wt/b6-html-to-dom-dialog`, worktree `.claude/worktrees/agent-a066e68c13d0741fc`, cut from
`81d31d94e` (B3 + X1a/X1b). **Nothing here has been compiled** (house rule). What did run:

- `node dll/src/desktop/shell2/common/debugger/debugger-export.test.js`: 14 passed
- `node scripts/debugger-ui/builder-export-smoke.mjs` (headless Chrome, real page, mocked
  server): 42 passed
- `rustfmt --check` as a parse check on all 62 Rust files the branch adds or changes: 0 parse
  errors
- `clang -fsyntax-only` / `clang++ -std=c++17 -fsyntax-only` on the C and C++ `dom_components`
  goldens (against the generated `azul.h` / `azul17.hpp` of the main checkout), and
  `ast.parse` on the Python golden

The user asked for four things, which were done in this order:

1. "the azbuilder should have a „html to dom (code)" dialog"
2. "codegen will need to have that, but that needs to go in core behind a feature flag, similar
   to the css gen. Then the final „zip" API combines these two (i.e. building on the component
   API we already have)"
3. "codegen for Dom is more difficult since it needs to be aware of Component boundaries,
   currently we have this „compile to rust code" API which needs to be more generic."
4. "what is important is that NodeData stuff can be set via special XML attributes, needs to be
   extensible"

## 1. The result in one paragraph

There is still one code generator. azul-css (`codegen`) owns the language-neutral IR and the 35
printers. azul-core (`codegen`, new, off by default) owns lowering markup to that IR, now
**component-aware**: a `<library:name ..>` instance is a *call* of that component's function,
and each component used is defined *once* as its own item (callees first). Its builder
arguments are the instance's attributes. The per-language string hook `compile_fn` /
`CompileTarget` is gone. It is replaced by a language-neutral `ComponentDef::codegen:
ComponentCodegen` that every printer prints. **One extensible table** maps an XML attribute to
what it sets on a node, and both XML → DOM loaders and the code generator read it.
`azul_core::codegen::project` is the "zip" API: app + component libraries + named styles +
README, per language. layout's `export.rs` is now a thin JSON caller. On top of all this sits
the dialog: **Export > HTML → DOM (code)…**.

```
pasted HTML / builder document / component template / live page
        │  crate::xml::parse_xml_string  (attributes through azul_core::xml::attributes)
        ▼
XmlNodeChild ──azul_core::codegen::dom (component-aware)──▶ azul_css::codegen::ir::Module
                                                                  │  Expr::ItemCall = a component call
               azul_core::codegen::project ───────────────────────┤
                 fragment_code / html_code / library_code         │
                 project_files (app + components/ + styles + README)
               azul_css::codegen::backend_for(lang): emit_module / emit_project_files / emit_css
```

## 2. What was built

### 2a. azul-core `codegen` (step 1a)
- `core/src/codegen/mod.rs`: `backend(language)` (the one lookup, which answers the list on an
  unknown language), `render_fn_name`, `dom_warning` (a non-DOM language's reason).
- `core/src/codegen/dom.rs`: the DOM lowering that moved out of `xml.rs`
  (`lower_xml_fragment`, `lower_xml_fragment_app`, `lower_xml_page_app`, the CSS matcher, the
  node-constructor analysis) plus its unit tests (`dom_test.rs`).
- Deleted, with no replacement needed: the four page walkers `str_to_{rust,c,cpp,python}_code`
  and their helpers, `dll/tests/xml_to_rust_compilation.rs` and its `[[test]]` (its cases now
  live in `core/tests/export_page_codegen.rs`), and the CI step that ran it (it now runs the
  core tests).

### 2b. Component boundaries (step 1b)
- IR: `Expr::ItemCall { item, params, args }` (`Expr::item_call(item, vec![("title", arg)])`).
  All 30 DOM printers print it in their own spelling: Swift labels, OCaml `~labels`, Smalltalk
  keyword selectors, Go through `WrapperDomSyntax::string_arg`, and so on. C / Zig / Odin
  refuse a *joined* string argument with a comment.
- `ComponentDef::codegen: ComponentCodegen` (core/src/xml.rs):
  - `RenderFunction`, variant 0: the component's own function `render_<name>(..)`
  - `Element`: a builtin HTML element
  - `Call(ComponentCallCodegen { class, constructor, args, setters, finish })`: a widget
    constructor in api.json vocabulary, e.g. `Button::create(label).dom()`

  Builtin elements are `Element`, `if` / `for` / `map` are `RenderFunction`, and builder
  template components are `RenderFunction`.
- `core::codegen::dom`: `Components { map, template }`, `ComponentMarkup`, `Registry`, which
  lowers in DFS order, callees first, each component once. It cuts a recursive use with a note
  and names clashes `render_<lib>_<name>`.
  - The API: `lower_components_fragment`, `lower_components_app`, `lower_component_library`,
    `component_params`, `default_text`, `styled_dom_markup`.
  - Parameters stay text. A slot is the instance's children appended to the call's result.
    Extra `class` / `id` / `style` on an instance style the returned Dom.
  - A component without a template is exported as its rendering, as a function without
    parameters, with a note.
- Removed with no escape hatch: `compile_fn`, `compile_fn_source`, `ComponentCompileFn`,
  `CompileTarget`, `ResultStringCompileError`, `builtin_compile_fn`, `user_defined_compile_fn`,
  `builtin_{if,for,map}_compile_fn`, and the op `update_component_compile_fn` with its
  Components-view editor. `get_component_source {source_type: compile_fn | code}` now answers
  the component as code, from the shared printers.

### 2c. One extensible XML attribute table (step 1c)
- `azul_core::xml::attributes` (core/src/xml_attributes.rs, always compiled). Each
  `XmlAttribute { name, scope, order, setting }` answers a `NodeSetting`, one of: `Ids`,
  `Classes`, `TabIndex`, `Editable`, `Attribute(AttributeType)`, `Direction`, `Style`, or
  `NotExported(why)`.
- The builtin entries are: id / class, focusable, contenteditable, autofocus, placeholder, the
  form controls' attributes (`type` … `capture`, `data-*`), tabindex, colspan, rowspan, dir,
  style, `data-l10n*` (NotExported) and `on*` (NotExported: a callback by name).
- **Parse half**: core's `apply_xml_node_attributes` and layout's streaming loader both go
  through `setting_of` / `node_settings` + `apply_settings`. The hand-written matches are
  gone. The two loaders used to disagree:
  - `contenteditable="false"` now walls the subtree off in core's loader too.
  - The streaming loader gained the form attributes, the cell spans and `dir`.
- **Codegen half** (`codegen` feature): each setting is printed as the builder call that sets
  it, and what a constructor already took is not repeated. `NotExported` becomes a note in the
  item's doc.
- **Extensible**: `register_xml_attribute(XmlAttribute)` (`std`). A registered entry is looked
  up before the builtin ones. Unknown attributes are still ignored.

### 2d. The "zip" API (step 2)
- `azul_core::codegen::project`:
  - `project_files(language, &ProjectSpec { title, app: AppMarkup, components, libraries })`
    writes:
    - the app, which calls the components
    - one file per library with its registration (Rust `src/components/<lib>.rs` + `mod.rs`,
      else `components/<lib>.<ext>`)
    - the app's stylesheet as named styles (Rust `src/styles.rs`, else `styles.<ext>`, via
      `emit_css`)
    - a README
  - Also `fragment_code`, `html_code`, `library_code`, `app_files`, `CodeExport`, `CodeMode`,
    `AppMarkup`.
- `layout/src/e2e/export.rs` only picks the builder's markup, templates and libraries and
  answers JSON. Deleted as twins of the core family: `CodeFile`, `SubtreeMode`, `main_file`,
  `app_files`, `project_files`, `readme`, `library_code`, `default_string`, `template_params`,
  `qualified`, `render_fn_name`, `library_module`, `builder_template_compile_fn` and
  `component_export_xml`.

### 2e. The dialog (step 3)
- Op `html_to_code {html, language, mode?: "function"|"app", function_name?, css?: bool}`
  (full.rs → `export::html_to_code` → `core::codegen::project::html_code`).
  - A document is its `<body>`, and every `<style>` block is its stylesheet.
  - `style=""` attributes and the rules become each node's `with_css`.
  - `<library:name>` is a call of the app's component.
  - `css: true` adds the styles file.
  - Markup that does not parse is an *answer*: no code, plus
    `errors: [{message, line, column}]`, 1-based, in the text as pasted.
  - A non-DOM language answers its reason in `warnings`.
  - OP_POLICY entry in doc/src/gene2e.rs: "codegen surface, not engine behaviour".
- `parse_xml_string` (layout/src/xml/mod.rs) now tokenizes the text after the `<?xml?>` /
  doctype it skips as a *range of the whole text*. Error rows and columns used to be counted
  from after the doctype.
- `debugger-export.js`: menu item **HTML → DOM (code)…** right after **Compile CSS to…**. The
  dialog has:
  - a paste area (the last paste is kept while the page lives)
  - "Export as": render function / runnable app
  - the function name (hidden for an app)
  - the ONE language list, with non-DOM languages disabled and their reason shown
  - "With its CSS as named styles"
  - the shared code panel: Copy, Download, and a file picker for a project

  It converts as you type (debounced). A parse error shows as `line L, column C: message`.
- Docs:
  - gui-builder.md: the quick exports and the dialog ops
  - components.md: `ComponentCodegen` replaces `compile_fn`, with a real lowering example
  - dom.md: "Node Attributes in XML" and `register_xml_attribute`

## 3. Commits (oldest first)

| Commit | What |
|---|---|
| 747bd0b59 | test(core): DOM lowering in azul_core::codegen behind `codegen` (RED) |
| 6f6ee890c | refactor(core): azul_core::codegen behind `codegen`; page walkers gone |
| baab37ca0 | docs(b6): progress checkpoint |
| 518387c7b | test(codegen): a component instance is a call of its own function (RED) |
| 85833bc7c | feat(css): `Expr::ItemCall` (rust/c/cpp/python, C-family registration) |
| 765e0deb2 | test(builder): the code export calls a converted component (RED) |
| 75ce2e612 | feat(core): component-aware lowering; `ComponentDef::codegen` replaces `compile_fn` |
| 6fbd708fe | refactor(builder): export keeps component boundaries; `compile_fn` gone |
| a665219a8 | feat(css): the other 26 DOM printers call a component's function (helpers) |
| 04b7bccc5 | refactor(css): an item call carries the callee's parameter names; shared hooks |
| e8cf150fe | docs(b6): progress checkpoint |
| 6bb2777da | refactor(css): the 26 printers take parameter names from the IR (helpers) |
| fe4e81062 | test(core): one attribute table, both ways (RED) |
| 8a363bf88 | feat(core): one extensible XML attribute table for loaders and codegen |
| 901e5419f | test(core): one project API combines DOM and CSS generators (RED) |
| 25414b258 | feat(core): `azul_core::codegen::project`; the builder calls it |
| bd5484150 | test(builder): "HTML → DOM (code)" (RED: Rust, node, smoke) |
| b0c8d9296 | feat(builder): "HTML → DOM (code)": op, parse positions, dialog |
| 27b5778be | docs(guide): quick exports, component-aware codegen, the attribute table |
| 94d65214d | test(codegen): the missing `dom_components` goldens (cpp + 5 non-DOM printers) |

## 4. Crate ownership and feature edges

| Crate | Owns |
|---|---|
| azul-css `codegen` | IR (`ir.rs`, `Expr::ItemCall`), 35 printers, `emit_css`, `lower_types.rs` (generated from api.json), the goldens |
| azul-core (always) | `ComponentCodegen`, `ComponentCallCodegen`, `ComponentDef::codegen` (xml.rs); `xml::attributes` parse half (`register_xml_attribute` needs `std`) |
| azul-core `codegen` | `core::codegen::{backend, render_fn_name, dom_warning}`, `codegen::dom` (lowering), `codegen::project` (zip API); the attribute table's codegen half |
| azul-layout `codegen` | nothing of its own: it forwards. `e2e/export.rs` (JSON caller) and `e2e/full.rs` ops run under `e2e-server` |
| azul-dll | turns the feature on for the debug server; `debugger-export.js` / `debugger.js` |

Edges (before → after):

- `azul-core → azul-css`: features `["parser", "codegen"]` → `["parser"]`. Core no longer
  always pulls in the CSS code generator.
- **new** `azul-core/codegen = ["azul-css/codegen"]`
- **new** `azul-layout/codegen = ["azul-core/codegen", "azul-css/codegen"]`
- `azul-layout/e2e-server` gains `codegen`
- `azul-dll/debug-server` and `azul-dll/e2e-scripting` gain `azul-layout/codegen` (redundant
  with `e2e-server`; named so the gate is visible)
- `core/Cargo.toml`: five `[[test]]`s with `required-features = ["codegen"]`
  (`dom_fragment_codegen`, `export_page_codegen`, `codegen_components`, `codegen_attributes`,
  `codegen_project`). `xml_attributes` needs no feature.
- `dll/Cargo.toml`: `[[test]] xml_to_rust_compilation` removed. The CI step now runs
  `cargo test -p azul-core --features codegen --test export_page_codegen --test dom_fragment_codegen`.

## 5. Commands

```
# core without the feature: no codegen module; the five codegen test targets are SKIPPED
cargo check -p azul-core
cargo test  -p azul-core                                   # lib + xml_attributes + the rest
# core with it
cargo test  -p azul-core --features codegen                # + dom_test, codegen_components,
                                                           #   codegen_attributes, codegen_project,
                                                           #   dom_fragment_codegen, export_page_codegen
# printers and goldens (bless: AZ_BLESS=1 in front)
cargo test  -p azul-css --features codegen --test codegen_goldens --test codegen_structure
# the dispatcher (html_to_code, component calls, projects)
cargo test  -p azul-layout --features e2e-server --lib export_tests
cargo check -p azul-layout --features xml                  # the loader without codegen
cargo check -p azul-dll --features debug-server
# UI
node dll/src/desktop/shell2/common/debugger/debugger-export.test.js
node scripts/debugger-ui/builder-export-smoke.mjs
```

## 6. api.json: what to change (by autofix, not by hand)

Until this is regenerated, **the dll will not build**: the generated FFI still names
`azul_core::xml::{CompileTarget, ComponentCompileFn, ResultStringCompileError}` and
`ComponentDef.compile_fn`. `css/src/codegen/lower_types.rs` is generated from api.json and still
lists `CompileTarget` / `ComponentCompileFn`. That is harmless, and it goes away on regeneration.

1. **Remove** `component/classes/ComponentCompileFn` (the callback typedef),
   `component/classes/CompileTarget` and `error/classes/ResultStringCompileError`.
   `error/classes/CompileError` stays, because `lower_xml_page_app` still returns it.
2. **Change** `component/classes/ComponentDef.struct_fields`:
   - remove `compile_fn` and `compile_fn_source`
   - add `codegen: ComponentCodegen` between `render_fn` and `render_fn_source`
   - the new order is `id, display_name, description, css, source, data_model, render_fn,
     codegen, render_fn_source`

   Every new field has 8-byte alignment. The pre-existing `source: ComponentSource` (a
   `repr(C)` fieldless enum, 4 bytes) still sits before 8-byte fields with 4 bytes of padding.
   It was not moved, to keep the ABI change minimal. Moving it last would satisfy
   "decreasing alignment".
3. **Add** `component/classes/ComponentCodegen`:
   - `repr: "C, u8"`
   - derive `Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash`
   - `enum_fields: [{ "RenderFunction": {}, "Element": {}, "Call": { "type":
     "ComponentCallCodegen" } }]`, with variant 0 = RenderFunction, so a zeroed C struct is a
     render-function component
   - no `constructors`: the variant constructors are generated
   - the tag is u8, then 7 bytes of padding, then the 8-aligned payload
4. **Add** `component/classes/ComponentCallCodegen`:
   - `repr: "C"`, with the same derives as `ComponentCodegen`
   - `struct_fields` in order: `class: String`, `constructor: String`, `args: StringVec`,
     `setters: StringPairVec`, `finish: String`. All are 8-aligned, so there is no padding.
   - constructor `create`: `fn_args: [class: String, constructor: String, args: StringVec,
     finish: String]`,
     `fn_body: "azul_core::xml::ComponentCallCodegen::create(class, constructor, args, finish)"`
     (it sets `setters` empty)
5. **Proposal only, not added**: the attribute registry over FFI. Today
   `register_xml_attribute(XmlAttribute { name: &'static str, scope, order: u8, setting:
   fn(&str, &str) -> Option<NodeSetting> })` is Rust-only, and `NodeSetting` holds
   `Vec<AzString>`. For the bindings:
   - `NodeSetting` as `repr(C, u8)` with `Ids(StringVec)`, `Classes(StringVec)`,
     `TabIndex(TabIndex)`, `Editable`, `Attribute(AttributeType)`, `Direction(StyleDirection)`,
     `Style(String)`, `NotExported(String)`, plus `OptionNodeSetting`
   - `AttributeScope` as `repr(C)` `{AnyElement, FormControls}`
   - a callback `XmlAttributeSettingCallbackType = extern "C" fn(RefAny, String name, String
     value) -> OptionNodeSetting`, with `XmlAttributeSettingCallback { cb, ctx: OptionRefAny }`
     (CallbackType argument rules)
   - an entry `XmlAttributeEntry { name: String, callback: XmlAttributeSettingCallback, scope:
     AttributeScope, order: u8 }` (decreasing alignment), and a free function
     `xml::register_xml_attribute(entry)`

## 7. Least sure to compile

1. **core/src/codegen/dom.rs**: `Lower<'a, 'r>` holding `&mut Registry` while it walks
   `Components<'_>` (whose `template` is a `&dyn Fn(&ComponentDef) -> Option<Result<..>>`).
   This is the most intricate new borrow structure.
2. **The 26 helper-written printers** (a665219a8, 6bb2777da): they are parse-checked and
   reviewed, not type-checked. Their `item_call` signatures must match `ExprSyntax` /
   `WrapperDomSyntax` / `LinearSyntax` exactly.
3. **layout/src/xml/mod.rs streaming loader**: `&mut |s: &str| str_arena.intern(s)` passed
   as `&mut dyn FnMut(&str) -> AzString` from inside a closure that itself takes
   `str_arena: &mut StringArena`.
4. **layout/src/e2e/export.rs `xml_error_position`**: ref-pattern matches over `XmlError` /
   `XmlParseError` / `XmlStreamError`. I checked every variant and payload `pos` field by
   grep.
5. **The 30 generated bindings**: `ComponentCodegen` / `ComponentCallCodegen` spellings in
   each language are unverified until api.json is regenerated (§6).
6. Unused-import warnings are possible in `core/src/xml.rs` after the walkers left. They are
   not errors: the crates only `deny(unused_must_use)`.

## 8. Goldens to bless

Every `dom_components.*` except Rust / C / Python, and the `dom_library` files the registering
printers changed, were derived by hand from the printers:

- a665219a8 and 6bb2777da: 26 languages
- 94d65214d: cpp and the five non-DOM languages (perl, algol68, cobol, red, vb6). These were
  missing, and `codegen_goldens` would have failed with "golden file missing".

If a byte differs, bless after reviewing the output:
`AZ_BLESS=1 cargo test -p azul-css --features codegen --test codegen_goldens`.

## 9. What is left

- **Regenerate api.json** (§6). The dll does not build before that.
- **No widget library registers `ComponentCodegen::Call` yet.** The widget path is tested with
  a `Button` component built inline in `core/tests/codegen_components.rs`. Registering
  `widgets:*` components (Button, TextInput, …) with their constructor descriptions is a
  follow-up.
- **Skipped items**: a component item a language cannot print is skipped with its reason. Its
  callers still print the call, so the output is incomplete in that language rather than
  wrong.
- **Swift labels every argument.** Fine for generated functions, but not idiomatic for a
  single `text` parameter.
- Parameters are text only: typed data-model fields are passed as strings, the way templates
  substitute them.
- Stale remnants I left untouched:
  - `debugger.js`'s SourceEditor comment "Language tabs (for compile_fn)"
  - the historical `CompileTarget` measurement note in `doc/src/lint_derives.rs`
