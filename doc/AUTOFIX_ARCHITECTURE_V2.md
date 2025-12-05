# Autofix Architecture V2 - Refactoring Plan

## Problem Statement

The current autofix implementation has several architectural issues:

### 1. Duplicate Processing
```
[PATH_CORRECTION] azul_core::gl::FontCache (azul_core::resources::FontCache) @ /Users/fschutt/.../core/src/resources.rs:1234
[PATH_CORRECTION] azul_core::gl::FontCache (azul_core::resources::FontCache) @ /Users/fschutt/.../core/src/resources.rs:1234
```
Types like `FontCache` appear multiple times because each API entry is processed separately.

### 2. Incorrect Type Discovery
```
[PATH_CORRECTION] azul_text_layout::ScriptType (azul_core::resources::ScriptType) ...
```
`use` re-imports are being treated as type definitions. When a file has:
```rust
pub use azul_core::resources::ScriptType;
```
This is NOT a type definition - it's a re-export. The actual definition is in `azul_core::resources`.

### 3. Unnecessary Complexity
The "recursive virtual patch application" approach is overly complex. We don't need to simulate patches - we just need to compare "what should be" vs "what is".

---

## New Architecture

### Phase 1: Parse & Build Type Index (Parallel, Per-File)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         PHASE 1: BUILD TYPE INDEX                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐                     │
│  │  core/*.rs  │    │  css/*.rs   │    │  dll/*.rs   │   ... (parallel)    │
│  └──────┬──────┘    └──────┬──────┘    └──────┬──────┘                     │
│         │                  │                  │                             │
│         ▼                  ▼                  ▼                             │
│  ┌─────────────────────────────────────────────────────────────────┐       │
│  │                    syn::parse_file()                            │       │
│  └─────────────────────────────────────────────────────────────────┘       │
│         │                  │                  │                             │
│         ▼                  ▼                  ▼                             │
│  ┌─────────────────────────────────────────────────────────────────┐       │
│  │              extract_type_definitions()                          │       │
│  │  - ONLY Item::Struct, Item::Enum, Item::Type                    │       │
│  │  - SKIP Item::Use (re-exports are NOT definitions)              │       │
│  │  - Resolve impl_vec!, impl_option!, impl_vec_debug! macros      │       │
│  └─────────────────────────────────────────────────────────────────┘       │
│         │                  │                  │                             │
│         ▼                  ▼                  ▼                             │
│  ┌─────────────────────────────────────────────────────────────────┐       │
│  │                    TypeDefinition                                │       │
│  │  {                                                               │       │
│  │    full_path: "azul_core::resources::FontCache",                │       │
│  │    type_name: "FontCache",                                       │       │
│  │    file_path: PathBuf,                                           │       │
│  │    kind: Struct { fields, has_repr_c, generics, derives },      │       │
│  │    generated_types: Vec<GeneratedType>,  // from impl_vec! etc  │       │
│  │  }                                                               │       │
│  └─────────────────────────────────────────────────────────────────┘       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    ┌─────────────────┐
                    │   TypeIndex     │
                    │  HashMap<Name,  │
                    │   Vec<TypeDef>> │
                    └─────────────────┘
```

#### Key Changes:
- **Filter at parse time**: Don't store anything from `Item::Use`
- **Resolve macros immediately**: When we see `impl_vec!(Foo, FooVec, FooVecDestructor)`, generate:
  - `FooVec` with `{ ptr: *const Foo, len: usize, cap: usize, destructor: FooVecDestructor }`
  - `FooVecDestructor` with callback type
- **One pass, parallel**: Each file is processed independently

---

### Phase 2: Resolve Expected API Types (Parallel, Per-Function)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 2: RESOLVE EXPECTED TYPES                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  For each function in workspace:                                            │
│                                                                             │
│  fn create_window(options: WindowCreateOptions) -> Window                   │
│         │                      │                      │                     │
│         ▼                      ▼                      ▼                     │
│  ┌─────────────────────────────────────────────────────────────────┐       │
│  │              resolve_type_chain(type_name, index)               │       │
│  │                                                                  │       │
│  │  "WindowCreateOptions"                                           │       │
│  │       └─> azul_core::window::WindowCreateOptions                │       │
│  │              └─> fields: { size: LogicalSize, ... }             │       │
│  │                     └─> azul_core::geom::LogicalSize            │       │
│  │                            └─> fields: { width: f32, height: f32 }      │
│  │                                   └─> PRIMITIVE (stop)          │       │
│  └─────────────────────────────────────────────────────────────────┘       │
│                                                                             │
│  Output: Set<ResolvedType>                                                  │
│  {                                                                          │
│    "azul_core::window::WindowCreateOptions",                               │
│    "azul_core::geom::LogicalSize",                                         │
│    ...                                                                      │
│  }                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Resolution Rules:
1. Look up type name in `TypeIndex`
2. If multiple matches, prefer:
   - Same crate > different crate
   - `pub` > private
   - Definition > macro-generated
3. For each field/variant, recursively resolve
4. Stop at primitives: `u8`, `i32`, `f32`, `bool`, `*const T`, `*mut T`, `usize`, etc.

---

### Phase 3: Resolve Current API Types

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 3: RESOLVE CURRENT API                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Read current api.json                                                      │
│                                                                             │
│  For each type in api.json:                                                 │
│    - Try to resolve using same TypeIndex                                    │
│    - Mark as FOUND or MISSING                                               │
│    - Track resolution path                                                  │
│                                                                             │
│  Output: Map<TypeName, ResolutionResult>                                    │
│  {                                                                          │
│    "WindowCreateOptions": Found("azul_core::window::WindowCreateOptions"), │
│    "OldRemovedType": Missing,                                               │
│    "FontCache": Found("azul_core::resources::FontCache"),  // NOT gl::     │
│  }                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

### Phase 4: Generate Diff & Patches

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 4: DIFF & PATCH GENERATION                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Compare:                                                                   │
│    EXPECTED (from workspace)  vs  CURRENT (from api.json)                  │
│                                                                             │
│  ┌──────────────────────┐         ┌──────────────────────┐                 │
│  │ Expected Types       │         │ Current Types        │                 │
│  │                      │         │                      │                 │
│  │ FontCache @          │         │ FontCache @          │                 │
│  │ core::resources      │   !=    │ core::gl             │  → PATH_FIX     │
│  │                      │         │                      │                 │
│  │ NewType @ core::dom  │   ∉     │ (missing)            │  → ADD_TYPE     │
│  │                      │         │                      │                 │
│  │ (missing)            │   ∉     │ OldType @ core::old  │  → REMOVE_TYPE  │
│  └──────────────────────┘         └──────────────────────┘                 │
│                                                                             │
│  Deduplication:                                                             │
│    - Use HashSet<(TypeName, NewPath)> to prevent duplicate patches         │
│    - Group patches by type for cleaner output                               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Data Structures

### TypeDefinition (Phase 1 Output)

```rust
/// A type definition discovered from parsing source files.
/// This is NOT created for `use` re-exports - only actual definitions.
pub struct TypeDefinition {
    /// Full path: "azul_core::resources::FontCache"
    pub full_path: String,
    /// Simple name: "FontCache"
    pub type_name: String,
    /// Source file where defined
    pub file_path: PathBuf,
    /// Kind of type with all metadata
    pub kind: TypeDefKind,
    /// Types generated by macros (impl_vec!, impl_option!, etc.)
    pub generated_types: Vec<GeneratedType>,
}

pub enum TypeDefKind {
    Struct {
        fields: IndexMap<String, FieldDef>,
        has_repr_c: bool,
        generic_params: Vec<String>,
        derives: Vec<String>,
    },
    Enum {
        variants: IndexMap<String, VariantDef>,
        has_repr_c: bool,
        generic_params: Vec<String>,
        derives: Vec<String>,
    },
    TypeAlias {
        target: String,
    },
    CallbackTypedef {
        args: Vec<CallbackArg>,
        returns: Option<String>,
    },
}

/// A type generated by a macro invocation
pub struct GeneratedType {
    /// e.g., "FooVec" from impl_vec!(Foo, FooVec, ...)
    pub type_name: String,
    /// The macro that generated it
    pub source_macro: String,
    /// Synthetic fields/structure
    pub kind: TypeDefKind,
}
```

### TypeIndex (Phase 1 Output)

```rust
/// Fast lookup index for type definitions
pub struct TypeIndex {
    /// Map from simple type name to all definitions with that name
    by_name: HashMap<String, Vec<TypeDefinition>>,
    /// Map from full path to definition
    by_path: HashMap<String, TypeDefinition>,
}

impl TypeIndex {
    /// Find the best match for a type name
    pub fn resolve(&self, type_name: &str, context: &ResolutionContext) -> Option<&TypeDefinition>;
    
    /// Check if a type is primitive (no resolution needed)
    pub fn is_primitive(type_name: &str) -> bool;
}
```

### ResolvedTypeSet (Phase 2/3 Output)

```rust
/// All types reachable from a set of entry points
pub struct ResolvedTypeSet {
    /// Types successfully resolved with their full paths
    pub resolved: HashMap<String, ResolvedType>,
    /// Types that could not be resolved
    pub unresolved: HashSet<String>,
}

pub struct ResolvedType {
    pub full_path: String,
    pub definition: TypeDefinition,
    /// How we got here (for debugging)
    pub resolution_chain: Vec<String>,
}
```

### ApiDiff (Phase 4 Output)

```rust
pub struct ApiDiff {
    /// Types that need path corrections
    pub path_fixes: Vec<PathFix>,
    /// Types to add to api.json
    pub additions: Vec<TypeDefinition>,
    /// Types to remove from api.json
    pub removals: Vec<String>,
    /// Field/variant changes within types
    pub modifications: Vec<TypeModification>,
}

pub struct PathFix {
    pub type_name: String,
    pub old_path: String,
    pub new_path: String,
    /// Dedupe key to prevent duplicates
    pub key: String,  // format!("{}:{}", type_name, new_path)
}
```

---

## Implementation Plan

### Step 1: Create `TypeDefinition` and `TypeIndex` (New Module)

**File**: `doc/src/autofix/type_index.rs`

```rust
// 1. Define TypeDefinition, TypeDefKind, GeneratedType
// 2. Define TypeIndex with by_name and by_path maps
// 3. Implement extract_types_from_file() that:
//    - Parses with syn
//    - Skips Item::Use entirely
//    - Processes Item::Struct, Item::Enum, Item::Type
//    - Detects and expands impl_vec!, impl_option!, impl_vec_debug!
// 4. Implement TypeIndex::build() that processes files in parallel
```

**Unit Tests**:
```rust
#[test]
fn test_skips_use_imports() {
    let source = r#"
        use azul_core::resources::FontCache;
        pub use other::Thing;
    "#;
    let types = extract_types_from_source(source, "test.rs");
    assert!(types.is_empty());  // No types from use statements
}

#[test]
fn test_extracts_struct_definition() {
    let source = r#"
        #[repr(C)]
        pub struct FontCache {
            pub fonts: FontVec,
        }
    "#;
    let types = extract_types_from_source(source, "test.rs");
    assert_eq!(types.len(), 1);
    assert_eq!(types[0].type_name, "FontCache");
}

#[test]
fn test_expands_impl_vec_macro() {
    let source = r#"
        pub struct Font { pub data: u8 }
        impl_vec!(Font, FontVec, FontVecDestructor);
    "#;
    let types = extract_types_from_source(source, "test.rs");
    // Should have: Font, FontVec, FontVecDestructor
    assert_eq!(types.len(), 3);
}
```

### Step 2: Create Type Resolution Logic

**File**: `doc/src/autofix/type_resolver.rs`

```rust
// 1. Define ResolutionContext (current crate, current module, etc.)
// 2. Implement resolve_type_chain() that:
//    - Looks up type in index
//    - Recursively resolves fields/variants
//    - Tracks visited types to prevent cycles
//    - Stops at primitives
// 3. Implement is_primitive() check
// 4. Implement resolve_all_from_functions() for workspace functions
```

### Step 3: Create Diff Generator

**File**: `doc/src/autofix/diff.rs`

```rust
// 1. Load current api.json types
// 2. Resolve each type using TypeIndex
// 3. Compare with expected types from workspace
// 4. Generate ApiDiff with deduplication
```

### Step 4: Refactor `autofix_api_recursive`

**File**: `doc/src/autofix/mod.rs`

```rust
pub fn autofix_api_recursive(workspace_root: &Path, verbosity: Verbosity) -> Result<()> {
    // STEP 1: Build TypeIndex (parallel, per-file)
    let index = TypeIndex::build(workspace_root, verbosity)?;
    
    // STEP 2: Resolve expected types from workspace functions
    let expected = resolve_expected_types(&index, workspace_root)?;
    
    // STEP 3: Resolve current api.json types  
    let current = resolve_current_api_types(&index, api_json)?;
    
    // STEP 4: Generate diff
    let diff = generate_diff(&expected, &current);
    
    // STEP 5: Apply patches (or output report)
    apply_diff(&diff, api_json)?;
    
    Ok(())
}
```

---

## Key Differences from Current Implementation

| Aspect | Current | New |
|--------|---------|-----|
| Use imports | Filtered in score calculation | Filtered at parse time |
| Macro expansion | Handled during resolution | Handled during indexing |
| Deduplication | None (causes duplicates) | HashSet-based dedup |
| Resolution | Virtual patch application | Direct type resolution |
| Parallelism | Some | Full (per-file parsing, per-function resolution) |
| Complexity | High (recursive patching) | Low (parse → resolve → diff) |

---

## Implementation Status (Current)

### ✅ Completed

1. **`type_index.rs`** - Parallel type indexing
   - Parses 261 Rust files in parallel with rayon
   - Indexes 2075 unique type names, 2374 total paths
   - Filters `use` imports at parse time (Item::Use skipped)
   - Expands `impl_vec!`, `impl_option!`, `impl_callback!` macros
   - Extracts struct fields, enum variants, type alias targets
   - 8 unit tests passing

2. **`type_resolver.rs`** - Recursive type chain resolution
   - Resolves types from function signatures
   - Handles nested generics (e.g., `Option<Vec<Foo>>`)
   - Cycle detection with `ResolutionContext`
   - 2 unit tests passing

3. **`diff.rs`** - API diff generation
   - Compares workspace types vs api.json types
   - Generates path fixes, additions, removals
   - HashMap-based deduplication
   - 3 unit tests passing

4. **`debug.rs`** - Debug CLI commands
   - `autofix debug type <name>` - Look up type in index
   - `autofix debug chain <name>` - Trace type chain resolution  
   - `autofix debug api <name>` - Check type in api.json
   - `autofix debug file <path>` - Parse single file
   - 6 unit tests passing

5. **CLI Integration**
   - `cargo run -p azul-doc -- autofix v2` runs new system
   - Generates patch files to `target/autofix/autofix_v2/`

### 📊 Current Results

```
[TypeIndex] Found 261 Rust files to parse
[TypeIndex] Indexed 2075 unique type names, 2374 total paths
[TypeResolver] Found 531 public functions
[TypeResolver] Resolved 549 types, 165 unresolved
[Diff] API: 958 found, 101 missing, 1 mismatches

Path fixes needed: 1
  - CheckBoxOnToggle : azul_dll::CheckBoxOnToggle → azul_layout::widgets::check_box::CheckBoxOnToggle

Types to remove: 101
```

### 🔍 Removal Categories

The 101 "removals" fall into these categories:

1. **Renamed Types** (api.json name differs from code):
   - `StyleBorderTopColor` → `StyleBorderTopColorValue`
   - `NodeGraphOnNodeFieldEditedCallbackType` → `OnNodeFieldEditedCallbackType`
   
2. **Actually Deleted Types**:
   - `InlineGlyph`, `InlineLine`, `InlineWord`
   - `FontCache`, `Clipboard`
   
3. **Special Cases**:
   - `refmut` (lowercase module, not a type)
   - `[u8;4]` (array syntax in api.json)
   - `String:azul_css::corety::AzString` (name mismatch: String vs AzString)

### ❌ Not Yet Implemented

1. **Type Renaming Detection**: When `api.json` has `StyleBorderTopColor` but code has `StyleBorderTopColorValue`, detect this as a rename, not a removal

2. **Patch Generation**: Currently only generates path fix patches, not field/variant modification patches

3. **api.json Updates**: Patches are generated but not automatically applied

---

## Files to Modify/Create

### New Files:
- `doc/src/autofix/type_index.rs` - TypeDefinition, TypeIndex, extraction
- `doc/src/autofix/type_resolver.rs` - Type resolution logic
- `doc/src/autofix/diff.rs` - Diff generation

### Files to Modify:
- `doc/src/autofix/mod.rs` - Simplified main flow
- `doc/src/patch/index.rs` - Can be simplified or removed after migration

### Files Unaffected:
- `doc/src/codegen/*` - Code generation unchanged
- `doc/src/codegen/memtest.rs` - Memory test generation unchanged
- `doc/src/codegen/struct_gen.rs` - Struct generation unchanged

---

## Success Criteria

1. **No duplicates**: Each type appears at most once in output
2. **Correct discovery**: `use` re-exports are never treated as definitions
3. **Faster execution**: Parallel processing throughout
4. **Simpler code**: No "virtual patch application" complexity
5. **Unit testable**: Each component can be tested in isolation
6. **Same output**: Final api.json should be equivalent (but generated more cleanly)

---

## Migration Strategy

1. Implement new `type_index.rs` alongside existing code
2. Add unit tests for use-import filtering
3. Implement `type_resolver.rs` with tests
4. Implement `diff.rs` with tests
5. Create new `autofix_api_v2()` function using new components
6. Compare output of old vs new
7. Once validated, replace old implementation
8. Remove unused code from `patch/index.rs`
