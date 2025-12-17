# Python Extension v2 Migration Report

## Executive Summary

The v2 Python extension generator has **328 errors** compared to the working v1 generator. 
This report analyzes the root causes and proposes solutions.

## Current Error Categories (328 total)

### 1. Name Conflicts / Duplicate Definitions (10 errors)

**Error Type:** `E0428: the name 'AzXxx' is defined multiple times`

**Affected Types:**
- `AzString`, `AzU8Vec`, `AzStringVec`
- `AzRefAny`, `AzInstantPtr`, `AzStringMenuItem`
- `AzGLuintVec`, `AzGLintVec`
- `AzU8VecDestructor`, `AzStringVecDestructor`

**Root Cause:**
The v2 generator creates both:
1. Type aliases in `generate_capi_type_aliases()`: `type AzString = __dll_api_inner::dll::AzString;`
2. pyclass wrapper structs: `pub struct AzString { pub inner: ... }`

The v1 generator avoided this by using these types **directly** from the C-API without creating
wrapper structs for them.

**V1 Approach:**
- These types are in `MANUAL_TYPES` and are NOT wrapped - they use the C-API types directly
- Custom `From<String>/Into<AzString>` impls are generated for interop
- PyO3 traits (`FromPyObject`, `IntoPyObject`) are implemented on the C-API types directly

---

### 2. Conflicting Trait Implementations (44 errors)

**Error Type:** `E0119: conflicting implementations`

**Affected Traits:**
- `Clone` for AzString, AzU8Vec, AzStringVec, AzRefAny, etc.
- `Debug` for same types
- `From<T>` for same types  
- `FromPyObject`, `IntoPyObject` for AzString, AzU8Vec, AzStringVec

**Root Cause:**
Same as #1 - the C-API module already has these traits implemented (via `#[derive(...)]`),
and then v2 tries to implement them again on wrapper structs that shouldn't exist.

**V1 Approach:**
These types are excluded from wrapper generation entirely. The C-API implementations are used.

---

### 3. No `inner` Field Errors (75+ errors)

**Error Type:** `E0609: no field 'inner'` and `E0560: struct has no field named 'inner'`

**Affected Types:**
- `AzRefAny`, `AzImageRef`, `AzFontRef`, `AzStringMenuItem`
- `AzU8Vec`, `AzStringVec`, `AzString`
- `AzInstantPtr`, `AzSvg`, `AzGlVoidPtrConst`
- Callback function pointer types (e.g., `extern "C" fn(AzRefAny, ...) -> ...`)

**Root Cause:**
The v2 generator generates method bodies that assume all types have a `.inner` field,
but the C-API types (used directly) don't have this wrapper structure.

**V1 Approach:**
- For "special types" (String, U8Vec, RefAny, etc.), v1 generates different code paths
- The `converted_arg_names` system tracks which args need special conversion
- Callback types are never accessed with `.inner` - they're handled via trampolines

---

### 4. PyO3 `FromPyObject` Not Implemented (50+ errors)

**Error Type:** `E0277: cannot be used as a Python function argument`

**Affected Types:**
- `AzImageRef`, `AzFontRef`, `AzSvg`
- `AzCallback`, `AzIFrameNode`, `AzWindowCreateOptions`
- All callback function pointer types (e.g., `extern "C" fn(...)`)
- Widget callback types (`AzButtonOnClick`, `AzTextInputOnValueChange`, etc.)
- Generic types (`AzPhysicalPosition<i32>`)
- Pointer types (`*mut c_void`, `*const c_void`)

**Root Cause:**
These types don't implement `FromPyObject` trait, so they can't be used as Python method arguments.

**V1 Approach:**
V1 uses `function_has_unsupported_args()` to **skip methods** that take these types as arguments.
The philosophy is: if a type can't be converted from Python, don't generate methods that use it.

Key check in v1 (`can_have_python_constructor()`):
```rust
// Skip if type is RefAny (requires special handling)
if type_str == "RefAny" { return false; }

// Skip if type is a callback type 
if is_callback_typedef(field_class) { return false; }

// Skip if type ends with "CallbackType"
if type_str.ends_with("CallbackType") { return false; }
```

---

### 5. Missing Methods (25+ errors)

**Error Type:** `E0599: no method named 'xxx' found`

**Missing Methods:**
- `Dom::hash`, `NodeData::hash`
- `ColorU::from_str`, `ColorU::white`, `ColorU::black`, `ColorU::transparent`
- `StyledDom::from_xml`, `StyledDom::from_file` (renamed to `from_xhtml`)
- `RawImage::empty`, `RawImage::allocate_clip_mask`
- `RawImage::encode_png/jpeg/gif/bmp/tga/pnm/tiff`
- `SvgMultiPolygon::contains_point/intersection/difference/union/xor`
- `SvgXxx::tessellate_fill/tessellate_stroke`
- `AngleValue::get_degrees`
- `ProgressBar::with_container_style`
- `Menu::with_popup_position`

**Root Cause:**
These methods exist in `api.json` but not in the actual Rust source code, or have been renamed.
V2 trusts `api.json` blindly while v1 has the same problem but generates fewer methods overall.

**V1 Approach:**
V1 has the same issue - it generates calls to methods that don't exist. This is an api.json
synchronization problem, not a v1 vs v2 difference.

---

### 6. Type Mismatch Errors (23 errors)

**Error Type:** `E0308: mismatched types`

**Examples:**
- Expected `AzString`, found `&str` or `String`
- Method return type mismatches

**Root Cause:**
V2 generates code like `method_name(arg.as_str())` or `method_name(arg_ext)` where the
actual method expects `AzString` not `&str`.

**V1 Approach:**
V1 uses `azul_css::corety::AzString::from(arg.clone())` to convert String args to AzString.

---

## Architectural Differences: V1 vs V2

### V1 Architecture (Old Generator)

```
┌─────────────────────────────────────────────────────────────┐
│                    python_api.rs                            │
├─────────────────────────────────────────────────────────────┤
│  1. Generate inline C-API module (__dll_api_inner::dll)     │
│     - All types with #[repr(C)]                             │
│     - Clone/Debug/Drop via transmute to external types      │
│     - NO C-ABI functions (skip_c_abi_functions=true)        │
├─────────────────────────────────────────────────────────────┤
│  2. Type classification                                     │
│     - RECURSIVE_TYPES: Skip entirely (infinite size)        │
│     - VECREF_TYPES: Skip (borrow semantics)                 │
│     - Callback+Data pairs: Special wrapper generation       │
│     - "Special types": Use C-API directly, no wrapper       │
│       (String, U8Vec, StringVec, RefAny, etc.)              │
│     - Regular types: Generate wrapper struct { inner: T }   │
├─────────────────────────────────────────────────────────────┤
│  3. For EACH regular type:                                  │
│     - #[pyclass] wrapper struct                             │
│     - From<CApi>/Into<CApi> impls                           │
│     - Clone/Debug/Drop impls                                │
│     - #[pymethods] with transmute to external types         │
├─────────────────────────────────────────────────────────────┤
│  4. For Callback+Data pairs:                                │
│     - Generate XxxTy wrapper holding Py<PyAny>              │
│     - Generate invoke_py_xxx trampoline                     │
│     - #[new] constructor takes (data, callback)             │
├─────────────────────────────────────────────────────────────┤
│  5. Special handling for method arguments:                  │
│     - String → AzString::from(arg.clone())                  │
│     - Vec<T> → External Vec type via collect                │
│     - Callback types → Skip method entirely                 │
│     - RefAny → Skip method entirely                         │
│     - Regular types → transmute(arg.inner)                  │
└─────────────────────────────────────────────────────────────┘
```

### V2 Architecture (New Generator)

```
┌─────────────────────────────────────────────────────────────┐
│                    lang_python.rs                           │
├─────────────────────────────────────────────────────────────┤
│  1. Generate inline C-API module via RustGenerator          │
│     - Same as v1                                            │
├─────────────────────────────────────────────────────────────┤
│  2. Type filtering (PROBLEM AREA)                           │
│     - Only filters by name patterns                         │
│     - Missing: special type handling                        │
│     - Missing: callback+data pair detection                 │
├─────────────────────────────────────────────────────────────┤
│  3. For ALL included types:                                 │
│     - Generate wrapper struct { inner: T }                  │
│     - INCLUDING types that shouldn't be wrapped!            │
│       (String, U8Vec, RefAny, etc.)                         │
├─────────────────────────────────────────────────────────────┤
│  4. Method generation (PROBLEM AREA)                        │
│     - Assumes all args have .inner field                    │
│     - Doesn't detect callback types                         │
│     - String handling: Uses .as_str() instead of AzString   │
├─────────────────────────────────────────────────────────────┤
│  5. Missing features:                                       │
│     - Callback+Data pair wrapper generation                 │
│     - Per-type argument conversion                          │
│     - Special type exclusion from wrapper generation        │
└─────────────────────────────────────────────────────────────┘
```

## Core Problem

**V2 treats all types uniformly while V1 has distinct handling for different type categories.**

V1's type categories:

| Category | Examples | Wrapper Struct? | Methods? |
|----------|----------|-----------------|----------|
| Recursive | XmlNode, Xml | ❌ No | ❌ No |
| VecRef | U8VecRef, Refstr | ❌ No | ❌ No |
| Special/Manual | String, U8Vec, RefAny, ImageRef, FontRef | ❌ No (use C-API directly) | ✅ Yes (special conversion) |
| Callback+Data | Callback, IFrameCallback | ✅ Yes (custom) | ✅ Yes (with trampoline) |
| Regular | Dom, Button, ColorU | ✅ Yes (inner wrapper) | ✅ Yes (transmute) |

## Solution Approach

### Option A: Port V1 Logic to V2

1. Add `SPECIAL_TYPES` constant for types that shouldn't get wrapper structs
2. Don't generate pyclass wrappers for special types
3. Implement PyO3 traits on C-API types directly (as v1 does)
4. Port `is_callback_data_pair_struct()` detection
5. Port argument conversion logic per type category

### Option B: Use V1 Generator (Recommended Short-Term)

The v1 generator works. The v2 generator needs significant work to match v1's type handling.
For now, use v1 and incrementally port its logic to v2.

### Option C: Unified Type Handling System

Design a proper type classification system in the IR:
```rust
enum TypeCategory {
    Recursive,           // Skip entirely
    VecRef,             // Skip entirely  
    Primitive,          // Use directly
    String,             // Use C-API, special conversion
    Vec,                // Use C-API, special conversion
    RefAny,             // Use C-API, callback data wrapper
    Callback,           // Trampoline required
    CallbackDataPair,   // Custom wrapper struct
    Regular,            // Standard inner wrapper
}
```

This would replace all the ad-hoc checks with a single source of truth.

## Specific Fixes Needed

### Fix 1: Don't Generate Wrappers for Special Types

Remove from wrapper generation:
- `String`, `U8Vec`, `StringVec`
- `RefAny`, `RefCount`
- `GLuintVec`, `GLintVec`
- `U8VecDestructor`, `StringVecDestructor`
- `InstantPtr`, `StringMenuItem`

These should use the C-API types directly with PyO3 trait impls.

### Fix 2: Skip Methods with Incompatible Argument Types

Add to `function_has_unsupported_args()`:
- Check for `Callback` in type name
- Check for `RefAny` argument
- Check for function pointer types (`extern "C" fn`)
- Check for generic types (contain `<` and `>`)

### Fix 3: Fix String Argument Conversion

Change:
```rust
format!("{}.as_str()", arg.name)  // Wrong
```
To:
```rust
format!("azul_css::corety::AzString::from({}.clone())", arg.name)
```

### Fix 4: Implement Callback+Data Pair Detection

Port from v1:
```rust
fn is_callback_data_pair_struct(class_data: &ClassData) -> Option<(String, String, CallbackSignature)>
```

### Fix 5: Generate Callback Trampolines Properly

V1 generates:
1. `XxxTy` wrapper struct holding `Py<PyAny>` objects
2. `invoke_py_xxx` trampoline function
3. Custom `#[new]` that creates RefAny from wrapper

V2 has some of this but doesn't use it correctly.

## Conclusion

The v2 generator fundamentally misunderstands how the Python extension works:

1. **Not all types get wrapper structs** - Some use C-API types directly
2. **Arguments need per-type conversion** - Not just `.inner` access
3. **Callbacks need trampolines** - Not method calls
4. **Some methods must be skipped** - If args can't be converted

The fastest path forward is to either:
- Use v1 generator as-is
- Port v1's type classification logic to v2 before any method generation

The ideal long-term solution is a proper type classification system in the IR that
eliminates all special cases from the generators.
