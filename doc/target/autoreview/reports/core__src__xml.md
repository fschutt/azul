# Review: core/src/xml.rs

## Summary
- Lines: 6074
- Public functions: ~40
- Public structs/enums: ~60+
- Findings: 2 high, 4 medium, 2 low

## Findings

### [HIGH] Dead Code — Remaining public types with zero external references
- **Location**: Throughout the file
- **Details**: The following public types/aliases have zero external references outside `xml.rs` and generated `target/` code:
  - Type aliases: `XmlTextContent`
  - Enum: `XmlNodeType`
  - Box wrappers: `ComponentFieldValueBox`
  - Component model types: `ComponentEnumVariant`, `ComponentEnumModel`, `ComponentInstanceDefault`, `ComponentFieldOverride`, `ComponentFieldValueSource`, `ComponentFieldValue`, `ComponentFieldNamedValue`
  - Type aliases: `ComponentRenderFn`, `ComponentCompileFn`, `RegisterComponentFnType`, `RegisterComponentLibraryFnType`
- **Evidence**: Grep for each name across the codebase excluding `xml.rs` and `target/` returned zero results.
- **Note**: Many of these types are referenced in `api.json` for FFI bindings and cannot have their visibility reduced. `ComponentArgumentName`, `ComponentArgumentType`, `ComponentArgumentOrder`, `ComponentName`, `CompiledComponent`, `ComponentArguments`, and `DEFAULT_ARGS` were made private.

### [HIGH] Dead Code — Error types with zero external references
- **Location**: Lines 3810-3993
- **Details**: These error types have zero external references:
  - `DomXmlParseError` (line 3812)
  - `ComponentParseError` (line 3975)
  - `ComponentError` (line 3917)
  - `UselessFunctionArgumentError` (line 3909)
  - `MissingTypeError` (line 3951)
  - `WhiteSpaceInComponentNameError` (line 3959)
  - `WhiteSpaceInComponentTypeError` (line 3967)
- **Evidence**: Grep for each name outside `xml.rs` returned zero results.
- **Recommendation**: These are used internally by the parsing pipeline. Make them `pub(crate)` if they don't need to be in the public API.

### [HIGH] Duplicated Code — `xml_node_to_dom_fast` vs `xml_node_to_fast_dom`
- **Location**: `xml.rs:4607` and `xml.rs:4930`
- **Details**: These two functions contain near-identical attribute parsing logic (~80 lines): id/class extraction, focusable/tabindex handling, inline style parsing, SVG context detection. The first builds a `Dom` tree, the second uses `CompactDomBuilder` (arena). The missing SVG shapes in the fast path have been fixed.
- **Evidence**: Side-by-side comparison of lines 4612-4682 and 4938-4999 shows verbatim duplication.
- **Recommendation**: Extract shared attribute-parsing logic into a helper function like `apply_common_xml_attrs(xml_node) -> NodeData`.


### [MEDIUM] Duplicated Functions — `tag_to_node_type` and `tag_to_node_type_tag`
- **Location**: `xml.rs:2205` and `xml.rs:2389`
- **Details**: Two massive match statements (~180 arms each) that map the same tag strings to `NodeType` and `NodeTypeTag` respectively. They have identical structure but map to different enum types. Any new HTML/SVG tag must be added to both.
- **Evidence**: Lines 2205-2385 and 2389-2570 are structurally identical.
- **Recommendation**: Consider deriving one from the other, or using a macro to generate both from a single tag list.

### [MEDIUM] File Size — 6074 lines, multiple concerns
- **Location**: Entire file
- **Details**: The file mixes several distinct subsystems:
  1. XML parsing types and error hierarchy (lines 1-1066)
  2. Component type system with rich types, serde, data models (lines 1068-1962)
  3. Builtin component registration and rendering (lines 2199-3642)
  4. DOM construction from XML (lines 3648-5036)
  5. CSS matching and Rust code compilation (lines 5400-5984)
  6. String templating utilities (lines 5174-5399)
- **Recommendation**: Consider splitting into submodules: `xml/types.rs`, `xml/components.rs`, `xml/builtin.rs`, `xml/dom_builder.rs`, `xml/compiler.rs`. The file is cohesive enough to remain as-is, but the mixing of CSS compilation with XML parsing is a concern boundary violation.


### [MEDIUM] Refactoring — `user_defined_compile_fn` is 150+ LOC with 4 target-language branches
- **Location**: `xml.rs:2791-2949`
- **Details**: Four nearly identical code blocks for Rust/C/Cpp/Python, each iterating fields and generating similar code. The per-language differences are small (comment syntax, function names).
- **Recommendation**: Extract language-specific syntax into a small config struct and use a single loop.

### [LOW] Vibe-Coding — Remaining TODO markers
- **Location**: Various
- **Details**:
  - Line 3462: `// For now, render a placeholder` — stub acknowledgment
  - Line 3485-3488: `// TODO: iterate parsed JSON array` — in generated code strings
- **Evidence**: Direct grep results.
- **Recommendation**: Address or track the remaining TODOs. The SVG shape TODO (formerly at line 5010) has been fixed — all shapes now handled in the fast path.

### [MEDIUM] Code Style — Deep nesting in `scan_node`
- **Location**: `xml.rs:334-543`
- **Details**: `Xml::scan_node` is 210 lines with deep nesting inside `match tag_name.as_str()` arms, each containing `if let Some(src)` blocks. The function handles 10+ tag types.
- **Recommendation**: Extract per-tag scanning into small helper functions (e.g., `scan_img_node`, `scan_link_node`).

### [LOW] Magic Number — KAPPA constant
- **Location**: `xml.rs:4730`
- **Details**: `const KAPPA: f32 = 0.5522847498;` is defined inside the `"ellipse"` match arm. This is the standard Bezier approximation constant for circles/ellipses.
- **Evidence**: Line 4730.
- **Recommendation**: This is acceptable as a local const with a clear name, but could be moved to a shared SVG utilities module if used elsewhere.

### [LOW] `#[allow(non_camel_case_types)]` on `c_void`
- **Location**: `xml.rs:165-166`
- **Details**: `pub enum c_void {}` is a hand-rolled void type. This is commonly available from `core::ffi::c_void` since Rust 1.30+.
- **Evidence**: Line 165-166.
- **Recommendation**: Use `core::ffi::c_void` instead, or if this must be `#[repr(C)]` compatible, add a doc comment explaining why.

## System Documentation
- System identified: yes — XML/XHTML parsing and component system
- Existing doc: `doc/guide/widgets.md` covers widget concepts but not the XML parsing/component pipeline
- Doc needed: A guide document for the **XML component system** explaining:
  - How `.azul` XML files are parsed into DOM
  - The component registration system (`ComponentDef`, `ComponentLibrary`, `ComponentMap`)
  - The builtin component bridge and how new components are registered
  - The XML-to-Rust code compilation pipeline
  - How the debug server uses the component system for live editing
