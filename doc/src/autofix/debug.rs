//! Debug utilities for autofix V2
//!
//! This module provides debugging functionality to trace type resolution
//! and verify that the indexing and resolution works correctly.

use std::path::Path;
use anyhow::Result;

use super::type_index::{TypeIndex, TypeDefinition, TypeDefKind, FieldDef, RefKind};
use super::type_resolver::{TypeResolver, ResolutionContext, ResolvedTypeSet};
use super::diff::{ApiTypeResolution, resolve_api_types};
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
                
                match &def.kind {
                    TypeDefKind::Struct { fields, repr, derives, custom_impls, .. } => {
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
                    TypeDefKind::Enum { variants, repr, derives, custom_impls, .. } => {
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
                    TypeDefKind::MacroGenerated { source_macro, base_type, kind, .. } => {
                        println!("      Macro: {}", source_macro);
                        println!("      Base type: {}", base_type);
                        println!("      MacroKind: {:?}", kind);
                        
                        // Show the expanded form
                        let expanded = def.expand_macro_generated();
                        match &expanded {
                            TypeDefKind::Struct { fields, repr, derives, custom_impls, .. } => {
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
                            TypeDefKind::Enum { variants, repr, derives, custom_impls, .. } => {
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
                                println!("        Args: {:?}", args.iter().map(|a| &a.ty).collect::<Vec<_>>());
                                println!("        Returns: {:?}", returns);
                            }
                            _ => {}
                        }
                    }
                    TypeDefKind::TypeAlias { target, generic_base, generic_args } => {
                        println!("      Target: {}", target);
                        if let Some(base) = generic_base {
                            println!("      Generic base: {}", base);
                            println!("      Generic args: {:?}", generic_args);
                        }
                    }
                    TypeDefKind::CallbackTypedef { args, returns } => {
                        println!("      Args: {:?}", args.iter().map(|a| &a.ty).collect::<Vec<_>>());
                        println!("      Returns: {:?}", returns);
                    }
                }
            }
        }
        None => {
            println!("  NOT FOUND in index");
            
            // Try to find similar names
            let similar: Vec<_> = index.all_type_names()
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
    use syn::{File, Item};
    use quote::ToTokens;
    
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
                let macro_name = m.mac.path.segments
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

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::type_index::*;
    use std::path::PathBuf;

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
        use syn::{File, Item};
        use quote::ToTokens;
        use indexmap::IndexMap;
        
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
                                    ty: field.ty.to_token_stream().to_string()
                                        .split_whitespace().collect::<Vec<_>>().join(" "),
                                    ref_kind: RefKind::Value,
                                    doc: String::new(),
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
                            generic_params: vec![],
                            derives: vec![],
                        },
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
                            Some(variant.fields.iter()
                                .map(|f| f.ty.to_token_stream().to_string())
                                .collect::<Vec<_>>()
                                .join(", "))
                        };
                        
                        variants.insert(
                            variant_name.clone(),
                            VariantDef {
                                name: variant_name,
                                ty: variant_ty,
                                doc: String::new(),
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
                            generic_params: vec![],
                            derives: vec![],
                        },
                        source_code: String::new(),
                    });
                }
                
                _ => {}
            }
        }
        
        types
    }

    // ========================================================================
    // Test: Use imports are skipped
    // ========================================================================
    
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

    // ========================================================================
    // Test: Struct fields are correctly extracted
    // ========================================================================
    
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

    // ========================================================================
    // Test: Enum variants are correctly extracted
    // ========================================================================
    
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
                assert_eq!(variants.get("InvalidPosition").unwrap().ty.as_deref(), Some("TextPos"));
                assert_eq!(variants.get("ParseError").unwrap().ty.as_deref(), Some("ParseErrorInfo"));
            }
            _ => panic!("Expected Enum"),
        }
    }

    // ========================================================================
    // Test: Type resolution follows field types
    // ========================================================================
    
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

    // ========================================================================
    // Test: Cycle detection
    // ========================================================================
    
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

    // ========================================================================
    // Test: Missing type detection
    // ========================================================================
    
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

    // ========================================================================
    // Test: Prefer same crate resolution
    // ========================================================================
    
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
        
        let index = create_test_index(&[
            ("azul_core", source_core),
            ("azul_dll", source_dll),
        ]);
        
        // Should prefer azul_core over azul_dll
        let def = index.resolve("SharedType", None).unwrap();
        assert_eq!(def.crate_name, "azul_core");
        
        // But with preferred_crate, should use that
        let def_dll = index.resolve("SharedType", Some("azul_dll")).unwrap();
        assert_eq!(def_dll.crate_name, "azul_dll");
    }
}
