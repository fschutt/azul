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
pub mod function_diff;
pub mod debug;
pub mod patch_format;
pub mod module_map;
pub mod unified_index;

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

    // Run the full diff analysis (returns both diff and type index)
    let (diff, index) = analyze_api_diff(project_root, api_data, verbose)?;
    
    // Check FFI safety only for types that exist in api.json
    let ffi_warnings = check_ffi_safety(&index, api_data);
    print_ffi_safety_warnings(&ffi_warnings);
    
    // Count only critical errors (not informational warnings)
    let critical_errors: Vec<_> = ffi_warnings.iter().filter(|w| w.is_critical()).collect();
    
    // Fail if there are any critical FFI safety issues
    if !critical_errors.is_empty() {
        return Err(anyhow::anyhow!(
            "Found {} critical FFI safety issues in API types. Fix them before proceeding.",
            critical_errors.len()
        ));
    }
    
    // Print info about non-critical warnings
    let info_warnings = ffi_warnings.len() - critical_errors.len();
    if info_warnings > 0 {
        println!("\n{} {} informational warnings (non-blocking)", "ℹ".blue(), info_warnings);
    }

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
                diff::ModificationKind::CustomImplAdded { impl_name } => {
                    println!("  {}: {} custom_impl({})", modification.type_name.white(), "+".green(), impl_name.green());
                }
                diff::ModificationKind::CustomImplRemoved { impl_name } => {
                    println!("  {}: {} custom_impl({})", modification.type_name.white(), "-".red(), impl_name.red());
                }
                diff::ModificationKind::ReprChanged { old_repr, new_repr } => {
                    let old_display = old_repr.as_deref().unwrap_or("none");
                    let new_display = new_repr.as_deref().unwrap_or("none");
                    println!("  {}: repr {} {} {}", 
                        modification.type_name.white(), 
                        old_display.red(),
                        "->".dimmed(),
                        new_display.green()
                    );
                }
                diff::ModificationKind::FieldAdded { field_name, field_type, ref_kind } => {
                    let type_display = if ref_kind.is_default() {
                        field_type.clone()
                    } else {
                        format!("{}{}{}", ref_kind.to_rust_prefix(), field_type, ref_kind.to_rust_suffix())
                    };
                    println!("  {}: {} field {} : {}", 
                        modification.type_name.white(), 
                        "+".green(), 
                        field_name.green(), 
                        type_display.cyan()
                    );
                }
                diff::ModificationKind::FieldRemoved { field_name } => {
                    println!("  {}: {} field {}", 
                        modification.type_name.white(), 
                        "-".red(), 
                        field_name.red()
                    );
                }
                diff::ModificationKind::FieldTypeChanged { field_name, old_type, new_type, ref_kind } => {
                    let new_type_display = if ref_kind.is_default() {
                        new_type.clone()
                    } else {
                        format!("{}{}{}", ref_kind.to_rust_prefix(), new_type, ref_kind.to_rust_suffix())
                    };
                    println!("  {}: field {} : {} {} {}", 
                        modification.type_name.white(), 
                        field_name.white(),
                        old_type.red(),
                        "->".dimmed(),
                        new_type_display.green()
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
                diff::ModificationKind::CallbackTypedefAdded { args, returns } => {
                    let args_str: Vec<String> = args.iter()
                        .map(|arg| {
                            let (ref_str, suffix) = match arg.ref_kind {
                                type_index::RefKind::Ref => ("&", ""),
                                type_index::RefKind::RefMut => ("&mut ", ""),
                                type_index::RefKind::ConstPtr => ("*const ", ""),
                                type_index::RefKind::MutPtr => ("*mut ", ""),
                                type_index::RefKind::Boxed => ("Box<", ">"),
                                type_index::RefKind::OptionBoxed => ("Option<Box<", ">>"),
                                type_index::RefKind::Value => ("", ""),
                            };
                            format!("{}{}{}", ref_str, arg.ty, suffix)
                        })
                        .collect();
                    println!("  {}: {} callback_typedef({}) -> {:?}", 
                        modification.type_name.white(), 
                        "+".green(),
                        args_str.join(", ").cyan(),
                        returns
                    );
                }
                diff::ModificationKind::CallbackArgChanged { arg_index, old_type, new_type, old_ref_kind, new_ref_kind } => {
                    let (old_ref_str, old_suffix) = match old_ref_kind {
                        Some(type_index::RefKind::Ref) => ("&", ""),
                        Some(type_index::RefKind::RefMut) => ("&mut ", ""),
                        Some(type_index::RefKind::ConstPtr) => ("*const ", ""),
                        Some(type_index::RefKind::MutPtr) => ("*mut ", ""),
                        Some(type_index::RefKind::Boxed) => ("Box<", ">"),
                        Some(type_index::RefKind::OptionBoxed) => ("Option<Box<", ">>"),
                        Some(type_index::RefKind::Value) | None => ("", ""),
                    };
                    let (new_ref_str, new_suffix) = match new_ref_kind {
                        type_index::RefKind::Ref => ("&", ""),
                        type_index::RefKind::RefMut => ("&mut ", ""),
                        type_index::RefKind::ConstPtr => ("*const ", ""),
                        type_index::RefKind::MutPtr => ("*mut ", ""),
                        type_index::RefKind::Boxed => ("Box<", ">"),
                        type_index::RefKind::OptionBoxed => ("Option<Box<", ">>"),
                        type_index::RefKind::Value => ("", ""),
                    };
                    println!("  {}: callback arg[{}] : {}{}{} {} {}{}{}", 
                        modification.type_name.white(), 
                        arg_index,
                        old_ref_str.red(),
                        old_type.red(),
                        old_suffix.red(),
                        "->".dimmed(),
                        new_ref_str.green(),
                        new_type.green(),
                        new_suffix.green()
                    );
                }
                diff::ModificationKind::CallbackReturnChanged { old_type, new_type } => {
                    println!("  {}: callback return : {:?} {} {:?}", 
                        modification.type_name.white(), 
                        old_type,
                        "->".dimmed(),
                        new_type
                    );
                }
                diff::ModificationKind::TypeAliasAdded { target, generic_args } => {
                    let display_target = if generic_args.is_empty() {
                        target.clone()
                    } else {
                        format!("{}<{}>", target, generic_args.join(", "))
                    };
                    println!("  {}: {} type_alias = {}", 
                        modification.type_name.white(), 
                        "+".green(),
                        display_target.cyan()
                    );
                }
                diff::ModificationKind::TypeAliasTargetChanged { old_target, new_target, new_generic_args } => {
                    let display_new = if new_generic_args.is_empty() {
                        new_target.clone()
                    } else {
                        format!("{}<{}>", new_target, new_generic_args.join(", "))
                    };
                    println!("  {}: type_alias = {} {} {}", 
                        modification.type_name.white(), 
                        old_target.red(),
                        "->".dimmed(),
                        display_new.green()
                    );
                }
                diff::ModificationKind::GenericParamsChanged { old_params, new_params } => {
                    let old_display = if old_params.is_empty() {
                        "none".to_string()
                    } else {
                        format!("<{}>", old_params.join(", "))
                    };
                    let new_display = if new_params.is_empty() {
                        "none".to_string()
                    } else {
                        format!("<{}>", new_params.join(", "))
                    };
                    println!("  {}: generic_params {} {} {}", 
                        modification.type_name.white(), 
                        old_display.red(),
                        "->".dimmed(),
                        new_display.green()
                    );
                }
                diff::ModificationKind::StructFieldsReplaced { fields } => {
                    let field_names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                    println!("  {}: {} struct_fields [{}]", 
                        modification.type_name.white(), 
                        "=".cyan(),
                        field_names.join(", ").cyan()
                    );
                }
                diff::ModificationKind::EnumVariantsReplaced { variants } => {
                    let variant_names: Vec<&str> = variants.iter().map(|v| v.name.as_str()).collect();
                    println!("  {}: {} enum_variants [{}]", 
                        modification.type_name.white(), 
                        "=".cyan(),
                        variant_names.join(", ").cyan()
                    );
                }
                diff::ModificationKind::StructFieldsRemoved => {
                    println!("  {}: {} struct_fields (type changed to enum)", 
                        modification.type_name.white(), 
                        "-".red()
                    );
                }
                diff::ModificationKind::EnumFieldsRemoved => {
                    println!("  {}: {} enum_fields (type changed to struct)", 
                        modification.type_name.white(), 
                        "-".red()
                    );
                }
                diff::ModificationKind::FunctionSelfMismatch { fn_name, expected_self, actual_self } => {
                    let expected = expected_self.as_deref().unwrap_or("static");
                    let actual = actual_self.as_deref().unwrap_or("static");
                    println!("  {}.{}: self mismatch - expected '{}', got '{}'", 
                        modification.type_name.white(), 
                        fn_name.yellow(),
                        expected.green(),
                        actual.red()
                    );
                }
                diff::ModificationKind::FunctionArgCountMismatch { fn_name, expected_count, actual_count } => {
                    println!("  {}.{}: arg count mismatch - expected {}, got {}", 
                        modification.type_name.white(), 
                        fn_name.yellow(),
                        expected_count.to_string().green(),
                        actual_count.to_string().red()
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
    
    // Clear old patches before generating new ones
    if patches_dir.exists() {
        fs::remove_dir_all(&patches_dir)?;
    }
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
    let mut custom_impls_added: Vec<String> = Vec::new();
    let mut custom_impls_removed: Vec<String> = Vec::new();
    
    for m in modifications {
        match &m.kind {
            diff::ModificationKind::DeriveAdded { derive_name } => {
                derives_added.push(derive_name.clone());
            }
            diff::ModificationKind::DeriveRemoved { derive_name } => {
                derives_removed.push(derive_name.clone());
            }
            diff::ModificationKind::CustomImplAdded { impl_name } => {
                custom_impls_added.push(impl_name.clone());
            }
            diff::ModificationKind::CustomImplRemoved { impl_name } => {
                custom_impls_removed.push(impl_name.clone());
            }
            diff::ModificationKind::ReprChanged { old_repr, new_repr } => {
                changes.push(ModifyChange::SetRepr {
                    old: old_repr.clone(),
                    new: new_repr.clone(),
                });
            }
            diff::ModificationKind::StructFieldsReplaced { fields } => {
                // Convert to complete field replacement (preserves order for repr(C))
                let field_defs: Vec<patch_format::StructFieldDef> = fields.iter()
                    .map(|f| patch_format::StructFieldDef {
                        name: f.name.clone(),
                        field_type: f.ty.clone(),
                        ref_kind: if f.ref_kind.is_default() {
                            None
                        } else {
                            Some(f.ref_kind.to_string())
                        },
                    })
                    .collect();
                changes.push(ModifyChange::ReplaceStructFields { fields: field_defs });
            }
            diff::ModificationKind::EnumVariantsReplaced { variants } => {
                // Convert to complete variant replacement (preserves order)
                let variant_defs: Vec<patch_format::EnumVariantDef> = variants.iter()
                    .map(|v| patch_format::EnumVariantDef {
                        name: v.name.clone(),
                        variant_type: v.ty.clone(),
                    })
                    .collect();
                changes.push(ModifyChange::ReplaceEnumVariants { variants: variant_defs });
            }
            diff::ModificationKind::StructFieldsRemoved => {
                // Type changed from struct to enum - remove struct_fields
                changes.push(ModifyChange::RemoveStructFields);
            }
            diff::ModificationKind::EnumFieldsRemoved => {
                // Type changed from enum to struct - remove enum_fields
                changes.push(ModifyChange::RemoveEnumFields);
            }
            // Legacy individual field operations (kept for backwards compatibility)
            diff::ModificationKind::FieldAdded { field_name, field_type, ref_kind } => {
                let ref_kind_str = if ref_kind.is_default() {
                    None
                } else {
                    Some(ref_kind.to_string())
                };
                changes.push(ModifyChange::AddField {
                    name: field_name.clone(),
                    field_type: field_type.clone(),
                    ref_kind: ref_kind_str,
                    doc: None,
                });
            }
            diff::ModificationKind::FieldRemoved { field_name } => {
                changes.push(ModifyChange::RemoveField {
                    name: field_name.clone(),
                });
            }
            diff::ModificationKind::FieldTypeChanged { field_name, old_type, new_type, ref_kind } => {
                let ref_kind_str = if ref_kind.is_default() {
                    None
                } else {
                    Some(ref_kind.to_string())
                };
                changes.push(ModifyChange::ChangeFieldType {
                    name: field_name.clone(),
                    old_type: old_type.clone(),
                    new_type: new_type.clone(),
                    ref_kind: ref_kind_str,
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
            diff::ModificationKind::CallbackTypedefAdded { args, returns } => {
                changes.push(ModifyChange::SetCallbackTypedef {
                    args: args.iter().map(|arg| {
                        patch_format::CallbackArgDef {
                            arg_type: arg.ty.clone(),
                            ref_kind: arg.ref_kind.clone(),
                            name: arg.name.clone(),
                        }
                    }).collect(),
                    returns: returns.clone(),
                });
            }
            diff::ModificationKind::CallbackArgChanged { arg_index, old_type, new_type, old_ref_kind, new_ref_kind } => {
                changes.push(ModifyChange::ChangeCallbackArg {
                    arg_index: *arg_index,
                    old_type: old_type.clone(),
                    new_type: new_type.clone(),
                    old_ref: old_ref_kind.clone(),
                    new_ref: new_ref_kind.clone(),
                });
            }
            diff::ModificationKind::CallbackReturnChanged { old_type, new_type } => {
                changes.push(ModifyChange::ChangeCallbackReturn {
                    old_type: old_type.clone(),
                    new_type: new_type.clone(),
                });
            }
            diff::ModificationKind::TypeAliasAdded { target, generic_args } => {
                changes.push(ModifyChange::SetTypeAlias {
                    target: target.clone(),
                    generic_args: generic_args.clone(),
                });
            }
            diff::ModificationKind::TypeAliasTargetChanged { old_target, new_target, new_generic_args } => {
                changes.push(ModifyChange::ChangeTypeAlias {
                    old_target: old_target.clone(),
                    new_target: new_target.clone(),
                    new_generic_args: new_generic_args.clone(),
                });
            }
            diff::ModificationKind::GenericParamsChanged { old_params, new_params } => {
                changes.push(ModifyChange::SetGenericParams {
                    old_params: old_params.clone(),
                    new_params: new_params.clone(),
                });
            }
            diff::ModificationKind::FunctionSelfMismatch { fn_name, expected_self, actual_self: _ } => {
                changes.push(ModifyChange::FixFunctionSelf {
                    fn_name: fn_name.clone(),
                    expected_self: expected_self.clone(),
                });
            }
            diff::ModificationKind::FunctionArgCountMismatch { fn_name, expected_count, actual_count: _ } => {
                changes.push(ModifyChange::FixFunctionArgs {
                    fn_name: fn_name.clone(),
                    expected_count: *expected_count,
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
    
    // Add grouped custom_impls
    if !custom_impls_added.is_empty() {
        changes.push(ModifyChange::AddCustomImpls { impls: custom_impls_added });
    }
    if !custom_impls_removed.is_empty() {
        changes.push(ModifyChange::RemoveCustomImpls { impls: custom_impls_removed });
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
        "callback_typedef" => TypeKind::CallbackTypedef,
        "callback_value" => TypeKind::CallbackValue,
        _ => TypeKind::Struct,
    };
    
    // Convert struct_fields to FieldDef format
    let struct_fields = addition.struct_fields.as_ref().map(|fields| {
        fields.iter().map(|(name, ty, ref_kind)| patch_format::FieldDef {
            name: name.clone(),
            field_type: ty.clone(),
            ref_kind: if ref_kind == "value" { None } else { Some(ref_kind.clone()) },
            doc: None,
        }).collect()
    });
    
    // Convert enum_variants to VariantDef format
    let enum_variants = addition.enum_variants.as_ref().map(|variants| {
        variants.iter().map(|(name, ty)| patch_format::VariantDef {
            name: name.clone(),
            variant_type: ty.clone(),
        }).collect()
    });
    
    let derives = if addition.derives.is_empty() { 
        None 
    } else { 
        Some(addition.derives.clone()) 
    };
    
    // Convert callback_typedef info to CallbackTypedefDef
    let callback_typedef = addition.callback_typedef.as_ref().map(|info| {
        let fn_args: Vec<CallbackArg> = info.fn_args.iter().map(|(ty, ref_kind)| {
            CallbackArg {
                arg_type: ty.clone(),
                ref_kind: if ref_kind == "value" { None } else { Some(ref_kind.clone()) },
            }
        }).collect();
        
        let returns = info.returns.as_ref().map(|ret| {
            CallbackReturn {
                return_type: ret.clone(),
                ref_kind: None,
            }
        });
        
        CallbackTypedefDef {
            fn_args,
            returns,
        }
    });

    let mut patch = AutofixPatch::new(format!("Add new type {}", addition.type_name));
    patch.add_operation(PatchOperation::Add(AddOperation {
        type_name: addition.type_name.clone(),
        external: addition.full_path.clone(),
        kind,
        module: None,
        derives,
        repr_c: Some(true), // All API types should have repr(C)
        struct_fields,
        enum_variants,
        callback_typedef,
        type_alias: None,
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

// ============================================================================
// FFI SAFETY CHECKS
// ============================================================================

/// An FFI safety warning for a type
#[derive(Debug)]
pub struct FfiSafetyWarning {
    pub type_name: String,
    pub file_path: String,
    pub kind: FfiSafetyWarningKind,
}

impl FfiSafetyWarning {
    /// Returns true if this is a critical error that should fail the build
    /// Returns false if this is just a warning/info message
    pub fn is_critical(&self) -> bool {
        self.kind.is_critical()
    }
}

/// The kind of FFI safety issue
#[derive(Debug)]
pub enum FfiSafetyWarningKind {
    /// Struct is missing repr(C) attribute
    StructMissingReprC {
        current_repr: Option<String>,
    },
    /// Enum variant has more than one field (not FFI-safe)
    MultiFieldVariant {
        variant_name: String,
        field_count: usize,
        field_types: String,
    },
    /// Enum variant uses std::Option (should use custom OptionXxx type)
    StdOptionInVariant {
        variant_name: String,
        option_type: String,
    },
    /// Enum with data variants but missing repr(C, u8) or similar
    EnumWithDataMissingReprCU8 {
        current_repr: Option<String>,
    },
    /// Enum uses non-C repr like repr(u16) with explicit discriminant values
    /// This breaks FFI because discriminant values aren't portable
    NonCReprEnum {
        current_repr: String,
    },
    /// Type name is duplicated in multiple files
    DuplicateTypeName {
        other_files: Vec<String>,
    },
    /// Field uses Box<c_void> which is undefined behavior in Rust
    /// Box requires a valid sized type, c_void is unsized
    BoxCVoidField {
        field_name: String,
    },
    /// Field uses an array type like [u8; 4] that may not be handled by C codegen
    ArrayTypeField {
        field_name: String,
        array_type: String,
    },
}

impl FfiSafetyWarningKind {
    /// Returns true if this is a critical error that should fail the build
    /// ArrayTypeField is just informational - it's now handled correctly by codegen
    pub fn is_critical(&self) -> bool {
        match self {
            // Critical errors that must be fixed
            FfiSafetyWarningKind::StructMissingReprC { .. } => true,
            FfiSafetyWarningKind::MultiFieldVariant { .. } => true,
            FfiSafetyWarningKind::StdOptionInVariant { .. } => true,
            FfiSafetyWarningKind::EnumWithDataMissingReprCU8 { .. } => true,
            FfiSafetyWarningKind::NonCReprEnum { .. } => true,
            FfiSafetyWarningKind::DuplicateTypeName { .. } => true,
            FfiSafetyWarningKind::BoxCVoidField { .. } => true,
            // Informational only - array types are now handled correctly
            FfiSafetyWarningKind::ArrayTypeField { .. } => false,
        }
    }
}

/// Check FFI safety of types that exist in api.json
pub fn check_ffi_safety(index: &type_index::TypeIndex, api_data: &ApiData) -> Vec<FfiSafetyWarning> {
    use std::collections::HashSet;
    use std::collections::HashMap;
    
    // Build a set of type names that exist in api.json
    let api_types: HashSet<String> = api_data.0.values()
        .flat_map(|version| version.api.values())
        .flat_map(|module| module.classes.keys())
        .cloned()
        .collect();
    
    let mut warnings = Vec::new();
    
    // Check for duplicate type names first
    // Build a map of type_name -> list of (file path, is_generic)
    let mut type_locations: HashMap<String, Vec<(String, bool)>> = HashMap::new();
    for (type_name, defs) in index.iter_all() {
        if !api_types.contains(type_name) {
            continue;
        }
        for typedef in defs {
            let file_path = typedef.file_path.display().to_string();
            let is_generic = match &typedef.kind {
                type_index::TypeDefKind::Struct { generic_params, .. } => !generic_params.is_empty(),
                type_index::TypeDefKind::Enum { generic_params, .. } => !generic_params.is_empty(),
                _ => false,
            };
            type_locations
                .entry(type_name.clone())
                .or_default()
                .push((file_path, is_generic));
        }
    }
    
    // Report duplicates (only if BOTH are non-generic or BOTH are generic)
    for (type_name, files) in &type_locations {
        // Filter to only non-generic types for duplicate check
        let non_generic_files: Vec<&String> = files.iter()
            .filter(|(_, is_generic)| !*is_generic)
            .map(|(path, _)| path)
            .collect();
        
        if non_generic_files.len() > 1 {
            // Report for each non-generic file
            for file in &non_generic_files {
                let other_files: Vec<String> = non_generic_files.iter()
                    .filter(|f| *f != file)
                    .map(|f| (*f).clone())
                    .collect();
                warnings.push(FfiSafetyWarning {
                    type_name: type_name.clone(),
                    file_path: (*file).clone(),
                    kind: FfiSafetyWarningKind::DuplicateTypeName {
                        other_files,
                    },
                });
            }
        }
    }
    
    // Iterate over all types, but only check those in api.json
    for (type_name, defs) in index.iter_all() {
        // Skip types that aren't in api.json
        if !api_types.contains(type_name) {
            continue;
        }
        
        for typedef in defs {
            let file_path = typedef.file_path.display().to_string();
            
            // Check structs for repr(C) and field issues
            if let type_index::TypeDefKind::Struct { repr, fields, .. } = &typedef.kind {
                // Structs in api.json must have repr(C)
                let has_repr_c = match repr {
                    Some(r) => r.to_lowercase().contains("c"),
                    None => false,
                };
                
                if !has_repr_c {
                    warnings.push(FfiSafetyWarning {
                        type_name: type_name.clone(),
                        file_path: file_path.clone(),
                        kind: FfiSafetyWarningKind::StructMissingReprC {
                            current_repr: repr.clone(),
                        },
                    });
                }
                
                // Check each field for FFI issues
                for (field_name, field_def) in fields {
                    // Check 1: Box<c_void> is UB - Box requires sized type
                    if field_def.ty == "c_void" && field_def.ref_kind == type_index::RefKind::Boxed {
                        warnings.push(FfiSafetyWarning {
                            type_name: type_name.clone(),
                            file_path: file_path.clone(),
                            kind: FfiSafetyWarningKind::BoxCVoidField {
                                field_name: field_name.clone(),
                            },
                        });
                    }
                    
                    // Check 2: Array types like [u8; 4] - warn if not handled
                    if field_def.ty.starts_with('[') && field_def.ty.contains(';') {
                        warnings.push(FfiSafetyWarning {
                            type_name: type_name.clone(),
                            file_path: file_path.clone(),
                            kind: FfiSafetyWarningKind::ArrayTypeField {
                                field_name: field_name.clone(),
                                array_type: field_def.ty.clone(),
                            },
                        });
                    }
                }
            }
            
            // Check enums
            if let type_index::TypeDefKind::Enum { variants, repr, .. } = &typedef.kind {
                
                // Check if any variant has data
                let has_data_variants = variants.values().any(|v| v.ty.is_some());
                
                for (variant_name, variant_def) in variants {
                    if let Some(ref ty) = variant_def.ty {
                        // Check 1: Multiple fields per variant (contains comma)
                        if ty.contains(',') {
                            let field_count = ty.split(',').count();
                            warnings.push(FfiSafetyWarning {
                                type_name: type_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::MultiFieldVariant {
                                    variant_name: variant_name.clone(),
                                    field_count,
                                    field_types: ty.clone(),
                                },
                            });
                        }
                        
                        // Check 2: std::Option usage
                        if ty.starts_with("Option<") || ty.starts_with("Option <") || ty == "Option" {
                            warnings.push(FfiSafetyWarning {
                                type_name: type_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::StdOptionInVariant {
                                    variant_name: variant_name.clone(),
                                    option_type: ty.clone(),
                                },
                            });
                        }
                    }
                }
                
                // Check 3: Enum with data needs repr(C, u8) or repr(C, i8) etc.
                if has_data_variants {
                    let repr_ok = match repr {
                        Some(r) => {
                            // Must have both C and a discriminant type
                            let r_lower = r.to_lowercase();
                            r_lower.contains("c") && (
                                r_lower.contains("u8") || 
                                r_lower.contains("u16") || 
                                r_lower.contains("u32") || 
                                r_lower.contains("i8") || 
                                r_lower.contains("i16") || 
                                r_lower.contains("i32")
                            )
                        }
                        None => false,
                    };
                    
                    if !repr_ok {
                        warnings.push(FfiSafetyWarning {
                            type_name: type_name.clone(),
                            file_path: file_path.clone(),
                            kind: FfiSafetyWarningKind::EnumWithDataMissingReprCU8 {
                                current_repr: repr.clone(),
                            },
                        });
                    }
                }
                
                // Check 4: Enums with repr(u16), repr(u8) without C are dangerous
                // because explicit discriminant values (= 100, = 200) are not FFI-portable
                if let Some(r) = repr {
                    let r_lower = r.to_lowercase().replace(" ", "");
                    // Check for bare repr(u16), repr(u8), repr(i32) etc. without C
                    let is_bare_int_repr = (r_lower == "u8" || r_lower == "u16" || r_lower == "u32" ||
                                            r_lower == "i8" || r_lower == "i16" || r_lower == "i32") &&
                                           !r_lower.contains("c");
                    if is_bare_int_repr {
                        warnings.push(FfiSafetyWarning {
                            type_name: type_name.clone(),
                            file_path: file_path.clone(),
                            kind: FfiSafetyWarningKind::NonCReprEnum {
                                current_repr: r.clone(),
                            },
                        });
                    }
                }
            }
        }
    }
    
    warnings
}

/// Print FFI safety warnings
pub fn print_ffi_safety_warnings(warnings: &[FfiSafetyWarning]) {
    if warnings.is_empty() {
        return;
    }
    
    println!("\n{} {} FFI safety issues:", 
        "⚠️  WARNING:".yellow().bold(),
        warnings.len().to_string().red().bold()
    );
    
    for warning in warnings {
        match &warning.kind {
            FfiSafetyWarningKind::StructMissingReprC { current_repr } => {
                let repr_display = current_repr.as_deref().unwrap_or("none");
                println!("  {} {}", "✗".red(), warning.type_name.white());
                println!("    {} Struct has repr: {}", "→".dimmed(), repr_display.yellow());
                println!("    {} Add #[repr(C)] for FFI-safe struct layout.", "FIX:".cyan());
                println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
            }
            FfiSafetyWarningKind::MultiFieldVariant { variant_name, field_count, field_types } => {
                println!("  {} {}", "✗".red(), format!("{}::{}", warning.type_name, variant_name).white());
                println!("    {} Enum variant has {} fields: {}", 
                    "→".dimmed(), 
                    field_count.to_string().red(), 
                    field_types.yellow()
                );
                println!("    {} FFI requires exactly ONE field per variant. Wrap in a struct.", "FIX:".cyan());
                println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
            }
            FfiSafetyWarningKind::StdOptionInVariant { variant_name, option_type } => {
                println!("  {} {}", "✗".red(), format!("{}::{}", warning.type_name, variant_name).white());
                println!("    {} Uses std::Option: {}", "→".dimmed(), option_type.yellow());
                println!("    {} Use custom OptionXxx type from impl_option! macro instead.", "FIX:".cyan());
                println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
            }
            FfiSafetyWarningKind::EnumWithDataMissingReprCU8 { current_repr } => {
                let repr_display = current_repr.as_deref().unwrap_or("none");
                println!("  {} {}", "✗".red(), warning.type_name.white());
                println!("    {} Enum with data has repr: {}", "→".dimmed(), repr_display.yellow());
                println!("    {} Add #[repr(C, u8)] for enums with data variants.", "FIX:".cyan());
                println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
            }
            FfiSafetyWarningKind::NonCReprEnum { current_repr } => {
                println!("  {} {}", "✗".red(), warning.type_name.white());
                println!("    {} Enum uses non-C repr: #[repr({})]", "→".dimmed(), current_repr.yellow());
                println!("    {} Explicit discriminant values (= 100, = 200) are not FFI-safe.", "REASON:".magenta());
                println!("    {} Use #[repr(C)] for enums without data, remove explicit discriminant values.", "FIX:".cyan());
                println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
            }
            FfiSafetyWarningKind::DuplicateTypeName { other_files } => {
                println!("  {} {}", "✗".red(), warning.type_name.white());
                println!("    {} Type name is duplicated in multiple files!", "→".dimmed());
                println!("    {} Also defined in:", "ALSO:".magenta());
                for other in other_files {
                    println!("       - {}", other.yellow());
                }
                println!("    {} Rename one of the types to avoid name collision in C API.", "FIX:".cyan());
                println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
            }
            FfiSafetyWarningKind::BoxCVoidField { field_name } => {
                println!("  {} {}", "✗".red(), warning.type_name.white());
                println!("    {} Field '{}' uses Box<c_void> which is undefined behavior!", 
                    "→".dimmed(), field_name.yellow());
                println!("    {} Box<T> requires T to be Sized, but c_void is not.", "REASON:".magenta());
                println!("    {} Use *mut c_void instead of Box<c_void>.", "FIX:".cyan());
                println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
            }
            FfiSafetyWarningKind::ArrayTypeField { field_name, array_type } => {
                println!("  {} {}", "⚠".yellow(), warning.type_name.white());
                println!("    {} Field '{}' uses array type: {}", 
                    "→".dimmed(), field_name.yellow(), array_type.cyan());
                println!("    {} Array types require special handling in C codegen.", "NOTE:".magenta());
                println!("    {} Verify that extract_array_from_type() handles this type.", "CHECK:".cyan());
                println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
            }
        }
        println!();
    }
}