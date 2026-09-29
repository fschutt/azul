# B6: HTML → DOM (code) dialog + core `codegen` (progress)

Branch `wt/b6-html-to-dom-dialog` from `81d31d94e` (B3 + X1a/X1b). Nothing compiled (house rule).
Final report: scripts/B6_HTML_TO_DOM_DIALOG_2026_09_29.md.

## Scope (original task + three coordinator extensions, 2026-09-29)
- 1a. azul-core `codegen` feature (off by default → `azul-css/codegen`). The DOM lowering moves to
  `core/src/codegen/dom.rs`; the old page walkers `str_to_{rust,c,cpp,python}_code` + helpers are
  deleted, their tests ported or dropped. layout `codegen` feature forwards; e2e-server / dll
  debug-server enable it.
- 1b. Component-aware lowering: a component instance is a CALL to its render function, each
  component used is lowered ONCE into its own item. `compile_fn` / `CompileTarget` replaced by a
  language-neutral `ComponentDef::codegen: ComponentCodegen`. IR gains `Expr::ItemCall`; every
  DOM printer prints it.
- 1c. ONE extensible XML-attribute ↔ NodeData table used by the XML parsers AND the codegen.
- 2. `core::codegen::project`: the zip API; layout `export.rs` / `full.rs` are thin callers.
- 3. The "HTML → DOM (code)" dialog (op `html_to_code`, debugger-export.js, docs).

## Decisions
- `compile_fn`, `compile_fn_source`, `ComponentCompileFn`, `CompileTarget`,
  `ResultStringCompileError` are REMOVED (no string escape hatch).
  `ComponentCodegen { RenderFunction, Element, Call(ComponentCallCodegen) }`, variant 0 =
  RenderFunction (a zero-initialised C struct is a render-function component).
- A page / subtree module holds the page item plus one item per component it uses (each once);
  a project additionally writes each library file with its registration.
- One `core::codegen::backend`; layout's `CodeFile` / `CodeExport` / `SubtreeMode` moved to core.
- Parse positions: `parse_xml_string` reports rows/cols of the text as given (xmlparser
  `from_fragment(full, range)`), not of the trimmed remainder.
- Typed params stay text (templates substitute text); slots = children appended to the call.
- `html_to_code`: a parse error is an answer (`errors: [{message, line, column}]`), not a refusal.

## DONE
- 1a: 747bd0b59 (RED), 6f6ee890c
- 1b: 518387c7b, 765e0deb2 (RED); 85833bc7c (css), 75ce2e612 (core), 6fbd708fe (layout),
  a665219a8 + 04b7bccc5 + 6bb2777da (printers)
- 1c: fe4e81062 (RED), 8a363bf88
- 2: 901e5419f (RED), 25414b258
- 3: bd5484150 (RED), b0c8d9296 (node 14/14, smoke 42/42)
- docs 27b5778be (gui-builder.md, components.md, dom.md)
- goldens 94d65214d (cpp + perl/algol68/cobol/red/vb6 `dom_components`, were missing)
- report scripts/B6_HTML_TO_DOM_DIALOG_2026_09_29.md

## NEXT (outside B6)
- regenerate api.json (report §6) - the dll does not build before that
- bless the hand-derived goldens if a byte differs
- register a `widgets:*` component library with `ComponentCodegen::Call`
