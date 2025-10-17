use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::Path,
};

use anyhow::Result;

use self::{
    message::{AutofixMessages, ExternalPathChange, PatchSummary},
    workspace::{
        collect_all_api_types, collect_referenced_types_from_type_info, discover_type,
        find_type_in_workspace, generate_patches, is_workspace_type, virtual_patch_application,
        TypeOrigin,
    },
};
use crate::{
    api::{collect_all_referenced_types_from_api, ApiData},
    patch::index::{TypeKind, WorkspaceIndex},
};

pub mod discover;
pub mod message;
pub mod utils;
pub mod workspace;

/// Main entry point for autofix with recursive type discovery
pub fn autofix_api_recursive(
    api_data: &ApiData,
    project_root: &Path,
    output_dir: &Path,
) -> Result<()> {
    let mut messages = AutofixMessages::new();

    // Step 1: Build workspace index (quiet)
    let workspace_index = WorkspaceIndex::build_with_verbosity(project_root, false)?;
    messages.info(
        "init",
        format!(
            "Indexed {} types from {} files",
            workspace_index.types.len(),
            workspace_index.files.len()
        ),
    );

    // Step 2: Collect all types referenced in API
    let mut api_types = collect_all_api_types(api_data);
    messages.info(
        "analysis",
        format!("Found {} types in API", api_types.len()),
    );

    // Step 3: Find all referenced types (from function signatures, fields, etc.)
    let referenced_types = collect_all_referenced_types_from_api(api_data);
    messages.info(
        "analysis",
        format!("Found {} referenced types", referenced_types.len()),
    );

    // Collect all callback_typedefs from API - these don't need discovery
    let mut api_callback_typedefs = BTreeSet::new();
    for version_data in api_data.0.values() {
        for module_data in version_data.api.values() {
            for (class_name, class_data) in &module_data.classes {
                if class_data.callback_typedef.is_some() {
                    api_callback_typedefs.insert(class_name.clone());
                }
            }
        }
    }
    messages.info(
        "analysis",
        format!(
            "Found {} callback_typedefs in API",
            api_callback_typedefs.len()
        ),
    );

    // Step 4: Recursive type discovery with cycle detection
    let mut discovered_types = HashMap::new();
    let mut types_to_add = Vec::new();
    let mut visited_types = BTreeSet::new(); // Track visited types to detect cycles
    let mut type_origins: HashMap<String, TypeOrigin> = HashMap::new(); // Track why each type was added
    let mut iteration = 0;
    let max_iterations = 10; // Prevent infinite loops

    // Start with initial missing types (exclude API callback_typedefs)
    let mut types_to_discover: Vec<String> = referenced_types
        .iter()
        .filter(|type_name| {
            // Skip if already in API
            if api_types.contains_key(*type_name) {
                return false;
            }
            // Skip if it's a callback_typedef in API (will be handled separately)
            if api_callback_typedefs.contains(*type_name) {
                messages.info(
                    "skip",
                    format!("Skipping API callback_typedef: {}", type_name),
                );
                return false;
            }
            true
        })
        .cloned()
        .collect();

    // Mark initial types as coming from API
    for type_name in &types_to_discover {
        type_origins.insert(type_name.clone(), TypeOrigin::ApiReference);
    }

    loop {
        iteration += 1;
        if iteration > max_iterations {
            messages.warning("recursion", "Reached maximum iteration limit");
            break;
        }

        if types_to_discover.is_empty() {
            messages.info("iteration", "No more types to discover");
            break;
        }

        messages.info(
            "iteration",
            format!(
                "Iteration {}: {} types to discover",
                iteration,
                types_to_discover.len()
            ),
        );

        let mut newly_discovered = Vec::new();

        // Discover each missing type
        for type_name in &types_to_discover {
            // Skip if already visited (cycle detection)
            if visited_types.contains(type_name) {
                messages.info(
                    "cycle",
                    format!("Skipping already visited type: {}", type_name),
                );
                continue;
            }

            visited_types.insert(type_name.clone());

            if let Some(type_info) = discover_type(&workspace_index, type_name, &mut messages) {
                // Skip types from external crates (not part of azul workspace)
                if !is_workspace_type(&type_info.full_path) {
                    messages.warning(
                        "discovery",
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
                    TypeKind::TypeAlias { .. } => {
                        // Type aliases don't have layout attributes, allow them
                        true
                    }
                };

                if !has_repr_c {
                    messages.warning(
                        "layout",
                        format!(
                            "Skipping type without #[repr(C)]: {} (not FFI-safe)",
                            type_name
                        ),
                    );
                    continue;
                }

                // Print why this type is being added
                if let Some(origin) = type_origins.get(type_name) {
                    let reason = match origin {
                        TypeOrigin::ApiReference => format!("referenced in API"),
                        TypeOrigin::StructField {
                            parent_type,
                            field_name,
                        } => {
                            format!("field '{}' in struct '{}'", field_name, parent_type)
                        }
                        TypeOrigin::EnumVariant {
                            parent_type,
                            variant_name,
                        } => {
                            format!("variant '{}' in enum '{}'", variant_name, parent_type)
                        }
                        TypeOrigin::TypeAlias { parent_type } => {
                            format!("type alias '{}'", parent_type)
                        }
                    };
                    messages.info(
                        "discovery",
                        format!("= note: adding '{}' because it's {}", type_name, reason),
                    );
                }

                // Recursively collect all types referenced by this type
                let sub_types = collect_referenced_types_from_type_info(&type_info);

                discovered_types.insert(type_name.clone(), type_info.clone());
                types_to_add.push(type_info);

                // Add sub-types to discovery queue if not yet known
                for (sub_type, origin) in sub_types {
                    if !api_types.contains_key(&sub_type)
                        && !discovered_types.contains_key(&sub_type)
                        && !visited_types.contains(&sub_type)
                    {
                        newly_discovered.push(sub_type.clone());
                        type_origins.insert(sub_type, origin);
                    }
                }
            } else {
                messages.warning("discovery", format!("Could not find type: {}", type_name));
            }
        }

        // Update discovery queue with newly found dependencies
        types_to_discover = newly_discovered;

        if types_to_discover.is_empty() {
            messages.info("iteration", "All dependencies discovered");
            break;
        }
    }

    messages.info(
        "discovery",
        format!("Discovered {} new types to add", types_to_add.len()),
    );
    messages.info(
        "cycle-detection",
        format!(
            "Visited {} unique types (including cycles)",
            visited_types.len()
        ),
    );

    // Step 5: Virtual Patch Application - Apply patches in-memory and re-discover
    // This enables truly recursive discovery by finding dependencies of newly added types
    let (final_types_to_add, final_patch_summary) = if !types_to_add.is_empty() {
        messages.info(
            "virtual-patch",
            "Starting virtual patch application for deeper discovery",
        );

        virtual_patch_application(
            api_data,
            &workspace_index,
            types_to_add,
            api_types.clone(), // Clone to avoid move
            &mut messages,
        )?
    } else {
        messages.info(
            "virtual-patch",
            "No new types discovered, skipping virtual patch application",
        );
        (types_to_add, PatchSummary::default())
    };

    // Step 6: Analyze existing types for changes
    let mut patch_summary = final_patch_summary;

    for (class_name, api_type_path) in &api_types {
        if let Some(workspace_type) =
            find_type_in_workspace(&workspace_index, class_name, api_type_path, &mut messages)
        {
            // Skip types from self crate
            if !is_workspace_type(&workspace_type.full_path) {
                messages.warning(
                    "analysis",
                    format!(
                        "Skipping external path change for {}: {} (not a workspace type)",
                        class_name, workspace_type.full_path
                    ),
                );
                continue;
            }

            // Check for external path changes
            if workspace_type.full_path != *api_type_path {
                patch_summary
                    .external_path_changes
                    .push(ExternalPathChange {
                        class_name: class_name.clone(),
                        old_path: api_type_path.clone(),
                        new_path: workspace_type.full_path.clone(),
                    });
            }

            // Check for field/variant changes
            // TODO: Implement field comparison
        }
    }

    // Step 7: Generate patches
    let work_dir = project_root.join("target").join("autofix");
    let patches_dir = work_dir.join("patches");
    fs::create_dir_all(&patches_dir)?;

    let patch_count = generate_patches(
        api_data,
        &workspace_index,
        &final_types_to_add,
        &patch_summary,
        &patches_dir,
        &mut messages,
    )?;

    // Step 8: Print warnings and errors
    messages.print_warnings_and_errors();

    // Step 9: Print summary
    patch_summary.print();

    let (info_count, warning_count, error_count) = messages.count_by_level();
    println!(
        "\n📊 Messages: {} info, {} warnings, {} errors",
        info_count, warning_count, error_count
    );
    println!("📁 Patches saved to: {}", patches_dir.display());

    if patch_count > 0 {
        println!("\n💡 Next steps:");
        println!("  1. Review the generated patches");
        println!(
            "  2. Apply them with: azul-docs patch {}",
            work_dir.display()
        );
    }

    Ok(())
}
