//! API Diff Generation V2
//!
//! This module compares the expected types (from workspace) with the current
//! types (from api.json) and generates patches for differences.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;

use super::type_index::{TypeIndex, TypeDefinition, TypeDefKind};
use super::type_resolver::{ResolvedTypeSet, ResolvedType, TypeResolver, ResolutionContext};
use super::module_map::get_correct_module;
use crate::api::ApiData;

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Diff between expected and current API
#[derive(Debug, Default)]
pub struct ApiDiff {
    /// Types that need path corrections
    pub path_fixes: Vec<PathFix>,
    /// Types to add to api.json
    pub additions: Vec<TypeAddition>,
    /// Types to remove from api.json  
    pub removals: Vec<String>,
    /// Field/variant changes within types
    pub modifications: Vec<TypeModification>,
    /// Types to move to a different module
    pub module_moves: Vec<ModuleMove>,
}

/// A path correction for a type
#[derive(Debug, Clone)]
pub struct PathFix {
    /// The simple type name
    pub type_name: String,
    /// The old (current) path in api.json
    pub old_path: String,
    /// The new (correct) path from workspace
    pub new_path: String,
}

/// A type that should be added to api.json
#[derive(Debug, Clone)]
pub struct TypeAddition {
    pub type_name: String,
    pub full_path: String,
    pub kind: String, // "struct", "enum", "callback", etc.
    /// Struct fields (for struct types): (field_name, field_type, ref_kind)
    pub struct_fields: Option<Vec<(String, String, String)>>, // (field_name, field_type, ref_kind)
    /// Enum variants (for enum types)
    pub enum_variants: Option<Vec<(String, Option<String>)>>, // (variant_name, variant_type)
    /// Derives from source code
    pub derives: Vec<String>,
}

/// A type that should be moved to a different module
#[derive(Debug, Clone)]
pub struct ModuleMove {
    /// The type name
    pub type_name: String,
    /// The current (wrong) module
    pub from_module: String,
    /// The target (correct) module
    pub to_module: String,
}

/// A modification to an existing type
#[derive(Debug, Clone)]
pub struct TypeModification {
    pub type_name: String,
    pub kind: ModificationKind,
}

/// Callback argument with name, type, and reference kind
#[derive(Debug, Clone)]
pub struct CallbackArgInfo {
    pub name: Option<String>,
    pub ty: String,
    pub ref_kind: super::type_index::RefKind,
}

#[derive(Debug, Clone)]
pub enum ModificationKind {
    FieldAdded { field_name: String, field_type: String, ref_kind: crate::api::RefKind },
    FieldRemoved { field_name: String },
    FieldTypeChanged { field_name: String, old_type: String, new_type: String, ref_kind: crate::api::RefKind },
    VariantAdded { variant_name: String },
    VariantRemoved { variant_name: String },
    VariantTypeChanged { variant_name: String, old_type: Option<String>, new_type: Option<String> },
    DeriveAdded { derive_name: String },
    DeriveRemoved { derive_name: String },
    CustomImplAdded { impl_name: String },
    CustomImplRemoved { impl_name: String },
    ReprCChanged { old_repr_c: bool, new_repr_c: bool },
    /// Callback typedef needs to be added (type exists but has no callback_typedef)
    CallbackTypedefAdded { args: Vec<CallbackArgInfo>, returns: Option<String> },
    /// Callback typedef arg changed
    CallbackArgChanged { arg_index: usize, old_type: String, new_type: String, old_ref_kind: Option<super::type_index::RefKind>, new_ref_kind: super::type_index::RefKind },
    /// Callback typedef return changed
    CallbackReturnChanged { old_type: Option<String>, new_type: Option<String> },
    /// Type alias needs to be added (type exists but has no type_alias)
    TypeAliasAdded { target: String, generic_args: Vec<String> },
    /// Type alias target changed
    TypeAliasTargetChanged { old_target: String, new_target: String, new_generic_args: Vec<String> },
    /// Generic params changed (e.g., PhysicalSize<T> missing generic_params: ["T"])
    GenericParamsChanged { old_params: Vec<String>, new_params: Vec<String> },
}

// ============================================================================
// API TYPE RESOLUTION
// ============================================================================

/// Resolve types from the current api.json
pub fn resolve_api_types(index: &TypeIndex, api_data: &ApiData) -> ApiTypeResolution {
    let mut resolution = ApiTypeResolution::default();
    let ctx = ResolutionContext::new();

    // Iterate through all versions in api.json
    for (_version_name, version_data) in &api_data.0 {
        // Iterate through all modules
        for (_module_name, module_data) in &version_data.api {
            // Iterate through all classes
            for (class_name, class_data) in &module_data.classes {
                // Get the external path for this class
                let external_path = class_data.external.as_deref().unwrap_or("");
                
                resolve_api_type(index, class_name, external_path, &ctx, &mut resolution);

                // Also resolve field types
                if let Some(ref fields) = class_data.struct_fields {
                    for field_map in fields {
                        for (_field_name, field_data) in field_map {
                            // field_data.r#type is the type string
                            resolve_api_type_name(index, &field_data.r#type, &ctx, &mut resolution);
                        }
                    }
                }

                // Resolve enum variants
                if let Some(ref variants) = class_data.enum_fields {
                    for variant_map in variants {
                        for (_variant_name, variant_data) in variant_map {
                            if let Some(ref variant_type) = variant_data.r#type {
                                resolve_api_type_name(index, variant_type, &ctx, &mut resolution);
                            }
                        }
                    }
                }
            }
        }
    }

    resolution
}

/// Resolution result for api.json types
#[derive(Debug, Default)]
pub struct ApiTypeResolution {
    /// Types that were found in workspace
    pub found: HashMap<String, FoundType>,
    /// Types that could not be found (with their api.json path)
    pub missing: HashMap<String, MissingType>,
    /// Types with path mismatches
    pub path_mismatches: Vec<PathMismatch>,
}

#[derive(Debug, Clone)]
pub struct FoundType {
    pub type_name: String,
    pub api_path: String,
    pub workspace_path: String,
}

#[derive(Debug, Clone)]
pub struct MissingType {
    pub type_name: String,
    pub api_path: String,
}

#[derive(Debug, Clone)]
pub struct PathMismatch {
    pub type_name: String,
    pub api_path: String,
    pub workspace_path: String,
}

fn resolve_api_type(
    index: &TypeIndex,
    type_name: &str,
    api_path: &str,
    ctx: &ResolutionContext,
    resolution: &mut ApiTypeResolution,
) {
    // Skip if already processed
    if resolution.found.contains_key(type_name) || resolution.missing.contains_key(type_name) {
        return;
    }

    // Try to find in workspace
    match index.resolve(type_name, None) {
        Some(typedef) => {
            let workspace_path = &typedef.full_path;

            // Check if paths match (only if api_path is not empty)
            if !api_path.is_empty() && api_path != workspace_path && !paths_are_equivalent(api_path, workspace_path) {
                resolution.path_mismatches.push(PathMismatch {
                    type_name: type_name.to_string(),
                    api_path: api_path.to_string(),
                    workspace_path: workspace_path.clone(),
                });
            }

            resolution.found.insert(type_name.to_string(), FoundType {
                type_name: type_name.to_string(),
                api_path: api_path.to_string(),
                workspace_path: workspace_path.clone(),
            });
        }
        None => {
            // Only mark as missing if it has an api_path (it's a class definition, not just a reference)
            if !api_path.is_empty() {
                resolution.missing.insert(type_name.to_string(), MissingType {
                    type_name: type_name.to_string(),
                    api_path: api_path.to_string(),
                });
            }
        }
    }
}

fn resolve_api_type_name(
    index: &TypeIndex,
    type_name: &str,
    ctx: &ResolutionContext,
    resolution: &mut ApiTypeResolution,
) {
    // Skip primitives
    if TypeIndex::is_primitive(type_name) {
        return;
    }

    // Extract base type name
    let base_name = extract_simple_type_name(type_name);
    if base_name.is_empty() || TypeIndex::is_primitive(&base_name) {
        return;
    }

    // Skip if already processed
    if resolution.found.contains_key(&base_name) || resolution.missing.contains_key(&base_name) {
        return;
    }

    // Try to find in workspace
    match index.resolve(&base_name, None) {
        Some(typedef) => {
            resolution.found.insert(base_name.clone(), FoundType {
                type_name: base_name,
                api_path: String::new(), // Unknown from just a type reference
                workspace_path: typedef.full_path.clone(),
            });
        }
        None => {
            // This is just a reference, not a class definition - don't add to missing
            // (missing is only for class definitions in api.json that don't exist in workspace)
        }
    }
}

/// Check if two paths are equivalent
/// 
/// Paths are only equivalent if they are exactly the same.
/// We DO want to catch crate renames (e.g., azul_dll -> azul_layout).
fn paths_are_equivalent(path1: &str, path2: &str) -> bool {
    path1 == path2
}

// ============================================================================
// AZ-PREFIX HANDLING
// ============================================================================

/// Strip "Az" prefix from a type name if present.
/// Types in the workspace may have "Az" prefix (e.g., AzStringPair)
/// but in api.json they are stored without it (e.g., StringPair)
/// to avoid "AzAzStringPair" when the memtest generator adds the prefix.
pub fn strip_az_prefix(type_name: &str) -> &str {
    if type_name.starts_with("Az") && type_name.len() > 2 {
        // Make sure the third character is uppercase (to avoid stripping "Azure" etc.)
        let third_char = type_name.chars().nth(2);
        if third_char.map(|c| c.is_uppercase()).unwrap_or(false) {
            return &type_name[2..];
        }
    }
    type_name
}

/// Get the api.json lookup name for a workspace type.
/// If the type has an "Az" prefix, return the name without it.
pub fn workspace_name_to_api_name(workspace_name: &str) -> &str {
    strip_az_prefix(workspace_name)
}

/// Check if a type name matches, considering Az prefix.
/// Returns true if:
/// - Names are identical
/// - workspace_name is "AzFoo" and api_name is "Foo"
pub fn type_names_match(workspace_name: &str, api_name: &str) -> bool {
    if workspace_name == api_name {
        return true;
    }
    strip_az_prefix(workspace_name) == api_name
}

/// Extract simple type name from a potentially complex type
fn extract_simple_type_name(type_str: &str) -> String {
    let s = type_str.trim();

    // Handle pointers
    if s.starts_with("*const ") {
        return extract_simple_type_name(&s[7..]);
    }
    if s.starts_with("*mut ") {
        return extract_simple_type_name(&s[5..]);
    }

    // Handle references
    if s.starts_with('&') {
        let without_amp = s.trim_start_matches('&').trim_start_matches("mut ");
        return extract_simple_type_name(without_amp.trim());
    }

    // Handle qualified paths
    if s.contains("::") {
        return s.rsplit("::").next().unwrap_or(s).to_string();
    }

    // Handle generics - return outer type
    if let Some(idx) = s.find('<') {
        return s[..idx].to_string();
    }

    s.to_string()
}

// ============================================================================
// DIFF GENERATION
// ============================================================================

/// Generate diff between expected and current API types
pub fn generate_diff(
    expected: &ResolvedTypeSet,
    api_resolution: &ApiTypeResolution,
    index: &TypeIndex,
) -> ApiDiff {
    let mut diff = ApiDiff::default();
    let mut seen_fixes: HashSet<String> = HashSet::new();
    let mut seen_additions: HashSet<String> = HashSet::new();

    // 1. Path fixes from mismatches
    for mismatch in &api_resolution.path_mismatches {
        let key = format!("{}:{}", mismatch.type_name, mismatch.workspace_path);
        if seen_fixes.insert(key) {
            diff.path_fixes.push(PathFix {
                type_name: mismatch.type_name.clone(),
                old_path: mismatch.api_path.clone(),
                new_path: mismatch.workspace_path.clone(),
            });
        }
    }

    // 2. Types in workspace (resolved from functions) but not in api.json -> additions
    for (type_name, resolved) in &expected.resolved {
        // Skip if already in api.json
        if api_resolution.found.contains_key(type_name) {
            continue;
        }
        // Skip if it's in api.json but just couldn't be found in workspace (that's a removal, not addition)
        if api_resolution.missing.contains_key(type_name) {
            continue;
        }
        
        // This type is used by workspace functions but not in api.json - should be added
        if seen_additions.insert(type_name.clone()) {
            // Look up the type to get its kind and fields
            let (kind, struct_fields, enum_variants, derives) = if let Some(typedef) = index.resolve(type_name, None) {
                // Get the expanded kind (handles MacroGenerated types)
                let expanded = typedef.expand_macro_generated();
                
                let kind_str = match &typedef.kind {
                    super::type_index::TypeDefKind::Struct { .. } => "struct",
                    super::type_index::TypeDefKind::Enum { .. } => "enum",
                    super::type_index::TypeDefKind::TypeAlias { .. } => "type_alias",
                    super::type_index::TypeDefKind::CallbackTypedef { .. } => "callback",
                    super::type_index::TypeDefKind::MacroGenerated { kind, .. } => {
                        match kind {
                            super::type_index::MacroGeneratedKind::Vec => "struct",
                            super::type_index::MacroGeneratedKind::VecDestructor => "enum",
                            super::type_index::MacroGeneratedKind::VecDestructorType => "callback_typedef",
                            super::type_index::MacroGeneratedKind::Option => "enum",
                            super::type_index::MacroGeneratedKind::OptionEnumWrapper => "struct",
                            super::type_index::MacroGeneratedKind::Result => "enum",
                            super::type_index::MacroGeneratedKind::CallbackWrapper => "struct",
                            super::type_index::MacroGeneratedKind::CallbackValue => "struct",
                        }
                    }
                };
                
                // Extract fields/variants from the EXPANDED kind
                let (fields, variants, derives) = match expanded {
                    super::type_index::TypeDefKind::Struct { fields, derives, .. } => {
                        let field_vec: Vec<(String, String, String)> = fields.iter()
                            .map(|(name, field)| (name.clone(), field.ty.clone(), field.ref_kind.as_str().to_string()))
                            .collect();
                        (Some(field_vec), None, derives)
                    }
                    super::type_index::TypeDefKind::Enum { variants, derives, .. } => {
                        let variant_vec: Vec<(String, Option<String>)> = variants.iter()
                            .map(|(name, variant)| (name.clone(), variant.ty.clone()))
                            .collect();
                        (None, Some(variant_vec), derives)
                    }
                    super::type_index::TypeDefKind::CallbackTypedef { .. } => {
                        (None, None, vec![])
                    }
                    super::type_index::TypeDefKind::TypeAlias { .. } => {
                        (None, None, vec![])
                    }
                    super::type_index::TypeDefKind::MacroGenerated { .. } => {
                        // This shouldn't happen after expand_macro_generated()
                        (None, None, vec![])
                    }
                };
                
                (kind_str, fields, variants, derives)
            } else {
                ("unknown", None, None, vec![])
            };
            
            diff.additions.push(TypeAddition {
                type_name: type_name.clone(),
                full_path: resolved.full_path.clone(),
                kind: kind.to_string(),
                struct_fields,
                enum_variants,
                derives,
            });
        }
    }

    // 3. Types in api.json that couldn't be found in workspace - mark for removal
    for (type_name, missing_info) in &api_resolution.missing {
        diff.removals.push(format!("{}:{}", type_name, missing_info.api_path));
    }

    diff
}

// ============================================================================
// MAIN ENTRY POINT
// ============================================================================

/// Run the full diff analysis
/// 
/// The logic:
/// 1. Build workspace type index (source of truth for TYPE DEFINITIONS)
/// 2. Extract functions from api.json (source of truth for API SURFACE)
/// 3. For each api.json function, resolve all types RECURSIVELY using WORKSPACE INDEX
/// 4. This gives us "expected" state - all types the API needs with their current workspace paths
/// 5. Compare expected vs current api.json → generate diff
pub fn analyze_api_diff(
    workspace_root: &Path,
    api_data: &ApiData,
    verbose: bool,
) -> Result<ApiDiff> {
    use colored::Colorize;
    
    // Step 1: Build type index from workspace (source of truth for types)
    if verbose {
        eprintln!("[Diff] Building type index from workspace...");
    }
    let index = TypeIndex::build(workspace_root, verbose)?;

    // Step 2: Extract all type names referenced by api.json functions
    // Then resolve them RECURSIVELY using the WORKSPACE INDEX
    if verbose {
        eprintln!("[Diff] Resolving types from api.json functions using workspace index...");
    }
    let expected = resolve_api_functions_with_workspace_index(&index, api_data, verbose);

    // Print warnings about non-C-compatible types (always print all - these are important!)
    if !expected.warnings.is_empty() {
        eprintln!("\n{}", "Warnings (non-C-compatible types):".yellow().bold());
        for (i, warning) in expected.warnings.iter().enumerate() {
            eprintln!("  {} {}: {}", 
                format!("[{}]", i + 1).dimmed(),
                warning.type_expr.red(),
                warning.message.yellow()
            );
            if !warning.context.is_empty() {
                eprintln!("      {} {}", "in".dimmed(), warning.context.dimmed());
            }
        }
        eprintln!();
    }

    // Step 3: Collect current api.json type definitions (for comparison)
    if verbose {
        eprintln!("[Diff] Collecting current api.json types...");
    }
    let current_api_types = collect_api_json_types(api_data);

    if verbose {
        eprintln!(
            "[Diff] Expected (workspace-resolved): {} types, Current (api.json): {} types",
            expected.resolved.len(),
            current_api_types.len()
        );
    }

    // Step 4: Generate diff between expected and current
    let diff = generate_diff_v2(&expected, &current_api_types, &index);

    if verbose {
        eprintln!(
            "[Diff] Generated {} path fixes, {} additions, {} removals",
            diff.path_fixes.len(),
            diff.additions.len(),
            diff.removals.len()
        );
    }

    Ok(diff)
}

/// Resolve all types referenced by api.json functions, using the WORKSPACE INDEX
/// This gives us the "expected" state - what types the API needs with their current paths
fn resolve_api_functions_with_workspace_index(
    index: &TypeIndex,
    api_data: &ApiData,
    verbose: bool,
) -> ResolvedTypeSet {
    use super::type_resolver::{TypeResolver, ResolutionContext};
    
    let mut resolver = TypeResolver::new(index);
    let ctx = ResolutionContext::new();
    
    let mut function_count = 0;

    // Iterate through all api.json entries
    // NOTE: We only resolve types that are reachable from function signatures
    // (parameters and return types). The class name itself is NOT an entry point,
    // because types like ImageCache may be in api.json but only used via *const pointers
    // which are trace blockers. Only types actually used in public API signatures matter.
    for (_version_name, version_data) in &api_data.0 {
        for (_module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Resolve constructor parameters
                if let Some(ref constructors) = class_data.constructors {
                    for (ctor_name, ctor_data) in constructors {
                        // Resolve parameter types - fn_args is Vec<IndexMap<String, String>>
                        for arg_map in &ctor_data.fn_args {
                            for (arg_name, arg_type) in arg_map {
                                let parent_context = format!("{}::{} arg '{}'", class_name, ctor_name, arg_name);
                                resolver.resolve_type_with_context(arg_type, &ctx, Some(&parent_context));
                            }
                        }
                        
                        // Resolve return type
                        if let Some(ref returns) = ctor_data.returns {
                            let parent_context = format!("{}::{} -> return", class_name, ctor_name);
                            resolver.resolve_type_with_context(&returns.r#type, &ctx, Some(&parent_context));
                        }
                    }
                }
                
                // Resolve function parameters and return types
                if let Some(ref functions) = class_data.functions {
                    for (fn_name, fn_data) in functions {
                        function_count += 1;
                        
                        // Resolve parameter types
                        for arg_map in &fn_data.fn_args {
                            for (arg_name, arg_type) in arg_map {
                                let parent_context = format!("{}::{} arg '{}'", class_name, fn_name, arg_name);
                                resolver.resolve_type_with_context(arg_type, &ctx, Some(&parent_context));
                            }
                        }
                        
                        // Resolve return type
                        if let Some(ref returns) = fn_data.returns {
                            let parent_context = format!("{}::{} -> return", class_name, fn_name);
                            resolver.resolve_type_with_context(&returns.r#type, &ctx, Some(&parent_context));
                        }
                    }
                }
                
                // Resolve struct_fields types
                // These are the types used in struct field definitions
                if let Some(ref fields) = class_data.struct_fields {
                    for field_map in fields {
                        for (field_name, field_data) in field_map {
                            let parent_context = format!("{}.{}", class_name, field_name);
                            resolver.resolve_type_with_context(&field_data.r#type, &ctx, Some(&parent_context));
                        }
                    }
                }
                
                // Resolve enum_fields variant types
                if let Some(ref variants) = class_data.enum_fields {
                    for variant_map in variants {
                        for (variant_name, variant_data) in variant_map {
                            if let Some(ref variant_type) = variant_data.r#type {
                                let parent_context = format!("{}::{}", class_name, variant_name);
                                resolver.resolve_type_with_context(variant_type, &ctx, Some(&parent_context));
                            }
                        }
                    }
                }
                
                // Resolve callback_typedef argument types
                // These are entry points for callbacks and their argument types need to be resolved
                if let Some(ref callback_def) = class_data.callback_typedef {
                    for (i, arg_data) in callback_def.fn_args.iter().enumerate() {
                        let parent_context = format!("{} callback arg[{}]", class_name, i);
                        resolver.resolve_type_with_context(&arg_data.r#type, &ctx, Some(&parent_context));
                    }
                    
                    // Resolve return type
                    if let Some(ref returns) = callback_def.returns {
                        let parent_context = format!("{} callback -> return", class_name);
                        resolver.resolve_type_with_context(&returns.r#type, &ctx, Some(&parent_context));
                    }
                }
            }
        }
    }

    if verbose {
        eprintln!("[Diff] Processed {} api.json functions", function_count);
    }

    resolver.finish()
}

/// Collect all type definitions from api.json (for comparison)
/// Returns: HashMap<type_name, ApiTypeInfo>
fn collect_api_json_types(api_data: &ApiData) -> HashMap<String, ApiTypeInfo> {
    let mut types = HashMap::new();
    
    for (_version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                let path = class_data.external.clone().unwrap_or_default();
                let derives = class_data.derive.clone().unwrap_or_default();
                let custom_impls = class_data.custom_impls.clone().unwrap_or_default();
                let has_repr_c = class_data.repr.as_deref() == Some("C");
                
                // Extract struct fields with ref_kind
                let struct_fields = class_data.struct_fields.as_ref().map(|fields_vec| {
                    fields_vec.iter()
                        .flat_map(|field_map| {
                            field_map.iter().map(|(name, data)| {
                                (name.clone(), data.r#type.clone(), data.ref_kind)
                            })
                        })
                        .collect()
                });
                
                // Extract enum variants
                let enum_variants = class_data.enum_fields.as_ref().map(|variants_vec| {
                    variants_vec.iter()
                        .flat_map(|variant_map| {
                            variant_map.iter().map(|(name, data)| {
                                (name.clone(), data.r#type.clone())
                            })
                        })
                        .collect()
                });
                
                // Extract callback typedef
                let (callback_args, callback_returns) = if let Some(ref callback_def) = class_data.callback_typedef {
                    use crate::api::BorrowMode;
                    let args: Vec<(String, Option<String>)> = callback_def.fn_args.iter()
                        .map(|arg| {
                            let ref_str = match arg.ref_kind {
                                BorrowMode::Ref => Some("ref".to_string()),
                                BorrowMode::RefMut => Some("refmut".to_string()),
                                BorrowMode::Value => Some("value".to_string()),
                            };
                            (arg.r#type.clone(), ref_str)
                        })
                        .collect();
                    let returns = callback_def.returns.as_ref().map(|r| r.r#type.clone());
                    (Some(args), returns)
                } else {
                    (None, None)
                };
                
                // Extract type alias
                let (type_alias_target, type_alias_generic_args) = match &class_data.type_alias {
                    Some(ta) => (Some(ta.target.clone()), ta.generic_args.clone()),
                    None => (None, Vec::new()),
                };
                
                // Extract generic params
                let generic_params = class_data.generic_params.clone().unwrap_or_default();
                
                types.insert(class_name.clone(), ApiTypeInfo {
                    path,
                    module: module_name.clone(),
                    derives,
                    custom_impls,
                    has_repr_c,
                    struct_fields,
                    enum_variants,
                    callback_args,
                    callback_returns,
                    type_alias_target,
                    type_alias_generic_args,
                    generic_params,
                });
            }
        }
    }
    
    types
}

/// Information about a type from api.json
#[derive(Debug, Clone, Default)]
pub struct ApiTypeInfo {
    pub path: String,
    pub module: String,
    pub derives: Vec<String>,
    pub custom_impls: Vec<String>,
    pub has_repr_c: bool,
    /// Struct fields: (field_name, field_type, ref_kind)
    pub struct_fields: Option<Vec<(String, String, crate::api::RefKind)>>,
    /// Enum variants: (variant_name, variant_type)
    pub enum_variants: Option<Vec<(String, Option<String>)>>,
    /// Callback typedef args: (arg_type, ref_kind_string)
    /// ref_kind_string is "ref", "refmut", or "value"
    pub callback_args: Option<Vec<(String, Option<String>)>>,
    /// Callback typedef return type
    pub callback_returns: Option<String>,
    /// Type alias target
    pub type_alias_target: Option<String>,
    /// Type alias generic args (e.g., ["u32"] for PhysicalSizeU32 = PhysicalSize<u32>)
    pub type_alias_generic_args: Vec<String>,
    /// Generic type parameters (e.g., ["T"] for PhysicalSize<T>)
    pub generic_params: Vec<String>,
}

/// Generate diff between expected (workspace-resolved) and current (api.json) types
fn generate_diff_v2(
    expected: &ResolvedTypeSet,
    current_api_types: &HashMap<String, ApiTypeInfo>,
    index: &TypeIndex,
) -> ApiDiff {
    let mut diff = ApiDiff::default();
    let mut seen_additions: HashSet<String> = HashSet::new();
    let mut matched_api_types: HashSet<String> = HashSet::new();

    // 1. Types in expected (resolved from workspace) but not in api.json → additions
    // Also check for Az-prefix matches (AzStringPair in workspace = StringPair in api.json)
    for (workspace_name, resolved) in &expected.resolved {
        let api_lookup_name = workspace_name_to_api_name(workspace_name);
        
        // Check if it exists in api.json (either exact match or without Az prefix)
        let api_match = if current_api_types.contains_key(workspace_name) {
            Some(workspace_name.as_str())
        } else if workspace_name != api_lookup_name && current_api_types.contains_key(api_lookup_name) {
            Some(api_lookup_name)
        } else {
            None
        };

        if let Some(matched_api_name) = api_match {
            // Type exists in both - mark as matched
            matched_api_types.insert(matched_api_name.to_string());
            
            // Check if path matches
            if let Some(api_info) = current_api_types.get(matched_api_name) {
                if !api_info.path.is_empty() && !paths_are_equivalent(&api_info.path, &resolved.full_path) {
                    diff.path_fixes.push(PathFix {
                        type_name: matched_api_name.to_string(),
                        old_path: api_info.path.clone(),
                        new_path: resolved.full_path.clone(),
                    });
                }
                
                // Check for derive/impl changes and field/variant differences
                if let Some(typedef) = index.resolve(workspace_name, None) {
                    diff.modifications.extend(
                        compare_derives_and_impls(matched_api_name, typedef, api_info)
                    );
                    
                    // Check for struct field differences
                    diff.modifications.extend(
                        compare_struct_fields(matched_api_name, typedef, api_info)
                    );
                    
                    // Check for enum variant differences
                    diff.modifications.extend(
                        compare_enum_variants(matched_api_name, typedef, api_info)
                    );
                    
                    // Check for callback_typedef differences
                    diff.modifications.extend(
                        compare_callback_typedef(matched_api_name, typedef, api_info)
                    );
                    
                    // Check for type_alias differences
                    diff.modifications.extend(
                        compare_type_alias(matched_api_name, typedef, api_info)
                    );
                    
                    // Check for generic_params differences
                    diff.modifications.extend(
                        compare_generic_params(matched_api_name, typedef, api_info)
                    );
                }
                
                // Check if type is in the wrong module
                if let Some(correct_module) = get_correct_module(matched_api_name, &api_info.module) {
                    diff.module_moves.push(ModuleMove {
                        type_name: matched_api_name.to_string(),
                        from_module: api_info.module.clone(),
                        to_module: correct_module,
                    });
                }
            }
        } else {
            // Type is in workspace but not in api.json - should be added
            // Use the api_lookup_name (without Az prefix) as the type_name for api.json
            if seen_additions.insert(api_lookup_name.to_string()) {
                let (kind, struct_fields, enum_variants, derives) = get_type_kind_with_fields(index, workspace_name);
                diff.additions.push(TypeAddition {
                    type_name: api_lookup_name.to_string(),
                    full_path: resolved.full_path.clone(),
                    kind,
                    struct_fields,
                    enum_variants,
                    derives,
                });
            }
        }
    }

    // 2. Types in api.json but not matched from workspace → removals
    // Also check for module moves on ALL api.json types (not just matched ones)
    for (api_name, api_info) in current_api_types {
        if !matched_api_types.contains(api_name) {
            // Type is in api.json but couldn't be resolved from workspace
            // This could mean:
            // a) Type was deleted from workspace
            // b) Type was renamed (different name now)  
            // c) Type is no longer reachable from any function
            diff.removals.push(format!("{}:{}", api_name, api_info.path));
        }
        
        // Check if ANY type (matched or not) is in the wrong module
        // This ensures we move legacy module types even if they weren't resolved from workspace
        if let Some(correct_module) = get_correct_module(api_name, &api_info.module) {
            // Avoid duplicate moves (already added in matched types loop)
            let already_has_move = diff.module_moves.iter()
                .any(|m| m.type_name == *api_name);
            
            if !already_has_move {
                diff.module_moves.push(ModuleMove {
                    type_name: api_name.to_string(),
                    from_module: api_info.module.clone(),
                    to_module: correct_module,
                });
            }
        }
        
        // 3. Check path fixes for ALL api.json types against workspace index
        // This catches types that aren't reachable from function signatures
        // but still have wrong paths (e.g., azul_dll -> azul_layout moves)
        if !api_info.path.is_empty() && !matched_api_types.contains(api_name) {
            // Try to find this type in the workspace index
            // First try exact name, then with Az prefix
            let workspace_typedef = index.resolve(api_name, None)
                .or_else(|| index.resolve(&format!("Az{}", api_name), None));
            
            if let Some(typedef) = workspace_typedef {
                // Found in workspace - check if path matches
                if !paths_are_equivalent(&api_info.path, &typedef.full_path) {
                    // Path mismatch - need to fix
                    let already_has_fix = diff.path_fixes.iter()
                        .any(|f| f.type_name == *api_name);
                    
                    if !already_has_fix {
                        diff.path_fixes.push(PathFix {
                            type_name: api_name.to_string(),
                            old_path: api_info.path.clone(),
                            new_path: typedef.full_path.clone(),
                        });
                    }
                }
            }
        }
    }

    diff
}

/// Compare derives and custom_impls between workspace type and api.json type
fn compare_derives_and_impls(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    let mut modifications = Vec::new();
    
    // Get workspace derives, custom_impls and repr_c
    let (workspace_derives, workspace_custom_impls, workspace_repr_c) = match &workspace_type.kind {
        TypeDefKind::Struct { derives, custom_impls, has_repr_c, .. } => (derives.clone(), custom_impls.clone(), *has_repr_c),
        TypeDefKind::Enum { derives, custom_impls, has_repr_c, .. } => (derives.clone(), custom_impls.clone(), *has_repr_c),
        _ => return modifications, // Skip non-struct/enum types
    };
    
    // Compare derives
    let workspace_derive_set: HashSet<_> = workspace_derives.iter().collect();
    let api_derive_set: HashSet<_> = api_info.derives.iter().collect();
    
    // Derives added in workspace (not in api.json)
    for derive in workspace_derive_set.difference(&api_derive_set) {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::DeriveAdded {
                derive_name: (*derive).clone(),
            },
        });
    }
    
    // Derives removed from workspace (in api.json but not workspace)
    for derive in api_derive_set.difference(&workspace_derive_set) {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::DeriveRemoved {
                derive_name: (*derive).clone(),
            },
        });
    }
    
    // Compare custom_impls
    let workspace_impl_set: HashSet<_> = workspace_custom_impls.iter().collect();
    let api_impl_set: HashSet<_> = api_info.custom_impls.iter().collect();
    
    // Custom impls added in workspace (not in api.json)
    for impl_name in workspace_impl_set.difference(&api_impl_set) {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::CustomImplAdded {
                impl_name: (*impl_name).clone(),
            },
        });
    }
    
    // Custom impls removed from workspace (in api.json but not workspace)
    for impl_name in api_impl_set.difference(&workspace_impl_set) {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::CustomImplRemoved {
                impl_name: (*impl_name).clone(),
            },
        });
    }
    
    // Compare repr(C)
    if workspace_repr_c != api_info.has_repr_c {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::ReprCChanged {
                old_repr_c: api_info.has_repr_c,
                new_repr_c: workspace_repr_c,
            },
        });
    }
    
    modifications
}

/// Field info for comparison including ref_kind
#[derive(Debug, Clone)]
struct FieldCompareInfo {
    name: String,
    ty: String,
    ref_kind: crate::api::RefKind,
}

/// Compare struct fields between workspace type and api.json type
fn compare_struct_fields(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    let mut modifications = Vec::new();
    
    // Get workspace fields (expand MacroGenerated types)
    let expanded = workspace_type.expand_macro_generated();
    let workspace_fields: Vec<FieldCompareInfo> = match expanded {
        TypeDefKind::Struct { fields, .. } => {
            fields.iter()
                .map(|(name, field)| FieldCompareInfo {
                    name: name.clone(),
                    ty: field.ty.clone(),
                    ref_kind: field.ref_kind,
                })
                .collect()
        }
        _ => return modifications, // Not a struct
    };
    
    // Get api.json fields
    let api_fields = match &api_info.struct_fields {
        Some(fields) => fields.clone(),
        None => {
            // api.json has no fields but workspace does - all fields need to be added
            for field in workspace_fields {
                modifications.push(TypeModification {
                    type_name: type_name.to_string(),
                    kind: ModificationKind::FieldAdded {
                        field_name: field.name,
                        field_type: field.ty,
                        ref_kind: field.ref_kind,
                    },
                });
            }
            return modifications;
        }
    };
    
    // Create lookup maps - for api.json, we also need ref_kind
    let workspace_map: HashMap<&str, &FieldCompareInfo> = workspace_fields.iter()
        .map(|f| (f.name.as_str(), f))
        .collect();
    let api_map: HashMap<&str, (&str, crate::api::RefKind)> = api_fields.iter()
        .map(|(n, t, rk)| (n.as_str(), (t.as_str(), *rk)))
        .collect();
    
    // Check for added/changed fields (in workspace but not in api.json or different type)
    for field in &workspace_fields {
        match api_map.get(field.name.as_str()) {
            None => {
                // Field added in workspace
                modifications.push(TypeModification {
                    type_name: type_name.to_string(),
                    kind: ModificationKind::FieldAdded {
                        field_name: field.name.clone(),
                        field_type: field.ty.clone(),
                        ref_kind: field.ref_kind,
                    },
                });
            }
            Some((api_type, api_ref_kind)) => {
                // Field exists - check if type or ref_kind matches
                let type_changed = normalize_type_name(&field.ty) != normalize_type_name(api_type);
                let ref_kind_changed = field.ref_kind != *api_ref_kind;
                
                // DEBUG: Log when ref_kind differs for c_void fields
                if field.ty == "c_void" && ref_kind_changed {
                    eprintln!("[DEBUG] {}.{}: workspace ref_kind={:?}, api ref_kind={:?}", 
                        type_name, field.name, field.ref_kind, api_ref_kind);
                }
                
                if type_changed || ref_kind_changed {
                    modifications.push(TypeModification {
                        type_name: type_name.to_string(),
                        kind: ModificationKind::FieldTypeChanged {
                            field_name: field.name.clone(),
                            old_type: api_type.to_string(),
                            new_type: field.ty.clone(),
                            ref_kind: field.ref_kind,
                        },
                    });
                }
            }
        }
    }
    
    // Check for removed fields (in api.json but not in workspace)
    for (field_name, _, _) in &api_fields {
        if !workspace_map.contains_key(field_name.as_str()) {
            modifications.push(TypeModification {
                type_name: type_name.to_string(),
                kind: ModificationKind::FieldRemoved {
                    field_name: field_name.clone(),
                },
            });
        }
    }
    
    modifications
}

/// Compare enum variants between workspace type and api.json type
fn compare_enum_variants(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    let mut modifications = Vec::new();
    
    // Get workspace variants (expand MacroGenerated types)
    let expanded = workspace_type.expand_macro_generated();
    let workspace_variants: Vec<(String, Option<String>)> = match expanded {
        TypeDefKind::Enum { variants, .. } => {
            variants.iter()
                .map(|(name, variant)| (name.clone(), variant.ty.clone()))
                .collect()
        }
        _ => return modifications, // Not an enum
    };
    
    // Get api.json variants
    let api_variants = match &api_info.enum_variants {
        Some(variants) => variants.clone(),
        None => {
            // api.json has no variants but workspace does - all variants need to be added
            for (variant_name, _) in workspace_variants {
                modifications.push(TypeModification {
                    type_name: type_name.to_string(),
                    kind: ModificationKind::VariantAdded {
                        variant_name,
                    },
                });
            }
            return modifications;
        }
    };
    
    // Create lookup maps
    let workspace_map: HashMap<&str, &Option<String>> = workspace_variants.iter()
        .map(|(n, t)| (n.as_str(), t))
        .collect();
    let api_map: HashMap<&str, &Option<String>> = api_variants.iter()
        .map(|(n, t)| (n.as_str(), t))
        .collect();
    
    // Check for added/changed variants (in workspace but not in api.json or different type)
    for (variant_name, variant_type) in &workspace_variants {
        match api_map.get(variant_name.as_str()) {
            None => {
                // Variant added in workspace
                modifications.push(TypeModification {
                    type_name: type_name.to_string(),
                    kind: ModificationKind::VariantAdded {
                        variant_name: variant_name.clone(),
                    },
                });
            }
            Some(api_type) => {
                // Variant exists - check if type matches
                let workspace_normalized = variant_type.as_ref().map(|t| normalize_type_name(t));
                let api_normalized = api_type.as_ref().map(|t| normalize_type_name(t));
                
                if workspace_normalized != api_normalized {
                    modifications.push(TypeModification {
                        type_name: type_name.to_string(),
                        kind: ModificationKind::VariantTypeChanged {
                            variant_name: variant_name.clone(),
                            old_type: (*api_type).clone(),
                            new_type: variant_type.clone(),
                        },
                    });
                }
            }
        }
    }
    
    // Check for removed variants (in api.json but not in workspace)
    for (variant_name, _) in &api_variants {
        if !workspace_map.contains_key(variant_name.as_str()) {
            modifications.push(TypeModification {
                type_name: type_name.to_string(),
                kind: ModificationKind::VariantRemoved {
                    variant_name: variant_name.clone(),
                },
            });
        }
    }
    
    modifications
}

/// Compare callback_typedef between workspace type and api.json type
fn compare_callback_typedef(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    use super::type_index::RefKind;
    
    let mut modifications = Vec::new();
    
    // Get workspace callback info (expand MacroGenerated types)
    let expanded = workspace_type.expand_macro_generated();
    let (workspace_args, workspace_returns): (Vec<CallbackArgInfo>, Option<String>) = match expanded {
        TypeDefKind::CallbackTypedef { args, returns } => {
            let arg_vec: Vec<CallbackArgInfo> = args.iter()
                .map(|arg| CallbackArgInfo {
                    name: arg.name.clone(),
                    ty: arg.ty.clone(),
                    ref_kind: arg.ref_kind,
                })
                .collect();
            (arg_vec, returns)
        }
        _ => return modifications, // Not a callback typedef
    };
    
    // Get api.json callback info
    match (&api_info.callback_args, &api_info.callback_returns) {
        (None, _) => {
            // api.json has no callback_typedef but workspace does - needs to be added
            modifications.push(TypeModification {
                type_name: type_name.to_string(),
                kind: ModificationKind::CallbackTypedefAdded {
                    args: workspace_args,
                    returns: workspace_returns,
                },
            });
        }
        (Some(api_args), api_returns) => {
            // Compare args - check both type and ref_kind
            // If ANY argument differs, we need to replace the entire callback_typedef
            // because ChangeCallbackArg doesn't work correctly in the patch application
            let mut any_arg_differs = false;
            
            // Check argument count
            if workspace_args.len() != api_args.len() {
                any_arg_differs = true;
            } else {
                for (i, workspace_arg) in workspace_args.iter().enumerate() {
                    if let Some((api_arg_ty, api_arg_ref)) = api_args.get(i) {
                        let workspace_normalized = normalize_type_name(&workspace_arg.ty);
                        let api_normalized = normalize_type_name(api_arg_ty);
                        
                        // Convert api ref string to RefKind for comparison
                        let api_ref_kind = match api_arg_ref.as_deref() {
                            Some("ref") => Some(RefKind::Ref),
                            Some("refmut") => Some(RefKind::RefMut),
                            Some("value") | None => Some(RefKind::Value),
                            _ => None,
                        };
                        
                        // Check if type or ref_kind differs
                        let type_differs = workspace_normalized != api_normalized;
                        let ref_differs = api_ref_kind != Some(workspace_arg.ref_kind);
                        
                        if type_differs || ref_differs {
                            any_arg_differs = true;
                            break;
                        }
                    }
                }
            }
            
            // Compare return type
            let workspace_ret_normalized = workspace_returns.as_ref().map(|t| normalize_type_name(t));
            let api_ret_normalized = api_returns.as_ref().map(|t| normalize_type_name(t));
            let return_differs = workspace_ret_normalized != api_ret_normalized;
            
            // If anything differs, replace the entire callback_typedef
            if any_arg_differs || return_differs {
                modifications.push(TypeModification {
                    type_name: type_name.to_string(),
                    kind: ModificationKind::CallbackTypedefAdded {
                        args: workspace_args,
                        returns: workspace_returns,
                    },
                });
            }
        }
    }
    
    modifications
}

/// Compare type_alias between workspace type and api.json type
fn compare_type_alias(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    let mut modifications = Vec::new();
    
    // Get workspace type alias info
    let (workspace_target, workspace_generic_args): (String, Vec<String>) = match &workspace_type.kind {
        TypeDefKind::TypeAlias { target, generic_base, generic_args } => {
            // Use generic_base if available, otherwise use target
            let base = generic_base.clone().unwrap_or_else(|| target.clone());
            (base, generic_args.clone())
        }
        _ => return modifications, // Not a type alias
    };
    
    // Get api.json type alias
    match &api_info.type_alias_target {
        None => {
            // api.json has no type_alias but workspace does - needs to be added
            modifications.push(TypeModification {
                type_name: type_name.to_string(),
                kind: ModificationKind::TypeAliasAdded {
                    target: workspace_target,
                    generic_args: workspace_generic_args,
                },
            });
        }
        Some(api_target) => {
            // Compare targets (normalized)
            if normalize_type_name(&workspace_target) != normalize_type_name(api_target) {
                modifications.push(TypeModification {
                    type_name: type_name.to_string(),
                    kind: ModificationKind::TypeAliasTargetChanged {
                        old_target: api_target.clone(),
                        new_target: workspace_target,
                        new_generic_args: workspace_generic_args,
                    },
                });
            }
        }
    }
    
    modifications
}

/// Compare generic_params between workspace type and api.json type
fn compare_generic_params(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    let mut modifications = Vec::new();
    
    // Get workspace generic params
    let workspace_generic_params: Vec<String> = match &workspace_type.kind {
        TypeDefKind::Struct { generic_params, .. } => generic_params.clone(),
        TypeDefKind::Enum { generic_params, .. } => generic_params.clone(),
        _ => return modifications, // TypeAlias/Callback don't have their own generic_params
    };
    
    // Compare with api.json generic_params
    if workspace_generic_params != api_info.generic_params {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::GenericParamsChanged {
                old_params: api_info.generic_params.clone(),
                new_params: workspace_generic_params,
            },
        });
    }
    
    modifications
}

/// Normalize a type name for comparison (remove whitespace, handle Az prefix)
fn normalize_type_name(type_name: &str) -> String {
    let trimmed = type_name.replace(" ", "");
    // Remove Az prefix for comparison
    if trimmed.starts_with("Az") && trimmed.len() > 2 {
        let third_char = trimmed.chars().nth(2);
        if third_char.map(|c| c.is_uppercase()).unwrap_or(false) {
            return trimmed[2..].to_string();
        }
    }
    trimmed
}

/// Get the kind of a type from the index
fn get_type_kind(index: &TypeIndex, type_name: &str) -> String {
    if let Some(typedef) = index.resolve(type_name, None) {
        match &typedef.kind {
            super::type_index::TypeDefKind::Struct { .. } => "struct",
            super::type_index::TypeDefKind::Enum { .. } => "enum",
            super::type_index::TypeDefKind::TypeAlias { .. } => "type_alias",
            super::type_index::TypeDefKind::CallbackTypedef { .. } => "callback",
            super::type_index::TypeDefKind::MacroGenerated { kind, .. } => {
                match kind {
                    super::type_index::MacroGeneratedKind::Vec => "vec",
                    super::type_index::MacroGeneratedKind::VecDestructor => "vec_destructor",
                    super::type_index::MacroGeneratedKind::VecDestructorType => "callback_typedef",
                    super::type_index::MacroGeneratedKind::Option => "option",
                    super::type_index::MacroGeneratedKind::OptionEnumWrapper => "option_wrapper",
                    super::type_index::MacroGeneratedKind::Result => "result",
                    super::type_index::MacroGeneratedKind::CallbackWrapper => "callback_wrapper",
                    super::type_index::MacroGeneratedKind::CallbackValue => "callback_value",
                }
            }
        }.to_string()
    } else {
        "unknown".to_string()
    }
}

/// Get the kind, fields, variants, and derives of a type from the index
/// This expands MacroGenerated types to get their actual fields
fn get_type_kind_with_fields(
    index: &TypeIndex, 
    type_name: &str
) -> (String, Option<Vec<(String, String, String)>>, Option<Vec<(String, Option<String>)>>, Vec<String>) {
    if let Some(typedef) = index.resolve(type_name, None) {
        // Get the expanded kind (handles MacroGenerated types)
        let expanded = typedef.expand_macro_generated();
        
        let kind_str = match &typedef.kind {
            super::type_index::TypeDefKind::Struct { .. } => "struct",
            super::type_index::TypeDefKind::Enum { .. } => "enum",
            super::type_index::TypeDefKind::TypeAlias { .. } => "type_alias",
            super::type_index::TypeDefKind::CallbackTypedef { .. } => "callback",
            super::type_index::TypeDefKind::MacroGenerated { kind, .. } => {
                match kind {
                    super::type_index::MacroGeneratedKind::Vec => "struct",
                    super::type_index::MacroGeneratedKind::VecDestructor => "enum",
                    super::type_index::MacroGeneratedKind::VecDestructorType => "callback_typedef",
                    super::type_index::MacroGeneratedKind::Option => "enum",
                    super::type_index::MacroGeneratedKind::OptionEnumWrapper => "struct",
                    super::type_index::MacroGeneratedKind::Result => "enum",
                    super::type_index::MacroGeneratedKind::CallbackWrapper => "struct",
                    super::type_index::MacroGeneratedKind::CallbackValue => "struct",
                }
            }
        };
        
        // Extract fields/variants from the EXPANDED kind
        match expanded {
            super::type_index::TypeDefKind::Struct { fields, derives, .. } => {
                let field_vec: Vec<(String, String, String)> = fields.iter()
                    .map(|(name, field)| (name.clone(), field.ty.clone(), field.ref_kind.as_str().to_string()))
                    .collect();
                (kind_str.to_string(), Some(field_vec), None, derives)
            }
            super::type_index::TypeDefKind::Enum { variants, derives, .. } => {
                let variant_vec: Vec<(String, Option<String>)> = variants.iter()
                    .map(|(name, variant)| (name.clone(), variant.ty.clone()))
                    .collect();
                (kind_str.to_string(), None, Some(variant_vec), derives)
            }
            _ => (kind_str.to_string(), None, None, vec![])
        }
    } else {
        ("unknown".to_string(), None, None, vec![])
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_simple_type_name() {
        assert_eq!(extract_simple_type_name("FontCache"), "FontCache");
        assert_eq!(extract_simple_type_name("*const FontCache"), "FontCache");
        assert_eq!(extract_simple_type_name("&FontCache"), "FontCache");
        assert_eq!(extract_simple_type_name("azul_core::dom::Dom"), "Dom");
        assert_eq!(extract_simple_type_name("Option<Foo>"), "Option");
    }

    #[test]
    fn test_paths_are_equivalent() {
        assert!(paths_are_equivalent("azul_core::dom::Dom", "azul_core::dom::Dom"));
        assert!(paths_are_equivalent("azul_core::resources::FontCache", "azul_css::resources::FontCache"));
        assert!(!paths_are_equivalent("azul_core::dom::Dom", "azul_core::window::Dom"));
    }

    #[test]
    fn test_deduplication() {
        let mut seen: HashSet<String> = HashSet::new();
        
        // First insertion succeeds
        assert!(seen.insert("FontCache:azul_core::resources::FontCache".to_string()));
        
        // Duplicate insertion fails
        assert!(!seen.insert("FontCache:azul_core::resources::FontCache".to_string()));
    }

    #[test]
    fn test_strip_az_prefix() {
        // Should strip "Az" prefix when followed by uppercase
        assert_eq!(strip_az_prefix("AzStringPair"), "StringPair");
        assert_eq!(strip_az_prefix("AzString"), "String");
        assert_eq!(strip_az_prefix("AzCallback"), "Callback");
        
        // Should NOT strip when not followed by uppercase
        assert_eq!(strip_az_prefix("Azure"), "Azure");
        assert_eq!(strip_az_prefix("Azimuth"), "Azimuth");
        
        // Should NOT strip when no Az prefix
        assert_eq!(strip_az_prefix("StringPair"), "StringPair");
        assert_eq!(strip_az_prefix("Dom"), "Dom");
        
        // Edge cases
        assert_eq!(strip_az_prefix("Az"), "Az");
        assert_eq!(strip_az_prefix("A"), "A");
        assert_eq!(strip_az_prefix(""), "");
    }

    #[test]
    fn test_type_names_match() {
        // Exact match
        assert!(type_names_match("StringPair", "StringPair"));
        assert!(type_names_match("Dom", "Dom"));
        
        // Az-prefix match (workspace has Az, api.json doesn't)
        assert!(type_names_match("AzStringPair", "StringPair"));
        assert!(type_names_match("AzString", "String"));
        
        // No match
        assert!(!type_names_match("StringPair", "StringPairVec"));
        assert!(!type_names_match("AzStringPair", "StringPairVec"));
    }

    #[test]
    fn test_workspace_name_to_api_name() {
        assert_eq!(workspace_name_to_api_name("AzStringPair"), "StringPair");
        assert_eq!(workspace_name_to_api_name("StringPair"), "StringPair");
        assert_eq!(workspace_name_to_api_name("Dom"), "Dom");
    }
}
