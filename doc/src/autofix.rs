/// Automatic API fixing - analyzes source code and generates patches
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use quote::ToTokens;

use crate::{
    api::{ApiData, CallbackArgData, CallbackDefinition, EnumVariantData, FieldData},
    discover,
    patch::{
        locatesource::{self, get_current_crate_name},
        parser::{self, SymbolInfo},
        ApiPatch, ClassPatch, ModulePatch, VersionPatch,
    },
};

/// Statistics about the autofix operation
#[derive(Debug, Default)]
pub struct AutofixStats {
    pub total_types_checked: usize,
    pub types_with_missing_external: usize,
    pub types_with_mismatched_fields: usize,
    pub patches_generated: usize,
    pub types_needing_manual_fix: Vec<String>,
}

impl AutofixStats {
    pub fn print_summary(&self) {
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║                    Autofix Summary                             ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        println!("📊 Statistics:");
        println!("  Total types checked: {}", self.total_types_checked);
        println!(
            "  Types with missing external paths: {}",
            self.types_with_missing_external
        );
        println!(
            "  Types with mismatched fields: {}",
            self.types_with_mismatched_fields
        );
        println!("  Patches generated: {}", self.patches_generated);

        if !self.types_needing_manual_fix.is_empty() {
            println!(
                "\n⚠️  Types needing manual attention ({}):",
                self.types_needing_manual_fix.len()
            );
            for type_path in &self.types_needing_manual_fix {
                println!("  • {}", type_path);
            }
        }

        if self.patches_generated > 0 {
            println!("\n✅ Generated {} patch files", self.patches_generated);
        } else {
            println!("\n✨ No patches needed - API is up to date!");
        }
    }
}

/// Generate patches for all issues found in the API
pub fn autofix_api(
    api_data: &ApiData,
    project_root: &Path,
    output_dir: &Path,
) -> Result<AutofixStats> {
    println!("🔍 Analyzing API and source code...\n");

    let mut stats = AutofixStats::default();

    // Use target directory for all temporary and generated files
    let work_dir = project_root.join("target").join("autofix");
    fs::create_dir_all(&work_dir)
        .with_context(|| format!("Failed to create work directory: {}", work_dir.display()))?;

    // Step 0: Get cargo metadata once at the start
    println!("  🔧 Loading cargo metadata...");
    use crate::patch::locatesource;
    let cargo_metadata =
        locatesource::get_cargo_metadata(project_root).context("Failed to get cargo metadata")?;
    let current_crate_name = get_current_crate_name(project_root)?;
    println!(
        "  Loaded metadata for workspace with {} packages\n",
        cargo_metadata.packages.len()
    );

    // Step 1: Collect all types that need to be checked
    println!("  📋 Collecting all types from API...");
    let mut all_types = Vec::new();
    for (_version_name, version_data) in &api_data.0 {
        for (module_name, module_data) in &version_data.api {
            for (class_name, _class_data) in &module_data.classes {
                all_types.push((module_name.clone(), class_name.clone()));
            }
        }
    }
    println!("  Found {} types to analyze\n", all_types.len());

    // Step 2: Use compiler as oracle to discover correct type paths
    println!("  🔮 Using compiler as oracle to discover correct paths...");

    // Check if we have cached results
    let cache_file = work_dir.join("cache.json");
    let discovered_infos = if cache_file.exists() {
        println!("  📦 Loading cached discovered types...");
        let cache_content = fs::read_to_string(&cache_file).context("Failed to read cache file")?;
        serde_json::from_str(&cache_content).context("Failed to parse cache file")?
    } else {
        let infos = discover::discover_type_paths(project_root, &all_types)?;
        // Save to cache
        let cache_content = serde_json::to_string_pretty(&infos)?;
        fs::write(&cache_file, cache_content)?;
        println!("  💾 Saved discovered types to cache");
        infos
    };
    println!("  Discovered {} type paths\n", discovered_infos.len());

    // Step 3: Analyze source code for all types (existing + newly discovered)
    println!("  📚 Analyzing source code for all types...");
    let enriched_infos = enrich_with_source_analysis(
        project_root,
        &work_dir,
        api_data,
        &discovered_infos,
        &current_crate_name,
        &cargo_metadata,
    )?;
    println!("  Analyzed {} types from source\n", enriched_infos.len());

    // Step 4: Generate patches by comparing analyzed types with current API
    println!("  📝 Generating patches by comparing with current API...\n");
    let patches_dir = work_dir.join("patches");
    fs::create_dir_all(&patches_dir)?;
    generate_patches_from_analysis(api_data, &work_dir, &patches_dir, &mut stats)?;

    // Print summary
    println!("\n✅ Autofix completed!\n");
    println!("📂 Discovered types: {}", work_dir.join("types").display());
    println!("📂 Generated patches: {}", patches_dir.display());
    println!(
        "📊 Summary: {} patches generated for {} types\n",
        stats.patches_generated, stats.total_types_checked
    );

    Ok(stats)
}

/// Check if fields differ between API and oracle
fn fields_differ(
    api_fields: &IndexMap<String, FieldData>,
    oracle_fields: &IndexMap<String, FieldData>,
) -> bool {
    if api_fields.len() != oracle_fields.len() {
        return true;
    }

    for (name, api_field) in api_fields {
        if let Some(oracle_field) = oracle_fields.get(name) {
            if api_field.r#type != oracle_field.r#type {
                return true;
            }
        } else {
            return true; // Field missing in oracle
        }
    }

    // Check order (important for struct layout)
    let api_names: Vec<_> = api_fields.keys().collect();
    let oracle_names: Vec<_> = oracle_fields.keys().collect();
    api_names != oracle_names
}

/// Check if enum variants differ between API and oracle
fn variants_differ(
    api_variants: &IndexMap<String, EnumVariantData>,
    oracle_variants: &IndexMap<String, EnumVariantData>,
) -> bool {
    if api_variants.len() != oracle_variants.len() {
        return true;
    }

    for (name, api_variant) in api_variants {
        if let Some(oracle_variant) = oracle_variants.get(name) {
            let api_ty = api_variant.r#type.as_deref().unwrap_or("unit");
            let oracle_ty = oracle_variant.r#type.as_deref().unwrap_or("unit");
            if api_ty != oracle_ty {
                return true;
            }
        } else {
            return true; // Variant missing in oracle
        }
    }

    // Check order (important for discriminant values)
    let api_names: Vec<_> = api_variants.keys().collect();
    let oracle_names: Vec<_> = oracle_variants.keys().collect();
    api_names != oracle_names
}

/// Enrich type information by analyzing source code using locate_source
fn enrich_with_source_analysis(
    project_root: &Path,
    work_dir: &Path,
    api_data: &ApiData,
    discovered_infos: &HashMap<String, discover::OracleTypeInfo>,
    current_crate_name: &str,
    cargo_metadata: &crate::patch::locatesource::CargoMetadata,
) -> Result<HashMap<String, discover::OracleTypeInfo>> {
    use rayon::prelude::*;

    // Collect all types to analyze: existing types + newly discovered
    let mut all_type_paths: Vec<(String, String)> = Vec::new();

    // Add all existing types from API with their current external paths
    for (_version_name, version_data) in &api_data.0 {
        for (_module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(ref external_path) = class_data.external {
                    all_type_paths.push((class_name.clone(), external_path.clone()));
                }
            }
        }
    }

    // Add newly discovered types
    for (type_name, info) in discovered_infos {
        if let Some(ref path) = info.correct_path {
            // Only add if not already present
            if !all_type_paths.iter().any(|(name, _)| name == type_name) {
                all_type_paths.push((type_name.clone(), path.clone()));
            }
        }
    }

    println!(
        "    Analyzing {} unique types in parallel...",
        all_type_paths.len()
    );

    // Create output directory for individual type patches
    let types_output_dir = work_dir.join("types");
    fs::create_dir_all(&types_output_dir).with_context(|| {
        format!(
            "Failed to create types output directory: {}",
            types_output_dir.display()
        )
    })?;

    // Process all types in parallel using rayon
    // Clone metadata for each thread
    let analyzed: Vec<(String, Result<discover::OracleTypeInfo>)> = all_type_paths
        .par_iter()
        .map(|(type_name, type_path)| {
            let result = analyze_type_from_source(
                project_root,
                type_name,
                type_path,
                current_crate_name,
                cargo_metadata,
            );

            // Write result to disk immediately
            if let Ok(ref info) = result {
                let type_file = types_output_dir.join(format!("{}.patch.json", type_name));
                if let Ok(json) = serde_json::to_string_pretty(info) {
                    let _ = fs::write(&type_file, json);
                }
            }

            (type_name.clone(), result)
        })
        .collect();

    // Collect successful analyses
    let mut enriched_infos = HashMap::new();
    let mut success_count = 0;
    let mut failure_count = 0;

    for (type_name, result) in analyzed {
        match result {
            Ok(info) => {
                enriched_infos.insert(type_name, info);
                success_count += 1;
            }
            Err(e) => {
                // If analysis failed, try to use the discovered info if available
                if let Some(info) = discovered_infos.get(&type_name) {
                    enriched_infos.insert(type_name, info.clone());
                }
                failure_count += 1;
                // Don't print errors for each failure - too noisy
            }
        }
    }

    println!(
        "    Successfully analyzed: {}, Failed: {}",
        success_count, failure_count
    );

    Ok(enriched_infos)
}

/// Analyze a single type from its source code
fn analyze_type_from_source(
    project_root: &Path,
    type_name: &str,
    type_path: &str,
    current_crate_name: &str,
    cargo_metadata: &crate::patch::locatesource::CargoMetadata,
) -> Result<discover::OracleTypeInfo> {
    use syn::{File, Item};

    use crate::patch::locatesource;

    // Extract the actual type name from the path (last segment)
    // e.g., "azul_core::id::NodeId" -> "NodeId"
    let actual_type_name = type_path.split("::").last().unwrap_or(type_name);

    // Use retrieve_item_source_with_metadata with pre-computed metadata
    let source_code = locatesource::retrieve_item_source_with_metadata(
        project_root,
        type_path,
        current_crate_name,
        cargo_metadata,
    )
    .with_context(|| format!("Failed to locate source for {}", type_path))?;

    // Parse the source code directly
    let syntax_tree: File = syn::parse_str(&source_code).context("Failed to parse source code")?;

    // Find the type definition
    for item in syntax_tree.items {
        match item {
            Item::Struct(s) if s.ident == actual_type_name => {
                let mut fields = IndexMap::new();
                let has_repr_c = s.attrs.iter().any(|attr| {
                    attr.path().is_ident("repr")
                        && attr.meta.to_token_stream().to_string().contains("C")
                });

                // Extract doc comments
                let _doc = extract_doc_comments(&s.attrs);

                eprintln!("✓ Found struct {}: {} fields", type_name, s.fields.len());

                // Extract fields
                for field in s.fields.iter() {
                    let field_name = field
                        .ident
                        .as_ref()
                        .map(|i| i.to_string())
                        .unwrap_or_else(|| "tuple_field".to_string());

                    let field_type = field.ty.to_token_stream().to_string();
                    let field_doc = extract_doc_comments(&field.attrs);

                    fields.insert(
                        field_name,
                        FieldData {
                            r#type: clean_type_string(&field_type),
                            doc: field_doc,
                            derive: None,
                        },
                    );
                }

                return Ok(discover::OracleTypeInfo {
                    correct_path: Some(type_path.to_string()),
                    fields,
                    variants: IndexMap::new(),
                    has_repr_c,
                    is_enum: false,
                });
            }
            Item::Enum(e) if e.ident == actual_type_name => {
                let mut variants = IndexMap::new();
                let has_repr_c = e.attrs.iter().any(|attr| {
                    attr.path().is_ident("repr")
                        && attr.meta.to_token_stream().to_string().contains("C")
                });

                // Extract doc comments
                let _doc = extract_doc_comments(&e.attrs);

                eprintln!("✓ Found enum {}: {} variants", type_name, e.variants.len());

                // Extract variants
                for variant in &e.variants {
                    let variant_name = variant.ident.to_string();
                    let variant_doc = extract_doc_comments(&variant.attrs);

                    // Format variant type - clean up the parentheses
                    let variant_type = if variant.fields.is_empty() {
                        None
                    } else {
                        let fields_str = variant
                            .fields
                            .iter()
                            .map(|f| f.ty.to_token_stream().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        // Remove surrounding parentheses if present
                        Some(clean_type_string(&fields_str))
                    };

                    variants.insert(
                        variant_name,
                        EnumVariantData {
                            r#type: variant_type,
                            doc: variant_doc,
                        },
                    );
                }

                return Ok(discover::OracleTypeInfo {
                    correct_path: Some(type_path.to_string()),
                    fields: IndexMap::new(),
                    variants,
                    has_repr_c,
                    is_enum: true,
                });
            }
            _ => {}
        }
    }

    eprintln!(
        "✗ Type {} (actual_type_name: {}) not found in source",
        type_name, actual_type_name
    );
    anyhow::bail!("Type {} not found in source", type_name)
}

/// Extract doc comments from attributes
fn extract_doc_comments(attrs: &[syn::Attribute]) -> Option<String> {
    let doc_lines: Vec<String> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        return Some(lit_str.value().trim().to_string());
                    }
                }
            }
            None
        })
        .collect();

    if doc_lines.is_empty() {
        None
    } else {
        // Join with newlines first, then normalize to spaces
        let joined = doc_lines.join("\n");
        
        // Normalize newlines, tabs, and multiple spaces to single spaces
        let normalized = joined
            .replace("\n", " ")
            .replace("\t", " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        
        Some(normalized)
    }
}

/// Detect and normalize generic types (Vec<T>, Option<T>, Result<T,E>)
/// Returns (normalized_type, optional_generic_info)
fn normalize_generic_type(type_str: &str) -> (String, Option<GenericTypeInfo>) {
    // Remove all spaces first (syn returns types with spaces like "Option < Box < T > >")
    let no_spaces = type_str.replace(" ", "");
    let trimmed = no_spaces.trim();
    
    // Special case: Option<Box<T>> -> *const c_void (opaque Rust types in FFI)
    if let Some(inner) = extract_generic_type(trimmed, "Option") {
        if let Some(_box_inner) = extract_generic_type(&inner, "Box") {
            // Option<Box<T>> is opaque in the FFI API
            return ("*const c_void".to_string(), None);
        }
        
        // Not Option<Box<T>>, so normalize normally
        // Recursively normalize the inner type
        let (inner_normalized, _) = normalize_generic_type(&inner);
        let inner_clean = inner_normalized.replace(" ", "");
        let normalized = format!("Option{}", inner_clean);
        return (
            normalized.clone(),
            Some(GenericTypeInfo::Option {
                inner_type: inner_clean,
                normalized_name: normalized,
            }),
        );
    }
    
    // Check for Vec<T>
    if let Some(inner) = extract_generic_type(trimmed, "Vec") {
        // Recursively normalize the inner type
        let (inner_normalized, _) = normalize_generic_type(&inner);
        let inner_clean = inner_normalized.replace(" ", "");
        let normalized = format!("{}Vec", inner_clean);
        return (
            normalized.clone(),
            Some(GenericTypeInfo::Vec {
                inner_type: inner_clean,
                normalized_name: normalized,
            }),
        );
    }
    
    // Check for Box<T> (special case - treat like Option)
    if let Some(inner) = extract_generic_type(trimmed, "Box") {
        // Recursively normalize the inner type
        let (inner_normalized, _) = normalize_generic_type(&inner);
        let inner_clean = inner_normalized.replace(" ", "");
        let normalized = format!("Box{}", inner_clean);
        return (normalized, None); // Don't track Box types for now
    }
    
    // Check for Result<T, E>
    if let Some(inner) = extract_generic_type(trimmed, "Result") {
        // Parse Result<T, E> - split by comma at top level
        let parts = split_generic_args(&inner);
        if parts.len() == 2 {
            // Recursively normalize both types
            let (ok_normalized, _) = normalize_generic_type(&parts[0]);
            let (err_normalized, _) = normalize_generic_type(&parts[1]);
            let ok_type = ok_normalized.replace(" ", "");
            let err_type = err_normalized.replace(" ", "");
            let normalized = format!("Result{}{}", ok_type, err_type);
            return (
                normalized.clone(),
                Some(GenericTypeInfo::Result {
                    ok_type,
                    err_type,
                    normalized_name: normalized,
                }),
            );
        }
    }
    
    (type_str.to_string(), None)
}

/// Extract the inner type from a generic like Vec<T> or Option<T>
/// Assumes type_str has no spaces (call normalize_generic_type first)
fn extract_generic_type(type_str: &str, generic_name: &str) -> Option<String> {
    let prefix = format!("{}<", generic_name);
    
    if type_str.starts_with(&prefix) && type_str.ends_with('>') {
        let start = prefix.len();
        let end = type_str.len() - 1;
        if start < end {
            return Some(type_str[start..end].to_string());
        }
    }
    
    None
}

/// Split generic arguments by comma at the top level
fn split_generic_args(args: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    
    for ch in args.chars() {
        match ch {
            '<' | '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            '>' | ')' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                result.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }
    
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    
    result
}

/// Get the normalized type name from GenericTypeInfo
fn get_generic_type_name(info: &GenericTypeInfo) -> String {
    match info {
        GenericTypeInfo::Vec { normalized_name, .. } => normalized_name.clone(),
        GenericTypeInfo::Option { normalized_name, .. } => normalized_name.clone(),
        GenericTypeInfo::Result { normalized_name, .. } => normalized_name.clone(),
    }
}

/// Information about a detected generic type
#[derive(Debug, Clone)]
enum GenericTypeInfo {
    Vec {
        inner_type: String,
        normalized_name: String,
    },
    Option {
        inner_type: String,
        normalized_name: String,
    },
    Result {
        ok_type: String,
        err_type: String,
        normalized_name: String,
    },
}

/// Clean up type strings by removing unnecessary parentheses, extracting last path segment,
/// and normalizing generic types
fn clean_type_string(type_str: &str) -> String {
    // First, remove spaces around :: (syn's token stream adds these)
    let type_str = type_str.replace(" :: ", "::");

    // If it's a single type wrapped in parentheses, remove them
    // e.g., "(usize)" -> "usize"
    let trimmed = type_str.trim();
    let unwrapped = if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len() - 1];
        // Check if there's only one type (no comma outside of nested structures)
        let mut depth = 0;
        let mut has_comma_at_top_level = false;
        for ch in inner.chars() {
            match ch {
                '<' | '(' | '[' => depth += 1,
                '>' | ')' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    has_comma_at_top_level = true;
                    break;
                }
                _ => {}
            }
        }
        if !has_comma_at_top_level {
            inner
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    // Normalize generic types (Vec<T> -> TVec, Option<T> -> OptionT, Result<T,E> -> ResultTE)
    // Also handles Option<Box<T>> -> *const c_void
    let (normalized, generic_info) = normalize_generic_type(unwrapped);
    
    // If normalization changed the type, return the normalized version
    if normalized != unwrapped {
        return normalized;
    }
    
    // If it's a tracked generic type, we already normalized it
    if generic_info.is_some() {
        return normalized;
    }

    // Extract the last segment of the path (after the last ::)
    // e.g., "crate::thread::CreateThreadCallback" -> "CreateThreadCallback"
    if let Some(last_segment_pos) = unwrapped.rfind("::") {
        unwrapped[last_segment_pos + 2..].to_string()
    } else {
        unwrapped.to_string()
    }
}

/// Generate final patches by comparing analyzed types with current API
fn generate_patches_from_analysis(
    api_data: &ApiData,
    work_dir: &Path,
    output_dir: &Path,
    stats: &mut AutofixStats,
) -> Result<()> {
    let types_dir = work_dir.join("types");
    // output_dir is already the patches directory
    fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "Failed to create patches directory: {}",
            output_dir.display()
        )
    })?;

    // Track all generic types that need to be generated
    let mut generic_types_needed: HashMap<String, GenericTypeInfo> = HashMap::new();

    for (version_name, version_data) in &api_data.0 {
        println!("📦 Generating patches for version: {}", version_name);

        for (module_name, module_data) in &version_data.api {
            for (class_name, class_data) in &module_data.classes {
                stats.total_types_checked += 1;

                // Try to load the analyzed type info
                let type_file = types_dir.join(format!("{}.patch.json", class_name));
                if !type_file.exists() {
                    continue;
                }

                let analyzed_info: discover::OracleTypeInfo = match fs::read_to_string(&type_file) {
                    Ok(content) => match serde_json::from_str(&content) {
                        Ok(info) => info,
                        Err(e) => {
                            eprintln!("⚠️  Failed to parse {}: {}", type_file.display(), e);
                            continue;
                        }
                    },
                    Err(_) => continue,
                };

                // Scan struct fields for generic types
                for (_, field_data) in &analyzed_info.fields {
                    let type_str = &field_data.r#type;
                    let (_, generic_info) = normalize_generic_type(type_str);
                    if let Some(info) = generic_info {
                        let name = get_generic_type_name(&info);
                        generic_types_needed.insert(name, info);
                    }
                }

                // Scan enum variants for generic types
                for (_, variant_data) in &analyzed_info.variants {
                    if let Some(ref type_str) = variant_data.r#type {
                        let (_, generic_info) = normalize_generic_type(type_str);
                        if let Some(info) = generic_info {
                            let name = get_generic_type_name(&info);
                            generic_types_needed.insert(name, info);
                        }
                    }
                }

                // Compare and generate patch if needed
                let mut class_patch = ClassPatch::default();
                let mut needs_patch = false;

                // Check external path
                if class_data.external != analyzed_info.correct_path {
                    if let Some(ref correct_path) = analyzed_info.correct_path {
                        class_patch.external = Some(correct_path.clone());
                        needs_patch = true;
                    }
                }

                // Check struct fields
                if !analyzed_info.fields.is_empty() {
                    let needs_update = match &class_data.struct_fields {
                        None => true,
                        Some(v) if v.is_empty() => true,
                        Some(v) => {
                            // Compare fields manually since IndexMap doesn't implement PartialEq
                            // with different types
                            match v.first() {
                                None => true,
                                Some(existing) => {
                                    // Simple comparison: if sizes differ, they're different
                                    if existing.len() != analyzed_info.fields.len() {
                                        true
                                    } else {
                                        // Check if all keys and values match
                                        existing.iter().any(|(k, v)| {
                                            analyzed_info
                                                .fields
                                                .get(k)
                                                .map(|av| av != v)
                                                .unwrap_or(true)
                                        }) || analyzed_info
                                            .fields
                                            .iter()
                                            .any(|(k, _)| !existing.contains_key(k))
                                    }
                                }
                            }
                        }
                    };
                    if needs_update {
                        class_patch.struct_fields = Some(vec![analyzed_info.fields.clone()]);
                        needs_patch = true;
                    }
                }

                // Check enum variants
                if !analyzed_info.variants.is_empty() {
                    let needs_update = match &class_data.enum_fields {
                        None => true,
                        Some(v) if v.is_empty() => true,
                        Some(v) => match v.first() {
                            None => true,
                            Some(existing) => {
                                if existing.len() != analyzed_info.variants.len() {
                                    true
                                } else {
                                    existing.iter().any(|(k, v)| {
                                        analyzed_info
                                            .variants
                                            .get(k)
                                            .map(|av| av != v)
                                            .unwrap_or(true)
                                    }) || analyzed_info
                                        .variants
                                        .iter()
                                        .any(|(k, _)| !existing.contains_key(k))
                                }
                            }
                        },
                    };
                    if needs_update {
                        class_patch.enum_fields = Some(vec![analyzed_info.variants.clone()]);
                        needs_patch = true;
                    }
                }

                // Generate individual patch file for this type if needed
                if needs_patch {
                    println!("  📝 {}.{}: Generating patch", module_name, class_name);

                    let patch = ApiPatch {
                        versions: BTreeMap::from([(
                            version_name.clone(),
                            VersionPatch {
                                modules: BTreeMap::from([(
                                    module_name.clone(),
                                    ModulePatch {
                                        classes: BTreeMap::from([(
                                            class_name.clone(),
                                            class_patch,
                                        )]),
                                    },
                                )]),
                            },
                        )]),
                    };

                    let patch_filename = format!("{}.patch.json", class_name);
                    let patch_path = output_dir.join(&patch_filename);

                    let patch_json = serde_json::to_string_pretty(&patch)?;
                    fs::write(&patch_path, patch_json).with_context(|| {
                        format!("Failed to write patch file: {}", patch_path.display())
                    })?;

                    stats.patches_generated += 1;
                }
            }
        }
    }

    // Generate patches for missing generic wrapper types
    if !generic_types_needed.is_empty() {
        println!("\n📦 Generating patches for {} generic wrapper types...", generic_types_needed.len());
        generate_generic_type_patches(
            api_data,
            &generic_types_needed,
            output_dir,
            stats,
        )?;
    }

    Ok(())
}

/// Generate patches for generic wrapper types (Vec, Option, Result)
fn generate_generic_type_patches(
    api_data: &ApiData,
    generic_types: &HashMap<String, GenericTypeInfo>,
    output_dir: &Path,
    stats: &mut AutofixStats,
) -> Result<()> {
    for (type_name, generic_info) in generic_types {
        // Check if this type already exists in the API
        let type_exists = api_data.0.iter().any(|(_, version_data)| {
            version_data.api.iter().any(|(_, module_data)| {
                module_data.classes.contains_key(type_name)
            })
        });

        if type_exists {
            // Type already exists, skip
            continue;
        }

        println!("  ⚙️  Generating wrapper type: {}", type_name);

        match generic_info {
            GenericTypeInfo::Vec { inner_type, normalized_name } => {
                // Generate Vec wrapper, VecDestructor, and VecDestructorType
                generate_vec_wrapper_patches(
                    api_data,
                    inner_type,
                    normalized_name,
                    output_dir,
                    stats,
                )?;
            }
            GenericTypeInfo::Option { inner_type, normalized_name } => {
                // Generate Option wrapper
                generate_option_wrapper_patch(
                    api_data,
                    inner_type,
                    normalized_name,
                    output_dir,
                    stats,
                )?;
            }
            GenericTypeInfo::Result { ok_type, err_type, normalized_name } => {
                // Generate Result wrapper
                generate_result_wrapper_patch(
                    api_data,
                    ok_type,
                    err_type,
                    normalized_name,
                    output_dir,
                    stats,
                )?;
            }
        }
    }

    Ok(())
}

/// Generate patches for Vec<T> wrapper types
fn generate_vec_wrapper_patches(
    _api_data: &ApiData,
    inner_type: &str,
    normalized_name: &str,
    output_dir: &Path,
    stats: &mut AutofixStats,
) -> Result<()> {
    // For Vec<T>, we need to generate:
    // 1. TVec (the vec itself)
    // 2. TVecDestructor
    // 3. TVecDestructorType

    // Generate TVec
    let vec_patch = create_vec_patch(normalized_name);
    save_patch(&vec_patch, normalized_name, output_dir)?;
    stats.patches_generated += 1;

    // Generate TVecDestructor
    let destructor_name = format!("{}Destructor", normalized_name);
    let destructor_patch = create_vec_destructor_patch(&destructor_name, normalized_name);
    save_patch(&destructor_patch, &destructor_name, output_dir)?;
    stats.patches_generated += 1;

    // Generate TVecDestructorType
    let destructor_type_name = format!("{}DestructorType", normalized_name);
    let destructor_type_patch = create_vec_destructor_type_patch(&destructor_type_name, normalized_name);
    save_patch(&destructor_type_patch, &destructor_type_name, output_dir)?;
    stats.patches_generated += 1;

    println!("    ✓ Generated {}, {}Destructor, {}DestructorType", normalized_name, normalized_name, normalized_name);

    Ok(())
}

/// Generate patch for Option<T> wrapper type
fn generate_option_wrapper_patch(
    _api_data: &ApiData,
    inner_type: &str,
    normalized_name: &str,
    output_dir: &Path,
    stats: &mut AutofixStats,
) -> Result<()> {
    let patch = create_option_patch(normalized_name, inner_type);
    save_patch(&patch, normalized_name, output_dir)?;
    stats.patches_generated += 1;

    println!("    ✓ Generated {}", normalized_name);

    Ok(())
}

/// Generate patch for Result<T, E> wrapper type
fn generate_result_wrapper_patch(
    _api_data: &ApiData,
    ok_type: &str,
    err_type: &str,
    normalized_name: &str,
    output_dir: &Path,
    stats: &mut AutofixStats,
) -> Result<()> {
    let patch = create_result_patch(normalized_name, ok_type, err_type);
    save_patch(&patch, normalized_name, output_dir)?;
    stats.patches_generated += 1;

    println!("    ✓ Generated {}", normalized_name);

    Ok(())
}

/// Create a patch for Vec<T> type
fn create_vec_patch(normalized_name: &str) -> ApiPatch {
    // TVec will be created as an empty class that needs external path from compiler
    ApiPatch {
        versions: BTreeMap::from([(
            "1.0.0-alpha1".to_string(),
            VersionPatch {
                modules: BTreeMap::from([(
                    "vec".to_string(),
                    ModulePatch {
                        classes: BTreeMap::from([(
                            normalized_name.to_string(),
                            ClassPatch {
                                external: Some(format!("azul_css::{}", normalized_name)),
                                ..Default::default()
                            },
                        )]),
                    },
                )]),
            },
        )]),
    }
}

/// Create a patch for Vec destructor
fn create_vec_destructor_patch(destructor_name: &str, vec_name: &str) -> ApiPatch {
    use indexmap::IndexMap;
    
    let mut enum_fields = IndexMap::new();
    enum_fields.insert("DefaultRust".to_string(), EnumVariantData::default());
    enum_fields.insert("NoDestructor".to_string(), EnumVariantData::default());
    enum_fields.insert(
        "External".to_string(),
        EnumVariantData {
            r#type: Some(format!("{}Type", destructor_name)),
            doc: None,
        },
    );

    ApiPatch {
        versions: BTreeMap::from([(
            "1.0.0-alpha1".to_string(),
            VersionPatch {
                modules: BTreeMap::from([(
                    "vec".to_string(),
                    ModulePatch {
                        classes: BTreeMap::from([(
                            destructor_name.to_string(),
                            ClassPatch {
                                external: Some(format!("azul_css::{}", destructor_name)),
                                derive: Some(vec!["Copy".to_string()]),
                                enum_fields: Some(vec![enum_fields]),
                                ..Default::default()
                            },
                        )]),
                    },
                )]),
            },
        )]),
    }
}

/// Create a patch for Vec destructor type (callback typedef)
fn create_vec_destructor_type_patch(destructor_type_name: &str, vec_name: &str) -> ApiPatch {
    ApiPatch {
        versions: BTreeMap::from([(
            "1.0.0-alpha1".to_string(),
            VersionPatch {
                modules: BTreeMap::from([(
                    "vec".to_string(),
                    ModulePatch {
                        classes: BTreeMap::from([(
                            destructor_type_name.to_string(),
                            ClassPatch {
                                callback_typedef: Some(CallbackDefinition {
                                    fn_args: vec![CallbackArgData {
                                        r#type: vec_name.to_string(),
                                        ref_kind: "refmut".to_string(),
                                        doc: None,
                                    }],
                                    returns: None,
                                }),
                                ..Default::default()
                            },
                        )]),
                    },
                )]),
            },
        )]),
    }
}

/// Create a patch for Option<T> type
fn create_option_patch(normalized_name: &str, inner_type: &str) -> ApiPatch {
    use indexmap::IndexMap;
    
    let mut enum_fields = IndexMap::new();
    enum_fields.insert("None".to_string(), EnumVariantData::default());
    enum_fields.insert(
        "Some".to_string(),
        EnumVariantData {
            r#type: Some(inner_type.to_string()),
            doc: None,
        },
    );

    ApiPatch {
        versions: BTreeMap::from([(
            "1.0.0-alpha1".to_string(),
            VersionPatch {
                modules: BTreeMap::from([(
                    "option".to_string(),
                    ModulePatch {
                        classes: BTreeMap::from([(
                            normalized_name.to_string(),
                            ClassPatch {
                                external: Some(format!("azul_css::{}", normalized_name)),
                                enum_fields: Some(vec![enum_fields]),
                                ..Default::default()
                            },
                        )]),
                    },
                )]),
            },
        )]),
    }
}

/// Create a patch for Result<T, E> type
fn create_result_patch(normalized_name: &str, ok_type: &str, err_type: &str) -> ApiPatch {
    use indexmap::IndexMap;
    
    let mut enum_fields = IndexMap::new();
    enum_fields.insert(
        "Ok".to_string(),
        EnumVariantData {
            r#type: Some(ok_type.to_string()),
            doc: None,
        },
    );
    enum_fields.insert(
        "Err".to_string(),
        EnumVariantData {
            r#type: Some(err_type.to_string()),
            doc: None,
        },
    );

    ApiPatch {
        versions: BTreeMap::from([(
            "1.0.0-alpha1".to_string(),
            VersionPatch {
                modules: BTreeMap::from([(
                    "error".to_string(),
                    ModulePatch {
                        classes: BTreeMap::from([(
                            normalized_name.to_string(),
                            ClassPatch {
                                external: Some(format!("azul_core::{}", normalized_name)),
                                enum_fields: Some(vec![enum_fields]),
                                ..Default::default()
                            },
                        )]),
                    },
                )]),
            },
        )]),
    }
}

/// Save a patch to disk
fn save_patch(patch: &ApiPatch, type_name: &str, output_dir: &Path) -> Result<()> {
    let patch_filename = format!("{}.patch.json", type_name);
    let patch_path = output_dir.join(&patch_filename);

    let patch_json = serde_json::to_string_pretty(patch)?;
    fs::write(&patch_path, patch_json).with_context(|| {
        format!("Failed to write patch file: {}", patch_path.display())
    })?;

    Ok(())
}
