# Review: layout/src/json.rs

## Summary
- Lines: 240
- Public functions: 6 (`json_parse`, `json_stringify`, `serialize_refany_to_json`, `deserialize_refany_from_json`, `refany_serialize_to_json`, `json_deserialize_to_refany`)
- Public structs/enums: 1 (`ResultRefAnyString`), 2 type aliases (`RefAnySerializeFnType`, `RefAnyDeserializeFnType`)
- Findings: 0 high, 1 medium, 2 low

## Findings

### [MEDIUM] Duplicated wrapper functions — thin wrappers add indirection without value
- **Location**: `layout/src/json.rs:149-154` (`refany_serialize_to_json`) and `layout/src/json.rs:158-159` (`json_deserialize_to_refany`)
- **Details**: `refany_serialize_to_json` is a trivial wrapper around `serialize_refany_to_json` that converts `Option<Json>` → `OptionJson`. `json_deserialize_to_refany` wraps `deserialize_refany_from_json` converting `Result` → `ResultRefAnyString`. These exist for C API compatibility (referenced in `api.json`), so they serve a purpose, but the naming is confusingly similar to the inner functions.
- **Recommendation**: Consider renaming the inner functions to `_impl` suffixed names, or making them private since external code (debug_server.rs) calls the inner functions directly while the C API uses the wrappers.

### [LOW] Thin wrapper functions — `json_parse` and `json_stringify` are trivial delegations
- **Location**: `layout/src/json.rs:18-20` and `layout/src/json.rs:30-32`
- **Details**: `json_parse` just calls `Json::parse(s)` and `json_stringify` just calls `json.to_json_string()`. These exist for the C API (they are re-exported in `layout/src/lib.rs:101`), which is a valid reason.
- **Recommendation**: No action needed — they exist for C API naming conventions. Just noting.

### [LOW] `ResultRefAnyString` duplicates `std::result::Result` semantics
- **Location**: `layout/src/json.rs:48-80`
- **Details**: This is a `#[repr(C, u8)]` result type for FFI, which is a standard pattern in this codebase. The `is_ok`/`is_err`/`ok`/`err` methods duplicate `Result` but are needed because this is a custom FFI type.
- **Recommendation**: No action needed — this is the expected pattern for C-compatible result types.

## System Documentation
- System identified: yes — JSON serialization / C API data interchange
- Existing doc: none (no `doc/guide/json.md` or similar)
- Doc needed: A guide covering JSON support would be useful, explaining: the split between `core::json` (data types) and `layout::json` (serde_json parsing), RefAny serialization/deserialization for the debug server, and the C API function pointer mechanism. This could be a section within a broader "C API" or "data interchange" guide rather than a standalone document.
