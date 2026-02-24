//! Debug utilities for autofix V2
//!
//! This module provides debugging functionality to trace type resolution
//! and verify that the indexing and resolution works correctly.

use std::path::Path;

use anyhow::Result;

use super::{
    diff::{resolve_api_types, ApiTypeResolution},
    type_index::{FieldDef, RefKind, TypeDefKind, TypeDefinition, TypeIndex},
    type_resolver::{ResolutionContext, ResolvedTypeSet, TypeResolver},
};
use crate::api::ApiData;

/// Debug: Print information about a specific type in the index
pub fn debug_type_in_index(index: &TypeIndex, type_name: &str) {
    println!("\nType '{}' in Index:", type_name);

    match index.get_all_by_name(type_name) {
        Some(definitions) => {
            println!("Found {} definition(s):", definitions.len());
            for (i, def) in definitions.iter().enumerate() {
                println!("\n  [{}] {}", i + 1, def.full_path);
                println!("      File: {}", def.file_path.display());
                println!("      Kind: {}", kind_to_string(&def.kind));

                // Show methods count
                if !def.methods.is_empty() {
                    println!("      Methods ({}):", def.methods.len());
                    for m in def.methods.iter().take(10) {
                        let self_str = match &m.self_kind {
                            None => "static",
                            Some(super::type_index::SelfKind::Value) => "self",
                            Some(super::type_index::SelfKind::Ref) => "&self",
                            Some(super::type_index::SelfKind::RefMut) => "&mut self",
                        };
                        let ret_str = m.return_type.as_deref().unwrap_or("()");
                        let pub_str = if m.is_public { "pub " } else { "" };
                        println!(
                            "        - {}fn {}({}) -> {}",
                            pub_str, m.name, self_str, ret_str
                        );
                    }
                    if def.methods.len() > 10 {
                        println!("        ... and {} more", def.methods.len() - 10);
                    }
                }

                match &def.kind {
                    TypeDefKind::Struct {
                        fields,
                        repr,
                        derives,
                        custom_impls,
                        ..
                    } => {
                        println!("      repr: {:?}", repr);
                        println!("      Derives: {:?}", derives);
                        if !custom_impls.is_empty() {
                            println!("      Custom impls: {:?}", custom_impls);
                        }
                        println!("      Fields ({}):", fields.len());
                        for (name, field) in fields.iter().take(5) {
                            let ref_kind_str = if field.ref_kind.is_default() {
                                String::new()
                            } else {
                                format!(" [ref_kind: {}]", field.ref_kind)
                            };
                            println!("        - {}: {}{}", name, field.ty, ref_kind_str);
                        }
                        if fields.len() > 5 {
                            println!("        ... and {} more", fields.len() - 5);
                        }
                    }
                    TypeDefKind::Enum {
                        variants,
                        repr,
                        derives,
                        custom_impls,
                        ..
                    } => {
                        println!("      repr: {:?}", repr);
                        println!("      Derives: {:?}", derives);
                        if !custom_impls.is_empty() {
                            println!("      Custom impls: {:?}", custom_impls);
                        }
                        println!("      Variants ({}):", variants.len());
                        for (name, variant) in variants.iter().take(10) {
                            if let Some(ty) = &variant.ty {
                                println!("        - {}({})", name, ty);
                            } else {
                                println!("        - {}", name);
                            }
                        }
                        if variants.len() > 10 {
                            println!("        ... and {} more", variants.len() - 10);
                        }
                    }
                    TypeDefKind::MacroGenerated {
                        source_macro,
                        base_type,
                        kind,
                        ..
                    } => {
                        println!("      Macro: {}", source_macro);
                        println!("      Base type: {}", base_type);
                        println!("      MacroKind: {:?}", kind);

                        // Show the expanded form
                        let expanded = def.expand_macro_generated();
                        match &expanded {
                            TypeDefKind::Struct {
                                fields,
                                repr,
                                derives,
                                custom_impls,
                                ..
                            } => {
                                println!("      [Expanded to Struct]");
                                println!("        repr: {:?}", repr);
                                println!("        Fields ({}):", fields.len());
                                for (name, field) in fields {
                                    let ref_kind_str = if field.ref_kind.is_default() {
                                        String::new()
                                    } else {
                                        format!(" [ref_kind: {}]", field.ref_kind)
                                    };
                                    println!("          - {}: {}{}", name, field.ty, ref_kind_str);
                                }
                                if !derives.is_empty() {
                                    println!("        Derives: {:?}", derives);
                                }
                                if !custom_impls.is_empty() {
                                    println!("        Custom impls: {:?}", custom_impls);
                                }
                            }
                            TypeDefKind::Enum {
                                variants,
                                repr,
                                derives,
                                custom_impls,
                                ..
                            } => {
                                println!("      [Expanded to Enum]");
                                println!("        repr: {:?}", repr);
                                println!("        Variants ({}):", variants.len());
                                for (name, variant) in variants {
                                    if let Some(ty) = &variant.ty {
                                        println!("          - {}({})", name, ty);
                                    } else {
                                        println!("          - {}", name);
                                    }
                                }
                                if !derives.is_empty() {
                                    println!("        Derives: {:?}", derives);
                                }
                                if !custom_impls.is_empty() {
                                    println!("        Custom impls: {:?}", custom_impls);
                                }
                            }
                            TypeDefKind::CallbackTypedef { args, returns } => {
                                println!("      [Expanded to CallbackTypedef]");
                                println!(
                                    "        Args: {:?}",
                                    args.iter().map(|a| &a.ty).collect::<Vec<_>>()
                                );
                                println!("        Returns: {:?}", returns);
                            }
                            _ => {}
                        }
                    }
                    TypeDefKind::TypeAlias {
                        target,
                        generic_base,
                        generic_args,
                    } => {
                        println!("      Target: {}", target);
                        if let Some(base) = generic_base {
                            println!("      Generic base: {}", base);
                            println!("      Generic args: {:?}", generic_args);
                        }
                    }
                    TypeDefKind::CallbackTypedef { args, returns } => {
                        println!(
                            "      Args: {:?}",
                            args.iter().map(|a| &a.ty).collect::<Vec<_>>()
                        );
                        println!("      Returns: {:?}", returns);
                    }
                }
            }
        }
        None => {
            println!("  NOT FOUND in index");

            // Try to find similar names
            let similar: Vec<_> = index
                .all_type_names()
                .filter(|n| n.to_lowercase().contains(&type_name.to_lowercase()))
                .take(5)
                .collect();

            if !similar.is_empty() {
                println!("  Similar names: {:?}", similar);
            }
        }
    }
}

/// Debug: Resolve a type and all its dependencies, printing the chain
pub fn debug_resolve_type_chain(index: &TypeIndex, type_name: &str) {
    println!("\nResolving type chain for '{}':", type_name);

    let mut resolver = TypeResolver::new(index);
    let ctx = ResolutionContext::new();

    resolver.resolve_type(type_name, &ctx);

    let result = resolver.finish();

    println!("\nResolved {} types:", result.resolved.len());
    for (name, resolved) in &result.resolved {
        println!("  + {} -> {}", name, resolved.full_path);
        if !resolved.resolution_chain.is_empty() {
            println!("    Chain: {:?}", resolved.resolution_chain);
        }
    }

    if !result.unresolved.is_empty() {
        println!("\nUnresolved {} types:", result.unresolved.len());
        for (name, info) in &result.unresolved {
            println!("  - {} ({:?})", name, info.reason);
            if !info.referenced_from.is_empty() {
                println!("    Referenced from: {:?}", info.referenced_from);
            }
        }
    }
}

/// Debug: Check a specific type from api.json against the workspace
pub fn debug_api_type(index: &TypeIndex, api_data: &ApiData, type_name: &str) {
    println!("\nAPI type '{}':", type_name);

    // Find in api.json
    for (version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            if let Some(class_data) = module_data.classes.get(type_name) {
                let api_path = class_data.external.as_deref().unwrap_or("<no external>");
                println!("\nFound in api.json:");
                println!("  Version: {}", version_name);
                println!("  Module: {}", module_name);
                println!("  External path: {}", api_path);

                // Check against workspace
                println!("\nWorkspace lookup:");
                debug_type_in_index(index, type_name);

                // Check if paths match
                if let Some(def) = index.resolve(type_name, None) {
                    if api_path != def.full_path {
                        println!("\n  PATH MISMATCH:");
                        println!("    api.json:  {}", api_path);
                        println!("    workspace: {}", def.full_path);
                    } else {
                        println!("\n  Paths match: OK");
                    }
                } else {
                    println!("\n  TYPE NOT FOUND IN WORKSPACE - should generate REMOVAL patch");
                }

                return;
            }
        }
    }

    println!("  NOT FOUND in api.json");
}

/// Debug: Parse a specific file and show extracted types
pub fn debug_parse_file(file_path: &Path) -> Result<()> {
    use std::fs;

    use quote::ToTokens;
    use syn::{File, Item};

    println!("\nParsing file {}:", file_path.display());

    let content = fs::read_to_string(file_path)?;
    let syntax_tree: File = syn::parse_file(&content)?;

    let mut structs = Vec::new();
    let mut enums = Vec::new();
    let mut type_aliases = Vec::new();
    let mut use_imports = Vec::new();
    let mut macros = Vec::new();

    for item in &syntax_tree.items {
        match item {
            Item::Struct(s) => structs.push(s.ident.to_string()),
            Item::Enum(e) => enums.push(e.ident.to_string()),
            Item::Type(t) => type_aliases.push(t.ident.to_string()),
            Item::Use(u) => {
                let use_str = u.to_token_stream().to_string();
                use_imports.push(use_str);
            }
            Item::Macro(m) => {
                let macro_name = m
                    .mac
                    .path
                    .segments
                    .last()
                    .map(|s| s.ident.to_string())
                    .unwrap_or_default();
                if macro_name.starts_with("impl_") {
                    macros.push(format!("{}!({})", macro_name, m.mac.tokens.to_string()));
                }
            }
            _ => {}
        }
    }

    println!("\nStructs ({}):", structs.len());
    for s in &structs {
        println!("  - {}", s);
    }

    println!("\nEnums ({}):", enums.len());
    for e in &enums {
        println!("  - {}", e);
    }

    println!("\nType aliases ({}):", type_aliases.len());
    for t in &type_aliases {
        println!("  - {}", t);
    }

    println!("\nUse imports ({}) - SHOULD BE SKIPPED:", use_imports.len());
    for u in use_imports.iter().take(10) {
        println!("  - {}", u);
    }
    if use_imports.len() > 10 {
        println!("  ... and {} more", use_imports.len() - 10);
    }

    println!("\nMacro invocations ({}):", macros.len());
    for m in &macros {
        println!("  - {}", m);
    }

    Ok(())
}

fn kind_to_string(kind: &TypeDefKind) -> &'static str {
    match kind {
        TypeDefKind::Struct { .. } => "Struct",
        TypeDefKind::Enum { .. } => "Enum",
        TypeDefKind::TypeAlias { .. } => "TypeAlias",
        TypeDefKind::CallbackTypedef { .. } => "CallbackTypedef",
        TypeDefKind::MacroGenerated { .. } => "MacroGenerated",
    }
}

// unit tests
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{super::type_index::*, *};

    /// Helper: Create a TypeIndex from test source code
    fn create_test_index(sources: &[(&str, &str)]) -> TypeIndex {
        let mut index = TypeIndex::new();

        for (crate_name, source) in sources {
            let types = extract_types_from_test_source(crate_name, source);
            for typedef in types {
                index.add_type_for_test(typedef);
            }
        }

        index
    }

    fn extract_types_from_test_source(crate_name: &str, source: &str) -> Vec<TypeDefinition> {
        use indexmap::IndexMap;
        use quote::ToTokens;
        use syn::{File, Item};

        let syntax_tree: File = syn::parse_file(source).expect("Failed to parse test source");
        let mut types = Vec::new();

        for item in &syntax_tree.items {
            match item {
                Item::Use(_) => continue, // Skip use statements!

                Item::Struct(s) => {
                    let type_name = s.ident.to_string();
                    let mut fields = IndexMap::new();

                    for field in s.fields.iter() {
                        if let Some(field_name) = field.ident.as_ref() {
                            fields.insert(
                                field_name.to_string(),
                                FieldDef {
                                    name: field_name.to_string(),
                                    ty: field
                                        .ty
                                        .to_token_stream()
                                        .to_string()
                                        .split_whitespace()
                                        .collect::<Vec<_>>()
                                        .join(" "),
                                    ref_kind: RefKind::Value,
                                    doc: Vec::new(),
                                },
                            );
                        }
                    }

                    types.push(TypeDefinition {
                        full_path: format!("{}::{}", crate_name, type_name),
                        type_name,
                        file_path: PathBuf::from("test.rs"),
                        module_path: String::new(),
                        crate_name: crate_name.to_string(),
                        kind: TypeDefKind::Struct {
                            fields,
                            repr: Some("C".to_string()),
                            repr_attr_count: 1,
                            generic_params: vec![],
                            derives: vec![],
                            custom_impls: vec![],
                            is_tuple_struct: false,
                        },
                        methods: vec![],
                        source_code: String::new(),
                    });
                }

                Item::Enum(e) => {
                    let type_name = e.ident.to_string();
                    let mut variants = IndexMap::new();

                    for variant in &e.variants {
                        let variant_name = variant.ident.to_string();
                        let variant_ty = if variant.fields.is_empty() {
                            None
                        } else {
                            Some(
                                variant
                                    .fields
                                    .iter()
                                    .map(|f| f.ty.to_token_stream().to_string())
                                    .collect::<Vec<_>>()
                                    .join(", "),
                            )
                        };

                        variants.insert(
                            variant_name.clone(),
                            VariantDef {
                                name: variant_name,
                                ty: variant_ty,
                                doc: Vec::new(),
                            },
                        );
                    }

                    types.push(TypeDefinition {
                        full_path: format!("{}::{}", crate_name, type_name),
                        type_name,
                        file_path: PathBuf::from("test.rs"),
                        module_path: String::new(),
                        crate_name: crate_name.to_string(),
                        kind: TypeDefKind::Enum {
                            variants,
                            repr: Some("C".to_string()),
                            repr_attr_count: 1,
                            generic_params: vec![],
                            derives: vec![],
                            custom_impls: vec![],
                        },
                        methods: vec![],
                        source_code: String::new(),
                    });
                }

                _ => {}
            }
        }

        types
    }

    // // test: use imports are skipped
    //
    #[test]
    fn test_use_imports_not_indexed() {
        let source = r#"
            use other_crate::SomeType;
            pub use another::ReExportedType;
            
            pub struct RealType {
                pub field: i32,
            }
        "#;

        let index = create_test_index(&[("test_crate", source)]);

        // RealType should be indexed
        assert!(index.resolve("RealType", None).is_some());

        // Use imports should NOT be indexed
        assert!(index.resolve("SomeType", None).is_none());
        assert!(index.resolve("ReExportedType", None).is_none());
    }

    // // test: struct fields are correctly extracted
    //
    #[test]
    fn test_struct_fields_extracted() {
        let source = r#"
            pub struct MyStruct {
                pub name: String,
                pub value: i32,
                pub nested: OtherType,
            }
        "#;

        let index = create_test_index(&[("test_crate", source)]);
        let def = index.resolve("MyStruct", None).unwrap();

        match &def.kind {
            TypeDefKind::Struct { fields, .. } => {
                assert_eq!(fields.len(), 3);
                assert!(fields.contains_key("name"));
                assert!(fields.contains_key("value"));
                assert!(fields.contains_key("nested"));
                assert_eq!(fields.get("nested").unwrap().ty, "OtherType");
            }
            _ => panic!("Expected Struct"),
        }
    }

    // // test: enum variants are correctly extracted
    //
    #[test]
    fn test_enum_variants_extracted() {
        let source = r#"
            pub enum XmlError {
                NoParserAvailable,
                InvalidPosition(TextPos),
                ParseError(ParseErrorInfo),
            }
            
            pub struct TextPos {
                pub row: u32,
                pub col: u32,
            }
            
            pub struct ParseErrorInfo {
                pub message: String,
            }
        "#;

        let index = create_test_index(&[("azul_core", source)]);
        let def = index.resolve("XmlError", None).unwrap();

        match &def.kind {
            TypeDefKind::Enum { variants, .. } => {
                assert_eq!(variants.len(), 3);
                assert!(variants.contains_key("NoParserAvailable"));
                assert!(variants.contains_key("InvalidPosition"));
                assert!(variants.contains_key("ParseError"));

                // Check variant types
                assert!(variants.get("NoParserAvailable").unwrap().ty.is_none());
                assert_eq!(
                    variants.get("InvalidPosition").unwrap().ty.as_deref(),
                    Some("TextPos")
                );
                assert_eq!(
                    variants.get("ParseError").unwrap().ty.as_deref(),
                    Some("ParseErrorInfo")
                );
            }
            _ => panic!("Expected Enum"),
        }
    }

    // // test: type resolution follows field types
    //
    #[test]
    fn test_recursive_type_resolution() {
        let source = r#"
            pub struct Window {
                pub options: WindowOptions,
            }
            
            pub struct WindowOptions {
                pub size: LogicalSize,
                pub title: String,
            }
            
            pub struct LogicalSize {
                pub width: f32,
                pub height: f32,
            }
        "#;

        let index = create_test_index(&[("azul_core", source)]);

        let mut resolver = TypeResolver::new(&index);
        let ctx = ResolutionContext::new();
        resolver.resolve_type("Window", &ctx);
        let result = resolver.finish();

        // Should resolve Window, WindowOptions, LogicalSize
        assert!(result.resolved.contains_key("Window"));
        assert!(result.resolved.contains_key("WindowOptions"));
        assert!(result.resolved.contains_key("LogicalSize"));

        // String is primitive-like, should not be in resolved
        assert!(!result.resolved.contains_key("String"));
    }

    // // test: cycle detection
    //
    #[test]
    fn test_cycle_detection() {
        let source = r#"
            pub struct NodeA {
                pub next: NodeB,
            }
            
            pub struct NodeB {
                pub back: NodeA,
            }
        "#;

        let index = create_test_index(&[("test", source)]);

        let mut resolver = TypeResolver::new(&index);
        let ctx = ResolutionContext::new();
        resolver.resolve_type("NodeA", &ctx);
        let result = resolver.finish();

        // Both should be resolved (no infinite loop)
        assert!(result.resolved.contains_key("NodeA"));
        assert!(result.resolved.contains_key("NodeB"));
    }

    // // test: missing type detection
    //
    #[test]
    fn test_missing_type_detection() {
        let source = r#"
            pub struct MyStruct {
                pub field: NonExistentType,
            }
        "#;

        let index = create_test_index(&[("test", source)]);

        let mut resolver = TypeResolver::new(&index);
        let ctx = ResolutionContext::new();
        resolver.resolve_type("MyStruct", &ctx);
        let result = resolver.finish();

        // MyStruct resolved, NonExistentType unresolved
        assert!(result.resolved.contains_key("MyStruct"));
        assert!(result.unresolved.contains_key("NonExistentType"));
    }

    // // test: prefer same crate resolution
    //
    #[test]
    fn test_prefer_same_crate() {
        let source_core = r#"
            pub struct SharedType {
                pub core_field: i32,
            }
        "#;

        let source_dll = r#"
            pub struct SharedType {
                pub dll_field: i32,
            }
        "#;

        let index = create_test_index(&[("azul_core", source_core), ("azul_dll", source_dll)]);

        // Should prefer azul_core over azul_dll
        let def = index.resolve("SharedType", None).unwrap();
        assert_eq!(def.crate_name, "azul_core");

        // But with preferred_crate, should use that
        let def_dll = index.resolve("SharedType", Some("azul_dll")).unwrap();
        assert_eq!(def_dll.crate_name, "azul_dll");
    }
}

/// Analyze and display types ranked by FFI difficulty
pub fn analyze_ffi_difficulty(api_data: &ApiData) {
    use super::module_map::{analyze_ffi_difficulty, is_internal_only_type, FfiDifficulty};
    use std::collections::BTreeMap;

    println!("\n=== FFI Difficulty Analysis ===\n");
    println!("Note: Vec<T> and String are NOT flagged - they have FFI wrappers.\n");
    println!("Only truly problematic types (BTreeMap, HashMap, Arc, VecDeque) are shown.\n");

    // Collect all types with their difficulty scores
    let mut type_difficulties: Vec<(
        String,
        String,
        FfiDifficulty,
        Vec<(String, String, FfiDifficulty)>,
    )> = Vec::new();

    let version = match api_data.get_latest_version_str() {
        Some(v) => v.to_string(),
        None => {
            println!("No versions in api.json");
            return;
        }
    };

    let version_data = match api_data.0.get(&version) {
        Some(v) => v,
        None => {
            println!("Version {} not found", version);
            return;
        }
    };

    let api = &version_data.api;

    for (module_name, module) in api.iter() {
        for (class_name, class_def) in module.classes.iter() {
            let mut field_difficulties: Vec<(String, String, FfiDifficulty)> = Vec::new();
            let mut max_difficulty = FfiDifficulty::Easy;

            // Check struct fields - it's Option<Vec<IndexMap<String, FieldData>>>
            if let Some(fields) = &class_def.struct_fields {
                for field_map in fields.iter() {
                    for (name, field) in field_map.iter() {
                        let diff = analyze_ffi_difficulty(&field.r#type);
                        if diff > max_difficulty {
                            max_difficulty = diff;
                        }
                        // Only flag VeryHard or worse
                        if diff >= FfiDifficulty::VeryHard {
                            field_difficulties.push((name.clone(), field.r#type.clone(), diff));
                        }
                    }
                }
            }

            // Check enum fields - it's Option<Vec<IndexMap<String, EnumVariantData>>>
            if let Some(variants) = &class_def.enum_fields {
                for variant_map in variants.iter() {
                    for (variant_name, variant_data) in variant_map.iter() {
                        if let Some(field_type) = variant_data.r#type.as_ref() {
                            let diff = analyze_ffi_difficulty(field_type);
                            if diff > max_difficulty {
                                max_difficulty = diff;
                            }
                            // Only flag VeryHard or worse
                            if diff >= FfiDifficulty::VeryHard {
                                field_difficulties.push((
                                    variant_name.clone(),
                                    field_type.to_string(),
                                    diff,
                                ));
                            }
                        }
                    }
                }
            }

            // Only include types with VeryHard or worse difficulty
            if max_difficulty >= FfiDifficulty::VeryHard {
                type_difficulties.push((
                    format!("{}.{}", module_name, class_name),
                    class_name.to_string(),
                    max_difficulty,
                    field_difficulties,
                ));
            }
        }
    }

    // Sort by difficulty (highest first)
    type_difficulties.sort_by(|a, b| b.2.cmp(&a.2));

    // Group by difficulty level
    let mut by_difficulty: BTreeMap<FfiDifficulty, Vec<_>> = BTreeMap::new();
    for item in type_difficulties {
        by_difficulty.entry(item.2).or_default().push(item);
    }

    // Print results
    if let Some(impossible) = by_difficulty.get(&FfiDifficulty::Impossible) {
        println!(
            "🚫 IMPOSSIBLE ({} types) - Requires complete redesign:",
            impossible.len()
        );
        for (path, name, _, fields) in impossible.iter().take(20) {
            println!("  {}", path);
            for (field_name, field_type, _) in fields.iter().take(3) {
                println!("    - {}: {}", field_name, field_type);
            }
        }
        if impossible.len() > 20 {
            println!("  ... and {} more", impossible.len() - 20);
        }
        println!();
    }

    if let Some(very_hard) = by_difficulty.get(&FfiDifficulty::VeryHard) {
        println!(
            "⛔ VERY HARD ({} types) - Contains BTreeMap/HashMap/Arc:",
            very_hard.len()
        );
        for (path, name, _, fields) in very_hard.iter().take(30) {
            let is_internal = if is_internal_only_type(name) {
                " [INTERNAL]"
            } else {
                ""
            };
            println!("  {}{}", path, is_internal);
            for (field_name, field_type, _) in fields.iter().take(2) {
                println!("    - {}: {}", field_name, truncate_type(field_type, 60));
            }
        }
        if very_hard.len() > 30 {
            println!("  ... and {} more", very_hard.len() - 30);
        }
        println!();
    }

    if let Some(hard) = by_difficulty.get(&FfiDifficulty::Hard) {
        println!(
            "⚠️  HARD ({} types) - Contains Vec/String (needs wrapper):",
            hard.len()
        );
        for (path, _, _, _) in hard.iter().take(20) {
            println!("  {}", path);
        }
        if hard.len() > 20 {
            println!("  ... and {} more", hard.len() - 20);
        }
        println!();
    }

    println!("Summary:");
    println!(
        "  Impossible: {}",
        by_difficulty
            .get(&FfiDifficulty::Impossible)
            .map(|v| v.len())
            .unwrap_or(0)
    );
    println!(
        "  Very Hard:  {}",
        by_difficulty
            .get(&FfiDifficulty::VeryHard)
            .map(|v| v.len())
            .unwrap_or(0)
    );
    println!(
        "  Hard:       {}",
        by_difficulty
            .get(&FfiDifficulty::Hard)
            .map(|v| v.len())
            .unwrap_or(0)
    );
}

/// Show types that should be internal-only (not exported to C API)
pub fn show_internal_only_types(api_data: &ApiData) {
    use super::module_map::{is_internal_only_type, INTERNAL_ONLY_TYPES};

    println!("\n=== Internal-Only Types Analysis ===\n");

    let version = match api_data.get_latest_version_str() {
        Some(v) => v.to_string(),
        None => {
            println!("No versions in api.json");
            return;
        }
    };

    let version_data = match api_data.0.get(&version) {
        Some(v) => v,
        None => {
            println!("Version {} not found", version);
            return;
        }
    };

    let api = &version_data.api;

    let mut found_internal: Vec<(String, String)> = Vec::new();

    for (module_name, module) in api.iter() {
        for (class_name, _) in module.classes.iter() {
            if is_internal_only_type(class_name) {
                found_internal.push((module_name.clone(), class_name.to_string()));
            }
        }
    }

    println!("Types in api.json that should be internal-only:");
    if found_internal.is_empty() {
        println!("  (none found)");
    } else {
        for (module, name) in &found_internal {
            println!("  {}.{}", module, name);
        }
    }

    println!(
        "\nFull list of internal-only types ({}):",
        INTERNAL_ONLY_TYPES.len()
    );
    for name in INTERNAL_ONLY_TYPES {
        let in_api = found_internal.iter().any(|(_, n)| n == *name);
        let status = if in_api {
            "⚠️  IN API"
        } else {
            "✅ not in API"
        };
        println!("  {} - {}", name, status);
    }

    println!("\nRecommendation: Remove types marked with ⚠️ from api.json");
}

/// Show types that are in the wrong module
pub fn show_wrong_module_types(api_data: &ApiData) {
    use crate::autofix::module_map::{determine_module, get_correct_module};

    println!("\n=== Wrong Module Analysis ===\n");

    let version = "1.0.0-alpha1".to_string();
    let version_data = match api_data.0.get(&version) {
        Some(v) => v,
        None => {
            println!("Version {} not found", version);
            return;
        }
    };

    let api = &version_data.api;

    let mut wrong_module: Vec<(String, String, String, String)> = Vec::new(); // (current_module, type_name, correct_module, reason)

    for (module_name, module) in api.iter() {
        for (class_name, _) in module.classes.iter() {
            if let Some(correct) = get_correct_module(class_name, module_name) {
                let (_, is_misc_fallback) = determine_module(class_name);
                let reason = if is_misc_fallback {
                    "no matching keywords".to_string()
                } else {
                    format!("keyword match → {}", correct)
                };
                wrong_module.push((module_name.clone(), class_name.to_string(), correct, reason));
            }
        }
    }

    if wrong_module.is_empty() {
        println!("✅ All types are in correct modules!");
        return;
    }

    // Group by current module
    let mut by_current: std::collections::BTreeMap<String, Vec<(String, String, String)>> =
        std::collections::BTreeMap::new();
    for (current, name, correct, reason) in wrong_module {
        by_current
            .entry(current)
            .or_default()
            .push((name, correct, reason));
    }

    let mut total = 0;
    for (current_module, types) in by_current.iter() {
        println!("In module '{}':", current_module);
        for (type_name, correct_module, reason) in types {
            println!("  {} → {} ({})", type_name, correct_module, reason);
            total += 1;
        }
        println!();
    }

    println!("Total: {} types in wrong modules", total);
}

/// Analyze which API functions pull in difficult/internal types
/// This helps identify the root cause of FFI issues
pub fn analyze_function_dependencies(api_data: &ApiData) {
    use super::module_map::{
        analyze_ffi_difficulty, is_internal_only_type, FfiDifficulty, INTERNAL_ONLY_TYPES,
    };
    use std::collections::{BTreeMap, BTreeSet, VecDeque};

    println!("\n=== Function Dependency Analysis ===\n");
    println!("Finding which API functions pull in difficult/internal types...\n");

    let version = match api_data.get_latest_version_str() {
        Some(v) => v.to_string(),
        None => {
            println!("No versions in api.json");
            return;
        }
    };

    let version_data = match api_data.0.get(&version) {
        Some(v) => v,
        None => {
            println!("Version {} not found", version);
            return;
        }
    };

    let api = &version_data.api;

    // Build a type dependency graph: type_name -> types it references
    let mut type_deps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // Track which types are difficult/internal
    let mut difficult_types: BTreeSet<String> = BTreeSet::new();
    let mut internal_types: BTreeSet<String> = BTreeSet::new();

    // First pass: collect all types and their direct dependencies
    for (_module_name, module) in api.iter() {
        for (class_name, class_def) in module.classes.iter() {
            let mut deps: BTreeSet<String> = BTreeSet::new();
            let mut has_difficult_field = false;

            // Check struct fields
            if let Some(fields) = &class_def.struct_fields {
                for field_map in fields.iter() {
                    for (_name, field) in field_map.iter() {
                        let diff = analyze_ffi_difficulty(&field.r#type);
                        // Only flag VeryHard types (BTreeMap, HashMap, Arc, etc.)
                        if diff >= FfiDifficulty::VeryHard {
                            has_difficult_field = true;
                        }
                        // Extract base type name from field type
                        if let Some(base) = extract_base_type(&field.r#type) {
                            deps.insert(base);
                        }
                    }
                }
            }

            // Check enum fields
            if let Some(variants) = &class_def.enum_fields {
                for variant_map in variants.iter() {
                    for (_variant_name, variant_data) in variant_map.iter() {
                        if let Some(field_type) = variant_data.r#type.as_ref() {
                            let diff = analyze_ffi_difficulty(field_type);
                            // Only flag VeryHard types (BTreeMap, HashMap, Arc, etc.)
                            if diff >= FfiDifficulty::VeryHard {
                                has_difficult_field = true;
                            }
                            if let Some(base) = extract_base_type(field_type) {
                                deps.insert(base);
                            }
                        }
                    }
                }
            }

            type_deps.insert(class_name.clone(), deps);

            if has_difficult_field {
                difficult_types.insert(class_name.clone());
            }
            if is_internal_only_type(class_name) {
                internal_types.insert(class_name.clone());
            }
        }
    }

    // Compute transitive closure: which types transitively depend on difficult/internal types
    // Uses iterative approach with cycle detection to avoid stack overflow
    fn reaches_difficult(
        type_name: &str,
        type_deps: &BTreeMap<String, BTreeSet<String>>,
        difficult_types: &BTreeSet<String>,
        internal_types: &BTreeSet<String>,
        cache: &mut BTreeMap<String, Option<Vec<String>>>,
    ) -> Option<Vec<String>> {
        // Check cache first
        if let Some(cached) = cache.get(type_name) {
            return cached.clone();
        }

        // Check if this type itself is difficult/internal
        if difficult_types.contains(type_name) || internal_types.contains(type_name) {
            let path = vec![type_name.to_string()];
            cache.insert(type_name.to_string(), Some(path.clone()));
            return Some(path);
        }

        // Use BFS with visited set to avoid cycles
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut queue: VecDeque<(String, Vec<String>)> = VecDeque::new();

        queue.push_back((type_name.to_string(), vec![type_name.to_string()]));
        visited.insert(type_name.to_string());

        while let Some((current, path)) = queue.pop_front() {
            // Limit path length to prevent infinite loops
            if path.len() > 20 {
                continue;
            }

            if let Some(deps) = type_deps.get(&current) {
                for dep in deps {
                    // Check if we reached a difficult/internal type
                    if difficult_types.contains(dep) || internal_types.contains(dep) {
                        let mut result_path = path.clone();
                        result_path.push(dep.clone());
                        cache.insert(type_name.to_string(), Some(result_path.clone()));
                        return Some(result_path);
                    }

                    // Continue search if not visited
                    if !visited.contains(dep) {
                        visited.insert(dep.clone());
                        let mut new_path = path.clone();
                        new_path.push(dep.clone());
                        queue.push_back((dep.clone(), new_path));
                    }
                }
            }
        }

        cache.insert(type_name.to_string(), None);
        None
    }

    // Analyze all functions
    struct FunctionProblem {
        function_path: String,  // e.g. "window.Window.new"
        arg_or_return: String,  // e.g. "arg: state" or "returns"
        direct_type: String,    // The direct type reference
        chain: Vec<String>,     // Chain to difficult type
        problem_type: String,   // The final difficult/internal type
        problem_reason: String, // Why it's difficult
    }

    let mut problems: Vec<FunctionProblem> = Vec::new();
    let mut cache: BTreeMap<String, Option<Vec<String>>> = BTreeMap::new();

    for (module_name, module) in api.iter() {
        for (class_name, class_def) in module.classes.iter() {
            // Check constructors
            if let Some(constructors) = &class_def.constructors {
                for (fn_name, fn_data) in constructors.iter() {
                    let fn_path = format!("{}.{}.{}", module_name, class_name, fn_name);

                    // Check arguments
                    for arg_map in &fn_data.fn_args {
                        for (arg_name, arg_type) in arg_map.iter() {
                            if let Some(base) = extract_base_type(arg_type) {
                                if let Some(chain) = reaches_difficult(
                                    &base,
                                    &type_deps,
                                    &difficult_types,
                                    &internal_types,
                                    &mut cache,
                                ) {
                                    let problem_type = chain.last().unwrap().clone();
                                    let reason = get_problem_reason(
                                        &problem_type,
                                        &difficult_types,
                                        &internal_types,
                                    );
                                    problems.push(FunctionProblem {
                                        function_path: fn_path.clone(),
                                        arg_or_return: format!("arg: {}", arg_name),
                                        direct_type: base.clone(),
                                        chain,
                                        problem_type,
                                        problem_reason: reason,
                                    });
                                }
                            }
                        }
                    }

                    // Check return type
                    if let Some(returns) = &fn_data.returns {
                        if let Some(base) = extract_base_type(&returns.r#type) {
                            if let Some(chain) = reaches_difficult(
                                &base,
                                &type_deps,
                                &difficult_types,
                                &internal_types,
                                &mut cache,
                            ) {
                                let problem_type = chain.last().unwrap().clone();
                                let reason = get_problem_reason(
                                    &problem_type,
                                    &difficult_types,
                                    &internal_types,
                                );
                                problems.push(FunctionProblem {
                                    function_path: fn_path.clone(),
                                    arg_or_return: "returns".to_string(),
                                    direct_type: base.clone(),
                                    chain,
                                    problem_type,
                                    problem_reason: reason,
                                });
                            }
                        }
                    }
                }
            }

            // Check regular functions
            if let Some(functions) = &class_def.functions {
                for (fn_name, fn_data) in functions.iter() {
                    let fn_path = format!("{}.{}.{}", module_name, class_name, fn_name);

                    // Check arguments
                    for arg_map in &fn_data.fn_args {
                        for (arg_name, arg_type) in arg_map.iter() {
                            if let Some(base) = extract_base_type(arg_type) {
                                if let Some(chain) = reaches_difficult(
                                    &base,
                                    &type_deps,
                                    &difficult_types,
                                    &internal_types,
                                    &mut cache,
                                ) {
                                    let problem_type = chain.last().unwrap().clone();
                                    let reason = get_problem_reason(
                                        &problem_type,
                                        &difficult_types,
                                        &internal_types,
                                    );
                                    problems.push(FunctionProblem {
                                        function_path: fn_path.clone(),
                                        arg_or_return: format!("arg: {}", arg_name),
                                        direct_type: base.clone(),
                                        chain,
                                        problem_type,
                                        problem_reason: reason,
                                    });
                                }
                            }
                        }
                    }

                    // Check return type
                    if let Some(returns) = &fn_data.returns {
                        if let Some(base) = extract_base_type(&returns.r#type) {
                            if let Some(chain) = reaches_difficult(
                                &base,
                                &type_deps,
                                &difficult_types,
                                &internal_types,
                                &mut cache,
                            ) {
                                let problem_type = chain.last().unwrap().clone();
                                let reason = get_problem_reason(
                                    &problem_type,
                                    &difficult_types,
                                    &internal_types,
                                );
                                problems.push(FunctionProblem {
                                    function_path: fn_path.clone(),
                                    arg_or_return: "returns".to_string(),
                                    direct_type: base.clone(),
                                    chain,
                                    problem_type,
                                    problem_reason: reason,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    // Deduplicate: collect unique (function_path, problem_type) pairs
    let mut function_problems: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut problem_reasons: BTreeMap<String, String> = BTreeMap::new();

    for problem in &problems {
        function_problems
            .entry(problem.function_path.clone())
            .or_default()
            .insert(problem.problem_type.clone());
        problem_reasons
            .entry(problem.problem_type.clone())
            .or_insert_with(|| problem.problem_reason.clone());
    }

    // Group problems by the problematic type (deduplicated by function)
    let mut by_problem_type: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (fn_path, problem_types) in &function_problems {
        for pt in problem_types {
            by_problem_type
                .entry(pt.clone())
                .or_default()
                .insert(fn_path.clone());
        }
    }

    // Print report
    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!("                    TRULY DIFFICULT TYPES (BTreeMap/HashMap/Arc)");
    println!("═══════════════════════════════════════════════════════════════════════════════\n");

    if by_problem_type.is_empty() {
        println!("✅ No truly difficult types found in API functions!");
        println!("   (Vec/String types are handled by existing FFI wrappers)\n");
    } else {
        let mut sorted_problems: Vec<_> = by_problem_type.iter().collect();
        sorted_problems.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        for (problem_type, affected_functions) in sorted_problems.iter() {
            let is_internal = internal_types.contains(*problem_type);
            let marker = if is_internal {
                "🔒 INTERNAL"
            } else {
                "⚠️  DIFFICULT"
            };
            let reason = problem_reasons
                .get(*problem_type)
                .map(|s| s.as_str())
                .unwrap_or("unknown");

            println!(
                "┌─────────────────────────────────────────────────────────────────────────────"
            );
            println!("│ {} {}", marker, problem_type);
            println!("│ Reason: {}", reason);
            println!("│ Affects {} unique function(s)", affected_functions.len());
            println!(
                "├─────────────────────────────────────────────────────────────────────────────"
            );

            for fn_path in affected_functions.iter().take(15) {
                println!("│   {}", fn_path);
            }

            if affected_functions.len() > 15 {
                println!(
                    "│   ... and {} more functions",
                    affected_functions.len() - 15
                );
            }
            println!(
                "└─────────────────────────────────────────────────────────────────────────────\n"
            );
        }
    }

    // Now show FUNCTIONS ranked by how many problems they have
    println!("═══════════════════════════════════════════════════════════════════════════════");
    println!("                    FUNCTIONS RANKED BY PROBLEM COUNT");
    println!("═══════════════════════════════════════════════════════════════════════════════\n");

    let mut sorted_functions: Vec<_> = function_problems.iter().collect();
    sorted_functions.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    if sorted_functions.is_empty() {
        println!("✅ No problematic functions found!\n");
    } else {
        for (fn_path, problem_types) in sorted_functions.iter().take(30) {
            let types_str: Vec<_> = problem_types.iter().take(3).cloned().collect();
            let suffix = if problem_types.len() > 3 {
                format!(" +{} more", problem_types.len() - 3)
            } else {
                String::new()
            };
            println!(
                "  {:3} problems: {} → {}{}",
                problem_types.len(),
                fn_path,
                types_str.join(", "),
                suffix
            );
        }

        if sorted_functions.len() > 30 {
            println!("\n  ... and {} more functions", sorted_functions.len() - 30);
        }
    }

    // Summary
    println!("\n═══════════════════════════════════════════════════════════════════════════════");
    println!("                                  SUMMARY");
    println!("═══════════════════════════════════════════════════════════════════════════════\n");

    let internal_count = by_problem_type
        .iter()
        .filter(|(t, _)| internal_types.contains(*t))
        .count();
    let difficult_count = by_problem_type.len() - internal_count;

    println!(
        "Problematic types (BTreeMap/HashMap/Arc/VecDeque): {}",
        by_problem_type.len()
    );
    println!("  🔒 Internal-only types: {}", internal_count);
    println!("  ⚠️  Other difficult types: {}", difficult_count);
    println!("Affected functions: {}", function_problems.len());
    println!();

    if !by_problem_type.is_empty() {
        let mut sorted_problems: Vec<_> = by_problem_type.iter().collect();
        sorted_problems.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

        println!("Top types to fix/remove:");
        for (i, (problem_type, funcs)) in sorted_problems.iter().take(10).enumerate() {
            let marker = if internal_types.contains(*problem_type) {
                "🔒"
            } else {
                "⚠️"
            };
            println!(
                "  {}. {} {} - affects {} functions",
                i + 1,
                marker,
                problem_type,
                funcs.len()
            );
        }

        println!();
        println!("RECOMMENDATIONS:");
        println!(
            "  1. Remove internal-only types (🔒) from api.json - they contain BTreeMap/HashMap"
        );
        println!("  2. Remove or redesign functions that expose these types");
        println!("  3. Note: Vec<T> and String are OK - they have FFI wrappers already");
    }
}

/// Extract base type name from a type string like "Vec<Foo>" -> "Foo", "Option<Bar>" -> "Bar"
fn extract_base_type(type_str: &str) -> Option<String> {
    let s = type_str.trim();

    // Skip primitives
    let primitives = [
        "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize",
        "f32", "f64", "bool", "char", "str", "()", "c_void",
    ];
    if primitives.contains(&s) {
        return None;
    }

    // Handle generic types: extract inner type
    if let Some(start) = s.find('<') {
        if let Some(end) = s.rfind('>') {
            let inner = &s[start + 1..end];
            // For tuple types, take first element
            if inner.contains(',') {
                let first = inner.split(',').next()?.trim();
                return extract_base_type(first);
            }
            return extract_base_type(inner);
        }
    }

    // Handle references
    let s = s.trim_start_matches('&').trim_start_matches("mut ");

    // Handle pointers
    let s = s.trim_start_matches("*const ").trim_start_matches("*mut ");

    // Return the type name if it looks like a custom type (starts with uppercase)
    let s = s.trim();
    if !s.is_empty() && s.chars().next()?.is_uppercase() {
        Some(s.to_string())
    } else {
        None
    }
}

/// Get reason why a type is problematic
fn get_problem_reason(
    type_name: &str,
    difficult_types: &std::collections::BTreeSet<String>,
    internal_types: &std::collections::BTreeSet<String>,
) -> String {
    use super::module_map::INTERNAL_ONLY_TYPES;

    if internal_types.contains(type_name) {
        return format!("Internal-only type (contains BTreeMap/HashMap/Arc)");
    }

    // Check what makes it difficult based on known patterns
    let lower = type_name.to_lowercase();
    if lower.contains("btreemap") || lower.contains("hashmap") {
        "Contains BTreeMap/HashMap (not C-compatible)".to_string()
    } else if lower.contains("arc") {
        "Contains Arc<T> (not C-compatible)".to_string()
    } else if lower.contains("vecdeque") {
        "Contains VecDeque (not C-compatible)".to_string()
    } else if lower.contains("mutex") || lower.contains("rwlock") {
        "Contains Mutex/RwLock (not C-compatible)".to_string()
    } else {
        "Contains non-FFI-safe field types".to_string()
    }
}

fn truncate_type(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
