//! Type Resolution V2
//!
//! This module handles resolving type chains from entry points (functions)
//! to all referenced types. It stops at primitives and tracks the resolution chain.
//!
//! Key features:
//! - Resolves types through pointers (`*const T`, `*mut T`)
//! - Resolves types through arrays (`[T; N]`)
//! - Warns about non-C-compatible types like `Vec<T>`
//!
//! NOTE: Type extraction is done on syn::Type AST nodes, not string manipulation.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use quote::ToTokens;
use syn::{Type, TypePath, TypePtr, TypeReference, TypeArray, TypeSlice, TypeTuple, TypeBareFn, TypeParen};

use super::type_index::{TypeIndex, TypeDefinition, TypeDefKind};

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// All types reachable from a set of entry points
#[derive(Debug, Default)]
pub struct ResolvedTypeSet {
    /// Types successfully resolved with their full paths
    pub resolved: HashMap<String, ResolvedType>,
    /// Types that could not be resolved (with context)
    pub unresolved: HashMap<String, UnresolvedInfo>,
    /// Warnings about non-C-compatible types
    pub warnings: Vec<TypeWarning>,
}

/// Warning about a type that is not C-compatible
#[derive(Debug, Clone)]
pub struct TypeWarning {
    /// The problematic type expression (e.g., "Vec<FontCache>")
    pub type_expr: String,
    /// Where this was found
    pub context: String,
    /// Description of the problem
    pub message: String,
}

/// A successfully resolved type
#[derive(Debug, Clone)]
pub struct ResolvedType {
    /// The full path to the type
    pub full_path: String,
    /// The simple type name
    pub type_name: String,
    /// How we got here (for debugging)
    pub resolution_chain: Vec<String>,
}

/// Information about an unresolved type
#[derive(Debug, Clone)]
pub struct UnresolvedInfo {
    /// The type name we tried to resolve
    pub type_name: String,
    /// Where we encountered this type
    pub referenced_from: Vec<String>,
    /// Why it couldn't be resolved
    pub reason: UnresolvedReason,
}

#[derive(Debug, Clone)]
pub enum UnresolvedReason {
    NotFound,
    AmbiguousMatch(Vec<String>),
    Cycle,
}

/// Context for type resolution
#[derive(Debug, Clone)]
pub struct ResolutionContext {
    /// The crate we're currently resolving from
    pub current_crate: Option<String>,
    /// Types we've already visited (cycle detection)
    pub visited: HashSet<String>,
    /// The chain of types that led us here
    pub chain: Vec<String>,
}

impl ResolutionContext {
    pub fn new() -> Self {
        Self {
            current_crate: None,
            visited: HashSet::new(),
            chain: Vec::new(),
        }
    }

    pub fn with_crate(crate_name: &str) -> Self {
        Self {
            current_crate: Some(crate_name.to_string()),
            visited: HashSet::new(),
            chain: Vec::new(),
        }
    }

    /// Create a child context for nested resolution
    pub fn child(&self, type_name: &str) -> Self {
        let mut visited = self.visited.clone();
        visited.insert(type_name.to_string());

        let mut chain = self.chain.clone();
        chain.push(type_name.to_string());

        Self {
            current_crate: self.current_crate.clone(),
            visited,
            chain,
        }
    }
}

// ============================================================================
// TYPE RESOLVER
// ============================================================================

/// Resolve all types reachable from a set of entry points
pub struct TypeResolver<'a> {
    index: &'a TypeIndex,
    result: ResolvedTypeSet,
}

impl<'a> TypeResolver<'a> {
    pub fn new(index: &'a TypeIndex) -> Self {
        Self {
            index,
            result: ResolvedTypeSet::default(),
        }
    }

    /// Resolve a single type and all its dependencies
    pub fn resolve_type(&mut self, type_name: &str, ctx: &ResolutionContext) {
        self.resolve_type_with_context(type_name, ctx, None);
    }

    /// Resolve a type with additional context for better warnings
    pub fn resolve_type_with_context(&mut self, type_name: &str, ctx: &ResolutionContext, parent_context: Option<&str>) {
        // Skip if already resolved
        if self.result.resolved.contains_key(type_name) {
            return;
        }

        // Note: We do NOT check is_primitive(type_name) here because complex types like
        // "*const Foo" would be incorrectly skipped. The primitive check happens AFTER
        // extraction, on individual extracted type names (see the loop below).

        // Cycle detection
        if ctx.visited.contains(type_name) {
            self.result.unresolved.entry(type_name.to_string())
                .or_insert_with(|| UnresolvedInfo {
                    type_name: type_name.to_string(),
                    referenced_from: ctx.chain.clone(),
                    reason: UnresolvedReason::Cycle,
                });
            return;
        }

        // Check for non-C-compatible types and warn
        // BUT: only warn if the type is NOT in the workspace index
        // Types in the index are assumed to be C-compatible wrappers
        // (e.g., api.json "String" maps to AzString which is C-compatible)
        let should_warn = self.should_warn_about_type(type_name);
        if let Some(warning) = should_warn {
            let context = parent_context
                .map(|p| format!("{} -> {}", p, ctx.chain.join(" -> ")))
                .unwrap_or_else(|| ctx.chain.join(" -> "));
            self.result.warnings.push(TypeWarning {
                type_expr: type_name.to_string(),
                context,
                message: warning,
            });
        }

        // Extract all types from complex type expressions (pointers, arrays, generics)
        // For function signatures (args AND return types), we trace through pointers 
        // because the type must be defined in the API even if accessed via pointer.
        // For struct fields, we DON'T trace through pointers (they become *const c_void)
        let is_function_signature = parent_context.map_or(false, |ctx| {
            // Match patterns like:
            // - "ClassName::fn_name arg 'param_name'" (function arg)
            // - "CallbackName callback arg[0]" (callback arg)  
            // - "ClassName::fn_name -> return" (function return type)
            // - "CallbackName callback -> return" (callback return type)
            ctx.contains(" arg ") || ctx.contains(" arg[") || ctx.contains("-> return")
        });
        
        let extracted = extract_all_types_from_expr_with_options(type_name, is_function_signature);
        
        for base_name in extracted {
            if base_name.is_empty() || TypeIndex::is_primitive(&base_name) {
                continue;
            }
            
            // Skip if already resolved
            if self.result.resolved.contains_key(&base_name) {
                continue;
            }

            // Try to resolve
            let preferred_crate = ctx.current_crate.as_deref();
            match self.index.resolve(&base_name, preferred_crate) {
                Some(typedef) => {
                    // Add to resolved set
                    self.result.resolved.insert(base_name.clone(), ResolvedType {
                        full_path: typedef.full_path.clone(),
                        type_name: base_name.clone(),
                        resolution_chain: ctx.chain.clone(),
                    });

                    // Recursively resolve referenced types
                    let child_ctx = ctx.child(&base_name);
                    self.resolve_referenced_types(typedef, &child_ctx);
                }
                None => {
                    // Check if there are multiple candidates
                    if let Some(candidates) = self.index.get_all_by_name(&base_name) {
                        if candidates.len() > 1 {
                            self.result.unresolved.entry(base_name.clone())
                                .or_insert_with(|| UnresolvedInfo {
                                    type_name: base_name,
                                    referenced_from: ctx.chain.clone(),
                                    reason: UnresolvedReason::AmbiguousMatch(
                                        candidates.iter().map(|c| c.full_path.clone()).collect()
                                    ),
                                });
                            continue;
                        }
                    }

                    self.result.unresolved.entry(base_name.clone())
                        .or_insert_with(|| UnresolvedInfo {
                            type_name: base_name,
                            referenced_from: ctx.chain.clone(),
                            reason: UnresolvedReason::NotFound,
                        });
                }
            }
        }
    }

    /// Resolve types referenced by a type definition
    fn resolve_referenced_types(&mut self, typedef: &TypeDefinition, ctx: &ResolutionContext) {
        let type_name = &typedef.type_name;
        
        match &typedef.kind {
            TypeDefKind::Struct { fields, generic_params, .. } => {
                for (field_name, field) in fields.iter() {
                    // Skip generic parameters (they're not real types)
                    if !generic_params.contains(&field.ty) {
                        // Provide context: "StructName.field_name"
                        let parent_context = format!("{}.{}", type_name, field_name);
                        self.resolve_type_with_context(&field.ty, ctx, Some(&parent_context));
                    }
                }
            }
            TypeDefKind::Enum { variants, generic_params, .. } => {
                for (variant_name, variant) in variants.iter() {
                    if let Some(ty) = &variant.ty {
                        // Skip generic parameters
                        if !generic_params.contains(ty) {
                            // Provide context: "EnumName::VariantName"
                            let parent_context = format!("{}::{}", type_name, variant_name);
                            self.resolve_type_with_context(ty, ctx, Some(&parent_context));
                        }
                    }
                }
            }
            TypeDefKind::TypeAlias { target, generic_base: _, generic_args } => {
                let parent_context = format!("{} (type alias)", type_name);
                self.resolve_type_with_context(target, ctx, Some(&parent_context));
                // Also resolve generic arguments
                for arg in generic_args {
                    self.resolve_type_with_context(arg, ctx, Some(&parent_context));
                }
            }
            TypeDefKind::CallbackTypedef { args, returns } => {
                for (i, arg) in args.iter().enumerate() {
                    let arg_name = arg.name.as_deref().unwrap_or("_");
                    let parent_context = format!("{} arg[{}]: {}", type_name, i, arg_name);
                    self.resolve_type_with_context(&arg.ty, ctx, Some(&parent_context));
                }
                if let Some(ret) = returns {
                    let parent_context = format!("{} -> return", type_name);
                    self.resolve_type_with_context(ret, ctx, Some(&parent_context));
                }
            }
            TypeDefKind::MacroGenerated { base_type, kind, source_macro } => {
                use super::type_index::MacroGeneratedKind;
                
                // For Vec types, resolve the base type
                let parent_context = format!("{} (macro-generated {:?})", type_name, kind);
                self.resolve_type_with_context(base_type, ctx, Some(&parent_context));
                
                // For Vec types, also resolve the associated Destructor and DestructorType
                // These are generated by impl_vec! but need to be explicitly resolved
                if matches!(kind, MacroGeneratedKind::Vec) {
                    // Vec type name is like "StringVec", derive destructor names
                    // The destructor enum is: {VecTypeName}Destructor (e.g., StringVecDestructor)
                    // The destructor type is: {VecTypeName}DestructorType (e.g., StringVecDestructorType)
                    let destructor_name = format!("{}Destructor", type_name);
                    let destructor_type_name = format!("{}DestructorType", type_name);
                    
                    let destructor_context = format!("{}.destructor", type_name);
                    self.resolve_type_with_context(&destructor_name, ctx, Some(&destructor_context));
                    
                    let destructor_type_context = format!("{}::External", destructor_name);
                    self.resolve_type_with_context(&destructor_type_name, ctx, Some(&destructor_type_context));
                }
            }
        }
    }

    /// Consume the resolver and return the result
    pub fn finish(self) -> ResolvedTypeSet {
        self.result
    }

    /// Try to resolve a type name with Az prefix first (preferred), then without.
    /// This handles api.json types like "String" which map to "AzString" in the workspace.
    /// Returns Some(TypeDefinition) if found, None otherwise.
    fn resolve_with_az_prefix(&self, type_name: &str) -> Option<&TypeDefinition> {
        // Try with Az prefix first (preferred)
        let az_prefixed = format!("Az{}", type_name);
        if let Some(typedef) = self.index.resolve(&az_prefixed, None) {
            return Some(typedef);
        }
        // Fallback: try without prefix
        self.index.resolve(type_name, None)
    }

    /// Check if we should warn about a type being non-C-compatible.
    /// 
    /// The key insight: types that exist in the workspace index are assumed to be
    /// C-compatible wrappers. For example, api.json uses "String" as a type name,
    /// but it maps to `AzString` in the workspace, which IS C-compatible.
    /// 
    /// We only warn about types that:
    /// 1. Are known std types (Vec, String, HashMap, etc.) AND
    /// 2. Are NOT found in the workspace index (meaning they're actual std types, not wrappers)
    fn should_warn_about_type(&self, type_name: &str) -> Option<String> {
        // Try to parse as syn::Type to extract the base type name
        let ty: Type = match syn::parse_str(type_name) {
            Ok(t) => t,
            Err(_) => return None,
        };
        
        self.check_type_c_compatibility_with_index(&ty)
    }

    /// Check C-compatibility, but consider the workspace index.
    /// If a type like "String" exists in the index, it's a custom wrapper and C-compatible.
    fn check_type_c_compatibility_with_index(&self, ty: &Type) -> Option<String> {
        match ty {
            Type::Path(type_path) => {
                if let Some(last) = type_path.path.segments.last() {
                    let name = last.ident.to_string();
                    
                    // Check for C-compatible "stop types"
                    match name.as_str() {
                        "Box" => return None,
                        "Option" => {
                            if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                                for arg in &args.args {
                                    if let syn::GenericArgument::Type(inner_ty) = arg {
                                        if is_box_type(inner_ty) {
                                            return None;
                                        }
                                        return self.check_type_c_compatibility_with_index(inner_ty);
                                    }
                                }
                            }
                            return None;
                        }
                        _ => {}
                    }
                    
                    // Try to resolve with Az prefix first (preferred), then without
                    // e.g., "String" -> try "AzString" first, then "String"
                    // If either is found in the index, it's a C-compatible wrapper
                    if self.resolve_with_az_prefix(&name).is_some() {
                        return None;
                    }
                    
                    // Check for potentially non-C-compatible std types
                    // If we get here, the type was NOT found in the index (neither with nor without Az prefix)
                    match name.as_str() {
                        "Vec" => {
                            return Some("Vec<T> is not C-compatible. Use a custom Vec type like FooVec instead.".to_string());
                        }
                        "String" => {
                            return Some("String is not C-compatible. Use AzString instead.".to_string());
                        }
                        "HashMap" | "BTreeMap" => {
                            return Some("HashMap/BTreeMap are not C-compatible. Use a custom map type.".to_string());
                        }
                        "Arc" | "Rc" => {
                            return Some("Arc<T>/Rc<T> are not C-compatible. Use a custom reference-counted type.".to_string());
                        }
                        "HashSet" | "BTreeSet" => {
                            return Some("HashSet/BTreeSet are not C-compatible. Use a custom set type.".to_string());
                        }
                        _ => {}
                    }
                    
                    // Recursively check generic arguments
                    if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                        for arg in &args.args {
                            if let syn::GenericArgument::Type(inner_ty) = arg {
                                if let Some(warning) = self.check_type_c_compatibility_with_index(inner_ty) {
                                    return Some(warning);
                                }
                            }
                        }
                    }
                }
                None
            }
            Type::Reference(_) => {
                Some("References (&T) are not C-compatible. Use pointers (*const T, *mut T) instead.".to_string())
            }
            Type::Ptr(type_ptr) => {
                // Pointers to unknown types become *const c_void (trace blocker)
                if is_known_c_compatible_type(&type_ptr.elem) {
                    self.check_type_c_compatibility_with_index(&type_ptr.elem)
                } else {
                    None // Trace blocker, no warning
                }
            }
            Type::Array(type_array) => {
                self.check_type_c_compatibility_with_index(&type_array.elem)
            }
            Type::Slice(_) => {
                Some("Slices ([T]) are not C-compatible. Use a pointer with length or a custom slice type.".to_string())
            }
            Type::Tuple(type_tuple) if !type_tuple.elems.is_empty() => {
                Some("Non-empty tuples are not C-compatible. Use a struct instead.".to_string())
            }
            Type::BareFn(_) => None,
            Type::Paren(type_paren) => {
                self.check_type_c_compatibility_with_index(&type_paren.elem)
            }
            _ => None
        }
    }
}

// ============================================================================
// FUNCTION PARAMETER EXTRACTION
// ============================================================================

/// Extracted function information
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    pub name: String,
    pub full_path: String,
    pub self_type: Option<String>,
    pub parameters: Vec<ParameterInfo>,
    pub return_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub ty: String,
}

/// Extract function information from workspace files
pub fn extract_functions_from_workspace(workspace_root: &Path) -> Vec<FunctionInfo> {
    let mut functions = Vec::new();

    // Scan the dll/src directory for pub functions
    let dll_src = workspace_root.join("dll/src");
    if dll_src.exists() {
        extract_functions_from_dir(&dll_src, "azul_dll", &mut functions);
    }

    functions
}

fn extract_functions_from_dir(dir: &Path, crate_name: &str, functions: &mut Vec<FunctionInfo>) {
    use std::fs;

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            extract_functions_from_dir(&path, crate_name, functions);
        } else if path.extension().map_or(false, |e| e == "rs") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(syntax) = syn::parse_file(&content) {
                    extract_functions_from_items(&syntax.items, crate_name, "", functions);
                }
            }
        }
    }
}

fn extract_functions_from_items(
    items: &[syn::Item],
    crate_name: &str,
    module_path: &str,
    functions: &mut Vec<FunctionInfo>,
) {
    use quote::ToTokens;

    for item in items {
        match item {
            syn::Item::Fn(f) => {
                // Only public functions
                if matches!(f.vis, syn::Visibility::Public(_)) {
                    let mut params = Vec::new();
                    let mut self_type = None;

                    for arg in &f.sig.inputs {
                        match arg {
                            syn::FnArg::Receiver(r) => {
                                // Self parameter - we need to find the impl block's type
                                // For now, mark as having self
                                self_type = Some("Self".to_string());
                            }
                            syn::FnArg::Typed(pat_type) => {
                                let name = match &*pat_type.pat {
                                    syn::Pat::Ident(i) => i.ident.to_string(),
                                    _ => "_".to_string(),
                                };
                                let ty = clean_type(&pat_type.ty.to_token_stream().to_string());
                                params.push(ParameterInfo { name, ty });
                            }
                        }
                    }

                    let return_type = match &f.sig.output {
                        syn::ReturnType::Default => None,
                        syn::ReturnType::Type(_, ty) => {
                            let ty_str = clean_type(&ty.to_token_stream().to_string());
                            if ty_str.is_empty() || ty_str == "()" {
                                None
                            } else {
                                Some(ty_str)
                            }
                        }
                    };

                    let name = f.sig.ident.to_string();
                    let full_path = if module_path.is_empty() {
                        format!("{}::{}", crate_name, name)
                    } else {
                        format!("{}::{}::{}", crate_name, module_path, name)
                    };

                    functions.push(FunctionInfo {
                        name,
                        full_path,
                        self_type,
                        parameters: params,
                        return_type,
                    });
                }
            }
            syn::Item::Impl(imp) => {
                // Extract the impl target type
                let impl_type = clean_type(&imp.self_ty.to_token_stream().to_string());

                for impl_item in &imp.items {
                    if let syn::ImplItem::Fn(f) = impl_item {
                        if matches!(f.vis, syn::Visibility::Public(_)) {
                            let mut params = Vec::new();
                            let mut self_type = None;

                            for arg in &f.sig.inputs {
                                match arg {
                                    syn::FnArg::Receiver(_) => {
                                        self_type = Some(impl_type.clone());
                                    }
                                    syn::FnArg::Typed(pat_type) => {
                                        let name = match &*pat_type.pat {
                                            syn::Pat::Ident(i) => i.ident.to_string(),
                                            _ => "_".to_string(),
                                        };
                                        let ty = clean_type(&pat_type.ty.to_token_stream().to_string());
                                        params.push(ParameterInfo { name, ty });
                                    }
                                }
                            }

                            let return_type = match &f.sig.output {
                                syn::ReturnType::Default => None,
                                syn::ReturnType::Type(_, ty) => {
                                    let ty_str = clean_type(&ty.to_token_stream().to_string());
                                    if ty_str.is_empty() || ty_str == "()" {
                                        None
                                    } else {
                                        Some(ty_str)
                                    }
                                }
                            };

                            let name = f.sig.ident.to_string();
                            let full_path = if module_path.is_empty() {
                                format!("{}::{}::{}", crate_name, impl_type, name)
                            } else {
                                format!("{}::{}::{}::{}", crate_name, module_path, impl_type, name)
                            };

                            functions.push(FunctionInfo {
                                name,
                                full_path,
                                self_type,
                                parameters: params,
                                return_type,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

// ============================================================================
// HELPERS - AST-BASED TYPE EXTRACTION
// ============================================================================

/// Check if a type expression is C-compatible by parsing it and analyzing the AST
/// Returns Some(warning_message) if not compatible
fn check_c_compatibility(type_str: &str) -> Option<String> {
    // Try to parse as syn::Type
    let ty: Type = match syn::parse_str(type_str) {
        Ok(t) => t,
        Err(_) => return None, // Can't parse, skip warning
    };
    
    check_type_c_compatibility(&ty)
}

/// Check C-compatibility on a parsed syn::Type
fn check_type_c_compatibility(ty: &Type) -> Option<String> {
    match ty {
        Type::Path(type_path) => {
            let path_str = type_path.path.segments.iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            
            // Get the last segment (the actual type name)
            if let Some(last) = type_path.path.segments.last() {
                let name = last.ident.to_string();
                
                // Check for C-compatible "stop types" - these map to *const c_void
                // and we don't need to check their inner types
                match name.as_str() {
                    "Box" => {
                        // Box<T> is C-compatible - it becomes *const c_void
                        // Don't recurse into the inner type
                        return None;
                    }
                    "Option" => {
                        // Option<Box<T>> is C-compatible - check if inner is Box
                        if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                            for arg in &args.args {
                                if let syn::GenericArgument::Type(inner_ty) = arg {
                                    if is_box_type(inner_ty) {
                                        // Option<Box<T>> is C-compatible
                                        return None;
                                    }
                                    // Other Option<T> - recurse to check inner type
                                    return check_type_c_compatibility(inner_ty);
                                }
                            }
                        }
                        return None;
                    }
                    _ => {}
                }
                
                // Check for non-C-compatible std types
                match name.as_str() {
                    "Vec" => return Some("Vec<T> is not C-compatible. Use a custom Vec type like FooVec instead.".to_string()),
                    "String" => return Some("String is not C-compatible. Use AzString instead.".to_string()),
                    "HashMap" | "BTreeMap" => return Some("HashMap/BTreeMap are not C-compatible. Use a custom map type.".to_string()),
                    "Arc" | "Rc" => return Some("Arc<T>/Rc<T> are not C-compatible. Use a custom reference-counted type.".to_string()),
                    "HashSet" | "BTreeSet" => return Some("HashSet/BTreeSet are not C-compatible. Use a custom set type.".to_string()),
                    _ => {}
                }
                
                // Recursively check generic arguments
                if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                    for arg in &args.args {
                        if let syn::GenericArgument::Type(inner_ty) = arg {
                            if let Some(warning) = check_type_c_compatibility(inner_ty) {
                                return Some(warning);
                            }
                        }
                    }
                }
            }
            None
        }
        Type::Reference(_) => {
            Some("References (&T) are not C-compatible. Use pointers (*const T, *mut T) instead.".to_string())
        }
        Type::Ptr(type_ptr) => {
            // Pointers to non-C-compatible types become *const c_void / *mut c_void
            // This is a "trace blocker" - we don't warn about the inner type
            // because it won't be exposed in the C API
            // Use is_known_c_compatible_type to be conservative: unknown types are treated as opaque
            if is_known_c_compatible_type(&type_ptr.elem) {
                // Inner type is known C-compatible (primitive, etc.), check it recursively
                check_type_c_compatibility(&type_ptr.elem)
            } else {
                // Inner type is not known C-compatible - pointer becomes *const c_void
                // This is fine, no warning needed (it's a trace blocker)
                None
            }
        }
        Type::Array(type_array) => {
            // Fixed arrays are C-compatible, check element type
            check_type_c_compatibility(&type_array.elem)
        }
        Type::Slice(_) => {
            Some("Slices ([T]) are not C-compatible. Use a pointer with length or a custom slice type.".to_string())
        }
        Type::Tuple(type_tuple) if !type_tuple.elems.is_empty() => {
            Some("Non-empty tuples are not C-compatible. Use a struct instead.".to_string())
        }
        Type::BareFn(_) => {
            // Bare function pointers are C-compatible (extern "C" fn)
            None
        }
        Type::Paren(type_paren) => {
            check_type_c_compatibility(&type_paren.elem)
        }
        _ => None
    }
}

/// Check if a type is C-compatible (returns true if compatible, false if not)
/// This is the inverse of check_type_c_compatibility - returns true if no warning would be generated
fn is_type_c_compatible(ty: &Type) -> bool {
    check_type_c_compatibility(ty).is_none()
}

/// Check if a type is KNOWN to be C-compatible (primitive, pointer, or known C-compatible struct).
/// This is more conservative than check_type_c_compatibility - returns false for unknown types.
/// Used for pointer trace blocking: if inner type is not known to be C-compatible,
/// the pointer becomes *const c_void and we don't extract the inner type.
fn is_known_c_compatible_type(ty: &Type) -> bool {
    match ty {
        Type::Path(type_path) => {
            if let Some(last) = type_path.path.segments.last() {
                let name = last.ident.to_string();
                
                // Primitives are known C-compatible
                match name.as_str() {
                    "bool" | "char" | 
                    "i8" | "i16" | "i32" | "i64" | "i128" | "isize" |
                    "u8" | "u16" | "u32" | "u64" | "u128" | "usize" |
                    "f32" | "f64" |
                    "c_void" | "c_char" | "c_int" | "c_uint" | "c_long" | "c_ulong" => true,
                    
                    // Box<T> and Option<Box<T>> map to *const c_void - known C-compatible
                    "Box" => true,
                    "Option" => {
                        // Option<Box<T>> is known C-compatible
                        if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                            for arg in &args.args {
                                if let syn::GenericArgument::Type(inner_ty) = arg {
                                    return is_box_type(inner_ty);
                                }
                            }
                        }
                        false
                    }
                    
                    // Unknown types - NOT known to be C-compatible
                    // They might be C-compatible (custom structs with #[repr(C)])
                    // but we can't know without parsing their definitions
                    _ => false,
                }
            } else {
                false
            }
        }
        Type::Ptr(_) => {
            // Pointers themselves are C-compatible
            true
        }
        Type::Array(type_array) => {
            // Arrays are C-compatible if element is
            is_known_c_compatible_type(&type_array.elem)
        }
        Type::BareFn(_) => {
            // Function pointers are C-compatible
            true
        }
        Type::Tuple(type_tuple) => {
            // Empty tuple (unit) is C-compatible
            type_tuple.elems.is_empty()
        }
        _ => false,
    }
}

/// Check if a type is Box<T>
fn is_box_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(last) = type_path.path.segments.last() {
            return last.ident == "Box";
        }
    }
    false
}

/// Extract ALL type names from a type string by parsing it as syn::Type
/// This handles pointers, arrays, and generics recursively.
/// 
/// By default (for struct fields), we DON'T trace through pointers - they become *const c_void.
fn extract_all_types_from_expr(type_str: &str) -> Vec<String> {
    extract_all_types_from_expr_with_options(type_str, false)
}

/// Extract ALL type names from a type string, with option to trace through pointers.
/// 
/// If `trace_through_pointers` is true, we extract types behind *const T and *mut T.
/// Use true for function arguments, false for struct fields.
fn extract_all_types_from_expr_with_options(type_str: &str, trace_through_pointers: bool) -> Vec<String> {
    // Try to parse as syn::Type
    let ty: Type = match syn::parse_str(type_str) {
        Ok(t) => t,
        Err(_) => {
            // Fallback: try to extract simple name from string
            let simple = extract_simple_name_from_str(type_str);
            if simple.is_empty() {
                return Vec::new();
            }
            return vec![simple];
        }
    };
    
    let mut types = Vec::new();
    extract_types_from_syn_type(&ty, &mut types, trace_through_pointers);
    types
}

/// Extract type names from a parsed syn::Type
/// 
/// If `trace_through_pointers` is true, we extract types behind *const T and *mut T.
/// This should be true for function arguments (the type T must be defined in the API).
/// This should be false for struct fields (pointer fields become *const c_void if T is opaque).
fn extract_types_from_syn_type(ty: &Type, types: &mut Vec<String>, trace_through_pointers: bool) {
    match ty {
        Type::Path(type_path) => {
            extract_types_from_type_path(type_path, types, trace_through_pointers);
        }
        Type::Ptr(type_ptr) => {
            if trace_through_pointers {
                // For function arguments: extract the inner type
                // The type behind the pointer needs to be defined in the API
                extract_types_from_syn_type(&type_ptr.elem, types, trace_through_pointers);
            }
            // If not tracing through pointers (struct fields), stop here - pointer becomes *const c_void
        }
        Type::Reference(type_ref) => {
            // Reference: extract the inner type
            extract_types_from_syn_type(&type_ref.elem, types, trace_through_pointers);
        }
        Type::Array(type_array) => {
            // Array [T; N]: extract the element type
            extract_types_from_syn_type(&type_array.elem, types, trace_through_pointers);
        }
        Type::Slice(type_slice) => {
            // Slice [T]: extract the element type
            extract_types_from_syn_type(&type_slice.elem, types, trace_through_pointers);
        }
        Type::Tuple(type_tuple) => {
            // Tuple (T1, T2, ...): extract all element types
            for elem in &type_tuple.elems {
                extract_types_from_syn_type(elem, types, trace_through_pointers);
            }
        }
        Type::BareFn(type_fn) => {
            // Function pointer: extract argument and return types (always trace through pointers for fn args)
            for arg in &type_fn.inputs {
                extract_types_from_syn_type(&arg.ty, types, true);
            }
            if let syn::ReturnType::Type(_, ret_ty) = &type_fn.output {
                extract_types_from_syn_type(ret_ty, types, true);
            }
        }
        Type::Paren(type_paren) => {
            extract_types_from_syn_type(&type_paren.elem, types, trace_through_pointers);
        }
        Type::Group(type_group) => {
            extract_types_from_syn_type(&type_group.elem, types, trace_through_pointers);
        }
        Type::TraitObject(_) | Type::ImplTrait(_) => {
            // Skip trait objects and impl Trait - not relevant for C API
        }
        _ => {}
    }
}

/// Extract types from a TypePath (e.g., `std::vec::Vec<T>` or `MyStruct`)
fn extract_types_from_type_path(type_path: &TypePath, types: &mut Vec<String>, trace_through_pointers: bool) {
    // Get the last segment of the path (the actual type name)
    if let Some(last) = type_path.path.segments.last() {
        let type_name = last.ident.to_string();
        
        // Box<T> and Option<Box<T>> are "stop types" - they map to *const c_void
        // We don't need to extract the inner type T
        if type_name == "Box" {
            // Box<T> becomes *const c_void - don't extract inner type
            return;
        }
        
        if type_name == "Option" {
            // Check if this is Option<Box<T>>
            if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
                for arg in &args.args {
                    if let syn::GenericArgument::Type(inner_ty) = arg {
                        if is_box_type(inner_ty) {
                            // Option<Box<T>> becomes *const c_void - don't extract inner type
                            return;
                        }
                        // Option<T> where T is not Box - extract T
                        extract_types_from_syn_type(inner_ty, types, trace_through_pointers);
                        return;
                    }
                }
            }
            return;
        }
        
        // Check if it's a wrapper type that we should "look through"
        let is_wrapper = matches!(type_name.as_str(), "Vec" | "Arc" | "Rc" | "Result");
        
        // If it's not a wrapper and not a primitive, add it
        if !is_wrapper && !TypeIndex::is_primitive(&type_name) {
            types.push(type_name.clone());
        }
        
        // Process generic arguments recursively
        if let syn::PathArguments::AngleBracketed(args) = &last.arguments {
            for arg in &args.args {
                if let syn::GenericArgument::Type(inner_ty) = arg {
                    extract_types_from_syn_type(inner_ty, types, trace_through_pointers);
                }
            }
        }
    }
}

/// Fallback: extract simple type name from string when syn parsing fails
fn extract_simple_name_from_str(s: &str) -> String {
    let s = s.trim();
    
    // Handle qualified paths
    if s.contains("::") {
        return s.rsplit("::").next().unwrap_or(s).to_string();
    }
    
    // Handle generics - just get the outer type name
    if let Some(idx) = s.find('<') {
        return s[..idx].trim().to_string();
    }
    
    s.to_string()
}

/// Check if a type is a wrapper that we want to "look through"
/// Note: Box and Option<Box> are "stop types" and handled separately
fn is_wrapper_type(name: &str) -> bool {
    matches!(name, "Option" | "Vec" | "Arc" | "Rc" | "Result")
}

/// Extract the base type name from a complex type string (legacy, for compatibility)
/// e.g., "Vec<FontCache>" -> "FontCache", "*const Foo" -> "Foo"
fn extract_base_type_name(type_str: &str) -> String {
    // Try to use AST-based extraction first
    let types = extract_all_types_from_expr(type_str);
    if let Some(first) = types.into_iter().next() {
        return first;
    }
    
    // Fallback to string-based extraction
    extract_simple_name_from_str(type_str)
}

/// Clean up a type string
fn clean_type(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ============================================================================
// RESOLVE ALL FROM WORKSPACE
// ============================================================================

/// Resolve all types referenced by functions in the workspace
pub fn resolve_all_workspace_types(
    index: &TypeIndex,
    workspace_root: &Path,
    verbose: bool,
) -> ResolvedTypeSet {
    let functions = extract_functions_from_workspace(workspace_root);

    if verbose {
        eprintln!("[TypeResolver] Found {} public functions", functions.len());
    }

    let mut resolver = TypeResolver::new(index);
    let ctx = ResolutionContext::new();

    for func in &functions {
        // Resolve self type
        if let Some(self_ty) = &func.self_type {
            resolver.resolve_type(self_ty, &ctx);
        }

        // Resolve parameter types
        for param in &func.parameters {
            resolver.resolve_type(&param.ty, &ctx);
        }

        // Resolve return type
        if let Some(ret_ty) = &func.return_type {
            resolver.resolve_type(ret_ty, &ctx);
        }
    }

    let result = resolver.finish();

    if verbose {
        eprintln!(
            "[TypeResolver] Resolved {} types, {} unresolved",
            result.resolved.len(),
            result.unresolved.len()
        );
    }

    result
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_all_types_from_expr() {
        // Simple types (not behind a pointer)
        assert_eq!(extract_all_types_from_expr("FontCache"), vec!["FontCache"]);
        
        // Pointers to unknown types are "trace blockers" - don't extract inner type
        // Because we can't know if the inner type is C-compatible without parsing its definition
        // So *const FontCache, *const FcFontCache, etc. all become *const c_void
        assert_eq!(extract_all_types_from_expr("*const FontCache"), Vec::<String>::new());
        assert_eq!(extract_all_types_from_expr("*mut FontCache"), Vec::<String>::new());
        
        // Pointers to known non-C-compatible types are also "trace blockers"
        assert_eq!(extract_all_types_from_expr("*const String"), Vec::<String>::new());
        assert_eq!(extract_all_types_from_expr("*mut Vec<i32>"), Vec::<String>::new());
        assert_eq!(extract_all_types_from_expr("*const FcFontCache"), Vec::<String>::new());
        
        // Pointers to primitives ARE extracted (primitives are known C-compatible)
        assert_eq!(extract_all_types_from_expr("*const u8"), Vec::<String>::new()); // u8 is primitive, not added
        assert_eq!(extract_all_types_from_expr("*mut i32"), Vec::<String>::new()); // i32 is primitive
        
        // References (will generate a warning, but we still extract the type)
        assert_eq!(extract_all_types_from_expr("&FontCache"), vec!["FontCache"]);
        assert_eq!(extract_all_types_from_expr("&mut FontCache"), vec!["FontCache"]);
        
        // Wrappers (should extract inner type, not wrapper)
        assert_eq!(extract_all_types_from_expr("Option<FontCache>"), vec!["FontCache"]);
        assert_eq!(extract_all_types_from_expr("Vec<FontCache>"), vec!["FontCache"]);
        
        // Box<T> is a "stop type" - it maps to *const c_void, so we don't extract inner type
        assert_eq!(extract_all_types_from_expr("Box<FontCache>"), Vec::<String>::new());
        
        // Option<Box<T>> is also a "stop type"
        assert_eq!(extract_all_types_from_expr("Option<Box<FontCache>>"), Vec::<String>::new());
        
        // Generic types (should include outer type)
        let types = extract_all_types_from_expr("CssPropertyValue<LayoutWidth>");
        assert!(types.contains(&"CssPropertyValue".to_string()));
        assert!(types.contains(&"LayoutWidth".to_string()));
        
        // Arrays
        assert_eq!(extract_all_types_from_expr("[u8; 4]"), Vec::<String>::new()); // u8 is primitive
        assert_eq!(extract_all_types_from_expr("[FontCache; 4]"), vec!["FontCache"]);
    }

    #[test]
    fn test_extract_all_types_with_pointer_tracing() {
        // Without pointer tracing (default, for struct fields): pointers are trace blockers
        assert_eq!(extract_all_types_from_expr("*const FontCache"), Vec::<String>::new());
        assert_eq!(extract_all_types_from_expr("*mut TessellatedGPUSvgNode"), Vec::<String>::new());
        
        // With pointer tracing (for function arguments): extract types behind pointers
        assert_eq!(extract_all_types_from_expr_with_options("*const FontCache", true), vec!["FontCache"]);
        assert_eq!(extract_all_types_from_expr_with_options("*mut TessellatedGPUSvgNode", true), vec!["TessellatedGPUSvgNode"]);
        assert_eq!(extract_all_types_from_expr_with_options("*const *const Foo", true), vec!["Foo"]);
        
        // With pointer tracing but primitives are still not added
        assert_eq!(extract_all_types_from_expr_with_options("*const u8", true), Vec::<String>::new());
        assert_eq!(extract_all_types_from_expr_with_options("*mut i32", true), Vec::<String>::new());
    }

    #[test]
    fn test_check_c_compatibility() {
        // C-compatible types
        assert!(check_c_compatibility("FontCache").is_none());
        assert!(check_c_compatibility("*const FontCache").is_none());
        assert!(check_c_compatibility("*mut u8").is_none());
        assert!(check_c_compatibility("[u8; 4]").is_none());
        
        // Box<T> and Option<Box<T>> are C-compatible (map to *const c_void)
        assert!(check_c_compatibility("Box<FontCache>").is_none());
        assert!(check_c_compatibility("Option<Box<FontCache>>").is_none());
        
        // Pointers to non-C-compatible types are "trace blockers" - no warning
        // They become *const c_void in the C API
        assert!(check_c_compatibility("*const String").is_none());
        assert!(check_c_compatibility("*mut Vec<i32>").is_none());
        assert!(check_c_compatibility("*const HashMap<String, i32>").is_none());
        
        // Non-C-compatible types (when NOT behind a pointer)
        assert!(check_c_compatibility("Vec<FontCache>").is_some());
        assert!(check_c_compatibility("String").is_some());
        assert!(check_c_compatibility("&FontCache").is_some());
        assert!(check_c_compatibility("HashMap<String, i32>").is_some());
    }

    #[test]
    fn test_resolution_context() {
        let ctx = ResolutionContext::new();
        assert!(ctx.visited.is_empty());
        assert!(ctx.chain.is_empty());

        let child = ctx.child("Foo");
        assert!(child.visited.contains("Foo"));
        assert_eq!(child.chain, vec!["Foo"]);

        let grandchild = child.child("Bar");
        assert!(grandchild.visited.contains("Foo"));
        assert!(grandchild.visited.contains("Bar"));
        assert_eq!(grandchild.chain, vec!["Foo", "Bar"]);
    }
}
