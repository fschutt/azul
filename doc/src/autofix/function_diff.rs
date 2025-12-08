//! Function Diff Generation
//!
//! This module compares the methods from source code with functions in api.json
//! and provides tools to list, add, and remove functions.

use std::collections::{BTreeMap, HashMap, HashSet};
use indexmap::IndexMap;

use super::type_index::{TypeIndex, TypeDefinition, MethodDef, SelfKind, RefKind};
use crate::api::{ApiData, ClassData, FunctionData, ModuleData, VersionData, ReturnTypeData};
use crate::patch::{ApiPatch, VersionPatch, ModulePatch, ClassPatch};

// ============================================================================
// DATA STRUCTURES
// ============================================================================

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

// ============================================================================
// CONVERSION FUNCTIONS
// ============================================================================

/// Convert a MethodDef to FunctionInfo
pub fn method_to_function_info(method: &MethodDef) -> FunctionInfo {
    let args: Vec<(String, String, String)> = method.args.iter()
        .map(|a| (a.name.clone(), a.ty.clone(), ref_kind_to_string(&a.ref_kind)))
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

// ============================================================================
// COMPARISON FUNCTIONS
// ============================================================================

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
    let source_methods: HashMap<String, &MethodDef> = type_def.methods.iter()
        .filter(|m| m.is_public)
        .map(|m| (m.name.clone(), m))
        .collect();
    
    // Get api.json functions (both constructors and regular functions)
    let mut api_functions: HashSet<String> = HashSet::new();
    if let Some(ref fns) = api_class.functions {
        api_functions.extend(fns.keys().cloned());
    }
    if let Some(ref ctors) = api_class.constructors {
        api_functions.extend(ctors.keys().cloned());
    }
    
    let source_names: HashSet<String> = source_methods.keys().cloned().collect();
    
    // Missing in API (in source but not in api.json)
    let missing_in_api: Vec<FunctionInfo> = source_names
        .difference(&api_functions)
        .filter_map(|name| source_methods.get(name))
        .map(|m| method_to_function_info(m))
        .collect();
    
    // Extra in API (in api.json but not in source)
    let extra_in_api: Vec<String> = api_functions
        .difference(&source_names)
        .cloned()
        .collect();
    
    // Matching functions
    let matching: Vec<String> = source_names
        .intersection(&api_functions)
        .cloned()
        .collect();
    
    // TODO: Check for differences in matching functions
    let differences = Vec::new();
    
    Some(FunctionComparison {
        missing_in_api,
        extra_in_api,
        differences,
        matching,
    })
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

// ============================================================================
// FN_BODY GENERATION
// ============================================================================

/// Generate fn_body for a method based on its signature
/// This creates the FFI wrapper function body that bridges Rust to C
pub fn generate_fn_body(
    method: &MethodDef,
    type_name: &str,
    crate_prefix: &str,
) -> String {
    // Determine function type based on self_kind and return type
    let fn_type = determine_fn_type(method, type_name);
    
    // Generate code based on function type
    match fn_type.as_str() {
        "constructor" => generate_constructor_body(method, type_name, crate_prefix),
        "getter" => generate_getter_body(method, type_name, crate_prefix),
        "setter" => generate_setter_body(method, type_name, crate_prefix),
        "method" => generate_method_body(method, type_name, crate_prefix),
        "static" => generate_static_body(method, type_name, crate_prefix),
        "destructor" => generate_destructor_body(method, type_name, crate_prefix),
        _ => generate_method_body(method, type_name, crate_prefix),
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
        && (method.return_type.is_none() || method.return_type.as_ref().map(|s| s.as_str()) == Some("Self"))
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

fn generate_constructor_body(
    method: &MethodDef,
    type_name: &str,
    crate_prefix: &str,
) -> String {
    // fn_body should just be the call expression - the wrapper code is auto-generated
    let args_str = method.args.iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    
    let full_type = if crate_prefix.is_empty() {
        type_name.to_string()
    } else {
        format!("{}::{}", crate_prefix, type_name)
    };
    
    format!("{}::{}({})", full_type, method.name, args_str)
}

fn generate_getter_body(
    method: &MethodDef,
    type_name: &str,
    crate_prefix: &str,
) -> String {
    // fn_body should just be the call expression
    format!("object.{}()", method.name)
}

fn generate_setter_body(
    method: &MethodDef,
    type_name: &str,
    crate_prefix: &str,
) -> String {
    // fn_body should just be the call expression
    let arg_name = method.args.first().map(|a| a.name.clone()).unwrap_or_default();
    format!("object.{}({})", method.name, arg_name)
}

fn generate_method_body(
    method: &MethodDef,
    type_name: &str,
    crate_prefix: &str,
) -> String {
    // fn_body should just be the call expression
    let args_str = method.args.iter()
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
        let full_type = if crate_prefix.is_empty() {
            type_name.to_string()
        } else {
            format!("{}::{}", crate_prefix, type_name)
        };
        format!("{}::{}({})", full_type, method.name, args_str)
    }
}

fn generate_static_body(
    method: &MethodDef,
    type_name: &str,
    crate_prefix: &str,
) -> String {
    // Static function: no self pointer
    let args_str = method.args.iter()
        .map(|a| a.name.clone())
        .collect::<Vec<_>>()
        .join(", ");
    
    let full_type = if crate_prefix.is_empty() {
        type_name.to_string()
    } else {
        format!("{}::{}", crate_prefix, type_name)
    };
    
    format!("{}::{}({})", full_type, method.name, args_str)
}

fn generate_destructor_body(
    method: &MethodDef,
    type_name: &str,
    crate_prefix: &str,
) -> String {
    // fn_body should just be the drop expression
    "core::mem::drop(object)".to_string()
}

// ============================================================================
// HELPER FUNCTIONS FOR LIST/ADD/REMOVE
// ============================================================================

/// List all functions for a type, comparing source vs api.json
pub fn list_type_functions(
    type_name: &str,
    type_index: &TypeIndex,
    api_data: &ApiData,
    version: &str,
) -> Result<FunctionListResult, String> {
    // Find type in source
    let type_def = type_index.resolve(type_name, None)
        .ok_or_else(|| format!("Type '{}' not found in source code", type_name))?;
    
    // Compare with api.json
    let comparison = compare_type_functions(type_def, api_data, version)
        .ok_or_else(|| format!("Type '{}' not found in api.json", type_name))?;
    
    Ok(FunctionListResult {
        type_name: type_name.to_string(),
        source_only: comparison.missing_in_api.into_iter().map(|f| f.name).collect(),
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
pub fn find_method_dependent_types(
    method: &MethodDef,
    api_data: &ApiData,
    version: &str,
) -> Vec<String> {
    let mut missing_types = Vec::new();
    
    let version_data = match api_data.get_version(version) {
        Some(v) => v,
        None => return missing_types,
    };
    
    // Check argument types
    for arg in &method.args {
        if find_type_module(&arg.ty, version_data).is_none() && !is_primitive_type(&arg.ty) {
            missing_types.push(arg.ty.clone());
        }
    }
    
    // Check return type
    if let Some(ret_ty) = &method.return_type {
        if find_type_module(ret_ty, version_data).is_none() && !is_primitive_type(ret_ty) {
            missing_types.push(ret_ty.clone());
        }
    }
    
    missing_types.sort();
    missing_types.dedup();
    missing_types
}

fn is_primitive_type(type_name: &str) -> bool {
    const PRIMITIVES: &[&str] = &[
        "i8", "i16", "i32", "i64", "i128", "isize",
        "u8", "u16", "u32", "u64", "u128", "usize",
        "f32", "f64", "bool", "char", "()", "Self",
    ];
    PRIMITIVES.contains(&type_name)
}

// ============================================================================
// PATCH GENERATION
// ============================================================================

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
    
    // Determine crate prefix from type_def
    let crate_prefix = match type_def.crate_name.as_str() {
        "azul_core" => "crate::azul_impl",
        "azul_css" => "crate::azul_impl::css",
        "azul_layout" => "crate::azul_impl",
        _ => "crate",
    };
    
    for method in methods {
        let fn_data = method_to_function_data(method, type_name, crate_prefix);
        
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

/// Generate a patch to remove functions from api.json
pub fn generate_remove_functions_patch(
    type_name: &str,
    function_names: &[&str],
    module_name: &str,
    version: &str,
) -> ApiPatch {
    let mut class_patch = ClassPatch::default();
    class_patch.remove_functions = Some(function_names.iter().map(|s| s.to_string()).collect());
    
    let mut classes = BTreeMap::new();
    classes.insert(type_name.to_string(), class_patch);
    
    let mut modules = BTreeMap::new();
    modules.insert(module_name.to_string(), ModulePatch { classes });
    
    let mut versions = BTreeMap::new();
    versions.insert(version.to_string(), VersionPatch { modules });
    
    ApiPatch { versions }
}

/// Convert a MethodDef to FunctionData for api.json
fn method_to_function_data(method: &MethodDef, type_name: &str, crate_prefix: &str) -> FunctionData {
    // Build fn_args
    let mut fn_args: Vec<IndexMap<String, String>> = Vec::new();
    
    for arg in &method.args {
        let mut arg_map = IndexMap::new();
        arg_map.insert(arg.name.clone(), arg.ty.clone());
        fn_args.push(arg_map);
    }
    
    // Build returns - constructors don't need it (implicit Self)
    let returns = if method.is_constructor {
        None
    } else {
        method.return_type.as_ref().map(|ret_ty| {
            ReturnTypeData {
                r#type: ret_ty.clone(),
                doc: None,
            }
        })
    };
    
    // Generate fn_body
    let fn_body = Some(generate_fn_body(method, type_name, crate_prefix));
    
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
