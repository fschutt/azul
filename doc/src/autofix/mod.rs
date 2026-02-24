use std::{fs, path::Path, time::Instant};

use anyhow::Result;
use colored::Colorize;

use crate::api::ApiData;

// V2 architecture modules - actively used
pub mod debug;
pub mod diff;
pub mod function_diff;
pub mod module_map;
pub mod patch_format;
pub mod type_index;
pub mod type_resolver;
pub mod unified_index;

// Legacy modules - still needed for some utilities
pub mod message;
pub mod utils;
pub mod workspace;

// Legacy modular architecture - kept for potential future use
pub mod analysis;
pub mod discovery;
pub mod output;
pub mod patches;
pub mod types;

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

// main entry point
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
    use self::{
        diff::{analyze_api_diff, ApiTypeInfo},
        type_index::TypeIndex,
    };

    let start_time = Instant::now();

    println!("\n{}\n", "Autofix: Starting analysis...".cyan().bold());

    // Run the full diff analysis (returns both diff and type index)
    let (diff, index) = analyze_api_diff(project_root, api_data, verbose)?;

    // Check FFI safety for types that exist in api.json AND types about to be added
    let addition_names: Vec<String> = diff.additions.iter().map(|a| a.type_name.clone()).collect();
    let mut ffi_warnings = check_ffi_safety(&index, api_data, &addition_names);

    // Also check documentation for invalid characters
    let doc_warnings = check_doc_characters(api_data);
    ffi_warnings.extend(doc_warnings);

    // Check for reserved keywords across all target languages
    let keyword_warnings = check_reserved_keywords(api_data);
    ffi_warnings.extend(keyword_warnings);

    print_ffi_safety_warnings(&ffi_warnings);

    // Count only critical errors (not informational warnings)
    // We'll check this later AFTER generating patches, so patches are always written
    let critical_error_count = ffi_warnings.iter().filter(|w| w.is_critical()).count();

    // Print info about non-critical warnings
    let info_warnings = ffi_warnings.len() - critical_error_count;
    if info_warnings > 0 {
        println!(
            "\n{} {} informational warnings (non-blocking)",
            "i".blue(),
            info_warnings
        );
    }

    // Report results
    println!("\n{}", "Analysis Results:".white().bold());
    println!(
        "  {} Path fixes needed: {}",
        "-".dimmed(),
        diff.path_fixes.len().to_string().yellow()
    );
    println!(
        "  {} Types to add: {}",
        "-".dimmed(),
        diff.additions.len().to_string().green()
    );
    println!(
        "  {} Types to remove: {}",
        "-".dimmed(),
        diff.removals.len().to_string().red()
    );
    println!(
        "  {} Type modifications: {}",
        "-".dimmed(),
        diff.modifications.len().to_string().blue()
    );

    // Print path fixes
    if !diff.path_fixes.is_empty() {
        println!("\n{}", "Path Fixes:".yellow().bold());
        for fix in &diff.path_fixes {
            println!(
                "  {} : {} {} {}",
                fix.type_name.white(),
                fix.old_path.red().strikethrough(),
                "->".dimmed(),
                fix.new_path.green()
            );
        }
    }

    // Print additions
    if !diff.additions.is_empty() {
        println!(
            "\n{} (first 30 of {}):",
            "Additions".green().bold(),
            diff.additions.len()
        );
        for addition in diff.additions.iter().take(30) {
            println!(
                "  {} {} ({}) @ {}",
                "+".green(),
                addition.type_name.white(),
                addition.kind.dimmed(),
                addition.full_path.cyan()
            );
        }
        if diff.additions.len() > 30 {
            println!(
                "  {} and {} more",
                "...".dimmed(),
                (diff.additions.len() - 30).to_string().yellow()
            );
        }
    }

    // Print removals (first 30)
    if !diff.removals.is_empty() {
        println!(
            "\n{} (first 30 of {}):",
            "Removals".red().bold(),
            diff.removals.len()
        );
        for removal in diff.removals.iter().take(30) {
            println!("  {} {}", "-".red(), removal.red());
        }
        if diff.removals.len() > 30 {
            println!(
                "  {} and {} more",
                "...".dimmed(),
                (diff.removals.len() - 30).to_string().yellow()
            );
        }
    }

    // Print module moves
    if !diff.module_moves.is_empty() {
        println!(
            "\n{} (first 30 of {}):",
            "Module Moves".magenta().bold(),
            diff.module_moves.len()
        );
        for module_move in diff.module_moves.iter().take(30) {
            println!(
                "  {} : {} {} {}",
                module_move.type_name.white(),
                module_move.from_module.red(),
                "->".dimmed(),
                module_move.to_module.green()
            );
        }
        if diff.module_moves.len() > 30 {
            println!(
                "  {} and {} more",
                "...".dimmed(),
                (diff.module_moves.len() - 30).to_string().yellow()
            );
        }
    }

    // Print modifications (derive/impl changes)
    if !diff.modifications.is_empty() {
        println!(
            "\n{} (first 30 of {}):",
            "Modifications".blue().bold(),
            diff.modifications.len()
        );
        for modification in diff.modifications.iter().take(30) {
            match &modification.kind {
                diff::ModificationKind::DeriveAdded { derive_name } => {
                    println!(
                        "  {}: {} derive({})",
                        modification.type_name.white(),
                        "+".green(),
                        derive_name.green()
                    );
                }
                diff::ModificationKind::DeriveRemoved { derive_name } => {
                    println!(
                        "  {}: {} derive({})",
                        modification.type_name.white(),
                        "-".red(),
                        derive_name.red()
                    );
                }
                diff::ModificationKind::CustomImplAdded { impl_name } => {
                    println!(
                        "  {}: {} custom_impl({})",
                        modification.type_name.white(),
                        "+".green(),
                        impl_name.green()
                    );
                }
                diff::ModificationKind::CustomImplRemoved { impl_name } => {
                    println!(
                        "  {}: {} custom_impl({})",
                        modification.type_name.white(),
                        "-".red(),
                        impl_name.red()
                    );
                }
                diff::ModificationKind::ReprChanged { old_repr, new_repr } => {
                    let old_display = old_repr.as_deref().unwrap_or("none");
                    let new_display = new_repr.as_deref().unwrap_or("none");
                    println!(
                        "  {}: repr {} {} {}",
                        modification.type_name.white(),
                        old_display.red(),
                        "->".dimmed(),
                        new_display.green()
                    );
                }
                diff::ModificationKind::FieldAdded {
                    field_name,
                    field_type,
                    ref_kind,
                } => {
                    let type_display = if ref_kind.is_default() {
                        field_type.clone()
                    } else {
                        format!(
                            "{}{}{}",
                            ref_kind.to_rust_prefix(),
                            field_type,
                            ref_kind.to_rust_suffix()
                        )
                    };
                    println!(
                        "  {}: {} field {} : {}",
                        modification.type_name.white(),
                        "+".green(),
                        field_name.green(),
                        type_display.cyan()
                    );
                }
                diff::ModificationKind::FieldRemoved { field_name } => {
                    println!(
                        "  {}: {} field {}",
                        modification.type_name.white(),
                        "-".red(),
                        field_name.red()
                    );
                }
                diff::ModificationKind::FieldTypeChanged {
                    field_name,
                    old_type,
                    new_type,
                    ref_kind,
                } => {
                    let new_type_display = if ref_kind.is_default() {
                        new_type.clone()
                    } else {
                        format!(
                            "{}{}{}",
                            ref_kind.to_rust_prefix(),
                            new_type,
                            ref_kind.to_rust_suffix()
                        )
                    };
                    println!(
                        "  {}: field {} : {} {} {}",
                        modification.type_name.white(),
                        field_name.white(),
                        old_type.red(),
                        "->".dimmed(),
                        new_type_display.green()
                    );
                }
                diff::ModificationKind::VariantAdded { variant_name } => {
                    println!(
                        "  {}: {} variant {}",
                        modification.type_name.white(),
                        "+".green(),
                        variant_name.green()
                    );
                }
                diff::ModificationKind::VariantRemoved { variant_name } => {
                    println!(
                        "  {}: {} variant {}",
                        modification.type_name.white(),
                        "-".red(),
                        variant_name.red()
                    );
                }
                diff::ModificationKind::VariantTypeChanged {
                    variant_name,
                    old_type,
                    new_type,
                } => {
                    println!(
                        "  {}: variant {} : {:?} {} {:?}",
                        modification.type_name.white(),
                        variant_name.white(),
                        old_type,
                        "->".dimmed(),
                        new_type
                    );
                }
                diff::ModificationKind::CallbackTypedefAdded { args, returns } => {
                    let args_str: Vec<String> = args
                        .iter()
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
                    println!(
                        "  {}: {} callback_typedef({}) -> {:?}",
                        modification.type_name.white(),
                        "+".green(),
                        args_str.join(", ").cyan(),
                        returns
                    );
                }
                diff::ModificationKind::CallbackArgChanged {
                    arg_index,
                    old_type,
                    new_type,
                    old_ref_kind,
                    new_ref_kind,
                } => {
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
                    println!(
                        "  {}: callback arg[{}] : {}{}{} {} {}{}{}",
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
                    println!(
                        "  {}: callback return : {:?} {} {:?}",
                        modification.type_name.white(),
                        old_type,
                        "->".dimmed(),
                        new_type
                    );
                }
                diff::ModificationKind::TypeAliasAdded {
                    target,
                    generic_args,
                } => {
                    let display_target = if generic_args.is_empty() {
                        target.clone()
                    } else {
                        format!("{}<{}>", target, generic_args.join(", "))
                    };
                    println!(
                        "  {}: {} type_alias = {}",
                        modification.type_name.white(),
                        "+".green(),
                        display_target.cyan()
                    );
                }
                diff::ModificationKind::TypeAliasTargetChanged {
                    old_target,
                    new_target,
                    new_generic_args,
                } => {
                    let display_new = if new_generic_args.is_empty() {
                        new_target.clone()
                    } else {
                        format!("{}<{}>", new_target, new_generic_args.join(", "))
                    };
                    println!(
                        "  {}: type_alias = {} {} {}",
                        modification.type_name.white(),
                        old_target.red(),
                        "->".dimmed(),
                        display_new.green()
                    );
                }
                diff::ModificationKind::GenericParamsChanged {
                    old_params,
                    new_params,
                } => {
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
                    println!(
                        "  {}: generic_params {} {} {}",
                        modification.type_name.white(),
                        old_display.red(),
                        "->".dimmed(),
                        new_display.green()
                    );
                }
                diff::ModificationKind::StructFieldsReplaced { fields } => {
                    let field_names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
                    println!(
                        "  {}: {} struct_fields [{}]",
                        modification.type_name.white(),
                        "=".cyan(),
                        field_names.join(", ").cyan()
                    );
                }
                diff::ModificationKind::EnumVariantsReplaced { variants } => {
                    let variant_names: Vec<&str> =
                        variants.iter().map(|v| v.name.as_str()).collect();
                    println!(
                        "  {}: {} enum_variants [{}]",
                        modification.type_name.white(),
                        "=".cyan(),
                        variant_names.join(", ").cyan()
                    );
                }
                diff::ModificationKind::StructFieldsRemoved => {
                    println!(
                        "  {}: {} struct_fields (type changed to enum)",
                        modification.type_name.white(),
                        "-".red()
                    );
                }
                diff::ModificationKind::EnumFieldsRemoved => {
                    println!(
                        "  {}: {} enum_fields (type changed to struct)",
                        modification.type_name.white(),
                        "-".red()
                    );
                }
                diff::ModificationKind::FunctionSelfMismatch {
                    fn_name,
                    expected_self,
                    actual_self,
                } => {
                    let expected = expected_self.as_deref().unwrap_or("static");
                    let actual = actual_self.as_deref().unwrap_or("static");
                    println!(
                        "  {}.{}: self mismatch - expected '{}', got '{}'",
                        modification.type_name.white(),
                        fn_name.yellow(),
                        expected.green(),
                        actual.red()
                    );
                }
                diff::ModificationKind::FunctionArgCountMismatch {
                    fn_name,
                    expected_count,
                    actual_count,
                } => {
                    println!(
                        "  {}.{}: arg count mismatch - expected {}, got {}",
                        modification.type_name.white(),
                        fn_name.yellow(),
                        expected_count.to_string().green(),
                        actual_count.to_string().red()
                    );
                }
                diff::ModificationKind::VecFunctionsMissing {
                    missing_functions,
                    element_type,
                } => {
                    println!(
                        "  {} (Vec<{}>): {} missing Vec functions: {}",
                        modification.type_name.white(),
                        element_type.cyan(),
                        "+".green(),
                        missing_functions.join(", ").yellow()
                    );
                }
                diff::ModificationKind::VecMissingOptionType {
                    vec_type,
                    element_type,
                    option_type_name,
                } => {
                    println!(
                        "  {} (Vec<{}>): {} missing Option type '{}' for c_get()",
                        vec_type.white(),
                        element_type.cyan(),
                        "+".green(),
                        option_type_name.yellow()
                    );
                }
                diff::ModificationKind::VecMissingSliceType {
                    vec_type,
                    element_type,
                    slice_type_name,
                } => {
                    println!(
                        "  {} (Vec<{}>): {} missing Slice type '{}' for as_c_slice()",
                        vec_type.white(),
                        element_type.cyan(),
                        "+".green(),
                        slice_type_name.yellow()
                    );
                }
            }
        }
        if diff.modifications.len() > 30 {
            println!(
                "  {} and {} more",
                "...".dimmed(),
                (diff.modifications.len() - 30).to_string().yellow()
            );
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
    let mut mods_by_type: std::collections::BTreeMap<String, Vec<&diff::TypeModification>> =
        std::collections::BTreeMap::new();
    for modification in &diff.modifications {
        mods_by_type
            .entry(modification.type_name.clone())
            .or_default()
            .push(modification);
    }

    // Find path fixes for types that also have modifications
    let mut path_fixes_by_type: std::collections::BTreeMap<String, &diff::PathFix> =
        std::collections::BTreeMap::new();
    for fix in &diff.path_fixes {
        path_fixes_by_type.insert(fix.type_name.clone(), fix);
    }

    // Generate combined patches for types with both path fixes and modifications
    let mut handled_types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for (type_name, mods) in &mods_by_type {
        let path_fix = path_fixes_by_type.get(type_name);
        let patch_content = generate_combined_patch(type_name, path_fix, mods);
        let patch_path = patches_dir.join(format!(
            "{:04}_modify_{}.patch.json",
            patch_count, type_name
        ));
        fs::write(&patch_path, &patch_content)?;
        patch_count += 1;
        handled_types.insert(type_name.clone());
    }

    // Generate standalone path fix patches for types without modifications
    for fix in &diff.path_fixes {
        if !handled_types.contains(&fix.type_name) {
            let patch_content = generate_path_fix_patch(fix);
            let patch_path = patches_dir.join(format!(
                "{:04}_path_fix_{}.patch.json",
                patch_count, fix.type_name
            ));
            fs::write(&patch_path, &patch_content)?;
            patch_count += 1;
        }
    }

    // Generate addition patches
    for addition in &diff.additions {
        let patch_content = generate_addition_patch(addition);
        let patch_path = patches_dir.join(format!(
            "{:04}_add_{}.patch.json",
            patch_count, addition.type_name
        ));
        fs::write(&patch_path, &patch_content)?;
        patch_count += 1;
    }

    // Generate removal patches
    for removal in &diff.removals {
        let type_name = removal.split(':').next().unwrap_or(removal);
        let patch_content = generate_removal_patch(removal);
        let patch_path = patches_dir.join(format!(
            "{:04}_remove_{}.patch.json",
            patch_count, type_name
        ));
        fs::write(&patch_path, &patch_content)?;
        patch_count += 1;
    }

    // Generate module move patches
    for module_move in &diff.module_moves {
        let patch_content = generate_module_move_patch(module_move);
        let patch_path = patches_dir.join(format!(
            "{:04}_move_{}.patch.json",
            patch_count, module_move.type_name
        ));
        fs::write(&patch_path, &patch_content)?;
        patch_count += 1;
    }

    println!(
        "\n{} {} patches in {}",
        "Generated".green().bold(),
        patch_count.to_string().white().bold(),
        patches_dir.display().to_string().cyan()
    );

    if patch_count > 0 {
        println!(
            "\n{}: Apply patches immediately or they may become stale:",
            "IMPORTANT".yellow().bold()
        );
        println!(
            "  cargo run --bin azul-doc -- patch {}",
            patches_dir.display()
        );
        println!("\nTo preview changes without applying:");
        println!("  cargo run --bin azul-doc -- autofix explain");
    }

    let duration = start_time.elapsed();
    println!(
        "{} ({:.2}s)\n",
        "Complete".green().bold(),
        duration.as_secs_f64()
    );

    // Now fail if there were critical FFI safety issues
    // This happens AFTER patches are written, so they can still be applied
    if critical_error_count > 0 {
        return Err(anyhow::anyhow!(
            "Found {} critical FFI safety issues in API types. Patches were generated but fix the errors before proceeding with codegen.",
            critical_error_count
        ));
    }

    Ok(())
}

/// Generate a combined patch for a type with path fix and/or modifications
fn generate_combined_patch(
    type_name: &str,
    path_fix: Option<&&diff::PathFix>,
    modifications: &[&diff::TypeModification],
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
                let field_defs: Vec<patch_format::StructFieldDef> = fields
                    .iter()
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
                let variant_defs: Vec<patch_format::EnumVariantDef> = variants
                    .iter()
                    .map(|v| patch_format::EnumVariantDef {
                        name: v.name.clone(),
                        variant_type: v.ty.clone(),
                    })
                    .collect();
                changes.push(ModifyChange::ReplaceEnumVariants {
                    variants: variant_defs,
                });
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
            diff::ModificationKind::FieldAdded {
                field_name,
                field_type,
                ref_kind,
            } => {
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
            diff::ModificationKind::FieldTypeChanged {
                field_name,
                old_type,
                new_type,
                ref_kind,
            } => {
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
            diff::ModificationKind::VariantTypeChanged {
                variant_name,
                old_type,
                new_type,
            } => {
                changes.push(ModifyChange::ChangeVariantType {
                    name: variant_name.clone(),
                    old_type: old_type.clone(),
                    new_type: new_type.clone(),
                });
            }
            diff::ModificationKind::CallbackTypedefAdded { args, returns } => {
                changes.push(ModifyChange::SetCallbackTypedef {
                    args: args
                        .iter()
                        .map(|arg| patch_format::CallbackArgDef {
                            arg_type: arg.ty.clone(),
                            ref_kind: arg.ref_kind.clone(),
                            name: arg.name.clone(),
                        })
                        .collect(),
                    returns: returns.clone(),
                });
            }
            diff::ModificationKind::CallbackArgChanged {
                arg_index,
                old_type,
                new_type,
                old_ref_kind,
                new_ref_kind,
            } => {
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
            diff::ModificationKind::TypeAliasAdded {
                target,
                generic_args,
            } => {
                changes.push(ModifyChange::SetTypeAlias {
                    target: target.clone(),
                    generic_args: generic_args.clone(),
                });
            }
            diff::ModificationKind::TypeAliasTargetChanged {
                old_target,
                new_target,
                new_generic_args,
            } => {
                changes.push(ModifyChange::ChangeTypeAlias {
                    old_target: old_target.clone(),
                    new_target: new_target.clone(),
                    new_generic_args: new_generic_args.clone(),
                });
            }
            diff::ModificationKind::GenericParamsChanged {
                old_params,
                new_params,
            } => {
                changes.push(ModifyChange::SetGenericParams {
                    old_params: old_params.clone(),
                    new_params: new_params.clone(),
                });
            }
            diff::ModificationKind::FunctionSelfMismatch {
                fn_name,
                expected_self,
                actual_self: _,
            } => {
                changes.push(ModifyChange::FixFunctionSelf {
                    fn_name: fn_name.clone(),
                    expected_self: expected_self.clone(),
                });
            }
            diff::ModificationKind::FunctionArgCountMismatch {
                fn_name,
                expected_count,
                actual_count: _,
            } => {
                changes.push(ModifyChange::FixFunctionArgs {
                    fn_name: fn_name.clone(),
                    expected_count: *expected_count,
                });
            }
            diff::ModificationKind::VecFunctionsMissing {
                missing_functions,
                element_type,
            } => {
                changes.push(ModifyChange::AddVecFunctions {
                    missing_functions: missing_functions.clone(),
                    element_type: element_type.clone(),
                });
            }
            diff::ModificationKind::VecMissingOptionType {
                vec_type: _,
                element_type,
                option_type_name,
            } => {
                // Generate the Option type automatically - it has a simple structure
                // Option<T> = { None, Some(T) }
                changes.push(ModifyChange::AddDependencyType {
                    dependency_type: option_type_name.clone(),
                    dependency_kind: "option".to_string(),
                    element_type: element_type.clone(),
                });
            }
            diff::ModificationKind::VecMissingSliceType {
                vec_type: _,
                element_type,
                slice_type_name,
            } => {
                // Generate the Slice type automatically - it has a simple structure
                // Slice<T> = { ptr: *const T, len: usize }
                changes.push(ModifyChange::AddDependencyType {
                    dependency_type: slice_type_name.clone(),
                    dependency_kind: "slice".to_string(),
                    element_type: element_type.clone(),
                });
            }
        }
    }

    // Add grouped derives
    if !derives_added.is_empty() {
        changes.push(ModifyChange::AddDerives {
            derives: derives_added,
        });
    }
    if !derives_removed.is_empty() {
        changes.push(ModifyChange::RemoveDerives {
            derives: derives_removed,
        });
    }

    // Add grouped custom_impls
    if !custom_impls_added.is_empty() {
        changes.push(ModifyChange::AddCustomImpls {
            impls: custom_impls_added,
        });
    }
    if !custom_impls_removed.is_empty() {
        changes.push(ModifyChange::RemoveCustomImpls {
            impls: custom_impls_removed,
        });
    }

    patch.add_operation(PatchOperation::Modify(ModifyOperation {
        type_name: type_name.to_string(),
        module: None,
        changes,
    }));

    patch
        .to_json()
        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
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

    patch
        .to_json()
        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
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
        fields
            .iter()
            .map(|(name, ty, ref_kind)| patch_format::FieldDef {
                name: name.clone(),
                field_type: ty.clone(),
                ref_kind: if ref_kind == "value" {
                    None
                } else {
                    Some(ref_kind.clone())
                },
                doc: None,
            })
            .collect()
    });

    // Convert enum_variants to VariantDef format
    let enum_variants = addition.enum_variants.as_ref().map(|variants| {
        variants
            .iter()
            .map(|(name, ty)| patch_format::VariantDef {
                name: name.clone(),
                variant_type: ty.clone(),
            })
            .collect()
    });

    let derives = if addition.derives.is_empty() {
        None
    } else {
        Some(addition.derives.clone())
    };

    // Convert callback_typedef info to CallbackTypedefDef
    let callback_typedef = addition.callback_typedef.as_ref().map(|info| {
        let fn_args: Vec<CallbackArg> = info
            .fn_args
            .iter()
            .map(|(ty, ref_kind)| CallbackArg {
                arg_type: ty.clone(),
                ref_kind: if ref_kind == "value" {
                    None
                } else {
                    Some(ref_kind.clone())
                },
            })
            .collect();

        let returns = info.returns.as_ref().map(|ret| CallbackReturn {
            return_type: ret.clone(),
            ref_kind: None,
        });

        CallbackTypedefDef { fn_args, returns }
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

    patch
        .to_json()
        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
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

    patch
        .to_json()
        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
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

    patch
        .to_json()
        .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}

// ffi safety checks
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
    StructMissingReprC { current_repr: Option<String> },
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
    EnumWithDataMissingReprCU8 { current_repr: Option<String> },
    /// Enum without data variants but missing repr(C)
    EnumMissingReprC { current_repr: Option<String> },
    /// Enum uses non-C repr like repr(u16) with explicit discriminant values
    /// This breaks FFI because discriminant values aren't portable
    NonCReprEnum { current_repr: String },
    /// Type name is duplicated in multiple files
    DuplicateTypeName { other_files: Vec<String> },
    /// Field uses Box<c_void> which is undefined behavior in Rust
    /// Box requires a valid sized type, c_void is unsized
    BoxCVoidField { field_name: String },
    /// Field uses an array type like [u8; 4] that may not be handled by C codegen
    ArrayTypeField {
        field_name: String,
        array_type: String,
    },
    /// Documentation contains invalid characters (emojis, non-ASCII symbols)
    /// These cause encoding issues in generated code and should use ASCII only
    InvalidDocCharacter {
        location: String,
        invalid_char: char,
        char_code: u32,
        context: String,
    },
    /// Name conflicts with a reserved keyword in one or more target languages
    /// This is a warning, not an error - codegen will escape the name
    ReservedKeyword {
        name_kind: String, // "type", "field", "function", "enum_variant"
        name: String,
        languages: Vec<String>,
    },
    /// Constructor name should be handled differently (e.g., "default" should use custom_impls)
    BadConstructorName { name: String, suggestion: String },
    /// Struct is a tuple struct like `struct Foo(pub u64)` instead of `struct Foo { inner: u64 }`
    /// Tuple structs don't work well with C API since the field has no name
    TupleStruct,
    /// Type uses `()` (unit type) which is not FFI-safe.
    /// Use `Void` type instead (e.g., `Result<Void, Error>` instead of `Result<(), Error>`).
    UnitTypeInSignature {
        location: String,       // e.g., "return type" or "field 'foo'"
        full_type: String,      // e.g., "Result<(), Error>"
    },
    /// Field uses BTreeMap or HashMap which is not FFI-safe.
    /// Replace with Vec-based pair type (e.g., StringPairVec).
    BTreeMapOrHashMapField {
        field_name: String,
        map_type: String,
    },
    /// struct_fields is empty `[{}]` — struct has no fields exposed to FFI.
    /// Either add fields or remove from api.json.
    EmptyStructFields,
    /// Type reference uses `Result<T, E>` which is not repr(C).
    /// Create a custom `ResultTE` enum instead.
    ResultTypeReference {
        location: String,
        result_type: String,
    },
    /// Type reference uses a tuple `(A, B)` which has no stable ABI.
    /// Create a wrapper struct instead.
    TupleTypeReference {
        location: String,
        tuple_type: String,
    },
    /// Type name contains `<` — raw generic syntax like `Vec<T>` or `Option<T>`
    /// is not allowed. Use `impl_vec!` / `impl_option!` macros and reference
    /// the generated wrapper type instead. Generic args go in the `generic_args`
    /// field of `type_alias` in api.json.
    AngleBracketInType {
        location: String,
        raw_type: String,
    },
    /// A type is referenced (as a field type, enum variant type, callback arg, etc.)
    /// in api.json but has no definition entry in api.json.
    /// This means codegen will emit code referencing a type that doesn't exist.
    UndefinedTypeReference {
        /// Where the reference occurs, e.g. "CssParsingErrorOwned::ColumnFill"
        location: String,
        /// The type name that is referenced but not defined
        referenced_type: String,
    },
    /// Type alias uses generic_args (e.g. `Vec<ComponentArgument>`) which is not FFI-safe
    /// unless both the target type and all generic args are defined in api.json.
    /// If the target (e.g. `CssPropertyValue`) and all args are in api.json, this is
    /// informational only. Otherwise, use the concrete type from impl_vec!/impl_option!.
    GenericTypeAlias {
        target: String,
        generic_args: Vec<String>,
        /// Whether the target type (e.g. CssPropertyValue) is defined in api.json
        target_in_api: bool,
        /// Whether all generic_args types are defined in api.json
        all_args_in_api: bool,
    },
    /// Type has more than one `#[repr(...)]` attribute.
    /// This is ambiguous and likely a bug — only one repr attribute should be present.
    DuplicateReprAttribute {
        /// Number of `#[repr(...)]` attributes found
        count: usize,
        /// The merged repr value
        merged_repr: String,
    },
}

impl FfiSafetyWarningKind {
    /// Returns true if this is a critical error that should fail the build
    /// ArrayTypeField is just informational - it's now handled correctly by codegen
    /// ReservedKeyword is a warning - codegen will escape the name
    pub fn is_critical(&self) -> bool {
        match self {
            // Critical errors that must be fixed
            FfiSafetyWarningKind::StructMissingReprC { .. } => true,
            FfiSafetyWarningKind::MultiFieldVariant { .. } => true,
            FfiSafetyWarningKind::StdOptionInVariant { .. } => true,
            FfiSafetyWarningKind::EnumWithDataMissingReprCU8 { .. } => true,
            FfiSafetyWarningKind::EnumMissingReprC { .. } => true,
            FfiSafetyWarningKind::NonCReprEnum { .. } => true,
            FfiSafetyWarningKind::DuplicateTypeName { .. } => true,
            FfiSafetyWarningKind::BoxCVoidField { .. } => true,
            FfiSafetyWarningKind::InvalidDocCharacter { .. } => true,
            // Informational only - array types are now handled correctly
            FfiSafetyWarningKind::ArrayTypeField { .. } => false,
            // Warning only - codegen will escape these names
            FfiSafetyWarningKind::ReservedKeyword { .. } => false,
            // Error - should use custom_impls instead
            FfiSafetyWarningKind::BadConstructorName { .. } => true,
            // Critical - tuple structs don't work with C API
            FfiSafetyWarningKind::TupleStruct => true,
            // Critical - () is not FFI-safe, use Void instead
            FfiSafetyWarningKind::UnitTypeInSignature { .. } => true,
            // Critical - BTreeMap/HashMap not repr(C)
            FfiSafetyWarningKind::BTreeMapOrHashMapField { .. } => true,
            // Critical - empty struct_fields means broken type
            FfiSafetyWarningKind::EmptyStructFields => true,
            // Critical - Result<T, E> not repr(C)
            FfiSafetyWarningKind::ResultTypeReference { .. } => true,
            // Critical - tuples have no stable ABI
            FfiSafetyWarningKind::TupleTypeReference { .. } => true,
            // Critical - raw generics like Vec<T> not allowed, use impl_vec!/impl_option!
            FfiSafetyWarningKind::AngleBracketInType { .. } => true,
            // Critical - referenced type not defined in api.json
            FfiSafetyWarningKind::UndefinedTypeReference { .. } => true,
            // Generic type aliases are only critical if the target or args are NOT in api.json.
            // e.g. Vec<ComponentArgument> is critical (Vec not in api.json),
            // but CssPropertyValue<StyleBackgroundContent> is fine (both in api.json).
            FfiSafetyWarningKind::GenericTypeAlias { target_in_api, all_args_in_api, .. } => {
                !target_in_api || !all_args_in_api
            }
            // Critical - multiple #[repr(...)] attributes are ambiguous
            FfiSafetyWarningKind::DuplicateReprAttribute { .. } => true,
        }
    }
}

/// Check if a type string contains the unit type `()` which is not FFI-safe.
/// Returns the full type if found, None otherwise.
fn contains_unit_type(type_str: &str) -> bool {
    // Check for `()` as a standalone type or in generic positions
    // We need to be careful not to match things like `FnOnce()` where () is args
    // The patterns we want to catch:
    // - `()` as a standalone type
    // - `Result<(), Error>` - () in first position of Result
    // - `Option<()>` - () inside Option
    
    // Simple check: look for `()` that is followed by `,` or `>` or end of string
    // or preceded by `<` or `,`
    let chars: Vec<char> = type_str.chars().collect();
    let len = chars.len();
    
    for i in 0..len {
        if i + 1 < len && chars[i] == '(' && chars[i + 1] == ')' {
            // Found `()` - check context
            let before = if i > 0 { Some(chars[i - 1]) } else { None };
            let after = if i + 2 < len { Some(chars[i + 2]) } else { None };
            
            // It's a unit type if:
            // - preceded by `<` or `,` or space or start
            // - followed by `,` or `>` or space or end
            let valid_before = matches!(before, None | Some('<') | Some(',') | Some(' '));
            let valid_after = matches!(after, None | Some(',') | Some('>') | Some(' '));
            
            if valid_before && valid_after {
                return true;
            }
        }
    }
    
    false
}

/// Check FFI safety of types that exist in api.json or are about to be added.
/// `additional_type_names` contains type names from diff.additions that aren't
/// in api.json yet but will be added — these also need repr checks in the source.
pub fn check_ffi_safety(
    index: &type_index::TypeIndex,
    api_data: &ApiData,
    additional_type_names: &[String],
) -> Vec<FfiSafetyWarning> {
    use std::collections::{BTreeMap, BTreeSet};

    // Build a set of type names that exist in api.json
    let mut api_types: BTreeSet<String> = api_data
        .0
        .values()
        .flat_map(|version| version.api.values())
        .flat_map(|module| module.classes.keys())
        .cloned()
        .collect();

    // Also include types about to be added — they need repr checks too
    for name in additional_type_names {
        api_types.insert(name.clone());
    }

    let mut warnings = Vec::new();

    // Check for duplicate type names first
    // Build a map of type_name -> list of (file path, is_generic)
    let mut type_locations: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    for (type_name, defs) in index.iter_all() {
        if !api_types.contains(type_name) {
            continue;
        }
        for typedef in defs {
            let file_path = typedef.file_path.display().to_string();
            let is_generic = match &typedef.kind {
                type_index::TypeDefKind::Struct { generic_params, .. } => {
                    !generic_params.is_empty()
                }
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
        let non_generic_files: Vec<&String> = files
            .iter()
            .filter(|(_, is_generic)| !*is_generic)
            .map(|(path, _)| path)
            .collect();

        if non_generic_files.len() > 1 {
            // Report for each non-generic file
            for file in &non_generic_files {
                let other_files: Vec<String> = non_generic_files
                    .iter()
                    .filter(|f| *f != file)
                    .map(|f| (*f).clone())
                    .collect();
                warnings.push(FfiSafetyWarning {
                    type_name: type_name.clone(),
                    file_path: (*file).clone(),
                    kind: FfiSafetyWarningKind::DuplicateTypeName { other_files },
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
            if let type_index::TypeDefKind::Struct {
                repr,
                repr_attr_count,
                fields,
                is_tuple_struct,
                ..
            } = &typedef.kind
            {
                // Check for duplicate #[repr(...)] attributes
                if *repr_attr_count > 1 {
                    warnings.push(FfiSafetyWarning {
                        type_name: type_name.clone(),
                        file_path: file_path.clone(),
                        kind: FfiSafetyWarningKind::DuplicateReprAttribute {
                            count: *repr_attr_count,
                            merged_repr: repr.clone().unwrap_or_default(),
                        },
                    });
                }

                // Tuple structs are not supported - they need named fields for C API
                if *is_tuple_struct {
                    warnings.push(FfiSafetyWarning {
                        type_name: type_name.clone(),
                        file_path: file_path.clone(),
                        kind: FfiSafetyWarningKind::TupleStruct,
                    });
                }

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
                    if field_def.ty == "c_void" && field_def.ref_kind == type_index::RefKind::Boxed
                    {
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
            if let type_index::TypeDefKind::Enum { variants, repr, repr_attr_count, .. } = &typedef.kind {
                // Check for duplicate #[repr(...)] attributes
                if *repr_attr_count > 1 {
                    warnings.push(FfiSafetyWarning {
                        type_name: type_name.clone(),
                        file_path: file_path.clone(),
                        kind: FfiSafetyWarningKind::DuplicateReprAttribute {
                            count: *repr_attr_count,
                            merged_repr: repr.clone().unwrap_or_default(),
                        },
                    });
                }

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
                        if ty.starts_with("Option<") || ty.starts_with("Option <") || ty == "Option"
                        {
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
                //          Enum without data needs repr(C)
                if has_data_variants {
                    let repr_ok = match repr {
                        Some(r) => {
                            // Must have both C and a discriminant type
                            let r_lower = r.to_lowercase();
                            r_lower.contains("c")
                                && (r_lower.contains("u8")
                                    || r_lower.contains("u16")
                                    || r_lower.contains("u32")
                                    || r_lower.contains("i8")
                                    || r_lower.contains("i16")
                                    || r_lower.contains("i32"))
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
                } else {
                    // Non-data enum must have repr(C)
                    let has_repr_c = match repr {
                        Some(r) => r.to_lowercase().contains("c"),
                        None => false,
                    };
                    if !has_repr_c {
                        warnings.push(FfiSafetyWarning {
                            type_name: type_name.clone(),
                            file_path: file_path.clone(),
                            kind: FfiSafetyWarningKind::EnumMissingReprC {
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
                    let is_bare_int_repr = (r_lower == "u8"
                        || r_lower == "u16"
                        || r_lower == "u32"
                        || r_lower == "i8"
                        || r_lower == "i16"
                        || r_lower == "i32")
                        && !r_lower.contains("c");
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

    // Check for () unit type in api.json function signatures
    for (_version_name, version) in &api_data.0 {
        for (module_name, module) in &version.api {
            for (class_name, class_def) in &module.classes {
                let file_path = format!("api.json - {}.{}", module_name, class_name);
                
                // Check functions
                if let Some(functions) = &class_def.functions {
                    for (fn_name, fn_def) in functions {
                        // Check return type
                        if let Some(ret) = &fn_def.returns {
                            if contains_unit_type(&ret.r#type) {
                                warnings.push(FfiSafetyWarning {
                                    type_name: format!("{}::{}", class_name, fn_name),
                                    file_path: file_path.clone(),
                                    kind: FfiSafetyWarningKind::UnitTypeInSignature {
                                        location: "return type".to_string(),
                                        full_type: ret.r#type.clone(),
                                    },
                                });
                            }
                        }
                        // Check args
                        for arg in &fn_def.fn_args {
                            for (arg_name, arg_type) in arg {
                                if contains_unit_type(arg_type) {
                                    warnings.push(FfiSafetyWarning {
                                        type_name: format!("{}::{}", class_name, fn_name),
                                        file_path: file_path.clone(),
                                        kind: FfiSafetyWarningKind::UnitTypeInSignature {
                                            location: format!("argument '{}'", arg_name),
                                            full_type: arg_type.clone(),
                                        },
                                    });
                                }
                            }
                        }
                    }
                }
                
                // Check constructors
                if let Some(constructors) = &class_def.constructors {
                    for (ctor_name, ctor_def) in constructors {
                        // Check return type
                        if let Some(ret) = &ctor_def.returns {
                            if contains_unit_type(&ret.r#type) {
                                warnings.push(FfiSafetyWarning {
                                    type_name: format!("{}::{}", class_name, ctor_name),
                                    file_path: file_path.clone(),
                                    kind: FfiSafetyWarningKind::UnitTypeInSignature {
                                        location: "return type".to_string(),
                                        full_type: ret.r#type.clone(),
                                    },
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Check api.json types for repr mismatches AND struct issues
    for (_version_name, version) in &api_data.0 {
        for (module_name, module) in &version.api {
            for (class_name, class_def) in &module.classes {
                let file_path = format!("api.json - {}.{}", module_name, class_name);

                // --- Enum checks ---
                if let Some(variants) = &class_def.enum_fields {
                    let has_data_variants = variants.iter().any(|v| {
                        v.values().any(|variant_data| variant_data.r#type.is_some())
                    });

                    if has_data_variants {
                        // Enum with data needs repr(C, u8) or repr(C, i8) etc.
                        let repr_ok = match &class_def.repr {
                            Some(r) => {
                                let r_lower = r.to_lowercase().replace(" ", "");
                                r_lower.contains("c")
                                    && (r_lower.contains("u8")
                                        || r_lower.contains("u16")
                                        || r_lower.contains("u32")
                                        || r_lower.contains("i8")
                                        || r_lower.contains("i16")
                                        || r_lower.contains("i32"))
                            }
                            None => false,
                        };

                        if !repr_ok {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::EnumWithDataMissingReprCU8 {
                                    current_repr: class_def.repr.clone(),
                                },
                            });
                        }
                    }
                }

                // --- Struct checks ---
                if let Some(struct_fields) = &class_def.struct_fields {
                    // Check 1: Struct must have repr(C) or repr(transparent)
                    // Type aliases inherit repr from their target type in Rust,
                    // so skip this check if the type has a type_alias field.
                    let is_type_alias = class_def.type_alias.is_some();
                    let has_repr = match &class_def.repr {
                        Some(r) => {
                            let r_lower = r.to_lowercase();
                            r_lower.contains("c") || r_lower.contains("transparent")
                        }
                        None => is_type_alias, // type aliases inherit repr from target
                    };

                    if !has_repr {
                        warnings.push(FfiSafetyWarning {
                            type_name: class_name.clone(),
                            file_path: file_path.clone(),
                            kind: FfiSafetyWarningKind::StructMissingReprC {
                                current_repr: class_def.repr.clone(),
                            },
                        });
                    }

                    // Check 2: Empty struct_fields [{}] — broken type with no fields
                    let all_empty = struct_fields.iter().all(|m| m.is_empty());
                    if all_empty && !struct_fields.is_empty() {
                        warnings.push(FfiSafetyWarning {
                            type_name: class_name.clone(),
                            file_path: file_path.clone(),
                            kind: FfiSafetyWarningKind::EmptyStructFields,
                        });
                    }

                    // Check 3: Fields using BTreeMap/HashMap, Result<>, or tuple types
                    for field_map in struct_fields {
                        for (field_name, field_data) in field_map {
                            let ty = &field_data.r#type;
                            if ty.contains("BTreeMap") || ty.contains("HashMap") {
                                warnings.push(FfiSafetyWarning {
                                    type_name: class_name.clone(),
                                    file_path: file_path.clone(),
                                    kind: FfiSafetyWarningKind::BTreeMapOrHashMapField {
                                        field_name: field_name.clone(),
                                        map_type: ty.clone(),
                                    },
                                });
                            }
                            if ty.starts_with("Result<") {
                                warnings.push(FfiSafetyWarning {
                                    type_name: class_name.clone(),
                                    file_path: file_path.clone(),
                                    kind: FfiSafetyWarningKind::ResultTypeReference {
                                        location: format!("field '{}'", field_name),
                                        result_type: ty.clone(),
                                    },
                                });
                            }
                            if ty.starts_with('(') && ty.ends_with(')') && ty.contains(',') {
                                warnings.push(FfiSafetyWarning {
                                    type_name: class_name.clone(),
                                    file_path: file_path.clone(),
                                    kind: FfiSafetyWarningKind::TupleTypeReference {
                                        location: format!("field '{}'", field_name),
                                        tuple_type: ty.clone(),
                                    },
                                });
                            }
                            if ty.contains('<') {
                                warnings.push(FfiSafetyWarning {
                                    type_name: class_name.clone(),
                                    file_path: file_path.clone(),
                                    kind: FfiSafetyWarningKind::AngleBracketInType {
                                        location: format!("field '{}'", field_name),
                                        raw_type: ty.clone(),
                                    },
                                });
                            }
                        }
                    }
                }

                // --- Callback typedef checks ---
                if let Some(cb) = &class_def.callback_typedef {
                    if let Some(ret) = &cb.returns {
                        let ty = &ret.r#type;
                        if ty.starts_with("Result<") {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::ResultTypeReference {
                                    location: "callback return type".to_string(),
                                    result_type: ty.clone(),
                                },
                            });
                        }
                        if ty.starts_with('(') && ty.ends_with(')') && ty.contains(',') {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::TupleTypeReference {
                                    location: "callback return type".to_string(),
                                    tuple_type: ty.clone(),
                                },
                            });
                        }
                        if ty.contains('<') {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::AngleBracketInType {
                                    location: "callback return type".to_string(),
                                    raw_type: ty.clone(),
                                },
                            });
                        }
                    }
                    // Check callback arg types for angle brackets
                    for arg in &cb.fn_args {
                        let aty = &arg.r#type;
                        if aty.contains('<') {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::AngleBracketInType {
                                    location: format!("callback arg '{}'", aty),
                                    raw_type: aty.clone(),
                                },
                            });
                        }
                    }
                }

                // --- Type alias checks ---
                if let Some(ta) = &class_def.type_alias {
                    // Check: generic_args should not be used - type aliases
                    // with generic args produce invalid types like AzVec<T>.
                    // Use the concrete type from impl_vec!/impl_option! instead.
                    if !ta.generic_args.is_empty() {
                        let target_in_api = api_types.contains(&ta.target);
                        let is_known_type = |t: &str| -> bool {
                            api_types.contains(t) || matches!(t,
                                "u8" | "u16" | "u32" | "u64" | "u128" |
                                "i8" | "i16" | "i32" | "i64" | "i128" |
                                "f32" | "f64" | "bool" | "usize" | "isize"
                            )
                        };
                        let all_args_in_api = ta.generic_args.iter().all(|arg| is_known_type(arg));
                        warnings.push(FfiSafetyWarning {
                            type_name: class_name.clone(),
                            file_path: file_path.clone(),
                            kind: FfiSafetyWarningKind::GenericTypeAlias {
                                target: ta.target.clone(),
                                generic_args: ta.generic_args.clone(),
                                target_in_api,
                                all_args_in_api,
                            },
                        });
                    }
                    // Check target type for angle brackets
                    if ta.target.contains('<') {
                        warnings.push(FfiSafetyWarning {
                            type_name: class_name.clone(),
                            file_path: file_path.clone(),
                            kind: FfiSafetyWarningKind::AngleBracketInType {
                                location: "type_alias target".to_string(),
                                raw_type: ta.target.clone(),
                            },
                        });
                    }
                    for arg in &ta.generic_args {
                        if arg.starts_with('(') && arg.ends_with(')') && arg.contains(',') {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::TupleTypeReference {
                                    location: "type_alias generic_arg".to_string(),
                                    tuple_type: arg.clone(),
                                },
                            });
                        }
                        if arg.contains("BTreeMap") || arg.contains("HashMap") {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::BTreeMapOrHashMapField {
                                    field_name: "generic_arg".to_string(),
                                    map_type: arg.clone(),
                                },
                            });
                        }
                        if arg.contains('<') {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: file_path.clone(),
                                kind: FfiSafetyWarningKind::AngleBracketInType {
                                    location: "type_alias generic_arg".to_string(),
                                    raw_type: arg.clone(),
                                },
                            });
                        }
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

    // Separate critical errors from info warnings
    let critical: Vec<_> = warnings.iter().filter(|w| w.is_critical()).collect();
    let info: Vec<_> = warnings.iter().filter(|w| !w.is_critical()).collect();

    // Print critical errors first with ERROR prefix
    if !critical.is_empty() {
        println!(
            "\n{} {} critical FFI safety errors:",
            "[ ERROR ]".red().bold(),
            critical.len().to_string().red().bold()
        );
        for warning in &critical {
            print_single_warning(warning);
        }
    }

    // Print info warnings
    if !info.is_empty() {
        println!(
            "\n{} {} FFI safety warnings (non-blocking):",
            "[ WARN ]".yellow().bold(),
            info.len().to_string().yellow().bold()
        );
        for warning in &info {
            print_single_warning(warning);
        }
    }
}

fn print_single_warning(warning: &FfiSafetyWarning) {
    match &warning.kind {
        FfiSafetyWarningKind::StructMissingReprC { current_repr } => {
            let repr_display = current_repr.as_deref().unwrap_or("none");
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Struct has repr: {}",
                "→".dimmed(),
                repr_display.yellow()
            );
            println!(
                "    {} Add #[repr(C)] for FFI-safe struct layout.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::MultiFieldVariant {
            variant_name,
            field_count,
            field_types,
        } => {
            println!(
                "  {} {}",
                "✗".red(),
                format!("{}::{}", warning.type_name, variant_name).white()
            );
            println!(
                "    {} Enum variant has {} fields: {}",
                "→".dimmed(),
                field_count.to_string().red(),
                field_types.yellow()
            );
            println!(
                "    {} FFI requires exactly ONE field per variant. Wrap in a struct.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::StdOptionInVariant {
            variant_name,
            option_type,
        } => {
            println!(
                "  {} {}",
                "✗".red(),
                format!("{}::{}", warning.type_name, variant_name).white()
            );
            println!(
                "    {} Uses std::Option: {}",
                "→".dimmed(),
                option_type.yellow()
            );
            println!(
                "    {} Use custom OptionXxx type from impl_option! macro instead.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::EnumWithDataMissingReprCU8 { current_repr } => {
            let repr_display = current_repr.as_deref().unwrap_or("none");
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Enum with data has repr: {}",
                "→".dimmed(),
                repr_display.yellow()
            );
            println!(
                "    {} Add #[repr(C, u8)] for enums with data variants.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::EnumMissingReprC { current_repr } => {
            let repr_display = current_repr.as_deref().unwrap_or("none");
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Enum (no data variants) has repr: {}",
                "→".dimmed(),
                repr_display.yellow()
            );
            println!(
                "    {} Add #[repr(C)] for enums without data variants.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::NonCReprEnum { current_repr } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Enum uses non-C repr: #[repr({})]",
                "→".dimmed(),
                current_repr.yellow()
            );
            println!(
                "    {} Explicit discriminant values (= 100, = 200) are not FFI-safe.",
                "REASON:".magenta()
            );
            println!(
                "    {} Use #[repr(C)] for enums without data, remove explicit discriminant \
                     values.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::DuplicateTypeName { other_files } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Type name is duplicated in multiple files!",
                "→".dimmed()
            );
            println!("    {} Also defined in:", "ALSO:".magenta());
            for other in other_files {
                println!("       - {}", other.yellow());
            }
            println!(
                "    {} Rename one of the types to avoid name collision in C API.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::BoxCVoidField { field_name } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Field '{}' uses Box<c_void> which is undefined behavior!",
                "→".dimmed(),
                field_name.yellow()
            );
            println!(
                "    {} Box<T> requires T to be Sized, but c_void is not.",
                "REASON:".magenta()
            );
            println!(
                "    {} Use *mut c_void instead of Box<c_void>.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::ArrayTypeField {
            field_name,
            array_type,
        } => {
            println!("  {} {}", "[ WARN ]".yellow(), warning.type_name.white());
            println!(
                "    {} Field '{}' uses array type: {}",
                "→".dimmed(),
                field_name.yellow(),
                array_type.cyan()
            );
            println!(
                "    {} Array types require special handling in C codegen.",
                "NOTE:".magenta()
            );
            println!(
                "    {} Verify that extract_array_from_type() handles this type.",
                "CHECK:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::InvalidDocCharacter {
            location,
            invalid_char,
            char_code,
            context,
        } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Invalid character '{}' (U+{:04X}) in documentation",
                "→".dimmed(),
                invalid_char,
                char_code
            );
            println!("    {} Location: {}", "AT:".magenta(), location.yellow());
            println!("    {} \"{}\"", "CONTEXT:".dimmed(), context.dimmed());
            println!(
                "    {} Use ASCII-only characters in documentation. Replace with text \
                     equivalent.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::ReservedKeyword {
            name_kind,
            name,
            languages,
        } => {
            let lang_list = languages.join(", ");
            println!("  {} {}", "[ INFO ]".blue(), warning.type_name.white());
            println!(
                "    {} {} '{}' is a reserved keyword in: {}",
                "→".dimmed(),
                name_kind.cyan(),
                name.yellow(),
                lang_list.red()
            );
            println!(
                "    {} Language bindings will use alternative naming (e.g., '{}', '{}_').",
                "NOTE:".magenta(),
                format!("new_{}", name),
                name
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::BadConstructorName { name, suggestion } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Constructor '{}' should not be used directly.",
                "→".dimmed(),
                name.yellow()
            );
            println!("    {} {}", "FIX:".cyan(), suggestion);
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::TupleStruct => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Tuple struct is not FFI-safe. Use named fields instead.",
                "→".dimmed()
            );
            println!(
                "    {} Convert `struct Foo(pub T)` to `struct Foo {{ inner: T }}`",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::UnitTypeInSignature { location, full_type } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} {} uses unit type '()' which is not FFI-safe: {}",
                "→".dimmed(),
                location.cyan(),
                full_type.yellow()
            );
            println!(
                "    {} The unit type '()' has zero size and cannot be used in FFI.",
                "REASON:".magenta()
            );
            println!(
                "    {} Use 'Void' instead. E.g., `Result<Void, Error>` instead of `Result<(), Error>`.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::BTreeMapOrHashMapField { field_name, map_type } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Field '{}' uses map type: {}",
                "→".dimmed(),
                field_name.yellow(),
                map_type.yellow()
            );
            println!(
                "    {} BTreeMap/HashMap is not repr(C). Replace with Vec-based pair type.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::EmptyStructFields => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Struct has empty struct_fields [{{}}] — no fields exposed to FFI.",
                "→".dimmed()
            );
            println!(
                "    {} Add fields or remove the type from api.json.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::ResultTypeReference { location, result_type } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} {} uses Result type: {}",
                "→".dimmed(),
                location.cyan(),
                result_type.yellow()
            );
            println!(
                "    {} Result<T,E> is not repr(C). Create a custom ResultTE enum.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::TupleTypeReference { location, tuple_type } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} {} uses tuple type: {}",
                "→".dimmed(),
                location.cyan(),
                tuple_type.yellow()
            );
            println!(
                "    {} Tuples have no stable ABI. Create a wrapper struct with named fields.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::AngleBracketInType { location, raw_type } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} {} uses raw generic type: {}",
                "→".dimmed(),
                location.cyan(),
                raw_type.yellow()
            );
            println!(
                "    {} Types with '<' are not FFI-safe. Use impl_vec!/impl_option! and put generic args in the 'generic_args' field.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::UndefinedTypeReference { location, referenced_type } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} {} references undefined type: {}",
                "→".dimmed(),
                location.cyan(),
                referenced_type.yellow()
            );
            println!(
                "    {} Add '{}' to api.json or remove the reference.",
                "FIX:".cyan(),
                referenced_type
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::GenericTypeAlias { target, generic_args, target_in_api, all_args_in_api } => {
            let display = format!("{}<{}>", target, generic_args.join(", "));
            if *target_in_api && *all_args_in_api {
                // Non-critical: both target and args are in api.json
                println!("  {} {}", "⚠".yellow(), warning.type_name.white());
                println!(
                    "    {} type_alias uses generic type: {} (target and args in api.json - OK)",
                    "→".dimmed(),
                    display.yellow()
                );
            } else {
                println!("  {} {}", "✗".red(), warning.type_name.white());
                println!(
                    "    {} type_alias uses generic type: {}",
                    "→".dimmed(),
                    display.yellow()
                );
                if !target_in_api {
                    println!(
                        "    {} Target type '{}' is not defined in api.json. Use impl_vec!/impl_option! to generate a concrete type.",
                        "FIX:".cyan(),
                        target
                    );
                }
                if !all_args_in_api {
                    println!(
                        "    {} Some generic args are not defined in api.json. Ensure all arg types are in api.json or use concrete types.",
                        "FIX:".cyan()
                    );
                }
            }
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
        FfiSafetyWarningKind::DuplicateReprAttribute { count, merged_repr } => {
            println!("  {} {}", "✗".red(), warning.type_name.white());
            println!(
                "    {} Type has {} #[repr(...)] attributes (merged: {})",
                "→".dimmed(),
                count.to_string().red(),
                merged_repr.yellow()
            );
            println!(
                "    {} Remove duplicate #[repr(...)] attributes. Only one is allowed per type.",
                "FIX:".cyan()
            );
            println!("    {} {}", "FILE:".dimmed(), warning.file_path.dimmed());
        }
    }
    println!();
}

/// Check for invalid characters in documentation strings
/// Returns warnings for any non-ASCII characters that could cause encoding issues
pub fn check_doc_characters(api_data: &ApiData) -> Vec<FfiSafetyWarning> {
    let mut warnings = Vec::new();

    // Characters that are not allowed in documentation
    // We allow basic ASCII printable characters (0x20-0x7E), newlines, tabs
    fn is_valid_doc_char(c: char) -> bool {
        matches!(c, ' '..='~' | '\n' | '\r' | '\t')
    }

    // Find the first invalid character and some context around it
    fn find_invalid_char(text: &str) -> Option<(char, u32, String)> {
        for (i, c) in text.char_indices() {
            if !is_valid_doc_char(c) {
                // Get context: up to 20 chars before and after
                let start = text[..i]
                    .chars()
                    .rev()
                    .take(20)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>();
                let end: String = text[i..].chars().skip(1).take(20).collect();
                let context = format!("...{}[{}]{}...", start, c, end);
                return Some((c, c as u32, context));
            }
        }
        None
    }

    for (_version_key, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            // Check module documentation
            if let Some(doc_lines) = &module_data.doc {
                let doc_text = doc_lines.join("\n");
                if let Some((invalid_char, char_code, context)) = find_invalid_char(&doc_text) {
                    warnings.push(FfiSafetyWarning {
                        type_name: format!("module:{}", module_name),
                        file_path: format!("api.json - {}.doc", module_name),
                        kind: FfiSafetyWarningKind::InvalidDocCharacter {
                            location: format!("module {} documentation", module_name),
                            invalid_char,
                            char_code,
                            context,
                        },
                    });
                }
            }

            // Check class documentation and functions
            for (class_name, class_data) in &module_data.classes {
                // Check class doc
                if let Some(doc_lines) = &class_data.doc {
                    let doc_text = doc_lines.join("\n");
                    if let Some((invalid_char, char_code, context)) = find_invalid_char(&doc_text) {
                        warnings.push(FfiSafetyWarning {
                            type_name: class_name.clone(),
                            file_path: format!("api.json - {}.{}.doc", module_name, class_name),
                            kind: FfiSafetyWarningKind::InvalidDocCharacter {
                                location: format!(
                                    "{}.{} class documentation",
                                    module_name, class_name
                                ),
                                invalid_char,
                                char_code,
                                context,
                            },
                        });
                    }
                }

                // Check struct fields documentation
                if let Some(fields_vec) = &class_data.struct_fields {
                    for field_map in fields_vec {
                        for (field_name, field_data) in field_map {
                            if let Some(doc_lines) = &field_data.doc {
                                let doc_text = doc_lines.join("\n");
                                if let Some((invalid_char, char_code, context)) =
                                    find_invalid_char(&doc_text)
                                {
                                    warnings.push(FfiSafetyWarning {
                                        type_name: class_name.clone(),
                                        file_path: format!(
                                            "api.json - {}.{}.{}",
                                            module_name, class_name, field_name
                                        ),
                                        kind: FfiSafetyWarningKind::InvalidDocCharacter {
                                            location: format!(
                                                "{}.{}.{} field documentation",
                                                module_name, class_name, field_name
                                            ),
                                            invalid_char,
                                            char_code,
                                            context,
                                        },
                                    });
                                }
                            }
                        }
                    }
                }

                // Check function documentation
                if let Some(functions) = &class_data.functions {
                    for (fn_name, fn_data) in functions {
                        if let Some(doc_lines) = &fn_data.doc {
                            let doc_text = doc_lines.join("\n");
                            if let Some((invalid_char, char_code, context)) =
                                find_invalid_char(&doc_text)
                            {
                                warnings.push(FfiSafetyWarning {
                                    type_name: class_name.clone(),
                                    file_path: format!(
                                        "api.json - {}.{}.{}",
                                        module_name, class_name, fn_name
                                    ),
                                    kind: FfiSafetyWarningKind::InvalidDocCharacter {
                                        location: format!(
                                            "{}.{}.{} function documentation",
                                            module_name, class_name, fn_name
                                        ),
                                        invalid_char,
                                        char_code,
                                        context,
                                    },
                                });
                            }
                        }
                    }
                }

                // Check constructor documentation
                if let Some(constructors) = &class_data.constructors {
                    for (ctor_name, ctor_data) in constructors {
                        if let Some(doc_lines) = &ctor_data.doc {
                            let doc_text = doc_lines.join("\n");
                            if let Some((invalid_char, char_code, context)) =
                                find_invalid_char(&doc_text)
                            {
                                warnings.push(FfiSafetyWarning {
                                    type_name: class_name.clone(),
                                    file_path: format!(
                                        "api.json - {}.{}.{}",
                                        module_name, class_name, ctor_name
                                    ),
                                    kind: FfiSafetyWarningKind::InvalidDocCharacter {
                                        location: format!(
                                            "{}.{}.{} constructor documentation",
                                            module_name, class_name, ctor_name
                                        ),
                                        invalid_char,
                                        char_code,
                                        context,
                                    },
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    warnings
}

/// Reserved keywords for all target languages
/// These are words that cannot be used as identifiers in the respective language
/// Organized by language for easy maintenance
pub mod reserved_keywords {
    use std::collections::HashMap;

    /// Get all reserved keywords organized by language
    pub fn get_all_keywords() -> HashMap<&'static str, &'static [&'static str]> {
        let mut map = HashMap::new();

        // C keywords
        map.insert(
            "c",
            &[
                "auto",
                "break",
                "case",
                "char",
                "const",
                "continue",
                "default",
                "do",
                "double",
                "else",
                "enum",
                "extern",
                "float",
                "for",
                "goto",
                "if",
                "inline",
                "int",
                "long",
                "register",
                "restrict",
                "return",
                "short",
                "signed",
                "sizeof",
                "static",
                "struct",
                "switch",
                "typedef",
                "union",
                "unsigned",
                "void",
                "volatile",
                "while",
                "_Alignas",
                "_Alignof",
                "_Atomic",
                "_Bool",
                "_Complex",
                "_Generic",
                "_Imaginary",
                "_Noreturn",
                "_Static_assert",
                "_Thread_local",
                "bool",
                "true",
                "false",
                "alignas",
                "alignof",
                "noreturn",
                "static_assert",
                "thread_local",
            ][..],
        );

        // C++ keywords (superset of C, plus these)
        map.insert(
            "cpp",
            &[
                // C++ specific
                "asm",
                "catch",
                "class",
                "const_cast",
                "delete",
                "dynamic_cast",
                "explicit",
                "export",
                "friend",
                "mutable",
                "namespace",
                "new",
                "operator",
                "private",
                "protected",
                "public",
                "reinterpret_cast",
                "static_cast",
                "template",
                "this",
                "throw",
                "try",
                "typeid",
                "typename",
                "using",
                "virtual",
                "wchar_t",
                // C++11
                "alignas",
                "alignof",
                "char16_t",
                "char32_t",
                "constexpr",
                "decltype",
                "noexcept",
                "nullptr",
                "static_assert",
                "thread_local",
                // C++14
                // C++17
                // C++20
                "char8_t",
                "concept",
                "consteval",
                "constinit",
                "co_await",
                "co_return",
                "co_yield",
                "requires",
                // C++23
                // Also include C keywords
                "auto",
                "break",
                "case",
                "char",
                "const",
                "continue",
                "default",
                "do",
                "double",
                "else",
                "enum",
                "extern",
                "float",
                "for",
                "goto",
                "if",
                "inline",
                "int",
                "long",
                "register",
                "return",
                "short",
                "signed",
                "sizeof",
                "static",
                "struct",
                "switch",
                "typedef",
                "union",
                "unsigned",
                "void",
                "volatile",
                "while",
                "bool",
                "true",
                "false",
            ][..],
        );

        // Python keywords
        map.insert(
            "python",
            &[
                "False",
                "None",
                "True",
                "and",
                "as",
                "assert",
                "async",
                "await",
                "break",
                "class",
                "continue",
                "def",
                "del",
                "elif",
                "else",
                "except",
                "finally",
                "for",
                "from",
                "global",
                "if",
                "import",
                "in",
                "is",
                "lambda",
                "nonlocal",
                "not",
                "or",
                "pass",
                "raise",
                "return",
                "try",
                "while",
                "with",
                "yield",
                // Also commonly problematic builtins
                "type",
                "id",
                "list",
                "dict",
                "set",
                "str",
                "int",
                "float",
                "bool",
                "object",
                "property",
                "staticmethod",
                "classmethod",
                "super",
                "self",
            ][..],
        );

        // Java keywords
        map.insert(
            "java",
            &[
                "abstract",
                "assert",
                "boolean",
                "break",
                "byte",
                "case",
                "catch",
                "char",
                "class",
                "const",
                "continue",
                "default",
                "do",
                "double",
                "else",
                "enum",
                "extends",
                "final",
                "finally",
                "float",
                "for",
                "goto",
                "if",
                "implements",
                "import",
                "instanceof",
                "int",
                "interface",
                "long",
                "native",
                "new",
                "null",
                "package",
                "private",
                "protected",
                "public",
                "return",
                "short",
                "static",
                "strictfp",
                "super",
                "switch",
                "synchronized",
                "this",
                "throw",
                "throws",
                "transient",
                "try",
                "void",
                "volatile",
                "while",
                // Reserved for future
                "true",
                "false",
            ][..],
        );

        // Kotlin keywords
        map.insert(
            "kotlin",
            &[
                "as",
                "break",
                "class",
                "continue",
                "do",
                "else",
                "false",
                "for",
                "fun",
                "if",
                "in",
                "interface",
                "is",
                "null",
                "object",
                "package",
                "return",
                "super",
                "this",
                "throw",
                "true",
                "try",
                "typealias",
                "typeof",
                "val",
                "var",
                "when",
                "while",
                // Soft keywords
                "by",
                "catch",
                "constructor",
                "delegate",
                "dynamic",
                "field",
                "file",
                "finally",
                "get",
                "import",
                "init",
                "param",
                "property",
                "receiver",
                "set",
                "setparam",
                "value",
                "where",
            ][..],
        );

        // C# keywords
        map.insert(
            "csharp",
            &[
                "abstract",
                "as",
                "base",
                "bool",
                "break",
                "byte",
                "case",
                "catch",
                "char",
                "checked",
                "class",
                "const",
                "continue",
                "decimal",
                "default",
                "delegate",
                "do",
                "double",
                "else",
                "enum",
                "event",
                "explicit",
                "extern",
                "false",
                "finally",
                "fixed",
                "float",
                "for",
                "foreach",
                "goto",
                "if",
                "implicit",
                "in",
                "int",
                "interface",
                "internal",
                "is",
                "lock",
                "long",
                "namespace",
                "new",
                "null",
                "object",
                "operator",
                "out",
                "override",
                "params",
                "private",
                "protected",
                "public",
                "readonly",
                "ref",
                "return",
                "sbyte",
                "sealed",
                "short",
                "sizeof",
                "stackalloc",
                "static",
                "string",
                "struct",
                "switch",
                "this",
                "throw",
                "true",
                "try",
                "typeof",
                "uint",
                "ulong",
                "unchecked",
                "unsafe",
                "ushort",
                "using",
                "virtual",
                "void",
                "volatile",
                "while",
            ][..],
        );

        // Go keywords
        map.insert(
            "go",
            &[
                "break",
                "case",
                "chan",
                "const",
                "continue",
                "default",
                "defer",
                "else",
                "fallthrough",
                "for",
                "func",
                "go",
                "goto",
                "if",
                "import",
                "interface",
                "map",
                "package",
                "range",
                "return",
                "select",
                "struct",
                "switch",
                "type",
                "var",
                // Predeclared identifiers
                "bool",
                "byte",
                "complex64",
                "complex128",
                "error",
                "float32",
                "float64",
                "int",
                "int8",
                "int16",
                "int32",
                "int64",
                "rune",
                "string",
                "uint",
                "uint8",
                "uint16",
                "uint32",
                "uint64",
                "uintptr",
                "true",
                "false",
                "iota",
                "nil",
                "append",
                "cap",
                "close",
                "complex",
                "copy",
                "delete",
                "imag",
                "len",
                "make",
                "new",
                "panic",
                "print",
                "println",
                "real",
                "recover",
            ][..],
        );

        // Ruby keywords
        map.insert(
            "ruby",
            &[
                "BEGIN",
                "END",
                "alias",
                "and",
                "begin",
                "break",
                "case",
                "class",
                "def",
                "defined?",
                "do",
                "else",
                "elsif",
                "end",
                "ensure",
                "false",
                "for",
                "if",
                "in",
                "module",
                "next",
                "nil",
                "not",
                "or",
                "redo",
                "rescue",
                "retry",
                "return",
                "self",
                "super",
                "then",
                "true",
                "undef",
                "unless",
                "until",
                "when",
                "while",
                "yield",
                "__FILE__",
                "__LINE__",
                "__ENCODING__",
            ][..],
        );

        // Lua keywords
        map.insert(
            "lua",
            &[
                "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto",
                "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true",
                "until", "while",
            ][..],
        );

        // PHP keywords
        map.insert(
            "php",
            &[
                "abstract",
                "and",
                "array",
                "as",
                "break",
                "callable",
                "case",
                "catch",
                "class",
                "clone",
                "const",
                "continue",
                "declare",
                "default",
                "die",
                "do",
                "echo",
                "else",
                "elseif",
                "empty",
                "enddeclare",
                "endfor",
                "endforeach",
                "endif",
                "endswitch",
                "endwhile",
                "eval",
                "exit",
                "extends",
                "final",
                "finally",
                "fn",
                "for",
                "foreach",
                "function",
                "global",
                "goto",
                "if",
                "implements",
                "include",
                "include_once",
                "instanceof",
                "insteadof",
                "interface",
                "isset",
                "list",
                "match",
                "namespace",
                "new",
                "or",
                "print",
                "private",
                "protected",
                "public",
                "readonly",
                "require",
                "require_once",
                "return",
                "static",
                "switch",
                "throw",
                "trait",
                "try",
                "unset",
                "use",
                "var",
                "while",
                "xor",
                "yield",
                "yield from",
                "true",
                "false",
                "null",
            ][..],
        );

        // Perl keywords
        map.insert(
            "perl",
            &[
                "AUTOLOAD",
                "BEGIN",
                "CHECK",
                "CORE",
                "DESTROY",
                "END",
                "INIT",
                "UNITCHECK",
                "__DATA__",
                "__END__",
                "__FILE__",
                "__LINE__",
                "__PACKAGE__",
                "and",
                "cmp",
                "continue",
                "do",
                "else",
                "elsif",
                "eq",
                "for",
                "foreach",
                "ge",
                "goto",
                "gt",
                "if",
                "last",
                "le",
                "local",
                "lt",
                "my",
                "ne",
                "next",
                "no",
                "not",
                "or",
                "our",
                "package",
                "redo",
                "require",
                "return",
                "state",
                "sub",
                "tie",
                "unless",
                "until",
                "use",
                "wantarray",
                "while",
                "xor",
            ][..],
        );

        // Scala keywords
        map.insert(
            "scala",
            &[
                "abstract",
                "case",
                "catch",
                "class",
                "def",
                "do",
                "else",
                "extends",
                "false",
                "final",
                "finally",
                "for",
                "forSome",
                "if",
                "implicit",
                "import",
                "lazy",
                "match",
                "new",
                "null",
                "object",
                "override",
                "package",
                "private",
                "protected",
                "return",
                "sealed",
                "super",
                "this",
                "throw",
                "trait",
                "try",
                "true",
                "type",
                "val",
                "var",
                "while",
                "with",
                "yield",
            ][..],
        );

        // Haskell keywords
        map.insert(
            "haskell",
            &[
                "as",
                "case",
                "class",
                "data",
                "default",
                "deriving",
                "do",
                "else",
                "family",
                "forall",
                "foreign",
                "hiding",
                "if",
                "import",
                "in",
                "infix",
                "infixl",
                "infixr",
                "instance",
                "let",
                "mdo",
                "module",
                "newtype",
                "of",
                "proc",
                "qualified",
                "rec",
                "then",
                "type",
                "where",
            ][..],
        );

        // OCaml keywords
        map.insert(
            "ocaml",
            &[
                "and",
                "as",
                "assert",
                "asr",
                "begin",
                "class",
                "constraint",
                "do",
                "done",
                "downto",
                "else",
                "end",
                "exception",
                "external",
                "false",
                "for",
                "fun",
                "function",
                "functor",
                "if",
                "in",
                "include",
                "inherit",
                "initializer",
                "land",
                "lazy",
                "let",
                "lor",
                "lsl",
                "lsr",
                "lxor",
                "match",
                "method",
                "mod",
                "module",
                "mutable",
                "new",
                "nonrec",
                "object",
                "of",
                "open",
                "or",
                "private",
                "rec",
                "sig",
                "struct",
                "then",
                "to",
                "true",
                "try",
                "type",
                "val",
                "virtual",
                "when",
                "while",
                "with",
            ][..],
        );

        // Zig keywords
        map.insert(
            "zig",
            &[
                "addrspace",
                "align",
                "allowzero",
                "and",
                "anyframe",
                "anytype",
                "asm",
                "async",
                "await",
                "break",
                "callconv",
                "catch",
                "comptime",
                "const",
                "continue",
                "defer",
                "else",
                "enum",
                "errdefer",
                "error",
                "export",
                "extern",
                "false",
                "fn",
                "for",
                "if",
                "inline",
                "linksection",
                "noalias",
                "nosuspend",
                "null",
                "opaque",
                "or",
                "orelse",
                "packed",
                "pub",
                "resume",
                "return",
                "struct",
                "suspend",
                "switch",
                "test",
                "threadlocal",
                "true",
                "try",
                "undefined",
                "union",
                "unreachable",
                "usingnamespace",
                "var",
                "volatile",
                "while",
            ][..],
        );

        // Pascal / Delphi keywords
        map.insert(
            "pascal",
            &[
                "absolute",
                "and",
                "array",
                "asm",
                "begin",
                "case",
                "const",
                "constructor",
                "destructor",
                "div",
                "do",
                "downto",
                "else",
                "end",
                "file",
                "for",
                "function",
                "goto",
                "if",
                "implementation",
                "in",
                "inherited",
                "inline",
                "interface",
                "label",
                "mod",
                "nil",
                "not",
                "object",
                "of",
                "on",
                "operator",
                "or",
                "packed",
                "procedure",
                "program",
                "record",
                "reintroduce",
                "repeat",
                "self",
                "set",
                "shl",
                "shr",
                "string",
                "then",
                "to",
                "type",
                "unit",
                "until",
                "uses",
                "var",
                "while",
                "with",
                "xor",
            ][..],
        );

        // FORTRAN keywords
        map.insert(
            "fortran",
            &[
                "allocatable",
                "allocate",
                "assign",
                "associate",
                "asynchronous",
                "backspace",
                "bind",
                "block",
                "block data",
                "call",
                "case",
                "character",
                "class",
                "close",
                "codimension",
                "common",
                "complex",
                "concurrent",
                "contains",
                "contiguous",
                "continue",
                "critical",
                "cycle",
                "data",
                "deallocate",
                "default",
                "deferred",
                "dimension",
                "do",
                "double precision",
                "elemental",
                "else",
                "elsewhere",
                "end",
                "endfile",
                "endif",
                "entry",
                "enum",
                "enumerator",
                "equivalence",
                "error",
                "exit",
                "extends",
                "external",
                "final",
                "flush",
                "forall",
                "format",
                "function",
                "generic",
                "go to",
                "goto",
                "if",
                "images",
                "implicit",
                "import",
                "impure",
                "in",
                "include",
                "inout",
                "inquire",
                "integer",
                "intent",
                "interface",
                "intrinsic",
                "kind",
                "len",
                "lock",
                "logical",
                "module",
                "name",
                "namelist",
                "non_overridable",
                "none",
                "nopass",
                "nullify",
                "only",
                "open",
                "operator",
                "optional",
                "out",
                "parameter",
                "pass",
                "pause",
                "pointer",
                "print",
                "private",
                "procedure",
                "program",
                "protected",
                "public",
                "pure",
                "read",
                "real",
                "recursive",
                "result",
                "return",
                "rewind",
                "save",
                "select",
                "sequence",
                "stop",
                "submodule",
                "subroutine",
                "sync",
                "target",
                "then",
                "to",
                "type",
                "unlock",
                "use",
                "value",
                "volatile",
                "wait",
                "where",
                "while",
                "write",
            ][..],
        );

        // COBOL keywords (major ones)
        map.insert(
            "cobol",
            &[
                "accept",
                "add",
                "advancing",
                "after",
                "all",
                "alphabetic",
                "also",
                "alter",
                "alternate",
                "and",
                "are",
                "area",
                "ascending",
                "assign",
                "at",
                "before",
                "blank",
                "block",
                "bottom",
                "by",
                "call",
                "cancel",
                "cd",
                "cf",
                "ch",
                "character",
                "characters",
                "class",
                "clock-units",
                "close",
                "cobol",
                "code",
                "collating",
                "column",
                "comma",
                "communication",
                "comp",
                "computational",
                "compute",
                "configuration",
                "contains",
                "content",
                "continue",
                "control",
                "copy",
                "corresponding",
                "count",
                "currency",
                "data",
                "date",
                "day",
                "de",
                "debugging",
                "decimal-point",
                "declaratives",
                "delete",
                "delimited",
                "delimiter",
                "depending",
                "descending",
                "destination",
                "detail",
                "disable",
                "display",
                "divide",
                "division",
                "down",
                "duplicates",
                "dynamic",
            ][..],
        );

        // Ada keywords
        map.insert(
            "ada",
            &[
                "abort",
                "abs",
                "abstract",
                "accept",
                "access",
                "aliased",
                "all",
                "and",
                "array",
                "at",
                "begin",
                "body",
                "case",
                "constant",
                "declare",
                "delay",
                "delta",
                "digits",
                "do",
                "else",
                "elsif",
                "end",
                "entry",
                "exception",
                "exit",
                "for",
                "function",
                "generic",
                "goto",
                "if",
                "in",
                "interface",
                "is",
                "limited",
                "loop",
                "mod",
                "new",
                "not",
                "null",
                "of",
                "or",
                "others",
                "out",
                "overriding",
                "package",
                "parallel",
                "pragma",
                "private",
                "procedure",
                "protected",
                "raise",
                "range",
                "record",
                "rem",
                "renames",
                "requeue",
                "return",
                "reverse",
                "select",
                "separate",
                "some",
                "subtype",
                "synchronized",
                "tagged",
                "task",
                "terminate",
                "then",
                "type",
                "until",
                "use",
                "when",
                "while",
                "with",
                "xor",
            ][..],
        );

        // Visual Basic keywords
        map.insert(
            "vb",
            &[
                "AddHandler",
                "AddressOf",
                "Alias",
                "And",
                "AndAlso",
                "As",
                "Boolean",
                "ByRef",
                "Byte",
                "ByVal",
                "Call",
                "Case",
                "Catch",
                "CBool",
                "CByte",
                "CChar",
                "CDate",
                "CDbl",
                "CDec",
                "Char",
                "CInt",
                "Class",
                "CLng",
                "CObj",
                "Const",
                "Continue",
                "CSByte",
                "CShort",
                "CSng",
                "CStr",
                "CType",
                "CUInt",
                "CULng",
                "CUShort",
                "Date",
                "Decimal",
                "Declare",
                "Default",
                "Delegate",
                "Dim",
                "DirectCast",
                "Do",
                "Double",
                "Each",
                "Else",
                "ElseIf",
                "End",
                "EndIf",
                "Enum",
                "Erase",
                "Error",
                "Event",
                "Exit",
                "False",
                "Finally",
                "For",
                "Friend",
                "Function",
                "Get",
                "GetType",
                "GetXMLNamespace",
                "Global",
                "GoSub",
                "GoTo",
                "Handles",
                "If",
                "Implements",
                "Imports",
                "In",
                "Inherits",
                "Integer",
                "Interface",
                "Is",
                "IsNot",
                "Let",
                "Lib",
                "Like",
                "Long",
                "Loop",
                "Me",
                "Mod",
                "Module",
                "MustInherit",
                "MustOverride",
                "MyBase",
                "MyClass",
                "Namespace",
                "Narrowing",
                "New",
                "Next",
                "Not",
                "Nothing",
                "NotInheritable",
                "NotOverridable",
                "Object",
                "Of",
                "On",
                "Operator",
                "Option",
                "Optional",
                "Or",
                "OrElse",
                "Overloads",
                "Overridable",
                "Overrides",
                "ParamArray",
                "Partial",
                "Private",
                "Property",
                "Protected",
                "Public",
                "RaiseEvent",
                "ReadOnly",
                "ReDim",
                "REM",
                "RemoveHandler",
                "Resume",
                "Return",
                "SByte",
                "Select",
                "Set",
                "Shadows",
                "Shared",
                "Short",
                "Single",
                "Static",
                "Step",
                "Stop",
                "String",
                "Structure",
                "Sub",
                "SyncLock",
                "Then",
                "Throw",
                "To",
                "True",
                "Try",
                "TryCast",
                "TypeOf",
                "UInteger",
                "ULong",
                "UShort",
                "Using",
                "Variant",
                "Wend",
                "When",
                "While",
                "Widening",
                "With",
                "WithEvents",
                "WriteOnly",
                "Xor",
            ][..],
        );

        // R keywords
        map.insert(
            "r",
            &[
                "break",
                "else",
                "for",
                "function",
                "if",
                "in",
                "next",
                "repeat",
                "return",
                "while",
                "TRUE",
                "FALSE",
                "NULL",
                "Inf",
                "NaN",
                "NA",
                "NA_integer_",
                "NA_real_",
                "NA_complex_",
                "NA_character_",
            ][..],
        );

        // Erlang keywords
        map.insert(
            "erlang",
            &[
                "after", "and", "andalso", "band", "begin", "bnot", "bor", "bsl", "bsr", "bxor",
                "case", "catch", "cond", "div", "end", "fun", "if", "let", "not", "of", "or",
                "orelse", "receive", "rem", "try", "when", "xor",
            ][..],
        );

        // Elixir keywords
        map.insert(
            "elixir",
            &[
                "after", "and", "catch", "do", "else", "end", "false", "fn", "for", "if", "import",
                "in", "nil", "not", "or", "quote", "raise", "receive", "rescue", "true", "try",
                "unless", "unquote", "when", "with",
            ][..],
        );

        // Node.js / JavaScript keywords
        map.insert(
            "javascript",
            &[
                "await",
                "break",
                "case",
                "catch",
                "class",
                "const",
                "continue",
                "debugger",
                "default",
                "delete",
                "do",
                "else",
                "enum",
                "export",
                "extends",
                "false",
                "finally",
                "for",
                "function",
                "if",
                "implements",
                "import",
                "in",
                "instanceof",
                "interface",
                "let",
                "new",
                "null",
                "package",
                "private",
                "protected",
                "public",
                "return",
                "static",
                "super",
                "switch",
                "this",
                "throw",
                "true",
                "try",
                "typeof",
                "undefined",
                "var",
                "void",
                "while",
                "with",
                "yield",
            ][..],
        );

        // PowerShell keywords
        map.insert(
            "powershell",
            &[
                "begin",
                "break",
                "catch",
                "class",
                "continue",
                "data",
                "define",
                "do",
                "dynamicparam",
                "else",
                "elseif",
                "end",
                "enum",
                "exit",
                "filter",
                "finally",
                "for",
                "foreach",
                "from",
                "function",
                "hidden",
                "if",
                "in",
                "param",
                "process",
                "return",
                "static",
                "switch",
                "throw",
                "trap",
                "try",
                "until",
                "using",
                "var",
                "while",
            ][..],
        );

        map
    }

    /// Check if a name is a reserved keyword in any target language
    /// Returns a list of languages where this name is reserved
    pub fn check_reserved(name: &str) -> Vec<&'static str> {
        let keywords = get_all_keywords();
        let mut conflicts = Vec::new();

        let name_lower = name.to_lowercase();

        for (lang, reserved) in keywords {
            for kw in reserved.iter() {
                // Case-insensitive comparison for most languages
                // Some languages are case-sensitive, but for safety we check lowercase
                if kw.to_lowercase() == name_lower {
                    conflicts.push(lang);
                    break;
                }
            }
        }

        conflicts.sort();
        conflicts.dedup();
        conflicts
    }
}

/// Check all names in api.json for reserved keyword conflicts
pub fn check_reserved_keywords(api_data: &ApiData) -> Vec<FfiSafetyWarning> {
    use reserved_keywords::check_reserved;

    let mut warnings = Vec::new();

    for (_version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Check type name
                let conflicts = check_reserved(class_name);
                if !conflicts.is_empty() {
                    warnings.push(FfiSafetyWarning {
                        type_name: class_name.clone(),
                        file_path: format!("api.json - {}.{}", module_name, class_name),
                        kind: FfiSafetyWarningKind::ReservedKeyword {
                            name_kind: "type".to_string(),
                            name: class_name.clone(),
                            languages: conflicts.into_iter().map(|s| s.to_string()).collect(),
                        },
                    });
                }

                // Check struct fields
                if let Some(struct_fields) = &class_data.struct_fields {
                    for field_map in struct_fields {
                        for (field_name, _field_data) in field_map {
                            let conflicts = check_reserved(field_name);
                            if !conflicts.is_empty() {
                                warnings.push(FfiSafetyWarning {
                                    type_name: class_name.clone(),
                                    file_path: format!(
                                        "api.json - {}.{}.{}",
                                        module_name, class_name, field_name
                                    ),
                                    kind: FfiSafetyWarningKind::ReservedKeyword {
                                        name_kind: "field".to_string(),
                                        name: field_name.to_string(),
                                        languages: conflicts
                                            .into_iter()
                                            .map(|s| s.to_string())
                                            .collect(),
                                    },
                                });
                            }
                        }
                    }
                }

                // NOTE: We skip enum variants because they always have a type prefix
                // (e.g., MouseCursorType::Default becomes MouseCursorType_Default in C)
                // so reserved keywords are not problematic there.

                // Check constructor names
                if let Some(constructors) = &class_data.constructors {
                    for (ctor_name, _ctor_data) in constructors {
                        // Special case: "default" constructor should use custom_impls: ["Default"] instead
                        if ctor_name == "default" {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: format!("api.json - {}.{}::{}", module_name, class_name, ctor_name),
                                kind: FfiSafetyWarningKind::BadConstructorName {
                                    name: ctor_name.clone(),
                                    suggestion: "Use custom_impls: [\"Default\"] or derive: [\"Default\"] instead. The codegen will automatically generate a _default() function.".to_string(),
                                },
                            });
                            continue;
                        }

                        let conflicts = check_reserved(ctor_name);
                        if !conflicts.is_empty() {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: format!(
                                    "api.json - {}.{}::{}",
                                    module_name, class_name, ctor_name
                                ),
                                kind: FfiSafetyWarningKind::ReservedKeyword {
                                    name_kind: "constructor".to_string(),
                                    name: ctor_name.clone(),
                                    languages: conflicts
                                        .into_iter()
                                        .map(|s| s.to_string())
                                        .collect(),
                                },
                            });
                        }
                    }
                }

                // Check function names
                if let Some(functions) = &class_data.functions {
                    for (fn_name, _fn_data) in functions {
                        let conflicts = check_reserved(fn_name);
                        if !conflicts.is_empty() {
                            warnings.push(FfiSafetyWarning {
                                type_name: class_name.clone(),
                                file_path: format!(
                                    "api.json - {}.{}::{}",
                                    module_name, class_name, fn_name
                                ),
                                kind: FfiSafetyWarningKind::ReservedKeyword {
                                    name_kind: "function".to_string(),
                                    name: fn_name.clone(),
                                    languages: conflicts
                                        .into_iter()
                                        .map(|s| s.to_string())
                                        .collect(),
                                },
                            });
                        }
                    }
                }
            }
        }
    }

    warnings
}
