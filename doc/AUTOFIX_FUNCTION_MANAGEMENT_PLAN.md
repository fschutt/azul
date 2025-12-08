# Autofix Function Management - Planning Document

## Executive Summary

This document outlines the architecture for three new autofix commands:
1. `autofix list <module.Type>` - List available vs registered functions
2. `autofix add <module.Type.function>` - Add function(s) to api.json
3. `autofix remove <module.Type.function>` - Remove function(s) from api.json

## Current Architecture Analysis

### What We Have

1. **TypeIndex** (`type_index.rs`) - Excellent
   - Parses workspace with syn
   - Indexes structs, enums, type aliases
   - Expands macros (impl_vec!, impl_option!, etc.)
   - Stores fields, derives, custom_impls

2. **FunctionInfo** (`type_resolver.rs`) - Basic
   - Extracts functions from `impl` blocks
   - Stores: name, full_path, self_type, parameters, return_type
   - **Problem**: Only extracts from `dll/src`, not organized by type

3. **ApiData** (`api.rs`) - Complete
   - Parses api.json
   - Has modules → classes → constructors/functions

### What's Missing

1. **Function Index by Type**
   - Current: Functions are extracted but not organized by impl target type
   - Needed: `HashMap<TypeName, Vec<FunctionDef>>` with full method details

2. **fn_body Generation**
   - Current: Manual fn_body writing
   - Needed: Auto-generate simple fn_body from method signature

3. **Dependent Type Discovery**
   - Current: TypeResolver can resolve types recursively
   - Needed: When adding a function, discover all types in args/returns

4. **Patch Generation for Functions**
   - Current: Only type patches (additions, modifications)
   - Needed: Function-level patch format

## Proposed Architecture

### Phase 1: Extend TypeIndex with Methods

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         EXTENDED TYPE INDEX                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  TypeDefinition                                                             │
│  {                                                                          │
│    full_path: "azul_core::dom::Dom",                                       │
│    type_name: "Dom",                                                        │
│    kind: Struct { fields, repr, derives, custom_impls },                   │
│    methods: Vec<MethodDef>,  // NEW: extracted from impl blocks            │
│  }                                                                          │
│                                                                             │
│  MethodDef (NEW)                                                            │
│  {                                                                          │
│    name: "add_callback",                                                    │
│    self_kind: Some(SelfKind::RefMut),  // &mut self                        │
│    args: Vec<ArgDef>,                                                       │
│    return_type: Option<TypeRef>,                                           │
│    is_constructor: bool,  // fn new() -> Self                              │
│    doc_comment: Option<String>,                                             │
│    source_code: String,  // for fn_body generation                         │
│  }                                                                          │
│                                                                             │
│  ArgDef                                                                     │
│  {                                                                          │
│    name: "callback",                                                        │
│    ty: TypeRef { base: "CallbackType", ref_kind: Value },                  │
│  }                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Phase 2: New Command Structure

```
azul-doc autofix list dom.Dom
  → Shows:
    WORKSPACE METHODS (from source):
      + new(node_type: NodeType) -> Dom           [not in api.json]
      + div() -> Dom                              [not in api.json]  
      = add_callback(&mut self, ...) -> ()        [in api.json]
      = add_child(&mut self, child: Dom) -> ()    [in api.json]
      
    API.JSON METHODS (from api.json):
      - old_deprecated_method()                   [not in source]

azul-doc autofix add dom.Dom.new
  → Generates patch:
    {
      "Dom": {
        "constructors": {
          "new": {
            "fn_args": [{"node_type": "NodeType"}],
            "returns": {"type": "Dom"},
            "fn_body": "azul_core::dom::Dom::new(node_type)"
          }
        }
      }
    }
  → Also adds NodeType if not present

azul-doc autofix add dom.Dom.*
  → Adds ALL methods from Dom not yet in api.json

azul-doc autofix remove dom.Dom.old_deprecated_method
  → Generates removal patch
```

### Phase 3: Implementation Plan

#### 3.1 Extend TypeDefinition with Methods

```rust
// In type_index.rs

#[derive(Debug, Clone)]
pub struct MethodDef {
    pub name: String,
    pub self_kind: Option<SelfKind>,
    pub args: Vec<MethodArg>,
    pub return_type: Option<TypeRef>,
    pub is_constructor: bool,  // true if returns Self/type name
    pub doc_comment: Option<String>,
    pub visibility: Visibility,
}

#[derive(Debug, Clone)]
pub enum SelfKind {
    Value,      // self
    Ref,        // &self
    RefMut,     // &mut self
}

#[derive(Debug, Clone)]
pub struct MethodArg {
    pub name: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone)]
pub struct TypeRef {
    pub base_type: String,
    pub ref_kind: RefKind,
}

impl TypeDefinition {
    // Add methods field
    pub methods: Vec<MethodDef>,
}
```

#### 3.2 Extract Methods During Indexing

```rust
// In type_index.rs - extend index_file_types()

fn extract_methods_for_type(
    items: &[syn::Item],
    type_name: &str,
) -> Vec<MethodDef> {
    let mut methods = Vec::new();
    
    for item in items {
        if let syn::Item::Impl(impl_item) = item {
            // Check if this impl is for our type
            if get_impl_target_name(impl_item) == type_name {
                // Skip trait impls (Clone, Debug, etc.)
                if impl_item.trait_.is_some() {
                    continue;
                }
                
                for impl_member in &impl_item.items {
                    if let syn::ImplItem::Fn(fn_item) = impl_member {
                        if is_public(&fn_item.vis) {
                            methods.push(parse_method_def(fn_item));
                        }
                    }
                }
            }
        }
    }
    
    methods
}
```

#### 3.3 New Module: function_diff.rs

```rust
// New file: doc/src/autofix/function_diff.rs

pub struct FunctionComparison {
    /// Methods in source but not in api.json
    pub to_add: Vec<(String, MethodDef)>,  // (type_name, method)
    /// Methods in api.json but not in source  
    pub to_remove: Vec<(String, String)>,  // (type_name, method_name)
    /// Methods in both (might need updating)
    pub matching: Vec<(String, String)>,
}

pub fn compare_type_functions(
    type_name: &str,
    index: &TypeIndex,
    api_data: &ApiData,
) -> FunctionComparison {
    // Get methods from workspace
    let workspace_methods = index.get_methods_for_type(type_name);
    
    // Get methods from api.json
    let api_methods = get_api_methods(api_data, type_name);
    
    // Compare
    ...
}

pub fn generate_add_patch(
    type_name: &str,
    method: &MethodDef,
    index: &TypeIndex,
) -> serde_json::Value {
    // Generate fn_body
    let fn_body = generate_fn_body(type_name, method);
    
    // Build patch JSON
    ...
}
```

#### 3.4 fn_body Generation Strategy

```rust
fn generate_fn_body(type_name: &str, method: &MethodDef) -> String {
    let full_type_path = /* get from index */;
    let self_var = to_snake_case(type_name);
    
    if method.is_constructor {
        // Constructor: TypePath::method_name(args)
        format!("{}::{}({})", 
            full_type_path,
            method.name,
            method.args.iter().map(|a| &a.name).join(", ")
        )
    } else if let Some(self_kind) = &method.self_kind {
        // Instance method: self_var.method_name(args)
        let args = method.args.iter()
            .filter(|a| a.name != "self")
            .map(|a| a.name.clone())
            .join(", ");
        format!("{}.{}({})", self_var, method.name, args)
    } else {
        // Static method (no self): TypePath::method_name(args)
        format!("{}::{}({})",
            full_type_path,
            method.name,
            method.args.iter().map(|a| &a.name).join(", ")
        )
    }
}
```

#### 3.5 Dependent Type Discovery

When adding a function, we need to ensure all types in its signature exist:

```rust
fn collect_dependent_types(method: &MethodDef, index: &TypeIndex) -> Vec<TypeDefinition> {
    let mut deps = Vec::new();
    
    // From arguments
    for arg in &method.args {
        collect_types_recursive(&arg.ty.base_type, index, &mut deps);
    }
    
    // From return type
    if let Some(ret) = &method.return_type {
        collect_types_recursive(&ret.base_type, index, &mut deps);
    }
    
    deps
}

fn collect_types_recursive(
    type_name: &str,
    index: &TypeIndex,
    collected: &mut Vec<TypeDefinition>
) {
    // Skip primitives
    if is_primitive(type_name) {
        return;
    }
    
    // Look up type
    if let Some(typedef) = index.get(type_name) {
        if !collected.iter().any(|t| t.type_name == type_name) {
            collected.push(typedef.clone());
            
            // Recurse into fields
            if let TypeDefKind::Struct { fields, .. } = &typedef.kind {
                for (_, field) in fields {
                    collect_types_recursive(&field.ty, index, collected);
                }
            }
        }
    }
}
```

### Phase 4: Command Line Interface

```rust
// In main.rs

["autofix", "list", type_path] => {
    // type_path: "dom.Dom" or "dom.Dom.add_callback"
    let index = TypeIndex::build(&project_root, true)?;
    autofix::function_diff::list_functions(&index, &api_data, type_path)?;
}

["autofix", "add", target] => {
    // target: "dom.Dom.new" or "dom.Dom.*"
    let index = TypeIndex::build(&project_root, true)?;
    autofix::function_diff::add_functions(&index, &api_data, target, &output_dir)?;
}

["autofix", "remove", target] => {
    // target: "dom.Dom.old_method"
    autofix::function_diff::remove_functions(&api_data, target, &output_dir)?;
}
```

### Phase 5: Refactoring Requirements

#### 5.1 Changes to type_index.rs

1. Add `methods: Vec<MethodDef>` to `TypeDefinition`
2. Add `MethodDef`, `SelfKind`, `MethodArg`, `TypeRef` structs
3. Extend `index_file_types()` to extract methods from impl blocks
4. Add helper to associate impl blocks with types

#### 5.2 New File: function_diff.rs

1. `FunctionComparison` struct
2. `compare_type_functions()` - compare source vs api.json
3. `list_functions()` - pretty print comparison
4. `generate_add_patch()` - create JSON patch for function
5. `generate_remove_patch()` - create JSON patch for removal
6. `generate_fn_body()` - auto-generate fn_body
7. `collect_dependent_types()` - find types needed for function

#### 5.3 Changes to main.rs

1. Add `autofix list <type>` command handler
2. Add `autofix add <target>` command handler
3. Add `autofix remove <target>` command handler

#### 5.4 Patch Format for Functions

```json
// patches/add_dom_Dom_new.json
{
  "operation": "add_function",
  "module": "dom",
  "class": "Dom",
  "function_type": "constructor",  // or "function"
  "name": "new",
  "definition": {
    "fn_args": [
      {"node_type": "NodeType"}
    ],
    "returns": {"type": "Dom"},
    "fn_body": "azul_core::dom::Dom::new(node_type)"
  },
  "dependent_types": ["NodeType"]  // types to add if missing
}
```

## Implementation Order

1. **Phase 1**: Extend TypeDefinition with methods (2-3 hours)
   - Add structs
   - Modify indexing to extract methods
   - Add tests

2. **Phase 2**: Create function_diff.rs (3-4 hours)
   - Comparison logic
   - fn_body generation
   - Dependent type discovery

3. **Phase 3**: Implement list command (1-2 hours)
   - CLI parsing
   - Pretty output

4. **Phase 4**: Implement add command (2-3 hours)
   - Patch generation
   - Wildcard support (*)
   - Dependent type handling

5. **Phase 5**: Implement remove command (1 hour)
   - Removal patch format

Total: ~10-14 hours

## Open Questions

1. **Visibility of methods**: Should we only add `pub` methods, or also `pub(crate)`?
   - Recommendation: Only `pub` methods for C API

2. **Trait implementations**: Should Clone::clone, Debug::fmt be extractable?
   - Recommendation: No, these are handled via `derive` or `custom_impls`

3. **Generic methods**: How to handle `fn foo<T: Trait>()`?
   - Recommendation: Skip generic methods for now

4. **Async methods**: How to handle `async fn`?
   - Recommendation: Skip async for C API compatibility

## Conclusion

The current architecture is 80% there. The main gap is that **methods are not indexed with their parent types**. The `FunctionInfo` extraction in `type_resolver.rs` exists but is disconnected.

**Key Refactoring**: Move method extraction into `TypeDefinition` during the type indexing phase, rather than as a separate pass. This will:
1. Associate methods with their types naturally
2. Enable the `list` command to show source vs api.json differences
3. Enable `add` to generate correct fn_body with full type paths
4. Enable wildcard `*` to add all methods at once

The existing `TypeIndex`, `TypeResolver`, and patch generation infrastructure can be reused with minimal changes.
