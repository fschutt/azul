use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::Path,
    time::Instant,
};

use anyhow::Result;
use colored::Colorize;

use self::{
    message::{AutofixMessage, AutofixMessages, ExternalPathChange, PatchSummary, SkipReason},
    workspace::{
        are_paths_synonyms, collect_all_api_types, collect_referenced_types_from_type_info,
        discover_type, find_type_in_workspace, generate_patches, has_field_changes,
        is_workspace_type, virtual_patch_application, TypeOrigin,
    },
};
use crate::{
    api::{collect_all_referenced_types_from_api_with_chains, find_all_unused_types_recursive, generate_removal_patches, ApiData},
    patch::index::{TypeKind, WorkspaceIndex},
};

// Legacy modules (to be refactored)
pub mod message;
pub mod utils;
pub mod workspace;

// New modular architecture
pub mod types;
pub mod discovery;
pub mod analysis;
pub mod patches;
pub mod output;

// V2 architecture modules
pub mod type_index;
pub mod type_resolver;
pub mod diff;
pub mod debug;
pub mod patch_format;
pub mod module_map;

/// Check if a type should be ignored in "Could not find type" warnings
pub fn should_suppress_type_not_found(type_name: &str) -> bool {
    // Primitive types
    const PRIMITIVES: &[&str] = &[
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char",
    ];

    // Standard library types
    const STD_TYPES: &[&str] = &[
        "String", "str", "Vec", "Option", "Result", "Box", "Rc", "Arc", "RefCell", "Cell",
    ];

    // Type aliases that are commonly used but internal (no longer suppressed for macro-generated types)
    const INTERNAL_ALIASES: &[&str] = &[
        "BoxCssPropertyCache",
        "Widows",
        "Orphans",
        "OptionStyledDom",
        "OptionCoreMenuCallback",
        "OptionLinuxDecorationsState",
        "OptionComputedScrollbarStyle",
        "OptionPixelValue",
        "SystemClipboard",
        "AzDuration",
    ];

    let trimmed = type_name.trim();

    // Check primitives
    if PRIMITIVES.contains(&trimmed) {
        return true;
    }

    // Check std types
    if STD_TYPES.contains(&trimmed) {
        return true;
    }

    // Check internal aliases
    if INTERNAL_ALIASES.contains(&trimmed) {
        return true;
    }

    // Check arrays [T; N]
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        return true;
    }

    // Check function pointers: extern "C" fn (...)
    if trimmed.contains("extern") && trimmed.contains("fn") {
        return true;
    }

    // Check for partial function signatures (missing extern but has fn and ->)
    if trimmed.contains("fn") && (trimmed.contains("->") || trimmed.contains("c_void")) {
        return true;
    }

    // Check if it contains documentation text (spaces and punctuation)
    if trimmed.contains(' ') && (trimmed.contains('.') || trimmed.len() > 50) {
        return true;
    }

    // Check for namespace qualifiers (::) - these are often implementation details
    if trimmed.contains("::") {
        return true;
    }

    // Check for compound types with commas (tuples or multiple types)
    if trimmed.contains(',') {
        return true;
    }

    // Check for type parameters in the name itself
    if trimmed.contains('<') || trimmed.contains('>') {
        return true;
    }

    // Check for lowercase single words like "value", "ref", "refmut"
    if trimmed.len() < 10 && trimmed.chars().all(|c| c.is_lowercase() || c == '_') {
        return true;
    }

    false
}

/// Main entry point for autofix with recursive type discovery
pub fn autofix_api_recursive(
    api_data: &ApiData,
    project_root: &Path,
    output_dir: &Path,
) -> Result<()> {
    use std::sync::Arc;
    use rayon::prelude::*;
    
    // Phase 0: Initialization
    println!("[SEARCH] Initializing autofix...");
    println!("   - Loading api.json");

    let start_time = Instant::now();
    let phase_start = Instant::now();
    let mut messages = AutofixMessages::new();

    // Build workspace index
    println!("   - Building workspace index...");
    
    let workspace_index = WorkspaceIndex::build(project_root)?;
    
    println!(
        "     [OK] Indexed {} types from {} files in {:.1}s",
        workspace_index.types.len(),
        workspace_index.files.len(),
        start_time.elapsed().as_secs_f64()
    );

    println!("\n[REFRESH] Running analysis...\n");

    // Step 1: Collect all types referenced in API
    let step1_start = Instant::now();
    // Now returns Vec<(class_name, module_name, type_path)> to handle all occurrences
    let api_types_list = collect_all_api_types(api_data);
    
    // Build a set of class names for quick lookup (used for filtering)
    let api_type_names: std::collections::HashSet<String> = api_types_list
        .iter()
        .map(|(class_name, _, _)| class_name.clone())
        .collect();
    
    // Get all types referenced by fields, variants, etc. in the API - now with reference chains
    let (referenced_types, reference_chains) = collect_all_referenced_types_from_api_with_chains(api_data);
    println!("  [STEP 1] Collected {} API types, {} referenced types ({:.2}s)", 
        api_types_list.len(), referenced_types.len(), step1_start.elapsed().as_secs_f64());

    // CRITICAL: Also check for missing types referenced by EXISTING API types
    // This handles the case where a type like XmlNode exists in api.json,
    // but its field type XmlNodeChildVec doesn't exist as a class definition
    let step2_start = Instant::now();
    let mut missing_from_existing: HashSet<String> = HashSet::new();
    for ref_type in &referenced_types {
        // Skip callback signatures (they start with "extern")
        if ref_type.starts_with("extern ") {
            continue;
        }
        // Skip array types
        if ref_type.starts_with("[") {
            continue;
        }
        // Skip documentation strings that were accidentally parsed as types
        if ref_type.contains(' ') && !ref_type.contains("::") {
            continue;
        }
        if !api_type_names.contains(ref_type) {
            missing_from_existing.insert(ref_type.clone());
        }
    }
    if !missing_from_existing.is_empty() {
        println!("  [STEP 2] Found {} types referenced in API but not defined ({:.2}s)", 
            missing_from_existing.len(), step2_start.elapsed().as_secs_f64());
        for t in missing_from_existing.iter().take(10) {
            // Print with reference chain if available
            if let Some(chain) = reference_chains.get(t) {
                println!("    - {} (via: {})", t, chain);
            } else {
                println!("    - {}", t);
            }
        }
        if missing_from_existing.len() > 10 {
            println!("    ... and {} more", missing_from_existing.len() - 10);
        }
    }

    // Collect all callback_typedefs from API - these don't need discovery
    let step3_start = Instant::now();
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
    println!("  [STEP 3] Collected {} callback typedefs ({:.2}s)", 
        api_callback_typedefs.len(), step3_start.elapsed().as_secs_f64());

    // Step 4: Recursive type discovery with cycle detection
    let step4_start = Instant::now();
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
            if api_type_names.contains(*type_name) {
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
                // Don't log cycles for standard library types (they're expected)
                let is_std_type = matches!(
                    type_name.as_str(),
                    "String"
                        | "str"
                        | "usize"
                        | "isize"
                        | "u8"
                        | "u16"
                        | "u32"
                        | "u64"
                        | "u128"
                        | "i8"
                        | "i16"
                        | "i32"
                        | "i64"
                        | "i128"
                        | "f32"
                        | "f64"
                        | "bool"
                        | "char"
                        | "Vec"
                        | "Option"
                        | "Result"
                        | "Box"
                        | "Arc"
                        | "Rc"
                        | "Cell"
                        | "RefCell"
                );

                if !is_std_type {
                    messages.push(AutofixMessage::TypeSkipped {
                        type_name: type_name.clone(),
                        reason: SkipReason::AlreadyVisited,
                    });
                }
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
                    TypeKind::CallbackTypedef { .. } => {
                        // Callback typedefs are always extern "C", allow them
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
                    if !api_type_names.contains(&sub_type)
                        && !discovered_types.contains_key(&sub_type)
                        && !visited_types.contains(&sub_type)
                    {
                        newly_discovered.push(sub_type.clone());
                        type_origins.insert(sub_type, origin);
                    }
                }
            } else {
                // Only report TypeNotFound if it's not a suppressed type
                if !should_suppress_type_not_found(&type_name) {
                    messages.push(AutofixMessage::TypeNotFound {
                        type_name: type_name.clone(),
                    });
                }
            }
        }

        // Update discovery queue with newly found dependencies
        types_to_discover = newly_discovered;

        if types_to_discover.is_empty() {
            // All dependencies discovered - will be shown in final report
            break;
        }
    }

    println!("  [STEP 4] Type discovery complete: {} types in {} iterations ({:.2}s)", 
        discovered_types.len(), iteration - 1, step4_start.elapsed().as_secs_f64());

    // Step 5: Virtual Patch Application - Apply patches in-memory and re-discover
    // This enables truly recursive discovery by finding dependencies of newly added types
    let step5_start = Instant::now();
    // Build a HashMap for virtual_patch_application (it needs type_name -> path mapping)
    let api_types_map: HashMap<String, String> = api_types_list
        .iter()
        .map(|(class_name, _, type_path)| (class_name.clone(), type_path.clone()))
        .collect();
    
    let (mut final_types_to_add, final_patch_summary) = if !types_to_add.is_empty() {
        // Virtual patching will be reflected in final report

        virtual_patch_application(
            api_data,
            &workspace_index,
            types_to_add,
            api_types_map, // Use the HashMap version
            &mut messages,
        )?
    } else {
        // No virtual patching needed - will be shown in report
        (types_to_add, PatchSummary::default())
    };
    println!("  [STEP 5] Virtual patch application ({:.2}s)", step5_start.elapsed().as_secs_f64());

    // Step 6: Analyze existing types for changes
    let step6_start = Instant::now();
    let mut patch_summary = final_patch_summary;

    // Step 6: Check for field changes in existing types
    // Now iterates over ALL occurrences, not just unique class names
    let total_api_types = api_types_list.len();
    let mut processed = 0;
    for (class_name, module_name, api_type_path) in &api_types_list {
        processed += 1;
        if processed % 100 == 0 {
            print!("\r  [STEP 6] Checking field changes... {}/{}", processed, total_api_types);
            std::io::Write::flush(&mut std::io::stdout()).ok();
        }
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
                        module_name: module_name.clone(),
                        old_path: api_type_path.clone(),
                        new_path: workspace_type.full_path.clone(),
                    });
            }

            // Check for field/variant changes
            if has_field_changes(api_data, class_name, &workspace_type, &mut messages) {
                // Add workspace type to final_types_to_add so it generates a structural patch
                // This will update the struct_fields/enum_fields in api.json
                final_types_to_add.push(workspace_type.clone());

                // Also record as path change if paths differ
                if workspace_type.full_path != *api_type_path {
                    patch_summary
                        .external_path_changes
                        .push(ExternalPathChange {
                            class_name: class_name.clone(),
                            module_name: module_name.clone(),
                            old_path: api_type_path.clone(),
                            new_path: workspace_type.full_path.clone(),
                        });
                }
            }
        }
    }
    println!("\r  [STEP 6] Checked {} types for field changes ({:.2}s)     ", 
        total_api_types, step6_start.elapsed().as_secs_f64());

    // Step 7: Generate patches
    let step7_start = Instant::now();
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
    println!("  [STEP 7] Generated {} patches ({:.2}s)", patch_count, step7_start.elapsed().as_secs_f64());

    // Step 8: Generate patches to remove unused types (recursively finds all)
    let step8_start = Instant::now();
    // These are written to the same patches directory so a single "patch" command applies everything
    let unused_types = find_all_unused_types_recursive(api_data);
    let mut total_unused_removed = 0;
    
    if !unused_types.is_empty() {
        println!("  [STEP 8] Found {} unused types to remove ({:.2}s)", 
            unused_types.len(), step8_start.elapsed().as_secs_f64());
        
        // Generate removal patches and write to the main patches directory
        let removal_patches = generate_removal_patches(&unused_types);
        
        for (idx, patch) in removal_patches.iter().enumerate() {
            // Use zzz_ prefix to ensure removal patches are applied LAST
            // (after all other patches have been applied)
            let patch_filename = format!("zzz_{:03}_remove_unused.patch.json", idx);
            let patch_path = patches_dir.join(&patch_filename);
            let patch_json = serde_json::to_string_pretty(patch)?;
            fs::write(&patch_path, patch_json)?;
        }
        
        total_unused_removed = unused_types.len();
        
        // List the unused types
        let mut by_module: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
        for ut in &unused_types {
            by_module.entry(ut.module_name.clone()).or_default().push(ut.type_name.clone());
        }
        
        for (module, types) in &by_module {
            println!("   - {}: {}", module, types.join(", "));
        }
    }

    // Analysis complete - print comprehensive report
    let duration = start_time.elapsed();
    println!("\n[OK] Analysis complete ({:.1}s)\n", duration.as_secs_f32());

    messages.print_report(
        &patch_summary,
        duration.as_secs_f32(),
        &patches_dir,
        patch_count + total_unused_removed,
    );
    
    if total_unused_removed > 0 {
        println!("\n[CLEANUP] {} unused type removal patches included", total_unused_removed);
    }

    Ok(())
}

// ============================================================================
// MAIN ENTRY POINT
// ============================================================================

/// Main autofix implementation
/// 
/// This version:
/// 1. Builds a clean type index from workspace (skipping `use` re-exports)
/// 2. Extracts entry points from api.json functions
/// 3. Resolves all types recursively using the workspace index
/// 4. Generates a deduplicated diff (path fixes, additions, removals, modifications)
/// 5. Generates patch files for review
pub fn autofix_api(
    api_data: &ApiData,
    project_root: &Path,
    output_dir: &Path,
    verbose: bool,
) -> Result<()> {
    use self::diff::{analyze_api_diff, ApiTypeInfo};
    use self::type_index::TypeIndex;
    
    let start_time = Instant::now();
    
    println!("\n{}\n", "Autofix: Starting analysis...".cyan().bold());

    // Run the full diff analysis
    let diff = analyze_api_diff(project_root, api_data, verbose)?;

    // Report results
    println!("\n{}", "Analysis Results:".white().bold());
    println!("  {} Path fixes needed: {}", "-".dimmed(), diff.path_fixes.len().to_string().yellow());
    println!("  {} Types to add: {}", "-".dimmed(), diff.additions.len().to_string().green());
    println!("  {} Types to remove: {}", "-".dimmed(), diff.removals.len().to_string().red());
    println!("  {} Type modifications: {}", "-".dimmed(), diff.modifications.len().to_string().blue());

    // Print path fixes
    if !diff.path_fixes.is_empty() {
        println!("\n{}", "Path Fixes:".yellow().bold());
        for fix in &diff.path_fixes {
            println!("  {} : {} {} {}", 
                fix.type_name.white(), 
                fix.old_path.red().strikethrough(),
                "->".dimmed(),
                fix.new_path.green()
            );
        }
    }

    // Print additions
    if !diff.additions.is_empty() {
        println!("\n{} (first 30 of {}):", "Additions".green().bold(), diff.additions.len());
        for addition in diff.additions.iter().take(30) {
            println!("  {} {} ({}) @ {}", 
                "+".green(), 
                addition.type_name.white(), 
                addition.kind.dimmed(), 
                addition.full_path.cyan()
            );
        }
        if diff.additions.len() > 30 {
            println!("  {} and {} more", "...".dimmed(), (diff.additions.len() - 30).to_string().yellow());
        }
    }

    // Print removals (first 30)
    if !diff.removals.is_empty() {
        println!("\n{} (first 30 of {}):", "Removals".red().bold(), diff.removals.len());
        for removal in diff.removals.iter().take(30) {
            println!("  {} {}", "-".red(), removal.red());
        }
        if diff.removals.len() > 30 {
            println!("  {} and {} more", "...".dimmed(), (diff.removals.len() - 30).to_string().yellow());
        }
    }
    
    // Print module moves
    if !diff.module_moves.is_empty() {
        println!("\n{} (first 30 of {}):", "Module Moves".magenta().bold(), diff.module_moves.len());
        for module_move in diff.module_moves.iter().take(30) {
            println!("  {} : {} {} {}", 
                module_move.type_name.white(),
                module_move.from_module.red(),
                "->".dimmed(),
                module_move.to_module.green()
            );
        }
        if diff.module_moves.len() > 30 {
            println!("  {} and {} more", "...".dimmed(), (diff.module_moves.len() - 30).to_string().yellow());
        }
    }

    // Print modifications (derive/impl changes)
    if !diff.modifications.is_empty() {
        println!("\n{} (first 30 of {}):", "Modifications".blue().bold(), diff.modifications.len());
        for modification in diff.modifications.iter().take(30) {
            match &modification.kind {
                diff::ModificationKind::DeriveAdded { derive_name } => {
                    println!("  {}: {} derive({})", modification.type_name.white(), "+".green(), derive_name.green());
                }
                diff::ModificationKind::DeriveRemoved { derive_name } => {
                    println!("  {}: {} derive({})", modification.type_name.white(), "-".red(), derive_name.red());
                }
                diff::ModificationKind::ReprCChanged { old_repr_c, new_repr_c } => {
                    println!("  {}: repr(C) {} {} {}", 
                        modification.type_name.white(), 
                        old_repr_c.to_string().red(),
                        "->".dimmed(),
                        new_repr_c.to_string().green()
                    );
                }
                diff::ModificationKind::FieldAdded { field_name, field_type } => {
                    println!("  {}: {} field {} : {}", 
                        modification.type_name.white(), 
                        "+".green(), 
                        field_name.green(), 
                        field_type.cyan()
                    );
                }
                diff::ModificationKind::FieldRemoved { field_name } => {
                    println!("  {}: {} field {}", 
                        modification.type_name.white(), 
                        "-".red(), 
                        field_name.red()
                    );
                }
                diff::ModificationKind::FieldTypeChanged { field_name, old_type, new_type } => {
                    println!("  {}: field {} : {} {} {}", 
                        modification.type_name.white(), 
                        field_name.white(),
                        old_type.red(),
                        "->".dimmed(),
                        new_type.green()
                    );
                }
                diff::ModificationKind::VariantAdded { variant_name } => {
                    println!("  {}: {} variant {}", 
                        modification.type_name.white(), 
                        "+".green(), 
                        variant_name.green()
                    );
                }
                diff::ModificationKind::VariantRemoved { variant_name } => {
                    println!("  {}: {} variant {}", 
                        modification.type_name.white(), 
                        "-".red(), 
                        variant_name.red()
                    );
                }
                diff::ModificationKind::VariantTypeChanged { variant_name, old_type, new_type } => {
                    println!("  {}: variant {} : {:?} {} {:?}", 
                        modification.type_name.white(), 
                        variant_name.white(),
                        old_type,
                        "->".dimmed(),
                        new_type
                    );
                }
            }
        }
        if diff.modifications.len() > 30 {
            println!("  {} and {} more", "...".dimmed(), (diff.modifications.len() - 30).to_string().yellow());
        }
    }

    // Generate patch files - group by type for compact output
    let patches_dir = output_dir.join("patches");
    fs::create_dir_all(&patches_dir)?;
    
    let mut patch_count = 0;

    // Group modifications by type name
    let mut mods_by_type: std::collections::HashMap<String, Vec<&diff::TypeModification>> = 
        std::collections::HashMap::new();
    for modification in &diff.modifications {
        mods_by_type.entry(modification.type_name.clone())
            .or_default()
            .push(modification);
    }
    
    // Find path fixes for types that also have modifications
    let mut path_fixes_by_type: std::collections::HashMap<String, &diff::PathFix> = 
        std::collections::HashMap::new();
    for fix in &diff.path_fixes {
        path_fixes_by_type.insert(fix.type_name.clone(), fix);
    }

    // Generate combined patches for types with both path fixes and modifications
    let mut handled_types: std::collections::HashSet<String> = std::collections::HashSet::new();
    
    for (type_name, mods) in &mods_by_type {
        let path_fix = path_fixes_by_type.get(type_name);
        let patch_content = generate_combined_patch(type_name, path_fix, mods);
        let patch_path = patches_dir.join(format!("{:04}_modify_{}.patch.json", patch_count, type_name));
        fs::write(&patch_path, &patch_content)?;
        patch_count += 1;
        handled_types.insert(type_name.clone());
    }
    
    // Generate standalone path fix patches for types without modifications
    for fix in &diff.path_fixes {
        if !handled_types.contains(&fix.type_name) {
            let patch_content = generate_path_fix_patch(fix);
            let patch_path = patches_dir.join(format!("{:04}_path_fix_{}.patch.json", patch_count, fix.type_name));
            fs::write(&patch_path, &patch_content)?;
            patch_count += 1;
        }
    }
    
    // Generate addition patches  
    for addition in &diff.additions {
        let patch_content = generate_addition_patch(addition);
        let patch_path = patches_dir.join(format!("{:04}_add_{}.patch.json", patch_count, addition.type_name));
        fs::write(&patch_path, &patch_content)?;
        patch_count += 1;
    }
    
    // Generate removal patches
    for removal in &diff.removals {
        let type_name = removal.split(':').next().unwrap_or(removal);
        let patch_content = generate_removal_patch(removal);
        let patch_path = patches_dir.join(format!("{:04}_remove_{}.patch.json", patch_count, type_name));
        fs::write(&patch_path, &patch_content)?;
        patch_count += 1;
    }
    
    // Generate module move patches
    for module_move in &diff.module_moves {
        let patch_content = generate_module_move_patch(module_move);
        let patch_path = patches_dir.join(format!("{:04}_move_{}.patch.json", patch_count, module_move.type_name));
        fs::write(&patch_path, &patch_content)?;
        patch_count += 1;
    }

    println!("\n{} {} patches in {}", 
        "Generated".green().bold(), 
        patch_count.to_string().white().bold(), 
        patches_dir.display().to_string().cyan()
    );
    
    let duration = start_time.elapsed();
    println!("{} ({:.2}s)\n", "Complete".green().bold(), duration.as_secs_f64());

    Ok(())
}

/// Generate a combined patch for a type with path fix and/or modifications
fn generate_combined_patch(
    type_name: &str, 
    path_fix: Option<&&diff::PathFix>,
    modifications: &[&diff::TypeModification]
) -> String {
    use patch_format::*;

    let mut patch = AutofixPatch::new(format!("Modify type {}", type_name));
    
    let mut changes = Vec::new();
    
    // Path change
    if let Some(fix) = path_fix {
        changes.push(ModifyChange::SetExternal {
            old: fix.old_path.clone(),
            new: fix.new_path.clone(),
        });
    }
    
    // Group modifications by kind
    let mut derives_added: Vec<String> = Vec::new();
    let mut derives_removed: Vec<String> = Vec::new();
    
    for m in modifications {
        match &m.kind {
            diff::ModificationKind::DeriveAdded { derive_name } => {
                derives_added.push(derive_name.clone());
            }
            diff::ModificationKind::DeriveRemoved { derive_name } => {
                derives_removed.push(derive_name.clone());
            }
            diff::ModificationKind::ReprCChanged { old_repr_c, new_repr_c } => {
                changes.push(ModifyChange::SetReprC {
                    old: *old_repr_c,
                    new: *new_repr_c,
                });
            }
            diff::ModificationKind::FieldAdded { field_name, field_type } => {
                changes.push(ModifyChange::AddField {
                    name: field_name.clone(),
                    field_type: field_type.clone(),
                    doc: None,
                });
            }
            diff::ModificationKind::FieldRemoved { field_name } => {
                changes.push(ModifyChange::RemoveField {
                    name: field_name.clone(),
                });
            }
            diff::ModificationKind::FieldTypeChanged { field_name, old_type, new_type } => {
                changes.push(ModifyChange::ChangeFieldType {
                    name: field_name.clone(),
                    old_type: old_type.clone(),
                    new_type: new_type.clone(),
                });
            }
            diff::ModificationKind::VariantAdded { variant_name } => {
                changes.push(ModifyChange::AddVariant {
                    name: variant_name.clone(),
                    variant_type: None,
                });
            }
            diff::ModificationKind::VariantRemoved { variant_name } => {
                changes.push(ModifyChange::RemoveVariant {
                    name: variant_name.clone(),
                });
            }
            diff::ModificationKind::VariantTypeChanged { variant_name, old_type, new_type } => {
                changes.push(ModifyChange::ChangeVariantType {
                    name: variant_name.clone(),
                    old_type: old_type.clone(),
                    new_type: new_type.clone(),
                });
            }
        }
    }
    
    // Add grouped derives
    if !derives_added.is_empty() {
        changes.push(ModifyChange::AddDerives { derives: derives_added });
    }
    if !derives_removed.is_empty() {
        changes.push(ModifyChange::RemoveDerives { derives: derives_removed });
    }
    
    patch.add_operation(PatchOperation::Modify(ModifyOperation {
        type_name: type_name.to_string(),
        module: None,
        changes,
    }));
    
    patch.to_json().unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}

/// Generate a patch for a path fix
fn generate_path_fix_patch(fix: &diff::PathFix) -> String {
    use patch_format::*;

    let mut patch = AutofixPatch::new(format!("Fix external path for {}", fix.type_name));
    patch.add_operation(PatchOperation::PathFix(PathFixOperation {
        type_name: fix.type_name.clone(),
        old_path: fix.old_path.clone(),
        new_path: fix.new_path.clone(),
    }));
    
    patch.to_json().unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}

/// Generate a patch for adding a new type
fn generate_addition_patch(addition: &diff::TypeAddition) -> String {
    use patch_format::*;

    let kind = match addition.kind.as_str() {
        "struct" => TypeKind::Struct,
        "enum" => TypeKind::Enum,
        "type_alias" => TypeKind::TypeAlias,
        "callback" => TypeKind::Callback,
        "callback_value" => TypeKind::CallbackValue,
        _ => TypeKind::Struct,
    };

    let mut patch = AutofixPatch::new(format!("Add new type {}", addition.type_name));
    patch.add_operation(PatchOperation::Add(AddOperation {
        type_name: addition.type_name.clone(),
        external: addition.full_path.clone(),
        kind,
        module: None,
        derives: None,
        repr_c: None,
        struct_fields: None,
        enum_variants: None,
    }));
    
    patch.to_json().unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}

/// Generate a patch for removing a type
fn generate_removal_patch(removal: &str) -> String {
    use patch_format::*;

    let parts: Vec<&str> = removal.splitn(2, ':').collect();
    let type_name = parts.get(0).unwrap_or(&removal);
    let path = parts.get(1);
    
    let mut patch = AutofixPatch::new(format!("Remove unused type {}", type_name));
    patch.add_operation(PatchOperation::Remove(RemoveOperation {
        type_name: type_name.to_string(),
        path: path.map(|p| p.trim().to_string()),
        reason: Some("Not reachable from public API".to_string()),
    }));
    
    patch.to_json().unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}

/// Generate a patch for moving a type to a different module
fn generate_module_move_patch(module_move: &diff::ModuleMove) -> String {
    use patch_format::*;

    let mut patch = AutofixPatch::new(format!(
        "Move {} from '{}' to '{}' module",
        module_move.type_name, module_move.from_module, module_move.to_module
    ));
    patch.add_operation(PatchOperation::MoveModule(MoveModuleOperation {
        type_name: module_move.type_name.clone(),
        from_module: module_move.from_module.clone(),
        to_module: module_move.to_module.clone(),
    }));
    
    patch.to_json().unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}