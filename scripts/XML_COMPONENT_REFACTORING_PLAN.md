# XML Component System Refactoring Plan

## Goal

Remove the old `XmlComponentTrait`-based system entirely. The new system uses
`ComponentMap` + `ComponentDef` with fn pointers and is driven by JSON via the
debug server API.

**Key simplification:** No validation in the pipeline. JSON arrives via the API →
serde deserializes it into a `ComponentDataModel` → we pass that to `render_fn` →
`render_fn` returns `StyledDom` (or an error if anything is wrong) → we restyle
with the component's CSS. Component selection is by `"library:name"` (or builtin
shorthand). All validation happens implicitly through JSON parsing and inside the
`render_fn` itself.

---

## Architecture: How the new system works

**Two entry points, same core:**

```
  ┌─────────────────────────────────┐   ┌─────────────────────────────────┐
  │   Debug Server / API path       │   │   XML / DomXml path             │
  │                                 │   │   (reftests, hot-reload)        │
  │   JSON { "library:name", args } │   │   <a href="foo">click</a>      │
  │            │                    │   │            │                    │
  │            ▼                    │   │            ▼                    │
  │   ComponentMap.get("lib","name")│   │   ComponentMap.get_unqualified  │
  │         → &ComponentDef         │   │   ("a") → &ComponentDef        │
  │            │                    │   │            │                    │
  │            ▼                    │   │            ▼                    │
  │   serde: JSON → ComponentData  │   │   xml_attrs_to_data_model():    │
  │   Model (Deserialize)          │   │   clone def.data_model,         │
  │                                │   │   override href="foo",          │
  │                                │   │   text="click"                  │
  └────────────┬────────────────────┘   └────────────┬────────────────────┘
               │                                     │
               ▼                                     ▼
       ┌───────────────────────────────────────────────────┐
       │  (def.render_fn)(&def, &data_model)               │
       │     → Result<StyledDom, RenderDomError>            │
       │                                                    │
       │  restyle with def.css → final StyledDom            │
       └───────────────────────────────────────────────────┘
```

Both paths converge on the same `render_fn` / `compile_fn` call.
No validation layer — JSON parsing or `xml_attrs_to_data_model()` just
populates the `ComponentDataModel`, and the `render_fn` handles the rest.

---

## Current State

### New system (KEEP — already in `core/src/xml.rs`)

| Item | Lines | Description |
|------|-------|-------------|
| `ComponentDef` | ~2040 | `{ id, display_name, description, css, source, data_model, render_fn, compile_fn }` |
| `ComponentDataModel` | ~1139+ | `{ name, description, fields }` — with serde behind `serde-json` feature |
| `ComponentRenderFn` | ~1139+ | `fn(&ComponentDef, &ComponentDataModel) -> ResultStyledDomRenderDomError` |
| `ComponentCompileFn` | ~1139+ | `fn(&ComponentDef, &CompileTarget, &ComponentDataModel, usize) -> ResultStringCompileError` |
| `ComponentMap` | ~2411+ | `{ libraries: ComponentLibraryVec }` — lookup by library+name or unqualified |
| `tag_to_node_type()` | ~2174 | Maps tag string → `NodeType` enum |
| `builtin_render_fn` / `builtin_compile_fn` | ~2210 | Generic render/compile using `tag_to_node_type()` |
| `user_defined_render_fn` / `user_defined_compile_fn` | ~2287 | For JSON-imported components |
| `builtin_component_def()` | ~2343 | Creates a `ComponentDef` for a builtin HTML tag |
| `builtin_data_model()` | ~2411 | Returns tag-specific fields (href for `<a>`, src for `<img>`, etc.) |
| `register_builtin_components()` | ~2460 | Registers all 52 builtin HTML elements |

### Old system (REMOVE)

| Item | Lines | Description |
|------|-------|-------------|
| `XmlComponentTrait` | ~2547-2640 | Trait: `clone_box`, `get_type_id`, `get_xml_node`, `get_available_arguments`, `render_dom`, `compile_to_rust_code` |
| `XmlComponent` | ~2804-2833 | `{ id, renderer: Box<dyn XmlComponentTrait>, inherit_vars }` |
| `XmlComponentMap` | ~2837-3145 | `{ components: XmlComponentVec }` + `Default` impl with 52 old-style registrations |
| `html_component!` macro | ~3434-3500 | Macro + 42 invocations generating renderer structs |
| `DivRenderer` | ~3550 | Hand-written `XmlComponentTrait` impl |
| `BodyRenderer` | ~3596 | Hand-written `XmlComponentTrait` impl |
| `BrRenderer` | ~3642 | Hand-written `XmlComponentTrait` impl |
| `IconRenderer` | ~3697 | Hand-written `XmlComponentTrait` impl |
| `TextRenderer` | ~3760 | Hand-written `XmlComponentTrait` impl (for `<p>`) |
| `DynamicXmlComponent` | ~6022-6085 | Impl of `XmlComponentTrait` for user-defined XML components |
| `ComponentArguments` | ~1087 | Old `{ args: ComponentArgumentVec, accepts_text: bool }` |
| `FilteredComponentArguments` | ~1112 | Old `{ types, values, accepts_text }` |
| `validate_and_filter_component_args()` | ~3862 | Validation function — removing entirely |
| `validate_component_template_recursive()` | ~3994 | Recursive template validation |
| `validate_xml_node_recursive()` | ~4033 | Helper for above |
| `validate_attribute_value()` | ~3940 | Type checking attributes |
| `parse_component_arguments()` | ~3816 | Old `"a: String, b: bool"` parser |
| `get_node_type_for_component()` | ~4605 | Duplicate of `tag_to_node_type()` |

### Data structures (KEEP)

These are XML parser infrastructure, not component-specific:

- `XmlNode`, `XmlNodeChild`, `XmlNodeChildVec`
- `DomXml`
- `XmlTagName`, `XmlAttributeMap`
- `ComponentArgument` (used for format_args_dynamic in code generation)
- Error types: `DomXmlParseError`, `CompileError`, `RenderDomError`, `ComponentError`, `ComponentParseError`
- Helper functions: `normalize_casing`, `get_html_node`, `get_body_node`, `find_node_by_type`, `find_attribute`, `get_item`, `prepare_string`, `parse_bool`
- Dynamic string formatting: `split_dynamic_string`, `format_args_dynamic`, `DynamicItem`, `combine_and_replace_dynamic_items`
- CSS matching: `CssMatcher`, `get_css_blocks`, `group_matches`, `CssBlock`

---

## Pipeline Changes

### Old pipeline (XML-based, trait objects)

```
XML string → parse → XmlNode tree
  → str_to_dom(nodes, &mut XmlComponentMap) → StyledDom
  → str_to_rust_code(nodes, imports, &mut XmlComponentMap) → String

XmlComponentMap::default() registers 52 old-style renderers
Each node lookup: component_map.get(name) → &XmlComponent
  → renderer.get_available_arguments() → ComponentArguments
  → validate_and_filter_component_args(attrs, args) → FilteredComponentArguments
  → renderer.render_dom(map, filtered_args, text_content) → StyledDom
  → renderer.compile_to_rust_code(map, args, text) → String
```

### New pipeline (fn pointers, two entry points)

**Debug server / API path (primary):**
```
JSON arrives via API: { "component": "library:name", "args": { ... } }
  → ComponentMap.get("library", "name") → &ComponentDef
  → serde: JSON args → ComponentDataModel
  → (def.render_fn)(&def, &data_model) → Result<StyledDom, RenderDomError>
  → restyle with def.css → final StyledDom

No validation step — JSON parsing IS the validation.
render_fn returns error if data is wrong.
```

**XML / DomXml path (reftests, hot-reload, `str_to_dom`):**
```
XML string → parse → XmlNode tree
  → for each node: look up ComponentDef by tag name
  → flat-parse XML attributes into the component's default data model:
      e.g. <a href="foo" target="_blank">click</a>
        → ComponentMap.get_unqualified("a") → &ComponentDef (builtin:a)
        → clone def.data_model (has defaults: href="", target="", rel="")
        → override: href="foo", target="_blank", text="click"
        → (def.render_fn)(&def, &data_model) → StyledDom for this node
  → recursively render children, append as child DOMs
  → restyle with global CSS → final StyledDom

This is `xml_attrs_to_data_model()` — a simple helper that:
  1. Clones the ComponentDef's default data_model
  2. For each XML attribute, finds the matching field and sets its value
  3. If the node has text content, sets the "text" field
  4. Unknown attributes are ignored (no validation)
```

**XML compile path (`str_to_rust_code`):**
```
XML string → parse → XmlNode tree
  → for each node: look up ComponentDef by tag
  → flat-parse XML attrs into ComponentDataModel (same as above)
  → (def.compile_fn)(&def, &target, &data_model, indent) → String
```

---

## Step-by-Step Plan

### Phase 1: Add `xml_attrs_to_data_model()` + rewrite XML render pipeline

Both the render and compile XML paths need to flat-parse XML attributes into
a `ComponentDataModel`. This is a simple helper — no validation, just override
defaults with the values found in the XML attributes.

1. **Add `xml_attrs_to_data_model()`** in `core/src/xml.rs`:
   ```rust
   /// Flat-parse XML attributes into a ComponentDataModel.
   /// Clones the def's default data_model, overrides field values from
   /// XML attributes. Text content is set as the "text" field.
   /// Unknown attributes are silently ignored.
   fn xml_attrs_to_data_model(
       base_model: &ComponentDataModel,
       xml_attributes: &XmlAttributeMap,
       text_content: Option<&str>,
   ) -> ComponentDataModel
   ```
2. **Rewrite `xml_node_to_dom_fast()`**: Take `&ComponentMap` instead of
   `&XmlComponentMap`. For each node:
   - Look up `ComponentDef` by tag name via `component_map.get_unqualified(tag)`
   - Call `xml_attrs_to_data_model(&def.data_model, &node.attributes, text)`
   - Call `(def.render_fn)(&def, &data_model)` → get `StyledDom`
   - Recursively render children, append as child DOMs
3. **Remove `get_node_type_for_component()`** — duplicate of `tag_to_node_type()`
4. **Rewrite `render_dom_from_body_node()`**: Take `&ComponentMap` instead of
   `&XmlComponentMap`. Calls rewritten `xml_node_to_dom_fast()`.
5. **Remove `render_dom_from_body_node_inner()`** — replaced by the above.
6. **Rewrite `str_to_dom()`**: Change `&mut XmlComponentMap` → `&ComponentMap`.
   Skip dynamic `<component>` registration for now (can be added back later
   by inserting into a mutable ComponentMap clone).

### Phase 2: Rewrite XML compile pipeline

Same pattern — use `ComponentMap` + `xml_attrs_to_data_model()` + `compile_fn`.

1. **`str_to_rust_code()`**: Change `&mut XmlComponentMap` → `&ComponentMap`
2. **`compile_body_node_to_rust_code()`**: Change `&XmlComponentMap` → `&ComponentMap`,
   look up `ComponentDef` by tag, build `ComponentDataModel`, call `(def.compile_fn)()`
3. **`compile_node_to_rust_code_inner()`**: Same change
4. **`render_component_inner()`**: Change to use `&ComponentDef` / `&ComponentMap`
5. **`compile_components_to_rust_code()`**: Change `&XmlComponentMap` → `&ComponentMap`

### Phase 3: Delete old code (~1500 lines)

1. Remove `XmlComponentTrait` trait definition (~90 lines)
2. Remove `XmlComponent` struct + `impl_option!`/`impl_vec!` macros (~30 lines)
3. Remove `XmlComponentMap` struct + `Default` impl + methods (~310 lines)
4. Remove `html_component!` macro + all 42 invocations (~70 lines)
5. Remove hand-written renderers: `DivRenderer`, `BodyRenderer`, `BrRenderer`,
   `IconRenderer`, `TextRenderer` (~265 lines)
6. Remove `DynamicXmlComponent` struct + `XmlComponentTrait` impl (~85 lines)
7. Remove `validate_and_filter_component_args()` (~50 lines)
8. Remove `validate_component_template_recursive()` + `validate_xml_node_recursive()` (~50 lines)
9. Remove `validate_attribute_value()` (~40 lines)
10. Remove `parse_component_arguments()` (~30 lines)
11. Remove `get_node_type_for_component()` (duplicate of `tag_to_node_type()`) (~50 lines)
12. Remove `ComponentArguments` struct (~20 lines)
13. Remove `FilteredComponentArguments` struct (~20 lines)

### Phase 4: Update external consumers

All callers pass a `&ComponentMap` (built from `register_builtin_components()`).

| File | Change |
|------|--------|
| `layout/src/xml/mod.rs` | `domxml_from_str`: take `&ComponentMap` instead of `&mut XmlComponentMap` |
| `layout/src/extra.rs` | `styled_dom_from_str`: build `ComponentMap` from `register_builtin_components()` |
| `layout/src/extra.rs` | `styled_dom_from_parsed_xml`: same |
| `layout/src/desktop/extra.rs` | Same as above |
| `doc/src/reftest/mod.rs` | `XmlComponentMap::default()` → `ComponentMap::from_libraries(&vec![register_builtin_components()].into())` |
| `dll/tests/xml_to_rust_compilation.rs` | Same |
| `dll/tests/kitchen_sink_integration.rs` | Same |
| `tests/test_xml_inline_parsing.rs` | Same |

### Phase 5: Compile & fix

1. `cargo check -p azul-core`
2. `cargo check -p azul-layout`
3. `cargo check -p azul-dll`
4. Fix any remaining references

---

## What `ComponentArgument` / `ComponentArgumentVec` is still used for

The `ComponentArgument { name, arg_type }` type and `ComponentArgumentVec` are
used by the **dynamic string formatting** system (`format_args_dynamic`,
`compile_and_format_dynamic_items`, `set_stringified_attributes`).

These are used in the Rust code compilation path where variable references like
`{counter}` need to be resolved. They map variable names to type strings.

**Decision:** Keep `ComponentArgument` / `ComponentArgumentVec` for now as they're
part of the code generation infrastructure, but they are NOT part of the component
definition system anymore. The old `ComponentArguments` (plural, with `accepts_text`)
and `FilteredComponentArguments` wrappers are removed.

For `compile_node_to_rust_code_inner` and similar functions, we build a
`ComponentArgumentVec` on-the-fly from the `ComponentDataModel` fields when
needed for string interpolation.

---

## Summary of type changes

| Old | New |
|-----|-----|
| `XmlComponentMap` | `ComponentMap` (compile path) or removed (render path) |
| `XmlComponent` | `ComponentDef` (looked up from `ComponentMap`) |
| `XmlComponentTrait` | `render_fn` / `compile_fn` fn pointers on `ComponentDef` |
| `ComponentArguments` | `ComponentDataModel` (via serde from JSON) |
| `FilteredComponentArguments` | `ComponentDataModel` (via serde from JSON) |
| `validate_and_filter_component_args()` | Removed — JSON parsing IS the validation |
| `renderer.render_dom(map, args, text)` | `(def.render_fn)(&def, &data_model)` |
| `renderer.compile_to_rust_code(map, args, text)` | `(def.compile_fn)(&def, &target, &data_model, indent)` |
