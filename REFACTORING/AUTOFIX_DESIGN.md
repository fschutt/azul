# Autofix Command Design

## Overview

The `autofix` command automatically discovers missing types from the API definition and generates patches to add them with correct paths and metadata.

## Process Flow

### Phase 0: Initialization (Upfront Work)
- Load `api.json`
- **Compile all regex patterns** (type normalization, path parsing, etc.)
- **Build complete workspace index** (scan and parse all Rust source files)
- Collect all types currently defined in the API
- Collect all types referenced in the API (from function signatures, fields, etc.)

### 1. Initial Analysis
- Identify missing types (referenced but not defined)

### 2. Recursive Type Discovery (First Pass)
For each missing type:
1. Look up the type in the workspace index
2. Verify it has `#[repr(C)]` layout (FFI-safe)
3. Verify it's from the azul workspace (not external crates)
4. Analyze the type's fields/variants to find dependencies
5. Add newly discovered dependency types to the discovery queue
6. Continue until no more types are discovered (or max iterations reached)

**Track why each type was added:**
- "Referenced in API function `Foo::bar`"
- "Field `x` in struct `Foo`"
- "Variant `Some` in enum `Option<T>`"
- "Type alias target in `TypeAlias`"

### 3. Virtual Patch Application (In-Memory)
- Generate patches for all discovered types
- Apply patches to api.json **in memory only** (create a modified copy)
- This allows the next phase to see what the API would look like with the changes

### 4. Recursive Type Discovery (Second Pass)
- Using the virtually-patched API data (with new types added)
- Discover any additional types that were missed because they were dependencies of newly added types
- This enables truly recursive discovery
- Track the dependency chain for reporting

### 5. Existing Type Validation
For each type already in the API:
1. Look up in workspace index
2. Check if the external path has changed
3. Check if fields/variants have changed
4. Generate patches for any discrepancies

### 6. Compiler Oracle Verification (Optional)
- Generate a temporary `lib.rs` file that imports all discovered types
- Run `rustc` to check for compilation errors
- Parse compiler output to find any path corrections needed
- This catches edge cases where the workspace index might be wrong

### 7. Patch Generation
For each discovered type or change:
1. Create a JSON patch file in `target/autofix/patches/`
2. Include complete metadata:
   - Type name and path
   - Fields/variants with types and documentation
   - Reason for addition/change

### 8. Summary Report
Print a comprehensive report showing:
- **Statistics:**
  - Number of types discovered
  - Number of paths corrected
  - Number of field updates
  
- **Per-Type Details:**
  - Type name
  - **Why it was added** (dependency chain)
  - What changed (if existing type)
  - Snippet of the patch
  
- **Dependency Chains:**
  ```
  Function `Window::create` references `WindowCreateOptions`
  └─ Field `icon` has type `WindowIcon` ← needs to be added
     └─ Field `data` has type `IconData` ← needs to be added
        └─ Field `pixels` has type `RgbaColor` ← needs to be added
  ```

- **Next Steps:**
  - Review generated patches
  - Apply with `azul-docs patch target/autofix/patches`

## Output Format

### Initialization (Immediate)

```
🔍 Initializing autofix...
   • Loading api.json
   • Building workspace index
   • Compiling regexes
   • Indexing 56 files...
   
✓ Initialization complete (2.3s)
  • Found 1,234 types in workspace
  • Found 89 types in API
  • Ready for analysis

🔄 Running analysis (this may take a moment)...
```

### Final Report (After Completion)

```
✅ Analysis complete (5.2s)

╔══════════════════════════════════════════════════════════════╗
║                     AUTOFIX SUMMARY                           ║
╚══════════════════════════════════════════════════════════════╝

📊 Statistics:
   • Types discovered: 5
   • Paths corrected: 2
   • Field updates: 0

🔍 Discovered Types:

┌─ WindowCreateOptions
│  Why: Referenced in API function `Window::create`
│  Path: azul_core::window::WindowCreateOptions
│  Fields: 5 fields, 3 are public
│  
├─ WindowIcon
│  Why: Field `icon` in struct `WindowCreateOptions`
│  Path: azul_core::window::WindowIcon
│  Fields: 2 fields, 2 are public
│  
├─ IconData
│  Why: Field `data` in struct `WindowIcon`
│  Path: azul_core::window::IconData
│  Fields: 3 fields, 3 are public
│  
└─ RgbaColor
   Why: Field `pixels` in struct `IconData`
   Path: azul_core::app::RgbaColor
   Note: Also used in 2 other locations

🔧 Path Corrections:

┌─ LayoutRect
│  Old: azul_core::layout::LayoutRect
│  New: azul_core::dom::LayoutRect
│  Reason: Module moved in workspace
│  
└─ CallbackInfo
   Old: azul_core::callbacks::CallbackInfo
   New: azul_core::dom::CallbackInfo
   Reason: Module moved in workspace

💡 Next Steps:
   1. Review patches: ls target/autofix/patches/
   2. Apply patches: azul-docs patch target/autofix/patches
   3. Verify changes: git diff api.json

📁 Patches saved to: target/autofix/patches/
```

## Implementation Notes

### Key Data Structures

#### Message Types (Enum-based)

```rust
pub enum AutofixMessage {
    // Discovery phase
    TypeDiscovered { type_name: String, path: String, reason: TypeOrigin },
    TypeSkipped { type_name: String, reason: SkipReason },
    TypeNotFound { type_name: String },
    
    // Validation phase
    PathChanged { type_name: String, old_path: String, new_path: String },
    FieldAdded { type_name: String, field_name: String, field_type: String },
    FieldRemoved { type_name: String, field_name: String },
    FieldTypeChanged { type_name: String, field_name: String, old_type: String, new_type: String },
    
    // Warnings
    ExternalCrateType { type_name: String, crate_name: String },
    MissingReprC { type_name: String },
    CycleDetected { type_name: String },
    MaxIterationsReached { iteration: usize },
    
    // Errors
    WorkspaceIndexFailed { path: String, error: String },
    PatchGenerationFailed { type_name: String, error: String },
}

impl std::fmt::Display for AutofixMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::TypeDiscovered { type_name, path, reason } => {
                write!(f, "✓ Discovered {} at {}\n  Reason: {}", type_name, path, reason)
            }
            Self::TypeSkipped { type_name, reason } => {
                write!(f, "⊘ Skipped {}: {}", type_name, reason)
            }
            // ... etc
        }
    }
}

pub enum SkipReason {
    ExternalCrate(String),
    MissingReprC,
    AlreadyInApi,
    CallbackTypedef,
    AlreadyVisited,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::ExternalCrate(name) => write!(f, "external crate '{}'", name),
            Self::MissingReprC => write!(f, "missing #[repr(C)]"),
            Self::AlreadyInApi => write!(f, "already in API"),
            Self::CallbackTypedef => write!(f, "is callback typedef"),
            Self::AlreadyVisited => write!(f, "already visited (cycle)"),
        }
    }
}
```

#### Other Data Structures

- `TypeOrigin`: Tracks why a type was discovered (API reference, struct field, enum variant, etc.)
  - **Enhancement:** Include full parent chain, not just immediate parent
- `AutofixMessages`: Collects typed messages during execution
  - Messages stored as enums
  - Can be filtered by variant
  - Display implementation for pretty printing
- `PatchSummary`: Accumulates statistics about changes made
- `ClassAdded`: Records a newly discovered type with its origin chain
- `ExternalPathChange`: Records path corrections for existing types
- `CompiledRegexes`: Pre-compiled regex patterns used throughout analysis

### Message Levels

Messages are typed enums, not strings:

```rust
pub enum AutofixMessage {
    // Info-level messages (successful operations)
    TypeDiscovered { ... },
    PathChanged { ... },
    
    // Warning-level messages (non-fatal issues)
    TypeSkipped { ... },
    ExternalCrateType { ... },
    MissingReprC { ... },
    
    // Error-level messages (fatal issues)
    WorkspaceIndexFailed { ... },
    PatchGenerationFailed { ... },
}

impl AutofixMessage {
    pub fn level(&self) -> MessageLevel {
        match self {
            Self::TypeDiscovered { .. } | Self::PathChanged { .. } => MessageLevel::Info,
            Self::TypeSkipped { .. } | Self::ExternalCrateType { .. } => MessageLevel::Warning,
            Self::WorkspaceIndexFailed { .. } | Self::PatchGenerationFailed { .. } => MessageLevel::Error,
        }
    }
}

pub enum MessageLevel {
    Info,
    Warning,
    Error,
}
```

This allows filtering messages programmatically:
```rust
// Get only warnings
let warnings: Vec<_> = messages.iter()
    .filter(|m| m.level() == MessageLevel::Warning)
    .collect();

// Get specific message types
let type_discoveries: Vec<_> = messages.iter()
    .filter_map(|m| match m {
        AutofixMessage::TypeDiscovered { type_name, .. } => Some(type_name),
        _ => None,
    })
    .collect();
```

### Execution Flow

1. **Silent execution** - No output during analysis (except initialization status)
2. **Collect all messages** - Store typed enum messages during execution
3. **Generate comprehensive report** - After completion, print full structured report
4. **Report sections:**
   - Statistics summary
   - Discovery results (with dependency trees)
   - Path corrections
   - Warnings (grouped by type)
   - Errors (if any)
   - Next steps

### Regex Compilation

All regexes compiled once at initialization:

```rust
pub struct CompiledRegexes {
    pub raw_pointer: Regex,          // *const T, *mut T
    pub generic_type: Regex,         // Vec<T>, Option<T>, etc.
    pub path_separator: Regex,       // ::
    pub type_normalization: Regex,   // Spacing in types
    pub doc_comment: Regex,          // /// or //!
    // ... etc
}

impl CompiledRegexes {
    pub fn new() -> Result<Self> {
        Ok(Self {
            raw_pointer: Regex::new(r"\*\s*(const|mut)\s+")?,
            generic_type: Regex::new(r"(\w+)\s*<\s*(.+?)\s*>")?,
            // ... etc
        })
    }
}
```

These are passed through the analysis pipeline to avoid recompilation.

### Workspace Loading

The workspace index is fully built before analysis:

```rust
pub struct WorkspaceIndex {
    pub types: HashMap<String, Vec<ParsedTypeInfo>>,  // All types found
    pub files: Vec<PathBuf>,                          // All files scanned
    pub crate_names: HashMap<PathBuf, String>,        // File -> crate mapping
    pub regexes: Arc<CompiledRegexes>,                // Shared regexes
}

impl WorkspaceIndex {
    pub fn build(project_root: &Path) -> Result<Self> {
        // Compile regexes first
        let regexes = Arc::new(CompiledRegexes::new()?);
        
        // Discover all Rust files
        let files = discover_rust_files(project_root)?;
        
        // Parse all files in parallel (optional optimization)
        let types = parse_all_files(&files, &regexes)?;
        
        // Build crate mapping
        let crate_names = build_crate_mapping(project_root, &files)?;
        
        Ok(Self { types, files, crate_names, regexes })
    }
}
```

All parsing happens upfront, so the analysis phase only needs to query the index.
