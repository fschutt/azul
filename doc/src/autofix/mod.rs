use std::{
    fs,
    path::Path,
    time::Instant,
};

use anyhow::Result;
use colored::Colorize;

use crate::api::ApiData;

// V2 architecture modules - actively used
pub mod type_index;
pub mod type_resolver;
pub mod diff;
pub mod debug;
pub mod patch_format;
pub mod module_map;

// Legacy modules - still needed for some utilities
pub mod message;
pub mod utils;
pub mod workspace;

// Legacy modular architecture - kept for potential future use
pub mod types;
pub mod discovery;
pub mod analysis;
pub mod patches;
pub mod output;

/// Check if a type should be ignored in "Could not find type" warnings
pub fn should_suppress_type_not_found(type_name: &str) -> bool {
    const PRIMITIVES: &[&str] = &[
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char",
    ];
    const STD_TYPES: &[&str] = &[
        "String", "str", "Vec", "Option", "Result", "Box", "Rc", "Arc", "RefCell", "Cell",
    ];
    const INTERNAL_ALIASES: &[&str] = &[
        "BoxCssPropertyCache", "Widows", "Orphans", "OptionStyledDom", "OptionCoreMenuCallback",
        "OptionLinuxDecorationsState", "OptionComputedScrollbarStyle", "OptionPixelValue",
        "SystemClipboard", "AzDuration",
    ];

    let trimmed = type_name.trim();
    PRIMITIVES.contains(&trimmed)
        || STD_TYPES.contains(&trimmed)
        || INTERNAL_ALIASES.contains(&trimmed)
        || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        || (trimmed.contains("extern") && trimmed.contains("fn"))
        || (trimmed.contains("fn") && (trimmed.contains("->") || trimmed.contains("c_void")))
        || (trimmed.contains(' ') && (trimmed.contains('.') || trimmed.len() > 50))
        || trimmed.contains("::")
        || trimmed.contains(',')
        || trimmed.contains('<')
        || trimmed.contains('>')
        || (trimmed.len() < 10 && trimmed.chars().all(|c| c.is_lowercase() || c == '_'))
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