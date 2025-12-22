# Azul Codegen V2 Architecture

## Overview

The Azul codegen system generates multi-language bindings (Rust, C, C++, Python) from a single source of truth: `api.json`. The system uses an **Intermediate Representation (IR)** to decouple parsing from code generation.

```
api.json
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│              IRBuilder (ir_builder.rs)                           │
│  Parses api.json and builds CodegenIR                            │
└──────────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│              CodegenIR (ir.rs)                                   │
│  - structs: Vec<StructDef>                                       │
│  - enums: Vec<EnumDef>                                           │
│  - functions: Vec<FunctionDef>                                   │
│  - type_aliases, constants, callback_typedefs                    │
└──────────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│              CodegenConfig (config.rs)                           │
│  Configures how to generate code:                                │
│  - target_lang: Rust | CHeader | CppHeader | Python              │
│  - cabi_functions: InternalBindings | ExternalBindings | None    │
│  - struct_mode: Prefixed | Unprefixed | None                     │
│  - trait_impl_mode: UsingDerive | UsingTransmute | UsingCAPI     │
│  - type_prefix: "Az" | ""                                        │
└──────────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│              CodeGenerator (generator.rs)                        │
│  Dispatches to language-specific generators based on config      │
└──────────────────────────────────────────────────────────────────┘
    │
    ├──► RustGenerator (lang_rust.rs)
    ├──► CGenerator (lang_c.rs)  
    ├──► CppGenerator (lang_cpp/)
    └──► PythonGenerator (lang_python.rs)
```

## File Locations

```
doc/src/codegen/v2/
├── mod.rs              # Entry points: generate_dll_static(), generate_all_v2(), etc.
├── config.rs           # CodegenConfig, TargetLang, CAbiFunctionMode, etc.
├── ir.rs               # IR types: StructDef, EnumDef, FunctionDef, etc.
├── ir_builder.rs       # IRBuilder: converts api.json → CodegenIR
├── generator.rs        # CodeGenerator trait and dispatcher
├── lang_rust.rs        # RustGenerator: generates Rust code
├── lang_c.rs           # CGenerator: generates C headers
├── lang_cpp/           # C++ generators (dialect-based)
├── lang_python.rs      # PythonGenerator: generates PyO3 bindings
├── lang_reexports.rs   # Generates reexports.rs (Az* → unprefixed)
├── transmute_helpers.rs # Helpers for transmute-based impl generation
└── rust/               # Alternative Rust generators (static/dynamic)
    ├── mod.rs
    ├── shared.rs
    ├── static_binding.rs
    └── dynamic_binding.rs
```

## Generated Output Files

Running `cd doc && cargo run --release -- codegen all` generates:

```
target/codegen/v2/
├── dll_api_build.rs    # For building libazul.dylib (feature = "build-dll")
│                       # Contains: types + #[no_mangle] C-ABI functions
│
├── dll_api_static.rs   # For static linking (feature = "link-static")
│                       # Contains: types + trait impls via transmute + impl blocks
│
├── dll_api_dynamic.rs  # For dynamic linking (feature = "link-dynamic")
│                       # Contains: types + extern "C" declarations + impl blocks
│
├── reexports.rs        # Re-exports Az* types as unprefixed in modules
│                       # e.g., pub mod app { pub use ...::AzApp as App; }
│
├── azul.h              # C header
├── azul03.hpp          # C++03 header
├── azul11.hpp          # C++11 header
├── azul14.hpp          # C++14 header
├── azul17.hpp          # C++17 header
├── azul20.hpp          # C++20 header
├── azul23.hpp          # C++23 header
├── python_api.rs       # Python/PyO3 extension module
└── memtest.rs          # Memory layout tests
```

## DLL Feature Flags

In `dll/Cargo.toml`:

```toml
[features]
build-dll = []      # Build the shared library with #[no_mangle] exports
link-static = []    # Static linking: types + transmute-based trait impls
link-dynamic = []   # Dynamic linking: types + extern "C" declarations
```

In `dll/src/lib.rs`, these features control which generated file is included:

- `build-dll`: includes `dll_api_build.rs`
- `link-static`: includes `dll_api_static.rs`
- `link-dynamic`: includes `dll_api_dynamic.rs`

## api.json Structure

```json
{
  "1.0.0-alpha1": {
    "api": {
      "<module_name>": {
        "doc": ["Module documentation"],
        "classes": {
          "<ClassName>": {
            "external": "azul_core::dom::Dom",  // Internal Rust path
            "struct_fields": [...],              // For structs
            "enum_fields": [...],                // For enums
            "derive": ["Clone", "Debug"],
            "custom_impls": ["Drop"],
            "constructors": {
              "create": {
                "fn_args": [{"node_type": "NodeType"}],
                "fn_body": "azul_core::dom::Dom::create_node(node_type)"
              }
            },
            "functions": {
              "add_child": {
                "fn_args": [{"self": "refmut"}, {"child": "Dom"}],
                "fn_body": "object.add_child(child)"
              },
              "with_child": {
                "fn_args": [{"self": "value"}, {"child": "Dom"}],
                "returns": {"type": "Dom"},
                "fn_body": "object.with_child(child)"
              }
            }
          }
        }
      }
    }
  }
}
```

### Key Concepts in api.json

1. **Constructors**: Static methods that create new instances
   - No `self` parameter
   - Become `ClassName_constructorName` in C-ABI

2. **Functions/Methods**: Instance methods
   - Have `self` parameter (value, ref, refmut)
   - Become `ClassName_methodName` in C-ABI

3. **Self parameter convention**:
   - `{"self": "value"}` → takes ownership (moves self)
   - `{"self": "ref"}` → `&self`
   - `{"self": "refmut"}` → `&mut self`

4. **Return types**:
   - `"returns": {"type": "Dom"}` → returns Dom

## Code Generation Configs

### dll_build (for building libazul.dylib)

```rust
CodegenConfig {
    target_lang: Rust,
    cabi_functions: InternalBindings { no_mangle: true },
    struct_mode: Prefixed,  // AzDom, AzApp, etc.
    trait_impl_mode: UsingTransmute { external_crate: "azul_core" },
    type_prefix: "Az",
}
```

Generates:
- Structs with `Az` prefix: `pub struct AzDom { ... }`
- C-ABI functions with `#[no_mangle]`: `pub extern "C" fn AzDom_create(...)`
- Trait impls using transmute: `impl Clone for AzDom { ... }`
- Impl blocks with methods: `impl AzDom { pub fn create(...) { ... } }`

### dll_static (for static linking)

```rust
CodegenConfig {
    target_lang: Rust,
    cabi_functions: InternalBindings { no_mangle: false },  // No export
    struct_mode: Prefixed,
    trait_impl_mode: UsingTransmute { external_crate: "azul_core" },
    type_prefix: "Az",
}
```

Same as dll_build but without `#[no_mangle]` - functions are internal only.

### dll_dynamic (for dynamic linking)

```rust
CodegenConfig {
    target_lang: Rust,
    cabi_functions: ExternalBindings { link_library: "azul" },
    struct_mode: Prefixed,
    trait_impl_mode: UsingCAPI,  // Calls C-ABI functions for traits
    type_prefix: "Az",
}
```

Generates:
- Same structs
- `extern "C" { fn AzDom_create(...) -> AzDom; }`
- Trait impls that call C-ABI: `impl Clone for AzDom { AzDom_deepCopy(self) }`

## Type Naming Convention

- **Internal (Rust core)**: `Dom`, `App`, `Css` (in azul_core, azul_css)
- **C-ABI / FFI**: `AzDom`, `AzApp`, `AzCss` (with `Az` prefix)
- **Public API**: Re-exported without prefix via `reexports.rs`

Example usage:
```rust
use azul::dom::Dom;  // Imports AzDom as Dom
use azul::ffi::dll::AzDom;  // Direct access to prefixed type
```

## IRBuilder Phases (ir_builder.rs)

1. **Validate** - Check for disallowed patterns (arrays, non-FFI-safe types)
2. **Build type lookups** - Map type names to modules
3. **Build type definitions** - Parse structs and enums
4. **Build callback typedefs** - Function pointer types
5. **Build type aliases**
6. **Link callback wrappers** - Connect callbacks to their wrappers
7. **Build API functions** - Parse constructors and methods from api.json
8. **Build enum variant constructors** - Auto-generate variant creators
9. **Build trait functions** - Generate `_deepCopy`, `_delete`, `_partialEq`, etc.
10. **Build constants**
11. **Sort by dependencies** - Topological sort for C/C++

## RustGenerator Details (lang_rust.rs)

The RustGenerator produces:

1. **Type definitions**: Structs with `#[repr(C)]` and enums
2. **Trait implementations**: Clone, Drop, PartialEq, etc.
3. **Impl blocks**: Methods that wrap C-ABI calls
4. **C-ABI functions**: The actual exported functions (for dll_build)

### Method Generation

Methods are generated in impl blocks by wrapping C-ABI function calls:

```rust
impl AzDom {
    pub fn create_text(value: AzString) -> AzDom {
        unsafe { AzDom_createText(value) }
    }
    
    pub fn add_child(&mut self, child: AzDom) {
        unsafe { AzDom_addChild(self, child) }
    }
    
    pub fn with_child(self, child: AzDom) -> AzDom {
        unsafe { AzDom_withChild(self, child) }
    }
}
```

### Self Parameter Detection

In `generate_method()`, a parameter is detected as `self` if:
1. Its name is literally `"self"`, OR
2. Its name equals `class_name.to_lowercase()` AND its type equals `class_name`, OR
3. Its name is `"object"` AND its type equals `class_name`

## extended_api Module (dll/src/lib.rs)

The `extended_api` module provides Rust-only ergonomic methods that can't be expressed in C-ABI:

```rust
impl AzRefAny {
    pub fn new<T: 'static>(value: T) -> Self { ... }
    pub fn downcast_ref<T: 'static>(&mut self) -> Option<Ref<'_, T>> { ... }
}

impl AzString {
    pub fn from_str(s: &str) -> Self { ... }
}

impl From<&str> for AzString { ... }
```

These use `transmute` to convert between `Az*` types and their internal counterparts.

## Typical Rust Example Usage

```rust
use azul::prelude::*;
use azul::app::App;
use azul::dom::Dom;
use azul::callbacks::LayoutCallback;
use azul::extended_api::*;  // For from_str, new, downcast

fn main() {
    let data = RefAny::new(MyData::new());
    let app = App::new(data, AppConfig::new());
    app.run(WindowCreateOptions::from_fn(layout_fn));
}

extern "C" fn layout_fn(data: &mut RefAny, _info: &mut LayoutCallbackInfo) -> StyledDom {
    Dom::create_text(AzString::from_str("Hello"))
        .with_inline_style(AzString::from_str("font-size: 24px;"))
        .style(Css::empty())
}
```

## Running Codegen

```bash
# Generate all outputs
cd doc && cargo run --release -- codegen all

# Generate specific targets
cd doc && cargo run --release -- codegen rust
cd doc && cargo run --release -- codegen c
cd doc && cargo run --release -- codegen cpp
cd doc && cargo run --release -- codegen python
```

## Building Examples

```bash
# Static linking (compiles everything into binary)
cd examples/rust && cargo build --features "link-static"

# Dynamic linking (requires libazul.dylib at runtime)
cd examples/rust && cargo build --features "link-dynamic"

# Build the DLL itself
cd dll && cargo build --release --features "build-dll"
```

## Key Design Decisions

1. **Az prefix**: All FFI types use `Az` prefix to avoid conflicts and be C-friendly
2. **transmute**: Static linking uses transmute between `Az*` types and internal types
3. **Separate configs**: Different output files for different use cases
4. **IR-based**: Single IR allows multiple language backends
5. **api.json driven**: All bindings derived from one source

## Common Issues

1. **"no method named X on type AzY"**
   - Method might be missing from api.json
   - Codegen might not have been run after api.json changes
   - Method might need `self` parameter in api.json

2. **"cannot find type AzString"**
   - Make sure the correct feature flag is enabled
   - Run codegen to regenerate files

3. **"mismatched types"**
   - Use `AzString::from_str()` not `&str`
   - Check if method takes ownership vs reference

4. **"cannot find function AzDom_xxx"**
   - Dynamic linking: ensure libazul.dylib is in path
   - Static linking: ensure `link-static` feature is enabled
