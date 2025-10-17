/// Automatic API fixing with recursive type discovery
///
/// This module analyzes the API and automatically:
/// 1. Finds all referenced types in the workspace
/// 2. Recursively discovers dependencies
/// 3. Generates patches for missing/incorrect types
/// 4. Provides a clean summary of changes
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::Result;

use crate::{
    api::ApiData,
    autofix::message::{AutofixMessages, ClassAdded, PatchSummary},
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
            messages.info("discovery", format!("Found type: {}", type_name));
            return Some(candidates[0].clone());
        } else if !candidates.is_empty() {
            messages.warning(
                "discovery",
                format!("Multiple matches for {}, using first", type_name),
            );
            return Some(candidates[0].clone());
        }
    }

    // Try string search (for macro-defined types)
    if let Some(type_info) = workspace_index.find_type_by_string_search(type_name) {
        messages.info(
            "discovery",
            format!("Found via string search: {}", type_name),
        );
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

        // No exact match, use first one but warn
        messages.warning(
            "analysis",
            format!("Multiple matches for {}, using first", simple_name),
        );
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
    let parts: Vec<&str> = type_path.split("::").collect();

    if parts.len() >= 2 {
        // Take the part after the crate name
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
        messages.info("patches", "No patches to generate");
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
        all_patches.insert((module, class_name), class_patch);
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
            messages.warning(
                "patches",
                format!("Skipping invalid class name: {}", class_name),
            );
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

    messages.info("patches", format!("Generated {} patch files", patch_count));

    Ok(patch_count)
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
            messages.warning(
                "virtual-patch",
                "Reached maximum virtual patch iteration limit",
            );
            break;
        }

        messages.info(
            "virtual-patch",
            format!(
                "Virtual iteration {}: analyzing {} types",
                iteration,
                all_discovered_types.len()
            ),
        );

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
            messages.info(
                "virtual-patch",
                format!("No new dependencies found after {} iterations", iteration),
            );
            break;
        }

        messages.info(
            "virtual-patch",
            format!("Found {} new dependencies to discover", missing_types.len()),
        );

        // Discover the missing types
        let mut newly_discovered = Vec::new();
        for type_name in &missing_types {
            // Mark as visited to prevent cycles
            visited_types.insert(type_name.clone());

            if let Some(type_info) = discover_type(workspace_index, type_name, messages) {
                // Skip types from external crates
                if !is_workspace_type(&type_info.full_path) {
                    messages.warning(
                        "virtual-patch",
                        format!(
                            "Skipping external crate type: {} ({})",
                            type_name, type_info.full_path
                        ),
                    );
                    continue;
                }

                // Check if type has repr(C) layout
                let has_repr_c = match &type_info.kind {
                    TypeKind::Struct { has_repr_c, .. } => *has_repr_c,
                    TypeKind::Enum { has_repr_c, .. } => *has_repr_c,
                    TypeKind::TypeAlias { .. } => true, // Type aliases don't have layout
                };

                if !has_repr_c {
                    messages.warning(
                        "virtual-patch",
                        format!(
                            "Skipping type without #[repr(C)]: {} (not FFI-safe)",
                            type_name
                        ),
                    );
                    continue;
                }

                known_types.insert(type_name.clone(), type_info.full_path.clone());
                newly_discovered.push(type_info);
            } else {
                messages.warning(
                    "virtual-patch",
                    format!("Could not find dependency: {}", type_name),
                );
            }
        }

        if newly_discovered.is_empty() {
            messages.info("virtual-patch", "No new types discovered in this iteration");
            break;
        }

        messages.info(
            "virtual-patch",
            format!(
                "Discovered {} new types via virtual patching",
                newly_discovered.len()
            ),
        );

        // Add to the list of all discovered types
        all_discovered_types.extend(newly_discovered);
    }

    messages.info(
        "virtual-patch",
        format!(
            "Virtual patch application complete: {} total types discovered",
            all_discovered_types.len()
        ),
    );

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
