# AUTOFIX REFACTORING PLAN

Generated: 2025-12-04

## Executive Summary

The `azul-doc autofix` command has grown organically and accumulated significant technical debt. The core issues stem from:

1. **Data Model Mismatch**: The in-memory representation doesn't match the JSON structure
2. **No Clear Separation of Concerns**: Type parsing, discovery, validation, and patching are intertwined
3. **Blacklist/Whitelist Confusion**: No clear rules for which crates/types should be processed
4. **Duplicate Detection Failures**: Same operation performed multiple times without deduplication

This document proposes a systematic refactoring approach.

---

## Architecture Analysis

### Current Module Structure

```
doc/src/autofix/
├── mod.rs          # Main entry point, orchestration (527 lines)
├── discover.rs     # Compiler oracle for type discovery (967 lines)
├── message.rs      # Message/warning types
├── regexes.rs      # Pre-compiled regex patterns
├── utils.rs        # Utility functions
└── workspace.rs    # Workspace indexing, type lookup (1756 lines)
```

### Core Problems

#### 1. Data Model Issues

**Problem**: `FunctionData.fn_args` is `Vec<IndexMap<String, String>>` but the JSON has mixed structures:

```json
// Self reference:
{ "self": "ref" }

// Normal argument:
{ "window": "WindowCreateOptions", "doc": "..." }
```

**Impact**: The parser interprets "ref", "refmut", "value" as type names.

**Solution**: Define a proper `FunctionArg` struct:

```rust
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum FunctionArg {
    SelfRef { 
        #[serde(rename = "self")]
        self_ref: String  // "ref", "refmut", "value"
    },
    Named {
        #[serde(flatten)]
        args: IndexMap<String, String>,
        #[serde(default)]
        doc: Option<String>,
    }
}
```

#### 2. Crate Priority/Blacklist Issues

**Problem**: No clear hierarchy for which crate's definition takes precedence.

**Current Behavior**:
- `azul_dll` types are discovered even though we want to exclude them
- When multiple definitions exist, the first found is used (random order)
- `WindowState` from `azul_dll::desktop::shell2` is preferred over `azul_core::window`

**Solution**: Define explicit crate priority:

```rust
const CRATE_PRIORITY: &[&str] = &[
    "azul_core",      // Highest priority - core types
    "azul_css",       // CSS types
    "azul_layout",    // Layout and widgets
    "azul_dll",       // Only for types that MUST be in DLL (e.g., App)
];

const BLACKLISTED_MODULES: &[&str] = &[
    "azul_dll::desktop::shell2",  // Internal shell implementation
    "azul_dll::python",           // Python bindings (optional)
    "azul_dll::desktop::menu_renderer", // Internal
];
```

#### 3. Type Parsing Issues

**Problem**: Multiple type-parsing functions with subtle differences:
- `extract_base_type()` - strips generics but doesn't validate
- `extract_base_type_if_not_opaque()` - skips pointer types
- `extract_base_type_including_pointers()` - includes pointer types
- `extract_types_from_*()` - family of functions that call the above

**Issues Found**:
- "Option<usize>" becomes "usize", but somewhere "Optionusize" is stored literally in api.json
- Tuple types "(String, String)" are not split correctly
- Generic bounds "T: Clone" are sometimes parsed as types

**Solution**: Create a unified `TypeParser` with clear semantics:

```rust
pub struct ParsedType {
    pub base_type: String,           // e.g., "CssPropertyValue"
    pub generic_args: Vec<String>,   // e.g., ["LayoutZIndex"]
    pub is_pointer: bool,            // *const, *mut
    pub is_reference: bool,          // &, &mut
    pub is_optional: bool,           // Option<T>
    pub is_vec: bool,                // Vec<T>
}

impl TypeParser {
    pub fn parse(type_str: &str) -> Result<ParsedType, ParseError>;
    pub fn is_primitive(type_name: &str) -> bool;
    pub fn is_ffi_safe(parsed: &ParsedType) -> bool;
}
```

#### 4. Unused Type Detection Issues

**Problem**: `find_all_unused_types_recursive()` marks many used types as unused.

**Root Causes**:
1. Only tracks types from functions/constructors as "entry points"
2. Doesn't consider:
   - Types only used in struct fields
   - Types in generic arguments
   - Types referenced via external paths
   - Types used in callbacks

**Solution**: Proper reachability analysis:

```rust
pub struct TypeUsageGraph {
    // type_name -> set of types that USE this type
    used_by: HashMap<String, HashSet<String>>,
    // type_name -> set of types that this type USES
    uses: HashMap<String, HashSet<String>>,
    // Types that are public API entry points
    entry_points: HashSet<String>,
}

impl TypeUsageGraph {
    pub fn build(api_data: &ApiData) -> Self;
    pub fn find_unreachable(&self) -> Vec<String>;
}
```

#### 5. Duplicate Output Issues

**Problem**: Path corrections are duplicated because:
1. `api_types_list` contains multiple entries for same type (from different modules)
2. Each reference generates a separate correction
3. No deduplication before output

**Solution**: Use a deduplicating data structure:

```rust
pub struct PatchSet {
    // Use BTreeMap for deterministic ordering
    path_changes: BTreeMap<(String, String), PathChange>,  // (type_name, old_path) -> change
    field_changes: BTreeMap<String, FieldChange>,          // type_name -> change
    removals: BTreeSet<String>,                            // type_name
}
```

---

## Proposed Architecture

### New Module Structure

```
doc/src/autofix/
├── mod.rs              # Entry point, orchestration only
├── types/
│   ├── mod.rs          # Type parsing and representation
│   ├── parser.rs       # Unified type parser
│   └── ffi.rs          # FFI safety checks
├── discovery/
│   ├── mod.rs          # Type discovery
│   ├── workspace.rs    # Workspace indexing
│   └── priority.rs     # Crate priority rules
├── analysis/
│   ├── mod.rs          # Analysis orchestration
│   ├── usage.rs        # Type usage graph
│   └── unused.rs       # Unused type detection
├── patches/
│   ├── mod.rs          # Patch generation
│   └── dedup.rs        # Deduplication
└── output/
    ├── mod.rs          # Output formatting
    └── messages.rs     # Warning/error messages
```

### Data Flow

```
1. LOAD
   api.json → ApiData (validated)
   
2. INDEX
   workspace files → WorkspaceIndex (with crate priority)
   
3. ANALYZE
   ApiData + WorkspaceIndex → TypeUsageGraph
   
4. DIFF
   TypeUsageGraph → PatchSet (deduplicated)
   
5. OUTPUT
   PatchSet → patch files + report
```

---

## Implementation Plan

### Phase 1: Fix Critical Bugs (1-2 days)

1. **Fix FunctionArg parsing** (Issue #1)
   - Add `SelfRef` variant handling
   - Filter out "ref", "refmut", "value" from type extraction

2. **Add crate blacklist** (Issue #6)
   - Blacklist `azul_dll::desktop::shell2`, `azul_dll::python`
   - Skip type discovery from blacklisted modules

3. **Fix Optionusize in api.json** (Issue #2)
   - This is a data issue, not code - fix the JSON directly
   - Add validation to prevent future occurrences

### Phase 2: Improve Accuracy (2-3 days)

4. **Implement crate priority** (Issue #6)
   - When multiple definitions exist, prefer by priority
   - Log when falling back to lower-priority crate

5. **Fix unused type detection** (Issue #4)
   - Build proper TypeUsageGraph
   - Include all usage sites (fields, generics, callbacks)

6. **Add deduplication** (Issue #5)
   - Deduplicate path corrections before output
   - Use deterministic ordering

### Phase 3: Refactor Architecture (3-5 days)

7. **Extract TypeParser**
   - Unified parsing with proper error handling
   - Clear semantics for each operation

8. **Restructure modules**
   - Separate concerns into new module structure
   - Reduce file sizes (target: <500 lines each)

9. **Add comprehensive tests**
   - Unit tests for type parsing
   - Integration tests for discovery
   - Regression tests for known issues

---

## Immediate Fixes (Can Be Done Now)

### Fix 1: Filter "self" from fn_args

In `doc/src/api.rs`, function `extract_types_from_function_data`:

```rust
pub fn extract_types_from_function_data(fn_data: &FunctionData) -> HashSet<String> {
    let mut types = HashSet::new();

    if let Some(return_data) = &fn_data.returns {
        types.extend(extract_types_from_return_data(return_data));
    }

    for arg_map in &fn_data.fn_args {
        for (param_name, param_type) in arg_map {
            // SKIP "self" - the value is a borrow mode, not a type
            if param_name == "self" {
                continue;
            }
            if let Some(base_type) = extract_base_type_if_not_opaque(param_type) {
                types.insert(base_type);
            }
        }
    }

    types
}
```

### Fix 2: Add crate blacklist

In `doc/src/autofix/workspace.rs`:

```rust
/// Modules that should never be used for type discovery
const BLACKLISTED_MODULES: &[&str] = &[
    "azul_dll::desktop::shell2",
    "azul_dll::python",
    "azul_dll::desktop::menu_renderer",
    "azul_dll::desktop::csd",
];

pub fn is_workspace_type(full_path: &str) -> bool {
    // Check blacklist first
    for blacklisted in BLACKLISTED_MODULES {
        if full_path.starts_with(blacklisted) {
            return false;
        }
    }
    
    // Existing logic...
}
```

### Fix 3: Deduplicate path corrections

In `doc/src/autofix/mod.rs`, before generating output:

```rust
// Deduplicate path corrections
patch_summary.external_path_changes.sort_by(|a, b| {
    (&a.class_name, &a.old_path).cmp(&(&b.class_name, &b.old_path))
});
patch_summary.external_path_changes.dedup_by(|a, b| {
    a.class_name == b.class_name && a.old_path == b.old_path && a.new_path == b.new_path
});
```

---

## Success Metrics

After refactoring:

1. **Zero false positives** in unused type detection
2. **Zero duplicate** path corrections in output
3. **No internal types** (shell2, python) in corrections
4. **Correct priority**: azul_core > azul_css > azul_layout > azul_dll
5. **All files < 500 lines**
6. **Test coverage > 80%** for type parsing
