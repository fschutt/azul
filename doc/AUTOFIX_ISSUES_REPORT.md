# AUTOFIX ISSUES REPORT

Generated: 2025-12-04

## Summary

The `azul-doc autofix` command has multiple critical bugs that cause it to generate incorrect patches. This document catalogs each issue with root cause analysis.

---

## Issue 1: "ref", "value" Recognized as Types

**Symptom:**
```
[DISCOVER] Found 6 types referenced in API but not defined as classes
  - ref
  - value
```

**Root Cause:**
In `FunctionData`, `fn_args` is defined as `Vec<IndexMap<String, String>>`. The actual JSON structure is:
```json
"fn_args": [
  { "self": "ref" },            // <- "ref" is interpreted as a type!
  { "window": "WindowCreateOptions" }
]
```

When the parser iterates over `fn_args`, it takes every value in the IndexMap as a type. For `"self": "ref"`, "ref" is extracted as a type name.

**Affected Function:** `extract_types_from_function_data()` in `doc/src/api.rs:560-579`

**Fix Required:** Special handling for `"self"` key - values like "ref", "refmut", "value" are borrow modes, not types.

---

## Issue 2: "Optionusize", "Optionu32" Recognized as Missing Types

**Symptom:**
```
[DISCOVER] Found 6 types referenced in API but not defined as classes
  - Optionusize
  - Optionu32
```

**Root Cause:**
Somewhere in the code, `Option<usize>` is incorrectly parsed as `Optionusize` (without space/angle brackets). The problem likely lies in `extract_base_type()` or type normalization code that strips generics incorrectly.

**Note:** These types should not appear at all - the API uses `OptionU32`, `OptionI32`, etc. with explicit FFI-safe wrappers.

**Affected Function:** Type parsing in `extract_base_type()` or related functions

---

## Issue 3: "String, String" Discovered as a Type

**Symptom:**
```
[SEARCH] DISCOVERED TYPES (6)
  ┌─ String, String
  │  Path: azul_layout::str::String, String
```

**Root Cause:**
Tuple types `(String, String)` or comma-separated lists are incorrectly interpreted as a single type `"String, String"`.

**Affected Code:** Type-Discovery in `doc/src/autofix/mod.rs` or Type-Parsing

---

## Issue 4: "29 unused types to remove" - False Positives

**Symptom:**
```
[CLEANUP] Found 29 unused types to remove (recursive analysis)
   • callbacks: RenderImageCallback
   • clipboard: Clipboard
   • file: File
   • gl: GLboolean, GLsizei
   • option: OptionAngleValue, OptionAzString, OptionClipboard, ...
```

**Root Cause:**
The recursive unused-type analysis in `find_all_unused_types_recursive()` is flawed. It doesn't consider all usage sites:
1. Types only referenced in struct_fields/enum_fields
2. Types in generic arguments
3. Types in external paths

Many of these types ARE used but the reachability analysis misses them.

---

## Issue 5: Duplicate Path Corrections

**Symptom:**
```
[FIX] PATH CORRECTIONS (287)

  ┌─ FontCache
  │  Old: azul_dll::desktop::shell2::linux::wayland::FontCache
  │  New: azul_dll::desktop::shell2::run::FontCache
  │
  ┌─ FontCache
  │  Old: azul_dll::desktop::shell2::linux::wayland::FontCache
  │  New: azul_dll::desktop::shell2::run::FontCache
```

**Root Cause:**
The path-correction logic generates duplicates. A separate correction is created for each reference to a type, instead of deduplicating corrections.

---

## Issue 6: Wrong Path Corrections (azul_dll instead of azul_core/azul_layout)

**Symptom:**
```
┌─ WindowState
│  Old: azul_dll::desktop::shell2::common::callback_processing::WindowState
│  New: azul_dll::desktop::shell2::common::event_v2::WindowState

┌─ U32
│  Old: azul_core::gl::U32
│  New: azul_dll::python::AzU32
```

**Root Cause:**
1. `azul_dll` is not blacklisted for type discovery
2. Path resolution prefers the first found path, not the "correct" one (public API vs. internal)
3. `WindowState` should come from `azul_core::window_state`, not `azul_dll::desktop::shell2`
4. `FontCache` is not in the public API and should not be corrected

---

## Issue 7: Field Count Mismatches

**Symptom:**
```
• WARN:  WindowState: Field count mismatch (workspace: 0, API: 16)
• WARN:  SystemCallbacks: Field count mismatch (workspace: 0, API: 2)
```

**Root Cause:**
The workspace index finds a type with the same name but without fields (or a different type). This happens when:
1. The same type name exists in different crates
2. The parser doesn't correctly extract fields

---

## Priority Matrix

| Priority | Issue | Impact | Effort |
|----------|-------|--------|--------|
| HIGH | Filter "self" key in fn_args | Breaks type discovery | Low |
| HIGH | Blacklist azul_dll for type discovery | Wrong paths everywhere | Medium |
| HIGH | Fix generic type parsing (Option<usize>) | Phantom types | Medium |
| MEDIUM | Deduplicate path corrections | Noise in output | Low |
| MEDIUM | Fix unused-type analysis | False removals | High |
| MEDIUM | Prioritize azul_core/azul_layout paths | Wrong path selection | Medium |
| LOW | Fix tuple-type parsing | Minor issue | Low |
