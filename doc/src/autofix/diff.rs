//! API Diff Generation V2
//!
//! This module compares the expected types (from workspace) with the current
//! types (from api.json) and generates patches for differences.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use anyhow::Result;

use super::{
    module_map::get_correct_module,
    type_index::{TypeDefKind, TypeDefinition, TypeIndex},
    type_resolver::{ResolutionContext, ResolvedType, ResolvedTypeSet, TypeResolver, TypeWarning},
    utils::canonicalize_option_type_name,
};
use crate::api::ApiData;

/// Derives that should be ignored in diff generation.
/// These are internal implementation details and should not be exposed in the C API.
const BLACKLISTED_DERIVES: &[&str] = &[
    "strum_macros :: EnumIter",
    "strum_macros::EnumIter",
    "EnumIter",
    "strum_macros :: EnumString",
    "strum_macros::EnumString",
    "EnumString",
    "strum_macros :: Display",
    "strum_macros::Display",
    "strum_macros :: AsRefStr",
    "strum_macros::AsRefStr",
    "AsRefStr",
    "strum_macros :: IntoStaticStr",
    "strum_macros::IntoStaticStr",
    "IntoStaticStr",
];

/// Check if a derive should be ignored in diff generation
fn is_derive_blacklisted(derive_name: &str) -> bool {
    BLACKLISTED_DERIVES
        .iter()
        .any(|&b| b == derive_name || derive_name.contains(b))
}

// data structures
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
    pub kind: String, // "struct", "enum", "callback_typedef", etc.
    /// Struct fields (for struct types): (field_name, field_type, ref_kind)
    pub struct_fields: Option<Vec<(String, String, String)>>, // (field_name, field_type, ref_kind)
    /// Enum variants (for enum types)
    pub enum_variants: Option<Vec<(String, Option<String>)>>, // (variant_name, variant_type)
    /// Derives from source code
    pub derives: Vec<String>,
    /// Callback typedef definition (for function pointer types)
    pub callback_typedef: Option<CallbackTypedefInfo>,
}

/// Information about a callback typedef (function pointer type)
#[derive(Debug, Clone)]
pub struct CallbackTypedefInfo {
    /// Arguments to the callback function: (arg_type, ref_kind)
    pub fn_args: Vec<(String, String)>,
    /// Return type (None = void)
    pub returns: Option<String>,
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

/// A struct field with name, type, and reference kind (in declaration order)
#[derive(Debug, Clone)]
pub struct StructFieldInfo {
    pub name: String,
    pub ty: String,
    pub ref_kind: crate::api::RefKind,
}

/// An enum variant with name and optional payload type (in declaration order)
#[derive(Debug, Clone)]
pub struct EnumVariantInfo {
    pub name: String,
    pub ty: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ModificationKind {
    /// Replace ALL struct fields with these fields in the correct order.
    /// This is used instead of individual FieldAdded/FieldRemoved/FieldTypeChanged
    /// because for repr(C) structs, field ORDER matters for memory layout.
    StructFieldsReplaced {
        fields: Vec<StructFieldInfo>,
    },
    /// Replace ALL enum variants with these variants in the correct order.
    /// Similar to StructFieldsReplaced - order matters for discriminant values.
    EnumVariantsReplaced {
        variants: Vec<EnumVariantInfo>,
    },
    /// Remove struct_fields entirely (type changed from struct to enum/type_alias/callback)
    StructFieldsRemoved,
    /// Remove enum_fields entirely (type changed from enum to struct/type_alias/callback)
    EnumFieldsRemoved,
    // Legacy individual field operations (kept for backwards compatibility, but not generated)
    FieldAdded {
        field_name: String,
        field_type: String,
        ref_kind: crate::api::RefKind,
    },
    FieldRemoved {
        field_name: String,
    },
    FieldTypeChanged {
        field_name: String,
        old_type: String,
        new_type: String,
        ref_kind: crate::api::RefKind,
    },
    VariantAdded {
        variant_name: String,
    },
    VariantRemoved {
        variant_name: String,
    },
    VariantTypeChanged {
        variant_name: String,
        old_type: Option<String>,
        new_type: Option<String>,
    },
    DeriveAdded {
        derive_name: String,
    },
    DeriveRemoved {
        derive_name: String,
    },
    CustomImplAdded {
        impl_name: String,
    },
    CustomImplRemoved {
        impl_name: String,
    },
    /// repr attribute changed (e.g., "C" -> "C, u8", or None -> Some("C"))
    ReprChanged {
        old_repr: Option<String>,
        new_repr: Option<String>,
    },
    /// Callback typedef needs to be added (type exists but has no callback_typedef)
    CallbackTypedefAdded {
        args: Vec<CallbackArgInfo>,
        returns: Option<String>,
    },
    /// Callback typedef arg changed
    CallbackArgChanged {
        arg_index: usize,
        old_type: String,
        new_type: String,
        old_ref_kind: Option<super::type_index::RefKind>,
        new_ref_kind: super::type_index::RefKind,
    },
    /// Callback typedef return changed
    CallbackReturnChanged {
        old_type: Option<String>,
        new_type: Option<String>,
    },
    /// Type alias needs to be added (type exists but has no type_alias)
    TypeAliasAdded {
        target: String,
        generic_args: Vec<String>,
    },
    /// Type alias target changed
    TypeAliasTargetChanged {
        old_target: String,
        new_target: String,
        new_generic_args: Vec<String>,
    },
    /// Generic params changed (e.g., PhysicalSize<T> missing generic_params: ["T"])
    GenericParamsChanged {
        old_params: Vec<String>,
        new_params: Vec<String>,
    },
    /// Function self parameter mismatch (missing or wrong kind)
    FunctionSelfMismatch {
        fn_name: String,
        expected_self: Option<String>, // "ref", "refmut", "value", or None for static
        actual_self: Option<String>,   // what api.json has
    },
    /// Function argument count mismatch
    FunctionArgCountMismatch {
        fn_name: String,
        expected_count: usize,
        actual_count: usize,
    },
    /// Missing Vec functions (generated by impl_vec! macro)
    /// Contains the list of function names that should be added
    VecFunctionsMissing {
        missing_functions: Vec<String>,
        element_type: String,
    },
    /// Missing Option type required by Vec's c_get function
    /// This indicates that an OptionX type needs to be added to api.json
    /// before the Vec can have its c_get function
    VecMissingOptionType {
        /// The Vec type that needs this Option type (e.g., "MenuItemVec")
        vec_type: String,
        /// The element type of the Vec (e.g., "MenuItem")
        element_type: String,
        /// The required Option type name (e.g., "OptionMenuItem")
        option_type_name: String,
    },
    /// Missing Slice type required by Vec's as_c_slice functions
    /// This indicates that a FooVecSlice type needs to be added to api.json
    VecMissingSliceType {
        /// The Vec type that needs this Slice type (e.g., "MenuItemVec")
        vec_type: String,
        /// The element type of the Vec (e.g., "MenuItem")
        element_type: String,
        /// The required Slice type name (e.g., "MenuItemVecSlice")
        slice_type_name: String,
    },
}

// api type resolution
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
    pub found: BTreeMap<String, FoundType>,
    /// Types that could not be found (with their api.json path)
    pub missing: BTreeMap<String, MissingType>,
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
            if !api_path.is_empty()
                && api_path != workspace_path
                && !paths_are_equivalent(api_path, workspace_path)
            {
                resolution.path_mismatches.push(PathMismatch {
                    type_name: type_name.to_string(),
                    api_path: api_path.to_string(),
                    workspace_path: workspace_path.clone(),
                });
            }

            resolution.found.insert(
                type_name.to_string(),
                FoundType {
                    type_name: type_name.to_string(),
                    api_path: api_path.to_string(),
                    workspace_path: workspace_path.clone(),
                },
            );
        }
        None => {
            // Only mark as missing if it has an api_path (it's a class definition, not just a
            // reference)
            if !api_path.is_empty() {
                resolution.missing.insert(
                    type_name.to_string(),
                    MissingType {
                        type_name: type_name.to_string(),
                        api_path: api_path.to_string(),
                    },
                );
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
            resolution.found.insert(
                base_name.clone(),
                FoundType {
                    type_name: base_name,
                    api_path: String::new(), // Unknown from just a type reference
                    workspace_path: typedef.full_path.clone(),
                },
            );
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

// az-prefix handling
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

// diff generation
/// Generate diff between expected and current API types
pub fn generate_diff(
    expected: &ResolvedTypeSet,
    api_resolution: &ApiTypeResolution,
    index: &TypeIndex,
) -> ApiDiff {
    let mut diff = ApiDiff::default();
    let mut seen_fixes: BTreeSet<String> = BTreeSet::new();
    let mut seen_additions: BTreeSet<String> = BTreeSet::new();

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
        // Skip if it's in api.json but just couldn't be found in workspace (that's a removal, not
        // addition)
        if api_resolution.missing.contains_key(type_name) {
            continue;
        }

        // This type is used by workspace functions but not in api.json - should be added
        if seen_additions.insert(type_name.clone()) {
            // Look up the type to get its kind and fields
            let (kind, struct_fields, enum_variants, derives, callback_typedef) =
                if let Some(typedef) = index.resolve(type_name, None) {
                    // Get the expanded kind (handles MacroGenerated types)
                    let expanded = typedef.expand_macro_generated();

                    let kind_str = match &typedef.kind {
                        super::type_index::TypeDefKind::Struct { .. } => "struct",
                        super::type_index::TypeDefKind::Enum { .. } => "enum",
                        super::type_index::TypeDefKind::TypeAlias { .. } => "type_alias",
                        super::type_index::TypeDefKind::CallbackTypedef { .. } => {
                            "callback_typedef"
                        }
                        super::type_index::TypeDefKind::MacroGenerated { kind, .. } => match kind {
                            super::type_index::MacroGeneratedKind::Vec => "struct",
                            super::type_index::MacroGeneratedKind::VecDestructor => "enum",
                            super::type_index::MacroGeneratedKind::VecDestructorType => {
                                "callback_typedef"
                            }
                            super::type_index::MacroGeneratedKind::VecSlice => "struct",
                            super::type_index::MacroGeneratedKind::Option => "enum",
                            super::type_index::MacroGeneratedKind::OptionEnumWrapper => "struct",
                            super::type_index::MacroGeneratedKind::Result => "enum",
                            super::type_index::MacroGeneratedKind::CallbackWrapper => "struct",
                            super::type_index::MacroGeneratedKind::CallbackValue => "struct",
                        },
                    };

                    // Extract fields/variants from the EXPANDED kind
                    let (fields, variants, derives, callback_typedef) = match expanded {
                        super::type_index::TypeDefKind::Struct {
                            fields, derives, ..
                        } => {
                            let field_vec: Vec<(String, String, String)> = fields
                                .iter()
                                .map(|(name, field)| {
                                    (
                                        name.clone(),
                                        field.ty.clone(),
                                        field.ref_kind.as_str().to_string(),
                                    )
                                })
                                .collect();
                            (Some(field_vec), None, derives, None)
                        }
                        super::type_index::TypeDefKind::Enum {
                            variants, derives, ..
                        } => {
                            let variant_vec: Vec<(String, Option<String>)> = variants
                                .iter()
                                .map(|(name, variant)| (name.clone(), variant.ty.clone()))
                                .collect();
                            (None, Some(variant_vec), derives, None)
                        }
                        super::type_index::TypeDefKind::CallbackTypedef {
                            args, returns, ..
                        } => {
                            let callback_info = CallbackTypedefInfo {
                                fn_args: args
                                    .iter()
                                    .map(|arg| (arg.ty.clone(), arg.ref_kind.as_str().to_string()))
                                    .collect(),
                                returns: returns.clone(),
                            };
                            (None, None, vec![], Some(callback_info))
                        }
                        super::type_index::TypeDefKind::TypeAlias { .. } => {
                            (None, None, vec![], None)
                        }
                        super::type_index::TypeDefKind::MacroGenerated { .. } => {
                            // This shouldn't happen after expand_macro_generated()
                            (None, None, vec![], None)
                        }
                    };

                    (kind_str, fields, variants, derives, callback_typedef)
                } else {
                    ("unknown", None, None, vec![], None)
                };

            diff.additions.push(TypeAddition {
                type_name: type_name.clone(),
                full_path: resolved.full_path.clone(),
                kind: kind.to_string(),
                struct_fields,
                enum_variants,
                derives,
                callback_typedef,
            });
        }
    }

    // 3. Types in api.json that couldn't be found in workspace - mark for removal
    for (type_name, missing_info) in &api_resolution.missing {
        diff.removals
            .push(format!("{}:{}", type_name, missing_info.api_path));
    }

    diff
}

// main entry point
/// Run the full diff analysis
///
/// The logic:
/// 1. Build workspace type index (source of truth for TYPE DEFINITIONS)
/// 2. Extract functions from api.json (source of truth for API SURFACE)
/// 3. For each api.json function, resolve all types RECURSIVELY using WORKSPACE INDEX
/// 4. This gives us "expected" state - all types the API needs with their current workspace paths
/// 5. Compare expected vs current api.json → generate diff
///
/// Returns: (ApiDiff, TypeIndex) tuple for further analysis
pub fn analyze_api_diff(
    workspace_root: &Path,
    api_data: &ApiData,
    verbose: bool,
) -> Result<(ApiDiff, TypeIndex, Vec<TypeWarning>)> {
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
            eprintln!(
                "  {} {}: {}",
                format!("[{}]", i + 1).dimmed(),
                warning.type_expr.red(),
                warning.message.yellow()
            );
            if !warning.context.is_empty() {
                eprintln!("      {} {}", "in".dimmed(), warning.context.dimmed());
            }
            if let Some(ref origin) = warning.origin {
                eprintln!("      {} {}", "from".cyan(), origin.cyan());
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

    Ok((diff, index, expected.warnings))
}

/// Resolve all types referenced by api.json functions, using the WORKSPACE INDEX
/// This gives us the "expected" state - what types the API needs with their current paths
fn resolve_api_functions_with_workspace_index(
    index: &TypeIndex,
    api_data: &ApiData,
    verbose: bool,
) -> ResolvedTypeSet {
    use super::type_resolver::{ResolutionContext, TypeResolver};

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
                        // Constructors implicitly return Self, so the class type is reachable
                        // This ensures types like App are not marked for removal just because
                        // the constructor doesn't have an explicit "returns" field
                        let self_context =
                            format!("{}::{} -> Self (implicit)", class_name, ctor_name);
                        resolver.resolve_type_with_context(class_name, &ctx, Some(&self_context));

                        // Resolve parameter types - fn_args is Vec<IndexMap<String, String>>
                        for arg_map in &ctor_data.fn_args {
                            for (arg_name, arg_type) in arg_map {
                                let parent_context =
                                    format!("{}::{} arg '{}'", class_name, ctor_name, arg_name);
                                resolver.resolve_type_with_context(
                                    arg_type,
                                    &ctx,
                                    Some(&parent_context),
                                );
                            }
                        }

                        // Resolve return type (if explicit, though constructors usually return
                        // Self)
                        if let Some(ref returns) = ctor_data.returns {
                            let parent_context = format!("{}::{} -> return", class_name, ctor_name);
                            resolver.resolve_type_with_context(
                                &returns.r#type,
                                &ctx,
                                Some(&parent_context),
                            );
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
                                let parent_context =
                                    format!("{}::{} arg '{}'", class_name, fn_name, arg_name);
                                resolver.resolve_type_with_context(
                                    arg_type,
                                    &ctx,
                                    Some(&parent_context),
                                );
                            }
                        }

                        // Resolve return type
                        if let Some(ref returns) = fn_data.returns {
                            let parent_context = format!("{}::{} -> return", class_name, fn_name);
                            resolver.resolve_type_with_context(
                                &returns.r#type,
                                &ctx,
                                Some(&parent_context),
                            );
                        }
                    }
                }

                // Resolve struct_fields types
                // These are the types used in struct field definitions
                if let Some(ref fields) = class_data.struct_fields {
                    for field_map in fields {
                        for (field_name, field_data) in field_map {
                            let parent_context = format!("{}.{}", class_name, field_name);
                            resolver.resolve_type_with_context(
                                &field_data.r#type,
                                &ctx,
                                Some(&parent_context),
                            );
                        }
                    }
                }

                // Resolve enum_fields variant types
                if let Some(ref variants) = class_data.enum_fields {
                    for variant_map in variants {
                        for (variant_name, variant_data) in variant_map {
                            if let Some(ref variant_type) = variant_data.r#type {
                                let parent_context = format!("{}::{}", class_name, variant_name);
                                resolver.resolve_type_with_context(
                                    variant_type,
                                    &ctx,
                                    Some(&parent_context),
                                );
                            }
                        }
                    }
                }

                // Resolve callback_typedef argument types
                // These are entry points for callbacks and their argument types need to be resolved
                if let Some(ref callback_def) = class_data.callback_typedef {
                    for (i, arg_data) in callback_def.fn_args.iter().enumerate() {
                        let parent_context = format!("{} callback arg[{}]", class_name, i);
                        resolver.resolve_type_with_context(
                            &arg_data.r#type,
                            &ctx,
                            Some(&parent_context),
                        );
                    }

                    // Resolve return type
                    if let Some(ref returns) = callback_def.returns {
                        let parent_context = format!("{} callback -> return", class_name);
                        resolver.resolve_type_with_context(
                            &returns.r#type,
                            &ctx,
                            Some(&parent_context),
                        );
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
/// Returns: BTreeMap<type_name, ApiTypeInfo>
fn collect_api_json_types(api_data: &ApiData) -> BTreeMap<String, ApiTypeInfo> {
    let mut types = BTreeMap::new();

    for (_version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                let path = class_data.external.clone().unwrap_or_default();
                let derives = class_data.derive.clone().unwrap_or_default();
                let custom_impls = class_data.custom_impls.clone().unwrap_or_default();
                let repr = class_data.repr.clone();

                // Extract struct fields with ref_kind
                let struct_fields = class_data.struct_fields.as_ref().map(|fields_vec| {
                    fields_vec
                        .iter()
                        .flat_map(|field_map| {
                            field_map.iter().map(|(name, data)| {
                                (name.clone(), data.r#type.clone(), data.ref_kind)
                            })
                        })
                        .collect()
                });

                // Extract enum variants
                let enum_variants = class_data.enum_fields.as_ref().map(|variants_vec| {
                    variants_vec
                        .iter()
                        .flat_map(|variant_map| {
                            variant_map
                                .iter()
                                .map(|(name, data)| (name.clone(), data.r#type.clone()))
                        })
                        .collect()
                });

                // Extract callback typedef - use RefKind directly, no string conversion
                let (callback_args, callback_returns) =
                    if let Some(ref callback_def) = class_data.callback_typedef {
                        let args: Vec<(String, crate::api::RefKind)> = callback_def
                            .fn_args
                            .iter()
                            .map(|arg| (arg.r#type.clone(), arg.ref_kind))
                            .collect();
                        let returns = callback_def.returns.as_ref().map(|r| r.r#type.clone());
                        (Some(args), returns)
                    } else {
                        (None, None)
                    };

                // Extract type alias - reconstruct full type including pointer prefix from ref_kind
                let (type_alias_target, type_alias_generic_args) = match &class_data.type_alias {
                    Some(ta) => {
                        // Reconstruct full type: ref_kind + target
                        let full_target = match ta.ref_kind {
                            crate::api::RefKind::ConstPtr => format!("*const {}", ta.target),
                            crate::api::RefKind::MutPtr => format!("*mut {}", ta.target),
                            crate::api::RefKind::Value => ta.target.clone(),
                            _ => ta.target.clone(), // For other ref kinds, just use target
                        };
                        (Some(full_target), ta.generic_args.clone())
                    }
                    None => (None, Vec::new()),
                };

                // Extract generic params
                let generic_params = class_data.generic_params.clone().unwrap_or_default();

                // Extract vec_element_type (for Vec types)
                let vec_element_type = class_data.vec_element_type.clone();

                // Extract functions (both regular functions and constructors)
                let mut functions = std::collections::BTreeMap::new();

                if let Some(ref fns) = class_data.functions {
                    for (fn_name, fn_data) in fns {
                        let fn_args: Vec<(String, String)> = fn_data
                            .fn_args
                            .iter()
                            .flat_map(|arg_map| {
                                arg_map.iter().map(|(arg_name, arg_type)| {
                                    (arg_name.clone(), arg_type.clone())
                                })
                            })
                            .collect();
                        let returns = fn_data.returns.as_ref().map(|r| r.r#type.clone());
                        functions.insert(fn_name.clone(), ApiFunctionInfo { fn_args, returns });
                    }
                }

                if let Some(ref ctors) = class_data.constructors {
                    for (ctor_name, ctor_data) in ctors {
                        let fn_args: Vec<(String, String)> = ctor_data
                            .fn_args
                            .iter()
                            .flat_map(|arg_map| {
                                arg_map.iter().map(|(arg_name, arg_type)| {
                                    (arg_name.clone(), arg_type.clone())
                                })
                            })
                            .collect();
                        let returns = ctor_data.returns.as_ref().map(|r| r.r#type.clone());
                        functions.insert(ctor_name.clone(), ApiFunctionInfo { fn_args, returns });
                    }
                }

                let has_constructors = class_data.constructors.as_ref().map_or(false, |c| !c.is_empty());

                types.insert(
                    class_name.clone(),
                    ApiTypeInfo {
                        path,
                        module: module_name.clone(),
                        derives,
                        custom_impls,
                        repr,
                        struct_fields,
                        enum_variants,
                        callback_args,
                        callback_returns,
                        type_alias_target,
                        type_alias_generic_args,
                        generic_params,
                        functions,
                        has_constructors,
                        vec_element_type,
                    },
                );
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
    pub repr: Option<String>,
    /// Struct fields: (field_name, field_type, ref_kind)
    pub struct_fields: Option<Vec<(String, String, crate::api::RefKind)>>,
    /// Enum variants: (variant_name, variant_type)
    pub enum_variants: Option<Vec<(String, Option<String>)>>,
    /// Callback typedef args: (arg_type, ref_kind) - uses RefKind directly, no string conversion
    pub callback_args: Option<Vec<(String, crate::api::RefKind)>>,
    /// Callback typedef return type
    pub callback_returns: Option<String>,
    /// Type alias target
    pub type_alias_target: Option<String>,
    /// Type alias generic args (e.g., ["u32"] for PhysicalSizeU32 = PhysicalSize<u32>)
    pub type_alias_generic_args: Vec<String>,
    /// Generic type parameters (e.g., ["T"] for PhysicalSize<T>)
    pub generic_params: Vec<String>,
    /// Functions from api.json: fn_name -> (arg_name, arg_type/ref_kind)
    pub functions: std::collections::BTreeMap<String, ApiFunctionInfo>,
    /// Whether this type has constructors (entry-point functions, not just methods)
    pub has_constructors: bool,
    /// Vec element type (for Vec types generated by impl_vec!)
    pub vec_element_type: Option<String>,
}

/// Information about a function from api.json
#[derive(Debug, Clone, Default)]
pub struct ApiFunctionInfo {
    /// Arguments: (arg_name, arg_type_or_ref_kind)
    pub fn_args: Vec<(String, String)>,
    /// Return type
    pub returns: Option<String>,
}

/// Generate diff between expected (workspace-resolved) and current (api.json) types
fn generate_diff_v2(
    expected: &ResolvedTypeSet,
    current_api_types: &BTreeMap<String, ApiTypeInfo>,
    index: &TypeIndex,
) -> ApiDiff {
    let mut diff = ApiDiff::default();
    let mut seen_additions: BTreeSet<String> = BTreeSet::new();
    let mut matched_api_types: BTreeSet<String> = BTreeSet::new();

    // Build mapping from element type to Option type (from impl_option! macros)
    // e.g., "f32" -> "OptionF32", "StringPair" -> "OptionStringPair"
    let element_to_option_map = build_element_to_option_map(current_api_types);

    // 1. Types in expected (resolved from workspace) but not in api.json → additions
    // Also check for Az-prefix matches (AzStringPair in workspace = StringPair in api.json)
    for (workspace_name, resolved) in &expected.resolved {
        let api_lookup_name = workspace_name_to_api_name(workspace_name);

        // Check if it exists in api.json (either exact match or without Az prefix)
        let api_match = if current_api_types.contains_key(workspace_name) {
            Some(workspace_name.as_str())
        } else if workspace_name != api_lookup_name
            && current_api_types.contains_key(api_lookup_name)
        {
            Some(api_lookup_name)
        } else {
            None
        };

        if let Some(matched_api_name) = api_match {
            // Type exists in both - mark as matched
            matched_api_types.insert(matched_api_name.to_string());

            // Check if path matches
            if let Some(api_info) = current_api_types.get(matched_api_name) {
                if !api_info.path.is_empty()
                    && !paths_are_equivalent(&api_info.path, &resolved.full_path)
                {
                    diff.path_fixes.push(PathFix {
                        type_name: matched_api_name.to_string(),
                        old_path: api_info.path.clone(),
                        new_path: resolved.full_path.clone(),
                    });
                }

                // Check for derive/impl changes and field/variant differences
                if let Some(typedef) = index.resolve(workspace_name, None) {
                    diff.modifications.extend(compare_derives_and_impls(
                        matched_api_name,
                        typedef,
                        api_info,
                    ));

                    // Check for type kind mismatch (e.g., struct_fields in api.json but workspace
                    // has enum)
                    diff.modifications.extend(check_type_kind_mismatch(
                        matched_api_name,
                        typedef,
                        api_info,
                    ));

                    // Check for struct field differences
                    diff.modifications.extend(compare_struct_fields(
                        matched_api_name,
                        typedef,
                        api_info,
                    ));

                    // Check for enum variant differences
                    diff.modifications.extend(compare_enum_variants(
                        matched_api_name,
                        typedef,
                        api_info,
                    ));

                    // Check for callback_typedef differences
                    diff.modifications.extend(compare_callback_typedef(
                        matched_api_name,
                        typedef,
                        api_info,
                    ));

                    // Check for type_alias differences
                    diff.modifications.extend(compare_type_alias(
                        matched_api_name,
                        typedef,
                        api_info,
                    ));

                    // Check for generic_params differences
                    diff.modifications.extend(compare_generic_params(
                        matched_api_name,
                        typedef,
                        api_info,
                    ));

                    // Check for function signature differences (self parameter, arg count)
                    diff.modifications.extend(compare_functions(
                        matched_api_name,
                        typedef,
                        api_info,
                    ));
                }

                // Check for missing Vec functions (generated by impl_vec! macro)
                // This check is independent of workspace type, as impl_vec! is a macro
                // Also checks for missing dependency types (OptionX, XVecSlice)
                diff.modifications.extend(check_vec_functions(
                    matched_api_name,
                    api_info,
                    current_api_types,
                    &element_to_option_map,
                ));

                // Check if type is in the wrong module
                if let Some(correct_module) = get_correct_module(matched_api_name, &api_info.module)
                {
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
                let (kind, struct_fields, enum_variants, derives, callback_typedef) =
                    get_type_kind_with_fields(index, workspace_name);
                diff.additions.push(TypeAddition {
                    type_name: api_lookup_name.to_string(),
                    full_path: resolved.full_path.clone(),
                    kind,
                    struct_fields,
                    enum_variants,
                    derives,
                    callback_typedef,
                });
            }
        }
    }

    // 2. Types in api.json but not matched from workspace → removals
    // Only protect Vec-required types if the Vec type itself is reachable.
    // This prevents dead circular type clusters (e.g. XmlComponentVec → XmlComponent
    // → OptionXmlComponent → ...) from being kept alive when nothing references them.
    let vec_required_types = collect_vec_required_types(
        current_api_types,
        &element_to_option_map,
        &matched_api_types,
    );
    // Also check for module moves on ALL api.json types (not just matched ones)
    for (api_name, api_info) in current_api_types {
        if !matched_api_types.contains(api_name) {
            // Before marking for removal, check if this type is required by Vec types
            // Vec types need Option types (for c_get) and Slice types (for as_c_slice)
            // These are macro-generated and not directly reachable via function signatures
            if !vec_required_types.contains(api_name) {
                // Type is in api.json but couldn't be resolved from workspace
                // This could mean:
                // a) Type was deleted from workspace
                // b) Type was renamed (different name now)
                // c) Type is no longer reachable from any function
                diff.removals
                    .push(format!("{}:{}", api_name, api_info.path));
            }
        }

        // Check if ANY type (matched or not) is in the wrong module
        // This ensures we move legacy module types even if they weren't resolved from workspace
        if let Some(correct_module) = get_correct_module(api_name, &api_info.module) {
            // Avoid duplicate moves (already added in matched types loop)
            let already_has_move = diff.module_moves.iter().any(|m| m.type_name == *api_name);

            if !already_has_move {
                diff.module_moves.push(ModuleMove {
                    type_name: api_name.to_string(),
                    from_module: api_info.module.clone(),
                    to_module: correct_module,
                });
            }
        }

        // 3. Check path fixes and modifications for ALL api.json types against workspace index
        // This catches types that aren't reachable from function signatures
        // but still have wrong paths or missing definitions (e.g., type_alias)
        if !matched_api_types.contains(api_name) {
            // Try to find this type in the workspace index
            // First try exact name, then with Az prefix
            let workspace_typedef = index
                .resolve(api_name, None)
                .or_else(|| index.resolve(&format!("Az{}", api_name), None));

            if let Some(typedef) = workspace_typedef {
                // Found in workspace - check if path matches
                if !api_info.path.is_empty()
                    && !paths_are_equivalent(&api_info.path, &typedef.full_path)
                {
                    // Path mismatch - need to fix
                    let already_has_fix = diff.path_fixes.iter().any(|f| f.type_name == *api_name);

                    if !already_has_fix {
                        diff.path_fixes.push(PathFix {
                            type_name: api_name.to_string(),
                            old_path: api_info.path.clone(),
                            new_path: typedef.full_path.clone(),
                        });
                    }
                }

                // Also check for modifications on unmatched types
                // This ensures type_alias, struct_fields, etc. are detected for all api.json types
                diff.modifications
                    .extend(compare_derives_and_impls(api_name, typedef, api_info));
                diff.modifications
                    .extend(check_type_kind_mismatch(api_name, typedef, api_info));
                diff.modifications
                    .extend(compare_struct_fields(api_name, typedef, api_info));
                diff.modifications
                    .extend(compare_enum_variants(api_name, typedef, api_info));
                diff.modifications
                    .extend(compare_callback_typedef(api_name, typedef, api_info));
                diff.modifications
                    .extend(compare_type_alias(api_name, typedef, api_info));
                diff.modifications
                    .extend(compare_generic_params(api_name, typedef, api_info));
            }
        }
    }

    // 4. Dead circular type cluster detection
    // Some types in api.json only reference each other but are not reachable
    // from any type that has public functions/constructors. These are dead types.
    // Example: XmlComponentVec → XmlComponent → OptionXmlComponent → XmlComponent (circular)
    // None of these are referenced by any function, but each one references the others,
    // so they all appear "matched" in the resolver.
    //
    // We must detect dead types on the MERGED state (current - removals + additions),
    // otherwise removing a dead parent can cause its children to be re-added as new types,
    // oscillating between add/remove cycles.
    let already_removed: BTreeSet<String> = diff.removals.iter()
        .map(|r| r.split(':').next().unwrap_or(r).to_string())
        .collect();

    // Build merged type set: current types minus removals, plus proposed additions
    let mut merged_types: BTreeMap<String, ApiTypeInfo> = current_api_types
        .iter()
        .filter(|(name, _)| !already_removed.contains(name.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Convert proposed additions to ApiTypeInfo for dead cluster detection
    for addition in &diff.additions {
        let struct_fields = addition.struct_fields.as_ref().map(|fields| {
            fields.iter().map(|(name, ty, ref_kind)| {
                let rk = match ref_kind.as_str() {
                    "constptr" => crate::api::RefKind::ConstPtr,
                    "mutptr" => crate::api::RefKind::MutPtr,
                    "ref" => crate::api::RefKind::Ref,
                    "refmut" => crate::api::RefKind::RefMut,
                    _ => crate::api::RefKind::Value,
                };
                (name.clone(), ty.clone(), rk)
            }).collect()
        });
        let enum_variants = addition.enum_variants.clone();
        let (callback_args, callback_returns) = if let Some(ref cb) = addition.callback_typedef {
            let args: Vec<(String, crate::api::RefKind)> = cb.fn_args.iter().map(|(ty, rk)| {
                let ref_kind = match rk.as_str() {
                    "constptr" => crate::api::RefKind::ConstPtr,
                    "mutptr" => crate::api::RefKind::MutPtr,
                    "ref" => crate::api::RefKind::Ref,
                    "refmut" => crate::api::RefKind::RefMut,
                    _ => crate::api::RefKind::Value,
                };
                (ty.clone(), ref_kind)
            }).collect();
            let returns = cb.returns.clone();
            (Some(args), returns)
        } else {
            (None, None)
        };

        merged_types.insert(addition.type_name.clone(), ApiTypeInfo {
            path: addition.full_path.clone(),
            module: String::new(),
            derives: addition.derives.clone(),
            custom_impls: vec![],
            repr: None,
            struct_fields,
            enum_variants,
            callback_args,
            callback_returns,
            type_alias_target: None,
            type_alias_generic_args: vec![],
            generic_params: vec![],
            functions: BTreeMap::new(), // additions don't have functions
            has_constructors: false,
            vec_element_type: None,
        });
    }

    let dead_types = find_dead_type_clusters(&merged_types);

    // Add dead types that are currently in api.json to removals
    for dead_type in &dead_types {
        if !already_removed.contains(dead_type) {
            if let Some(api_info) = current_api_types.get(dead_type) {
                diff.removals.push(format!("{}:{}", dead_type, api_info.path));
            }
        }
    }

    // Filter out modifications and additions for dead types - they'd conflict with removals
    if !dead_types.is_empty() {
        diff.modifications.retain(|m| !dead_types.contains(&m.type_name));
        diff.additions.retain(|a| !dead_types.contains(&a.type_name));
    }

    diff
}

/// Detect dead circular type clusters in api.json.
///
/// A "dead" type is one that has no public functions or constructors AND is not
/// referenced (directly or transitively) by any type that does have functions/constructors.
///
/// Algorithm:
/// 1. Build a reference graph: type A → type B if A's struct fields, enum variants,
///    callback args, type_alias target, or generic_args reference B.
/// 2. Define "root types" = types that have `constructors` (user entry points).
///    Types with only `functions` (no constructors, e.g. auto-generated Vec methods)
///    become roots only if reachable from a constructor root.
/// 3. BFS from root types through function signatures AND structural references.
/// 4. Any api.json type NOT reachable from a root type is dead.
///
/// Primitive types (u8, bool, usize, etc.) and `c_void` are not api.json types
/// and are ignored in the graph.
fn find_dead_type_clusters(
    current_api_types: &BTreeMap<String, ApiTypeInfo>,
) -> BTreeSet<String> {
    use std::collections::VecDeque;

    let all_type_names: BTreeSet<&str> = current_api_types.keys().map(|s| s.as_str()).collect();

    // 1. Build reference graph: for each type, which other api.json types does it reference?
    let mut references: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (type_name, api_info) in current_api_types {
        let mut refs: BTreeSet<&str> = BTreeSet::new();

        // Struct fields
        if let Some(ref fields) = api_info.struct_fields {
            for (_, field_type, _) in fields {
                let clean = strip_ptr_prefix(field_type);
                if all_type_names.contains(clean) {
                    refs.insert(clean);
                }
            }
        }

        // Enum variants
        if let Some(ref variants) = api_info.enum_variants {
            for (_, variant_type) in variants {
                if let Some(vt) = variant_type {
                    let clean = strip_ptr_prefix(vt);
                    if all_type_names.contains(clean) {
                        refs.insert(clean);
                    }
                }
            }
        }

        // Callback typedef args + return
        if let Some(ref cb_args) = api_info.callback_args {
            for (arg_type, _) in cb_args {
                let clean = strip_ptr_prefix(arg_type);
                if all_type_names.contains(clean) {
                    refs.insert(clean);
                }
            }
        }
        if let Some(ref cb_ret) = api_info.callback_returns {
            let clean = strip_ptr_prefix(cb_ret);
            if all_type_names.contains(clean) {
                refs.insert(clean);
            }
        }

        // Type alias target + generic args
        if let Some(ref ta_target) = api_info.type_alias_target {
            if all_type_names.contains(ta_target.as_str()) {
                refs.insert(ta_target.as_str());
            }
        }
        for arg in &api_info.type_alias_generic_args {
            if all_type_names.contains(arg.as_str()) {
                refs.insert(arg.as_str());
            }
        }

        // Function arg types and return types
        for fn_data in api_info.functions.values() {
            for (_, arg_type) in &fn_data.fn_args {
                let clean = strip_ptr_prefix(arg_type);
                if all_type_names.contains(clean) {
                    refs.insert(clean);
                }
            }
            if let Some(ref ret) = fn_data.returns {
                let clean = strip_ptr_prefix(ret);
                if all_type_names.contains(clean) {
                    refs.insert(clean);
                }
            }
        }

        references.insert(type_name.as_str(), refs);
    }

    // 2. Two-phase root detection:
    //    Phase A: Types with constructors are unconditional roots (user can create them directly)
    //    Phase B: Types with only functions (no constructors) are roots ONLY if they are
    //             referenced by a constructor-root's functions (directly or transitively)
    //    This prevents orphaned auto-generated Vec functions from anchoring dead type clusters.

    // Phase A: find constructor roots
    let mut roots: BTreeSet<&str> = BTreeSet::new();
    for (type_name, api_info) in current_api_types {
        if api_info.has_constructors {
            roots.insert(type_name.as_str());
        }
    }

    // Phase B: BFS from constructor roots, collecting types referenced by function signatures.
    // Types with functions that are reachable become secondary roots.
    let mut reachable: BTreeSet<&str> = BTreeSet::new();
    let mut queue: VecDeque<&str> = roots.iter().copied().collect();
    for root in &roots {
        reachable.insert(root);
    }

    while let Some(current) = queue.pop_front() {
        // Follow function arg/return type references (these define the API surface)
        if let Some(api_info) = current_api_types.get(current) {
            for fn_data in api_info.functions.values() {
                for (_, arg_type) in &fn_data.fn_args {
                    let clean = strip_ptr_prefix(arg_type);
                    if all_type_names.contains(clean) && reachable.insert(clean) {
                        queue.push_back(clean);
                    }
                }
                if let Some(ref ret) = fn_data.returns {
                    let clean = strip_ptr_prefix(ret);
                    if all_type_names.contains(clean) && reachable.insert(clean) {
                        queue.push_back(clean);
                    }
                }
            }
        }
        // Also follow structural references (struct fields, enum variants, callbacks, etc.)
        if let Some(refs) = references.get(current) {
            for referenced in refs {
                if reachable.insert(referenced) {
                    queue.push_back(referenced);
                }
            }
        }
    }

    // 4. Types not reachable from roots = dead types
    let mut dead_types = BTreeSet::new();
    for type_name in all_type_names {
        if !reachable.contains(type_name) {
            dead_types.insert(type_name.to_string());
        }
    }

    dead_types
}

/// Strip pointer prefixes like "*const " or "*mut " from a type string
fn strip_ptr_prefix(s: &str) -> &str {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix("*const ") {
        rest.trim()
    } else if let Some(rest) = s.strip_prefix("*mut ") {
        rest.trim()
    } else {
        s
    }
}

/// Compare derives and custom_impls between workspace type and api.json type
fn compare_derives_and_impls(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    let mut modifications = Vec::new();

    // Expand MacroGenerated types to get their derives and custom_impls
    let expanded = workspace_type.expand_macro_generated();

    // Get workspace derives, custom_impls and repr
    let (workspace_derives, workspace_custom_impls, workspace_repr) = match expanded {
        TypeDefKind::Struct {
            derives,
            custom_impls,
            repr,
            ..
        } => (derives, custom_impls, repr),
        TypeDefKind::Enum {
            derives,
            custom_impls,
            repr,
            ..
        } => (derives, custom_impls, repr),
        _ => return modifications, // Skip non-struct/enum types
    };

    // Compare derives (filter out blacklisted derives like strum macros)
    let workspace_derive_set: BTreeSet<_> = workspace_derives
        .iter()
        .filter(|d| !is_derive_blacklisted(d))
        .collect();
    let api_derive_set: BTreeSet<_> = api_info
        .derives
        .iter()
        .filter(|d| !is_derive_blacklisted(d))
        .collect();

    // Derives added in workspace (not in api.json)
    for derive in workspace_derive_set.difference(&api_derive_set) {
        // Double-check blacklist for safety
        if !is_derive_blacklisted(derive) {
            modifications.push(TypeModification {
                type_name: type_name.to_string(),
                kind: ModificationKind::DeriveAdded {
                    derive_name: (*derive).clone(),
                },
            });
        }
    }

    // Derives removed from workspace (in api.json but not workspace)
    for derive in api_derive_set.difference(&workspace_derive_set) {
        // Double-check blacklist for safety
        if !is_derive_blacklisted(derive) {
            modifications.push(TypeModification {
                type_name: type_name.to_string(),
                kind: ModificationKind::DeriveRemoved {
                    derive_name: (*derive).clone(),
                },
            });
        }
    }

    // Compare custom_impls
    let workspace_impl_set: BTreeSet<_> = workspace_custom_impls.iter().collect();
    let api_impl_set: BTreeSet<_> = api_info.custom_impls.iter().collect();

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

    // Compare repr - now using Option<String> for exact value comparison
    if workspace_repr != api_info.repr {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::ReprChanged {
                old_repr: api_info.repr.clone(),
                new_repr: workspace_repr,
            },
        });
    }

    modifications
}

/// Check if the type kind has changed (e.g., struct→enum)
/// If api.json has struct_fields but workspace is an enum, we need to remove struct_fields
/// If api.json has enum_fields but workspace is a struct, we need to remove enum_fields
fn check_type_kind_mismatch(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    let mut modifications = Vec::new();

    let expanded = workspace_type.expand_macro_generated();

    // Determine what the workspace type IS
    let ws_is_struct = matches!(expanded, TypeDefKind::Struct { .. });
    let ws_is_enum = matches!(expanded, TypeDefKind::Enum { .. });

    // Determine what api.json THINKS it is (using enum_variants, not enum_fields)
    let api_has_struct_fields = api_info
        .struct_fields
        .as_ref()
        .map_or(false, |f| !f.is_empty());
    let api_has_enum_variants = api_info
        .enum_variants
        .as_ref()
        .map_or(false, |f| !f.is_empty());

    // If workspace is an enum but api.json has struct_fields → remove struct_fields
    if ws_is_enum && api_has_struct_fields {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::StructFieldsRemoved,
        });
    }

    // If workspace is a struct but api.json has enum_variants → remove enum_fields
    if ws_is_struct && api_has_enum_variants {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::EnumFieldsRemoved,
        });
    }

    modifications
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
    let workspace_fields: Vec<StructFieldInfo> = match expanded {
        TypeDefKind::Struct { fields, .. } => fields
            .iter()
            .map(|(name, field)| StructFieldInfo {
                name: name.clone(),
                ty: field.ty.clone(),
                ref_kind: field.ref_kind,
            })
            .collect(),
        _ => return modifications, // Not a struct
    };

    // Get api.json fields
    let api_fields: Vec<(String, String, crate::api::RefKind)> = match &api_info.struct_fields {
        Some(fields) => fields.clone(),
        None => Vec::new(),
    };

    // Check if fields differ in any way: count, names, types, order, or ref_kind
    let fields_differ = if workspace_fields.len() != api_fields.len() {
        true
    } else {
        // Compare field by field in order
        workspace_fields.iter().zip(api_fields.iter()).any(
            |(ws, (api_name, api_type, api_ref_kind))| {
                let name_differs = ws.name != *api_name;
                // Check if types are equivalent (handles Arc<T>/Box<T>/Rc<T> = *const c_void)
                let types_equivalent = are_types_equivalent(&ws.ty, ws.ref_kind, api_type, *api_ref_kind);
                name_differs || !types_equivalent
            },
        )
    };

    // If any difference exists, replace ALL fields with the correct ones from workspace
    if fields_differ && !workspace_fields.is_empty() {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::StructFieldsReplaced {
                fields: workspace_fields,
            },
        });
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
    let workspace_variants: Vec<EnumVariantInfo> = match expanded {
        TypeDefKind::Enum { variants, .. } => variants
            .iter()
            .map(|(name, variant)| EnumVariantInfo {
                name: name.clone(),
                ty: variant.ty.clone(),
            })
            .collect(),
        _ => return modifications, // Not an enum
    };

    // Get api.json variants
    let api_variants: Vec<(String, Option<String>)> = match &api_info.enum_variants {
        Some(variants) => variants.clone(),
        None => Vec::new(),
    };

    // Check if variants differ in any way: count, names, types, or order
    let variants_differ = if workspace_variants.len() != api_variants.len() {
        true
    } else {
        // Compare variant by variant in order
        workspace_variants
            .iter()
            .zip(api_variants.iter())
            .any(|(ws, (api_name, api_type))| {
                let name_differs = ws.name != *api_name;
                let workspace_normalized = ws.ty.as_ref().map(|t| normalize_type_name(t));
                let api_normalized = api_type.as_ref().map(|t| normalize_type_name(t));
                let type_differs = workspace_normalized != api_normalized;
                name_differs || type_differs
            })
    };

    // If any difference exists, replace ALL variants with the correct ones from workspace
    if variants_differ && !workspace_variants.is_empty() {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::EnumVariantsReplaced {
                variants: workspace_variants,
            },
        });
    }

    modifications
}

/// Compare callback_typedef between workspace type and api.json type
fn compare_callback_typedef(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    let mut modifications = Vec::new();

    // Get workspace callback info (expand MacroGenerated types)
    let expanded = workspace_type.expand_macro_generated();
    let (workspace_args, workspace_returns): (Vec<CallbackArgInfo>, Option<String>) = match expanded
    {
        TypeDefKind::CallbackTypedef { args, returns } => {
            let arg_vec: Vec<CallbackArgInfo> = args
                .iter()
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

    // Get api.json callback info - now uses RefKind directly, no string conversion
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
            // Compare args - check both type and ref_kind (both are now typed, no strings)
            let mut any_arg_differs = false;

            // Check argument count
            if workspace_args.len() != api_args.len() {
                any_arg_differs = true;
            } else {
                for (i, workspace_arg) in workspace_args.iter().enumerate() {
                    if let Some((api_arg_ty, api_arg_ref_kind)) = api_args.get(i) {
                        let workspace_normalized = normalize_type_name(&workspace_arg.ty);
                        let api_normalized = normalize_type_name(api_arg_ty);

                        // Direct RefKind comparison - no string conversion needed
                        let type_differs = workspace_normalized != api_normalized;
                        let ref_differs = workspace_arg.ref_kind != *api_arg_ref_kind;

                        if type_differs || ref_differs {
                            any_arg_differs = true;
                            break;
                        }
                    }
                }
            }

            // Compare return type
            let workspace_ret_normalized =
                workspace_returns.as_ref().map(|t| normalize_type_name(t));
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
    let (workspace_target, workspace_generic_args): (String, Vec<String>) =
        match &workspace_type.kind {
            TypeDefKind::TypeAlias {
                target,
                generic_base,
                generic_args,
            } => {
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

/// Check if a workspace type and an API type are equivalent
/// This handles cases like Arc<T>/Box<T>/Rc<T> in workspace = c_void with constptr in API
fn are_types_equivalent(
    ws_type: &str,
    ws_ref_kind: crate::api::RefKind,
    api_type: &str,
    api_ref_kind: crate::api::RefKind,
) -> bool {
    // Check if workspace type is a smart pointer that maps to *const c_void
    let ws_is_smart_ptr = ws_type.starts_with("Arc<")
        || ws_type.starts_with("Box<")
        || ws_type.starts_with("Rc<");
    
    // If workspace has Arc<T>/Box<T>/Rc<T> and API has c_void with constptr, they're equivalent
    if ws_is_smart_ptr {
        let api_is_cvoid_ptr = normalize_type_name(api_type) == "c_void"
            && api_ref_kind == crate::api::RefKind::ConstPtr;
        if api_is_cvoid_ptr {
            return true;
        }
    }
    
    // Otherwise, compare normally: types must match and ref_kinds must match
    let types_match = normalize_type_name(ws_type) == normalize_type_name(api_type);
    let refs_match = ws_ref_kind == api_ref_kind;
    types_match && refs_match
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
            super::type_index::TypeDefKind::MacroGenerated { kind, .. } => match kind {
                super::type_index::MacroGeneratedKind::Vec => "vec",
                super::type_index::MacroGeneratedKind::VecDestructor => "vec_destructor",
                super::type_index::MacroGeneratedKind::VecDestructorType => "callback_typedef",
                super::type_index::MacroGeneratedKind::VecSlice => "vec_slice",
                super::type_index::MacroGeneratedKind::Option => "option",
                super::type_index::MacroGeneratedKind::OptionEnumWrapper => "option_wrapper",
                super::type_index::MacroGeneratedKind::Result => "result",
                super::type_index::MacroGeneratedKind::CallbackWrapper => "callback_wrapper",
                super::type_index::MacroGeneratedKind::CallbackValue => "callback_value",
            },
        }
        .to_string()
    } else {
        "unknown".to_string()
    }
}

/// Get the kind, fields, variants, and derives of a type from the index
/// This expands MacroGenerated types to get their actual fields
fn get_type_kind_with_fields(
    index: &TypeIndex,
    type_name: &str,
) -> (
    String,
    Option<Vec<(String, String, String)>>,
    Option<Vec<(String, Option<String>)>>,
    Vec<String>,
    Option<CallbackTypedefInfo>,
) {
    if let Some(typedef) = index.resolve(type_name, None) {
        // Get the expanded kind (handles MacroGenerated types)
        let expanded = typedef.expand_macro_generated();

        let kind_str = match &typedef.kind {
            super::type_index::TypeDefKind::Struct { .. } => "struct",
            super::type_index::TypeDefKind::Enum { .. } => "enum",
            super::type_index::TypeDefKind::TypeAlias { .. } => "type_alias",
            super::type_index::TypeDefKind::CallbackTypedef { .. } => "callback_typedef",
            super::type_index::TypeDefKind::MacroGenerated { kind, .. } => match kind {
                super::type_index::MacroGeneratedKind::Vec => "struct",
                super::type_index::MacroGeneratedKind::VecDestructor => "enum",
                super::type_index::MacroGeneratedKind::VecDestructorType => "callback_typedef",
                super::type_index::MacroGeneratedKind::VecSlice => "struct",
                super::type_index::MacroGeneratedKind::Option => "enum",
                super::type_index::MacroGeneratedKind::OptionEnumWrapper => "struct",
                super::type_index::MacroGeneratedKind::Result => "enum",
                super::type_index::MacroGeneratedKind::CallbackWrapper => "struct",
                super::type_index::MacroGeneratedKind::CallbackValue => "struct",
            },
        };

        // Extract fields/variants from the EXPANDED kind
        match expanded {
            super::type_index::TypeDefKind::Struct {
                fields, derives, ..
            } => {
                let field_vec: Vec<(String, String, String)> = fields
                    .iter()
                    .map(|(name, field)| {
                        (
                            name.clone(),
                            field.ty.clone(),
                            field.ref_kind.as_str().to_string(),
                        )
                    })
                    .collect();
                (kind_str.to_string(), Some(field_vec), None, derives, None)
            }
            super::type_index::TypeDefKind::Enum {
                variants, derives, ..
            } => {
                let variant_vec: Vec<(String, Option<String>)> = variants
                    .iter()
                    .map(|(name, variant)| (name.clone(), variant.ty.clone()))
                    .collect();
                (kind_str.to_string(), None, Some(variant_vec), derives, None)
            }
            super::type_index::TypeDefKind::CallbackTypedef { args, returns, .. } => {
                let callback_info = CallbackTypedefInfo {
                    fn_args: args
                        .iter()
                        .map(|arg| (arg.ty.clone(), arg.ref_kind.as_str().to_string()))
                        .collect(),
                    returns: returns.clone(),
                };
                (
                    kind_str.to_string(),
                    None,
                    None,
                    vec![],
                    Some(callback_info),
                )
            }
            _ => (kind_str.to_string(), None, None, vec![], None),
        }
    } else {
        ("unknown".to_string(), None, None, vec![], None)
    }
}

// unit tests
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
        // Paths must be exactly the same
        assert!(paths_are_equivalent(
            "azul_core::dom::Dom",
            "azul_core::dom::Dom"
        ));
        // Different crates are NOT equivalent - we want to catch crate renames
        assert!(!paths_are_equivalent(
            "azul_core::resources::FontCache",
            "azul_css::resources::FontCache"
        ));
        assert!(!paths_are_equivalent(
            "azul_core::dom::Dom",
            "azul_core::window::Dom"
        ));
    }

    #[test]
    fn test_deduplication() {
        let mut seen: BTreeSet<String> = BTreeSet::new();

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

/// Compare functions between workspace type and api.json class
/// Checks for self parameter mismatches and argument count differences
fn compare_functions(
    type_name: &str,
    workspace_type: &TypeDefinition,
    api_info: &ApiTypeInfo,
) -> Vec<TypeModification> {
    use super::type_index::SelfKind;

    let mut modifications = Vec::new();

    // Get workspace methods (only public ones)
    let workspace_methods: std::collections::BTreeMap<String, &super::type_index::MethodDef> =
        workspace_type
            .methods
            .iter()
            .filter(|m| m.is_public)
            .map(|m| (m.name.clone(), m))
            .collect();

    // Check each api.json function
    for (fn_name, fn_info) in api_info.functions.iter() {
        // Find matching workspace method
        let Some(workspace_method) = workspace_methods.get(fn_name) else {
            continue; // Function only in api.json, not our concern here
        };

        // Check self parameter
        let api_has_self = fn_info
            .fn_args
            .iter()
            .any(|(arg_name, _)| arg_name == "self");
        let api_self_kind = fn_info
            .fn_args
            .iter()
            .find(|(arg_name, _)| arg_name == "self")
            .map(|(_, ref_kind)| ref_kind.as_str());

        let workspace_self_kind = workspace_method.self_kind.as_ref().map(|sk| match sk {
            SelfKind::Value => "value",
            SelfKind::Ref => "ref",
            SelfKind::RefMut => "refmut",
        });

        let workspace_has_self = workspace_self_kind.is_some();

        // Check for mismatch
        if workspace_has_self != api_has_self
            || (workspace_has_self && api_has_self && workspace_self_kind != api_self_kind)
        {
            modifications.push(TypeModification {
                type_name: type_name.to_string(),
                kind: ModificationKind::FunctionSelfMismatch {
                    fn_name: fn_name.clone(),
                    expected_self: workspace_self_kind.map(|s| s.to_string()),
                    actual_self: api_self_kind.map(|s| s.to_string()),
                },
            });
        }

        // Check argument count (excluding self)
        let api_arg_count = fn_info
            .fn_args
            .iter()
            .filter(|(arg_name, _)| arg_name != "self")
            .count();
        let workspace_arg_count = workspace_method.args.len();

        if api_arg_count != workspace_arg_count {
            modifications.push(TypeModification {
                type_name: type_name.to_string(),
                kind: ModificationKind::FunctionArgCountMismatch {
                    fn_name: fn_name.clone(),
                    expected_count: workspace_arg_count,
                    actual_count: api_arg_count,
                },
            });
        }
    }

    modifications
}
/// Standard functions that should be present on all Vec types (generated by impl_vec! macro)
/// These functions are always generated by the macro and should be exposed in the C API
/// NOTE: as_ptr was removed - use as_c_slice instead for safer C API
const VEC_STANDARD_FUNCTIONS: &[&str] = &[
    "create",           // Creates an empty Vec
    "with_capacity",    // Creates a Vec with given capacity
    "len",              // Returns the length
    "capacity",         // Returns the capacity
    "is_empty",         // Returns whether the Vec is empty
    "c_get",            // Gets an element by index (returns OptionT for C-API safety)
    "from_item",        // Creates a Vec from a single element (requires Clone)
    "copy_from_ptr",    // Creates a Vec from a C pointer + length (requires Clone)
    "as_c_slice",       // Returns a C-compatible slice (ptr + len struct)
    "as_c_slice_range", // Returns a C-compatible slice of a range
];

/// Build a mapping from element type to Option type name by scanning api.json for Option types.
/// This uses the impl_option! macro pattern: Option types have Some(ElementType) variant.
/// For example: OptionF32 has Some(f32) -> maps "f32" to "OptionF32"
fn build_element_to_option_map(
    current_api_types: &BTreeMap<String, ApiTypeInfo>,
) -> BTreeMap<String, String> {
    let mut element_to_option: BTreeMap<String, String> = BTreeMap::new();
    
    for (type_name, api_info) in current_api_types {
        // Only consider Option types (start with "Option" and have enum variants)
        if !type_name.starts_with("Option") {
            continue;
        }
        
        if let Some(ref variants) = api_info.enum_variants {
            // Look for the Some variant and extract its inner type
            for (variant_name, variant_type) in variants {
                if variant_name == "Some" {
                    if let Some(inner_type) = variant_type {
                        // Map: inner_type -> OptionTypeName
                        // e.g., "f32" -> "OptionF32", "StringPair" -> "OptionStringPair"
                        element_to_option.insert(inner_type.clone(), type_name.clone());
                    }
                }
            }
        }
    }
    
    element_to_option
}

/// Collect all types that are required by Vec types for their macro-generated functions.
/// 
/// Vec types (generated by impl_vec!) have functions like:
/// - `c_get(&self, index) -> OptionElementType` - needs the Option wrapper type
/// - `as_c_slice(&self) -> VecTypeSlice` - needs the Slice type
/// - `as_c_slice_range(&self, start, end) -> VecTypeSlice` - needs the Slice type
///
/// These types are macro-generated and may not be directly reachable via normal function
/// signature analysis, but they ARE required for the API to be complete.
///
/// This function returns a set of type names that should NOT be removed from api.json
/// even if they're not directly reachable.
fn collect_vec_required_types(
    current_api_types: &BTreeMap<String, ApiTypeInfo>,
    element_to_option_map: &BTreeMap<String, String>,
    reachable_types: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut required_types = BTreeSet::new();
    
    for (type_name, api_info) in current_api_types {
        // Only protect sub-types of Vec types that are themselves reachable.
        // This prevents dead circular type clusters from being kept alive:
        // e.g. if XmlComponentVec is not reachable from any function signature,
        // its element type XmlComponent, OptionXmlComponent, XmlComponentVecSlice
        // etc. should also be eligible for removal.
        if !reachable_types.contains(type_name) {
            continue;
        }
        // Skip Option wrapper types that happen to wrap Vec types
        if type_name.starts_with("Option") && type_name.ends_with("Vec") {
            continue;
        }
        
        // Skip VecRef and VecRefMut types
        if type_name.ends_with("VecRef") || type_name.ends_with("VecRefMut") {
            continue;
        }
        
        // Check if this is a Vec type
        let is_vec_type = if api_info.vec_element_type.is_some() {
            true
        } else if type_name.ends_with("Vec") {
            // Check if it has the standard Vec fields: ptr, len, cap, destructor
            if let Some(ref fields) = api_info.struct_fields {
                let field_names: BTreeSet<&str> = fields.iter()
                    .map(|(name, _, _)| name.as_str())
                    .collect();
                field_names.contains("ptr") && 
                field_names.contains("len") && 
                field_names.contains("cap") && 
                field_names.contains("destructor")
            } else {
                false
            }
        } else {
            false
        };
        
        if !is_vec_type {
            continue;
        }
        
        // Determine element type
        let element_type = if let Some(ref elem_type) = api_info.vec_element_type {
            elem_type.clone()
        } else if let Some(ref fields) = api_info.struct_fields {
            // Look for the 'ptr' field and get its type
            fields.iter()
                .find(|(name, _, _)| name == "ptr")
                .map(|(_, type_str, _)| type_str.clone())
                .unwrap_or_else(|| infer_element_type_from_name(type_name))
        } else {
            infer_element_type_from_name(type_name)
        };
        
        // Get the Option type for c_get function
        let option_type_name = element_to_option_map
            .get(&element_type)
            .cloned()
            .unwrap_or_else(|| canonicalize_option_type_name(&element_type));
        
        // Get the Slice type for as_c_slice / as_c_slice_range functions
        let slice_type_name = format!("{}Slice", type_name);
        
        // Add these as required types (should not be removed)
        required_types.insert(option_type_name);
        required_types.insert(slice_type_name);
        
        // Also add the element type itself and its destructor
        required_types.insert(element_type.clone());
        required_types.insert(format!("{}VecDestructor", element_type));
    }
    
    required_types
}

/// Check if a Vec type is missing any standard functions
/// Vec types are identified by:
/// 1. Having vec_element_type set in api.json, OR
/// 2. Having a type name ending with "Vec" AND having the standard Vec fields (ptr, len, cap, destructor)
///
/// Also checks if required dependency types (OptionX, XVecSlice) exist in api.json.
/// Uses element_to_option_map to find the correct Option type for the element type
/// (e.g., f32 -> OptionF32, not Optionf32).
fn check_vec_functions(
    type_name: &str,
    api_info: &ApiTypeInfo,
    current_api_types: &BTreeMap<String, ApiTypeInfo>,
    element_to_option_map: &BTreeMap<String, String>,
) -> Vec<TypeModification> {
    let mut modifications = Vec::new();

    // Skip Option wrapper types that happen to wrap Vec types
    // e.g., "OptionStringVec" is Option<StringVec>, not a Vec type itself
    if type_name.starts_with("Option") && type_name.ends_with("Vec") {
        return modifications;
    }

    // Skip VecRef and VecRefMut types
    if type_name.ends_with("VecRef") || type_name.ends_with("VecRefMut") {
        return modifications;
    }

    // Check if this is a valid Vec type
    let is_vec_type = if api_info.vec_element_type.is_some() {
        true
    } else if type_name.ends_with("Vec") {
        // Check if it has the standard Vec fields: ptr, len, cap, destructor
        if let Some(ref fields) = api_info.struct_fields {
            let field_names: std::collections::BTreeSet<&str> = fields.iter()
                .map(|(name, _, _)| name.as_str())
                .collect();
            field_names.contains("ptr") && 
            field_names.contains("len") && 
            field_names.contains("cap") && 
            field_names.contains("destructor")
        } else {
            false
        }
    } else {
        false
    };

    if !is_vec_type {
        return modifications;
    }

    // Determine element type from either:
    // 1. Explicit vec_element_type field (preferred)
    // 2. The type of the 'ptr' field in struct_fields (most accurate)
    // 3. Inferred from type name ending with "Vec" (fallback)
    let element_type = if let Some(ref elem_type) = api_info.vec_element_type {
        elem_type.clone()
    } else if let Some(ref fields) = api_info.struct_fields {
        // Look for the 'ptr' field and get its type
        let ptr_type = fields.iter()
            .find(|(name, _, _)| name == "ptr")
            .map(|(_, type_str, _)| type_str.clone());
        
        if let Some(pt) = ptr_type {
            pt
        } else {
            infer_element_type_from_name(type_name)
        }
    } else {
        infer_element_type_from_name(type_name)
    };

    // Look up the Option type from the map (built from impl_option! macros)
    // If not found, use canonicalize_option_type_name for primitives and type aliases
    // This correctly handles cases like f32 -> OptionF32, GLint -> OptionI32, etc.
    let option_type_name = element_to_option_map
        .get(&element_type)
        .cloned()
        .unwrap_or_else(|| canonicalize_option_type_name(&element_type));
    
    let slice_type_name = format!("{}Slice", type_name);
    
    // Check if OptionX exists in api.json (needed for c_get)
    let option_type_exists = current_api_types.contains_key(&option_type_name);
    
    // Check if XVecSlice exists in api.json (needed for as_c_slice, as_c_slice_range)
    let slice_type_exists = current_api_types.contains_key(&slice_type_name);

    // Generate modifications for missing dependency types
    if !option_type_exists {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::VecMissingOptionType {
                vec_type: type_name.to_string(),
                element_type: element_type.clone(),
                option_type_name: option_type_name.clone(),
            },
        });
    }
    
    if !slice_type_exists {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::VecMissingSliceType {
                vec_type: type_name.to_string(),
                element_type: element_type.clone(),
                slice_type_name: slice_type_name.clone(),
            },
        });
    }

    // Find missing functions
    let existing_functions: std::collections::BTreeSet<&str> = api_info
        .functions
        .keys()
        .map(|s| s.as_str())
        .collect();

    // Filter out functions that require missing dependency types
    let missing: Vec<String> = VEC_STANDARD_FUNCTIONS
        .iter()
        .filter(|&&fn_name| !existing_functions.contains(fn_name))
        .filter(|&&fn_name| {
            // Skip c_get if OptionX doesn't exist yet
            if fn_name == "c_get" && !option_type_exists {
                return false;
            }
            // Skip as_c_slice and as_c_slice_range if Slice type doesn't exist yet
            if (fn_name == "as_c_slice" || fn_name == "as_c_slice_range") && !slice_type_exists {
                return false;
            }
            true
        })
        .map(|s| s.to_string())
        .collect();

    if !missing.is_empty() {
        modifications.push(TypeModification {
            type_name: type_name.to_string(),
            kind: ModificationKind::VecFunctionsMissing {
                missing_functions: missing,
                element_type: element_type.clone(),
            },
        });
    }

    modifications
}

/// Infer Vec element type from type name (fallback when struct_fields not available)
fn infer_element_type_from_name(type_name: &str) -> String {
    let base = &type_name[..type_name.len() - 3]; // Remove "Vec" suffix
    // Handle special cases like "U8Vec" -> "u8", "U16Vec" -> "u16", etc.
    match base {
        "U8" => "u8".to_string(),
        "U16" => "u16".to_string(),
        "U32" => "u32".to_string(),
        "U64" => "u64".to_string(),
        "I8" => "i8".to_string(),
        "I16" => "i16".to_string(),
        "I32" => "i32".to_string(),
        "I64" => "i64".to_string(),
        "F32" => "f32".to_string(),
        "F64" => "f64".to_string(),
        "GLuint" => "GLuint".to_string(),
        "GLint" => "GLint".to_string(),
        _ => base.to_string(),
    }
}