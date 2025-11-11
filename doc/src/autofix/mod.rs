use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::Path,
    time::Instant,
};

use anyhow::Result;

use self::{
    message::{AutofixMessage, AutofixMessages, ExternalPathChange, PatchSummary, SkipReason},
    workspace::{
        are_paths_synonyms, collect_all_api_types, collect_referenced_types_from_type_info,
        discover_type, find_type_in_workspace, generate_patches, is_workspace_type,
        virtual_patch_application, TypeOrigin,
    },
};
use crate::{
    api::{collect_all_referenced_types_from_api, ApiData},
    patch::index::{TypeKind, WorkspaceIndex},
};

pub mod discover;
pub mod message;
pub mod regexes;
pub mod utils;
pub mod workspace;

/// Main entry point for autofix with recursive type discovery
pub fn autofix_api_recursive(
    api_data: &ApiData,
    project_root: &Path,
    output_dir: &Path,
) -> Result<()> {
    // Phase 0: Initialization
    println!("🔍 Initializing autofix...");
    println!("   • Loading api.json");

    println!("   • Compiling regexes");
    let regexes = regexes::CompiledRegexes::new()
        .map_err(|e| anyhow::anyhow!("Failed to compile regexes: {}", e))?;

    println!("   • Building workspace index");

    let start_time = Instant::now();
    let mut messages = AutofixMessages::new();

    let workspace_index = WorkspaceIndex::build_with_regexes(project_root, regexes.clone())?;
    println!(
        "     ✓ Indexed {} types from {} files",
        workspace_index.types.len(),
        workspace_index.files.len()
    );

    println!("\n🔄 Running analysis (this may take a moment)...\n");

    // Step 1: Collect all types referenced in API
    let api_types = collect_all_api_types(api_data);
    let referenced_types = collect_all_referenced_types_from_api(api_data);

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

    // Step 2: Recursive type discovery with cycle detection
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
                messages.push(AutofixMessage::TypeSkipped {
                    type_name: (*type_name).clone(),
                    reason: SkipReason::CallbackTypedef,
                });
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
            messages.push(AutofixMessage::MaxIterationsReached { iteration });
            break;
        }

        if types_to_discover.is_empty() {
            messages.push(AutofixMessage::IterationComplete {
                iteration: iteration - 1,
            });
            break;
        }

        messages.push(AutofixMessage::IterationStarted {
            iteration,
            count: types_to_discover.len(),
        });

        let mut newly_discovered = Vec::new();

        // Discover each missing type
        for type_name in &types_to_discover {
            // Skip if already visited (cycle detection)
            if visited_types.contains(type_name) {
                messages.push(AutofixMessage::TypeSkipped {
                    type_name: type_name.clone(),
                    reason: SkipReason::AlreadyVisited,
                });
                continue;
            }

            visited_types.insert(type_name.clone());

            if let Some(type_info) = discover_type(&workspace_index, type_name, &mut messages) {
                // Skip types from external crates (not part of azul workspace)
                if !is_workspace_type(&type_info.full_path) {
                    // Extract crate name from path
                    let crate_name = type_info
                        .full_path
                        .split("::")
                        .next()
                        .unwrap_or("unknown")
                        .to_string();
                    messages.push(AutofixMessage::TypeSkipped {
                        type_name: type_name.clone(),
                        reason: SkipReason::ExternalCrate(crate_name),
                    });
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
                    messages.push(AutofixMessage::TypeSkipped {
                        type_name: type_name.clone(),
                        reason: SkipReason::MissingReprC,
                    });
                    continue;
                }

                // Record successful discovery
                if let Some(origin) = type_origins.get(type_name) {
                    messages.push(AutofixMessage::TypeDiscovered {
                        type_name: type_name.clone(),
                        path: type_info.full_path.clone(),
                        reason: origin.clone(),
                    });
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
                messages.push(AutofixMessage::TypeNotFound {
                    type_name: type_name.clone(),
                });
            }
        }

        // Update discovery queue with newly found dependencies
        types_to_discover = newly_discovered;

        if types_to_discover.is_empty() {
            // All dependencies discovered - will be shown in final report
            break;
        }
    }

    // Discovery statistics will be shown in final report

    // Step 5: Virtual Patch Application - Apply patches in-memory and re-discover
    // This enables truly recursive discovery by finding dependencies of newly added types
    let (final_types_to_add, final_patch_summary) = if !types_to_add.is_empty() {
        // Virtual patching will be reflected in final report

        virtual_patch_application(
            api_data,
            &workspace_index,
            types_to_add,
            api_types.clone(), // Clone to avoid move
            &mut messages,
        )?
    } else {
        // No virtual patching needed - will be shown in report
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
                messages.push(AutofixMessage::TypeSkipped {
                    type_name: class_name.clone(),
                    reason: SkipReason::ExternalCrate(workspace_type.full_path.clone()),
                });
                continue;
            }

            // Check for external path changes (but skip synonyms)
            if workspace_type.full_path != *api_type_path
                && !are_paths_synonyms(&workspace_type.full_path, api_type_path)
            {
                patch_summary
                    .external_path_changes
                    .push(ExternalPathChange {
                        class_name: class_name.clone(),
                        old_path: api_type_path.clone(),
                        new_path: workspace_type.full_path.clone(),
                    });
            }

            // Check for field/variant changes
            if has_field_changes(api_data, class_name, &workspace_type, &mut messages) {
                // Add workspace type to types_to_add so it generates a patch
                // This will update the struct_fields/enum_fields
                patch_summary
                    .external_path_changes
                    .push(ExternalPathChange {
                        class_name: class_name.clone(),
                        old_path: api_type_path.clone(),
                        new_path: workspace_type.full_path.clone(),
                    });
            }
        }
    }

    // Generate patches
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

    // Analysis complete - print comprehensive report
    let duration = start_time.elapsed();
    println!("✅ Analysis complete ({:.1}s)\n", duration.as_secs_f32());

    messages.print_report(
        &patch_summary,
        duration.as_secs_f32(),
        &patches_dir,
        patch_count,
    );

    Ok(())
}
