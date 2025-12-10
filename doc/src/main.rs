//! Main entry point for the documentation generation and management tool.
#![allow(unused)]
use std::{env, fs, path::PathBuf};

use anyhow::Context;
use dllgen::deploy::Config;
use reftest::RunRefTestsConfig;

pub mod api;
pub mod autofix;
pub mod codegen;
pub mod dllgen;
pub mod docgen;
pub mod patch;
pub mod print;
pub mod reftest;
pub mod utils;

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
            let mut api_data = load_api_json(&api_path)?;
            
            // Normalize array types: [T; N] -> type: T, arraysize: N
            let array_count = api::normalize_array_types(&mut api_data);
            if array_count > 0 {
                println!("[ARRAY] Normalized {} array type fields", array_count);
            }
            
            // Normalize type_alias: "*mut c_void" -> target: "c_void", ref_kind: "mutptr"
            let type_alias_count = api::normalize_type_aliases(&mut api_data);
            if type_alias_count > 0 {
                println!("[TYPE_ALIAS] Normalized {} type alias entries", type_alias_count);
            }
            
            let api_json = serde_json::to_string_pretty(&api_data)?;
            fs::write(&api_path, api_json)?;
            println!("[SAVE] Saved normalized api.json\n");
            return Ok(());
        }
        ["dedup"] => {
            println!("[DEDUP] Removing duplicate types from api.json...\n");
            let mut api_data = load_api_json(&api_path)?;
            let removed = patch::remove_duplicate_types(&mut api_data);
            if removed > 0 {
                let api_json = serde_json::to_string_pretty(&api_data)?;
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
        ["autofix", "explain"] => {
            let patches_dir = project_root.join("target").join("autofix").join("patches");

            if !patches_dir.exists() {
                eprintln!("No patches found. Run 'azul-doc autofix' first.");
                std::process::exit(1);
            }

            patch::explain_patches(&patches_dir)?;
            return Ok(());
        }
        // ====================================================================
        // FUNCTION MANAGEMENT COMMANDS
        // ====================================================================
        ["autofix", "list", type_spec] => {
            // List functions for a type: autofix list Dom
            // Or with module prefix: autofix list dom.Dom
            let api_data = load_api_json(&api_path)?;
            let index = autofix::type_index::TypeIndex::build(&project_root, false)?;
            
            // Get the latest version
            let version = api_data.get_latest_version_str()
                .ok_or_else(|| anyhow::anyhow!("No versions in api.json"))?;
            
            // Parse type_spec: could be "TypeName" or "module.TypeName"
            let type_name = if type_spec.contains('.') {
                type_spec.rsplit('.').next().unwrap_or(type_spec)
            } else {
                type_spec
            };
            
            match autofix::function_diff::list_type_functions(type_name, &index, &api_data, version) {
                Ok(result) => {
                    println!("\n=== Functions for {} ===\n", result.type_name);
                    
                    if !result.source_only.is_empty() {
                        println!("📦 In source only ({} - need to add to api.json):", result.source_only.len());
                        for name in &result.source_only {
                            println!("  + {}", name);
                        }
                        println!();
                    }
                    
                    if !result.api_only.is_empty() {
                        println!("📄 In api.json only ({} - may be stale):", result.api_only.len());
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
                    
                    println!("Summary: {} source-only, {} api-only, {} matching",
                        result.source_only.len(), result.api_only.len(), result.both.len());
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
            let version = api_data.get_latest_version_str()
                .ok_or_else(|| anyhow::anyhow!("No versions in api.json"))?
                .to_string();
            
            // Parse fn_spec: "TypeName.method" or "TypeName.*" or "module.TypeName.method"
            let parts: Vec<&str> = fn_spec.split('.').collect();
            
            if parts.len() < 2 {
                eprintln!("Error: Invalid format. Use: TypeName.method or TypeName.* or module.TypeName.method");
                std::process::exit(1);
            }
            
            let (type_name, method_spec) = if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                // module.TypeName.method - ignore the module prefix, we'll determine it automatically
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
            let version_data = api_data.get_version(&version)
                .ok_or_else(|| anyhow::anyhow!("Version not found"))?;
            
            let type_exists = autofix::function_diff::type_exists_in_api(type_name, version_data);
            
            if !type_exists {
                // Type doesn't exist in api.json - use the new function to add it with dependencies
                println!("[ADD] Type '{}' not found in api.json, adding with transitive dependencies...\n", type_name);
                
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
                        let patches_dir = project_root.join("target").join("autofix").join("patches");
                        fs::create_dir_all(&patches_dir)?;
                        
                        for (i, patch) in patches.iter().enumerate() {
                            let patch_filename = format!("add_{}_{}.patch.json", 
                                type_name.to_lowercase(),
                                i
                            );
                            let patch_path = patches_dir.join(&patch_filename);
                            
                            let json = patch.to_json().unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e));
                            fs::write(&patch_path, &json)?;
                        }
                        
                        // Also generate the functions patch if methods were requested
                        if !result.added_methods.is_empty() {
                            let methods: Vec<_> = type_def.methods.iter()
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
                            
                            let patch_filename = format!("add_{}_functions.patch.json", type_name.to_lowercase());
                            let patch_path = patches_dir.join(&patch_filename);
                            let json = serde_json::to_string_pretty(&func_patch)?;
                            fs::write(&patch_path, &json)?;
                        }
                        
                        println!("\n[OK] {} patches written to: {}", patches.len() + 1, patches_dir.display());
                        println!("\nTo apply the patches:");
                        println!("  cargo run -- patch {}", patches_dir.display());
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
            let methods: Vec<_> = type_def.methods.iter()
                .filter(|m| m.is_public)
                .filter(|m| method_spec == "*" || m.name == method_spec)
                .collect();
            
            if methods.is_empty() {
                println!("No matching public methods found for '{}.{}'", type_name, method_spec);
                return Ok(());
            }
            
            println!("[ADD] Generating patch to add {} function(s) to {}\n", methods.len(), type_name);
            
            // Show what will be added
            for m in &methods {
                let self_str = match &m.self_kind {
                    None => "static",
                    Some(autofix::type_index::SelfKind::Value) => "self",
                    Some(autofix::type_index::SelfKind::Ref) => "&self",
                    Some(autofix::type_index::SelfKind::RefMut) => "&mut self",
                };
                let ret_str = m.return_type.as_deref().unwrap_or("()");
                let ctor_str = if m.is_constructor { " [constructor]" } else { "" };
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
            
            let patch_filename = format!("add_{}_{}.patch.json", 
                type_name.to_lowercase(),
                if method_spec == "*" { "all" } else { method_spec }
            );
            let patch_path = patches_dir.join(&patch_filename);
            
            let json = serde_json::to_string_pretty(&patch)?;
            fs::write(&patch_path, &json)?;
            
            println!("\n[OK] Patch written to: {}", patch_path.display());
            println!("\nTo apply the patch:");
            println!("  cargo run -- patch {}", patches_dir.display());
            
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
            let version = api_data.get_latest_version_str()
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
            let version_data = api_data.get_version(&version)
                .ok_or_else(|| anyhow::anyhow!("Version not found"))?;
            let module_name = autofix::function_diff::find_type_module(type_name, version_data)
                .ok_or_else(|| anyhow::anyhow!("Type '{}' not found in api.json", type_name))?
                .to_string();
            
            println!("[REMOVE] Generating patch to remove '{}' from {}\n", method_name, type_name);
            
            // Generate the patch
            let patch = autofix::function_diff::generate_remove_functions_patch(
                type_name,
                &[method_name],
                &module_name,
                &version,
            );
            
            // Write patch to file
            let patch_filename = format!("remove_{}_{}.patch.json", 
                type_name.to_lowercase(),
                method_name
            );
            let patch_path = patches_dir.join(&patch_filename);
            
            let json = serde_json::to_string_pretty(&patch)?;
            fs::write(&patch_path, &json)?;
            
            println!("[OK] Patch written to: {}", patch_path.display());
            println!("\nTo apply the patch:");
            println!("  cargo run -- patch {}", patches_dir.display());
            
            return Ok(());
        }
        ["unused"] => {
            println!("[SEARCH] Finding unused types in api.json (recursive analysis)...\n");
            let api_data = load_api_json(&api_path)?;
            let unused_types = api::find_all_unused_types_recursive(&api_data);
            
            if unused_types.is_empty() {
                println!("[OK] No unused types found. All types are reachable from the public API.");
            } else {
                println!("[WARN] Found {} unused types:\n", unused_types.len());
                
                // Group by module for better readability
                let mut by_module: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
                for info in &unused_types {
                    by_module.entry(info.module_name.clone())
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
                let (module_name, type_count) = patch.versions.values()
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
            
            println!("\n[OK] Generated {} removal patch files for {} types in:", patches.len(), unused_types.len());
            println!("     {}", patches_dir.display());
            println!("\nTo review a patch:");
            println!("  cat {}/*.patch.json", patches_dir.display());
            println!("\nTo apply the patches:");
            println!("  cargo run -- patch {}", patches_dir.display());
            
            return Ok(());
        }
        ["patch", "safe", patch_dir] => {
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
                let api_json = serde_json::to_string_pretty(&api_data)?;
                fs::write(&api_path, api_json)?;
                println!("\n[SAVE] Saved updated api.json");
            }

            if stats.failed > 0 {
                std::process::exit(1);
            }

            return Ok(());
        }
        ["patch", patch_file] => {
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
                    let api_json = serde_json::to_string_pretty(&api_data)?;
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
                    let api_json = serde_json::to_string_pretty(&fresh_api_data)?;
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
                            println!("[WARN]  Applied {} changes with {} errors\n", count, errors.len());
                        }

                        // Normalize class names where external path differs from API name
                        match patch::normalize_class_names(&mut api_data) {
                            Ok(count) if count > 0 => {
                                println!("[OK] Renamed {} classes to match external paths\n", count);
                            }
                            Ok(_) => {}
                            Err(e) => {
                                eprintln!("[WARN]  Warning: Failed to normalize class names: {}\n", e);
                            }
                        }

                        // Save updated api.json
                        let api_json = serde_json::to_string_pretty(&api_data)?;
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
        ["memtest", "run"] => {
            let api_data = load_api_json(&api_path)?;
            
            // Run FFI safety checks first
            println!("[CHECK] Running FFI safety checks...\n");
            let type_index = autofix::type_index::TypeIndex::build(&project_root, false)
                ?;
            let ffi_warnings = autofix::check_ffi_safety(&type_index, &api_data);
            if !ffi_warnings.is_empty() {
                autofix::print_ffi_safety_warnings(&ffi_warnings);
                eprintln!("\nError: Found {} FFI safety issues. Fix them before running memtest.", ffi_warnings.len());
                std::process::exit(1);
            }
            println!("[OK] FFI safety checks passed.\n");
            
            println!("[TEST] Generating memory layout test crate...\n");
            codegen::memtest::generate_memtest_crate(&api_data, &project_root)
                .map_err(|e| anyhow::anyhow!(e))?;

            println!("\n[RUN] Running memory layout tests...\n");
            let memtest_dir = project_root.join("target").join("memtest");
            let status = std::process::Command::new("cargo")
                .arg("test")
                .arg("--")
                .arg("--nocapture")
                .current_dir(&memtest_dir)
                .status()?;

            if !status.success() {
                eprintln!("\nMemory layout tests failed!");
                std::process::exit(1);
            } else {
                println!("\n[OK] All memory layout tests passed!");
            }

            return Ok(());
        }
        ["memtest"] => {
            let api_data = load_api_json(&api_path)?;
            
            // Run FFI safety checks first
            println!("[CHECK] Running FFI safety checks...\n");
            let type_index = autofix::type_index::TypeIndex::build(&project_root, false)
                ?;
            let ffi_warnings = autofix::check_ffi_safety(&type_index, &api_data);
            if !ffi_warnings.is_empty() {
                autofix::print_ffi_safety_warnings(&ffi_warnings);
                eprintln!("\nError: Found {} FFI safety issues. Fix them before running memtest.", ffi_warnings.len());
                std::process::exit(1);
            }
            println!("[OK] FFI safety checks passed.\n");
            
            println!("[TEST] Generating memory layout test crate...\n");
            codegen::memtest::generate_memtest_crate(&api_data, &project_root)
                .map_err(|e| anyhow::anyhow!(e))?;
            return Ok(());
        }
        ["memtest", "dll"] => {
            let api_data = load_api_json(&api_path)?;
            println!("[DLL] Generating DLL API definitions...\n");
            codegen::memtest::generate_dll_api(&api_data, &project_root)
                .map_err(|e| anyhow::anyhow!(e))?;
            return Ok(());
        }
        ["reftest", "headless", test_name] => {
            println!("Running headless reftest for: {}", test_name);

            let output_dir = PathBuf::from("target").join("reftest_headless");
            let test_dir = PathBuf::from(manifest_dir).join("src/reftest/working");

            reftest::run_single_reftest_headless(test_name, &test_dir, &output_dir)?;

            println!("\nHeadless reftest for '{}' complete.", test_name);
            println!("   Debug information has been printed to the console.");
            println!(
                "   Generated images can be found in: {}",
                output_dir.display()
            );

            return Ok(());
        }
        ["reftest"] => {
            println!("Running local reftests...");

            let output_dir = PathBuf::from("target").join("reftest");
            let config = RunRefTestsConfig {
                // The test files are in `doc/src/reftest/working`
                test_dir: PathBuf::from(manifest_dir).join("src/reftest/working"),
                output_dir: output_dir.clone(),
                output_filename: "index.html",
            };

            reftest::run_reftests(config)?;

            let report_path = output_dir.join("index.html");
            println!(
                "\nReftest complete. Report generated at: {}",
                report_path.display()
            );

            if args.len() > 2 && args[1] == "open" {
                if open::that(report_path).is_ok() {
                    println!("Opened report in default browser.");
                } else {
                    eprintln!("Could not open browser. Please open the report manually.");
                }
            }

            return Ok(());
        }
        ["codegen"] | ["codegen", "rust"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("[FIX] Generating Rust library code...\n");

            // NOTE: dll/lib.rs is no longer generated here.
            // The DLL uses include!() with target/memtest/dll_api.rs instead.
            // To regenerate the DLL API: cd doc && cargo run --release -- memtest dll

            // Generate azul-rs/azul.rs
            let rust_api_code = codegen::rust_api::generate_rust_api(&api_data, version);
            let rust_api_path = project_root.join("target").join("codegen").join("azul.rs");
            fs::create_dir_all(rust_api_path.parent().unwrap())?;
            fs::write(&rust_api_path, rust_api_code)?;
            println!("[OK] Generated: {}", rust_api_path.display());

            println!("\nRust code generation complete.");
            return Ok(());
        }
        ["codegen", "c"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("[FIX] Generating C header file...\n");

            let c_api_code = codegen::c_api::generate_c_api(&api_data, version);
            let c_api_path = project_root.join("target").join("codegen").join("azul.h");
            fs::create_dir_all(c_api_path.parent().unwrap())?;
            fs::write(&c_api_path, c_api_code)?;
            println!("[OK] Generated: {}", c_api_path.display());

            println!("\nC header generation complete.");
            return Ok(());
        }
        ["codegen", "cpp"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("[FIX] Generating C++ header file...\n");

            let cpp_api_code = codegen::cpp_api::generate_cpp_api(&api_data, version);
            let cpp_api_path = project_root.join("target").join("codegen").join("azul.hpp");
            fs::create_dir_all(cpp_api_path.parent().unwrap())?;
            fs::write(&cpp_api_path, cpp_api_code)?;
            println!("[OK] Generated: {}", cpp_api_path.display());

            println!("\nC++ header generation complete.");
            return Ok(());
        }
        ["codegen", "python"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("[FIX] Generating Python bindings...\n");

            let python_api_code = codegen::python_api::generate_python_api(&api_data, version);
            let python_api_path = project_root.join("target").join("codegen").join("python_capi.rs");
            fs::write(&python_api_path, python_api_code)?;
            println!("[OK] Generated: {}", python_api_path.display());

            println!("\nPython bindings generation complete.");
            return Ok(());
        }
        ["codegen", "all"] => {
            let api_data = load_api_json(&api_path)?;
            let version = api_data.0.keys().next().expect("No version in api.json");

            println!("[FIX] Generating all language bindings...\n");

            // NOTE: dll/lib.rs is no longer generated here.
            // The DLL uses include!() with target/memtest/dll_api.rs instead.
            // To regenerate the DLL API: cd doc && cargo run --release -- memtest dll

            // Generate azul-rs/azul.rs
            let rust_api_code = codegen::rust_api::generate_rust_api(&api_data, version);
            let rust_api_path = project_root.join("target").join("codegen").join("azul.rs");
            fs::create_dir_all(rust_api_path.parent().unwrap())?;
            fs::write(&rust_api_path, rust_api_code)?;
            println!("[OK] Generated: {}", rust_api_path.display());

            // Generate C header
            let c_api_code = codegen::c_api::generate_c_api(&api_data, version);
            let c_api_path = project_root.join("target").join("codegen").join("azul.h");
            fs::write(&c_api_path, c_api_code)?;
            println!("[OK] Generated: {}", c_api_path.display());

            // Generate C++ header
            let cpp_api_code = codegen::cpp_api::generate_cpp_api(&api_data, version);
            let cpp_api_path = project_root.join("target").join("codegen").join("azul.hpp");
            fs::write(&cpp_api_path, cpp_api_code)?;
            println!("[OK] Generated: {}", cpp_api_path.display());

            // Generate Python bindings
            let python_api_code = codegen::python_api::generate_python_api(&api_data, version);
            let python_api_path = project_root.join("target").join("codegen").join("python_capi.rs");
            fs::write(&python_api_path, python_api_code)?;
            println!("[OK] Generated: {}", python_api_path.display());

            println!("\nAll language bindings generated successfully.");
            return Ok(());
        }
        ["deploy"] | ["deploy", ..] => {
            println!("Starting Azul Build and Deploy System...");
            let api_data = load_api_json(&api_path)?;
            let config = Config::from_args();
            println!("CONFIG={}", config.print());

            // Create output directory structure
            let output_dir = project_root.join("doc").join("target").join("deploy");
            let image_path = output_dir.join("images");
            let releases_dir = output_dir.join("release");

            fs::create_dir_all(&output_dir)?;
            fs::create_dir_all(&image_path)?;
            fs::create_dir_all(&releases_dir)?;

            // Generate documentation (API docs, guide, etc.)
            println!("Generating documentation...");
            for (path, html) in docgen::generate_docs(&api_data, &image_path, "https://azul.rs/images")? {
                let path_real = output_dir.join(&path);
                if let Some(parent) = path_real.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::write(&path_real, &html)?;
                println!("  [OK] Generated: {}", path);
            }

            // Generate releases pages
            println!("Generating releases pages...");
            let versions = api_data.get_sorted_versions();
            
            for version in &versions {
                let version_dir = releases_dir.join(version);
                fs::create_dir_all(&version_dir)?;
                
                // Generate release HTML page
                let release_html = dllgen::deploy::generate_release_html(version, &api_data);
                fs::write(releases_dir.join(&format!("{version}.html")), &release_html)?;
                println!("  [OK] Generated: release/{}.html", version);
                
                // Create placeholder files for missing build artifacts
                let placeholder_files = [
                    "azul.dll", "azul.lib", "windows.pyd", "LICENSE-WINDOWS.txt",
                    "libazul.so", "libazul.linux.a", "linux.pyd", "LICENSE-LINUX.txt",
                    "libazul.dylib", "libazul.macos.a", "macos.pyd", "LICENSE-MACOS.txt",
                    "azul.h", "azul.hpp",
                ];
                
                for filename in &placeholder_files {
                    let file_path = version_dir.join(filename);
                    if !file_path.exists() {
                        println!("  [WARN] Missing build artifact: release/{}/{} - creating placeholder", version, filename);
                        fs::write(&file_path, format!("# Placeholder - build artifact not available\n# Version: {}\n# File: {}\n", version, filename))?;
                    }
                }
            }
            
            // Generate releases index page
            let releases_index = dllgen::deploy::generate_releases_index(&versions);
            fs::write(output_dir.join("releases.html"), &releases_index)?;
            println!("  [OK] Generated: releases.html");

            // Copy static assets
            dllgen::deploy::copy_static_assets(&output_dir)?;

            println!("\nWebsite generated successfully in: {}", output_dir.display());
            return Ok(());
        }
        ["fast-deploy-with-reftests"] | ["fast-deploy-with-reftests", ..] => {
            println!("Starting Azul Fast Deploy with Reftests...");
            let api_data = load_api_json(&api_path)?;
            let config = Config::from_args();
            println!("CONFIG={}", config.print());

            // Create output directory structure
            let output_dir = project_root.join("doc").join("target").join("deploy");
            let image_path = output_dir.join("images");
            let releases_dir = output_dir.join("release");

            fs::create_dir_all(&output_dir)?;
            fs::create_dir_all(&image_path)?;
            fs::create_dir_all(&releases_dir)?;

            // Generate documentation (API docs, guide, etc.)
            println!("Generating documentation...");
            for (path, html) in docgen::generate_docs(&api_data, &image_path, "https://azul.rs/images")? {
                let path_real = output_dir.join(&path);
                if let Some(parent) = path_real.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                fs::write(&path_real, &html)?;
                println!("  [OK] Generated: {}", path);
            }

            // Generate releases pages
            println!("Generating releases pages...");
            let versions = api_data.get_sorted_versions();
            
            for version in &versions {
                let version_dir = releases_dir.join(version);
                fs::create_dir_all(&version_dir)?;
                
                // Generate release HTML page
                let release_html = dllgen::deploy::generate_release_html(version, &api_data);
                fs::write(releases_dir.join(&format!("{version}.html")), &release_html)?;
                println!("  [OK] Generated: release/{}.html", version);
                
                // Create placeholder files for missing build artifacts
                let placeholder_files = [
                    "azul.dll", "azul.lib", "windows.pyd", "LICENSE-WINDOWS.txt",
                    "libazul.so", "libazul.linux.a", "linux.pyd", "LICENSE-LINUX.txt",
                    "libazul.dylib", "libazul.macos.a", "macos.pyd", "LICENSE-MACOS.txt",
                    "azul.h", "azul.hpp",
                ];
                
                for filename in &placeholder_files {
                    let file_path = version_dir.join(filename);
                    if !file_path.exists() {
                        println!("  [WARN] Missing build artifact: release/{}/{} - creating placeholder", version, filename);
                        fs::write(&file_path, format!("# Placeholder - build artifact not available\n# Version: {}\n# File: {}\n", version, filename))?;
                    }
                }
            }
            
            // Generate releases index page
            let releases_index = dllgen::deploy::generate_releases_index(&versions);
            fs::write(output_dir.join("releases.html"), &releases_index)?;
            println!("  [OK] Generated: releases.html");

            // Copy static assets
            dllgen::deploy::copy_static_assets(&output_dir)?;

            // Run reftests and generate reftest.html
            println!("\nRunning reftests...");
            let reftest_output_dir = output_dir.join("reftest");
            fs::create_dir_all(&reftest_output_dir)?;
            
            let reftest_config = RunRefTestsConfig {
                test_dir: PathBuf::from(manifest_dir).join("src/reftest/working"),
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

            println!("\nWebsite with reftests generated successfully in: {}", output_dir.display());
            return Ok(());
        }
        _ => {
            print_cli_help()?;
            return Ok(());
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

fn print_cli_help() -> anyhow::Result<()> {
    println!("Azul Documentation and Deployment Tool");
    println!();
    println!("Usage:");
    println!("  azul-doc <command> [options]");
    println!();
    println!("Commands:");
    println!();
    println!("  AUTOFIX (synchronize workspace types with api.json):");
    println!("    autofix                       - Analyze and generate patches for api.json");
    println!("    autofix run                   - Same as 'autofix'");
    println!("    autofix explain               - Explain what generated patches will do");
    println!();
    println!("  AUTOFIX DEBUG (inspect type resolution):");
    println!("    autofix debug type <name>     - Show type definition in workspace index");
    println!("    autofix debug chain <name>    - Show recursive type resolution chain");
    println!("    autofix debug api <name>      - Compare workspace vs api.json for a type");
    println!("    autofix debug file <path>     - Debug parsing of a specific file");
    println!();
    println!("  PATCHING:");
    println!("    patch <file>                  - Apply a patch file to api.json");
    println!("    patch <dir>                   - Apply all patch files from a directory");
    println!("    patch safe <dir>              - Apply and delete safe (path-only) patches");
    println!();
    println!("  API MANAGEMENT:");
    println!("    normalize                     - Normalize/reformat api.json");
    println!("    print [options]               - Print API information");
    println!("    unused                        - Find unused types in api.json");
    println!("    unused patch                  - Generate patches to remove unused types");
    println!();
    println!("  CODE GENERATION:");
    println!("    codegen [rust|c|cpp|python|all] - Generate language bindings");
    println!("    memtest [run]                 - Generate and optionally run memory layout tests");
    println!("    memtest dll                   - Generate DLL API definitions for include!()");
    println!();
    println!("  TESTING:");
    println!("    reftest [open]                - Run reftests and optionally open report");
    println!();
    println!("  DEPLOYMENT:");
    println!("    deploy                        - Build and deploy the Azul library");
    println!("    fast-deploy-with-reftests     - Deploy with reftest generation");
    println!();
    Ok(())
}
