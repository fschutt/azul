/// Automatic API fixing with recursive type discovery
///
/// This module analyzes the API and automatically:
/// 1. Finds all referenced types in the workspace
/// 2. Recursively discovers dependencies
/// 3. Generates patches for missing/incorrect types
/// 4. Provides a clean summary of changes
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::Path,
};

use anyhow::Result;

use crate::{
    api::ApiData,
    autofix::{
        message::{AutofixMessage, AutofixMessages, ClassAdded, PatchSummary, SkipReason},
        module_map::{get_correct_module, MODULES},
        unified_index::TypeLookup,
    },
    patch::{
        index::{ParsedTypeInfo, TypeKind, WorkspaceIndex},
        ApiPatch, ClassPatch, ModulePatch, VersionPatch,
    },
};

/// Tracks where a type was discovered from
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TypeOrigin {
    /// Type was directly referenced in the API
    ApiReference,
    /// Type was found in a struct field
    StructField {
        parent_type: String,
        field_name: String,
    },
    /// Type was found in an enum variant
    EnumVariant {
        parent_type: String,
        variant_name: String,
    },
    /// Type was found in a type alias
    TypeAlias { parent_type: String },
    /// Type was found as a generic argument (e.g., T in CssPropertyValue<T>)
    GenericArgument { parent_type: String },
    /// Type was found in a callback typedef parameter
    CallbackParameter { parent_type: String },
    /// Type was found as a callback argument
    CallbackArg { parent_type: String },
    /// Type was found as a callback return type
    CallbackReturn { parent_type: String },
}

impl fmt::Display for TypeOrigin {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ApiReference => write!(f, "Referenced in API"),
            Self::StructField {
                parent_type,
                field_name,
            } => {
                write!(f, "Field '{}' in struct '{}'", field_name, parent_type)
            }
            Self::EnumVariant {
                parent_type,
                variant_name,
            } => {
                write!(f, "Variant '{}' in enum '{}'", variant_name, parent_type)
            }
            Self::TypeAlias { parent_type } => {
                write!(f, "Type alias in '{}'", parent_type)
            }
            Self::GenericArgument { parent_type } => {
                write!(f, "Generic argument in type '{}'", parent_type)
            }
            Self::CallbackParameter { parent_type } => {
                write!(f, "Callback parameter in '{}'", parent_type)
            }
            Self::CallbackArg { parent_type } => {
                write!(f, "Callback argument in '{}'", parent_type)
            }
            Self::CallbackReturn { parent_type } => {
                write!(f, "Callback return type in '{}'", parent_type)
            }
        }
    }
}

/// Normalize a type name by removing the "Az" prefix if present.
/// This is needed because some types in the source code already have the "Az" prefix
/// (e.g., `AzDuration` in azul_dll), but when stored in api.json they should not have it
/// since the code generator will add the prefix when generating FFI code.
fn normalize_type_name_for_api(type_name: &str) -> String {
    crate::autofix::utils::normalize_az_prefix(type_name)
}

/// Collect all types currently in the API (including callback_typedefs)
/// Returns a Vec of (class_name, module_name, type_path) to handle duplicate class names
pub fn collect_all_api_types(api_data: &ApiData) -> Vec<(String, String, String)> {
    let mut types = Vec::new();

    for (_version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Include callback_typedefs - they need patches for FFI
                // (e.g. FooDestructorType is callback_typedef but needs patch)

                let type_path = class_data
                    .external
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or(class_name.as_str())
                    .to_string();

                types.push((class_name.clone(), module_name.clone(), type_path));
            }
        }
    }

    types
}

/// Check if two paths are synonyms (refer to the same item)
/// For example: "crate::foo::Bar" and "azul_dll::foo::Bar" are synonyms
pub fn are_paths_synonyms(path1: &str, path2: &str) -> bool {
    // If paths are identical, they're obviously synonyms
    if path1 == path2 {
        return true;
    }

    // Define synonym groups - paths with these prefixes refer to the same crate
    const SYNONYM_GROUPS: &[&[&str]] = &[
        // azul_dll and crate both refer to the same crate (the DLL crate)
        &["azul_dll::", "crate::"],
    ];

    // Helper function to find prefix and suffix
    fn normalize(path: &str) -> Option<(&'static str, String)> {
        const SYNONYM_GROUPS: &[&[&str]] = &[&["azul_dll::", "crate::"]];

        for group in SYNONYM_GROUPS {
            for &prefix in *group {
                if let Some(suffix) = path.strip_prefix(prefix) {
                    return Some((prefix, suffix.to_string()));
                }
            }
        }
        None
    }

    // Try to normalize both paths
    if let (Some((prefix1, suffix1)), Some((prefix2, suffix2))) =
        (normalize(path1), normalize(path2))
    {
        // If suffixes match and prefixes are in the same synonym group
        if suffix1 == suffix2 {
            for group in SYNONYM_GROUPS {
                if group.contains(&prefix1) && group.contains(&prefix2) {
                    return true;
                }
            }
        }
    }

    false
}

/// Check if a type belongs to the azul workspace (not an external crate)
pub fn is_workspace_type(full_path: &str) -> bool {
    const WORKSPACE_CRATES: &[&str] = &[
        "azul_core::",
        "azul_css::",
        "azul_layout::",
        "azul_dll::",
        "azul_test::",
        "azul::",
        "crate::", // For types in the same crate
    ];

    // Exclude self crate (build tools)
    // Note: CARGO_PKG_NAME uses hyphens, but Rust paths use underscores
    let self_crate_name = env!("CARGO_PKG_NAME").replace('-', "_");
    let self_crate_prefix = format!("{}::", self_crate_name);
    if full_path.starts_with(&self_crate_prefix) {
        return false;
    }

    // Check if the path starts with any workspace crate
    WORKSPACE_CRATES
        .iter()
        .any(|prefix| full_path.starts_with(prefix))
}

/// Discover a type in the workspace
pub fn discover_type<T: TypeLookup>(
    workspace_index: &T,
    type_name: &str,
    messages: &mut AutofixMessages,
) -> Option<ParsedTypeInfo> {
    // Try exact match first
    if let Some(candidates) = workspace_index.find_type(type_name) {
        if candidates.len() == 1 {
            messages.push(AutofixMessage::TypeDiscovered {
                type_name: type_name.to_string(),
                path: candidates[0].module_path.join("::"),
                reason: TypeOrigin::ApiReference,
            });
            return Some(candidates[0].clone());
        } else if !candidates.is_empty() {
            // Multiple matches found, automatically using the first one
            // (no warning needed - this is expected behavior)
            messages.push(AutofixMessage::TypeDiscovered {
                type_name: type_name.to_string(),
                path: candidates[0].module_path.join("::"),
                reason: TypeOrigin::ApiReference,
            });
            return Some(candidates[0].clone());
        }
    }

    // Try string search (for macro-defined types)
    if let Some(type_info) = workspace_index.find_type_by_string_search(type_name) {
        messages.push(AutofixMessage::TypeDiscovered {
            type_name: type_name.to_string(),
            path: type_info.module_path.join("::"),
            reason: TypeOrigin::ApiReference,
        });
        return Some(type_info);
    }

    None
}

/// Find a type in workspace and verify it matches the expected path
pub fn find_type_in_workspace<T: TypeLookup>(
    workspace_index: &T,
    class_name: &str,
    expected_path: &str,
    messages: &mut AutofixMessages,
) -> Option<ParsedTypeInfo> {
    // Extract simple name from path
    let simple_name = expected_path.split("::").last().unwrap_or(class_name);

    // Try to find by full path first
    if let Some(type_info) = workspace_index.find_type_by_path(expected_path) {
        return Some(type_info);
    }

    // Try to find by simple name
    if let Some(candidates) = workspace_index.find_type(simple_name) {
        if candidates.len() == 1 {
            return Some(candidates[0].clone());
        }

        // Multiple matches, try to find the one matching the expected path
        for candidate in &candidates {
            if candidate.full_path == expected_path {
                return Some(candidate.clone());
            }
        }

        // No exact match, use first one
        // (no warning needed - this is expected behavior)
        return Some(candidates[0].clone());
    }

    // Try string search as fallback
    workspace_index.find_type_by_string_search(simple_name)
}

/// Collect all types referenced by a type (from its fields/variants)
/// Skips types behind pointers - they don't need to be exposed in the API
/// Returns a map of type name -> origin (where it was found)
pub fn collect_referenced_types_from_type_info(
    type_info: &ParsedTypeInfo,
) -> BTreeMap<String, TypeOrigin> {
    use crate::api::extract_base_type_if_not_opaque;

    let mut types = BTreeMap::new();
    let parent_type = type_info.type_name.clone();

    match &type_info.kind {
        TypeKind::Struct { fields, .. } => {
            for (field_name, field_info) in fields {
                // Skip types behind pointers - they're opaque
                if let Some(base_type) = extract_base_type_if_not_opaque(&field_info.ty) {
                    types.insert(
                        base_type.clone(),
                        TypeOrigin::StructField {
                            parent_type: parent_type.clone(),
                            field_name: field_name.clone(),
                        },
                    );
                }
            }
        }
        TypeKind::Enum { variants, .. } => {
            for (variant_name, variant_info) in variants {
                if let Some(variant_type) = &variant_info.ty {
                    if let Some(base_type) = extract_base_type_if_not_opaque(variant_type) {
                        types.insert(
                            base_type.clone(),
                            TypeOrigin::EnumVariant {
                                parent_type: parent_type.clone(),
                                variant_name: variant_name.clone(),
                            },
                        );
                    }
                }
            }
        }
        TypeKind::TypeAlias {
            target,
            generic_args,
            ..
        } => {
            // Add the target type (e.g., CssPropertyValue)
            if let Some(base_type) = extract_base_type_if_not_opaque(target) {
                types.insert(
                    base_type.clone(),
                    TypeOrigin::TypeAlias {
                        parent_type: parent_type.clone(),
                    },
                );
            }

            // Add all generic arguments (e.g., LayoutZIndex from CssPropertyValue<LayoutZIndex>)
            for generic_arg in generic_args {
                if let Some(base_type) = extract_base_type_if_not_opaque(generic_arg) {
                    types.insert(
                        base_type.clone(),
                        TypeOrigin::GenericArgument {
                            parent_type: parent_type.clone(),
                        },
                    );
                }
            }
        }
        TypeKind::CallbackTypedef {
            fn_args, returns, ..
        } => {
            // Add all argument types
            for arg in fn_args {
                if let Some(base_type) = extract_base_type_if_not_opaque(&arg.ty) {
                    types.insert(
                        base_type.clone(),
                        TypeOrigin::CallbackArg {
                            parent_type: parent_type.clone(),
                        },
                    );
                }
            }
            // Add return type if present
            if let Some(ret) = returns {
                if let Some(base_type) = extract_base_type_if_not_opaque(ret) {
                    types.insert(
                        base_type.clone(),
                        TypeOrigin::CallbackReturn {
                            parent_type: parent_type.clone(),
                        },
                    );
                }
            }
        }
    }

    types
}

/// Infer module name from type path using smart routing rules
///
/// Rules:
/// - Types from azul_css::* -> "css"
/// - Types ending with "Vec" -> "vec" (with auto-generated destructors)
/// - Types ending with "Option" -> "option"
/// - Types ending with "Result" -> "error"
/// - Other types -> inferred from file name in path
///
/// Examples:
/// - "azul_css::LayoutZIndex" -> "css"
/// - "azul_core::callbacks::CoreCallbackDataVec" -> "vec"
/// - "azul_core::dom::DomOption" -> "option"
/// - "azul_core::errors::ErrorResult" -> "error"
/// - "azul_core::foo::bar::BarType" -> "bar" (from file name)
pub fn infer_module_from_path(type_path: &str) -> String {
    // Extract type name from path
    let type_name = type_path.split("::").last().unwrap_or("");

    // Rule 1: All azul_css types go to "css" module
    if type_path.starts_with("azul_css::")
        || type_path.starts_with("crate::") && type_path.contains("css")
    {
        return "css".to_string();
    }

    // Rule 2: Vec types go to "vec" module
    if is_vec_type(type_name)
        || is_vec_destructor_type(type_name)
        || is_vec_destructor_callback_type(type_name)
    {
        return "vec".to_string();
    }

    // Rule 3: Option types go to "option" module
    if is_option_type(type_name) {
        return "option".to_string();
    }

    // Rule 4: Result types go to "error" module
    if is_result_type(type_name) {
        return "error".to_string();
    }

    // Rule 5: For regular types, use the file name (last segment before type name)
    // Example: "azul_core::foo::bar::BarType" -> "bar"
    let parts: Vec<&str> = type_path.split("::").collect();

    if parts.len() >= 3 {
        // If path is "crate::module::Type", use "module"
        // If path is "azul_core::module::Type", use "module"
        // If path is "azul_core::foo::bar::Type", use "bar" (file name)
        parts[parts.len() - 2].to_string()
    } else if parts.len() == 2 {
        // If path is "azul_core::Type", use the crate as module
        parts[0].to_string()
    } else {
        "unknown".to_string()
    }
}

/// Generate patches for discovered types
pub fn generate_patches<T: TypeLookup>(
    api_data: &ApiData,
    workspace_index: &T,
    types_to_add: &[ParsedTypeInfo],
    patch_summary: &PatchSummary,
    patches_dir: &Path,
    messages: &mut AutofixMessages,
) -> Result<usize> {
    use std::collections::BTreeMap;

    // NOTE: Don't return early here - we need to iterate over ALL API types
    // to check for missing struct_fields/enum_fields even if no new types to add

    // Get the first version from API (usually "1.0.0")
    let version_name = api_data
        .0
        .keys()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No version found in API data"))?
        .clone();

    let mut patch_count = 0;

    // Create a map of all patches: (module, class_name) -> ClassPatch
    let mut all_patches: BTreeMap<(String, String), ClassPatch> = BTreeMap::new();

    // 1. Add patches for new types
    for type_info in types_to_add {
        let module = infer_module_from_path(&type_info.full_path);
        // Normalize the type name - remove "Az" prefix if present
        // The code generator adds "Az" prefix, so it should never be in api.json
        let class_name = if crate::autofix::utils::should_normalize_az_prefix(&type_info.type_name)
        {
            normalize_type_name_for_api(&type_info.type_name)
        } else {
            type_info.type_name.clone()
        };

        let class_patch = convert_type_info_to_class_patch(type_info);

        // Extract derives from the type_info for passing to synthetic type generators
        let source_derives = match &type_info.kind {
            crate::patch::index::TypeKind::Struct { derives, .. } => Some(derives),
            crate::patch::index::TypeKind::Enum { derives, .. } => Some(derives),
            _ => None,
        };

        all_patches.insert((module.clone(), class_name.clone()), class_patch);

        // If this is a Vec type, generate the synthetic Destructor and DestructorType
        if is_vec_type(&class_name) {
            generate_synthetic_vec_types(
                &class_name,
                &type_info.full_path,
                source_derives,
                &mut all_patches,
            );
        }

        // If this is a VecDestructor enum, ensure it has proper enum_fields
        if is_vec_destructor_type(&class_name) {
            ensure_vec_destructor_enum_fields(&class_name, &type_info.full_path, &mut all_patches);
        }

        // If this is a VecDestructorType callback, ensure it has proper callback_typedef
        if is_vec_destructor_callback_type(&class_name) {
            ensure_vec_destructor_callback_typedef(&class_name, &mut all_patches);
        }

        // If this is an Option type, generate the synthetic Option enum in the "option" module
        if is_option_type(&class_name) {
            // Get derives from the parsed type info (from impl_option! macro)
            let derives = match &type_info.kind {
                crate::patch::index::TypeKind::Enum { derives, .. } => derives.clone(),
                _ => Vec::new(),
            };
            generate_synthetic_option_type(
                &class_name,
                &type_info.full_path,
                &derives,
                &mut all_patches,
            );
        }
    }

    // 2. Add patches for external path changes
    for change in &patch_summary.external_path_changes {
        // Use the module_name from the change (the module where the type is defined in api.json)
        let key = (change.module_name.clone(), change.class_name.clone());

        // Get struct/enum info from workspace if available
        if let Some(type_info) = workspace_index.find_type_by_path(&change.new_path) {
            // Merge with existing patch or create new one with full info
            let full_patch = convert_type_info_to_class_patch(&type_info);
            all_patches
                .entry(key.clone())
                .and_modify(|p| {
                    // Keep existing external if set, otherwise use new one
                    if p.external.is_none() {
                        p.external = full_patch.external.clone();
                    }
                    // Always update struct/enum fields from workspace
                    if full_patch.struct_fields.is_some() {
                        p.struct_fields = full_patch.struct_fields.clone();
                    }
                    if full_patch.enum_fields.is_some() {
                        p.enum_fields = full_patch.enum_fields.clone();
                    }
                    if full_patch.doc.is_some() {
                        p.doc = full_patch.doc.clone();
                    }
                })
                .or_insert(full_patch);

            // If this is a Vec type, generate the synthetic Destructor and DestructorType
            if is_vec_type(&change.class_name) {
                // Extract derives from type_info
                let source_derives = match &type_info.kind {
                    crate::patch::index::TypeKind::Struct { derives, .. } => Some(derives),
                    crate::patch::index::TypeKind::Enum { derives, .. } => Some(derives),
                    _ => None,
                };
                generate_synthetic_vec_types(
                    &change.class_name,
                    &change.new_path,
                    source_derives,
                    &mut all_patches,
                );
            }

            // If this is a VecDestructor enum, ensure it has proper enum_fields
            if is_vec_destructor_type(&change.class_name) {
                ensure_vec_destructor_enum_fields(
                    &change.class_name,
                    &change.new_path,
                    &mut all_patches,
                );
            }

            // If this is a VecDestructorType callback, ensure it has proper callback_typedef
            if is_vec_destructor_callback_type(&change.class_name) {
                ensure_vec_destructor_callback_typedef(&change.class_name, &mut all_patches);
            }

            // If this is an Option type, generate the synthetic Option enum in the "option" module
            if is_option_type(&change.class_name) {
                // Get derives from the parsed type info (from impl_option! macro)
                let derives = match &type_info.kind {
                    crate::patch::index::TypeKind::Enum { derives, .. } => derives.clone(),
                    _ => Vec::new(),
                };
                generate_synthetic_option_type(
                    &change.class_name,
                    &change.new_path,
                    &derives,
                    &mut all_patches,
                );
            }
        } else {
            // Fallback: just update external path if type not found in workspace
            all_patches.entry(key.clone()).or_default().external = Some(change.new_path.clone());

            // Even for fallback cases, generate synthetic types for Vec, VecDestructor, etc.
            // These types are defined via impl_vec! macros and may not be in the workspace index
            if is_vec_type(&change.class_name) {
                // No source derives available since type not found in workspace
                generate_synthetic_vec_types(
                    &change.class_name,
                    &change.new_path,
                    None, // Will use default derives (Clone)
                    &mut all_patches,
                );
            }

            // Even for fallback cases, ensure VecDestructor types have proper enum_fields
            if is_vec_destructor_type(&change.class_name) {
                ensure_vec_destructor_enum_fields(
                    &change.class_name,
                    &change.new_path,
                    &mut all_patches,
                );
            }

            if is_vec_destructor_callback_type(&change.class_name) {
                ensure_vec_destructor_callback_typedef(&change.class_name, &mut all_patches);
            }
        }
    }

    // 3. Add patches for callback_typedefs from API
    // These types don't exist in workspace but are needed for FFI
    let version_data = api_data
        .0
        .values()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No version data found"))?;

    for (module_name, module_data) in &version_data.api {
        for (class_name, class_data) in &module_data.classes {
            if let Some(callback_typedef) = &class_data.callback_typedef {
                let key = (module_name.clone(), class_name.clone());

                // Only create patch if not already exists (from workspace or external path change)
                all_patches.entry(key).or_insert_with(|| {
                    let mut class_patch = ClassPatch::default();
                    class_patch.callback_typedef = Some(callback_typedef.clone());
                    class_patch.doc = class_data.doc.clone();
                    // Add external if present in API
                    class_patch.external = class_data.external.clone();
                    class_patch
                });
            }
        }
    }

    // 4. Ensure all Vec types from API have proper derives (Clone AND Debug at minimum)
    // This handles Vec types that exist in API but weren't discovered as new or changed
    for (module_name, module_data) in &version_data.api {
        for (class_name, class_data) in &module_data.classes {
            // Only process Vec types (not VecDestructor or VecDestructorType)
            if is_vec_type(class_name) {
                let key = (module_name.clone(), class_name.clone());

                // Check if this Vec type is missing required derives (Clone AND Debug)
                let current_derives = class_data.derive.as_ref();
                let has_clone = current_derives
                    .map(|d| d.iter().any(|x| x == "Clone"))
                    .unwrap_or(false);
                let has_debug = current_derives
                    .map(|d| d.iter().any(|x| x == "Debug"))
                    .unwrap_or(false);
                let needs_derive_update = !has_clone || !has_debug;

                if needs_derive_update {
                    // Get external path from API or use a placeholder
                    let external_path = class_data
                        .external
                        .clone()
                        .unwrap_or_else(|| format!("unknown::{}", class_name));

                    // Generate synthetic types with default derives (Clone + Debug)
                    // These go to the "vec" module where they belong
                    generate_synthetic_vec_types(
                        class_name,
                        &external_path,
                        None, // No source derives - will use default (Clone, Debug)
                        &mut all_patches,
                    );
                }
            }

            // Also fix VecDestructor enums that have inline "extern C fn(...)" signatures
            // These should reference the *VecDestructorType callback type instead
            if is_vec_destructor_type(class_name) {
                // Check if any variant has an inline extern signature
                let has_inline_extern = class_data
                    .enum_fields
                    .as_ref()
                    .map(|fields| {
                        fields.iter().any(|variant| {
                            variant.values().any(|vdata| {
                                vdata
                                    .r#type
                                    .as_ref()
                                    .map(|t| t.starts_with("extern"))
                                    .unwrap_or(false)
                            })
                        })
                    })
                    .unwrap_or(false);

                if has_inline_extern {
                    let external_path = class_data
                        .external
                        .clone()
                        .unwrap_or_else(|| format!("unknown::{}", class_name));

                    // Use the SAME module as in api.json, not "vec"
                    ensure_vec_destructor_enum_fields_in_module(
                        class_name,
                        module_name, // Use the module from api.json
                        &external_path,
                        &mut all_patches,
                    );
                }
            }
        }
    }

    // 5. Extract derives, struct_fields, and enum_fields from workspace for ALL existing API types
    // This ensures that types like CssDeclaration get their derives from the source code
    // Also updates existing derives and struct/enum fields to match the actual source code
    for (module_name, module_data) in &version_data.api {
        for (class_name, class_data) in &module_data.classes {
            // Skip callback_typedef types (they don't have derives or fields)
            if class_data.callback_typedef.is_some() {
                continue;
            }

            // Try to find this type in the workspace by its external path
            if let Some(external_path) = &class_data.external {
                if let Some(type_info) = workspace_index.find_type_by_path(external_path) {
                    // Convert type info to a full class patch to get struct/enum fields
                    let full_patch = convert_type_info_to_class_patch(&type_info);

                    // Extract derives and implemented_traits from the workspace type
                    let (derives, implemented_traits) = match &type_info.kind {
                        crate::patch::index::TypeKind::Struct {
                            derives,
                            implemented_traits,
                            ..
                        } => (derives.clone(), implemented_traits.clone()),
                        crate::patch::index::TypeKind::Enum {
                            derives,
                            implemented_traits,
                            ..
                        } => (derives.clone(), implemented_traits.clone()),
                        _ => (Vec::new(), Vec::new()),
                    };

                    // Check if we need to update derives or custom_impls
                    let api_derives = class_data.derive.clone().unwrap_or_default();
                    let api_custom_impls = class_data.custom_impls.clone().unwrap_or_default();

                    let derives_changed = derives != api_derives;
                    let custom_impls_changed = implemented_traits != api_custom_impls;

                    // Check if struct_fields or enum_fields are missing or need update
                    let struct_fields_missing =
                        class_data.struct_fields.is_none() && full_patch.struct_fields.is_some();
                    let enum_fields_missing =
                        class_data.enum_fields.is_none() && full_patch.enum_fields.is_some();

                    // Create a patch if anything differs from api.json
                    if derives_changed
                        || custom_impls_changed
                        || struct_fields_missing
                        || enum_fields_missing
                    {
                        let key = (module_name.clone(), class_name.clone());
                        all_patches
                            .entry(key)
                            .and_modify(|p| {
                                // Always update derives from workspace
                                if derives_changed {
                                    p.derive = Some(derives.clone());
                                }
                                // Always update custom_impls from workspace
                                if custom_impls_changed {
                                    p.custom_impls = Some(implemented_traits.clone());
                                }
                                // Add struct_fields if missing in API but present in workspace
                                if struct_fields_missing {
                                    p.struct_fields = full_patch.struct_fields.clone();
                                }
                                // Add enum_fields if missing in API but present in workspace
                                if enum_fields_missing {
                                    p.enum_fields = full_patch.enum_fields.clone();
                                }
                            })
                            .or_insert_with(|| {
                                let mut patch = ClassPatch::default();
                                if derives_changed {
                                    patch.derive = Some(derives.clone());
                                }
                                if custom_impls_changed {
                                    patch.custom_impls = Some(implemented_traits.clone());
                                }
                                if struct_fields_missing {
                                    patch.struct_fields = full_patch.struct_fields.clone();
                                }
                                if enum_fields_missing {
                                    patch.enum_fields = full_patch.enum_fields.clone();
                                }
                                patch.external = Some(external_path.clone());
                                patch
                            });
                    }
                }
            }
        }
    }

    // 5.1. Ensure all Vec/VecDestructor/VecDestructorType types have proper
    // struct_fields/enum_fields This handles macro-generated types that weren't properly
    // resolved in step 5
    for (module_name, module_data) in &version_data.api {
        for (class_name, class_data) in &module_data.classes {
            // Get external path for generating synthetic types
            let external_path = match &class_data.external {
                Some(p) => p.clone(),
                None => continue,
            };

            // Handle Vec types missing struct_fields
            if is_vec_type(class_name) && class_data.struct_fields.is_none() {
                eprintln!(
                    "[DEBUG 5.1] Vec type '{}' missing struct_fields, generating...",
                    class_name
                );
                // Extract element type from the Vec type name (e.g., "DebugMessageVec" ->
                // "DebugMessage")
                let element_type = class_name.trim_end_matches("Vec");
                let generated = generate_vec_structure(class_name, element_type, &external_path);

                let key = (module_name.clone(), class_name.clone());
                all_patches
                    .entry(key)
                    .and_modify(|p| {
                        if p.struct_fields.is_none() {
                            p.struct_fields = generated.struct_fields.clone();
                            p.doc = generated.doc.clone();
                            p.custom_destructor = generated.custom_destructor;
                            p.vec_element_type = generated.vec_element_type.clone();
                        }
                        if p.derive.is_none()
                            || p.derive.as_ref().map(|d| d.is_empty()).unwrap_or(true)
                        {
                            p.derive = Some(vec!["Clone".to_string(), "Debug".to_string()]);
                        }
                    })
                    .or_insert_with(|| {
                        let mut patch = generated;
                        patch.derive = Some(vec!["Clone".to_string(), "Debug".to_string()]);
                        patch
                    });

                // Also ensure VecDestructor and VecDestructorType exist
                generate_synthetic_vec_types(class_name, &external_path, None, &mut all_patches);
            }

            // Handle VecDestructor types missing enum_fields
            if is_vec_destructor_type(class_name) && class_data.enum_fields.is_none() {
                ensure_vec_destructor_enum_fields(class_name, &external_path, &mut all_patches);
            }

            // Handle VecDestructorType types missing callback_typedef
            if is_vec_destructor_callback_type(class_name) && class_data.callback_typedef.is_none()
            {
                ensure_vec_destructor_callback_typedef(class_name, &mut all_patches);
            }
        }
    }

    // 5.5. Generate MoveModule patches for types in wrong modules
    // Check each type in api.json and generate move patches if needed
    for (module_name, module_data) in &version_data.api {
        for (class_name, _class_data) in &module_data.classes {
            // Determine where this type should actually be
            if let Some(correct_module) = get_correct_module(class_name, module_name) {
                // Type is in wrong module, generate move patch
                let key = (module_name.clone(), class_name.clone());
                all_patches
                    .entry(key)
                    .or_insert_with(|| ClassPatch::default())
                    .move_to_module = Some(correct_module);
            }
        }
    }

    // 6. Write one patch file per class
    for ((module_name, class_name), class_patch) in all_patches {
        // Skip invalid class names (e.g., containing commas or other invalid characters)
        if class_name.contains(',') || class_name.contains(' ') {
            messages.push(AutofixMessage::GenericWarning {
                message: format!("Skipping invalid class name: {}", class_name),
            });
            continue;
        }

        // Create patch structure for this single class
        let mut module_patches = BTreeMap::new();
        let mut classes = BTreeMap::new();
        classes.insert(class_name.clone(), class_patch);

        module_patches.insert(module_name.clone(), ModulePatch { classes });

        let api_patch = ApiPatch {
            versions: BTreeMap::from([(
                version_name.clone(),
                VersionPatch {
                    modules: module_patches,
                },
            )]),
        };

        // Generate filename: ModuleName.ClassName.patch.json
        let patch_filename = format!("{}.{}.patch.json", module_name, class_name);
        let patch_file = patches_dir.join(&patch_filename);

        let patch_json = serde_json::to_string_pretty(&api_patch)?;
        fs::write(&patch_file, patch_json)?;

        patch_count += 1;
    }

    // Patch count will be shown in final statistics
    Ok(patch_count)
}

/// Check if a type name represents a Vec type
fn is_vec_type(type_name: &str) -> bool {
    type_name.ends_with("Vec")
        && !type_name.ends_with("VecDestructor")
        && !type_name.ends_with("VecDestructorType")
        && !type_name.starts_with("Option") // Option*Vec are Option types, not Vec types
}

/// Check if a type name represents a VecDestructor type
fn is_vec_destructor_type(type_name: &str) -> bool {
    type_name.ends_with("VecDestructor") && !type_name.ends_with("VecDestructorType")
}

/// Check if a type name represents a VecDestructorType callback
fn is_vec_destructor_callback_type(type_name: &str) -> bool {
    type_name.ends_with("VecDestructorType")
}

/// Check if a type name represents an Option type
fn is_option_type(type_name: &str) -> bool {
    type_name.starts_with("Option")
}

/// Check if a type name represents a Result type
fn is_result_type(type_name: &str) -> bool {
    type_name.ends_with("Result")
}

/// Generate standard Vec structure: ptr, len, cap, destructor, run_destructor
/// Each field must be a separate IndexMap element to conform to the API schema
/// Also generates standard Vec functions: create(), len(), is_empty(), get(), as_slice()
fn generate_vec_structure(type_name: &str, element_type: &str, external_path: &str) -> ClassPatch {
    use indexmap::IndexMap;

    use crate::api::{FieldData, FunctionData, RefKind, ReturnTypeData};

    let destructor_type = type_name.trim_end_matches("Vec").to_string() + "VecDestructor";
    
    // Use lowercase type name for fn_body variable names (legacy convention)
    // e.g., "DomVec" -> "domvec" (NOT "dom_vec")
    // The transmute_helpers.rs will convert this to snake_case if needed
    let lowercase_type_name = type_name.to_lowercase();

    // IMPORTANT: Each field must be its own IndexMap element to preserve order
    // Schema: [{"ptr": {...}}, {"len": {...}}, {"cap": {...}}, {"destructor": {...}}, {"run_destructor": {...}}]
    let mut ptr_field = IndexMap::new();
    ptr_field.insert(
        "ptr".to_string(),
        FieldData {
            r#type: element_type.to_string(),
            ref_kind: RefKind::ConstPtr,
            arraysize: None,
            doc: None,
            derive: None,
        },
    );

    let mut len_field = IndexMap::new();
    len_field.insert(
        "len".to_string(),
        FieldData {
            r#type: "usize".to_string(),
            ref_kind: RefKind::Value,
            arraysize: None,
            doc: None,
            derive: None,
        },
    );

    let mut cap_field = IndexMap::new();
    cap_field.insert(
        "cap".to_string(),
        FieldData {
            r#type: "usize".to_string(),
            ref_kind: RefKind::Value,
            arraysize: None,
            doc: None,
            derive: None,
        },
    );

    let mut destructor_field = IndexMap::new();
    destructor_field.insert(
        "destructor".to_string(),
        FieldData {
            r#type: destructor_type,
            ref_kind: RefKind::Value,
            arraysize: None,
            doc: None,
            derive: None,
        },
    );

    let mut run_destructor_field = IndexMap::new();
    run_destructor_field.insert(
        "run_destructor".to_string(),
        FieldData {
            r#type: "bool".to_string(),
            ref_kind: RefKind::Value,
            arraysize: None,
            doc: None,
            derive: None,
        },
    );

    // Generate standard Vec functions (without known_types check since this is for initial structure generation)
    let functions = generate_vec_functions(type_name, element_type, &lowercase_type_name, None);

    ClassPatch {
        external: Some(external_path.to_string()),
        doc: Some(vec![format!(
            "Wrapper over a Rust-allocated `Vec<{}>",
            element_type
        )]),
        custom_destructor: Some(true),
        // Empty derive list = don't generate any #[derive(...)] attributes
        // The impl_vec! macro provides Debug, Clone, PartialEq, PartialOrd, Drop
        derive: Some(vec![]),
        struct_fields: Some(vec![
            ptr_field,
            len_field,
            cap_field,
            destructor_field,
            run_destructor_field,
        ]),
        vec_element_type: Some(element_type.to_string()),
        functions: Some(functions),
        ..Default::default()
    }
}

/// Generate standard Vec functions: create(), len(), capacity(), is_empty(), get(), as_slice()
/// If `known_types` is provided, only generates c_get/as_c_slice/as_c_slice_range 
/// when the required Option/Slice types exist.
pub fn generate_vec_functions(
    type_name: &str, 
    element_type: &str, 
    lowercase_type_name: &str,
    known_types: Option<&std::collections::HashSet<String>>,
) -> indexmap::IndexMap<String, crate::api::FunctionData> {
    use indexmap::IndexMap;
    use crate::api::{FunctionData, ReturnTypeData};
    
    let mut functions = IndexMap::new();
    
    // create() - creates an empty Vec
    let create_args = Vec::new();
    functions.insert(
        "create".to_string(),
        FunctionData {
            doc: Some(vec![format!("Creates an empty `{}`", type_name)]),
            fn_args: create_args,
            returns: Some(ReturnTypeData {
                r#type: type_name.to_string(),
                doc: None,
            }),
            fn_body: Some("Self::new()".to_string()),
            ..Default::default()
        },
    );
    
    // with_capacity(cap: usize) - creates a Vec with given capacity
    let mut with_cap_args = Vec::new();
    let mut cap_arg = IndexMap::new();
    cap_arg.insert("cap".to_string(), "usize".to_string());
    with_cap_args.push(cap_arg);
    functions.insert(
        "with_capacity".to_string(),
        FunctionData {
            doc: Some(vec![format!("Creates a `{}` with a given capacity", type_name)]),
            fn_args: with_cap_args,
            returns: Some(ReturnTypeData {
                r#type: type_name.to_string(),
                doc: None,
            }),
            fn_body: Some("Self::with_capacity(cap)".to_string()),
            ..Default::default()
        },
    );
    
    // len(&self) -> usize
    let mut len_args = Vec::new();
    let mut self_arg = IndexMap::new();
    self_arg.insert("self".to_string(), "ref".to_string());
    len_args.push(self_arg);
    functions.insert(
        "len".to_string(),
        FunctionData {
            doc: Some(vec!["Returns the number of elements in the Vec".to_string()]),
            fn_args: len_args,
            returns: Some(ReturnTypeData {
                r#type: "usize".to_string(),
                doc: None,
            }),
            fn_body: Some(format!("{}.len()", lowercase_type_name)),
            ..Default::default()
        },
    );
    
    // capacity(&self) -> usize
    let mut cap_args = Vec::new();
    let mut self_arg = IndexMap::new();
    self_arg.insert("self".to_string(), "ref".to_string());
    cap_args.push(self_arg);
    functions.insert(
        "capacity".to_string(),
        FunctionData {
            doc: Some(vec!["Returns the capacity of the Vec".to_string()]),
            fn_args: cap_args,
            returns: Some(ReturnTypeData {
                r#type: "usize".to_string(),
                doc: None,
            }),
            fn_body: Some(format!("{}.capacity()", lowercase_type_name)),
            ..Default::default()
        },
    );
    
    // is_empty(&self) -> bool
    let mut is_empty_args = Vec::new();
    let mut self_arg = IndexMap::new();
    self_arg.insert("self".to_string(), "ref".to_string());
    is_empty_args.push(self_arg);
    functions.insert(
        "is_empty".to_string(),
        FunctionData {
            doc: Some(vec!["Returns whether the Vec is empty".to_string()]),
            fn_args: is_empty_args,
            returns: Some(ReturnTypeData {
                r#type: "bool".to_string(),
                doc: None,
            }),
            fn_body: Some(format!("{}.is_empty()", lowercase_type_name)),
            ..Default::default()
        },
    );
    
    // c_get(&self, index: usize) -> OptionElement
    // C-API compatible get function that returns a copy wrapped in OptionElement
    // NOTE: Returns OptionElement for FFI safety (wrapped in Option type)
    // Only generate if Option type exists (when known_types is provided)
    // Use canonicalize_option_type_name to get correct casing (OptionU8, not Optionu8)
    let option_element_type = super::utils::canonicalize_option_type_name(element_type);
    let slice_type = format!("{}Slice", type_name);
    
    let option_type_exists = known_types
        .map(|kt| kt.contains(&option_element_type))
        .unwrap_or(true); // If no known_types provided, assume type exists
    
    let slice_type_exists = known_types
        .map(|kt| kt.contains(&slice_type))
        .unwrap_or(true); // If no known_types provided, assume type exists
    
    if option_type_exists {
        let mut get_args = Vec::new();
        let mut self_arg = IndexMap::new();
        self_arg.insert("self".to_string(), "ref".to_string());
        get_args.push(self_arg);
        let mut index_arg = IndexMap::new();
        index_arg.insert("index".to_string(), "usize".to_string());
        get_args.push(index_arg);
        functions.insert(
            "c_get".to_string(),
            FunctionData {
                doc: Some(vec![format!("Returns a copy of the element at the given index, or None if out of bounds. C-API compatible.")]),
                fn_args: get_args,
                returns: Some(ReturnTypeData {
                    r#type: option_element_type,
                    doc: None,
                }),
                fn_body: Some(format!("{}.c_get(index).into()", lowercase_type_name)),
                ..Default::default()
            },
        );
    }
    
    // as_c_slice(&self) -> FooVecSlice
    // Returns a C-compatible slice struct with ptr and len
    // Only generate if Slice type exists (when known_types is provided)
    if slice_type_exists {
        let mut as_c_slice_args = Vec::new();
        let mut self_arg = IndexMap::new();
        self_arg.insert("self".to_string(), "ref".to_string());
        as_c_slice_args.push(self_arg);
        functions.insert(
            "as_c_slice".to_string(),
            FunctionData {
                doc: Some(vec![format!("Returns a C-compatible slice of the entire Vec as a `{}`.", slice_type)]),
                fn_args: as_c_slice_args,
                returns: Some(ReturnTypeData {
                    r#type: slice_type.clone(),
                    doc: None,
                }),
                fn_body: Some(format!("{}.as_c_slice()", lowercase_type_name)),
                ..Default::default()
            },
        );
        
        // as_c_slice_range(&self, start: usize, end: usize) -> FooVecSlice
        // Returns a C-compatible slice of a range within the Vec
        let mut as_c_slice_range_args = Vec::new();
        let mut self_arg = IndexMap::new();
        self_arg.insert("self".to_string(), "ref".to_string());
        as_c_slice_range_args.push(self_arg);
        let mut start_arg = IndexMap::new();
        start_arg.insert("start".to_string(), "usize".to_string());
        as_c_slice_range_args.push(start_arg);
        let mut end_arg = IndexMap::new();
        end_arg.insert("end".to_string(), "usize".to_string());
        as_c_slice_range_args.push(end_arg);
        functions.insert(
            "as_c_slice_range".to_string(),
            FunctionData {
                doc: Some(vec![format!("Returns a C-compatible slice of a range within the Vec. Range is clamped to valid bounds.")]),
                fn_args: as_c_slice_range_args,
                returns: Some(ReturnTypeData {
                    r#type: slice_type,
                    doc: None,
                }),
                fn_body: Some(format!("{}.as_c_slice_range(start, end)", lowercase_type_name)),
                ..Default::default()
            },
        );
    }
    
    // from_item(item: Element) -> Self
    // Creates a Vec containing a single element
    let mut from_item_args = Vec::new();
    let mut item_arg = IndexMap::new();
    item_arg.insert("item".to_string(), element_type.to_string());
    from_item_args.push(item_arg);
    functions.insert(
        "from_item".to_string(),
        FunctionData {
            doc: Some(vec![format!("Creates a `{}` containing a single element", type_name)]),
            fn_args: from_item_args,
            returns: Some(ReturnTypeData {
                r#type: type_name.to_string(),
                doc: None,
            }),
            fn_body: Some("Self::from_item(item)".to_string()),
            ..Default::default()
        },
    );
    
    // copy_from_ptr(ptr: *const Element, len: usize) -> Self
    // Copies elements from a C array into a Vec
    let mut copy_from_ptr_args = Vec::new();
    let mut ptr_arg = IndexMap::new();
    ptr_arg.insert("ptr".to_string(), format!("*const {}", element_type));
    copy_from_ptr_args.push(ptr_arg);
    let mut len_arg = IndexMap::new();
    len_arg.insert("len".to_string(), "usize".to_string());
    copy_from_ptr_args.push(len_arg);
    functions.insert(
        "copy_from_ptr".to_string(),
        FunctionData {
            doc: Some(vec![format!("Copies elements from a C array into a `{}`. The array must be valid for `len` elements.", type_name)]),
            fn_args: copy_from_ptr_args,
            returns: Some(ReturnTypeData {
                r#type: type_name.to_string(),
                doc: None,
            }),
            fn_body: Some("unsafe { Self::copy_from_ptr(ptr, len) }".to_string()),
            ..Default::default()
        },
    );
    
    functions
}

/// Generate standard VecDestructor enum: DefaultRust, NoDestructor, External(Type)
fn generate_vec_destructor(type_name: &str, external_path: &str) -> ClassPatch {
    use indexmap::IndexMap;

    use crate::api::EnumVariantData;

    let destructor_callback_type =
        type_name.trim_end_matches("Destructor").to_string() + "DestructorType";
    // Remove "Vec" from the type name to get the base type
    let base_name = type_name.trim_end_matches("VecDestructor");
    let destructor_callback_simple = format!("{}VecDestructorType", base_name);

    // IMPORTANT: Each variant must be its own IndexMap element to preserve order
    // Schema: [{"DefaultRust": {}}, {"NoDestructor": {}}, {"External": {"type": "T"}}]
    let mut default_rust = IndexMap::new();
    default_rust.insert(
        "DefaultRust".to_string(),
        EnumVariantData {
            r#type: None,
            doc: None,
            ref_kind: Default::default(),
        },
    );

    let mut no_destructor = IndexMap::new();
    no_destructor.insert(
        "NoDestructor".to_string(),
        EnumVariantData {
            r#type: None,
            doc: None,
            ref_kind: Default::default(),
        },
    );

    let mut external = IndexMap::new();
    external.insert(
        "External".to_string(),
        EnumVariantData {
            r#type: Some(destructor_callback_simple),
            doc: None,
            ref_kind: Default::default(),
        },
    );

    // VecDestructor needs Copy AND Clone (Copy requires Clone in Rust)
    // Function pointers are Copy, so this works
    // IMPORTANT: VecDestructor enums have repr(C, u8) in the workspace macro
    ClassPatch {
        external: Some(external_path.to_string()),
        derive: Some(vec!["Clone".to_string(), "Copy".to_string()]),
        enum_fields: Some(vec![default_rust, no_destructor, external]),
        repr: Some("C, u8".to_string()),
        ..Default::default()
    }
}

/// Generate standard VecDestructorType callback typedef
fn generate_vec_destructor_callback(vec_type_name: &str) -> ClassPatch {
    use crate::api::{CallbackArgData, CallbackDefinition, RefKind};

    ClassPatch {
        callback_typedef: Some(CallbackDefinition {
            fn_args: vec![CallbackArgData {
                r#type: vec_type_name.to_string(),
                ref_kind: RefKind::MutPtr, // *mut VecType
                doc: None,
            }],
            returns: None,
        }),
        ..Default::default()
    }
}

/// Generate all three synthetic types for a Vec: Vec, VecDestructor, VecDestructorType
/// Adds them to the all_patches map
/// The `source_derives` parameter contains the derives extracted from impl_vec_*! macros in the
/// source file
fn generate_synthetic_vec_types(
    vec_type_name: &str,
    vec_external_path: &str,
    source_derives: Option<&Vec<String>>,
    all_patches: &mut std::collections::BTreeMap<(String, String), crate::patch::ClassPatch>,
) {
    // Extract element type and crate from external path
    // e.g. "azul_core::callbacks::CoreCallbackDataVec" -> "CoreCallbackData"
    let base_name = vec_type_name.trim_end_matches("Vec");

    // Map Pascal-case primitive types to lowercase Rust primitives
    let element_type = match base_name {
        "U8" => "u8",
        "U16" => "u16",
        "U32" => "u32",
        "U64" => "u64",
        "U128" => "u128",
        "Usize" => "usize",
        "I8" => "i8",
        "I16" => "i16",
        "I32" => "i32",
        "I64" => "i64",
        "I128" => "i128",
        "Isize" => "isize",
        "F32" => "f32",
        "F64" => "f64",
        "Bool" => "bool",
        "Char" => "char",
        other => other, // Keep as-is for non-primitive types
    };

    // Generate external paths for the destructor types
    let destructor_name = format!("{}VecDestructor", base_name);
    let destructor_type_name = format!("{}VecDestructorType", base_name);

    // Extract crate and module from vec_external_path
    let path_parts: Vec<&str> = vec_external_path.rsplitn(2, "::").collect();
    let crate_module = if path_parts.len() == 2 {
        path_parts[1]
    } else {
        vec_external_path
    };

    let destructor_external = format!("{}::{}", crate_module, destructor_name);

    // Vec types always get Clone and Debug derives as minimum
    // These are the most common traits needed, as parent types often derive them
    // Additional derives come from source_derives (impl_vec_*! macros)
    let default_vec_derives = vec!["Clone".to_string(), "Debug".to_string()];

    let effective_derives = if let Some(derives) = source_derives {
        if !derives.is_empty() {
            // Merge source derives with defaults to ensure Clone and Debug are always present
            let mut merged: Vec<String> = derives.clone();
            for d in &default_vec_derives {
                if !merged.contains(d) {
                    merged.push(d.clone());
                }
            }
            merged
        } else {
            default_vec_derives.clone()
        }
    } else {
        default_vec_derives.clone()
    };

    // 1. Add Vec structure if it doesn't have struct_fields yet
    // IMPORTANT: Always set derives for Vec types (Clone at minimum)
    let vec_key = ("vec".to_string(), vec_type_name.to_string());
    all_patches
        .entry(vec_key)
        .and_modify(|patch| {
            // Always ensure derives are set for Vec types
            if patch.derive.is_none() || patch.derive.as_ref().map(|d| d.is_empty()).unwrap_or(true)
            {
                patch.derive = Some(effective_derives.clone());
            }

            if patch.struct_fields.is_none() {
                // Only add struct_fields, keep existing derives
                let generated =
                    generate_vec_structure(vec_type_name, element_type, vec_external_path);
                patch.struct_fields = generated.struct_fields;
                patch.doc = generated.doc;
                patch.custom_destructor = generated.custom_destructor;
                patch.vec_element_type = generated.vec_element_type;
                if patch.external.is_none() {
                    patch.external = generated.external;
                }
            }
        })
        .or_insert_with(|| {
            let mut generated =
                generate_vec_structure(vec_type_name, element_type, vec_external_path);
            generated.derive = Some(effective_derives.clone());
            generated
        });

    // 2. Add VecDestructor enum - always ensure enum_fields and derives are set
    let destructor_key = ("vec".to_string(), destructor_name.clone());
    all_patches
        .entry(destructor_key)
        .and_modify(|existing| {
            let full_patch = generate_vec_destructor(&destructor_name, &destructor_external);
            // Always update derives for VecDestructor (needs Clone + Copy)
            existing.derive = full_patch.derive;
            // Always update repr for VecDestructor (needs C, u8)
            existing.repr = full_patch.repr;
            // If existing patch doesn't have enum_fields, add them
            if existing.enum_fields.is_none() {
                existing.enum_fields = full_patch.enum_fields;
            }
        })
        .or_insert_with(|| generate_vec_destructor(&destructor_name, &destructor_external));

    // 3. Add VecDestructorType callback
    let destructor_type_key = ("vec".to_string(), destructor_type_name);
    all_patches
        .entry(destructor_type_key)
        .or_insert_with(|| generate_vec_destructor_callback(vec_type_name));
}

/// Ensure a VecDestructor type has proper enum_fields
/// Called when a VecDestructor type is encountered (not just when the Vec type is found)
fn ensure_vec_destructor_enum_fields(
    destructor_type_name: &str,
    external_path: &str,
    all_patches: &mut std::collections::BTreeMap<(String, String), crate::patch::ClassPatch>,
) {
    // Extract the Vec type name from the destructor name
    // e.g., "DebugMessageVecDestructor" -> "DebugMessageVec"
    let vec_type_name = destructor_type_name.trim_end_matches("Destructor");
    let destructor_callback_type = format!("{}DestructorType", vec_type_name);

    let key = ("vec".to_string(), destructor_type_name.to_string());

    // ALWAYS generate fresh enum_fields to fix inline "extern C fn(...)" signatures
    // The old api.json has inline signatures like "extern \"C\" fn(*mut FooVec)"
    // which should be replaced with "FooVecDestructorType" callback references
    let full_patch = generate_vec_destructor(destructor_type_name, external_path);

    all_patches
        .entry(key)
        .and_modify(|existing| {
            // Always replace enum_fields to fix inline extern signatures
            existing.enum_fields = full_patch.enum_fields.clone();
            // Always set repr for VecDestructor (needs C, u8)
            existing.repr = full_patch.repr.clone();
            if existing.derive.is_none() {
                existing.derive = full_patch.derive.clone();
            }
        })
        .or_insert(full_patch);

    // Also ensure the callback type exists
    let callback_key = ("vec".to_string(), destructor_callback_type.clone());
    all_patches
        .entry(callback_key)
        .and_modify(|existing| {
            if existing.callback_typedef.is_none() {
                let full_patch = generate_vec_destructor_callback(vec_type_name);
                existing.callback_typedef = full_patch.callback_typedef;
            }
        })
        .or_insert_with(|| generate_vec_destructor_callback(vec_type_name));
}

/// Ensure a VecDestructor enum has proper enum_fields in a specific module
/// Used to fix inline "extern C fn(...)" signatures in existing API types
fn ensure_vec_destructor_enum_fields_in_module(
    destructor_type_name: &str,
    module_name: &str,
    external_path: &str,
    all_patches: &mut std::collections::BTreeMap<(String, String), crate::patch::ClassPatch>,
) {
    // Extract the Vec type name from the destructor name
    // e.g., "StyleBackgroundContentVecDestructor" -> "StyleBackgroundContentVec"
    let vec_type_name = destructor_type_name.trim_end_matches("Destructor");
    let destructor_callback_type = format!("{}DestructorType", vec_type_name);

    // Use the module from api.json, not "vec"
    let key = (module_name.to_string(), destructor_type_name.to_string());

    // ALWAYS generate fresh enum_fields to fix inline "extern C fn(...)" signatures
    let full_patch = generate_vec_destructor(destructor_type_name, external_path);

    all_patches
        .entry(key)
        .and_modify(|existing| {
            // Always replace enum_fields to fix inline extern signatures
            existing.enum_fields = full_patch.enum_fields.clone();
            // Always set repr for VecDestructor (needs C, u8)
            existing.repr = full_patch.repr.clone();
            if existing.derive.is_none() {
                existing.derive = full_patch.derive.clone();
            }
        })
        .or_insert(full_patch);

    // Also ensure the callback type exists (in same module)
    let callback_key = (module_name.to_string(), destructor_callback_type.clone());
    all_patches
        .entry(callback_key)
        .and_modify(|existing| {
            if existing.callback_typedef.is_none() {
                let full_patch = generate_vec_destructor_callback(vec_type_name);
                existing.callback_typedef = full_patch.callback_typedef;
            }
        })
        .or_insert_with(|| generate_vec_destructor_callback(vec_type_name));
}

/// Ensure a VecDestructorType callback has proper callback_typedef
fn ensure_vec_destructor_callback_typedef(
    destructor_callback_type_name: &str,
    all_patches: &mut std::collections::BTreeMap<(String, String), crate::patch::ClassPatch>,
) {
    // Extract the Vec type name from the callback type name
    // e.g., "DebugMessageVecDestructorType" -> "DebugMessageVec"
    let vec_type_name = destructor_callback_type_name.trim_end_matches("DestructorType");

    let key = ("vec".to_string(), destructor_callback_type_name.to_string());
    all_patches
        .entry(key)
        .and_modify(|existing| {
            if existing.callback_typedef.is_none() {
                let full_patch = generate_vec_destructor_callback(vec_type_name);
                existing.callback_typedef = full_patch.callback_typedef;
            }
        })
        .or_insert_with(|| generate_vec_destructor_callback(vec_type_name));
}

/// Generate synthetic Option type enum: None, Some(T)
/// All Option types go into the "option" module regardless of their source module
fn generate_synthetic_option_type(
    option_type_name: &str,
    external_path: &str,
    derives_from_macro: &[String],
    all_patches: &mut std::collections::BTreeMap<(String, String), crate::patch::ClassPatch>,
) {
    use indexmap::IndexMap;

    use crate::api::EnumVariantData;

    // Extract the inner type from OptionFoo -> Foo
    let inner_type_raw = option_type_name.trim_start_matches("Option");

    if inner_type_raw.is_empty() {
        return; // Invalid option type name
    }

    // Map Pascal-case primitive types to lowercase Rust primitives
    // These are special because api.json should use lowercase for primitives
    let inner_type = match inner_type_raw {
        "U8" => "u8",
        "U16" => "u16",
        "U32" => "u32",
        "U64" => "u64",
        "U128" => "u128",
        "Usize" => "usize",
        "I8" => "i8",
        "I16" => "i16",
        "I32" => "i32",
        "I64" => "i64",
        "I128" => "i128",
        "Isize" => "isize",
        "F32" => "f32",
        "F64" => "f64",
        "Bool" => "bool",
        "Char" => "char",
        other => other, // Keep as-is for non-primitive types
    };

    // All Option types go into the "option" module
    let option_key = ("option".to_string(), option_type_name.to_string());

    // Check if already exists with proper enum_fields
    if let Some(existing) = all_patches.get(&option_key) {
        if existing.enum_fields.is_some() {
            return; // Already properly defined
        }
    }

    // Generate the Option enum with proper schema:
    // [{"None": {}}, {"Some": {"type": "InnerType"}}]
    let mut none_variant = IndexMap::new();
    none_variant.insert(
        "None".to_string(),
        EnumVariantData {
            r#type: None,
            doc: None,
            ref_kind: Default::default(),
        },
    );

    let mut some_variant = IndexMap::new();
    some_variant.insert(
        "Some".to_string(),
        EnumVariantData {
            r#type: Some(inner_type.to_string()),
            doc: None,
            ref_kind: Default::default(),
        },
    );

    // Use derives from the actual impl_option! macro call in source code
    // If no derives provided, use a sensible default (Debug, Clone, PartialEq, PartialOrd)
    let derive = if !derives_from_macro.is_empty() {
        Some(derives_from_macro.to_vec())
    } else {
        // Fallback: provide standard derives for Option types
        Some(vec![
            "Debug".to_string(),
            "Clone".to_string(),
            "PartialEq".to_string(),
            "PartialOrd".to_string(),
        ])
    };

    let class_patch = ClassPatch {
        external: Some(external_path.to_string()),
        derive,
        enum_fields: Some(vec![none_variant, some_variant]),
        ..Default::default()
    };

    all_patches.insert(option_key, class_patch);
}

/// Convert ParsedTypeInfo to ClassPatch
pub fn convert_type_info_to_class_patch(type_info: &ParsedTypeInfo) -> ClassPatch {
    use indexmap::IndexMap;

    use crate::api::{EnumVariantData, FieldData};

    let mut class_patch = ClassPatch {
        external: Some(type_info.full_path.clone()),
        ..Default::default()
    };

    match &type_info.kind {
        TypeKind::Struct {
            fields,
            doc,
            generic_params,
            derives,
            implemented_traits,
            ..
        } => {
            class_patch.doc = doc.clone();

            // Set derives from source code (always set, even if empty, to be explicit)
            class_patch.derive = Some(derives.clone());

            // Set custom_impls from implemented_traits (manual impl Trait for Type blocks)
            if !implemented_traits.is_empty() {
                class_patch.custom_impls = Some(implemented_traits.clone());
            }

            // Set generic_params if not empty
            if !generic_params.is_empty() {
                class_patch.generic_params = Some(generic_params.clone());
            }

            // Convert IndexMap<String, FieldInfo> to Vec<IndexMap<String, FieldData>>
            // IMPORTANT: Each field must be its own IndexMap element to preserve order
            // Schema: [{"field1": {...}}, {"field2": {...}}, ...]
            let struct_fields: Vec<IndexMap<String, FieldData>> = fields
                .iter()
                .map(|(field_name, field_info)| {
                    // Normalize the field type for FFI (Box<T> -> *const c_void, etc.)
                    let (normalized_type, _) =
                        crate::autofix::utils::normalize_generic_type(&field_info.ty);

                    let mut single_field = IndexMap::new();
                    single_field.insert(
                        field_name.clone(),
                        FieldData {
                            r#type: normalized_type,
                            ref_kind: field_info.ref_kind,
                            arraysize: None,
                            doc: field_info.doc.clone(),
                            derive: None,
                        },
                    );
                    single_field
                })
                .collect();

            if !struct_fields.is_empty() {
                class_patch.struct_fields = Some(struct_fields);
            }
        }
        TypeKind::Enum {
            variants,
            doc,
            generic_params,
            derives,
            implemented_traits,
            ..
        } => {
            class_patch.doc = doc.clone();

            // Set derives from source code (always set, even if empty, to be explicit)
            class_patch.derive = Some(derives.clone());

            // Set custom_impls from implemented_traits (manual impl Trait for Type blocks)
            if !implemented_traits.is_empty() {
                class_patch.custom_impls = Some(implemented_traits.clone());
            }

            // Set generic_params if not empty
            if !generic_params.is_empty() {
                class_patch.generic_params = Some(generic_params.clone());
            }

            // Convert IndexMap<String, VariantInfo> to Vec<IndexMap<String, EnumVariantData>>
            // IMPORTANT: Each variant must be its own IndexMap element in the Vec
            // to preserve variant order (JSON objects don't guarantee order)
            // Schema: [{"Variant1": {}}, {"Variant2": {"type": "T"}}]
            let enum_fields: Vec<IndexMap<String, EnumVariantData>> = variants
                .iter()
                .map(|(variant_name, variant_info)| {
                    // Normalize variant type for FFI (Box<T> -> *const c_void, etc.)
                    let normalized_type = variant_info
                        .ty
                        .as_ref()
                        .map(|ty| crate::autofix::utils::normalize_generic_type(ty).0);

                    let mut single_variant = IndexMap::new();
                    single_variant.insert(
                        variant_name.clone(),
                        EnumVariantData {
                            r#type: normalized_type,
                            doc: variant_info.doc.clone(),
                            ref_kind: Default::default(),
                        },
                    );
                    single_variant
                })
                .collect();

            if !enum_fields.is_empty() {
                class_patch.enum_fields = Some(enum_fields);
            }
        }
        TypeKind::TypeAlias {
            target,
            generic_base,
            generic_args,
            doc,
        } => {
            // For type aliases, use existing doc or create one
            class_patch.doc = doc
                .clone()
                .or_else(|| Some(vec![format!("Type alias for {}", target)]));

            // Store type alias information
            // For generic instantiations: CssPropertyValue<LayoutZIndex>
            // For simple aliases: GridTemplate
            use crate::api::TypeAliasInfo;
            if let Some(base) = generic_base {
                // Generic type alias: target = CssPropertyValue<T>, base = CssPropertyValue
                class_patch.type_alias = Some(TypeAliasInfo {
                    target: base.clone(),
                    ref_kind: Default::default(),
                    generic_args: generic_args.clone(),
                });
            } else {
                // Simple type alias: target = GridTemplate, no generics
                class_patch.type_alias = Some(TypeAliasInfo {
                    target: target.clone(),
                    ref_kind: Default::default(),
                    generic_args: vec![],
                });
            }
        }
        TypeKind::CallbackTypedef {
            fn_args,
            returns,
            doc,
        } => {
            // For callback typedefs, create the callback_typedef structure
            use crate::api::{CallbackArgData, CallbackDefinition, ReturnTypeData};

            let callback_args: Vec<CallbackArgData> = fn_args
                .iter()
                .map(|arg| {
                    CallbackArgData {
                        r#type: arg.ty.clone(),
                        ref_kind: arg.ref_kind, // BorrowMode is Copy
                        doc: None,
                    }
                })
                .collect();

            let return_data = returns.as_ref().map(|ret| ReturnTypeData {
                r#type: ret.clone(),
                doc: None,
            });

            class_patch.callback_typedef = Some(CallbackDefinition {
                fn_args: callback_args,
                returns: return_data,
            });

            // Don't set type_alias for callbacks - it would be corrupt anyway
            class_patch.type_alias = None;

            // Use existing doc or create one describing the callback
            class_patch.doc = doc.clone().or_else(|| {
                let args_str = fn_args
                    .iter()
                    .map(|a| format!("{} {}", a.ref_kind, a.ty))
                    .collect::<Vec<_>>()
                    .join(", ");
                let ret_str = returns
                    .as_ref()
                    .map(|r| format!(" -> {}", r))
                    .unwrap_or_default();
                Some(vec![format!(
                    "Callback function: fn({}){}",
                    args_str, ret_str
                )])
            });
        }
    }

    class_patch
}

/// Virtual Patch Application: Apply patches in-memory and re-discover dependencies
///
/// This enables truly recursive discovery by:
/// 1. Creating a temporary API with newly discovered types
/// 2. Re-running type discovery to find their dependencies
/// 3. Repeating until no new types are found
pub fn virtual_patch_application<T: TypeLookup>(
    api_data: &ApiData,
    workspace_index: &T,
    initial_types: Vec<ParsedTypeInfo>,
    mut known_types: BTreeMap<String, String>,
    messages: &mut AutofixMessages,
) -> Result<(Vec<ParsedTypeInfo>, PatchSummary)> {
    let mut all_discovered_types = initial_types;
    let mut visited_types = BTreeSet::new();
    let mut iteration = 0;
    let max_virtual_iterations = 5;

    // Track types already in the initial discovery
    for type_info in &all_discovered_types {
        visited_types.insert(type_info.type_name.clone());
        known_types.insert(type_info.type_name.clone(), type_info.full_path.clone());
    }

    loop {
        iteration += 1;
        if iteration > max_virtual_iterations {
            messages.push(AutofixMessage::MaxIterationsReached { iteration });
            break;
        }

        messages.push(AutofixMessage::IterationStarted {
            iteration,
            count: all_discovered_types.len(),
        });

        // Collect all types referenced by the currently discovered types
        let mut newly_referenced_types = BTreeSet::new();
        for type_info in &all_discovered_types {
            let sub_types = collect_referenced_types_from_type_info(type_info);
            for (type_name, _origin) in sub_types {
                newly_referenced_types.insert(type_name);
            }
        }

        // Find which referenced types are not yet known
        let missing_types: Vec<String> = newly_referenced_types
            .iter()
            .filter(|type_name| {
                !known_types.contains_key(type_name.as_str())
                    && !visited_types.contains(type_name.as_str())
            })
            .cloned()
            .collect();

        if missing_types.is_empty() {
            // No new dependencies - will be reflected in final report
            break;
        }

        // Dependencies count will be shown in report

        // Discover the missing types
        let mut newly_discovered = Vec::new();
        for type_name in &missing_types {
            // Mark as visited to prevent cycles
            visited_types.insert(type_name.clone());

            if let Some(type_info) = discover_type(workspace_index, type_name, messages) {
                // Skip types from external crates
                if !is_workspace_type(&type_info.full_path) {
                    messages.push(AutofixMessage::TypeSkipped {
                        type_name: type_name.clone(),
                        reason: SkipReason::ExternalCrate(type_info.full_path.clone()),
                    });
                    continue;
                }

                // Check if type has repr(C) layout
                let has_ffi_repr = match &type_info.kind {
                    TypeKind::Struct { repr, .. } => repr.is_some(),
                    TypeKind::Enum { repr, .. } => repr.is_some(),
                    TypeKind::TypeAlias { .. } => true, // Type aliases don't have layout
                    TypeKind::CallbackTypedef { .. } => true, // Callbacks are extern "C"
                };

                if !has_ffi_repr {
                    messages.push(AutofixMessage::TypeSkipped {
                        type_name: type_name.clone(),
                        reason: SkipReason::MissingReprC,
                    });
                    continue;
                }

                known_types.insert(type_name.clone(), type_info.full_path.clone());
                newly_discovered.push(type_info);
            } else {
                // Only report TypeNotFound if it's not a suppressed type
                if !crate::autofix::should_suppress_type_not_found(&type_name) {
                    messages.push(AutofixMessage::TypeNotFound {
                        type_name: type_name.clone(),
                    });
                }
            }
        }

        if newly_discovered.is_empty() {
            // No new types - will be reflected in final report
            break;
        }

        // Discovery count will be shown in report

        // Add to the list of all discovered types
        all_discovered_types.extend(newly_discovered);
    }

    // Final statistics will be shown in report

    // Build final patch summary
    let mut patch_summary = PatchSummary::default();
    for type_info in &all_discovered_types {
        let module = infer_module_from_path(&type_info.full_path);
        patch_summary.classes_added.push(ClassAdded {
            class_name: type_info.type_name.clone(),
            module,
            external_path: type_info.full_path.clone(),
        });
    }

    Ok((all_discovered_types, patch_summary))
}

/// Check if a type has field/variant changes compared to the API
pub fn has_field_changes(
    api_data: &ApiData,
    class_name: &str,
    workspace_type: &ParsedTypeInfo,
    messages: &mut AutofixMessages,
) -> bool {
    use crate::{autofix::message::AutofixMessage, patch::index::TypeKind};

    // Find the class in the API - prefer exact path match
    let mut api_class = None;
    let workspace_path = &workspace_type.full_path;

    // First try: Find by matching external path
    'outer: for version_data in api_data.0.values() {
        for module_data in version_data.api.values() {
            if let Some(class_data) = module_data.classes.get(class_name) {
                if let Some(external) = &class_data.external {
                    if are_paths_synonyms(external, workspace_path) || external == workspace_path {
                        api_class = Some(class_data);
                        break 'outer;
                    }
                }
            }
        }
    }

    // Second try: If no exact match, take first match by name
    if api_class.is_none() {
        'outer2: for version_data in api_data.0.values() {
            for module_data in version_data.api.values() {
                if let Some(class_data) = module_data.classes.get(class_name) {
                    api_class = Some(class_data);
                    break 'outer2;
                }
            }
        }
    }

    let Some(api_class) = api_class else {
        // Type not in API, no field changes to detect
        return false;
    };

    // Compare struct fields
    match &workspace_type.kind {
        TypeKind::Struct { fields, .. } => {
            // Check if API has struct_fields
            let Some(api_fields) = &api_class.struct_fields else {
                // API doesn't have struct_fields but workspace does
                messages.push(AutofixMessage::GenericWarning {
                    message: format!("{}: API missing struct_fields, will update", class_name),
                });
                return true;
            };

            // struct_fields is an array where each element is a single-key map
            // Schema: [{"field1": {...}}, {"field2": {...}}, ...]
            // Count the total number of fields across all maps (should be 1 per map)
            let api_field_count = api_fields.len();

            // Compare field count
            if fields.len() != api_field_count {
                messages.push(AutofixMessage::GenericWarning {
                    message: format!(
                        "{}: Field count mismatch (workspace: {}, API: {})",
                        class_name,
                        fields.len(),
                        api_field_count
                    ),
                });
                return true;
            }

            // Build a set of all field names in the API
            let api_field_names: std::collections::BTreeSet<&String> = api_fields
                .iter()
                .flat_map(|field_map| field_map.keys())
                .collect();

            // Compare each field - check if workspace fields exist in API
            for (workspace_field_name, _workspace_field) in fields.iter() {
                if !api_field_names.contains(workspace_field_name) {
                    messages.push(AutofixMessage::GenericWarning {
                        message: format!(
                            "{}: Field '{}' not found in API",
                            class_name, workspace_field_name
                        ),
                    });
                    return true;
                }
            }

            // Note: We're not comparing types because they might use different representations
            // (e.g., *mut c_void vs *mut LayoutWindow). The important thing is that the
            // field names match and the count is correct.
        }
        TypeKind::Enum { variants, .. } => {
            // Check if API has enum_fields
            let Some(api_variants) = &api_class.enum_fields else {
                // API doesn't have enum_fields but workspace does
                messages.push(AutofixMessage::GenericWarning {
                    message: format!("{}: API missing enum_fields, will update", class_name),
                });
                return true;
            };

            // enum_fields is an array where each element is a single-key map
            // Schema: [{"Variant1": {...}}, {"Variant2": {...}}, ...]
            // Count the total number of variants
            let api_variant_count = api_variants.len();

            // Compare variant count
            if variants.len() != api_variant_count {
                messages.push(AutofixMessage::GenericWarning {
                    message: format!(
                        "{}: Variant count mismatch (workspace: {}, API: {})",
                        class_name,
                        variants.len(),
                        api_variant_count
                    ),
                });
                return true;
            }

            // Build a set of all variant names in the API
            let api_variant_names: std::collections::BTreeSet<&String> = api_variants
                .iter()
                .flat_map(|variant_map| variant_map.keys())
                .collect();

            // Compare each variant name - check if workspace variants exist in API
            for workspace_variant_name in variants.keys() {
                if !api_variant_names.contains(workspace_variant_name) {
                    messages.push(AutofixMessage::GenericWarning {
                        message: format!(
                            "{}: Variant '{}' not found in API",
                            class_name, workspace_variant_name
                        ),
                    });
                    return true;
                }
            }
        }
        TypeKind::TypeAlias { .. } => {
            // Check if API has type_alias field
            if api_class.type_alias.is_none() {
                // API doesn't have type_alias but workspace has a type alias
                messages.push(AutofixMessage::GenericWarning {
                    message: format!("{}: API missing type_alias field, will update", class_name),
                });
                return true;
            }
            // Type alias exists in both - no field comparison needed
            return false;
        }
        TypeKind::CallbackTypedef { .. } => {
            // Check if API has callback_typedef field
            if api_class.callback_typedef.is_none() {
                messages.push(AutofixMessage::GenericWarning {
                    message: format!(
                        "{}: API missing callback_typedef field, will update",
                        class_name
                    ),
                });
                return true;
            }
            // Callback typedef exists in both - no field comparison needed
            return false;
        }
    }

    false
}

/// Convert CamelCase to snake_case
/// 
/// Examples:
/// - "DomVec" -> "dom_vec"
/// - "AccessibilityActionVec" -> "accessibility_action_vec"
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}
