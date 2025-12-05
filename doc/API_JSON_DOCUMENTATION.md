# api.json Documentation

This document describes the structure and fields of `api.json`, which is the single source of truth for the Azul C-API. The file is used by multiple code generators:

- **autofix**: Generates Rust bindings, C headers, Python bindings
- **memtest**: Generates memory layout tests to validate struct sizes/alignments
- **documentation**: Generates API documentation

## Overview

```json
{
    "1.0.0-alpha1": {
        "git_revision": "abc123",
        "api": {
            "module_name": {
                "classes": {
                    "ClassName": { /* ClassData */ }
                }
            }
        }
    }
}
```

## ClassData Fields

### Basic Identity

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `doc` | `string` | No | Documentation comment for the type |
| `external` | `string` | Yes | Full Rust path to the type (e.g., `azul_core::window::WindowFlags`) |

**Example:**
```json
"WindowFlags": {
    "doc": "Boolean flags relating to the current window state",
    "external": "azul_core::window::WindowFlags"
}
```

### Type Definition (exactly ONE required)

| Field | Type | Description |
|-------|------|-------------|
| `struct_fields` | `array` | Defines a struct with named fields |
| `enum_fields` | `array` | Defines an enum with variants |
| `callback_typedef` | `object` | Defines a function pointer type |
| `type_alias` | `object` | Defines a type alias (generic instantiation) |

---

## struct_fields

Defines a C-compatible struct. Each element is a single-key object mapping field name to field data.

```json
"struct_fields": [
    {
        "field_name": {
            "type": "FieldType",
            "doc": "Field documentation"
        }
    }
]
```

**Example:**
```json
"SvgPoint": {
    "external": "azul_css::SvgPoint",
    "struct_fields": [
        { "x": { "type": "f32" } },
        { "y": { "type": "f32" } }
    ]
}
```

### Special struct_fields Features

#### VecRef Types (Slice References)

For types representing `&[T]` slices, add:

```json
"U8VecRef": {
    "external": "azul_core::U8VecRef",
    "vec_ref_element_type": "u8",
    "vec_ref_is_mut": false,
    "struct_fields": [
        { "ptr": { "type": "*const u8" } },
        { "len": { "type": "usize" } }
    ]
}
```

| Field | Description |
|-------|-------------|
| `vec_ref_element_type` | The element type (e.g., "u8", "StyleProperty") |
| `vec_ref_is_mut` | `true` for `&mut [T]`, `false` for `&[T]` |

**Auto-generated:** `as_slice()`, `as_mut_slice()`, `From<&[T]>`, `Debug`, `Clone`, `PartialEq`, `PartialOrd`, `Hash`, `Eq`, `Ord` (if element supports it)

---

## enum_fields

Defines a C-compatible enum. Supports unit variants, tuple variants, and struct variants.

```json
"enum_fields": [
    { "VariantName": { "doc": "Variant documentation" } },
    { "TupleVariant": { "type": "InnerType" } },
    { "StructVariant": { 
        "struct_fields": [
            { "field": { "type": "FieldType" } }
        ]
    }}
]
```

**Example:**
```json
"OptionSvgPoint": {
    "external": "azul_core::OptionSvgPoint",
    "enum_fields": [
        { "None": {} },
        { "Some": { "type": "SvgPoint" } }
    ]
}
```

---

## callback_typedef

Defines a function pointer type for callbacks.

```json
"callback_typedef": {
    "fn_args": [
        { "type": "ArgType", "ref": "ref|refmut|value", "doc": "Argument doc" }
    ],
    "returns": { "type": "ReturnType" }
}
```

| Field | Values | Description |
|-------|--------|-------------|
| `ref` | `"value"` | Pass by value |
| `ref` | `"ref"` | Pass by `&T` reference |
| `ref` | `"refmut"` | Pass by `&mut T` reference |

**Example:**
```json
"CallbackType": {
    "doc": "Main callback type for UI event handling",
    "external": "azul_layout::callbacks::CallbackType",
    "callback_typedef": {
        "fn_args": [
            { "type": "RefAny", "ref": "refmut", "doc": "User data" },
            { "type": "CallbackInfo", "ref": "refmut", "doc": "Event info" }
        ],
        "returns": { "type": "Update" }
    }
}
```

**Auto-generated for callback wrapper structs:** `Debug`, `Clone`, `Hash`, `PartialEq`, `PartialOrd`, `Ord`, `Eq`, `Copy`

A "callback wrapper struct" is detected when:
- Struct has exactly ONE field
- That field's type is a `callback_typedef` type

---

## type_alias

Defines a type alias, typically for generic instantiation.

```json
"type_alias": {
    "target": "GenericType",
    "generic_args": ["ConcreteType"]
}
```

**Example:**
```json
"PhysicalPositionI32": {
    "external": "azul_core::geom::PhysicalPositionI32",
    "doc": "Physical position with i32 coordinates",
    "type_alias": {
        "target": "PhysicalPosition",
        "generic_args": ["i32"]
    }
}
```

---

## derive

Controls which traits are derived for the type.

```json
"derive": ["Copy", "Eq", "Ord", "Hash", "Serialize", "Deserialize"]
```

| Trait | Effect |
|-------|--------|
| `Copy` | Type is `Copy` (must have no heap allocations) |
| `Eq` | Adds `#[derive(Eq)]` |
| `Ord` | Adds `#[derive(Ord)]` |
| `Hash` | Adds `#[derive(Hash)]` |
| `Serialize` | Adds serde `Serialize` (behind feature gate) |
| `Deserialize` | Adds serde `Deserialize` (behind feature gate) |

### Special Cases

| `derive` value | Meaning |
|----------------|---------|
| Not specified | Auto-derive `Debug`, `Clone`, `PartialEq`, `PartialOrd` |
| `[]` (empty array) | NO auto-derives (used for Vec/Option types with custom impls) |
| `["Copy"]` | Auto-derives + Copy |

**IMPORTANT:** Types with `"derive": []` are handled specially:
- In dll.rs: They use `impl_vec!`, `impl_option!` macros
- In memtest: The autofix system populates the correct derives from source code

---

## is_boxed_object

```json
"is_boxed_object": true
```

Indicates the type is a heap-allocated, opaque pointer type. Used for:
- RefAny
- Thread handles
- Platform-specific types

Effects:
- No `derive(Clone)` (requires custom clone via reference counting)
- `treat_external_as_ptr` is set
- Destructor handling

---

## custom_impls

```json
"custom_impls": ["Clone", "Drop"]
```

Lists traits that have MANUAL implementations in the source code. The generator will NOT auto-derive these traits.

---

## has_custom_destructor

Deprecated. Use `custom_impls: ["Drop"]` instead.

---

## Generic Types

### generic_params

For types with generic parameters:

```json
"PhysicalPosition": {
    "external": "azul_core::geom::PhysicalPosition",
    "generic_params": ["T"],
    "struct_fields": [
        { "x": { "type": "T" } },
        { "y": { "type": "T" } }
    ]
}
```

---

## Naming Conventions

### Type Names

| In api.json | Generated as | Notes |
|-------------|--------------|-------|
| `String` | `AzString` | Prefix added automatically |
| `Vec` | `AzVec` | Prefix added automatically |
| `u8`, `f32`, etc. | `u8`, `f32` | Primitives NOT prefixed |
| `T`, `U` | `T`, `U` | Single-letter generics NOT prefixed |

**IMPORTANT:** Never use `AzString` in api.json. Use `String`. The generator adds the `Az` prefix.

### Primitive Types (never prefixed)

```
bool, f32, f64, i8, i16, i32, i64, i128, isize,
u8, u16, u32, u64, u128, usize, c_void, str, char
```

---

## Auto-Detection Features

The generators automatically detect and handle:

### 1. Callback Wrapper Structs

**Detection:** Struct with exactly one field whose type is a `callback_typedef`

**Auto-generated traits:** Debug, Clone, Hash, PartialEq, PartialOrd, Ord, Eq, Copy

```json
"Callback": {
    "struct_fields": [
        { "cb": { "type": "CallbackType" } }
    ]
}
```

### 2. VecRef/Slice Types

**Detection:** `vec_ref_element_type` is set

**Auto-generated:** 
- `as_slice()` / `as_mut_slice()` methods
- `From<&[T]>` implementation
- All comparison/hash traits (if element supports them)

### 3. VecDestructor Enums

**Detection:** Type name ends with `VecDestructor`

**Auto-generated:** Debug, PartialEq, PartialOrd (comparing function pointers by address)

### 4. String Type

**Detection:** Type name is `String`

**Auto-generated:** Debug (using UTF-8 decoding), Clone, PartialEq, PartialOrd, Ord, Eq, Hash

### 5. HashMap/FastHashMap Fields

**Detection:** Field type contains `HashMap` or `FastHashMap`

**Effect:** PartialOrd NOT derived (HashMap doesn't support ordering)

### 6. c_void Fields

**Detection:** Field has `"type": "c_void"`

**Effect:** Clone, PartialEq, PartialOrd, Eq, Ord, Hash NOT derived

### 7. Copy Inference

**Detection:** `"derive": ["Copy"]` is set

**Effect:** `#[derive(Copy)]` added. Type must have all Copy-able fields.

---

## Complete Example

```json
{
    "1.0.0-alpha1": {
        "api": {
            "window": {
                "classes": {
                    "WindowFlags": {
                        "doc": "Boolean flags for window state",
                        "external": "azul_core::window::WindowFlags",
                        "derive": ["Copy", "Serialize", "Deserialize"],
                        "struct_fields": [
                            { "is_visible": { "type": "bool", "doc": "Window visibility" } },
                            { "is_resizable": { "type": "bool" } },
                            { "decorations": { "type": "WindowDecorations" } }
                        ]
                    },
                    "WindowDecorations": {
                        "doc": "Window decoration style",
                        "external": "azul_core::window::WindowDecorations",
                        "derive": ["Copy"],
                        "enum_fields": [
                            { "Normal": { "doc": "Full decorations" } },
                            { "NoTitle": {} },
                            { "None": {} }
                        ]
                    }
                }
            }
        }
    }
}
```

---

## Troubleshooting

### "AzAzString" double prefix

**Problem:** Type appears as `AzAzString` instead of `AzString`

**Solution:** In api.json, use `"type": "String"` not `"type": "AzString"`. The generator adds the prefix.

### Copy trait errors

**Problem:** `Copy cannot be implemented` errors

**Solution:** Ensure all field types also have `"derive": ["Copy"]` in api.json.

### Missing traits for callbacks

**Problem:** Callback types missing Debug/Clone/etc.

**Solution:** Ensure the callback wrapper struct has exactly ONE field of callback_typedef type.

### HashMap comparison errors

**Problem:** `PartialOrd` not implemented for HashMap

**Solution:** Types with HashMap fields automatically skip PartialOrd derivation. No action needed.

---

## Files Modified by Generators

| Generator | Output | Purpose |
|-----------|--------|---------|
| autofix | `dll/src/*.rs` | C-ABI bindings |
| memtest | `target/memtest/` | Memory layout tests |
| doc | `doc/target/` | Documentation |
