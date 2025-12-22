//! Function Diff Generation
//!
//! This module compares the methods from source code with functions in api.json
//! and provides tools to list, add, and remove functions.

use std::collections::{BTreeMap, BTreeSet};

use indexmap::IndexMap;

use super::type_index::{MethodDef, RefKind, SelfKind, TypeDefKind, TypeDefinition, TypeIndex};
use crate::{
    api::{ApiData, ClassData, FunctionData, ModuleData, ReturnTypeData, VersionData},
    patch::{ApiPatch, ClassPatch, ModulePatch, VersionPatch},
};

// data structures
/// Represents a function from either source code or api.json
#[derive(Debug, Clone)]
pub struct FunctionInfo {
    /// Function name
    pub name: String,
    /// Self kind (None for static, Some for instance methods)
    pub self_kind: Option<SelfKind>,
    /// Arguments (name, type, ref_kind)
    pub args: Vec<(String, String, String)>,
    /// Return type (None for void)
    pub return_type: Option<String>,
    /// Return ref kind
    pub return_ref_kind: String,
    /// Is this a constructor
    pub is_constructor: bool,
    /// Documentation
    pub doc: Vec<String>,
    /// Is public
    pub is_public: bool,
}

/// Result of comparing source methods with api.json functions
#[derive(Debug)]
pub struct FunctionComparison {
    /// Functions in source but not in api.json
    pub missing_in_api: Vec<FunctionInfo>,
    /// Functions in api.json but not in source
    pub extra_in_api: Vec<String>,
    /// Functions in both but with differences
    pub differences: Vec<FunctionDiff>,
    /// Functions that match exactly
    pub matching: Vec<String>,
}

/// Difference between source and api.json for a function
#[derive(Debug)]
pub struct FunctionDiff {
    /// Function name
    pub name: String,
    /// Description of the differences
    pub differences: Vec<String>,
}

// conversion functions
/// Convert a MethodDef to FunctionInfo
pub fn method_to_function_info(method: &MethodDef) -> FunctionInfo {
    let args: Vec<(String, String, String)> = method
        .args
        .iter()
        .map(|a| {
            (
                a.name.clone(),
                a.ty.clone(),
                ref_kind_to_string(&a.ref_kind),
            )
        })
        .collect();

    FunctionInfo {
        name: method.name.clone(),
        self_kind: method.self_kind.clone(),
        args,
        return_type: method.return_type.clone(),
        return_ref_kind: ref_kind_to_string(&method.return_ref_kind),
        is_constructor: method.is_constructor,
        doc: method.doc.clone(),
        is_public: method.is_public,
    }
}

/// Convert RefKind to string representation for api.json
pub fn ref_kind_to_string(ref_kind: &RefKind) -> String {
    match ref_kind {
        RefKind::Value => "value".to_string(),
        RefKind::Ref => "ref".to_string(),
        RefKind::RefMut => "refmut".to_string(),
        RefKind::ConstPtr => "const_ptr".to_string(),
        RefKind::MutPtr => "mut_ptr".to_string(),
        RefKind::Boxed => "boxed".to_string(),
        RefKind::OptionBoxed => "option_boxed".to_string(),
    }
}

/// Convert SelfKind to string for fn_body
pub fn self_kind_to_fn_ptr(self_kind: &Option<SelfKind>) -> String {
    match self_kind {
        None => "".to_string(), // Constructor or static
        Some(SelfKind::Value) => "*mut crate::AzType".to_string(),
        Some(SelfKind::Ref) => "*const crate::AzType".to_string(),
        Some(SelfKind::RefMut) => "*mut crate::AzType".to_string(),
    }
}

// comparison functions
/// Compare methods from source code with functions in api.json for a type
pub fn compare_type_functions(
    type_def: &TypeDefinition,
    api_data: &ApiData,
    version: &str,
) -> Option<FunctionComparison> {
    // Find the type in api.json for this version
    let version_data = api_data.get_version(version)?;
    let api_class = find_api_class(&type_def.type_name, version_data)?;

    // Get source methods (only public ones)
    let source_methods: BTreeMap<String, &MethodDef> = type_def
        .methods
        .iter()
        .filter(|m| m.is_public)
        .map(|m| (m.name.clone(), m))
        .collect();

    // Get api.json functions (both constructors and regular functions)
    let mut api_functions: BTreeSet<String> = BTreeSet::new();
    if let Some(ref fns) = api_class.functions {
        api_functions.extend(fns.keys().cloned());
    }
    if let Some(ref ctors) = api_class.constructors {
        api_functions.extend(ctors.keys().cloned());
    }

    let source_names: BTreeSet<String> = source_methods.keys().cloned().collect();

    // Missing in API (in source but not in api.json)
    let missing_in_api: Vec<FunctionInfo> = source_names
        .difference(&api_functions)
        .filter_map(|name| source_methods.get(name))
        .map(|m| method_to_function_info(m))
        .collect();

    // Extra in API (in api.json but not in source)
    let extra_in_api: Vec<String> = api_functions.difference(&source_names).cloned().collect();

    // Matching functions
    let matching: Vec<String> = source_names.intersection(&api_functions).cloned().collect();

    // Check for differences in matching functions
    let differences = find_function_differences(&matching, &source_methods, api_class);

    Some(FunctionComparison {
        missing_in_api,
        extra_in_api,
        differences,
        matching,
    })
}

/// Find differences between source methods and api.json functions
fn find_function_differences(
    matching: &[String],
    source_methods: &BTreeMap<String, &MethodDef>,
    api_class: &ClassData,
) -> Vec<FunctionDiff> {
    let mut differences = Vec::new();

    for name in matching {
        let Some(source_method) = source_methods.get(name) else {
            continue;
        };

        // Find the function in api.json (check both functions and constructors)
        let api_fn = api_class
            .functions
            .as_ref()
            .and_then(|fns| fns.get(name))
            .or_else(|| {
                api_class
                    .constructors
                    .as_ref()
                    .and_then(|ctors| ctors.get(name))
            });

        let Some(api_fn) = api_fn else { continue };

        let mut diffs = Vec::new();

        // Check self parameter
        let api_has_self = api_fn
            .fn_args
            .iter()
            .any(|arg_map| arg_map.contains_key("self"));
        let source_has_self = source_method.self_kind.is_some();

        if source_has_self && !api_has_self {
            let self_kind = match &source_method.self_kind {
                Some(SelfKind::Value) => "value",
                Some(SelfKind::Ref) => "ref",
                Some(SelfKind::RefMut) => "refmut",
                None => "none",
            };
            diffs.push(format!(
                "missing self parameter (should be '{}')",
                self_kind
            ));
        } else if !source_has_self && api_has_self {
            diffs.push("has self parameter but source method is static".to_string());
        } else if source_has_self && api_has_self {
            // Check self kind matches
            let api_self_kind = api_fn
                .fn_args
                .iter()
                .find_map(|arg_map| arg_map.get("self"))
                .map(|s| s.as_str())
                .unwrap_or("value");

            let source_self_kind = match &source_method.self_kind {
                Some(SelfKind::Value) => "value",
                Some(SelfKind::Ref) => "ref",
                Some(SelfKind::RefMut) => "refmut",
                None => "value",
            };

            if api_self_kind != source_self_kind {
                diffs.push(format!(
                    "self kind mismatch: api.json has '{}', source has '{}'",
                    api_self_kind, source_self_kind
                ));
            }
        }

        // Check argument count (excluding self)
        let api_arg_count = api_fn
            .fn_args
            .iter()
            .filter(|arg_map| !arg_map.contains_key("self"))
            .count();
        let source_arg_count = source_method.args.len();

        if api_arg_count != source_arg_count {
            diffs.push(format!(
                "argument count mismatch: api.json has {}, source has {}",
                api_arg_count, source_arg_count
            ));
        }

        if !diffs.is_empty() {
            differences.push(FunctionDiff {
                name: name.clone(),
                differences: diffs,
            });
        }
    }

    differences
}

/// Find a class/type in api.json for a specific version
fn find_api_class<'a>(type_name: &str, version_data: &'a VersionData) -> Option<&'a ClassData> {
    for (_, module) in &version_data.api {
        if let Some(class) = module.classes.get(type_name) {
            return Some(class);
        }
    }
    None
}

/// Find which module a type is in within api.json
pub fn find_type_module<'a>(type_name: &str, version_data: &'a VersionData) -> Option<&'a str> {
    for (module_name, module) in &version_data.api {
        if module.classes.get(type_name).is_some() {
            return Some(module_name);
        }
    }
    None
}

// fn_body generation
/// Generate fn_body for a method based on its signature
/// This creates the FFI wrapper function body that bridges Rust to C
pub fn generate_fn_body(method: &MethodDef, full_path: &str) -> String {
    // Extract type_name from full_path for fn_type detection
    let type_name = full_path.rsplit("::").next().unwrap_or(full_path);

    // Determine function type based on self_kind and return type
    let fn_type = determine_fn_type(method, type_name);

    // Generate code based on function type
    match fn_type.as_str() {
        "constructor" => generate_constructor_body(method, full_path),
        "getter" => generate_getter_body(method),
        "setter" => generate_setter_body(method),
        "method" => generate_method_body(method, full_path),
        "static" => generate_static_body(method, full_path),
        "destructor" => generate_destructor_body(method),
        _ => generate_method_body(method, full_path),
    }
}

/// Determine the type of function based on method signature
fn determine_fn_type(method: &MethodDef, type_name: &str) -> String {
    // Constructor: no self, returns Self or type_name
    if method.is_constructor {
        return "constructor".to_string();
    }

    // Destructor: typically named "drop" or similar
    if method.name == "drop" || method.name.starts_with("destroy") {
        return "destructor".to_string();
    }

    // Getter: &self, no args, returns something
    if method.self_kind == Some(SelfKind::Ref)
        && method.args.is_empty()
        && method.return_type.is_some()
        && method.name.starts_with("get_")
    {
        return "getter".to_string();
    }

    // Setter: &mut self, one arg, returns () or Self
    if method.self_kind == Some(SelfKind::RefMut)
        && method.args.len() == 1
        && (method.return_type.is_none()
            || method.return_type.as_ref().map(|s| s.as_str()) == Some("Self"))
        && method.name.starts_with("set_")
    {
        return "setter".to_string();
    }

    // Static: no self
    if method.self_kind.is_none() {
        return "static".to_string();
    }

    // Regular method
    "method".to_string()
}

fn generate_constructor_body(method: &MethodDef, full_path: &str) -> String {
    // fn_body should just be the call expression - the wrapper code is auto-generated
    let args_str = method
        .args
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    format!("{}::{}({})", full_path, method.name, args_str)
}

fn generate_getter_body(method: &MethodDef) -> String {
    // fn_body should just be the call expression
    format!("object.{}()", method.name)
}

fn generate_setter_body(method: &MethodDef) -> String {
    // fn_body should just be the call expression
    let arg_name = method
        .args
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    format!("object.{}({})", method.name, arg_name)
}

fn generate_method_body(method: &MethodDef, full_path: &str) -> String {
    // fn_body should just be the call expression
    let args_str = method
        .args
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    if method.self_kind.is_some() {
        if args_str.is_empty() {
            format!("object.{}()", method.name)
        } else {
            format!("object.{}({})", method.name, args_str)
        }
    } else {
        format!("{}::{}({})", full_path, method.name, args_str)
    }
}

fn generate_static_body(method: &MethodDef, full_path: &str) -> String {
    // Static function: no self pointer
    let args_str = method
        .args
        .iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", ");

    format!("{}::{}({})", full_path, method.name, args_str)
}

fn generate_destructor_body(method: &MethodDef) -> String {
    // fn_body should just be the drop expression
    "core::mem::drop(object)".to_string()
}

// return type conversion for ffi
/// Convert Rust return types to api.json format:
/// - `Result<X, Y>` -> `ResultXY` (with `.into()` added to fn_body)
/// - `Option<X>` -> `OptionX` (with `.into()` added to fn_body)
/// - `Self` -> class_name
///
/// Returns (converted_type, needs_into) where needs_into indicates if `.into()` should be appended
fn convert_return_type_for_ffi(return_type: &str, class_name: &str) -> (String, bool) {
    let trimmed = return_type.trim();

    // Handle Result<X, Y> -> ResultXY
    if trimmed.starts_with("Result<") && trimmed.ends_with('>') {
        let inner = &trimmed[7..trimmed.len() - 1]; // Remove "Result<" and ">"
                                                    // Split by comma, handling nested generics
        if let Some((ok_type, err_type)) = split_generic_args(inner) {
            let ok_clean = normalize_type_name(&ok_type, class_name);
            let err_clean = normalize_type_name(&err_type, class_name);
            return (format!("Result{}{}", ok_clean, err_clean), true);
        }
    }

    // Handle Option<X> -> OptionX
    if trimmed.starts_with("Option<") && trimmed.ends_with('>') {
        let inner = &trimmed[7..trimmed.len() - 1]; // Remove "Option<" and ">"
        let inner_clean = normalize_type_name(inner.trim(), class_name);
        return (format!("Option{}", inner_clean), true);
    }

    // Handle Self -> class_name
    if trimmed == "Self" {
        return (class_name.to_string(), false);
    }

    // Handle std types that need .into() conversion for FFI
    // These are Rust std types that have FFI equivalents (e.g., String -> AzString)
    if trimmed == "String" {
        return ("String".to_string(), true);
    }

    // No conversion needed
    (trimmed.to_string(), false)
}

/// Split generic args like "X, Y" handling nested generics
fn split_generic_args(s: &str) -> Option<(String, String)> {
    let mut depth = 0;
    let mut split_pos = None;

    for (i, c) in s.chars().enumerate() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                split_pos = Some(i);
                break;
            }
            _ => {}
        }
    }

    split_pos.map(|pos| (s[..pos].trim().to_string(), s[pos + 1..].trim().to_string()))
}

/// Normalize a type name for FFI result types
/// - Self -> class_name
/// - Remove leading & or &mut
/// - Remove leading * or *mut
fn normalize_type_name(ty: &str, class_name: &str) -> String {
    let trimmed = ty.trim();

    // Handle Self
    if trimmed == "Self" {
        return class_name.to_string();
    }

    // Strip reference prefixes
    let stripped = trimmed
        .strip_prefix("&mut ")
        .or_else(|| trimmed.strip_prefix("& "))
        .or_else(|| trimmed.strip_prefix("&"))
        .or_else(|| trimmed.strip_prefix("*mut "))
        .or_else(|| trimmed.strip_prefix("*const "))
        .unwrap_or(trimmed);

    stripped.trim().to_string()
}

/// Convert a Rust argument type to an FFI-compatible type
/// Returns (ffi_type, accessor_suffix) where accessor_suffix is appended to the variable in fn_body
/// e.g. ("String", ".as_str()") for &str
fn convert_arg_type_for_ffi(ty: &str) -> (String, Option<String>) {
    let trimmed = ty.trim();

    // Handle &str and str -> String (need .as_str() in fn_body)
    if trimmed == "&str" || trimmed == "str" {
        return ("String".to_string(), Some(".as_str()".to_string()));
    }

    // Handle &[u8] and [u8] -> U8VecRef (need .as_slice() in fn_body)
    if trimmed == "&[u8]" || trimmed == "[u8]" {
        return ("U8VecRef".to_string(), Some(".as_slice()".to_string()));
    }

    // Handle &String -> String (need .as_str() in fn_body if function expects &str)
    if trimmed == "&String" {
        return ("String".to_string(), Some(".as_str()".to_string()));
    }

    // Handle &Vec<u8> -> U8VecRef (need .as_slice() in fn_body)
    if trimmed == "&Vec<u8>" || trimmed == "Vec<u8>" {
        return ("U8VecRef".to_string(), Some(".as_slice()".to_string()));
    }

    // Handle generic slices &[T] -> TypeVecRef with .as_slice()
    if trimmed.starts_with("&[") && trimmed.ends_with(']') {
        let inner = &trimmed[2..trimmed.len() - 1];
        let inner_clean = inner.trim();
        return (
            format!("{}VecRef", inner_clean),
            Some(".as_slice()".to_string()),
        );
    }

    // Handle reference types - strip the & and use the underlying type
    if let Some(inner) = trimmed.strip_prefix("&mut ") {
        return (inner.trim().to_string(), None);
    }
    if let Some(inner) = trimmed.strip_prefix("& ") {
        return (inner.trim().to_string(), None);
    }
    if let Some(inner) = trimmed.strip_prefix("&") {
        return (inner.trim().to_string(), None);
    }

    // No conversion needed
    (trimmed.to_string(), None)
}

// // helper functions for list/add/remove
//
/// List all functions for a type, comparing source vs api.json
pub fn list_type_functions(
    type_name: &str,
    type_index: &TypeIndex,
    api_data: &ApiData,
    version: &str,
) -> Result<FunctionListResult, String> {
    // Find type in source
    let type_def = type_index
        .resolve(type_name, None)
        .ok_or_else(|| format!("Type '{}' not found in source code", type_name))?;

    // Compare with api.json
    let comparison = compare_type_functions(type_def, api_data, version)
        .ok_or_else(|| format!("Type '{}' not found in api.json", type_name))?;

    Ok(FunctionListResult {
        type_name: type_name.to_string(),
        source_only: comparison
            .missing_in_api
            .into_iter()
            .map(|f| f.name)
            .collect(),
        api_only: comparison.extra_in_api,
        both: comparison.matching,
    })
}

/// Result of listing functions for a type
#[derive(Debug)]
pub struct FunctionListResult {
    pub type_name: String,
    /// Functions only in source code
    pub source_only: Vec<String>,
    /// Functions only in api.json
    pub api_only: Vec<String>,
    /// Functions in both
    pub both: Vec<String>,
}

/// Find dependent types for a method
/// Returns types that would need to be added to api.json
/// Uses converted type names (e.g., ResultXY instead of Result<X, Y>)
pub fn find_method_dependent_types(
    method: &MethodDef,
    api_data: &ApiData,
    version: &str,
    class_name: &str,
) -> Vec<String> {
    let mut missing_types = Vec::new();

    let version_data = match api_data.get_version(version) {
        Some(v) => v,
        None => return missing_types,
    };

    // Check argument types
    for arg in &method.args {
        let ty = &arg.ty;
        // Don't convert argument types - they're used as-is
        if find_type_module(ty, version_data).is_none() && !is_primitive_type(ty) {
            missing_types.push(ty.clone());
        }
    }

    // Check return type - use converted form for Result/Option
    if let Some(ret_ty) = &method.return_type {
        let (converted_ty, _needs_into) = convert_return_type_for_ffi(ret_ty, class_name);
        if find_type_module(&converted_ty, version_data).is_none()
            && !is_primitive_type(&converted_ty)
        {
            missing_types.push(converted_ty);
        }
    }

    missing_types.sort();
    missing_types.dedup();
    missing_types
}

fn is_primitive_type(type_name: &str) -> bool {
    const PRIMITIVES: &[&str] = &[
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char", "()", "Self",
    ];
    PRIMITIVES.contains(&type_name)
}

// patch generation
/// Generate a patch to add functions to api.json
pub fn generate_add_functions_patch(
    type_name: &str,
    methods: &[&MethodDef],
    module_name: &str,
    version: &str,
    type_def: &TypeDefinition,
) -> ApiPatch {
    let mut functions: IndexMap<String, FunctionData> = IndexMap::new();
    let mut constructors: IndexMap<String, FunctionData> = IndexMap::new();

    // Use the full_path from type_def (e.g. "azul_core::dom::Dom")
    let full_path = &type_def.full_path;

    for method in methods {
        let fn_data = method_to_function_data(method, full_path);

        if method.is_constructor {
            constructors.insert(method.name.clone(), fn_data);
        } else {
            functions.insert(method.name.clone(), fn_data);
        }
    }

    let mut class_patch = ClassPatch::default();

    if !functions.is_empty() {
        class_patch.functions = Some(functions);
        class_patch.add_functions = Some(true); // Merge with existing
    }

    if !constructors.is_empty() {
        class_patch.constructors = Some(constructors);
        class_patch.add_constructors = Some(true); // Merge with existing
    }

    let mut classes = BTreeMap::new();
    classes.insert(type_name.to_string(), class_patch);

    let mut modules = BTreeMap::new();
    modules.insert(module_name.to_string(), ModulePatch { classes });

    let mut versions = BTreeMap::new();
    versions.insert(version.to_string(), VersionPatch { modules });

    ApiPatch { versions }
}

/// Generate a patch to remove functions and/or constructors from api.json
///
/// The function will check api.json to determine if each name is a constructor or function
/// and remove it from the appropriate collection.
pub fn generate_remove_functions_patch(
    type_name: &str,
    function_names: &[&str],
    module_name: &str,
    version: &str,
) -> ApiPatch {
    // For now, add both remove_functions and remove_constructors
    // The patch application logic will handle whichever is present
    let mut class_patch = ClassPatch::default();
    class_patch.remove_functions = Some(function_names.iter().map(|s| s.to_string()).collect());
    class_patch.remove_constructors = Some(function_names.iter().map(|s| s.to_string()).collect());

    let mut classes = BTreeMap::new();
    classes.insert(type_name.to_string(), class_patch);

    let mut modules = BTreeMap::new();
    modules.insert(module_name.to_string(), ModulePatch { classes });

    let mut versions = BTreeMap::new();
    versions.insert(version.to_string(), VersionPatch { modules });

    ApiPatch { versions }
}

/// Generate a patch to remove an entire type from api.json
pub fn generate_remove_type_patch(
    type_name: &str,
    module_name: &str,
    version: &str,
) -> ApiPatch {
    let mut class_patch = ClassPatch::default();
    // Set remove to signal that the entire class should be removed
    class_patch.remove = Some(true);

    let mut classes = BTreeMap::new();
    classes.insert(type_name.to_string(), class_patch);

    let mut modules = BTreeMap::new();
    modules.insert(module_name.to_string(), ModulePatch { classes });

    let mut versions = BTreeMap::new();
    versions.insert(version.to_string(), VersionPatch { modules });

    ApiPatch { versions }
}

/// Convert a MethodDef to FunctionData for api.json
fn method_to_function_data(method: &MethodDef, full_path: &str) -> FunctionData {
    use super::type_index::SelfKind;

    // Extract class name from full_path for Self replacement
    let class_name = full_path.rsplit("::").next().unwrap_or(full_path);

    // Build fn_args - first add self if present (non-constructor)
    let mut fn_args: Vec<IndexMap<String, String>> = Vec::new();

    // Track argument accessors for fn_body generation
    // Maps arg_name -> accessor_suffix (e.g. "svg_string" -> ".as_str()")
    let mut arg_accessors: Vec<(String, Option<String>)> = Vec::new();

    // Add self parameter for non-static, non-constructor methods
    if !method.is_constructor {
        if let Some(ref self_kind) = method.self_kind {
            let mut self_arg = IndexMap::new();
            let self_str = match self_kind {
                SelfKind::Value => "value",
                SelfKind::Ref => "ref",
                SelfKind::RefMut => "refmut",
            };
            self_arg.insert("self".to_string(), self_str.to_string());
            fn_args.push(self_arg);
        }
    }

    // Add remaining arguments with FFI type conversion
    for arg in &method.args {
        let mut arg_map = IndexMap::new();
        let (ffi_type, accessor) = convert_arg_type_for_ffi(&arg.ty);
        arg_map.insert(arg.name.clone(), ffi_type);
        fn_args.push(arg_map);
        arg_accessors.push((arg.name.clone(), accessor));
    }

    // Build returns - convert Result<X, Y> to ResultXY, Option<X> to OptionX
    // Also track if we need to add .into() to the fn_body
    let (returns, needs_into) = if method.is_constructor {
        // Constructors don't specify returns in api.json (implicit Self)
        // But if they return Result<Self, E>, we need to convert it
        if let Some(ref ret_ty) = method.return_type {
            let (converted, needs_into) = convert_return_type_for_ffi(ret_ty, class_name);
            if converted != class_name && converted != "Self" {
                // Constructor returns Result or Option - need explicit returns
                (
                    Some(ReturnTypeData {
                        r#type: converted,
                        doc: None,
                    }),
                    needs_into,
                )
            } else {
                (None, false)
            }
        } else {
            (None, false)
        }
    } else {
        if let Some(ref ret_ty) = method.return_type {
            let (converted, needs_into) = convert_return_type_for_ffi(ret_ty, class_name);
            (
                Some(ReturnTypeData {
                    r#type: converted,
                    doc: None,
                }),
                needs_into,
            )
        } else {
            (None, false)
        }
    };

    // Generate fn_body using the full external path
    let mut fn_body_str = generate_fn_body(method, full_path);

    // Apply argument accessors to fn_body
    // Replace each argument reference with the accessor version
    for (arg_name, accessor_opt) in &arg_accessors {
        if let Some(accessor) = accessor_opt {
            // Replace "arg_name," or "arg_name)" patterns
            // This handles cases like func(arg_name, other) or func(arg_name)
            fn_body_str = fn_body_str.replace(
                &format!("{},", arg_name),
                &format!("{}{},", arg_name, accessor),
            );
            fn_body_str = fn_body_str.replace(
                &format!("{})", arg_name),
                &format!("{}{})", arg_name, accessor),
            );
        }
    }

    // Add .into() if the return type was converted (Result/Option wrapper types)
    if needs_into {
        fn_body_str = format!("{}.into()", fn_body_str);
    }

    let fn_body = Some(fn_body_str);

    // Build doc
    let doc = if method.doc.is_empty() {
        None
    } else {
        Some(method.doc.clone())
    };

    FunctionData {
        doc,
        fn_args,
        returns,
        fn_body,
        use_patches: None,
        const_fn: false,
        generic_params: None,
        generic_bounds: None,
    }
}

// type addition with transitive dependencies
use super::{diff::TypeAddition, module_map::determine_module};

/// Result of adding a type with its dependencies
#[derive(Debug)]
pub struct AddTypeResult {
    /// The primary type being added
    pub primary_type: String,
    /// Module the primary type was added to
    pub primary_module: String,
    /// All types that were added (including transitive dependencies)
    pub added_types: Vec<(String, String)>, // (type_name, module)
    /// Methods that were added to the primary type
    pub added_methods: Vec<String>,
    /// Types that were already in api.json (skipped)
    pub skipped_types: Vec<String>,
    /// Types that couldn't be found in workspace (warnings)
    pub missing_types: Vec<String>,
}

/// Check if a type already exists in api.json
pub fn type_exists_in_api(type_name: &str, version_data: &VersionData) -> bool {
    find_type_module(type_name, version_data).is_some()
}

/// Helper to extract fields from TypeDefKind (expands MacroGenerated types)
fn get_fields_from_kind(type_def: &TypeDefinition) -> Vec<(String, String, RefKind)> {
    let expanded = type_def.expand_macro_generated();
    match &expanded {
        TypeDefKind::Struct { fields, .. } => fields
            .iter()
            .map(|(name, f)| (name.clone(), f.ty.clone(), f.ref_kind.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Helper to extract variants from TypeDefKind (expands MacroGenerated types)
fn get_variants_from_kind(type_def: &TypeDefinition) -> Vec<(String, Option<String>)> {
    let expanded = type_def.expand_macro_generated();
    match &expanded {
        TypeDefKind::Enum { variants, .. } => variants
            .iter()
            .map(|(name, v)| (name.clone(), v.ty.clone()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Helper to extract derives from TypeDefKind (expands MacroGenerated types)
fn get_derives_from_kind(type_def: &TypeDefinition) -> Vec<String> {
    let expanded = type_def.expand_macro_generated();
    match &expanded {
        TypeDefKind::Struct { derives, .. } => derives.clone(),
        TypeDefKind::Enum { derives, .. } => derives.clone(),
        _ => Vec::new(),
    }
}

/// Helper to check if TypeDefKind is an enum (expands MacroGenerated types)
fn is_enum_kind(type_def: &TypeDefinition) -> bool {
    let expanded = type_def.expand_macro_generated();
    matches!(expanded, TypeDefKind::Enum { .. })
}

/// Helper to check if TypeDefKind is a callback (expands MacroGenerated types)
fn is_callback_kind(type_def: &TypeDefinition) -> bool {
    let expanded = type_def.expand_macro_generated();
    matches!(expanded, TypeDefKind::CallbackTypedef { .. })
}

/// Helper to check if TypeDefKind is a type alias
fn is_type_alias_kind(type_def: &TypeDefinition) -> bool {
    matches!(&type_def.kind, TypeDefKind::TypeAlias { .. })
}

/// Get type alias info (target type, ref_kind) if this is a type alias
/// Parses pointer types like "*mut c_void" into (base_type, ref_kind)
fn get_type_alias_info(type_def: &TypeDefinition) -> Option<(String, RefKind)> {
    match &type_def.kind {
        TypeDefKind::TypeAlias { target, .. } => {
            // Parse pointer prefixes from the target type
            let trimmed = target.trim();
            if let Some(rest) = trimmed.strip_prefix("*mut ") {
                Some((rest.trim().to_string(), RefKind::MutPtr))
            } else if let Some(rest) = trimmed.strip_prefix("*const ") {
                Some((rest.trim().to_string(), RefKind::ConstPtr))
            } else if let Some(rest) = trimmed.strip_prefix("&mut ") {
                Some((rest.trim().to_string(), RefKind::RefMut))
            } else if let Some(rest) = trimmed.strip_prefix('&') {
                Some((rest.trim().to_string(), RefKind::Ref))
            } else {
                Some((trimmed.to_string(), RefKind::Value))
            }
        }
        _ => None,
    }
}

/// Get type alias target if this is a type alias
fn get_type_alias_target(
    type_def: &TypeDefinition,
) -> Option<crate::autofix::patch_format::TypeAliasDef> {
    match &type_def.kind {
        TypeDefKind::TypeAlias { target, .. } => {
            // Parse the target string to extract ref_kind if it's a pointer type
            let (base_target, ref_kind) = if target.starts_with("*const ") {
                (
                    target.strip_prefix("*const ").unwrap().to_string(),
                    Some("constptr".to_string()),
                )
            } else if target.starts_with("*mut ") {
                (
                    target.strip_prefix("*mut ").unwrap().to_string(),
                    Some("mutptr".to_string()),
                )
            } else if target.starts_with("* const ") {
                (
                    target.strip_prefix("* const ").unwrap().to_string(),
                    Some("constptr".to_string()),
                )
            } else if target.starts_with("* mut ") {
                (
                    target.strip_prefix("* mut ").unwrap().to_string(),
                    Some("mutptr".to_string()),
                )
            } else {
                (target.clone(), None)
            };
            Some(crate::autofix::patch_format::TypeAliasDef {
                target: base_target,
                ref_kind,
            })
        }
        _ => None,
    }
}

/// Get callback typedef info if this is a callback typedef
fn get_callback_typedef_info(
    type_def: &TypeDefinition,
) -> Option<(Vec<(String, String)>, Option<String>)> {
    let expanded = type_def.expand_macro_generated();
    match expanded {
        TypeDefKind::CallbackTypedef { args, returns } => {
            let arg_list: Vec<(String, String)> = args
                .iter()
                .map(|a| {
                    let ref_kind_str = match a.ref_kind {
                        RefKind::ConstPtr => "constptr".to_string(),
                        RefKind::MutPtr => "mutptr".to_string(),
                        RefKind::Ref => "ref".to_string(),
                        RefKind::RefMut => "refmut".to_string(),
                        _ => "value".to_string(),
                    };
                    (a.ty.clone(), ref_kind_str)
                })
                .collect();
            Some((arg_list, returns))
        }
        _ => None,
    }
}

/// Generate patches to add a type and all its transitive dependencies
///
/// This function:
/// 1. Finds the type in the workspace index
/// 2. Determines the correct module using determine_module()
/// 3. Collects all types referenced by the type's fields, methods, etc.
/// 4. Recursively adds those types if they're not in api.json
/// 5. Returns patches for all types that need to be added
pub fn generate_add_type_patches(
    type_name: &str,
    method_spec: Option<&str>, /* None = add type only, Some("*") = all methods, Some("name") =
                                * specific method */
    index: &TypeIndex,
    version_data: &VersionData,
    version: &str,
) -> Result<
    (
        Vec<crate::autofix::patch_format::AutofixPatch>,
        AddTypeResult,
    ),
    String,
> {
    use crate::autofix::patch_format::{
        AddOperation, AutofixPatch, FieldDef, PatchOperation, TypeKind,
    };

    let mut patches = Vec::new();
    let mut result = AddTypeResult {
        primary_type: type_name.to_string(),
        primary_module: String::new(),
        added_types: Vec::new(),
        added_methods: Vec::new(),
        skipped_types: Vec::new(),
        missing_types: Vec::new(),
    };

    // Track which types we've already processed to avoid infinite loops
    let mut processed: BTreeSet<String> = BTreeSet::new();
    let mut to_process: Vec<String> = vec![type_name.to_string()];

    // Primitives that don't need to be added
    let primitives: BTreeSet<&str> = [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char", "c_void", "String", "()", "Self",
    ]
    .into_iter()
    .collect();

    while let Some(current_type) = to_process.pop() {
        if processed.contains(&current_type) {
            continue;
        }
        processed.insert(current_type.clone());

        // Skip primitives
        if primitives.contains(current_type.as_str()) {
            continue;
        }

        // Check if already in api.json
        if type_exists_in_api(&current_type, version_data) {
            result.skipped_types.push(current_type.clone());
            continue;
        }

        // Find in workspace
        let type_def = match index.resolve(&current_type, None) {
            Some(t) => t,
            None => {
                result.missing_types.push(current_type.clone());
                continue;
            }
        };

        // Determine module
        let (module_name, is_misc) = determine_module(&current_type);
        if is_misc {
            eprintln!(
                "[WARN] Type '{}' mapped to 'misc' module - consider adding a keyword mapping",
                current_type
            );
        }

        if current_type == type_name {
            result.primary_module = module_name.clone();
        }

        // Collect referenced types from fields using helper functions
        let mut referenced_types: Vec<String> = Vec::new();

        let fields = get_fields_from_kind(type_def);
        for (_, ty, _) in &fields {
            collect_types_from_type_str(ty, &mut referenced_types);
        }

        let variants = get_variants_from_kind(type_def);
        for (_, ty_opt) in &variants {
            if let Some(ty) = ty_opt {
                collect_types_from_type_str(ty, &mut referenced_types);
            }
        }

        // Add referenced types to process queue
        for ref_type in &referenced_types {
            if !processed.contains(ref_type) && !primitives.contains(ref_type.as_str()) {
                to_process.push(ref_type.clone());
            }
        }

        // Generate patch for this type
        let kind = if is_type_alias_kind(type_def) {
            TypeKind::TypeAlias
        } else if is_enum_kind(type_def) {
            TypeKind::Enum
        } else if is_callback_kind(type_def) {
            TypeKind::CallbackTypedef
        } else {
            TypeKind::Struct
        };

        // Get type alias target if this is a type alias
        let type_alias_target = get_type_alias_target(type_def);

        // Build struct_fields
        let struct_fields: Option<Vec<FieldDef>> = if !fields.is_empty() {
            Some(
                fields
                    .iter()
                    .map(|(name, ty, ref_kind)| FieldDef {
                        name: name.clone(),
                        field_type: ty.clone(),
                        ref_kind: match ref_kind {
                            RefKind::Value => None,
                            RefKind::Ref => Some("ref".to_string()),
                            RefKind::RefMut => Some("refmut".to_string()),
                            RefKind::ConstPtr => Some("constptr".to_string()),
                            RefKind::MutPtr => Some("mutptr".to_string()),
                            _ => None,
                        },
                        doc: None,
                    })
                    .collect(),
            )
        } else {
            None
        };

        // Build enum_variants
        let enum_variants: Option<Vec<crate::autofix::patch_format::VariantDef>> =
            if !variants.is_empty() {
                Some(
                    variants
                        .iter()
                        .map(|(name, ty_opt)| crate::autofix::patch_format::VariantDef {
                            name: name.clone(),
                            variant_type: ty_opt.clone(),
                        })
                        .collect(),
                )
            } else {
                None
            };

        // Build derives
        let derives_list = get_derives_from_kind(type_def);
        let derives = if derives_list.is_empty() {
            None
        } else {
            Some(derives_list)
        };

        // Build callback_typedef if applicable
        let callback_typedef = get_callback_typedef_info(type_def).map(|(args, returns)| {
            crate::autofix::patch_format::CallbackTypedefDef {
                fn_args: args
                    .iter()
                    .map(|(ty, ref_kind)| crate::autofix::patch_format::CallbackArg {
                        arg_type: ty.clone(),
                        ref_kind: if ref_kind == "value" {
                            None
                        } else {
                            Some(ref_kind.clone())
                        },
                    })
                    .collect(),
                returns: returns.map(|r| crate::autofix::patch_format::CallbackReturn {
                    return_type: r,
                    ref_kind: None,
                }),
            }
        });

        // Create the patch
        let mut patch = AutofixPatch::new(format!("Add type {}", current_type));
        patch.add_operation(PatchOperation::Add(AddOperation {
            type_name: current_type.clone(),
            external: type_def.full_path.clone(),
            kind,
            module: Some(module_name.clone()),
            derives,
            repr_c: if type_alias_target.is_some() || callback_typedef.is_some() {
                None
            } else {
                Some(true)
            },
            struct_fields,
            enum_variants,
            callback_typedef,
            type_alias: type_alias_target,
        }));

        patches.push(patch);
        result.added_types.push((current_type.clone(), module_name));
    }

    // Now add methods to the primary type if requested
    if let Some(spec) = method_spec {
        let type_def = index
            .resolve(type_name, None)
            .ok_or_else(|| format!("Type '{}' not found", type_name))?;

        let methods: Vec<_> = type_def
            .methods
            .iter()
            .filter(|m| m.is_public)
            .filter(|m| spec == "*" || m.name == spec)
            .collect();

        if !methods.is_empty() {
            // Collect types from method signatures
            for method in &methods {
                for arg in &method.args {
                    let mut refs = Vec::new();
                    collect_types_from_type_str(&arg.ty, &mut refs);
                    for ref_type in refs {
                        if !processed.contains(&ref_type)
                            && !primitives.contains(ref_type.as_str())
                            && !type_exists_in_api(&ref_type, version_data)
                        {
                            // Need to add this type too
                            if let Some(ref_type_def) = index.resolve(&ref_type, None) {
                                let (module, _) = determine_module(&ref_type);
                                let ref_derives = get_derives_from_kind(ref_type_def);
                                // Generate a simple add patch for the referenced type
                                let mut ref_patch =
                                    AutofixPatch::new(format!("Add type {}", ref_type));
                                ref_patch.add_operation(PatchOperation::Add(AddOperation {
                                    type_name: ref_type.clone(),
                                    external: ref_type_def.full_path.clone(),
                                    kind: if is_enum_kind(ref_type_def) {
                                        TypeKind::Enum
                                    } else {
                                        TypeKind::Struct
                                    },
                                    module: Some(module.clone()),
                                    derives: if ref_derives.is_empty() {
                                        None
                                    } else {
                                        Some(ref_derives)
                                    },
                                    repr_c: Some(true),
                                    struct_fields: None, // Simplified - autofix run will fill these
                                    enum_variants: None,
                                    callback_typedef: None,
                                    type_alias: None,
                                }));
                                patches.push(ref_patch);
                                result.added_types.push((ref_type.clone(), module));
                                processed.insert(ref_type); // Mark as processed to avoid duplicates
                            }
                        }
                    }
                }
                if let Some(ret) = &method.return_type {
                    // First, convert the return type to its FFI form (Result<X,Y> -> ResultXY)
                    let (converted_ret, _needs_into) = convert_return_type_for_ffi(ret, type_name);

                    // Collect types from the converted return type
                    let mut refs = Vec::new();
                    collect_types_from_type_str(&converted_ret, &mut refs);

                    // For Result/Option wrapper types, we need to add both:
                    // 1. The wrapper type itself (ResultXY, OptionX)
                    // 2. The inner types (X, Y)
                    if converted_ret.starts_with("Result")
                        && converted_ret != "Result"
                        && !converted_ret.starts_with("Result<")
                    {
                        // This is a ResultXY style type
                        refs.push(converted_ret.clone());
                    } else if converted_ret.starts_with("Option")
                        && converted_ret != "Option"
                        && !converted_ret.starts_with("Option<")
                    {
                        // This is an OptionX style type
                        refs.push(converted_ret.clone());
                    }

                    // Also collect types from the original return type for inner types
                    collect_types_from_type_str(ret, &mut refs);

                    for ref_type in refs {
                        if !processed.contains(&ref_type)
                            && !primitives.contains(ref_type.as_str())
                            && ref_type != "Result"
                            && ref_type != "Option"
                            && !type_exists_in_api(&ref_type, version_data)
                        {
                            if let Some(ref_type_def) = index.resolve(&ref_type, None) {
                                let (module, _) = determine_module(&ref_type);
                                let ref_derives = get_derives_from_kind(ref_type_def);
                                let mut ref_patch =
                                    AutofixPatch::new(format!("Add type {}", ref_type));
                                ref_patch.add_operation(PatchOperation::Add(AddOperation {
                                    type_name: ref_type.clone(),
                                    external: ref_type_def.full_path.clone(),
                                    kind: if is_enum_kind(ref_type_def) {
                                        TypeKind::Enum
                                    } else {
                                        TypeKind::Struct
                                    },
                                    module: Some(module.clone()),
                                    derives: if ref_derives.is_empty() {
                                        None
                                    } else {
                                        Some(ref_derives)
                                    },
                                    repr_c: Some(true),
                                    struct_fields: None,
                                    enum_variants: None,
                                    callback_typedef: None,
                                    type_alias: None,
                                }));
                                patches.push(ref_patch);
                                result.added_types.push((ref_type.clone(), module));
                                processed.insert(ref_type); // Mark as processed to avoid duplicates
                            }
                        }
                    }
                }

                result.added_methods.push(method.name.clone());
            }

            // Generate the functions patch
            let func_patch = generate_add_functions_patch(
                type_name,
                &methods,
                &result.primary_module,
                version,
                type_def,
            );

            // Convert ApiPatch to AutofixPatch format
            // For now, we'll write the function patch separately
            // The caller should apply both the type patches and the function patch
        }
    }

    Ok((patches, result))
}

/// Extract type names from a type string like "Vec<Foo>" or "Option<Bar>"
fn collect_types_from_type_str(type_str: &str, out: &mut Vec<String>) {
    // Remove references and pointers
    let cleaned = type_str
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim_start_matches('*')
        .trim_start_matches("const ")
        .trim();

    // Handle generic types like Vec<T>, Option<T>, etc.
    if let Some(start) = cleaned.find('<') {
        let base = &cleaned[..start];
        out.push(base.to_string());

        if let Some(end) = cleaned.rfind('>') {
            let inner = &cleaned[start + 1..end];
            // Handle multiple generic args separated by comma
            for part in inner.split(',') {
                collect_types_from_type_str(part.trim(), out);
            }
        }
    } else {
        // Simple type
        if !cleaned.is_empty() {
            out.push(cleaned.to_string());
        }
    }
}
