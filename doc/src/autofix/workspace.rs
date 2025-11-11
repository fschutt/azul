/// Automatic API fixing with recursive type discovery
///
/// This module analyzes the API and automatically:
/// 1. Finds all referenced types in the workspace
/// 2. Recursively discovers dependencies
/// 3. Generates patches for missing/incorrect types
/// 4. Provides a clean summary of changes
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt, fs,
    path::Path,
};

use anyhow::Result;

use crate::{
    api::ApiData,
    autofix::message::{AutofixMessage, AutofixMessages, ClassAdded, PatchSummary, SkipReason},
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
        }
    }
}

/// Collect all types currently in the API (including callback_typedefs)
pub fn collect_all_api_types(api_data: &ApiData) -> HashMap<String, String> {
    let mut types = HashMap::new();

    for (_version_name, version_data) in &api_data.0 {
        for (_module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Include callback_typedefs - they need patches for FFI
                // (e.g. FooDestructorType is callback_typedef but needs patch)

                let type_path = class_data
                    .external
                    .as_ref()
                    .map(|s| s.as_str())
                    .unwrap_or(class_name.as_str())
                    .to_string();

                types.insert(class_name.clone(), type_path);
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
    let self_crate_name = env!("CARGO_PKG_NAME");
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
pub fn discover_type(
    workspace_index: &WorkspaceIndex,
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
pub fn find_type_in_workspace(
    workspace_index: &WorkspaceIndex,
    class_name: &str,
    expected_path: &str,
    messages: &mut AutofixMessages,
) -> Option<ParsedTypeInfo> {
    // Extract simple name from path
    let simple_name = expected_path.split("::").last().unwrap_or(class_name);

    // Try to find by full path first
    if let Some(type_info) = workspace_index.find_type_by_path(expected_path) {
        return Some(type_info.clone());
    }

    // Try to find by simple name
    if let Some(candidates) = workspace_index.find_type(simple_name) {
        if candidates.len() == 1 {
            return Some(candidates[0].clone());
        }

        // Multiple matches, try to find the one matching the expected path
        for candidate in candidates {
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
) -> HashMap<String, TypeOrigin> {
    use crate::api::extract_base_type_if_not_opaque;

    let mut types = HashMap::new();
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
        TypeKind::TypeAlias { target, .. } => {
            if let Some(base_type) = extract_base_type_if_not_opaque(target) {
                types.insert(
                    base_type.clone(),
                    TypeOrigin::TypeAlias {
                        parent_type: parent_type.clone(),
                    },
                );
            }
        }
    }

    types
}

/// Infer module name from type path (e.g., "azul_core::dom::DomNodeId" -> "dom")
pub fn infer_module_from_path(type_path: &str) -> String {
    // Extract type name from path
    let type_name = type_path.split("::").last().unwrap_or("");

    // Check if it's a synthetic type that should go to a specific module
    if is_vec_type(type_name)
        || is_vec_destructor_type(type_name)
        || is_vec_destructor_callback_type(type_name)
    {
        return "vec".to_string();
    }

    if is_option_type(type_name) {
        return "option".to_string();
    }

    if is_result_type(type_name) {
        return "error".to_string();
    }

    // For regular types, take the part after the crate name
    let parts: Vec<&str> = type_path.split("::").collect();

    if parts.len() >= 2 {
        parts[1].to_string()
    } else {
        "unknown".to_string()
    }
}

/// Generate patches for discovered types
pub fn generate_patches(
    api_data: &ApiData,
    workspace_index: &WorkspaceIndex,
    types_to_add: &[ParsedTypeInfo],
    patch_summary: &PatchSummary,
    patches_dir: &Path,
    messages: &mut AutofixMessages,
) -> Result<usize> {
    use std::collections::BTreeMap;

    if types_to_add.is_empty() && patch_summary.external_path_changes.is_empty() {
        // No message needed - will be reflected in final statistics
        return Ok(0);
    }

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
        let class_name = type_info.type_name.clone();

        let class_patch = convert_type_info_to_class_patch(type_info);
        all_patches.insert((module.clone(), class_name.clone()), class_patch);

        // If this is a Vec type, generate the synthetic Destructor and DestructorType
        if is_vec_type(&class_name) {
            generate_synthetic_vec_types(&class_name, &type_info.full_path, &mut all_patches);
        }
    }

    // 2. Add patches for external path changes
    for change in &patch_summary.external_path_changes {
        let module = infer_module_from_path(&change.new_path);
        let key = (module.clone(), change.class_name.clone());

        // Get struct/enum info from workspace if available
        if let Some(type_info) = workspace_index.find_type_by_path(&change.new_path) {
            // Merge with existing patch or create new one with full info
            let full_patch = convert_type_info_to_class_patch(type_info);
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
                generate_synthetic_vec_types(
                    &change.class_name,
                    &change.new_path,
                    &mut all_patches,
                );
            }
        } else {
            // Fallback: just update external path if type not found in workspace
            all_patches.entry(key).or_default().external = Some(change.new_path.clone());
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

    // 3. Write one patch file per class
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

/// Generate standard Vec structure: ptr, len, cap, destructor
fn generate_vec_structure(type_name: &str, element_type: &str, external_path: &str) -> ClassPatch {
    use indexmap::IndexMap;

    use crate::api::FieldData;

    let destructor_type = type_name.trim_end_matches("Vec").to_string() + "VecDestructor";

    let mut field_map = IndexMap::new();
    field_map.insert(
        "ptr".to_string(),
        FieldData {
            r#type: format!("*const {}", element_type),
            doc: None,
            derive: None,
        },
    );
    field_map.insert(
        "len".to_string(),
        FieldData {
            r#type: "usize".to_string(),
            doc: None,
            derive: None,
        },
    );
    field_map.insert(
        "cap".to_string(),
        FieldData {
            r#type: "usize".to_string(),
            doc: None,
            derive: None,
        },
    );
    field_map.insert(
        "destructor".to_string(),
        FieldData {
            r#type: destructor_type,
            doc: None,
            derive: None,
        },
    );

    ClassPatch {
        external: Some(external_path.to_string()),
        doc: Some(format!(
            "Wrapper over a Rust-allocated `Vec<{}>",
            element_type
        )),
        custom_destructor: Some(true),
        struct_fields: Some(vec![field_map]),
        ..Default::default()
    }
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

    let mut variant_map = IndexMap::new();
    variant_map.insert(
        "DefaultRust".to_string(),
        EnumVariantData {
            r#type: None,
            doc: None,
        },
    );
    variant_map.insert(
        "NoDestructor".to_string(),
        EnumVariantData {
            r#type: None,
            doc: None,
        },
    );
    variant_map.insert(
        "External".to_string(),
        EnumVariantData {
            r#type: Some(destructor_callback_simple),
            doc: None,
        },
    );

    ClassPatch {
        external: Some(external_path.to_string()),
        derive: Some(vec!["Copy".to_string()]),
        enum_fields: Some(vec![variant_map]),
        ..Default::default()
    }
}

/// Generate standard VecDestructorType callback typedef
fn generate_vec_destructor_callback(vec_type_name: &str) -> ClassPatch {
    use crate::api::{CallbackArgData, CallbackDefinition};

    ClassPatch {
        callback_typedef: Some(CallbackDefinition {
            fn_args: vec![CallbackArgData {
                r#type: vec_type_name.to_string(),
                ref_kind: "refmut".to_string(),
                doc: None,
            }],
            returns: None,
        }),
        ..Default::default()
    }
}

/// Generate all three synthetic types for a Vec: Vec, VecDestructor, VecDestructorType
/// Adds them to the all_patches map
fn generate_synthetic_vec_types(
    vec_type_name: &str,
    vec_external_path: &str,
    all_patches: &mut std::collections::BTreeMap<(String, String), crate::patch::ClassPatch>,
) {
    // Extract element type and crate from external path
    // e.g. "azul_core::callbacks::CoreCallbackDataVec" -> "CoreCallbackData"
    let base_name = vec_type_name.trim_end_matches("Vec");
    let element_type = base_name; // Simplified, would need proper element type extraction

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

    // 1. Add Vec structure if it doesn't have struct_fields yet
    let vec_key = ("vec".to_string(), vec_type_name.to_string());
    all_patches
        .entry(vec_key)
        .and_modify(|patch| {
            if patch.struct_fields.is_none() {
                *patch = generate_vec_structure(vec_type_name, element_type, vec_external_path);
            }
        })
        .or_insert_with(|| generate_vec_structure(vec_type_name, element_type, vec_external_path));

    // 2. Add VecDestructor enum
    let destructor_key = ("vec".to_string(), destructor_name.clone());
    all_patches
        .entry(destructor_key)
        .or_insert_with(|| generate_vec_destructor(&destructor_name, &destructor_external));

    // 3. Add VecDestructorType callback
    let destructor_type_key = ("vec".to_string(), destructor_type_name);
    all_patches
        .entry(destructor_type_key)
        .or_insert_with(|| generate_vec_destructor_callback(vec_type_name));
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
        TypeKind::Struct { fields, doc, .. } => {
            class_patch.doc = doc.clone();

            // Convert IndexMap<String, FieldInfo> to Vec<IndexMap<String, FieldData>>
            let mut field_map = IndexMap::new();
            for (field_name, field_info) in fields {
                field_map.insert(
                    field_name.clone(),
                    FieldData {
                        r#type: field_info.ty.clone(),
                        doc: field_info.doc.clone(),
                        derive: None,
                    },
                );
            }

            if !field_map.is_empty() {
                class_patch.struct_fields = Some(vec![field_map]);
            }
        }
        TypeKind::Enum { variants, doc, .. } => {
            class_patch.doc = doc.clone();

            // Convert IndexMap<String, VariantInfo> to Vec<IndexMap<String, EnumVariantData>>
            let mut variant_map = IndexMap::new();
            for (variant_name, variant_info) in variants {
                variant_map.insert(
                    variant_name.clone(),
                    EnumVariantData {
                        r#type: variant_info.ty.clone(),
                        doc: variant_info.doc.clone(),
                    },
                );
            }

            if !variant_map.is_empty() {
                class_patch.enum_fields = Some(vec![variant_map]);
            }
        }
        TypeKind::TypeAlias { target, doc } => {
            // For type aliases, use existing doc or create one
            class_patch.doc = doc
                .clone()
                .or_else(|| Some(format!("Type alias for {}", target)));
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
pub fn virtual_patch_application(
    api_data: &ApiData,
    workspace_index: &WorkspaceIndex,
    initial_types: Vec<ParsedTypeInfo>,
    mut known_types: HashMap<String, String>,
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
        let mut newly_referenced_types = HashSet::new();
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
                let has_repr_c = match &type_info.kind {
                    TypeKind::Struct { has_repr_c, .. } => *has_repr_c,
                    TypeKind::Enum { has_repr_c, .. } => *has_repr_c,
                    TypeKind::TypeAlias { .. } => true, // Type aliases don't have layout
                };

                if !has_repr_c {
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

            // struct_fields is an array with one element that is a map of all fields
            // Extract the actual fields from the first (and only) map element
            let api_field_count = if let Some(first_map) = api_fields.first() {
                first_map.len()
            } else {
                0
            };

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

            // Compare each field - iterate through the keys in the first map
            if let Some(api_field_map) = api_fields.first() {
                for (workspace_field_name, _workspace_field) in fields.iter() {
                    if !api_field_map.contains_key(workspace_field_name) {
                        messages.push(AutofixMessage::GenericWarning {
                            message: format!(
                                "{}: Field '{}' not found in API",
                                class_name, workspace_field_name
                            ),
                        });
                        return true;
                    }
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

            // enum_fields is an array with one element that is a map of all variants
            // Extract the actual variant count from the first (and only) map element
            let api_variant_count = if let Some(first_map) = api_variants.first() {
                first_map.len()
            } else {
                0
            };

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

            // Compare each variant name - check if they exist in the first map
            if let Some(api_variant_map) = api_variants.first() {
                for workspace_variant_name in variants.keys() {
                    if !api_variant_map.contains_key(workspace_variant_name) {
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
        }
        TypeKind::TypeAlias { .. } => {
            // Type aliases don't have fields to compare
            return false;
        }
    }

    false
}
