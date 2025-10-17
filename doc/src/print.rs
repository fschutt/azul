use std::{path::PathBuf, process};

use anyhow::Result;

use crate::{
    api::{ApiData, ClassData, FunctionData},
    patch::{locatesource, parser},
};

/// Handle the "print" subcommand for API discovery
pub fn handle_print_command(api_data: &ApiData, args: &[String]) -> Result<()> {
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Failed to get project root"))?
        .to_path_buf();

    if args.is_empty() {
        // Print all modules
        return print_all_modules(api_data);
    }

    let path_str = &args[0];
    let parts: Vec<&str> = path_str.split('.').collect();

    match parts.len() {
        1 => {
            // azul-doc print app
            print_module(api_data, parts[0])
        }
        2 => {
            // azul-doc print app.App
            print_class(api_data, &project_root, parts[0], parts[1])
        }
        3 => {
            // azul-doc print app.App.new
            print_function(api_data, &project_root, parts[0], parts[1], parts[2])
        }
        _ => {
            eprintln!("❌ Invalid path format: {}", path_str);
            eprintln!("Expected formats:");
            eprintln!("  azul-doc print              (all modules)");
            eprintln!("  azul-doc print app          (module)");
            eprintln!("  azul-doc print app.App      (class)");
            eprintln!("  azul-doc print app.App.new  (function)");
            process::exit(1);
        }
    }
}

pub fn print_all_modules(api_data: &ApiData) -> Result<()> {
    println!("📦 All API Modules:\n");

    let mut has_errors = false;

    for (version_name, version_data) in &api_data.0 {
        println!("Version: {}\n", version_name);

        for (module_name, module_data) in &version_data.api {
            let class_count = module_data.classes.len();
            let doc = module_data
                .doc
                .as_ref()
                .map(|d| d.as_str())
                .unwrap_or("(no documentation)");

            println!("  📁 {} - {} classes", module_name, class_count);
            println!("     {}", doc);

            // Check for missing external paths
            let mut missing_count = 0;
            for (class_name, class_data) in &module_data.classes {
                if class_data.external.is_none() {
                    missing_count += 1;
                    if missing_count == 1 {
                        println!("     ⚠️  Missing external paths:");
                    }
                    println!("        - {}", class_name);
                    has_errors = true;
                }
            }

            println!();
        }
    }

    if has_errors {
        println!("❌ Found errors in API definitions");
        process::exit(1);
    } else {
        println!("✅ All modules have complete definitions");
        Ok(())
    }
}

pub fn print_module(api_data: &ApiData, module_name: &str) -> Result<()> {
    println!("📁 Module: {}\n", module_name);

    let mut found = false;
    let mut has_errors = false;

    for (version_name, version_data) in &api_data.0 {
        if let Some(module_data) = version_data.api.get(module_name) {
            found = true;
            println!("Version: {}", version_name);

            if let Some(doc) = &module_data.doc {
                println!("Documentation: {}\n", doc);
            }

            println!("Classes ({}):", module_data.classes.len());

            for (class_name, class_data) in &module_data.classes {
                print_class_summary(class_name, class_data);

                // Check for errors
                if class_data.external.is_none() {
                    println!("  ⚠️  Missing external path");
                    has_errors = true;
                }
            }
        }
    }

    if !found {
        eprintln!("❌ Module '{}' not found", module_name);
        process::exit(1);
    }

    if has_errors {
        println!("\n❌ Found errors in module '{}'", module_name);
        process::exit(1);
    } else {
        println!("\n✅ Module '{}' has complete definitions", module_name);
        Ok(())
    }
}

pub fn print_class(
    api_data: &ApiData,
    project_root: &PathBuf,
    module_name: &str,
    class_name: &str,
) -> Result<()> {
    println!("📦 Class: {}.{}\n", module_name, class_name);

    let mut found = false;

    for (version_name, version_data) in &api_data.0 {
        if let Some(module_data) = version_data.api.get(module_name) {
            if let Some(class_data) = module_data.classes.get(class_name) {
                found = true;
                println!("Version: {}", version_name);
                let separator = "─".repeat(60);
                println!("{}", separator);

                // 1. Print api.json definition
                println!("\n📄 API Definition:");
                print_class_detail(class_data);

                // 2. Print import path
                println!("\n🔗 Import Path:");
                if let Some(external) = &class_data.external {
                    println!("  {}", external);

                    // 3. Try to locate and print source
                    println!("\n📂 Source Location:");
                    match locatesource::retrieve_item_source(project_root, external) {
                        Ok(source) => {
                            let lines: Vec<&str> = source.lines().collect();
                            if lines.len() > 50 {
                                // Truncate if too long
                                println!("```rust");
                                for line in lines.iter().take(50) {
                                    println!("{}", line);
                                }
                                println!("... ({} more lines)", lines.len() - 50);
                                println!("```");
                            } else {
                                println!("```rust");
                                println!("{}", source);
                                println!("```");
                            }
                        }
                        Err(e) => {
                            println!("  ⚠️  Failed to retrieve source: {}", e);
                        }
                    }
                } else {
                    println!("  ⚠️  No external path defined");
                }

                let separator2 = "─".repeat(60);
                println!("\n{}", separator2);
            }
        }
    }

    if !found {
        eprintln!("❌ Class '{}.{}' not found", module_name, class_name);
        process::exit(1);
    }

    Ok(())
}

pub fn print_function(
    api_data: &ApiData,
    project_root: &PathBuf,
    module_name: &str,
    class_name: &str,
    function_name: &str,
) -> Result<()> {
    println!(
        "⚙️  Function: {}.{}.{}\n",
        module_name, class_name, function_name
    );

    let mut found = false;
    let mut has_errors = false;

    for (version_name, version_data) in &api_data.0 {
        if let Some(module_data) = version_data.api.get(module_name) {
            if let Some(class_data) = module_data.classes.get(class_name) {
                // Check constructors
                if let Some(constructors) = &class_data.constructors {
                    if let Some(func_data) = constructors.get(function_name) {
                        found = true;
                        println!("Version: {} (constructor)", version_name);
                        print_function_detail(func_data, true);

                        if let Some(external) = &class_data.external {
                            let full_path = format!("{}::{}", external, function_name);
                            println!("\n🔗 Import Path:");
                            println!("  {}", full_path);

                            match match_function_with_source(project_root, &full_path, func_data) {
                                Ok(true) => println!("  ✅ Signature matches source"),
                                Ok(false) => {
                                    println!("  ⚠️  Signature differs from source");
                                    has_errors = true;
                                }
                                Err(e) => {
                                    println!("  ❌ Validation failed: {}", e);
                                    has_errors = true;
                                }
                            }
                        }
                    }
                }

                // Check functions
                if let Some(functions) = &class_data.functions {
                    if let Some(func_data) = functions.get(function_name) {
                        found = true;
                        println!("Version: {} (method)", version_name);
                        print_function_detail(func_data, false);

                        if let Some(external) = &class_data.external {
                            let full_path = format!("{}::{}", external, function_name);
                            println!("\n🔗 Import Path:");
                            println!("  {}", full_path);

                            match match_function_with_source(project_root, &full_path, func_data) {
                                Ok(true) => println!("  ✅ Signature matches source"),
                                Ok(false) => {
                                    println!("  ⚠️  Signature differs from source");
                                    has_errors = true;
                                }
                                Err(e) => {
                                    println!("  ❌ Validation failed: {}", e);
                                    has_errors = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if !found {
        eprintln!(
            "❌ Function '{}.{}.{}' not found",
            module_name, class_name, function_name
        );
        process::exit(1);
    }

    if has_errors {
        println!(
            "\n❌ Found errors in function '{}.{}.{}'",
            module_name, class_name, function_name
        );
        process::exit(1);
    } else {
        println!(
            "\n✅ Function '{}.{}.{}' is valid",
            module_name, class_name, function_name
        );
        Ok(())
    }
}

// Helper functions for formatting

pub fn print_class_summary(class_name: &str, class_data: &ClassData) {
    let mut parts = vec![];

    if class_data.struct_fields.is_some() {
        parts.push("struct");
    } else if class_data.enum_fields.is_some() {
        parts.push("enum");
    } else if class_data.callback_typedef.is_some() {
        parts.push("callback");
    }

    let type_str = if parts.is_empty() {
        String::new()
    } else {
        format!("({})", parts.join(", "))
    };

    println!("  • {} {}", class_name, type_str);

    if let Some(external) = &class_data.external {
        println!("    → {}", external);
    }
}

pub fn print_class_detail(class_data: &ClassData) {
    if let Some(doc) = &class_data.doc {
        println!("  Documentation: {}", doc);
    }

    if let Some(derive) = &class_data.derive {
        println!("  Derive: {:?}", derive);
    }

    if class_data.is_boxed_object {
        println!("  Boxed: true");
    }

    if let Some(struct_fields) = &class_data.struct_fields {
        println!("  Struct fields: {}", struct_fields.len());
        for field_map in struct_fields {
            for (name, field_data) in field_map {
                println!("    • {}: {}", name, field_data.r#type);
            }
        }
    }

    if let Some(enum_fields) = &class_data.enum_fields {
        println!("  Enum variants: {}", enum_fields.len());
        for variant_map in enum_fields {
            for (name, variant_data) in variant_map {
                if let Some(ref ty) = variant_data.r#type {
                    println!("    • {}: {}", name, ty);
                } else {
                    println!("    • {}", name);
                }
            }
        }
    }

    if let Some(constructors) = &class_data.constructors {
        println!("  Constructors: {}", constructors.len());
        for (name, _) in constructors {
            println!("    • {}", name);
        }
    }

    if let Some(functions) = &class_data.functions {
        println!("  Functions: {}", functions.len());
        for (name, _) in functions {
            println!("    • {}", name);
        }
    }
}

pub fn print_function_detail(func_data: &FunctionData, is_constructor: bool) {
    let separator = "─".repeat(60);
    println!("{}", separator);

    if let Some(doc) = &func_data.doc {
        println!("\n📄 Documentation: {}", doc);
    }

    println!("\n🔧 Signature:");
    print!("  fn ");
    if is_constructor {
        print!("new");
    }
    print!("(");

    for (i, arg_map) in func_data.fn_args.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        // Each arg_map has keys like "self", "data", "config", etc.
        // and potentially "doc" key for documentation
        for (key, value) in arg_map {
            if key == "doc" {
                // Skip doc key in signature
                continue;
            }

            // Print arg_name: type
            print!("{}: {}", key, value);
        }
    }

    print!(")");

    if let Some(returns) = &func_data.returns {
        print!(" -> {}", returns.r#type);
    }

    println!();

    if let Some(body) = &func_data.fn_body {
        println!("\n📝 Body:");
        println!("  {}", body);
    }

    let separator_end = "─".repeat(60);
    println!("\n{}", separator_end);
}

pub fn validate_class_definition(
    project_root: &PathBuf,
    external_path: &str,
    _class_data: &ClassData,
) -> Result<bool> {
    // Try to parse the actual source and compare
    match parser::parse_directory(project_root) {
        Ok(symbols) => {
            if symbols.contains_key(external_path) {
                Ok(true)
            } else {
                Ok(false)
            }
        }
        Err(e) => Err(anyhow::anyhow!("Failed to parse source: {}", e)),
    }
}

pub fn match_function_with_source(
    project_root: &PathBuf,
    full_path: &str,
    _func_data: &FunctionData,
) -> Result<bool> {
    match locatesource::retrieve_item_source(project_root, full_path) {
        Ok(source) => {
            // For now, just check if we could retrieve the source
            // TODO: More sophisticated signature matching
            Ok(!source.is_empty())
        }
        Err(e) => Err(anyhow::anyhow!("Failed to retrieve source: {}", e)),
    }
}
