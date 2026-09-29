# B6: HTML → DOM (code) dialog + core `codegen` (progress)

Branch `wt/b6-html-to-dom-dialog` from `81d31d94e` (B3 + X1a/X1b). Nothing compiled (house rule).

## Scope (original task + three coordinator extensions, 2026-09-29)
- 1a. azul-core `codegen` feature (off by default → `azul-css/codegen`). The DOM lowering moves to
  `core/src/codegen/dom.rs`; the old page walkers `str_to_{rust,c,cpp,python}_code` + helpers are
  deleted, their tests ported or dropped. layout `codegen` feature forwards; e2e-server / dll
  debug-server enable it.
- 1b. Component-aware lowering: a component instance is a CALL to its render function (typed
  params, slots as `Dom` params), each component used is lowered ONCE into its own item. The
  per-language string hook `compile_fn` / `CompileTarget` is replaced by a language-neutral
  `ComponentDef::codegen: ComponentCodegen` (repr(C) description → IR in `core::codegen`).
  IR gains `Expr::ItemCall`; every DOM printer prints it (4 reference printers by me, the rest by
  helper agents).
- 1c. ONE extensible XML-attribute ↔ NodeData table used by the XML parser AND the codegen.
- 2. `core::codegen::project`: the zip API (component libraries + stylesheet + app markup →
  project files); layout `export.rs` / `full.rs` become thin callers.
- 3. The "HTML → DOM (code)" dialog (op `html_to_code`, debugger-export.js, docs).

## Decisions
- `compile_fn`, `compile_fn_source`, `ComponentCompileFn`, `CompileTarget` are REMOVED (no string
  escape hatch). `ComponentCodegen { RenderFunction, Element, Widget(ComponentWidgetCodegen) }`,
  variant 0 = RenderFunction (a zero-initialised C struct is a render-function component).
- A page / subtree module holds the page item plus one item per component it uses (each once);
  a project additionally writes each library file with its registration.
- `backend()` twins (export.rs + xml_fragment_codegen.rs) → one `core::codegen::backend`.
- layout's `CodeFile` (twin of `GeneratedFile`) and `CodeExport` move to core.
- Parse positions: `parse_xml_string` reports rows/cols of the text as given (xmlparser
  `from_fragment(full, range)`), not of the trimmed remainder.

## DONE
- 1a RED 747bd0b59 (core tests at the new paths; dll walker tests deleted; CI step moved)
- 1a impl 6f6ee890c (core/src/codegen/{mod,dom,dom_test}.rs; walkers deleted; features)

## IN PROGRESS
- 1b design: IR `Expr::ItemCall`, typed `ItemParam`, `ComponentCodegen` replacing `compile_fn`

## NEXT
- 1b RED + impl (+ helpers for 26 printers), 1c inventory (scratchpad b6_attr_inventory.md) → table,
  2, 3, docs, report

## Open questions
- none yet
