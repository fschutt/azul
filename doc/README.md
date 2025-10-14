# azul-doc - API Documentation & Validation Tool

`azul-doc` is a comprehensive tool for managing, validating, and documenting the Azul GUI framework API. It provides automated source code analysis, API discovery, and validation workflows designed for both human developers and LLM agents.

## Features

### 1. **API Discovery** (`print` command)
Navigate and inspect the API definition from `api.json`:

```bash
# Print all modules
azul-doc print

# Print all classes in a module
azul-doc print app

# Print detailed information about a specific class
azul-doc print app.App

# Print a specific function/method
azul-doc print app.App.new
```

**What it shows:**
- API definition from `api.json`
- Documentation strings
- Constructor and method signatures
- Import paths (e.g., `crate::azul_impl::app::AzAppPtr`)
- **Source code retrieval** from actual Rust files
- **Validation status** (whether definition matches source)

### 2. **API Patching** (`patch` command)
Update `api.json` with changes defined in patch files:

```bash
azul-doc patch patches/fix_app_struct.patch
```

**Patch file format** (JSON):
```json
{
  "module": "app",
  "class": "App",
  "operation": "update_field",
  "field": "struct_fields",
  "value": {
    "window_count": {
      "type": "usize",
      "doc": "Number of windows in the application"
    }
  }
}
```

**Supported operations:**
- `add_class`: Add new class to module
- `update_field`: Update any field of a class (constructors, functions, struct_fields, enum_fields, etc.)
- `delete_class`: Remove a class
- `add_module`: Create new module
- `delete_module`: Remove entire module

**Patchable fields:**
- `external` - Import path
- `struct_fields` - Struct field definitions
- `enum_fields` - Enum variant definitions
- `constructors` - Constructor methods
- `functions` - Instance/static methods
- `constants` - Associated constants
- `callback_typedef` - Callback type definitions
- `destructor` - Destructor information

### 3. **Source Code Retrieval**
Automatically locates and retrieves Rust source code for any API item:

**How it works:**
1. Parses the entire workspace using `syn` (Rust syntax parser)
2. Builds a symbol table with line number spans
3. Uses `cargo metadata` to resolve cross-crate dependencies
4. Extracts exact source code for structs, enums, functions
5. Compares with `api.json` definitions

**Dependencies:**
- `syn 2.0.101` - Full Rust AST parsing
- `proc-macro2` with `span-locations` feature - Line number extraction
- `cargo_metadata` - Workspace and dependency resolution
- `ignore` - Gitignore-aware file traversal

### 4. **Exit Codes**
Designed for automated workflows:
- **0**: Success, no errors found
- **1**: Validation errors detected (definition differs from source)

## LLM Workflow: The Fix-Source Loop

This tool is designed to enable an **iterative validation and patching workflow** for LLMs:

### Step 1: Discover API Discrepancies

```bash
azul-doc print app.App
```

**Example output:**
```
📦 Class: app.App

🔗 Import Path:
  crate::azul_impl::app::AzAppPtr

📂 Source Location:
```rust
pub struct AzAppPtr {
    pub window_count: usize,
    pub monitors: Vec<Monitor>,
    pub config: AppConfig,
}
```

🔍 Validation:
  ⚠️  Definition differs from source

❌ Found errors in class 'app.App'
```

### Step 2: Analyze the Difference

The LLM should:
1. Compare the retrieved source with the `api.json` definition
2. Identify missing fields (`window_count` is missing in `api.json`)
3. Check if field types match
4. Verify function signatures match actual implementations

### Step 3: Create a Patch

Generate a patch file `fix_app_struct.patch`:

```json
{
  "module": "app",
  "class": "App",
  "operation": "update_field",
  "field": "struct_fields",
  "value": {
    "window_count": {
      "type": "usize",
      "doc": "Number of windows in the application"
    },
    "monitors": {
      "type": "Vec<azul::Monitor>",
      "doc": "Available monitors"
    },
    "config": {
      "type": "azul::AppConfig",
      "doc": "Application configuration"
    }
  }
}
```

### Step 4: Apply the Patch

```bash
azul-doc patch fix_app_struct.patch
```

**Expected output:**
```
✅ Successfully patched api.json
```

### Step 5: Re-Validate

```bash
azul-doc print app.App
```

**Expected output:**
```
🔍 Validation:
  ✅ Definition matches source

✅ Class 'app.App' is valid
```

**Exit code 0** = Success! Continue to next class.

### Step 6: Iterate

```bash
# Find all classes with missing external paths
azul-doc print | grep "external: None"

# Process each one:
for class in $(azul-doc print | grep "external: None" | cut -d' ' -f2); do
    echo "Processing $class"
    azul-doc print $class
    # LLM creates patch...
    azul-doc patch fix_$class.patch
    azul-doc print $class
done
```

## Memory Layout Testing (Future Feature)

Generate a test crate to validate that `api.json` definitions match actual memory layouts:

```bash
azul-doc memtest generate
```

**Generates:**
```
target/memtest/
  ├── Cargo.toml (minimal dependencies)
  └── src/
      └── lib.rs (memory layout tests)
```

**Example generated test:**
```rust
#[test]
fn test_app_memory_layout() {
    use azul::app::App;
    use api_generated::app::App as ApiApp;
    
    assert_eq!(
        std::mem::size_of::<App>(),
        std::mem::size_of::<ApiApp>(),
        "Size mismatch for app.App"
    );
    
    assert_eq!(
        std::mem::align_of::<App>(),
        std::mem::align_of::<ApiApp>(),
        "Alignment mismatch for app.App"
    );
}
```

**Workflow:**
1. Generate test crate from `api.json`
2. Run `cargo test` in `target/memtest/`
3. Failures indicate memory layout mismatches
4. LLM patches `api.json` with correct field types/ordering
5. Re-generate and re-test until all pass

## Common Error Patterns

### 1. Missing External Path
```
⚠️  No external path defined
```
**Fix:** Add `external` field pointing to actual Rust type:
```json
{
  "operation": "update_field",
  "field": "external",
  "value": "crate::module::StructName"
}
```

### 2. Field Type Mismatch
```
⚠️  Definition differs from source
```
**Fix:** Update `struct_fields` or `enum_fields` to match actual source

### 3. Missing Function Arguments
```
⚠️  Signature differs from source
```
**Fix:** Update `fn_args` array in function definition

### 4. Cargo.toml Parse Error
```
⚠️  Failed to retrieve source: package.name not found
```
**Fix:** Ensure workspace root has valid `Cargo.toml` with `[package]` section

## Architecture

### Parser (`doc/src/patch/parser.rs`)
- Uses `syn::visit::Visit` trait to traverse Rust AST
- Collects symbols: structs, enums, functions, traits, constants
- Stores line number spans using `proc-macro2::Span::start().line`
- Builds `HashMap<String, SymbolInfo>` for fast lookup

### Source Locator (`doc/src/patch/locatesource.rs`)
- Resolves qualified names (e.g., `crate::module::Struct`)
- Handles cross-crate dependencies via `cargo metadata`
- Extracts exact source code for specific items
- Uses `syn::parse_file` to re-parse and extract items

### Print Command (`doc/src/print_cmd.rs`)
- Hierarchical navigation: module → class → function
- Source retrieval integration
- Validation logic
- Error detection and exit code setting

### Patch System (`doc/src/patch/mod.rs`)
- JSON-based patch format
- Supports all `ClassData` fields
- Atomic updates (reads → modifies → writes)
- Validation before and after patching

## Installation

```bash
cd doc
cargo build --release
./target/release/azul-docs --help
```

## Examples

### Full Validation Workflow

```bash
# 1. Find all classes
azul-doc print > api_overview.txt

# 2. Check specific class
azul-doc print app.App

# 3. If errors found, create patch
cat > fix_app.patch << 'EOF'
{
  "module": "app",
  "class": "App",
  "operation": "update_field",
  "field": "external",
  "value": "crate::azul_impl::app::AzAppPtr"
}
EOF

# 4. Apply patch
azul-doc patch fix_app.patch

# 5. Verify fix
azul-doc print app.App
echo "Exit code: $?"  # Should be 0
```

### Batch Processing

```bash
#!/bin/bash
# Process all classes in a module
MODULE="app"

for class in $(azul-doc print $MODULE | grep "📦" | cut -d' ' -f3); do
    echo "Checking $MODULE.$class..."
    
    if ! azul-doc print $MODULE.$class; then
        echo "ERROR in $MODULE.$class - needs patching"
        # LLM generates patch here...
    fi
done
```

## Future Enhancements

1. **Interactive Mode**: TUI for browsing API and applying patches
2. **Diff Visualization**: Show exact differences between api.json and source
3. **Auto-Patch Generation**: Suggest patches based on source analysis
4. **Cross-Reference Validation**: Verify type references exist
5. **Documentation Coverage**: Report missing doc strings
6. **Breaking Change Detection**: Compare API versions

## License

MIT - See LICENSE file in repository root.