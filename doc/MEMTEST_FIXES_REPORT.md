# Memtest Error Fixes Report

## Summary

Fixed **8 remaining errors** after initial session (down from 22, originally 342).

## Errors Fixed

### 1. AzAzString Double Prefix (2 errors)

**Problem:** Type `AzString` was defined as `"AzString"` in api.json. When the generator added the `Az` prefix, it became `AzAzString`.

**Fix:** Renamed `"AzString"` to `"String"` throughout api.json:
```bash
# 137 occurrences replaced
"type": "AzString" → "type": "String"
"AzString": { → "String": {
```

**Files Modified:**
- `api.json`: All references to `AzString` → `String`
- `doc/src/codegen/memtest.rs`: Removed workaround `pub type AzString = AzAzString`

---

### 2. U8VecRef Missing Traits (4 errors)

**Problem:** `U8VecRef` (a `&[u8]` slice wrapper) was missing:
- `Debug`
- `Clone`  
- `PartialEq`
- `PartialOrd`

**Fix:** Added automatic trait generation for all VecRef types in `memtest.rs`:

```rust
// For types with vec_ref_element_type set:
impl core::fmt::Debug for AzU8VecRef { ... }
impl Clone for AzU8VecRef { ... }
impl PartialEq for AzU8VecRef { ... }
impl PartialOrd for AzU8VecRef { ... }
impl Hash for AzU8VecRef { ... }  // Only if element supports Hash
impl Eq for AzU8VecRef { ... }    // Only if element supports Eq
impl Ord for AzU8VecRef { ... }   // Only if element supports Ord
```

**Files Modified:**
- `doc/src/codegen/memtest.rs`: Added `generate_vecref_trait_impls()` section
- `doc/src/codegen/struct_gen.rs`: Skip auto-derives for VecRef types (we generate them manually)

---

### 3. HashMap PartialOrd (1 error)

**Problem:** `ImageCache` contains `FastHashMap<String, ImageRef>`. HashMap doesn't implement `PartialOrd`, but the struct was deriving it.

**Fix:** Added detection for HashMap/FastHashMap fields in `struct_gen.rs`:

```rust
// Skip PartialOrd for types with HashMap fields
let has_hashmap_field = struct_fields.iter().any(|field_map| {
    field_map.values().any(|field_data| {
        field_data.r#type.contains("HashMap") || 
        field_data.r#type.contains("FastHashMap")
    })
});
if has_hashmap_field {
    opt_derive_other.clear(); // Removes PartialEq, PartialOrd
}
```

**Files Modified:**
- `doc/src/codegen/struct_gen.rs`: Added HashMap field detection

---

### 4. AzString Missing Traits (1 error)

**Problem:** `AzString` had `"derive": []` (empty), meaning no auto-derives. But other types using `AzString` as a field needed it to have `Clone`, `PartialEq`, etc.

**Fix:** Added manual trait implementations for `AzString` in memtest:

```rust
impl core::fmt::Debug for AzString {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        // Decode as UTF-8 for readable output
        let slice = unsafe { core::slice::from_raw_parts(self.vec.ptr, self.vec.len) };
        write!(f, "\"{}\"", String::from_utf8_lossy(slice))
    }
}
impl Clone for AzString { ... }
impl PartialEq for AzString { ... }
impl PartialOrd for AzString { ... }
impl Ord for AzString { ... }
impl Eq for AzString { ... }
impl Hash for AzString { ... }
```

**Files Modified:**
- `doc/src/codegen/memtest.rs`: Added String trait impls section
- `doc/src/codegen/struct_gen.rs`: Skip all derives for `AzString` (we generate them manually)

---

## Previous Session Fixes (for reference)

### Callback Type Traits

**Problem:** Callback wrapper types (e.g., `Callback`, `IFrameCallback`) were missing `Debug`, `Clone`, etc.

**Fix:** Auto-detect callback wrapper structs (exactly one field of callback_typedef type) and generate all traits:

```rust
fn get_callback_wrapper_field(struct_fields, callback_typedef_types) -> Option<String>
```

### Copy Trait Missing

**Problem:** Several types had `"derive": ["Copy"]` but their field types lacked Copy.

**Fix:** Added `"derive": ["Copy"]` to api.json for:
- `WindowDecorations`
- `WindowBackgroundMaterial`
- `WindowType`
- `DpiScaleFactor`
- `SvgPoint`
- `PhysicalPositionI32`
- `SvgQuadraticCurve`
- `SvgCubicCurve`
- `PhysicalPosition`
- `LogicalPosition`

### c_void Field Handling

**Problem:** Types with `c_void` fields couldn't derive Clone, PartialEq, etc.

**Fix:** Added detection in `struct_gen.rs`:

```rust
let has_c_void_field = struct_fields.iter().any(|field_map| {
    field_map.values().any(|field_data| field_data.r#type == "c_void")
});
if has_c_void_field {
    // Clear all problematic derives
}
```

### VecDestructor Traits

**Problem:** `*VecDestructor` enum types contain function pointers that can't be compared directly.

**Fix:** Generate manual trait impls comparing function pointers by address:

```rust
impl PartialEq for AzU8VecDestructor {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::External(a), Self::External(b)) => (*a as usize) == (*b as usize),
            ...
        }
    }
}
```

---

## Architecture Changes

### Sorted Struct Generation

Changed `generate_structs` to sort types alphabetically before generation, ensuring consistent output and that fundamental types (like `AzString`) are defined before types that depend on them.

### Tuple Variant Prefixing

Added support for enum variants with multiple types (tuples):

```json
{ "Gradient": { "type": "ColorU, ColorU" } }
```

Now correctly prefixes each type in the tuple: `(AzColorU, AzColorU)`

---

## Files Modified Summary

| File | Changes |
|------|---------|
| `api.json` | `AzString` → `String`, added Copy derives |
| `doc/src/codegen/memtest.rs` | VecRef traits, String traits, removed AzString workaround |
| `doc/src/codegen/struct_gen.rs` | HashMap detection, VecRef skip, String skip, tuple prefixing, sorted generation |

---

## Compilation Time

The memtest crate takes longer to compile because it contains:
- ~1115 type definitions
- ~811 function stubs
- ~730KB of generated Rust code

This is expected for a complete API binding.
