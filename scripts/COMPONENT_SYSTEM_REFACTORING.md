# Component System Refactoring Analysis

## Overview

There are currently **two parallel component systems** in the codebase that serve overlapping purposes. The old XML-based system uses `dyn Trait` objects and `BTreeMap<String, …>`, making it inherently non-FFI-safe. The new `repr(C)` component system was designed to replace it but still depends on the old system's types in several critical paths.

---

## System 1: Old XML Component System

### Core types (all in `core/src/xml.rs`)

| Type | FFI-safe? | Notes |
|------|-----------|-------|
| `XmlComponentTrait` | **No** — `dyn Trait` | Trait with `render_dom()`, `compile_to_rust_code()`, `get_available_arguments()` |
| `XmlComponent` | **No** — `Box<dyn XmlComponentTrait>`, `String` | Wraps a trait object + id + inherit_vars |
| `XmlComponentMap` | **No** — `BTreeMap<String, XmlComponent>` | Registry of all XML components, keyed by normalized name |
| `ComponentArguments` | **No** — `Vec<(String, String)>`, no `repr(C)` | Arguments a component accepts (name→type pairs) |
| `FilteredComponentArguments` | **No** — `BTreeMap<String, String>` | Validated argument values after XML attribute filtering |
| `ComponentArgumentTypes` | **No** — `Vec<(String, String)>` type alias | The `(name, type)` pair list |
| `DynamicXmlComponent` | **No** — `String`, `ComponentArguments`, `XmlNode` | Component parsed from `<component>` XML tags at runtime |

### How it works

1. `XmlComponentMap::default()` registers **52 builtin HTML renderers** (DivRenderer, BodyRenderer, H1Renderer, etc.) — each is a struct implementing `XmlComponentTrait`.
2. `str_to_dom()` parses XML, discovers `<component>` tags in `<head>`, creates `DynamicXmlComponent` instances, registers them in `XmlComponentMap`.
3. `render_dom_from_body_node_inner()` walks the DOM tree, looks up each tag in `XmlComponentMap`, calls `renderer.render_dom()`.
4. `str_to_rust_code()` does the same but calls `compile_to_rust_code()` to generate Rust source.

### Where it's used

- `str_to_dom()` — XML→DOM rendering (the main codepath for `.azul` files)
- `str_to_rust_code()` — XML→Rust code generation
- `render_dom_from_body_node_inner()` — recursive DOM builder
- `compile_body_node_to_rust_code()` — recursive Rust code builder
- `validate_and_filter_component_args()` — attribute validation
- In the `html_component!` macro — generates 47 identical trait impls

### Concrete renderer structs (all identical pattern via `html_component!` macro)

```
HtmlRenderer, HeadRenderer, TitleRenderer, HeaderRenderer, FooterRenderer,
SectionRenderer, ArticleRenderer, AsideRenderer, NavRenderer, MainRenderer,
H1–H6Renderer, SpanRenderer, PreRenderer, CodeRenderer, BlockquoteRenderer,
UlRenderer, OlRenderer, LiRenderer, DlRenderer, DtRenderer, DdRenderer,
TableRenderer, TheadRenderer, TbodyRenderer, TfootRenderer, TrRenderer,
ThRenderer, TdRenderer, ARenderer, StrongRenderer, EmRenderer, BRenderer,
IRenderer, URenderer, SmallRenderer, MarkRenderer, SubRenderer, SupRenderer,
FormRenderer, LabelRenderer, ButtonRenderer, HrRenderer
```

Plus 3 manual impls: `DivRenderer`, `BodyRenderer`, `BrRenderer`, `IconRenderer`, `TextRenderer`.

---

## System 2: New `repr(C)` Component System

### Core types (all in `core/src/xml.rs`)

| Type | FFI-safe? | Notes |
|------|-----------|-------|
| `ComponentId` | **Yes** | `{ collection: AzString, name: AzString }` |
| `ComponentDef` | **Yes** | Full component definition with render/compile fn pointers |
| `ComponentDefVec` | **Yes** | `impl_vec!` wrapper |
| `ComponentLibrary` | **Yes** | Named collection of `ComponentDef`s |
| `ComponentLibraryVec` | **Yes** | `impl_vec!` wrapper |
| `ComponentMap` | **Yes** | `{ libraries: ComponentLibraryVec }` |
| `ComponentParam` | **Yes** | Parameter definition for GUI builder |
| `ComponentCallbackSlot` | **Yes** | Callback slot for wiring |
| `ComponentDataField` | **Yes** | Data model field |
| `ComponentDataModel` | **Yes** | Named struct def for code export |
| `ChildPolicy` | **Yes** | `enum { NoChildren, AnyChildren, TextOnly }` |
| `ComponentSource` | **Yes** | `enum { Builtin, Compiled, UserDefined }` |
| `CompileTarget` | **Yes** | `enum { Rust, C, Cpp, Python }` |
| `ComponentRenderFn` | **Partially** — fn pointer taking `&XmlComponentMap` and `&FilteredComponentArguments` | **This is the problem** |
| `ComponentCompileFn` | **Partially** — same issue | Same problem |
| `RegisterComponentFn` | **Yes** | `extern "C" fn() -> ComponentDef` |
| `RegisterComponentLibraryFn` | **Yes** | `extern "C" fn() -> ComponentLibrary` |

### How it works

1. `AppConfig::create()` calls `register_builtin_components()` which returns a `ComponentLibrary` with 52 `ComponentDef`s.
2. Users call `AppConfig::add_component()` or `AppConfig::add_component_library()` to register custom components.
3. `ComponentMap::from_libraries()` builds the lookup structure.
4. The debug server (`debug_server.rs`) uses `build_component_registry()` to serve the component list to the GUI builder.
5. Import/export of component libraries uses JSON (`ExportedLibraryResponse` ↔ `ComponentLibrary`).

### Where it's used

- `AppConfig` — stores `component_libraries: ComponentLibraryVec`
- Debug server — `build_component_registry()`, `build_exported_code()`
- Debug server events — `ImportComponentLibrary`, `ExportComponentLibrary`, `ExportCode`, `CreateComponentLibrary`, `DeleteComponentLibrary`
- `register_builtin_components()` — the 52 builtins, using `builtin_component_def()` helper

---

## The Problem: Cross-System Dependencies

The new system's `ComponentRenderFn` and `ComponentCompileFn` **depend on the old system's types**:

```rust
pub type ComponentRenderFn = fn(
    &ComponentDef,          // ← new system ✓
    &XmlComponentMap,       // ← OLD system ✗ (contains BTreeMap<String, XmlComponent>)
    &FilteredComponentArguments,  // ← OLD system ✗ (contains BTreeMap<String, String>)
    &OptionString,
) -> Result<StyledDom, RenderDomError>;

pub type ComponentCompileFn = fn(
    &ComponentDef,          // ← new system ✓
    &CompileTarget,         // ← new system ✓
    &XmlComponentMap,       // ← OLD system ✗
    &FilteredComponentArguments,  // ← OLD system ✗
    &OptionString,
    indent: usize,
) -> Result<String, CompileError>;
```

The `api.json` autofix detected these types and pulled them in, but they contain `BTreeMap` fields which are not FFI-safe. The previous "fix" just removed them from api.json and replaced callback args with `usize` — which is wrong because it hides the real problem.

### What the api.json currently has (incorrectly)

The callback typedefs in api.json have the `XmlComponentMap` and `FilteredComponentArguments` args replaced with opaque `usize` values. This means the generated C/C++/Python bindings would get meaningless integer arguments instead of typed pointers.

---

## What Should Be Refactored

### 1. Replace `FilteredComponentArguments` with an FFI-safe equivalent

Currently:
```rust
pub struct FilteredComponentArguments {
    pub types: Vec<(String, String)>,      // not FFI-safe
    pub values: BTreeMap<String, String>,   // not FFI-safe
    pub accepts_text: bool,
}
```

Proposed replacement:
```rust
#[repr(C)]
pub struct FilteredComponentArguments {
    pub types: StringPairVec,    // already exists and is FFI-safe
    pub values: StringPairVec,   // key-value pairs instead of BTreeMap
    pub accepts_text: bool,
}
```

`StringPairVec` (`Vec<AzStringPair>` where `AzStringPair = { key: AzString, value: AzString }`) is already used throughout the codebase for XML attributes. Lookups become O(n) instead of O(log n) but these maps are tiny (typically <20 entries).

**Impact**: ~15 call sites in xml.rs that do `.values.get("key")` or `.values.insert(k, v)` need to use a linear scan helper instead. The `validate_and_filter_component_args()` function needs updating.

### 2. Replace `ComponentArgumentTypes` type alias

Currently:
```rust
pub type ComponentArgumentTypes = Vec<(ComponentArgumentName, ComponentArgumentType)>;
```

This is `Vec<(String, String)>` which is not FFI-safe. Replace with `StringPairVec`.

**Impact**: ~20 call sites. The `ComponentArguments` struct (used only by old system) would also need updating or can be removed.

### 3. Replace `XmlComponentMap` in the render/compile fn signatures

The new `ComponentDef.render_fn` and `compile_fn` take `&XmlComponentMap` but they only need component lookup. Replace with `&ComponentMap`:

```rust
pub type ComponentRenderFn = fn(
    &ComponentDef,
    &ComponentMap,                    // ← NEW system, FFI-safe
    &FilteredComponentArguments,      // ← fixed to be FFI-safe (see #1)
    &OptionString,
) -> Result<StyledDom, RenderDomError>;
```

**Impact**: All 52 builtin render/compile fns, plus `user_defined_render_fn` and `user_defined_compile_fn`. Most of these ignore the component map argument entirely (they use `_components`).

### 4. Remove the old trait-based system entirely

Once the fn pointer signatures use FFI-safe types, the old system can be removed:

| Remove | Replacement |
|--------|-------------|
| `XmlComponentTrait` | `ComponentDef.render_fn` / `compile_fn` fn pointers |
| `XmlComponent` | `ComponentDef` |
| `XmlComponentMap` | `ComponentMap` |
| `DynamicXmlComponent` | `ComponentDef` with `source: UserDefined` |
| `ComponentArguments` | `ComponentParam` / `FilteredComponentArguments` |
| All `*Renderer` structs (52+) | Already duplicated as `builtin_component_def()` entries |
| `html_component!` macro | Not needed — `builtin_render_fn` already handles all builtins via `NodeType` |

### 5. Migrate `str_to_dom()` and `str_to_rust_code()` 

These are the only two entry points that use `XmlComponentMap`. They need to:
1. Accept `&ComponentMap` instead of `&mut XmlComponentMap`
2. Look up components via `ComponentMap::get()`/`get_unqualified()` instead of `XmlComponentMap::get()`
3. Call `def.render_fn` instead of `renderer.render_dom()`
4. Parse `<component>` tags into `ComponentDef` instead of `DynamicXmlComponent`

### 6. Clean up type aliases in api.json

Currently in api.json:
- `ComponentArgumentType` — type alias to `String`, unnecessary
- `ComponentArgumentTypes` — type alias to `Vec<(String, String)>`, not FFI-safe
- `ComponentArgumentName` — type alias to `String`, unnecessary
- `XmlComponent` — struct with `renderer: *mut c_void`, broken

These should be removed from api.json once the old system is gone.

---

## Execution Order

1. **Make `FilteredComponentArguments` FFI-safe** — change `BTreeMap<String, String>` → `StringPairVec`, change `ComponentArgumentTypes` → `StringPairVec`. Update all ~15 call sites.
2. **Change `ComponentRenderFn`/`ComponentCompileFn` signatures** — replace `&XmlComponentMap` with `&ComponentMap` and use the new `FilteredComponentArguments`.
3. **Update `builtin_render_fn`/`builtin_compile_fn`/`user_defined_*`** — trivial since they ignore the component map.
4. **Migrate `str_to_dom()` and `str_to_rust_code()`** — use `ComponentMap` lookup instead of `XmlComponentMap`. Create `ComponentDef` from `<component>` XML tags instead of `DynamicXmlComponent`.
5. **Delete old system** — remove `XmlComponentTrait`, `XmlComponent`, `XmlComponentMap`, `DynamicXmlComponent`, all `*Renderer` structs, `html_component!` macro, `ComponentArguments` struct.
6. **Clean api.json** — remove old type aliases, `XmlComponent`; re-run autofix to pick up the now-FFI-safe `FilteredComponentArguments`.
7. **Re-run autofix + codegen** — should produce clean output without `BTreeMap` errors.

---

## Risk Assessment

- **Low risk**: Steps 1–3 are mechanical replacements with no behavior change.
- **Medium risk**: Step 4 changes the XML parsing pipeline. Existing tests (`str_to_dom` tests) will validate.
- **Low risk**: Step 5 is pure deletion of dead code after migration.
- The `XmlComponentMap::default()` registered 52 builtins with `Box<dyn XmlComponentTrait>`. These are already duplicated by `register_builtin_components()` which creates 52 `ComponentDef`s. After migration, only one registration path remains.

## Lines of Code Estimate

- **Delete**: ~800 lines (trait, macro, 52 renderer structs, `DynamicXmlComponent`)
- **Modify**: ~100 lines (signature changes + call site updates)
- **Add**: ~30 lines (helper methods for `StringPairVec` lookup)
- **Net**: ~-670 lines
