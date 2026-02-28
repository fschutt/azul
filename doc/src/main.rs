//! Main entry point for the documentation generation and management tool.
#![allow(unused)]
use std::{env, fs, path::PathBuf};

use anyhow::Context;
use dllgen::deploy::Config;
use reftest::RunRefTestsConfig;
use serde::Serialize;

pub mod api;
pub mod autofix;
pub mod codegen;
pub mod dllgen;
pub mod docgen;
pub mod patch;
pub mod print;
pub mod reftest;
pub mod spec;
pub mod utils;

/// Serialize to pretty JSON with 4-space indentation (matching the original api.json format).
fn to_json_pretty_4space<T: serde::Serialize>(value: &T) -> serde_json::Result<String> {
    let mut buf = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"    ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    value.serialize(&mut ser)?;
    // serde_json always produces valid UTF-8
    Ok(String::from_utf8(buf).unwrap())
}

fn main() -> anyhow::Result<()> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = PathBuf::from(manifest_dir);
    let project_root = manifest_path.parent().unwrap().to_path_buf();
    let api_path = project_root.join("api.json");

    let _ = std::env::set_current_dir(manifest_dir);

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let args = args.iter().map(|s| s.as_str()).collect::<Vec<&str>>();

    match &args[1..] {
        ["print"] | ["print", ..] => {
            let api_json = load_api_json(&api_path)?;
            return print::handle_print_command(&api_json, &args[2..]);
        }
        ["normalize"] => {
            println!("[REFRESH] Normalizing api.json...\n");

            // Read original content first
            let original_content = fs::read_to_string(&api_path)?;
            let mut api_data = load_api_json(&api_path)?;

            // Normalize array types: [T; N] -> type: T, arraysize: N
            let array_count = api::normalize_array_types(&mut api_data);
            if array_count > 0 {
                println!("[ARRAY] Normalized {} array type fields", array_count);
            }

            // Normalize type_alias: "*mut c_void" -> target: "c_void", ref_kind: "mutptr"
            let type_alias_count = api::normalize_type_aliases(&mut api_data);
            if type_alias_count > 0 {
                println!(
                    "[TYPE_ALIAS] Normalized {} type alias entries",
                    type_alias_count
                );
            }

            // Normalize enum variant types: "*mut T" -> type: "T", ref_kind: "mutptr"
            let enum_variant_count = api::normalize_enum_variant_types(&mut api_data);
            if enum_variant_count > 0 {
                println!(
                    "[ENUM_VARIANT] Normalized {} enum variant type entries",
                    enum_variant_count
                );
            }

            let api_json = to_json_pretty_4space(&api_data)?;

            // Only write if content actually changed
            if api_json != original_content {
                fs::write(&api_path, api_json)?;
                println!("[SAVE] Saved normalized api.json\n");
            } else {
                println!("[OK] api.json already normalized, no changes needed\n");
            }
            return Ok(());
        }
        ["dedup"] => {
            println!("[DEDUP] Removing duplicate types from api.json...\n");
            let mut api_data = load_api_json(&api_path)?;
            let removed = patch::remove_duplicate_types(&mut api_data);
            if removed > 0 {
                let api_json = to_json_pretty_4space(&api_data)?;
                fs::write(&api_path, api_json)?;
                println!("\n[OK] Removed {} duplicate types", removed);
                println!("[SAVE] Saved updated api.json\n");
            } else {
                println!("[OK] No duplicates found\n");
            }
            return Ok(());
        }
        ["autofix"] | ["autofix", "run"] => {
            let output_dir = project_root.join("target").join("autofix");
            let api_data = load_api_json(&api_path)?;
            autofix::autofix_api(&api_data, &project_root, &output_dir, true)?;
            return Ok(());
        }
        ["discover"] => {
            // Discover all public functions in the workspace index
            // Prints one line per function: TypeName.method_name
            let index = autofix::type_index::TypeIndex::build(&project_root, false)?;

            let mut all_types: Vec<_> = index.iter_all().collect();
            all_types.sort_by_key(|(name, _)| *name);

            for (type_name, type_defs) in all_types {
                for type_def in type_defs {
                    for method in &type_def.methods {
                        if method.is_public {
                            let self_str = match &method.self_kind {
                                None => "static",
                                Some(autofix::type_index::SelfKind::Value) => "self",
                                Some(autofix::type_index::SelfKind::Ref) => "&self",
                                Some(autofix::type_index::SelfKind::RefMut) => "&mut self",
                            };
                            let ret_str = method.return_type.as_deref().unwrap_or("()");
                            println!(
                                "{}.{} ({}) -> {}",
                                type_name, method.name, self_str, ret_str
                            );
                        }
                    }
                }
            }
            return Ok(());
        }
        ["discover", pattern] => {
            // Discover functions matching a pattern (e.g., "Dom" or "Callback")
            let index = autofix::type_index::TypeIndex::build(&project_root, false)?;

            let pattern_lower = pattern.to_lowercase();
            let mut all_types: Vec<_> = index
                .iter_all()
                .filter(|(name, _)| name.to_lowercase().contains(&pattern_lower))
                .collect();
            all_types.sort_by_key(|(name, _)| *name);

            if all_types.is_empty() {
                println!("No types found matching '{}'", pattern);
                return Ok(());
            }

            for (type_name, type_defs) in all_types {
                for type_def in type_defs {
                    for method in &type_def.methods {
                        if method.is_public {
                            let self_str = match &method.self_kind {
                                None => "static",
                                Some(autofix::type_index::SelfKind::Value) => "self",
                                Some(autofix::type_index::SelfKind::Ref) => "&self",
                                Some(autofix::type_index::SelfKind::RefMut) => "&mut self",
                            };
                            let ret_str = method.return_type.as_deref().unwrap_or("()");
                            println!(
                                "{}.{} ({}) -> {}",
                                type_name, method.name, self_str, ret_str
                            );
                        }
                    }
                }
            }
            return Ok(());
        }
        ["autofix", "debug", "type", type_name] => {
            // Debug a specific type in the workspace index
            let index = autofix::type_index::TypeIndex::build(&project_root, true)?;
            autofix::debug::debug_type_in_index(&index, type_name);
            return Ok(());
        }
        ["autofix", "debug", "chain", type_name] => {
            // Debug type resolution chain
            let index = autofix::type_index::TypeIndex::build(&project_root, true)?;
            autofix::debug::debug_resolve_type_chain(&index, type_name);
            return Ok(());
        }
        ["autofix", "debug", "api", type_name] => {
            // Debug a specific type from api.json against workspace
            let api_data = load_api_json(&api_path)?;
            let index = autofix::type_index::TypeIndex::build(&project_root, true)?;
            autofix::debug::debug_api_type(&index, &api_data, type_name);
            return Ok(());
        }
        ["autofix", "debug", "file", file_path] => {
            // Debug parsing a specific file
            autofix::debug::debug_parse_file(std::path::Path::new(file_path))?;
            return Ok(());
        }
        ["autofix", "debug", "difficult"] | ["autofix", "difficult"] => {
            // Analyze and rank types by FFI difficulty
            let api_data = load_api_json(&api_path)?;
            autofix::debug::analyze_ffi_difficulty(&api_data);
            return Ok(());
        }
        ["autofix", "debug", "internal"] | ["autofix", "internal"] => {
            // Show types that should be internal-only
            let api_data = load_api_json(&api_path)?;
            autofix::debug::show_internal_only_types(&api_data);
            return Ok(());
        }
        ["autofix", "difficult", "remove", items @ ..] if !items.is_empty() => {
            // Remove multiple functions/types from api.json
            // Usage: autofix difficult remove ImageRef.get_data ImageRef.into_inner DecodedImage

            // Clear the patches folder to avoid stale patches
            let patches_dir = project_root.join("target").join("autofix").join("patches");
            if patches_dir.exists() {
                let _ = fs::remove_dir_all(&patches_dir);
            }
            fs::create_dir_all(&patches_dir)?;

            let api_data = load_api_json(&api_path)?;

            // Get the latest version
            let version = api_data
                .get_latest_version_str()
                .ok_or_else(|| anyhow::anyhow!("No versions in api.json"))?
                .to_string();

            let version_data = api_data
                .get_version(&version)
                .ok_or_else(|| anyhow::anyhow!("Version not found"))?;

            println!(
                "[REMOVE] Generating patches to remove {} items...\n",
                items.len()
            );

            let mut patch_count = 0;

            for item in items {
                if item.contains('.') {
                    // It's a function: TypeName.method
                    let parts: Vec<&str> = item.split('.').collect();
                    let (type_name, method_name) = if parts.len() == 2 {
                        (parts[0], parts[1])
                    } else {
                        (parts[parts.len() - 2], parts[parts.len() - 1])
                    };

                    if let Some(module_name) =
                        autofix::function_diff::find_type_module(type_name, version_data)
                    {
                        println!(
                            "  - {}.{} (from {} module)",
                            type_name, method_name, module_name
                        );

                        let patch = autofix::function_diff::generate_remove_functions_patch(
                            type_name,
                            &[method_name],
                            module_name,
                            &version,
                        );

                        let patch_filename = format!(
                            "remove_{}_{}.patch.json",
                            type_name.to_lowercase(),
                            method_name
                        );
                        let patch_path = patches_dir.join(&patch_filename);
                        let json = serde_json::to_string_pretty(&patch)?;
                        fs::write(&patch_path, &json)?;
                        patch_count += 1;
                    } else {
                        eprintln!("  [WARN] Type '{}' not found in api.json", type_name);
                    }
                } else {
                    // It's a type name - remove the entire type
                    if let Some(module_name) =
                        autofix::function_diff::find_type_module(item, version_data)
                    {
                        println!("  - {} (entire type from {} module)", item, module_name);

                        let patch = autofix::function_diff::generate_remove_type_patch(
                            item,
                            module_name,
                            &version,
                        );

                        let patch_filename = format!("remove_{}.patch.json", item.to_lowercase());
                        let patch_path = patches_dir.join(&patch_filename);
                        let json = serde_json::to_string_pretty(&patch)?;
                        fs::write(&patch_path, &json)?;
                        patch_count += 1;
                    } else {
                        eprintln!("  [WARN] Type '{}' not found in api.json", item);
                    }
                }
            }

            if patch_count > 0 {
                println!(
                    "\n[OK] {} patches written to: {}",
                    patch_count,
                    patches_dir.display()
                );
                println!("\n\x1b[1;33mIMPORTANT\x1b[0m: Apply patches immediately or they may become stale:");
                println!(
                    "  cargo run --bin azul-doc -- autofix apply {}",
                    patches_dir.display()
                );
                println!("\nTo preview changes without applying:");
                println!("  cargo run --bin azul-doc -- autofix explain");
            } else {
                println!("\n[WARN] No patches generated - items not found in api.json");
            }

            return Ok(());
        }
        ["autofix", "debug", "modules"] | ["autofix", "modules"] => {
            // Show types in wrong modules
            let api_data = load_api_json(&api_path)?;
            autofix::debug::show_wrong_module_types(&api_data);
            return Ok(());
        }
        ["autofix", "debug", "deps"] | ["autofix", "deps"] => {
            // Analyze function dependencies on difficult/internal types
            let api_data = load_api_json(&api_path)?;
            autofix::debug::analyze_function_dependencies(&api_data);
            return Ok(());
        }
        ["autofix", "explain"] => {
            let patches_dir = project_root.join("target").join("autofix").join("patches");

            if !patches_dir.exists() {
                eprintln!("No patches found. Run 'azul-doc autofix' first.");
                std::process::exit(1);
            }

            patch::explain_patches(&patches_dir)?;
            return Ok(());
        }
        // function management commands
        ["autofix", "list", type_spec] => {
            // List functions for a type: autofix list Dom
            // Or with module prefix: autofix list dom.Dom
            let api_data = load_api_json(&api_path)?;
            let index = autofix::type_index::TypeIndex::build(&project_root, false)?;

            // Get the latest version
            let version = api_data
                .get_latest_version_str()
                .ok_or_else(|| anyhow::anyhow!("No versions in api.json"))?;

            // Parse type_spec: could be "TypeName" or "module.TypeName"
            let type_name = if type_spec.contains('.') {
                type_spec.rsplit('.').next().unwrap_or(type_spec)
            } else {
                type_spec
            };

            match autofix::function_diff::list_type_functions(type_name, &index, &api_data, version)
            {
                Ok(result) => {
                    println!("\n=== Functions for {} ===\n", result.type_name);

                    if !result.source_only.is_empty() {
                        println!(
                            "[ INFO ] In source only ({} - need to add to api.json):",
                            result.source_only.len()
                        );
                        for name in &result.source_only {
                            println!("  + {}", name);
                        }
                        println!();
                    }

                    if !result.api_only.is_empty() {
                        println!(
                            "[ INFO ] In api.json only ({} - may be stale):",
                            result.api_only.len()
                        );
                        for name in &result.api_only {
                            println!("  - {}", name);
                        }
                        println!();
                    }

                    if !result.both.is_empty() {
                        println!("✓ In both ({}):", result.both.len());
                        for name in &result.both {
                            println!("  = {}", name);
                        }
                        println!();
                    }

                    println!(
                        "Summary: {} source-only, {} api-only, {} matching",
                        result.source_only.len(),
                        result.api_only.len(),
                        result.both.len()
                    );
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        ["autofix", "add", fn_spec] => {
            // Add function(s) to api.json: autofix add Dom.add_callback
            // Or with wildcard: autofix add Dom.*
            // Also automatically adds the type if it's not in api.json yet

            // Clear the patches folder to avoid stale patches
            let patches_dir = project_root.join("target").join("autofix").join("patches");
            if patches_dir.exists() {
                let _ = fs::remove_dir_all(&patches_dir);
            }
            fs::create_dir_all(&patches_dir)?;

            let api_data = load_api_json(&api_path)?;
            let index = autofix::type_index::TypeIndex::build(&project_root, false)?;

            // Get the latest version
            let version = api_data
                .get_latest_version_str()
                .ok_or_else(|| anyhow::anyhow!("No versions in api.json"))?
                .to_string();

            // Parse fn_spec: "TypeName.method" or "TypeName.*" or "module.TypeName.method"
            let parts: Vec<&str> = fn_spec.split('.').collect();

            if parts.len() < 2 {
                eprintln!(
                    "Error: Invalid format. Use: TypeName.method or TypeName.* or \
                     module.TypeName.method"
                );
                std::process::exit(1);
            }

            let (type_name, method_spec) = if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                // module.TypeName.method - ignore the module prefix, we'll determine it
                // automatically
                (parts[parts.len() - 2], parts[parts.len() - 1])
            };

            // Find the type in source
            let type_def = match index.resolve(type_name, None) {
                Some(t) => t,
                None => {
                    eprintln!("Error: Type '{}' not found in source code", type_name);
                    std::process::exit(1);
                }
            };

            // Find which module the type is in (from api.json), or determine automatically
            let version_data = api_data
                .get_version(&version)
                .ok_or_else(|| anyhow::anyhow!("Version not found"))?;

            let type_exists = autofix::function_diff::type_exists_in_api(type_name, version_data);

            if !type_exists {
                // Type doesn't exist in api.json - use the new function to add it with dependencies
                println!(
                    "[ADD] Type '{}' not found in api.json, adding with transitive \
                     dependencies...\n",
                    type_name
                );

                let method_spec_opt = Some(method_spec);

                match autofix::function_diff::generate_add_type_patches(
                    type_name,
                    method_spec_opt,
                    &index,
                    version_data,
                    &version,
                ) {
                    Ok((patches, result)) => {
                        // Show what will be added
                        println!("Types to add:");
                        for (ty, module) in &result.added_types {
                            println!("  + {} (-> {} module)", ty, module);
                        }

                        if !result.skipped_types.is_empty() {
                            println!("\nTypes already in api.json (skipped):");
                            for ty in &result.skipped_types {
                                println!("  - {}", ty);
                            }
                        }

                        if !result.missing_types.is_empty() {
                            println!("\n[WARN] Types not found in workspace:");
                            for ty in &result.missing_types {
                                println!("  ? {}", ty);
                            }
                        }

                        if !result.added_methods.is_empty() {
                            println!("\nMethods to add to {}:", type_name);
                            for m in &result.added_methods {
                                println!("  + {}", m);
                            }
                        }

                        // Write patches to files
                        let patches_dir =
                            project_root.join("target").join("autofix").join("patches");
                        fs::create_dir_all(&patches_dir)?;

                        for (i, patch) in patches.iter().enumerate() {
                            let patch_filename =
                                format!("add_{}_{}.patch.json", type_name.to_lowercase(), i);
                            let patch_path = patches_dir.join(&patch_filename);

                            let json = patch
                                .to_json()
                                .unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                            fs::write(&patch_path, &json)?;
                        }

                        // Also generate the functions patch if methods were requested
                        if !result.added_methods.is_empty() {
                            let methods: Vec<_> = type_def
                                .methods
                                .iter()
                                .filter(|m| m.is_public)
                                .filter(|m| method_spec == "*" || m.name == method_spec)
                                .collect();

                            let func_patch = autofix::function_diff::generate_add_functions_patch(
                                type_name,
                                &methods,
                                &result.primary_module,
                                &version,
                                type_def,
                            );

                            let patch_filename =
                                format!("add_{}_functions.patch.json", type_name.to_lowercase());
                            let patch_path = patches_dir.join(&patch_filename);
                            let json = serde_json::to_string_pretty(&func_patch)?;
                            fs::write(&patch_path, &json)?;
                        }

                        println!(
                            "\n[OK] {} patches written to: {}",
                            patches.len() + 1,
                            patches_dir.display()
                        );
                        println!("\n\x1b[1;33mIMPORTANT\x1b[0m: Apply patches immediately or they may become stale:");
                        println!(
                            "  cargo run --bin azul-doc -- autofix apply {}",
                            patches_dir.display()
                        );
                        println!("\nTo preview changes without applying:");
                        println!("  cargo run --bin azul-doc -- autofix explain");
                    }
                    Err(e) => {
                        eprintln!("Error generating patches: {}", e);
                        std::process::exit(1);
                    }
                }

                return Ok(());
            }

            // Type exists - just add the functions (original behavior)
            let module_name = autofix::function_diff::find_type_module(type_name, version_data)
                .unwrap()
                .to_string();

            // Get matching methods
            let methods: Vec<_> = type_def
                .methods
                .iter()
                .filter(|m| m.is_public)
                .filter(|m| method_spec == "*" || m.name == method_spec)
                .collect();

            if methods.is_empty() {
                println!(
                    "No matching public methods found for '{}.{}'",
                    type_name, method_spec
                );
                return Ok(());
            }

            println!(
                "[ADD] Generating patch to add {} function(s) to {}\n",
                methods.len(),
                type_name
            );

            // Show what will be added
            for m in &methods {
                let self_str = match &m.self_kind {
                    None => "static",
                    Some(autofix::type_index::SelfKind::Value) => "self",
                    Some(autofix::type_index::SelfKind::Ref) => "&self",
                    Some(autofix::type_index::SelfKind::RefMut) => "&mut self",
                };
                let ret_str = m.return_type.as_deref().unwrap_or("()");
                let ctor_str = if m.is_constructor {
                    " [constructor]"
                } else {
                    ""
                };
                println!("  + fn {}({}) -> {}{}", m.name, self_str, ret_str, ctor_str);
            }

            // Generate the patch
            let patch = autofix::function_diff::generate_add_functions_patch(
                type_name,
                &methods,
                &module_name,
                &version,
                type_def,
            );

            // Write patch to file
            let patches_dir = project_root.join("target").join("autofix").join("patches");
            fs::create_dir_all(&patches_dir)?;

            let patch_filename = format!(
                "add_{}_{}.patch.json",
                type_name.to_lowercase(),
                if method_spec == "*" {
                    "all"
                } else {
                    method_spec
                }
            );
            let patch_path = patches_dir.join(&patch_filename);

            let json = serde_json::to_string_pretty(&patch)?;
            fs::write(&patch_path, &json)?;

            println!("\n[OK] Patch written to: {}", patch_path.display());
            println!(
                "\n\x1b[1;33mIMPORTANT\x1b[0m: Apply patches immediately or they may become stale:"
            );
            println!(
                "  cargo run --bin azul-doc -- autofix apply {}",
                patches_dir.display()
            );
            println!("\nTo preview changes without applying:");
            println!("  cargo run --bin azul-doc -- autofix explain");

            return Ok(());
        }
        ["autofix", "remove", fn_spec] => {
            // Remove function from api.json: autofix remove Dom.some_function

            // Clear the patches folder to avoid stale patches
            let patches_dir = project_root.join("target").join("autofix").join("patches");
            if patches_dir.exists() {
                let _ = fs::remove_dir_all(&patches_dir);
            }
            fs::create_dir_all(&patches_dir)?;

            let api_data = load_api_json(&api_path)?;

            // Get the latest version
            let version = api_data
                .get_latest_version_str()
                .ok_or_else(|| anyhow::anyhow!("No versions in api.json"))?
                .to_string();

            // Parse fn_spec
            let parts: Vec<&str> = fn_spec.split('.').collect();

            if parts.len() < 2 {
                eprintln!("Error: Invalid format. Use: TypeName.method or module.TypeName.method");
                std::process::exit(1);
            }

            let (type_name, method_name) = if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                (parts[parts.len() - 2], parts[parts.len() - 1])
            };

            // Find which module the type is in (from api.json)
            let version_data = api_data
                .get_version(&version)
                .ok_or_else(|| anyhow::anyhow!("Version not found"))?;
            let module_name = autofix::function_diff::find_type_module(type_name, version_data)
                .ok_or_else(|| anyhow::anyhow!("Type '{}' not found in api.json", type_name))?
                .to_string();

            println!(
                "[REMOVE] Generating patch to remove '{}' from {}\n",
                method_name, type_name
            );

            // Generate the patch
            let patch = autofix::function_diff::generate_remove_functions_patch(
                type_name,
                &[method_name],
                &module_name,
                &version,
            );

            // Write patch to file
            let patch_filename = format!(
                "remove_{}_{}.patch.json",
                type_name.to_lowercase(),
                method_name
            );
            let patch_path = patches_dir.join(&patch_filename);

            let json = serde_json::to_string_pretty(&patch)?;
            fs::write(&patch_path, &json)?;

            println!("[OK] Patch written to: {}", patch_path.display());
            println!(
                "\n\x1b[1;33mIMPORTANT\x1b[0m: Apply patches immediately or they may become stale:"
            );
            println!(
                "  cargo run --bin azul-doc -- autofix apply {}",
                patches_dir.display()
            );
            println!("\nTo preview changes without applying:");
            println!("  cargo run --bin azul-doc -- autofix explain");

            return Ok(());
        }
        ["unused"] => {
            println!("[SEARCH] Finding unused types in api.json (recursive analysis)...\n");
            let api_data = load_api_json(&api_path)?;
            let unused_types = api::find_all_unused_types_recursive(&api_data);

            if unused_types.is_empty() {
                println!(
                    "[OK] No unused types found. All types are reachable from the public API."
                );
            } else {
                println!("[WARN] Found {} unused types:\n", unused_types.len());

                // Group by module for better readability
                let mut by_module: std::collections::BTreeMap<String, Vec<String>> =
                    std::collections::BTreeMap::new();
                for info in &unused_types {
                    by_module
                        .entry(info.module_name.clone())
                        .or_default()
                        .push(info.type_name.clone());
                }

                for (module, types) in &by_module {
                    println!("  Module `{}`:", module);
                    for type_name in types {
                        println!("    - {}", type_name);
                    }
                    println!();
                }

                println!("To generate removal patches, run: azul-doc unused patch");
            }
            return Ok(());
        }
        ["unused", "patch"] => {
            println!("[SEARCH] Generating patches to remove unused types (recursive)...\n");
            let api_data = load_api_json(&api_path)?;
            let unused_types = api::find_all_unused_types_recursive(&api_data);

            if unused_types.is_empty() {
                println!("[OK] No unused types found. Nothing to patch.");
                return Ok(());
            }

            let patches_dir = project_root.join("target").join("unused_types_patches");

            // Clean existing patches directory
            if patches_dir.exists() {
                fs::remove_dir_all(&patches_dir)?;
            }
            fs::create_dir_all(&patches_dir)?;

            // Generate removal patches using the new API function
            let patches = api::generate_removal_patches(&unused_types);

            // Write each patch to a file (one per module)
            for (idx, patch) in patches.iter().enumerate() {
                // Extract module name from the patch for the filename
                let (module_name, type_count) = patch
                    .versions
                    .values()
                    .flat_map(|v| v.modules.iter())
                    .next()
                    .map(|(m, mp)| (m.clone(), mp.classes.len()))
                    .unwrap_or_else(|| (format!("patch_{}", idx), 0));

                let patch_filename = format!("{:03}_remove_{}.patch.json", idx, module_name);
                let patch_path = patches_dir.join(&patch_filename);

                let json = serde_json::to_string_pretty(&patch)?;
                fs::write(&patch_path, json)?;

                println!("  [PATCH] {} ({} types)", patch_filename, type_count);
            }

            println!(
                "\n[OK] Generated {} removal patch files for {} types in:",
                patches.len(),
                unused_types.len()
            );
            println!("     {}", patches_dir.display());
            println!("\nTo review a patch:");
            println!("  cat {}/*.patch.json", patches_dir.display());
            println!("\nTo apply the patches:");
            println!("  cargo run -- autofix apply {}", patches_dir.display());

            return Ok(());
        }
        ["autofix", "apply", "safe", patch_dir] | ["patch", "safe", patch_dir] => {
            println!("[FIX] Applying safe (path-only) patches to api.json...\n");

            // Load API data (need mutable copy for patching)
            let api_json_str = fs::read_to_string(&api_path)
                .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
            let mut api_data =
                api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;

            // Get project root (parent of doc/) for resolving paths
            let patch_path = PathBuf::from(&args[3]);
            let patch_path = if patch_path.is_absolute() {
                patch_path
            } else {
                // Try relative to project root first
                let project_relative = project_root.join(&patch_path);
                if project_relative.exists() {
                    project_relative
                } else {
                    // Fall back to current dir (doc/)
                    patch_path
                }
            };

            if !patch_path.is_dir() {
                anyhow::bail!("Path must be a directory: {}", patch_path.display());
            }

            // Apply only path-only patches and delete them
            let stats = patch::apply_path_only_patches(&mut api_data, &patch_path)?;

            if stats.successful > 0 {
                // Normalize class names where external path differs from API name
                match patch::normalize_class_names(&mut api_data) {
                    Ok(count) if count > 0 => {
                        println!("\n[OK] Renamed {} classes to match external paths", count);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("[WARN]  Warning: Failed to normalize class names: {}", e);
                    }
                }

                // Save updated api.json
                let api_json = to_json_pretty_4space(&api_data)?;
                fs::write(&api_path, api_json)?;
                println!("\n[SAVE] Saved updated api.json");
            }

            if stats.failed > 0 {
                std::process::exit(1);
            }

            return Ok(());
        }
        ["autofix", "apply"] => {
            // Default to target/autofix/patches - intelligently discover it
            let patches_dir = find_patches_dir(&project_root)?;

            println!("[FIX] Applying patches from {}...\n", patches_dir.display());

            // Load API data (need mutable copy for patching)
            let api_json_str = fs::read_to_string(&api_path)
                .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
            let mut api_data =
                api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;

            // Apply all patches from directory
            let stats = patch::apply_patches_from_directory(&mut api_data, &patches_dir)?;

            stats.print_summary();

            if stats.successful > 0 || stats.total_changes > 0 {
                // Normalize class names where external path differs from API name
                match patch::normalize_class_names(&mut api_data) {
                    Ok(count) if count > 0 => {
                        println!("[OK] Renamed {} classes to match external paths", count);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("[WARN]  Warning: Failed to normalize class names: {}", e);
                    }
                }
            }

            // Always normalize Az prefixes (even if no patches applied)
            let az_renamed = patch::normalize_az_prefixes(&mut api_data);
            if az_renamed > 0 {
                println!("[FIX] Renamed {} types to remove Az prefix", az_renamed);
            }

            // Save updated api.json if any changes
            if stats.successful > 0 || stats.total_changes > 0 || az_renamed > 0 {
                let api_json = to_json_pretty_4space(&api_data)?;
                fs::write(&api_path, api_json)?;
                println!("\n[SAVE] Saved updated api.json");
            }

            // Remove empty modules after patching
            let api_json_str = fs::read_to_string(&api_path)?;
            let mut fresh_api_data = api::ApiData::from_str(&api_json_str)?;
            let empty_modules_removed = api::remove_empty_modules(&mut fresh_api_data);

            if empty_modules_removed > 0 {
                println!("[OK] Removed {} empty modules", empty_modules_removed);
                let api_json = to_json_pretty_4space(&fresh_api_data)?;
                fs::write(&api_path, api_json)?;
                println!("\n[SAVE] Saved updated api.json");
            }

            if stats.has_errors() {
                std::process::exit(1);
            }

            return Ok(());
        }
        ["autofix", "apply", patch_file] | ["patch", patch_file] => {
            println!("[FIX] Applying patches to api.json...\n");

            // Load API data (need mutable copy for patching)
            let api_json_str = fs::read_to_string(&api_path)
                .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
            let mut api_data =
                api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;

            // Get project root (parent of doc/) for resolving paths
            let patch_path = PathBuf::from(patch_file);
            let patch_path = if patch_path.is_absolute() {
                patch_path
            } else {
                // Try relative to project root first
                let project_relative = project_root.join(&patch_path);
                if project_relative.exists() {
                    project_relative
                } else {
                    // Fall back to current dir (doc/)
                    patch_path
                }
            };

            // Check if it's a directory or file
            if patch_path.is_dir() {
                // Apply all patches from directory
                let stats = patch::apply_patches_from_directory(&mut api_data, &patch_path)?;

                stats.print_summary();

                if stats.successful > 0 || stats.total_changes > 0 {
                    // Normalize class names where external path differs from API name
                    match patch::normalize_class_names(&mut api_data) {
                        Ok(count) if count > 0 => {
                            println!("[OK] Renamed {} classes to match external paths", count);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!("[WARN]  Warning: Failed to normalize class names: {}", e);
                        }
                    }
                }

                // Always normalize Az prefixes (even if no patches applied)
                let az_renamed = patch::normalize_az_prefixes(&mut api_data);
                if az_renamed > 0 {
                    println!("[FIX] Renamed {} types to remove Az prefix", az_renamed);
                }

                // Save updated api.json if any changes
                if stats.successful > 0 || stats.total_changes > 0 || az_renamed > 0 {
                    let api_json = to_json_pretty_4space(&api_data)?;
                    fs::write(&api_path, api_json)?;
                    println!("\n[SAVE] Saved updated api.json");
                }

                // NOTE: We do NOT remove unused types after autofix patches.
                // autofix adds types that are transitively reachable from functions
                // in the workspace index, but find_unused_types may not see them as
                // reachable because the api.json type definitions may be incomplete.
                // Use "azul-doc unused" to manually check/remove unused types.

                // Remove empty modules after patching
                let api_json_str = fs::read_to_string(&api_path)?;
                let mut fresh_api_data = api::ApiData::from_str(&api_json_str)?;
                let empty_modules_removed = api::remove_empty_modules(&mut fresh_api_data);

                if empty_modules_removed > 0 {
                    println!("[OK] Removed {} empty modules", empty_modules_removed);
                    let api_json = to_json_pretty_4space(&fresh_api_data)?;
                    fs::write(&api_path, api_json)?;
                    println!("\n[SAVE] Saved updated api.json");
                }

                if stats.has_errors() {
                    std::process::exit(1);
                }
            } else {
                // Apply single patch file
                let patch = patch::ApiPatch::from_file(&patch_path).with_context(|| {
                    format!("Failed to load patch file: {}", patch_path.display())
                })?;

                match patch.apply(&mut api_data) {
                    Ok((count, errors)) => {
                        if errors.is_empty() {
                            println!("[OK] Applied {} changes\n", count);
                        } else {
                            println!(
                                "[WARN]  Applied {} changes with {} errors\n",
                                count,
                                errors.len()
                            );
                        }

                        // Normalize class names where external path differs from API name
                        match patch::normalize_class_names(&mut api_data) {
                            Ok(count) if count > 0 => {
                                println!(
                                    "[OK] Renamed {} classes to match external paths\n",
                                    count
                                );
                            }
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!(
                                    "[WARN]  Warning: Failed to normalize class names: {}\n",
                                    e
                                );
                            }
                        }

                        // Save updated api.json
                        let api_json = to_json_pretty_4space(&api_data)?;
                        fs::write(&api_path, api_json)?;
                        println!("[SAVE] Saved updated api.json\n");

                        if !errors.is_empty() {
                            println!("\nPatch errors:");
                            for error in &errors {
                                println!("  - {}", error);
                            }
                            std::process::exit(1);
                        }
                    }
                    Err(e) => {
                        eprintln!("Error applying patch: {}", e);
                        return Err(e);
                    }
                }
            }

            return Ok(());
        }
        // Legacy memtest commands removed - use "codegen all" instead
        // Memory layout tests are now included in dll/src/lib.rs via include!()
        // Run them with: cd dll && cargo test
        // V2 commands are now the default - use "codegen <target>" instead
        ["v2", "dll"]
        | ["v2", "python"]
        | ["v2", "memtest"]
        | ["v2", "c"]
        | ["v2", "cpp"]
        | ["v2", "all"] => {
            println!("Note: 'v2' prefix is deprecated - v2 is now the default.\n");
            println!("Use 'codegen all' or 'codegen <target>' instead.\n");
            let api_data = load_api_json(&api_path)?;
            codegen::v2::generate_all_v2(&api_data, &project_root)?;
            return Ok(());
        }
        ["nfpm", version] => {
            // Generate NFPM configuration YAML from api.json package metadata
            let api_data = load_api_json(&api_path)?;
            let output_dir = project_root.join("target").join("packages");
            println!("[NFPM] Generating nfpm.yaml for version {}...\n", version);
            dllgen::deploy::generate_nfpm_yaml(version, &api_data, &output_dir)?;
            return Ok(());
        }
        ["nfpm"] => {
            // Use latest version if not specified
            let api_data = load_api_json(&api_path)?;
            let version = api_data
                .get_latest_version_str()
                .ok_or_else(|| anyhow::anyhow!("No versions found in api.json"))?;
            let output_dir = project_root.join("target").join("packages");
            println!("[NFPM] Generating nfpm.yaml for version {}...\n", version);
            dllgen::deploy::generate_nfpm_yaml(&version, &api_data, &output_dir)?;
            return Ok(());
        }
        ["reftest", "headless", test_name] => {
            println!("Running headless reftest for: {}", test_name);

            let output_dir = PathBuf::from("target").join("reftest_headless");
            let test_dir = PathBuf::from(manifest_dir).join("working");

            reftest::run_single_reftest_headless(test_name, &test_dir, &output_dir)?;

            println!("\nHeadless reftest for '{}' complete.", test_name);
            println!("   Debug information has been printed to the console.");
            println!(
                "   Generated images can be found in: {}",
                output_dir.display()
            );

            return Ok(());
        }
        // Debug a single test with LLM assistance
        ["debug", test_name, rest @ ..] => {
            // Parse optional flags and question from remaining args
            let mut add_working_diff = false;
            let mut dry_run = false;
            let mut no_screenshots = false;
            let mut question_parts: Vec<&str> = Vec::new();

            for arg in rest.iter() {
                match *arg {
                    "--add-working-diff" => add_working_diff = true,
                    "--dry-run" => dry_run = true,
                    "--no-screenshots" => no_screenshots = true,
                    other => question_parts.push(other),
                }
            }

            let question = if question_parts.is_empty() {
                None
            } else {
                Some(question_parts.join(" "))
            };

            let config = reftest::debug::DebugConfig {
                test_name: test_name.to_string(),
                question,
                azul_root: project_root.clone(),
                output_dir: PathBuf::from("target").join("debug"),
                add_working_diff,
                dry_run,
                no_screenshots,
            };

            reftest::debug::run_debug_analysis(config)?;

            return Ok(());
        }
        // Regression analysis - walk git history to find when tests changed
        // 
        // Usage:
        //   debug-regression <commits.txt>        - Process commits from file (one hash per line)
        //   debug-regression statistics           - Generate report from existing data
        ["debug-regression", "statistics"] => {
            let config = reftest::regression::RegressionConfig {
                azul_root: project_root.clone(),
                refs_file: None,
                refs: vec![],
                test_dir: PathBuf::from(manifest_dir).join("working"),
                output_dir: PathBuf::from("target").join("reftest"),
            };
            reftest::regression::run_statistics(config)?;
            return Ok(());
        }
        ["debug-regression", "visual"] => {
            let config = reftest::regression::RegressionConfig {
                azul_root: project_root.clone(),
                refs_file: None,
                refs: vec![],
                test_dir: PathBuf::from(manifest_dir).join("working"),
                output_dir: PathBuf::from("target").join("reftest"),
            };
            reftest::regression::run_visual_report(config)?;
            
            // Open in browser
            let report_path = PathBuf::from("target/reftest/regression/visual.html");
            if report_path.exists() {
                println!("Opening report in browser...");
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&report_path).spawn();
                #[cfg(target_os = "linux")]
                let _ = std::process::Command::new("xdg-open").arg(&report_path).spawn();
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("cmd").args(["/C", "start", report_path.to_str().unwrap()]).spawn();
            }
            return Ok(());
        }
        ["debug-regression", "statistics", "prompt"] => {
            let config = reftest::regression::RegressionConfig {
                azul_root: project_root.clone(),
                refs_file: None,
                refs: vec![],
                test_dir: PathBuf::from(manifest_dir).join("working"),
                output_dir: PathBuf::from("target").join("reftest"),
            };
            reftest::regression::run_statistics_prompt(config)?;
            return Ok(());
        }
        ["debug-regression", "statistics", "send"] => {
            let config = reftest::regression::RegressionConfig {
                azul_root: project_root.clone(),
                refs_file: None,
                refs: vec![],
                test_dir: PathBuf::from(manifest_dir).join("working"),
                output_dir: PathBuf::from("target").join("reftest"),
            };
            reftest::regression::run_statistics_send(config, None)?;
            return Ok(());
        }
        ["debug-regression", "statistics", "send", "-o", output_path] => {
            let config = reftest::regression::RegressionConfig {
                azul_root: project_root.clone(),
                refs_file: None,
                refs: vec![],
                test_dir: PathBuf::from(manifest_dir).join("working"),
                output_dir: PathBuf::from("target").join("reftest"),
            };
            reftest::regression::run_statistics_send(config, Some(PathBuf::from(output_path)))?;
            return Ok(());
        }
        ["debug-regression", file_path] => {
            let path = PathBuf::from(file_path);
            if path.exists() && path.is_file() {
                println!("Running regression analysis from file: {}", path.display());
                let config = reftest::regression::RegressionConfig {
                    azul_root: project_root.clone(),
                    refs_file: Some(path),
                    refs: vec![],
                    test_dir: PathBuf::from(manifest_dir).join("working"),
                    output_dir: PathBuf::from("target").join("reftest"),
                };
                reftest::regression::run_regression_analysis(config)?;
            } else {
                // Treat as single ref
                println!("Running regression analysis for ref: {}", file_path);
                let config = reftest::regression::RegressionConfig {
                    azul_root: project_root.clone(),
                    refs_file: None,
                    refs: vec![file_path.to_string()],
                    test_dir: PathBuf::from(manifest_dir).join("working"),
                    output_dir: PathBuf::from("target").join("reftest"),
                };
                reftest::regression::run_regression_analysis(config)?;
            }
            return Ok(());
        }
        ["debug-regression"] => {
            println!("Usage:");
            println!("  azul-doc debug-regression <commits.txt>      - Process commits from file");
            println!("  azul-doc debug-regression <git-ref>          - Process single ref");
            println!("  azul-doc debug-regression visual             - Generate visual HTML report");
            println!("  azul-doc debug-regression statistics         - Generate diff report (stdout)");
            println!("  azul-doc debug-regression statistics prompt  - Generate Gemini prompt with source");
            println!("  azul-doc debug-regression statistics send    - Send prompt to Gemini API");
            println!("  azul-doc debug-regression statistics send -o <file>  - Save response to file");
            println!();
            println!("Example workflow:");
            println!("  1. Generate commit list:  ./scripts/find_layout_commits.py c0e504a3..HEAD -o commits.txt");
            println!("  2. Run regression:        cargo run --release -- debug-regression commits.txt");
            println!("  3. View visual report:    cargo run --release -- debug-regression visual");
            println!("  4. View text diffs:       cargo run --release -- debug-regression statistics");
            println!("  5. Generate prompt:       cargo run --release -- debug-regression statistics prompt > prompt.md");
            println!("  6. Send to Gemini:        cargo run --release -- debug-regression statistics send -o response.md");
            return Ok(());
        }
        ["reftest", test_name] if *test_name != "open" && *test_name != "headless" => {
            // Run single reftest
            println!("Running reftest for: {}", test_name);

            let output_dir = PathBuf::from("target").join("reftest");
            let config = RunRefTestsConfig {
                test_dir: PathBuf::from(manifest_dir).join("working"),
                output_dir: output_dir.clone(),
                output_filename: "index.html",
            };

            reftest::run_single_reftest(test_name, config)?;

            let report_path = output_dir.join("index.html");
            println!(
                "\nReftest complete. Report generated at: {}",
                report_path.display()
            );

            if report_path.exists() {
                println!("Opening report in browser...");
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&report_path).spawn();
            }

            return Ok(());
        }
        ["spec", rest @ ..] => {
            let args: Vec<String> = rest.iter().map(|s| s.to_string()).collect();
            spec::run_spec_command(&args, &project_root)
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            return Ok(());
        }
        ["reftest"] | ["reftest", "open"] => {
            println!("Running local reftests...");
            let open_report = args.get(2) == Some(&"open");

            let output_dir = PathBuf::from("target").join("reftest");
            let config = RunRefTestsConfig {
                // The test files are in `doc/working`
                test_dir: PathBuf::from(manifest_dir).join("working"),
                output_dir: output_dir.clone(),
                output_filename: "index.html",
            };

            reftest::run_reftests(config)?;

            let report_path = output_dir.join("index.html");
            println!(
                "\nReftest complete. Report generated at: {}",
                report_path.display()
            );

            if open_report && report_path.exists() {
                println!("Opening report in browser...");
                #[cfg(target_os = "macos")]
                let _ = std::process::Command::new("open").arg(&report_path).spawn();
                #[cfg(target_os = "linux")]
                let _ = std::process::Command::new("xdg-open")
                    .arg(&report_path)
                    .spawn();
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("start")
                    .arg(&report_path)
                    .spawn();
            }

            return Ok(());
        }
        ["codegen"] | ["codegen", "rust"] => {
            let api_data = load_api_json(&api_path)?;
            println!("[CODEGEN] Generating Rust library code...\n");

            // Generate azul.rs using the v2 generator
            let code = codegen::v2::generate_rust_public_api(&api_data)?;
            let output_path = project_root.join("target").join("codegen").join("azul.rs");
            fs::create_dir_all(output_path.parent().unwrap())?;
            fs::write(&output_path, &code)?;
            println!(
                "[OK] Generated: {} ({} bytes)",
                output_path.display(),
                code.len()
            );

            println!("\nRust code generation complete.");
            return Ok(());
        }
        ["codegen", "c"] => {
            let api_data = load_api_json(&api_path)?;
            println!("[CODEGEN] Generating C header file...\n");

            let code = codegen::v2::generate_c_header(&api_data)?;
            let output_path = project_root.join("target").join("codegen").join("azul.h");
            fs::create_dir_all(output_path.parent().unwrap())?;
            fs::write(&output_path, &code)?;
            println!(
                "[OK] Generated: {} ({} bytes)",
                output_path.display(),
                code.len()
            );

            println!("\nC header generation complete.");
            return Ok(());
        }
        ["codegen", "cpp"] => {
            let api_data = load_api_json(&api_path)?;
            println!("[CODEGEN] Generating C++ header files...\n");

            let cpp_dir = project_root.join("target").join("codegen");
            fs::create_dir_all(&cpp_dir)?;

            // Generate C++11 header (main header)
            let code =
                codegen::v2::generate_cpp_header(&api_data, codegen::v2::CppStandard::Cpp11)?;
            let output_path = cpp_dir.join("azul.hpp");
            fs::write(&output_path, &code)?;
            println!(
                "[OK] Generated: {} ({} bytes)",
                output_path.display(),
                code.len()
            );

            println!("\nC++ header generation complete.");
            return Ok(());
        }
        ["codegen", "python"] => {
            let api_data = load_api_json(&api_path)?;
            println!("[CODEGEN] Generating Python bindings...\n");

            codegen::v2::generate_python_v2(&api_data, &project_root)?;

            println!("\nPython bindings generation complete.");
            return Ok(());
        }
        ["codegen", "all"] => {
            let api_data = load_api_json(&api_path)?;
            println!("[CODEGEN] Generating all language bindings using v2...\n");

            codegen::v2::generate_all_v2(&api_data, &project_root)?;

            println!("\nAll language bindings generated successfully.");
            return Ok(());
        }
        ["deploy"] | ["deploy", ..] => {
            // Check for debug mode: "deploy debug" uses external CSS, "deploy" inlines CSS
            let is_debug = args.len() > 2 && args[2] == "debug";

            if is_debug {
                println!("Starting Azul Fast Deploy (debug mode - external CSS)...");
            } else {
                println!("Starting Azul Build and Deploy System (production - inline CSS)...");
            }

            let api_data = load_api_json(&api_path)?;
            let config = Config::from_args();
            println!("CONFIG={}", config.print());

            // Create output directory structure
            let output_dir = project_root.join("doc").join("target").join("deploy");

            // Remove stale deploy folder before generating new content
            if output_dir.exists() {
                println!("Removing stale deploy folder...");
                fs::remove_dir_all(&output_dir)?;
            }

            let image_path = output_dir.join("images");
            let releases_dir = output_dir.join("release");

            fs::create_dir_all(&output_dir)?;
            fs::create_dir_all(&image_path)?;
            fs::create_dir_all(&releases_dir)?;

            // Generate documentation (API docs, guide, etc.)
            // In debug mode, use external stylesheet and relative paths. In production, inline CSS and absolute URLs.
            let inline_css = !is_debug;
            let image_url = if is_debug {
                "./images"
            } else {
                "https://azul.rs/images"
            };
            println!("Generating documentation (inline_css={})...", inline_css);
            for (path, html) in
                docgen::generate_docs(&api_data, &image_path, image_url, inline_css)?
            {
                let path_real = output_dir.join(&path);
                if let Some(parent) = path_real.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::write(&path_real, &html)?;
                println!("  [OK] Generated: {}", path);
            }

            // Verify all example files exist before proceeding
            let examples_dir = project_root.join("examples");
            println!("Verifying example files...");
            let strict_examples = config.deploy_mode == dllgen::deploy::DeployMode::Strict;
            dllgen::deploy::verify_examples(&api_data, &examples_dir, strict_examples)?;

            // Generate releases pages with api.json and examples.zip
            println!("Generating releases pages...");
            generate_release_pages(&api_data, &releases_dir, config.deploy_mode, &examples_dir)?;

            // Generate releases index page
            let versions = api_data.get_sorted_versions();
            let releases_index = dllgen::deploy::generate_releases_index(&versions);
            fs::write(output_dir.join("releases.html"), &releases_index)?;
            println!("  [OK] Generated: releases.html");

            // Generate donation page
            println!("Generating donation page...");
            let funding_yaml_bytes = include_str!("../../.github/FUNDING.yml");
            match docgen::donate::generate_donation_page(funding_yaml_bytes) {
                Ok(donation_html) => {
                    fs::write(output_dir.join("donate.html"), &donation_html)?;
                    println!("  [OK] Generated: donate.html");
                }
                Err(e) => {
                    eprintln!("  [WARN] Failed to generate donation page: {}", e);
                }
            }

            // Generate reftest page (without running tests)
            println!("Generating reftest page...");
            let reftest_output_dir = output_dir.join("reftest");
            let test_dir = PathBuf::from(manifest_dir).join("working");
            match reftest::generate_reftest_page(&reftest_output_dir, Some(&test_dir)) {
                Ok(_) => {
                    // Copy to reftest.html in deploy root
                    if reftest_output_dir.join("index.html").exists() {
                        fs::copy(
                            reftest_output_dir.join("index.html"),
                            output_dir.join("reftest.html"),
                        )?;
                        println!("  [OK] Generated: reftest.html");
                    }
                }
                Err(e) => {
                    eprintln!("  [WARN] Failed to generate reftest page: {}", e);
                }
            }

            // Copy static assets
            dllgen::deploy::copy_static_assets(&output_dir)?;

            println!(
                "\nWebsite generated successfully in: {}",
                output_dir.display()
            );
            return Ok(());
        }
        ["fast-deploy-with-reftests"] | ["fast-deploy-with-reftests", ..] => {
            println!("Starting Azul Fast Deploy with Reftests...");
            let api_data = load_api_json(&api_path)?;
            let config = Config::from_args();
            println!("CONFIG={}", config.print());

            // Create output directory structure
            let output_dir = project_root.join("doc").join("target").join("deploy");

            // Remove stale deploy folder before generating new content
            if output_dir.exists() {
                println!("Removing stale deploy folder...");
                fs::remove_dir_all(&output_dir)?;
            }

            let image_path = output_dir.join("images");
            let releases_dir = output_dir.join("release");

            fs::create_dir_all(&output_dir)?;
            fs::create_dir_all(&image_path)?;
            fs::create_dir_all(&releases_dir)?;

            // Generate documentation (API docs, guide, etc.)
            // Fast deploy uses external stylesheet (no inline CSS) and relative image paths
            println!("Generating documentation (external CSS for fast deploy)...");
            for (path, html) in docgen::generate_docs(&api_data, &image_path, "./images", false)? {
                let path_real = output_dir.join(&path);
                if let Some(parent) = path_real.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::write(&path_real, &html)?;
                println!("  [OK] Generated: {}", path);
            }

            // Verify all example files exist before proceeding
            let examples_dir = project_root.join("examples");
            println!("Verifying example files...");
            let strict_examples = config.deploy_mode == dllgen::deploy::DeployMode::Strict;
            dllgen::deploy::verify_examples(&api_data, &examples_dir, strict_examples)?;

            // Generate releases pages with api.json and examples.zip
            println!("Generating releases pages...");
            generate_release_pages(&api_data, &releases_dir, config.deploy_mode, &examples_dir)?;

            // Generate releases index page
            let versions = api_data.get_sorted_versions();
            let releases_index = dllgen::deploy::generate_releases_index(&versions);
            fs::write(output_dir.join("releases.html"), &releases_index)?;
            println!("  [OK] Generated: releases.html");

            // Generate donation page
            println!("Generating donation page...");
            let funding_yaml_bytes = include_str!("../../.github/FUNDING.yml");
            match docgen::donate::generate_donation_page(funding_yaml_bytes) {
                Ok(donation_html) => {
                    fs::write(output_dir.join("donate.html"), &donation_html)?;
                    println!("  [OK] Generated: donate.html");
                }
                Err(e) => {
                    eprintln!("  [WARN] Failed to generate donation page: {}", e);
                }
            }

            // Copy static assets
            dllgen::deploy::copy_static_assets(&output_dir)?;

            // Run reftests and generate reftest.html
            println!("\nRunning reftests...");
            let reftest_output_dir = output_dir.join("reftest");
            fs::create_dir_all(&reftest_output_dir)?;

            let reftest_config = RunRefTestsConfig {
                test_dir: PathBuf::from(manifest_dir).join("working"),
                output_dir: reftest_output_dir.clone(),
                output_filename: "index.html",
            };

            match reftest::run_reftests(reftest_config) {
                Ok(_) => {
                    println!("  [OK] Reftests completed");
                    // Copy reftest results to deploy folder
                    let reftest_html = reftest_output_dir.join("index.html");
                    if reftest_html.exists() {
                        fs::copy(&reftest_html, output_dir.join("reftest.html"))?;
                        println!("  [OK] Copied reftest.html to deploy folder");
                    }
                }
                Err(e) => {
                    eprintln!("  [WARN] Reftests failed: {}", e);
                }
            }

            println!(
                "\nWebsite with reftests generated successfully in: {}",
                output_dir.display()
            );
            return Ok(());
        }
        _ => {
            print_cli_help()?;
            return Ok(());
        }
    }

    Ok(())
}

/// Generates release pages for all versions including api.json and examples.zip
fn generate_release_pages(
    api_data: &api::ApiData,
    releases_dir: &std::path::Path,
    deploy_mode: dllgen::deploy::DeployMode,
    examples_dir: &std::path::Path,
) -> anyhow::Result<()> {
    use codegen::cpp_api::CppVersion;
    use dllgen::deploy::{DeployMode, ReleaseAssets};

    let versions = api_data.get_sorted_versions();

    for version in &versions {
        let version_dir = releases_dir.join(version);
        fs::create_dir_all(&version_dir)?;

        // Generate C header for this version
        let c_api_code = codegen::c_api::generate_c_api(api_data, version);
        fs::write(version_dir.join("azul.h"), &c_api_code)?;
        println!("  [OK] Generated: release/{}/azul.h", version);

        // Generate C++ headers for all supported standards
        let cpp_headers = dllgen::deploy::CppHeaders {
            cpp03: codegen::cpp_api::generate_cpp_api_for_version(
                api_data,
                version,
                CppVersion::Cpp03,
            ),
            cpp11: codegen::cpp_api::generate_cpp_api_for_version(
                api_data,
                version,
                CppVersion::Cpp11,
            ),
            cpp14: codegen::cpp_api::generate_cpp_api_for_version(
                api_data,
                version,
                CppVersion::Cpp14,
            ),
            cpp17: codegen::cpp_api::generate_cpp_api_for_version(
                api_data,
                version,
                CppVersion::Cpp17,
            ),
            cpp20: codegen::cpp_api::generate_cpp_api_for_version(
                api_data,
                version,
                CppVersion::Cpp20,
            ),
            cpp23: codegen::cpp_api::generate_cpp_api_for_version(
                api_data,
                version,
                CppVersion::Cpp23,
            ),
        };

        // Write individual C++ header files
        fs::write(version_dir.join("azul03.hpp"), &cpp_headers.cpp03)?;
        fs::write(version_dir.join("azul11.hpp"), &cpp_headers.cpp11)?;
        fs::write(version_dir.join("azul14.hpp"), &cpp_headers.cpp14)?;
        fs::write(version_dir.join("azul17.hpp"), &cpp_headers.cpp17)?;
        fs::write(version_dir.join("azul20.hpp"), &cpp_headers.cpp20)?;
        fs::write(version_dir.join("azul23.hpp"), &cpp_headers.cpp23)?;
        println!(
            "  [OK] Generated: release/{}/azul*.hpp (all C++ versions)",
            version
        );

        // Generate api.json for this version
        if let Some(version_data) = api_data.get_version(version) {
            let api_json = to_json_pretty_4space(&version_data)?;
            fs::write(version_dir.join("api.json"), &api_json)?;
            println!("  [OK] Generated: release/{}/api.json", version);
        }

        // Generate LICENSE files using cargo-license
        if let Err(e) = dllgen::deploy::generate_license_files(version, &version_dir) {
            eprintln!("  [WARN] Failed to generate license files: {}", e);
        } else {
            println!("  [OK] Generated: release/{}/LICENSE-*.txt", version);
        }

        // Generate examples.zip for this version (reads paths from api.json)
        if let Err(e) = dllgen::deploy::create_examples(
            version,
            &version_dir,
            &c_api_code,
            &cpp_headers,
            api_data,
            examples_dir,
        ) {
            eprintln!("  [WARN] Failed to create examples.zip: {}", e);
        } else {
            println!("  [OK] Generated: release/{}/examples.zip", version);
        }

        // Collect asset information (for HTML generation and validation)
        let assets = ReleaseAssets::collect(&version_dir);

        // In strict/CI mode, fail if binary assets are missing
        if deploy_mode == DeployMode::Strict {
            if let Err(missing) = assets.validate_binary_assets() {
                anyhow::bail!(
                    "Deploy failed: Missing binary assets for version {}: {:?}\nIn CI mode, all \
                     binary assets must be present before deployment.",
                    version,
                    missing
                );
            }
        } else {
            // In local mode, create placeholder files for missing build artifacts
            for asset in dllgen::deploy::BinaryAsset::all() {
                let file_path = version_dir.join(asset.filename);
                if !file_path.exists() {
                    println!(
                        "  [WARN] Missing build artifact: release/{}/{} - creating placeholder",
                        version, asset.filename
                    );
                    fs::write(
                        &file_path,
                        format!(
                            "# Placeholder - build artifact not available\n# Version: {}\n# File: \
                             {}\n",
                            version, asset.filename
                        ),
                    )?;
                }
            }
            // Re-collect assets after creating placeholders
            let assets = ReleaseAssets::collect(&version_dir);

            // Generate release HTML page with dynamic sizes
            let release_html = dllgen::deploy::generate_release_html(version, api_data, &assets);
            fs::write(releases_dir.join(&format!("{version}.html")), &release_html)?;
            println!("  [OK] Generated: release/{}.html", version);
        }

        // Generate release HTML page with dynamic sizes (for strict mode, after validation)
        if deploy_mode == DeployMode::Strict {
            let assets = ReleaseAssets::collect(&version_dir);
            let release_html = dllgen::deploy::generate_release_html(version, api_data, &assets);
            fs::write(releases_dir.join(&format!("{version}.html")), &release_html)?;
            println!("  [OK] Generated: release/{}.html", version);
        }
    }

    Ok(())
}

fn load_api_json(api_path: &PathBuf) -> anyhow::Result<api::ApiData> {
    let api_json_str = fs::read_to_string(&api_path)
        .with_context(|| format!("Failed to read api.json from {}", api_path.display()))?;
    let api_data =
        api::ApiData::from_str(&api_json_str).context("Failed to parse API definition")?;

    Ok(api_data)
}

/// Find the patches directory, trying multiple locations
fn find_patches_dir(project_root: &PathBuf) -> anyhow::Result<PathBuf> {
    // Try locations in order of priority
    let candidates = [
        // 1. Standard location relative to project root
        project_root.join("target").join("autofix").join("patches"),
        // 2. Current working directory's target/autofix/patches
        PathBuf::from("target").join("autofix").join("patches"),
        // 3. If we're in doc/, go up one level
        PathBuf::from("..")
            .join("target")
            .join("autofix")
            .join("patches"),
        // 4. If we're in target/, look in autofix/patches
        PathBuf::from("autofix").join("patches"),
    ];

    for candidate in &candidates {
        if candidate.exists() && candidate.is_dir() {
            // Check if it has any .patch.json files
            if let Ok(entries) = fs::read_dir(candidate) {
                let has_patches = entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.path().extension().map_or(false, |ext| ext == "json"));
                if has_patches {
                    return Ok(candidate.clone());
                }
            }
        }
    }

    // Default to the standard location even if it doesn't exist yet
    let default = project_root.join("target").join("autofix").join("patches");
    if default.exists() {
        Ok(default)
    } else {
        anyhow::bail!(
            "No patches directory found. Generate patches first with:\n  \
             azul-doc autofix difficult remove <items...>\n  \
             azul-doc autofix remove <Type.method>\n\n\
             Expected location: {}",
            default.display()
        )
    }
}

fn print_cli_help() -> anyhow::Result<()> {
    println!("Azul Documentation and Deployment Tool");
    println!();
    println!("Usage:");
    println!("  azul-doc <command> [options]");
    println!();
    println!("Commands:");
    println!();
    println!("  TESTING:");
    println!("    reftest                       - Run all reftests (open report in browser)");
    println!("    reftest open                  - Same as 'reftest'");
    println!("    reftest <test_name>           - Run a single reftest by name");
    println!("    reftest headless <test_name>  - Run single test without Chrome reference");
    println!();
    println!("  DEBUG (single test analysis):");
    println!("    debug <test_name>             - Debug test with Gemini LLM analysis");
    println!("    debug <test_name> --dry-run   - Generate prompt without calling API");
    println!("    debug <test_name> --add-working-diff");
    println!("                                  - Include current git diff in prompt");
    println!("    debug <test_name> --no-screenshots");
    println!("                                  - Exclude screenshots (saves tokens)");
    println!("    debug <test_name> <question>  - Ask specific question about the test");
    println!();
    println!("  DEBUG-REGRESSION (git history analysis):");
    println!("    debug-regression              - Show usage for regression testing");
    println!("    debug-regression <git-ref>    - Process single commit/branch/tag");
    println!("    debug-regression <file.txt>   - Process commits from file (one per line)");
    println!("    debug-regression visual       - Generate visual HTML report with screenshots");
    println!("    debug-regression statistics   - Generate diff report for all processed commits");
    println!("    debug-regression statistics prompt");
    println!("                                  - Generate Gemini prompt from statistics");
    println!("    debug-regression statistics send");
    println!("                                  - Send prompt to Gemini API, print response");
    println!("    debug-regression statistics send -o <file>");
    println!("                                  - Send prompt to Gemini API, save to file");
    println!();
    println!("  AUTOFIX (synchronize workspace types with api.json):");
    println!("    autofix                       - Analyze and generate patches for api.json");
    println!("    autofix run                   - Same as 'autofix'");
    println!("    autofix explain               - Explain what generated patches will do");
    println!("    autofix list <Type>           - List functions for a type (source vs api.json)");
    println!("    autofix add <Type.method>     - Add function(s) to api.json");
    println!("    autofix add <Type.*>          - Add all public methods of a type");
    println!("    autofix remove <Type.method>  - Remove function from api.json");
    println!("    autofix apply                 - Apply patches from target/autofix/patches");
    println!("    autofix apply <file|dir>      - Apply a patch file or directory");
    println!("    autofix apply safe <dir>      - Apply and delete safe (path-only) patches");
    println!("    autofix difficult remove ...  - Remove multiple functions/types at once");
    println!();
    println!("  AUTOFIX DEBUG (inspect type resolution):");
    println!("    autofix debug type <name>     - Show type definition in workspace index");
    println!("    autofix debug chain <name>    - Show recursive type resolution chain");
    println!("    autofix debug api <name>      - Compare workspace vs api.json for a type");
    println!("    autofix debug file <path>     - Debug parsing of a specific file");
    println!("    autofix difficult             - Rank types by FFI difficulty");
    println!("    autofix internal              - Show types that should be internal-only");
    println!("    autofix modules               - Show types in wrong modules");
    println!("    autofix deps                  - Analyze function dependencies on difficult types");
    println!();
    println!("  API MANAGEMENT:");
    println!("    normalize                     - Normalize/reformat api.json");
    println!("    dedup                         - Remove duplicate types from api.json");
    println!("    print [options]               - Print API information");
    println!("    unused                        - Find unused types in api.json");
    println!("    unused patch                  - Generate patches to remove unused types");
    println!();
    println!("  DISCOVERY (find unindexed types):");
    println!("    discover                      - Scan workspace for all unindexed types");
    println!("    discover <pattern>            - Scan for types matching pattern");
    println!();
    println!("  CODE GENERATION:");
    println!("    codegen                       - Generate Rust library code");
    println!("    codegen rust                  - Generate Rust library code");
    println!("    codegen c                     - Generate C header (azul.h)");
    println!("    codegen cpp                   - Generate C++ header (azul.hpp)");
    println!("    codegen python                - Generate Python bindings");
    println!("    codegen all                   - Generate all bindings + DLL API + memtest");
    println!();
    println!("  PACKAGING:");
    println!("    nfpm                          - Generate NFPM config (latest version)");
    println!("    nfpm <version>                - Generate NFPM config for specific version");
    println!();
    println!("  DEPLOYMENT:");
    println!("    deploy                        - Build and deploy (production, inline CSS)");
    println!("    deploy debug                  - Deploy in debug mode (external CSS)");
    println!("    fast-deploy-with-reftests     - Quick deploy with reftest regeneration");
    println!();
    println!("  OTHER:");
    println!("    v2 dll                        - Generate v2 DLL bindings");
    println!();
    Ok(())
}
