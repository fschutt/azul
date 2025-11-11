# Generic Type Support Implementation Plan

## Status: IN PROGRESS

**Date:** 2025-11-11
**Goal:** Add full support for generic types and type aliases in the FFI API system

## Problem Statement

The azul FFI API uses 120+ type aliases of the form:
```rust
pub type LayoutZIndexValue = CssPropertyValue<LayoutZIndex>;
```

Where `CssPropertyValue<T>` is a generic enum:
```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum CssPropertyValue<T> {
    Auto,
    None,
    Initial,
    Inherit,
    Exact(T),
}
```

Currently, the api.json only supports concrete types, causing:
- 120+ "cannot find type *Value" errors in memtest
- No validation of generic type safety
- Manual duplication would be required for each instantiation

## Solution: Generic Type Support in API Schema

### Phase 1: API Schema Extension ✅

**File:** `doc/src/api.rs`

Added two new fields to `ClassData`:
```rust
pub struct ClassData {
    // ... existing fields ...
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic_params: Option<Vec<String>>, // e.g., ["T", "U"]
    
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_alias: Option<TypeAliasInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct TypeAliasInfo {
    /// The target generic type (e.g., "CssPropertyValue")
    pub target: String,
    /// Generic arguments for instantiation (e.g., ["LayoutZIndex"])
    pub generic_args: Vec<String>,
}
```

### Phase 2: Workspace Scanner Enhancement (TODO)

**File:** `doc/src/autofix/workspace.rs`

#### 2.1: Parse Type Aliases

Add function to parse type aliases from workspace:
```rust
fn parse_type_alias(item: &syn::ItemType) -> Option<TypeAliasInfo> {
    // Parse: pub type LayoutZIndexValue = CssPropertyValue<LayoutZIndex>;
    // Extract:
    //   - target: "CssPropertyValue"
    //   - generic_args: ["LayoutZIndex"]
    
    match &*item.ty {
        syn::Type::Path(path) => {
            let segment = path.path.segments.last()?;
            let target = segment.ident.to_string();
            
            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                let generic_args = args.args.iter()
                    .filter_map(|arg| match arg {
                        syn::GenericArgument::Type(syn::Type::Path(p)) => {
                            Some(p.path.segments.last()?.ident.to_string())
                        }
                        _ => None,
                    })
                    .collect();
                
                Some(TypeAliasInfo { target, generic_args })
            } else {
                None
            }
        }
        _ => None,
    }
}
```

#### 2.2: Detect Generic Type Definitions

Add function to detect generic parameters:
```rust
fn extract_generic_params(item: &syn::Item) -> Vec<String> {
    match item {
        syn::Item::Struct(s) => s.generics.params.iter()
            .filter_map(|p| match p {
                syn::GenericParam::Type(t) => Some(t.ident.to_string()),
                _ => None,
            })
            .collect(),
        syn::Item::Enum(e) => e.generics.params.iter()
            .filter_map(|p| match p {
                syn::GenericParam::Type(t) => Some(t.ident.to_string()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}
```

#### 2.3: Integrate into Scanner

Modify `scan_workspace_types()` to:
1. Detect generic types and store `generic_params`
2. Detect type aliases and store `type_alias`
3. Mark type aliases as needing expansion

### Phase 3: Autofix Validation Logic (TODO)

**File:** `doc/src/autofix/mod.rs`

#### 3.1: Validate Generic Instantiations

```rust
fn validate_generic_type(
    type_alias_name: &str,
    type_alias_info: &TypeAliasInfo,
    workspace: &WorkspaceTypes,
    api: &ApiData,
    messages: &mut Vec<AutofixMessage>,
) {
    // 1. Find the generic base type (e.g., CssPropertyValue)
    let base_type = workspace.find_type(&type_alias_info.target)
        .or_else(|| api.find_class_definition(&type_alias_info.target));
    
    let Some((_, base_class_data)) = base_type else {
        messages.push(AutofixMessage::TypeNotFound {
            type_name: type_alias_info.target.clone(),
        });
        return;
    };
    
    // 2. Check if base type is generic
    let Some(generic_params) = &base_class_data.generic_params else {
        messages.push(AutofixMessage::Error {
            message: format!("{} is not a generic type", type_alias_info.target),
        });
        return;
    };
    
    // 3. Validate number of generic arguments
    if generic_params.len() != type_alias_info.generic_args.len() {
        messages.push(AutofixMessage::Error {
            message: format!(
                "{}: Expected {} generic arguments, got {}",
                type_alias_name,
                generic_params.len(),
                type_alias_info.generic_args.len()
            ),
        });
        return;
    };
    
    // 4. Validate each generic argument is FFI-safe
    for arg in &type_alias_info.generic_args {
        let arg_type = workspace.find_type(arg)
            .or_else(|| api.find_class_definition(arg));
        
        let Some((_, arg_class_data)) = arg_type else {
            messages.push(AutofixMessage::TypeNotFound {
                type_name: arg.clone(),
            });
            continue;
        };
        
        // Check if arg has repr(C)
        if !has_repr_c(arg_class_data) {
            messages.push(AutofixMessage::Error {
                message: format!(
                    "{}: Generic argument {} is not FFI-safe (missing repr(C))",
                    type_alias_name, arg
                ),
            });
        }
    }
}
```

#### 3.2: Expand Type Aliases

```rust
fn expand_type_alias(
    type_alias_name: &str,
    type_alias_info: &TypeAliasInfo,
    workspace: &WorkspaceTypes,
) -> Option<ClassData> {
    // 1. Get the generic base type
    let base_type = workspace.find_type(&type_alias_info.target)?;
    
    // 2. Clone and substitute generic parameters
    let mut expanded = base_type.clone();
    
    // 3. Substitute T -> concrete type in all fields
    if let Some(generic_params) = &base_type.generic_params {
        for (param, arg) in generic_params.iter().zip(&type_alias_info.generic_args) {
            substitute_generic_param(&mut expanded, param, arg);
        }
    }
    
    // 4. Remove generic_params (now concrete)
    expanded.generic_params = None;
    
    // 5. Set external path
    expanded.external = Some(format!(
        "azul_css::props::property::{}",
        type_alias_name
    ));
    
    Some(expanded)
}

fn substitute_generic_param(
    class_data: &mut ClassData,
    param: &str,
    arg: &str,
) {
    // Substitute in enum_fields
    if let Some(enum_fields) = &mut class_data.enum_fields {
        for variant_map in enum_fields.iter_mut() {
            for (_variant_name, variant_data) in variant_map.iter_mut() {
                if let Some(ref mut type_str) = variant_data.r#type {
                    if type_str == param {
                        *type_str = arg.to_string();
                    }
                }
            }
        }
    }
    
    // Substitute in struct_fields
    if let Some(struct_fields) = &mut class_data.struct_fields {
        for field_map in struct_fields.iter_mut() {
            for (_field_name, field_data) in field_map.iter_mut() {
                if let Some(ref mut type_str) = field_data.r#type {
                    if type_str == param {
                        *type_str = arg.to_string();
                    }
                }
            }
        }
    }
}
```

### Phase 4: Code Generator Integration (TODO)

**File:** `doc/src/codegen/struct_gen.rs`, `doc/src/codegen/memtest.rs`

#### 4.1: Generate Concrete Types from Aliases

```rust
fn generate_type_alias_concrete(
    type_alias_name: &str,
    type_alias_info: &TypeAliasInfo,
    expanded_data: &ClassData,
    config: &GenerateConfig,
) -> String {
    // Generate the concrete type with the correct name
    // e.g., Az1LayoutZIndexValue instead of Az1CssPropertyValue
    
    let prefixed_name = format!("{}{}", config.prefix, type_alias_name);
    
    // Generate enum/struct based on expanded_data
    // This is the same as generating any other type, but with
    // the type alias name instead of the generic type name
    
    generate_struct_or_enum(&prefixed_name, expanded_data, config)
}
```

#### 4.2: Handle Generic Types in memtest

Modify `generate_dll_module()` to:
1. Detect type aliases
2. Expand them using autofix logic
3. Generate concrete instantiations

### Phase 5: Testing & Validation (TODO)

1. **Unit Tests:**
   - Test type alias parsing
   - Test generic parameter extraction
   - Test substitution logic

2. **Integration Tests:**
   - Generate api.json with CssPropertyValue<T>
   - Run autofix with 120 type aliases
   - Verify all expanded correctly

3. **memtest Validation:**
   - Generate memtest crate
   - Verify all 120 *Value types compile
   - Check memory layout matches

## Expected API.json Format

### Generic Base Type:
```json
"CssPropertyValue": {
  "external": "azul_css::props::property::CssPropertyValue",
  "generic_params": ["T"],
  "derive": ["Debug", "Copy", "Clone", "PartialEq", "Eq", "Hash", "PartialOrd", "Ord"],
  "repr": "C, u8",
  "enum_fields": [{
    "Auto": {},
    "None": {},
    "Initial": {},
    "Inherit": {},
    "Exact": {"type": "T"}
  }]
}
```

### Type Alias (Concrete Instantiation):
```json
"LayoutZIndexValue": {
  "external": "azul_css::props::property::LayoutZIndexValue",
  "type_alias": {
    "target": "CssPropertyValue",
    "generic_args": ["LayoutZIndex"]
  },
  "derive": ["Copy"],
  "enum_fields": [{
    "Auto": {},
    "None": {},
    "Initial": {},
    "Inherit": {},
    "Exact": {"type": "LayoutZIndex"}
  }]
}
```

Note: The `enum_fields` in the type alias are **expanded** by autofix - 
they don't need to be manually written, they're computed from the generic base.

## Implementation Checklist

- [x] Phase 1: API Schema Extension
  - [x] Add `generic_params` field to ClassData
  - [x] Add `TypeAliasInfo` struct
  - [x] Add `type_alias` field to ClassData
  - [x] Compile and verify schema changes

- [ ] Phase 2: Workspace Scanner Enhancement
  - [ ] Implement `parse_type_alias()` function
  - [ ] Implement `extract_generic_params()` function
  - [ ] Integrate into `scan_workspace_types()`
  - [ ] Test with sample workspace

- [ ] Phase 3: Autofix Validation Logic
  - [ ] Implement `validate_generic_type()`
  - [ ] Implement `expand_type_alias()`
  - [ ] Implement `substitute_generic_param()`
  - [ ] Add validation to autofix command

- [ ] Phase 4: Code Generator Integration
  - [ ] Modify memtest generator to handle type aliases
  - [ ] Implement `generate_type_alias_concrete()`
  - [ ] Test memtest generation

- [ ] Phase 5: Testing & Validation
  - [ ] Add unit tests for all new functions
  - [ ] Run full autofix on workspace
  - [ ] Generate and compile memtest
  - [ ] Verify 0 errors (except ImageCache/FastHashMap)

## Next Steps

1. Implement Phase 2: Workspace Scanner Enhancement
2. Create CssPropertyValue<T> entry in api.json manually
3. Run autofix to detect all 120 type aliases
4. Validate and expand them automatically

## Notes

- Generic types are only supported if all type parameters are FFI-safe (repr(C))
- Type aliases are transparently replaced with their expanded form in generated code
- This approach scales to any number of generic parameters
- Future: Could extend to support trait bounds, lifetimes, etc.
