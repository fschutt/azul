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

    // Get source methods: public, and not a standard trait's impl (`default`,
    // `clone`, `drop`, ... are derives / custom_impls in api.json, never
    // functions - listing them as "missing" sent people adding them by hand).
    let source_methods: BTreeMap<String, &MethodDef> = type_def
        .methods
        .iter()
        .filter(|m| m.is_public && !m.is_non_api_trait_impl())
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
        } else {
            // Argument types, in order: what `autofix add` would write for the
            // source argument (a generic `C: Into<T>` resolved to `T`) against
            // what api.json declares. A drifted type is invisible to every
            // other check - the codegen trusts api.json - so e.g. a callback
            // declared as its bare function-pointer typedef, where the source
            // takes the context-carrying wrapper, silently lost the context.
            let api_args = api_fn
                .fn_args
                .iter()
                .filter(|arg_map| !arg_map.contains_key("self"))
                .flat_map(|arg_map| arg_map.iter());
            for ((api_name, api_ty), source_arg) in api_args.zip(source_method.args.iter()) {
                let (source_ty, accessor) = source_arg_ffi_type(source_arg);
                if !same_ffi_arg_type(api_ty, &source_ty) {
                    diffs.push(format!(
                        "argument `{api_name}`: api.json declares `{api_ty}`, source takes `{source_ty}`"
                    ));
                } else if accessor
                    .as_deref()
                    .is_some_and(|a| a.contains("into_library_owned_string"))
                    && api_fn
                        .fn_body
                        .as_deref()
                        .is_some_and(|body| passes_bare(body, api_name))
                {
                    // Same type on both sides (`String`), but the C side
                    // passes an AzString and the method takes a std String.
                    diffs.push(format!(
                        "argument `{api_name}`: the source takes a std `String`, the fn_body \
                         passes the AzString as it is (`{api_name}.into_library_owned_string()`)"
                    ));
                }
            }
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

/// An api.json function whose fn_body calls a Rust method its type no
/// longer has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoneApiFunction {
    /// The api.json module of the class
    pub module: String,
    /// The api.json class
    pub class: String,
    /// The api.json name (`create_old`, `set_title_row`)
    pub api_name: String,
    /// The Rust method its fn_body calls (`new_old`, `set_title_row`)
    pub rust_name: String,
    /// In `constructors` (else in `functions`)
    pub is_constructor: bool,
}

/// Methods a type has without an impl block the index can see: derive
/// output and std (blanket) trait methods.
const DERIVED_OR_BLANKET_METHODS: &[&str] = &[
    "clone", "clone_from", "to_string", "to_owned", "into", "try_into", "from", "try_from",
    "eq", "ne", "cmp", "partial_cmp", "lt", "le", "gt", "ge", "max", "min", "clamp", "hash",
    "fmt", "default", "as_ref", "as_mut", "borrow", "borrow_mut", "deref", "deref_mut", "drop",
];

/// The identifier at the start of `s`.
fn leading_ident(s: &str) -> &str {
    let end = s
        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .unwrap_or(s.len());
    &s[..end]
}

/// The Rust method an api.json fn_body calls on its own class: the first
/// `object.m(..)` of the body, else `<path>::m(..)` for one of the class's
/// `paths`. None for any other body (a free function, a field access, an
/// expression the scan cannot read).
fn called_method<'a>(fn_body: &'a str, paths: &[&str]) -> Option<&'a str> {
    let mut from = 0;
    while let Some(pos) = fn_body[from..].find("object.") {
        let at = from + pos;
        let whole_word = fn_body[..at]
            .chars()
            .next_back()
            .map_or(true, |c| !(c.is_ascii_alphanumeric() || c == '_'));
        if whole_word {
            let rest = &fn_body[at + "object.".len()..];
            let name = leading_ident(rest);
            return (!name.is_empty() && rest[name.len()..].starts_with('(')).then_some(name);
        }
        from = at + "object.".len();
    }
    for path in paths.iter().filter(|p| !p.is_empty()) {
        let needle = format!("{path}::");
        if let Some(pos) = fn_body.find(&needle) {
            let rest = &fn_body[pos + needle.len()..];
            let name = leading_ident(rest);
            if !name.is_empty() && rest[name.len()..].starts_with('(') {
                return Some(name);
            }
        }
    }
    None
}

/// Whether the source file text defines a fn `name` (`fn name(` /
/// `fn name<`): the safety net for methods the index does not extract
/// (generic ones that are not `Into<T>`-bound).
fn source_file_defines_fn(file: &std::path::Path, name: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(file) else {
        return false;
    };
    let needle = format!("fn {name}");
    text.match_indices(&needle).any(|(at, _)| {
        let after = text[at + needle.len()..].trim_start();
        after.starts_with('(') || after.starts_with('<')
    })
}

/// Every api.json function whose Rust method is gone: its fn_body calls
/// `object.m(..)` or `<the class's path>::m(..)` and the class's source type
/// has no method `m` (BLOCKS' 84 preset-shell setters, wave 5, which the
/// scan kept until they were removed by hand). Not judged: a class without a
/// source type or whose api.json path names a different type (a path fix
/// comes first), a macro-made type (impl_vec!, impl_option!: methods the
/// index does not see), a type with a `Deref` impl, any other body shape,
/// and derive / std-trait methods ([`DERIVED_OR_BLANKET_METHODS`]).
pub fn gone_api_functions(index: &TypeIndex, api_data: &ApiData) -> Vec<GoneApiFunction> {
    let Some(version) = api_data.get_latest_version_str() else {
        return Vec::new();
    };
    let Some(version_data) = api_data.get_version(version) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (module_name, module) in &version_data.api {
        for (class_name, class) in &module.classes {
            let Some(external) = class.external.as_deref() else {
                continue;
            };
            // The type at the class's own path first (two types of one name
            // in one crate are different types)
            let Some(def) = index
                .get_by_path(external)
                .or_else(|| index.resolve(class_name, None))
                .or_else(|| index.resolve(&format!("Az{class_name}"), None))
            else {
                continue;
            };
            if !super::diff::paths_are_equivalent(external, &def.full_path) {
                continue;
            }
            let derefs = match &def.kind {
                TypeDefKind::Struct { custom_impls, .. } | TypeDefKind::Enum { custom_impls, .. } => {
                    custom_impls.iter().any(|t| t.ends_with("Deref"))
                }
                _ => false,
            } || def
                .methods
                .iter()
                .any(|m| m.from_trait.as_deref() == Some("Deref"));
            if def.is_macro_generated() || derefs {
                continue;
            }
            let entries = class
                .constructors
                .iter()
                .flat_map(|c| c.iter().map(|e| (e, true)))
                .chain(
                    class
                        .functions
                        .iter()
                        .flat_map(|f| f.iter().map(|e| (e, false))),
                );
            for ((api_name, f), is_constructor) in entries {
                let Some(body) = f.fn_body.as_deref() else {
                    continue;
                };
                let Some(rust_name) = called_method(body, &[external, def.full_path.as_str()])
                else {
                    continue;
                };
                if DERIVED_OR_BLANKET_METHODS.contains(&rust_name)
                    || def.methods.iter().any(|m| m.name == rust_name)
                    || source_file_defines_fn(&def.file_path, rust_name)
                {
                    continue;
                }
                out.push(GoneApiFunction {
                    module: module_name.clone(),
                    class: class_name.clone(),
                    api_name: api_name.clone(),
                    rust_name: rust_name.to_string(),
                    is_constructor,
                });
            }
        }
    }
    out
}

/// Whether `body` passes the argument `name` to a call as it is
/// (`f(name)`, `f(a, name, b)`), not through a method of its own.
fn passes_bare(body: &str, name: &str) -> bool {
    [
        format!("({name})"),
        format!("({name},"),
        format!(", {name})"),
        format!(", {name},"),
    ]
    .iter()
    .any(|p| body.contains(p.as_str()))
}

/// Find a class/type in api.json for a specific version
pub(crate) fn find_api_class<'a>(type_name: &str, version_data: &'a VersionData) -> Option<&'a ClassData> {
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
        _ => generate_method_body(method, full_path),
    }
}

/// Determine the type of function based on method signature
fn determine_fn_type(method: &MethodDef, type_name: &str) -> String {
    // Constructor: no self, returns Self or type_name
    if method.is_constructor {
        return "constructor".to_string();
    }

    // A `destroy*` method is called like any other (a `drop` is a Drop
    // impl's: never a candidate). It used to be `core::mem::drop(object)`:
    // the method was not called, and the codegen does not rewrite a bare
    // `object`.

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
            || method.return_type.as_deref() == Some("Self"))
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

// return type conversion for ffi
/// Convert Rust return types to api.json format:
/// - `Result<X, Y>` -> `ResultXY` (with `.into()` added to fn_body)
/// - `Option<X>` -> `OptionX` (with `.into()` added to fn_body)
/// - `Self` -> class_name
///
/// - a borrowed `str` -> `String`, `Option<&str>` / `Option<String>` ->
///   `OptionString` (see [`ReturnConversion`])
///
/// Returns (converted_type, conversion): how the fn_body turns the Rust
/// call's value into the api.json type.
fn convert_return_type_for_ffi(return_type: &str, class_name: &str) -> (String, ReturnConversion) {
    let trimmed = return_type.trim();

    // A borrowed `str` (`&str`, extracted as `str` + a Ref kind) crosses as an
    // owned `String`; an `Option` of a `str` or of a std `String` as
    // `OptionString`, which converts from `Option<AzString>` only (`AzString`
    // is extracted as `String` too; `AzString::from` is the identity there).
    // Without this, `as_str -> str` and `link_str -> Optionstr` reached
    // api.json and the codegen emitted `Azstr` / `AzOptionstr` (wave 5).
    if trimmed == "str" || trimmed == "&str" {
        return ("String".to_string(), ReturnConversion::OwnedStr);
    }
    if let Some(inner) = trimmed
        .strip_prefix("Option<")
        .and_then(|rest| rest.strip_suffix('>'))
    {
        let inner = inner.trim().trim_start_matches('&').trim();
        if inner == "str" || inner == "String" {
            return ("OptionString".to_string(), ReturnConversion::OptionOwnedStr);
        }
    }

    let (converted, needs_into) = convert_return_type_for_ffi_inner(trimmed, class_name);
    let conversion = if needs_into {
        ReturnConversion::Into
    } else {
        ReturnConversion::None
    };
    (converted, conversion)
}

/// How a generated fn_body turns the Rust method's return value into the
/// api.json return type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReturnConversion {
    /// The value is the api.json type
    None,
    /// `<call>.into()` (`Result<X, Y>` -> `ResultXY`, `Option<X>` -> `OptionX`, `String`)
    Into,
    /// `azul_css::AzString::from(<call>)`: a borrowed `str` copied into a `String`
    OwnedStr,
    /// `<call>.map(|s| azul_css::AzString::from(s)).into()`: an `Option` of a
    /// `str` or a std `String` as `OptionString`
    OptionOwnedStr,
}

impl ReturnConversion {
    fn wrap(self, call: String) -> String {
        match self {
            ReturnConversion::None => call,
            ReturnConversion::Into => format!("{call}.into()"),
            ReturnConversion::OwnedStr => format!("azul_css::AzString::from({call})"),
            ReturnConversion::OptionOwnedStr => {
                format!("{call}.map(|s| azul_css::AzString::from(s)).into()")
            }
        }
    }
}

/// The Result / Option / Self / String rules of [`convert_return_type_for_ffi`].
fn convert_return_type_for_ffi_inner(trimmed: &str, class_name: &str) -> (String, bool) {
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
        // Use canonicalize_option_type_name for correct casing (OptionU8, not Optionu8)
        return (
            crate::autofix::utils::canonicalize_option_type_name(&inner_clean),
            true,
        );
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

/// Does api.json's `api_ty` declare the source argument `source_ty` (as
/// `source_arg_ffi_type` spells it)? Besides equality, three representations
/// are the same argument: a borrowed `&T` (`*const T`) passed by value and
/// re-borrowed in the `fn_body`, a byte slice (`U8VecRef`) passed as an owned
/// `U8Vec`, and `Option<T>` spelled as its FFI option (`OptionT`).
fn same_ffi_arg_type(api_ty: &str, source_ty: &str) -> bool {
    let norm = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
    let (api_ty, source_ty) = (norm(api_ty), norm(source_ty));
    if api_ty == source_ty {
        return true;
    }
    if source_ty.strip_prefix("*const ") == Some(api_ty.as_str()) {
        return true;
    }
    if source_ty == "U8VecRef" && api_ty == "U8Vec" {
        return true;
    }
    if let Some(inner) = source_ty
        .strip_prefix("Option<")
        .and_then(|t| t.strip_suffix('>'))
    {
        let inner = inner.trim();
        let mut cap = inner.chars();
        let upper = cap
            .next()
            .map(|c| c.to_ascii_uppercase().to_string() + cap.as_str())
            .unwrap_or_default();
        return api_ty == format!("Option{upper}");
    }
    false
}

/// Every api.json function whose signature differs from its source method
/// (self kind, argument count, argument types), by class, sorted.
pub fn signature_drift(
    index: &TypeIndex,
    api_data: &ApiData,
) -> Vec<(String, FunctionDiff)> {
    let Some(version) = api_data.get_latest_version_str() else {
        return Vec::new();
    };
    let Some(version_data) = api_data.get_version(version) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for module in version_data.api.values() {
        for class_name in module.classes.keys() {
            let Some(type_def) = index.resolve(class_name, None) else {
                continue;
            };
            if let Some(cmp) = compare_type_functions(type_def, api_data, version) {
                out.extend(cmp.differences.into_iter().map(|d| (class_name.clone(), d)));
            }
        }
    }
    out.sort_by(|a, b| (&a.0, &a.1.name).cmp(&(&b.0, &b.1.name)));
    out
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

    // Reference to an API type: crosses the ABI as a raw pointer (the FFI
    // checker forbids bare `&T` in signatures) and the fn_body re-borrows
    // it. The PREVIOUS behavior stripped the `&` down to a VALUE, so the
    // generated call tried to MOVE a struct the caller still owns — a
    // signature mismatch that broke codegen compilation, silently, on the
    // first method imported with a reference argument. The accessor is a
    // `{}` template (the whole argument expression is substituted), unlike
    // the plain-suffix accessors above.
    if let Some(inner) = trimmed.strip_prefix("&mut ") {
        return (
            format!("*mut {}", inner.trim()),
            Some("unsafe { &mut *{} }".to_string()),
        );
    }
    if let Some(inner) = trimmed.strip_prefix("& ") {
        return (
            format!("*const {}", inner.trim()),
            Some("unsafe { &*{} }".to_string()),
        );
    }
    if let Some(inner) = trimmed.strip_prefix("&") {
        return (
            format!("*const {}", inner.trim()),
            Some("unsafe { &*{} }".to_string()),
        );
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
        differences: comparison.differences,
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
    /// Functions in both whose signatures differ (self kind, argument count or
    /// argument types)
    pub differences: Vec<FunctionDiff>,
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
/// The API name of a Rust method. Public constructors are `create*` (`new`
/// is reserved in C++, Java, C# and JavaScript): `new` -> `create`,
/// `new_x` -> `create_x`. The fn_body keeps calling the Rust name.
pub fn api_name_of(method: &MethodDef) -> String {
    if method.name == "new" {
        "create".to_string()
    } else if let Some(rest) = method.name.strip_prefix("new_") {
        format!("create_{rest}")
    } else {
        method.name.clone()
    }
}

/// THE answer to "which methods of `type_name` does `autofix add
/// <Type>.<spec>` export" (the add command had three copies of this filter
/// and only one skipped trait impls, so `default` constructors went out on
/// 2026-10-01): public; not a standard trait's impl (those are derives /
/// custom_impls); matching `spec` (`*` = all, else the Rust or the API name);
/// and, when the type is already in the API, not reached by an existing
/// entry - under its API name, or as the call of an entry's body (`create`
/// whose body is `T::new(..)`). For `*`, only methods whose signature
/// crosses the FFI ([`wildcard_skip_reason`]; `carries` says which named
/// types the FFI carries - [`ffi_carries`] for the real API).
pub fn api_candidate_methods<'a>(
    type_name: &str,
    methods: &[&'a MethodDef],
    spec: &str,
    api_class: Option<&ClassData>,
    carries: &dyn Fn(&str) -> bool,
) -> Vec<&'a MethodDef> {
    let existing: Vec<(&String, &FunctionData)> = api_class
        .map(|c| {
            c.constructors
                .iter()
                .chain(c.functions.iter())
                .flat_map(|entries| entries.iter())
                .collect()
        })
        .unwrap_or_default();
    methods
        .iter()
        .copied()
        .filter(|m| m.is_public && !m.is_non_api_trait_impl())
        // A renamed `new` / `new_x` yields to a real method of that API name:
        // the type's own `create` is the FFI-shaped constructor, its `new` a
        // Rust convenience (`RichBlock::new(kind, Vec<RichRun>)`).
        .filter(|m| {
            let api_name = api_name_of(m);
            api_name == m.name
                || !methods
                    .iter()
                    .any(|o| o.is_public && !o.is_non_api_trait_impl() && o.name == api_name)
        })
        .filter(|m| spec == "*" || m.name == spec || api_name_of(m) == spec)
        .filter(|m| spec != "*" || wildcard_skip_reason(m, type_name, carries).is_none())
        .filter(|m| {
            let api_name = api_name_of(m);
            let static_call = format!("{type_name}::{}(", m.name);
            let method_call = format!("object.{}(", m.name);
            !existing.iter().any(|(name, f)| {
                **name == api_name
                    || f.fn_body.as_deref().is_some_and(|body| {
                        body.contains(&static_call) || body.starts_with(&method_call)
                    })
            })
        })
        .collect()
}

/// Why `autofix add <Type>.*` leaves `method` out, if it does - THE
/// wildcard rule. `Type.*` exports a method only when its whole signature
/// crosses the FFI as written:
/// - it returns no borrow (`&T`, `&str`, `&[T]`, `&mut T`, `Option<&T>`):
///   those are Rust-side accessors;
/// - every argument and the return type, spelled as api.json spells it
///   (`&str` -> `String`, `&T` -> `*const T`, `Option<X>` -> `OptionX`), is a
///   scalar, the type itself, or a type `carries` says the FFI carries (an
///   api.json type, a C-repr workspace type: [`ffi_carries`]). `Vec<T>`,
///   slices, tuples, `Option<&str>` arguments, `Instant`, a struct without a
///   C repr have no FFI form.
///
/// Everything else is a Rust-only helper (RichTextDoc's 29 of wave 5). A
/// helper that HAS an FFI form but is not API stays `pub(crate)` (house
/// rule); a Rust-only method the API does need is added by name
/// (`autofix add Type.method`), which this rule does not filter.
pub fn wildcard_skip_reason(
    method: &MethodDef,
    type_name: &str,
    carries: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let returns_borrow = matches!(method.return_ref_kind, RefKind::Ref | RefKind::RefMut)
        || method.return_type.as_deref().is_some_and(|r| r.contains('&'));
    if returns_borrow {
        return Some(format!("returns a borrow: {}", method.signature()));
    }
    for arg in &method.args {
        let (ffi, _) = source_arg_ffi_type(arg);
        if !ffi_type_is_carried(&ffi, type_name, carries) {
            return Some(format!(
                "argument `{}: {}{}` has no FFI form",
                arg.name,
                arg.ref_kind.as_prefix(),
                arg.ty
            ));
        }
    }
    if let Some(ret) = &method.return_type {
        let (ffi, _) = convert_return_type_for_ffi(ret, type_name);
        if !ffi_type_is_carried(&ffi, type_name, carries) {
            return Some(format!("returns `{ret}`, which has no FFI form"));
        }
    }
    None
}

/// Whether an api.json type spelling (`String`, `*const Foo`, `OptionU32`)
/// names something the FFI carries: one plain name, a scalar, the type
/// itself, or what `carries` accepts.
fn ffi_type_is_carried(ffi: &str, type_name: &str, carries: &dyn Fn(&str) -> bool) -> bool {
    let base = ffi.trim();
    let base = base
        .strip_prefix("*const ")
        .or_else(|| base.strip_prefix("*mut "))
        .unwrap_or(base)
        .trim();
    if base.is_empty() || !base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return false;
    }
    base == type_name || base == "c_void" || is_primitive_type(base) || carries(base)
}

pub fn generate_add_functions_patch(
    type_name: &str,
    methods: &[&MethodDef],
    module_name: &str,
    version: &str,
    type_def: &TypeDefinition,
) -> ApiPatch {
    // Use the full_path from type_def (e.g. "azul_core::dom::Dom")
    let full_path = &type_def.full_path;
    let entries = methods
        .iter()
        .map(|method| {
            (
                api_name_of(method),
                method_to_function_data(method, full_path),
                method.is_constructor,
            )
        })
        .collect();
    generate_add_entries_patch(type_name, entries, module_name, version)
}

/// A patch adding `entries` - (api.json name, entry, is a constructor) -
/// to the class `type_name`, merged with what the class has.
pub fn generate_add_entries_patch(
    type_name: &str,
    entries: Vec<(String, FunctionData, bool)>,
    module_name: &str,
    version: &str,
) -> ApiPatch {
    let mut functions: IndexMap<String, FunctionData> = IndexMap::new();
    let mut constructors: IndexMap<String, FunctionData> = IndexMap::new();
    for (name, data, is_constructor) in entries {
        if is_constructor {
            constructors.insert(name, data);
        } else {
            functions.insert(name, data);
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
    // Both lists: the patch application removes whichever exists
    generate_remove_entries_patch(type_name, function_names, function_names, module_name, version)
}

/// A patch removing exactly these `functions` and `constructors` of a class
/// (an empty list removes nothing from that map).
pub fn generate_remove_entries_patch(
    type_name: &str,
    functions: &[&str],
    constructors: &[&str],
    module_name: &str,
    version: &str,
) -> ApiPatch {
    let names = |list: &[&str]| -> Option<Vec<String>> {
        (!list.is_empty()).then(|| list.iter().map(|s| s.to_string()).collect())
    };
    let mut class_patch = ClassPatch::default();
    class_patch.remove_functions = names(functions);
    class_patch.remove_constructors = names(constructors);

    let mut classes = BTreeMap::new();
    classes.insert(type_name.to_string(), class_patch);

    let mut modules = BTreeMap::new();
    modules.insert(module_name.to_string(), ModulePatch { classes });

    let mut versions = BTreeMap::new();
    versions.insert(version.to_string(), VersionPatch { modules });

    ApiPatch { versions }
}

/// Generate a patch to remove an entire type from api.json
pub fn generate_remove_type_patch(type_name: &str, module_name: &str, version: &str) -> ApiPatch {
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

/// What `autofix remove` / `autofix difficult remove` removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveTarget {
    /// A whole class: `Type` or `module.Type`
    Class { module: String, class: String },
    /// One function or constructor: `Type.fn` or `module.Type.fn`
    Function {
        module: String,
        class: String,
        name: String,
    },
}

impl RemoveTarget {
    /// The removal patch (the one-item commands' format).
    pub fn patch(&self, version: &str) -> ApiPatch {
        match self {
            RemoveTarget::Class { module, class } => {
                generate_remove_type_patch(class, module, version)
            }
            RemoveTarget::Function {
                module,
                class,
                name,
            } => generate_remove_functions_patch(class, &[name.as_str()], module, version),
        }
    }

    /// The patch file name (`remove_<class>.patch.json`,
    /// `remove_<class>_<fn>.patch.json`).
    pub fn file_name(&self) -> String {
        match self {
            RemoveTarget::Class { class, .. } => {
                format!("remove_{}.patch.json", class.to_lowercase())
            }
            RemoveTarget::Function { class, name, .. } => {
                format!("remove_{}_{}.patch.json", class.to_lowercase(), name)
            }
        }
    }

    /// For the console: `widgets.ModuleSwitcher (whole class)`,
    /// `widgets.ModuleSwitcher.dom`.
    pub fn describe(&self) -> String {
        match self {
            RemoveTarget::Class { module, class } => format!("{module}.{class} (whole class)"),
            RemoveTarget::Function {
                module,
                class,
                name,
            } => format!("{module}.{class}.{name}"),
        }
    }
}

/// Parse an item of `autofix remove` / `autofix difficult remove`:
/// `module.Type.fn`, `Type.fn`, `module.Type` (a whole class) or `Type`. A
/// two-part item is a class when its first part is a module that has it,
/// else a function. The module comes from api.json (the one the class IS in).
pub fn parse_remove_spec(spec: &str, version_data: &VersionData) -> Result<RemoveTarget, String> {
    let parts: Vec<&str> = spec.split('.').filter(|p| !p.is_empty()).collect();
    let class_in = |module: &str, class: &str| {
        version_data
            .api
            .get(module)
            .is_some_and(|m| m.classes.contains_key(class))
    };
    let module_of = |class: &str| {
        find_type_module(class, version_data)
            .map(str::to_string)
            .ok_or_else(|| format!("Type '{class}' not found in api.json"))
    };
    match parts.as_slice() {
        [] => Err(format!("empty item '{spec}'")),
        [class] => Ok(RemoveTarget::Class {
            module: module_of(*class)?,
            class: class.to_string(),
        }),
        [module, class] if class_in(*module, *class) => Ok(RemoveTarget::Class {
            module: module.to_string(),
            class: class.to_string(),
        }),
        [class, name] => Ok(RemoveTarget::Function {
            module: module_of(*class)?,
            class: class.to_string(),
            name: name.to_string(),
        }),
        [.., module, class, name] => {
            let module = if class_in(*module, *class) {
                module.to_string()
            } else {
                module_of(*class)?
            };
            Ok(RemoveTarget::Function {
                module,
                class: class.to_string(),
                name: name.to_string(),
            })
        }
    }
}

/// Convert a MethodDef to FunctionData for api.json
/// A source argument's api.json type and the `fn_body` accessor it needs:
/// what `autofix add` writes for it, and so what an existing api.json entry
/// must declare for it. Generic `C: Into<T>` parameters arrive here already
/// resolved to `T` (`type_index::extract_into_bounds`).
fn source_arg_ffi_type(arg: &super::type_index::MethodArg) -> (String, Option<String>) {
    // A string argument crosses as an `AzString` (api.json `String`). The
    // index spells both a std `String` and an `AzString` `String`; the type
    // as written tells them apart. A std `String` is converted (passing the
    // AzString as it is broke the dll: RichRun.set_text, wave 5), a borrowed
    // one re-borrowed; a generic `S: Into<AzString>` takes it as it is.
    if arg.ty == "String" {
        let std_string = arg.source_ty == "String";
        let accessor = match (&arg.ref_kind, std_string) {
            (crate::api::RefKind::Value, true) => Some("{}.into_library_owned_string()"),
            (crate::api::RefKind::Ref, true) => Some("&{}.into_library_owned_string()"),
            (crate::api::RefKind::Ref, false) => Some("&{}"),
            _ => None,
        };
        if accessor.is_some() || arg.ref_kind == crate::api::RefKind::Value {
            return ("String".to_string(), accessor.map(str::to_string));
        }
    }
    // A borrowed callback info crosses by value and is re-borrowed (a `&mut`
    // one from a mutable copy the fn_body binds: `rebound_handle_args`)
    if is_by_value_handle(&arg.ty) {
        match arg.ref_kind {
            crate::api::RefKind::RefMut => return (arg.ty.clone(), Some("&mut {}".to_string())),
            crate::api::RefKind::Ref => return (arg.ty.clone(), Some("&{}".to_string())),
            _ => {}
        }
    }
    let (ffi_type, accessor) = convert_arg_type_for_ffi(&arg.ty);
    // The source parser splits `&mut Dom` into ty="Dom" + ref_kind=RefMut
    // BEFORE this point, so the string-prefix arms in
    // `convert_arg_type_for_ffi` never see a reference. Wrap API types
    // here per the FFI checker's own policy — pointers, never `&T` — and
    // re-borrow in the fn_body via a `{}` template accessor. Dropping the
    // ref to a VALUE (the old behavior) generated a call that MOVED a
    // struct the caller still owns: broken codegen, silently, on the
    // first imported method with a reference argument.
    if accessor.is_none() && ffi_type != "String" && !ffi_type.ends_with("VecRef") {
        match arg.ref_kind {
            crate::api::RefKind::Ref => (
                format!("*const {ffi_type}"),
                Some("unsafe { &*{} }".to_string()),
            ),
            crate::api::RefKind::RefMut => (
                format!("*mut {ffi_type}"),
                Some("unsafe { &mut *{} }".to_string()),
            ),
            _ => (ffi_type, accessor),
        }
    } else {
        (ffi_type, accessor)
    }
}

/// Whether a borrowed argument of type `ty` crosses the FFI by value: a
/// callback info handle (`CallbackInfo`, `TimerCallbackInfo`, ...). A
/// callback receives its info by value (CallbackType's fn_args) and the info
/// is a set of pointers into the window state, so a `&mut` call through a
/// copy changes the same state: `{ let mut info = info; f(&mut info, ..) }`
/// (ProgressBar.update_progress, TextInput.set_text_in). Any other struct
/// borrowed `&mut` crosses as a pointer: a copy would lose the change.
fn is_by_value_handle(ty: &str) -> bool {
    ty.ends_with("CallbackInfo")
}

/// The arguments a fn_body binds as a mutable copy before the call: the
/// `&mut` callback infos ([`is_by_value_handle`]).
fn rebound_handle_args(method: &MethodDef) -> Vec<&str> {
    method
        .args
        .iter()
        .filter(|a| is_by_value_handle(&a.ty) && a.ref_kind == crate::api::RefKind::RefMut)
        .map(|a| a.name.as_str())
        .collect()
}

/// The api.json entry `autofix add <class>.<api_name> --fn <path>` writes,
/// and whether it is a constructor: a function of `class_name` whose body
/// calls the free function. A first argument of the class's own type (by
/// value, `&` or `&mut`) is the receiver (`self`: value / ref / refmut),
/// passed on under the codegen's receiver name (`raw_image` for RawImage:
/// the generated function's parameter, which no fn_body rewrite has to
/// find); every other argument as for a method. A constructor when the API
/// name is `create*` and the function returns the class.
pub fn free_fn_entry(
    class_name: &str,
    api_name: &str,
    free_fn: &super::type_index::FreeFnDef,
) -> (FunctionData, bool) {
    let mut method = free_fn.method.clone();
    let receiver = method
        .args
        .first()
        .filter(|a| {
            a.ty == class_name
                && matches!(
                    a.ref_kind,
                    crate::api::RefKind::Value | crate::api::RefKind::Ref | crate::api::RefKind::RefMut
                )
        })
        .cloned();
    let mut call_args: Vec<String> = Vec::new();
    if let Some(receiver) = &receiver {
        method.args.remove(0);
        method.self_kind = Some(match receiver.ref_kind {
            crate::api::RefKind::Ref => SelfKind::Ref,
            crate::api::RefKind::RefMut => SelfKind::RefMut,
            _ => SelfKind::Value,
        });
        call_args.push(crate::codegen::v2::ir::receiver_arg_name(class_name));
    }
    call_args.extend(method.args.iter().map(|a| a.name.clone()));
    method.is_constructor =
        receiver.is_none() && method.is_constructor && api_name.starts_with("create");
    let call = format!("{}({})", free_fn.path, call_args.join(", "));
    let is_constructor = method.is_constructor;
    (function_data_for_call(&method, class_name, call), is_constructor)
}

fn method_to_function_data(method: &MethodDef, full_path: &str) -> FunctionData {
    // Extract class name from full_path for Self replacement
    let class_name = full_path.rsplit("::").next().unwrap_or(full_path);
    function_data_for_call(method, class_name, generate_fn_body(method, full_path))
}

/// The api.json entry for `method` of `class_name` whose fn_body is `call`
/// (the plain call: the arguments by name), with the arguments converted
/// for the FFI and the return value converted to its api.json type.
fn function_data_for_call(method: &MethodDef, class_name: &str, call: String) -> FunctionData {
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
        let (ffi_type, accessor) = source_arg_ffi_type(arg);
        arg_map.insert(arg.name.clone(), ffi_type);
        fn_args.push(arg_map);
        arg_accessors.push((arg.name.clone(), accessor));
    }

    // Build returns - convert Result<X, Y> to ResultXY, Option<X> to OptionX
    // Also track if we need to add .into() to the fn_body
    let (returns, conversion) = if method.is_constructor {
        // Constructors don't specify returns in api.json (implicit Self)
        // But if they return Result<Self, E>, we need to convert it
        if let Some(ref ret_ty) = method.return_type {
            let (converted, conversion) = convert_return_type_for_ffi(ret_ty, class_name);
            if converted != class_name && converted != "Self" {
                // Constructor returns Result or Option - need explicit returns
                (
                    Some(ReturnTypeData {
                        r#type: converted,
                        doc: None,
                    }),
                    conversion,
                )
            } else {
                (None, ReturnConversion::None)
            }
        } else {
            (None, ReturnConversion::None)
        }
    } else if let Some(ref ret_ty) = method.return_type {
        let (converted, conversion) = convert_return_type_for_ffi(ret_ty, class_name);
        (
            Some(ReturnTypeData {
                r#type: converted,
                doc: None,
            }),
            conversion,
        )
    } else {
        (None, ReturnConversion::None)
    };

    let mut fn_body_str = call;

    // Apply argument accessors to fn_body
    // Replace each argument reference with the accessor version
    for (arg_name, accessor_opt) in &arg_accessors {
        if let Some(accessor) = accessor_opt {
            if accessor.contains("{}") {
                // Template accessor: the WHOLE argument expression is
                // substituted (pointer re-borrows like `unsafe { &mut *x }`).
                let wrapped = accessor.replace("{}", arg_name);
                fn_body_str = fn_body_str
                    .replace(&format!("({},", arg_name), &format!("({},", wrapped))
                    .replace(&format!(", {},", arg_name), &format!(", {},", wrapped))
                    .replace(&format!("({})", arg_name), &format!("({})", wrapped))
                    .replace(&format!(", {})", arg_name), &format!(", {})", wrapped));
                continue;
            }
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

    // Convert the return value to its api.json type (`.into()` for the
    // Result / Option wrappers, an owned `String` for a borrowed `str`)
    let fn_body_str = conversion.wrap(fn_body_str);

    // A `&mut` callback info is re-borrowed from a mutable copy
    let rebound = rebound_handle_args(method);
    let fn_body_str = if rebound.is_empty() {
        fn_body_str
    } else {
        let bindings: String = rebound
            .iter()
            .map(|name| format!("let mut {name} = {name}; "))
            .collect();
        format!("{{ {bindings}{fn_body_str} }}")
    };

    let fn_body = Some(fn_body_str);

    // Build doc
    let doc = if method.doc.is_empty() {
        None
    } else {
        Some(method.doc.clone())
    };

    FunctionData {
        doc,
        // Curated by hand in api.json; a diff-derived function has none.
        priority: None,
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
    /// The functions / constructors of the primary type (merge mode), when
    /// methods were requested and some matched
    pub functions_patch: Option<ApiPatch>,
}

/// Check if a type already exists in api.json
pub fn type_exists_in_api(type_name: &str, version_data: &VersionData) -> bool {
    find_type_module(type_name, version_data).is_some()
}

/// THE `carries` predicate of the `Type.*` rule ([`wildcard_skip_reason`])
/// for the real API: a type api.json has, or a workspace type the FFI can
/// carry - a struct / enum with a C (or transparent) repr, a callback
/// typedef, a type alias, or a macro-made Vec / Option / Result.
pub fn ffi_carries(type_name: &str, version_data: &VersionData, index: &TypeIndex) -> bool {
    if type_exists_in_api(type_name, version_data) {
        return true;
    }
    let Some(def) = index
        .resolve(type_name, None)
        .or_else(|| index.resolve(&format!("Az{type_name}"), None))
    else {
        return false;
    };
    match &def.kind {
        TypeDefKind::Struct { repr, .. } | TypeDefKind::Enum { repr, .. } => {
            repr.as_deref().is_some_and(|r| {
                let r = r.to_lowercase();
                r.contains('c') || r.contains("transparent")
            })
        }
        TypeDefKind::CallbackTypedef { .. }
        | TypeDefKind::TypeAlias { .. }
        | TypeDefKind::MacroGenerated { .. } => true,
    }
}

/// Helper to extract fields from TypeDefKind (expands MacroGenerated types)
fn get_fields_from_kind(type_def: &TypeDefinition) -> Vec<(String, String, RefKind)> {
    let expanded = type_def.expand_macro_generated();
    match &expanded {
        TypeDefKind::Struct { fields, .. } => fields
            .iter()
            .map(|(name, f)| (name.clone(), f.ty.clone(), f.ref_kind))
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
) -> Option<(Vec<(Option<String>, String, String)>, Option<String>)> {
    let expanded = type_def.expand_macro_generated();
    match expanded {
        TypeDefKind::CallbackTypedef { args, returns } => {
            let arg_list: Vec<(Option<String>, String, String)> = args
                .iter()
                .map(|a| {
                    let ref_kind_str = match a.ref_kind {
                        RefKind::ConstPtr => "constptr".to_string(),
                        RefKind::MutPtr => "mutptr".to_string(),
                        RefKind::Ref => "ref".to_string(),
                        RefKind::RefMut => "refmut".to_string(),
                        _ => "value".to_string(),
                    };
                    (a.name.clone(), a.ty.clone(), ref_kind_str)
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
/// 2. Determines the module with module_map::new_type_module (the one the scan keeps)
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
        functions_patch: None,
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

        // The module the scan keeps a new type in (see new_type_module)
        let (module_name, is_misc) =
            crate::autofix::module_map::new_type_module(&current_type, &type_def.full_path);
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

        // Build enum_variants - split pointer prefixes out of the type string
        // into ref_kind so api.json stores the canonical `T` + `ref_kind` shape.
        let enum_variants: Option<Vec<crate::autofix::patch_format::VariantDef>> = if !variants
            .is_empty()
        {
            Some(
                variants
                    .iter()
                    .map(|(name, ty_opt)| {
                        let (base, rk) = match ty_opt.as_deref() {
                            Some(t) => {
                                let (b, k) = crate::autofix::utils::extract_type_and_ref_kind(t);
                                (Some(b), k)
                            }
                            None => (None, crate::api::RefKind::Value),
                        };
                        crate::autofix::patch_format::VariantDef {
                            name: name.clone(),
                            variant_type: base,
                            ref_kind: rk,
                        }
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
                    .map(|(name, ty, ref_kind)| crate::autofix::patch_format::CallbackArg {
                        name: name.clone(),
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

        // A standard trait's impl method (`impl Default for T`) is not an API
        // function: it becomes a `custom_impls` entry below, and the codegen
        // makes `T_default` from that. Exporting it as a constructor named
        // `default` is what the FFI check rejects.
        let all: Vec<&MethodDef> = type_def.methods.iter().collect();
        let methods = api_candidate_methods(
            type_name,
            &all,
            spec,
            find_api_class(type_name, version_data),
            &|t: &str| ffi_carries(t, version_data, index),
        );
        let mut std_impls: Vec<String> = type_def
            .methods
            .iter()
            .filter(|m| m.is_std_trait_impl())
            .filter_map(|m| m.from_trait.clone())
            .collect();
        std_impls.sort();
        std_impls.dedup();
        if spec == "*" && !std_impls.is_empty() {
            let mut impl_patch = AutofixPatch::new(format!(
                "Custom impls of {}: {}",
                type_name,
                std_impls.join(", ")
            ));
            impl_patch.add_operation(PatchOperation::Modify(
                crate::autofix::patch_format::ModifyOperation {
                    type_name: type_name.to_string(),
                    module: Some(result.primary_module.clone()),
                    changes: vec![crate::autofix::patch_format::ModifyChange::AddCustomImpls {
                        impls: std_impls,
                    }],
                },
            ));
            patches.push(impl_patch);
        }

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
                                let module = crate::autofix::module_map::new_type_module(
                                    &ref_type,
                                    &ref_type_def.full_path,
                                )
                                .0;
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
                                let module = crate::autofix::module_map::new_type_module(
                                    &ref_type,
                                    &ref_type_def.full_path,
                                )
                                .0;
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

            // The functions patch (the one-item commands' format): the
            // caller writes it next to the type patches
            result.functions_patch = Some(generate_add_functions_patch(
                type_name,
                &methods,
                &result.primary_module,
                version,
                type_def,
            ));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The methods of `impl T { .. }` in `source`, as the workspace index
    /// extracts them (generic `C: Into<W>` parameters resolved to `W`).
    fn methods(source: &str) -> BTreeMap<String, MethodDef> {
        let file: syn::File = syn::parse_file(source).expect("test source parses");
        let mut out = BTreeMap::new();
        for item in &file.items {
            if let syn::Item::Impl(block) = item {
                for impl_item in &block.items {
                    if let syn::ImplItem::Fn(f) = impl_item {
                        let m = super::super::type_index::extract_method_def(f, "T")
                            .expect("method extracts");
                        out.insert(m.name.clone(), m);
                    }
                }
            }
        }
        out
    }

    fn diffs(source: &str, api_class: &str) -> Vec<String> {
        let methods = methods(source);
        let refs: BTreeMap<String, &MethodDef> =
            methods.iter().map(|(k, v)| (k.clone(), v)).collect();
        let class: ClassData = serde_json::from_str(api_class).expect("test class parses");
        let names: Vec<String> = refs.keys().cloned().collect();
        find_function_differences(&names, &refs, &class)
            .into_iter()
            .flat_map(|d| d.differences)
            .collect()
    }

    /// `autofix add T.*` must not export a standard trait's method (the
    /// `default` constructors of 2026-10-01), must name a Rust `new` the way
    /// the API names constructors (`create`), and must not re-add a method the
    /// API already reaches under another name (`create` whose body calls `new`).
    #[test]
    fn the_add_candidates_skip_trait_impls_rename_new_and_skip_what_the_api_reaches() {
        let mut ms: Vec<MethodDef> = methods(
            r#"
            impl T {
                pub fn new(label: String) -> Self { todo!() }
                pub fn new_with_icon(label: String, icon: String) -> Self { todo!() }
                pub fn with_label(self, label: String) -> Self { todo!() }
                fn private_helper(&self) {}
            }
            impl T { pub fn default() -> Self { todo!() } }
        "#,
        )
        .into_values()
        .collect();
        for m in &mut ms {
            if m.name == "default" {
                m.from_trait = Some("Default".to_string());
            }
        }
        let refs: Vec<&MethodDef> = ms.iter().collect();

        let fresh = api_candidate_methods("T", &refs, "*", None, &|_: &str| true);
        let mut names: Vec<String> = fresh.iter().map(|m| api_name_of(m)).collect();
        names.sort();
        assert_eq!(names, vec!["create", "create_with_icon", "with_label"]);

        let class: ClassData = serde_json::from_str(
            r#"{"constructors": {"create": {"fn_args": [{"label": "String"}],
                "fn_body": "azul_layout::widgets::t::T::new(label)"}}}"#,
        )
        .expect("test class parses");
        let existing = api_candidate_methods("T", &refs, "*", Some(&class), &|_: &str| true);
        let mut names: Vec<String> = existing.iter().map(|m| api_name_of(m)).collect();
        names.sort();
        assert_eq!(names, vec!["create_with_icon", "with_label"], "`new` is reached by `create`");

        let one = api_candidate_methods("T", &refs, "with_label", Some(&class), &|_: &str| true);
        assert_eq!(one.len(), 1);
    }

    /// A type with its own `create` (the FFI-shaped constructor, taking a
    /// `FooVec`) and a Rust-convenience `new` (taking a `Vec`): the API entry
    /// `create` is the real `create`. Renaming `new` to `create` shadowed it
    /// (`RichBlock::create` went out as `new(kind, runs: Vec<RichRun>)`, a
    /// critical FFI error, 2026-10-02).
    #[test]
    fn a_real_create_is_not_shadowed_by_a_renamed_new() {
        let ms: Vec<MethodDef> = methods(
            r#"
            impl T {
                pub fn create(kind: Kind, runs: RunVec) -> Self { todo!() }
                pub fn new(kind: Kind, runs: Vec<Run>) -> Self { todo!() }
            }
        "#,
        )
        .into_values()
        .collect();
        let refs: Vec<&MethodDef> = ms.iter().collect();
        let fresh = api_candidate_methods("T", &refs, "*", None, &|_: &str| true);
        let rust: Vec<&str> = fresh.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(rust, vec!["create"], "only the real create: {rust:?}");
    }

    /// A repr(C) struct `name` at `azul_layout::<module_path>::<name>` with
    /// the methods of `source` (an `impl T { .. }`).
    fn type_def(name: &str, module_path: &str, source: &str) -> TypeDefinition {
        TypeDefinition {
            full_path: format!("azul_layout::{module_path}::{name}"),
            type_name: name.to_string(),
            file_path: std::path::PathBuf::from("/nonexistent/autofix_gone_test.rs"),
            module_path: module_path.to_string(),
            crate_name: "azul_layout".to_string(),
            kind: TypeDefKind::Struct {
                fields: IndexMap::new(),
                repr: Some("C".to_string()),
                repr_attr_count: 1,
                generic_params: Vec::new(),
                derives: Vec::new(),
                custom_impls: Vec::new(),
                is_tuple_struct: false,
            },
            source_code: String::new(),
            methods: methods(source).into_values().collect(),
        }
    }

    /// BLOCKS moved 42 setter pairs of the preset shells to OfficeShell
    /// (wave 5): api.json kept the 84 `DocumentShell.set_title_row` & co.,
    /// whose fn_body calls a method the type no longer has, and the scan did
    /// not notice - they were removed by hand. A function whose body calls
    /// `object.m(..)` or `<the class's path>::m(..)` is gone when the type
    /// has no method `m` (derive / std-trait methods like `clone` and
    /// `to_string` aside). Macro-made types, other bodies, and a class whose
    /// api.json path names a different type are not judged.
    #[test]
    fn an_api_function_whose_rust_method_is_gone_is_found() {
        let mut index = TypeIndex::new();
        index.add_type_for_test(type_def(
            "DocumentShell",
            "widgets::shells::document_shell",
            r#"impl T {
                pub fn create() -> Self { todo!() }
                pub fn office_shell(self) -> OfficeShell { todo!() }
            }"#,
        ));
        index.add_type_for_test(type_def("Other", "widgets::other", "impl T {}"));
        let mut vec_def = type_def("ShellPaneVec", "widgets::shells::office_shell", "impl T {}");
        vec_def.kind = TypeDefKind::MacroGenerated {
            source_macro: "impl_vec!".to_string(),
            base_type: "ShellPane".to_string(),
            kind: super::super::type_index::MacroGeneratedKind::Vec,
            derives: Vec::new(),
            implemented_traits: Vec::new(),
        };
        index.add_type_for_test(vec_def);

        let path = "azul_layout::widgets::shells::document_shell::DocumentShell";
        let api: ApiData = serde_json::from_value(serde_json::json!({
            "0.2.0": {"apiversion": 1, "git": "", "date": "", "api": {"shells": {"classes": {
                "DocumentShell": {
                    "external": path,
                    "constructors": {
                        "create": {"fn_body": format!("{path}::create()")},
                        "create_old": {"fn_body": format!("{path}::new_old()")}
                    },
                    "functions": {
                        "office_shell": {"fn_args": [{"self": "value"}], "fn_body": "object.office_shell()"},
                        "set_title_row": {"fn_args": [{"self": "refmut"}, {"title_row": "TitleRow"}],
                                          "fn_body": "object.set_title_row(title_row)"},
                        "with_theme": {"fn_args": [{"self": "value"}, {"theme": "UiTheme"}],
                                       "fn_body": "object.with_theme(theme)"},
                        "clone_shell": {"fn_args": [{"self": "ref"}], "fn_body": "object.clone()"},
                        "to_text": {"fn_args": [{"self": "ref"}], "fn_body": "object.to_string().into()"},
                        "helper": {"fn_args": [{"self": "ref"}],
                                   "fn_body": "azul_layout::widgets::shells::other::helper(object)"},
                        "title_len": {"fn_args": [{"self": "ref"}], "fn_body": "object.title.len()"}
                    }
                },
                "Other": {
                    "external": "azul_core::other::Other",
                    "functions": {"gone": {"fn_args": [{"self": "ref"}], "fn_body": "object.gone()"}}
                },
                "ShellPaneVec": {
                    "external": "azul_layout::widgets::shells::office_shell::ShellPaneVec",
                    "functions": {"len": {"fn_args": [{"self": "ref"}], "fn_body": "object.len()"}}
                }
            }}}}
        }))
        .expect("test api parses");

        let gone = gone_api_functions(&index, &api);
        let mut found: Vec<(String, String, String, bool)> = gone
            .iter()
            .map(|g| {
                (
                    format!("{}.{}", g.module, g.class),
                    g.api_name.clone(),
                    g.rust_name.clone(),
                    g.is_constructor,
                )
            })
            .collect();
        found.sort();
        let row = |api: &str, rust: &str, ctor: bool| {
            ("shells.DocumentShell".to_string(), api.to_string(), rust.to_string(), ctor)
        };
        assert_eq!(
            found,
            vec![
                row("create_old", "new_old", true),
                row("set_title_row", "set_title_row", false),
                row("with_theme", "with_theme", false),
            ]
        );
    }

    /// `autofix remove` took `module.Type.method` but not `module.Type`: a
    /// whole class (the ModuleSwitcher family, wave 5) needed `autofix
    /// difficult remove`, which clears the pending patches. One parser for
    /// both commands: `module.Type.fn`, `Type.fn`, `module.Type`, `Type`.
    #[test]
    fn a_remove_spec_names_a_function_or_a_whole_class() {
        let api: ApiData = serde_json::from_value(serde_json::json!({
            "0.2.0": {"apiversion": 1, "git": "", "date": "", "api": {
                "widgets": {"classes": {"ModuleSwitcher": {"functions": {"dom": {}}}}},
                "dom": {"classes": {"Dom": {}}}
            }}
        }))
        .expect("test api parses");
        let v = api.get_version("0.2.0").expect("version");
        let class = |module: &str, class: &str| RemoveTarget::Class {
            module: module.to_string(),
            class: class.to_string(),
        };
        let function = |module: &str, class: &str, name: &str| RemoveTarget::Function {
            module: module.to_string(),
            class: class.to_string(),
            name: name.to_string(),
        };

        assert_eq!(parse_remove_spec("widgets.ModuleSwitcher", v), Ok(class("widgets", "ModuleSwitcher")));
        assert_eq!(parse_remove_spec("ModuleSwitcher", v), Ok(class("widgets", "ModuleSwitcher")));
        assert_eq!(
            parse_remove_spec("ModuleSwitcher.dom", v),
            Ok(function("widgets", "ModuleSwitcher", "dom"))
        );
        assert_eq!(
            parse_remove_spec("widgets.ModuleSwitcher.dom", v),
            Ok(function("widgets", "ModuleSwitcher", "dom"))
        );
        assert!(parse_remove_spec("Nope.dom", v).is_err());
        assert!(parse_remove_spec("dom.ModuleSwitcher", v).is_err(), "not in that module");

        // the patch each one writes
        let removes_class = class("widgets", "ModuleSwitcher").patch("0.2.0");
        let cp = &removes_class.versions["0.2.0"].modules["widgets"].classes["ModuleSwitcher"];
        assert!(cp.is_removal());
        let removes_fn = function("widgets", "ModuleSwitcher", "dom").patch("0.2.0");
        let cp = &removes_fn.versions["0.2.0"].modules["widgets"].classes["ModuleSwitcher"];
        assert_eq!(cp.remove_functions.as_deref(), Some(&["dom".to_string()][..]));
        assert_eq!(class("widgets", "ModuleSwitcher").file_name(), "remove_moduleswitcher.patch.json");
    }

    /// `autofix add RichTextDoc.*` exported 29 Rust-only helpers (RichRun 7):
    /// slices, `Vec`s, tuples, `Option<&T>` and `&str` accessors, types
    /// without a C repr. THE wildcard rule: `Type.*` takes a method only when
    /// its whole signature crosses the FFI as written - no borrowed return,
    /// and every argument and the return type (as api.json spells it) a
    /// scalar, an api.json type or a type the FFI carries. A method named
    /// explicitly still goes out.
    #[test]
    fn the_wildcard_exports_only_methods_whose_signature_crosses_the_ffi() {
        let ms: Vec<MethodDef> = methods(
            r#"
            impl T {
                pub fn create() -> Self { todo!() }
                pub fn block_count(&self) -> usize { 0 }
                pub fn set_kind(&mut self, index: usize, kind: Kind) -> bool { true }
                pub fn preview(&self, title: &str) -> AzString { todo!() }
                pub fn link_at(&self, index: usize) -> Option<String> { None }
                pub fn same(&self, other: &T) -> bool { true }
                pub fn blocks(&self) -> &[Block] { todo!() }
                pub fn block(&self, index: usize) -> Option<&Block> { None }
                pub fn as_str(&self) -> &str { "" }
                pub fn link_str(&self) -> Option<&str> { None }
                pub fn take_blocks(&mut self) -> Vec<Block> { todo!() }
                pub fn put_blocks(&mut self, blocks: Vec<Block>) {}
                pub fn checklist(&self) -> (usize, usize) { (0, 0) }
                pub fn set_link(&mut self, url: Option<&str>) {}
                pub fn apply(&mut self, shortcut: &Shortcut) {}
                pub fn when(&self) -> Instant { todo!() }
            }
        "#,
        )
        .into_values()
        .collect();
        let refs: Vec<&MethodDef> = ms.iter().collect();
        let carries = |t: &str| matches!(t, "String" | "OptionString" | "Kind");

        let mut names: Vec<String> = api_candidate_methods("T", &refs, "*", None, &carries)
            .iter()
            .map(|m| api_name_of(m))
            .collect();
        names.sort();
        assert_eq!(
            names,
            vec!["block_count", "create", "link_at", "preview", "same", "set_kind"]
        );

        let named = api_candidate_methods("T", &refs, "take_blocks", None, &carries);
        assert_eq!(named.len(), 1, "a method named explicitly still goes out");
    }

    /// The api.json entry `autofix add` writes for method `name` of `source`.
    fn added(source: &str, name: &str) -> FunctionData {
        let ms = methods(source);
        method_to_function_data(&ms[name], "azul_layout::widgets::t::T")
    }

    fn returns_of(f: &FunctionData) -> Option<&str> {
        f.returns.as_ref().map(|r| r.r#type.as_str())
    }

    /// `autofix add RichRun.*` wrote `as_str -> str` (body `object.as_str()`)
    /// and `link_str -> Optionstr`: types the codegen spells `Azstr` /
    /// `AzOptionstr`, which do not exist (wave 5). A borrowed `str` crosses
    /// the FFI as an owned `String`, `Option<&str>` as `OptionString`, and
    /// so does an `Option` of a std `String` (`OptionString` converts from
    /// `Option<AzString>` only).
    #[test]
    fn a_borrowed_str_return_is_exported_as_an_owned_string() {
        let source = r#"
            impl T {
                pub fn as_str(&self) -> &str { todo!() }
                pub fn link_str(&self) -> Option<&str> { todo!() }
                pub fn label(&self) -> Option<String> { todo!() }
                pub fn title(&self) -> String { todo!() }
            }
        "#;
        let f = added(source, "as_str");
        assert_eq!(returns_of(&f), Some("String"));
        assert_eq!(f.fn_body.as_deref(), Some("azul_css::AzString::from(object.as_str())"));

        for name in ["link_str", "label"] {
            let f = added(source, name);
            assert_eq!(returns_of(&f), Some("OptionString"), "{name}");
            assert_eq!(
                f.fn_body.as_deref(),
                Some(format!("object.{name}().map(|s| azul_css::AzString::from(s)).into()").as_str()),
                "{name}"
            );
        }

        let f = added(source, "title");
        assert_eq!(returns_of(&f), Some("String"));
        assert_eq!(f.fn_body.as_deref(), Some("object.title().into()"));
    }

    /// The C side passes an `AzString` for an api.json `String` argument. A
    /// method taking a std `String` got it as it is (`object.set_text(text)`,
    /// RichRun.set_text, wave 5) and the dll did not build: the fn_body
    /// converts it. An `AzString` argument (also extracted as `String`) and
    /// a generic `S: Into<AzString>` take it as it is.
    #[test]
    fn a_std_string_argument_is_converted_in_the_fn_body() {
        let source = r#"
            impl T {
                pub fn set_text(&mut self, text: String) {}
                pub fn set_label(&mut self, label: AzString) {}
                pub fn with_title<S: Into<AzString>>(self, title: S) -> Self { todo!() }
                pub fn find(&self, needle: &String, from: usize) -> bool { todo!() }
                pub fn same(&self, other: &AzString) -> bool { todo!() }
            }
        "#;
        let cases = [
            ("set_text", "object.set_text(text.into_library_owned_string())"),
            ("set_label", "object.set_label(label)"),
            ("with_title", "object.with_title(title)"),
            ("find", "object.find(&needle.into_library_owned_string(), from)"),
            ("same", "object.same(&other)"),
        ];
        for (name, body) in cases {
            let f = added(source, name);
            assert_eq!(f.fn_body.as_deref(), Some(body), "{name}");
            let string_args = f
                .fn_args
                .iter()
                .flat_map(|a| a.iter())
                .filter(|(n, _)| *n != "self" && *n != "from")
                .all(|(_, ty)| ty == "String");
            assert!(string_args, "{name}: {:?}", f.fn_args);
        }
    }

    /// An api.json entry already passing an `AzString` to a std `String`
    /// parameter is reported (it is invisible to the type comparison:
    /// both sides say `String`).
    #[test]
    fn a_std_string_argument_passed_unconverted_is_reported() {
        let source = r#"impl T { pub fn set_text(&mut self, text: String) {} }"#;
        let entry = |body: &str| {
            format!(
                r#"{{"functions": {{"set_text": {{
                    "fn_args": [{{"self": "refmut"}}, {{"text": "String"}}],
                    "fn_body": "{body}"}}}}}}"#
            )
        };
        let found = diffs(source, &entry("object.set_text(text)"));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("`text`") && found[0].contains("std `String`"), "{found:?}");

        let converted = diffs(source, &entry("object.set_text(text.into_library_owned_string())"));
        assert!(converted.is_empty(), "{converted:?}");

        let az = diffs(
            r#"impl T { pub fn set_text(&mut self, text: AzString) {} }"#,
            &entry("object.set_text(text)"),
        );
        assert!(az.is_empty(), "an AzString parameter takes it as it is: {az:?}");
    }

    const SOURCE: &str = r#"
        impl T {
            pub fn add_component_library<R: Into<RegisterComponentLibraryFn>>(
                &mut self,
                name: AzString,
                register_fn: R,
            ) {}
        }
    "#;

    /// The bug class behind the 2026-09-19 callback audit: the source takes
    /// the context-carrying wrapper (`R: Into<RegisterComponentLibraryFn>`),
    /// api.json declared the bare function-pointer typedef, and nothing
    /// noticed - so no binding could hand a closure's context over.
    #[test]
    fn a_wrapper_argument_declared_as_its_typedef_is_reported() {
        let found = diffs(
            SOURCE,
            r#"{"functions": {"add_component_library": {
                "fn_args": [{"self": "refmut"}, {"name": "String"},
                            {"register_fn": "RegisterComponentLibraryFnType"}],
                "fn_body": "object.add_component_library(name, register_fn)"}}}"#,
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("register_fn") && found[0].contains("RegisterComponentLibraryFn`"));
    }

    #[test]
    fn a_matching_signature_is_not_reported() {
        let found = diffs(
            SOURCE,
            r#"{"functions": {"add_component_library": {
                "fn_args": [{"self": "refmut"}, {"name": "String"},
                            {"register_fn": "RegisterComponentLibraryFn"}],
                "fn_body": "object.add_component_library(name, register_fn)"}}}"#,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// Representations of the same argument: a borrowed `&T` passed by value
    /// (re-borrowed in the fn_body) and `Option<T>` spelled `OptionT`.
    #[test]
    fn equivalent_representations_are_not_reported() {
        let found = diffs(
            r#"
            impl T {
                pub fn get(&self, id: &DomId, span: Option<i32>) {}
            }
            "#,
            r#"{"functions": {"get": {
                "fn_args": [{"self": "ref"}, {"id": "DomId"}, {"span": "OptionI32"}],
                "fn_body": "object.get(&id, span.into())"}}}"#,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// The free function `name` of `source`, read for class `class` and
    /// called through `path`.
    fn free_fn(source: &str, name: &str, class: &str, path: &str) -> super::super::type_index::FreeFnDef {
        let file: syn::File = syn::parse_file(source).expect("test source parses");
        let method = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(f) if f.sig.ident == name => {
                    super::super::type_index::free_fn_method(f, class)
                }
                _ => None,
            })
            .expect("the free fn");
        super::super::type_index::FreeFnDef {
            path: path.to_string(),
            defined_in: String::new(),
            method,
        }
    }

    fn arg_list(f: &FunctionData) -> Vec<(String, String)> {
        f.fn_args
            .iter()
            .flat_map(|a| a.iter().map(|(n, t)| (n.clone(), t.clone())))
            .collect()
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
    }

    /// Xml.encode_text / encode_attribute, RawImage.from_text / draw_text
    /// needed a hand-written patch at the wave-6 integration: `autofix add`
    /// could not make a function whose body calls a FREE function. With
    /// `--fn <path>` the entry takes the free function's arguments; a first
    /// argument of the class's own type is the receiver, passed on in the
    /// codegen's receiver form (`raw_image`, the generated function's
    /// parameter - a bare `object` is not rewritten and did not compile).
    #[test]
    fn a_free_function_becomes_a_function_of_the_class() {
        let source = r#"
            pub fn text_image(text: AzString, style: TextRasterStyle) -> OptionRawImage { todo!() }
            pub fn draw_text(image: &mut RawImage, text: AzString, style: TextRasterStyle, x: f32, y: f32) -> bool { todo!() }
            pub fn encode_text(s: &str) -> String { todo!() }
            pub fn blank(width: u32, height: u32) -> RawImage { todo!() }
        "#;
        let path = "azul_layout::cpurender";

        let (f, ctor) = free_fn_entry(
            "RawImage",
            "from_text",
            &free_fn(source, "text_image", "RawImage", &format!("{path}::text_image")),
        );
        assert!(!ctor);
        assert_eq!(arg_list(&f), pairs(&[("text", "String"), ("style", "TextRasterStyle")]));
        assert_eq!(returns_of(&f), Some("OptionRawImage"));
        assert_eq!(f.fn_body.as_deref(), Some("azul_layout::cpurender::text_image(text, style)"));

        let (f, ctor) = free_fn_entry(
            "RawImage",
            "draw_text",
            &free_fn(source, "draw_text", "RawImage", &format!("{path}::draw_text")),
        );
        assert!(!ctor);
        assert_eq!(
            arg_list(&f),
            pairs(&[
                ("self", "refmut"),
                ("text", "String"),
                ("style", "TextRasterStyle"),
                ("x", "f32"),
                ("y", "f32")
            ])
        );
        assert_eq!(returns_of(&f), Some("bool"));
        assert_eq!(
            f.fn_body.as_deref(),
            Some("azul_layout::cpurender::draw_text(raw_image, text, style, x, y)")
        );

        let (f, _) = free_fn_entry(
            "Xml",
            "encode_text",
            &free_fn(source, "encode_text", "Xml", "azul_core::xml::html::encode_text"),
        );
        assert_eq!(arg_list(&f), pairs(&[("s", "String")]));
        assert_eq!(returns_of(&f), Some("String"));
        assert_eq!(
            f.fn_body.as_deref(),
            Some("azul_core::xml::html::encode_text(s.as_str()).into()")
        );

        // a `create*` name returning the class is a constructor
        let blank = free_fn(source, "blank", "RawImage", &format!("{path}::blank"));
        let (f, ctor) = free_fn_entry("RawImage", "create_blank", &blank);
        assert!(ctor);
        assert_eq!(f.fn_body.as_deref(), Some("azul_layout::cpurender::blank(width, height)"));
        let (_, ctor) = free_fn_entry("RawImage", "blank", &blank);
        assert!(!ctor, "only a create* name is a constructor");
    }

    /// A callback receives its info by value (CallbackType's fn_args), and
    /// the info is a set of pointers into the window state, so it crosses by
    /// value and the body re-borrows a mutable copy - the pattern of
    /// ProgressBar.update_progress and TextInput.set_text_in. `autofix add`
    /// wrote `*mut CallbackInfo` and the scan called the existing entries
    /// drifted.
    #[test]
    fn a_callback_info_argument_crosses_by_value_and_is_re_borrowed() {
        let source = r#"
            impl T {
                pub fn set_text_in(info: &mut CallbackInfo, container: DomNodeId, text: AzString) {}
                pub fn update_progress(callback_info: &mut CallbackInfo, node_id: DomNodeId, percent_done: f32) -> bool { true }
            }
        "#;
        let ms = methods(source);
        let f = method_to_function_data(&ms["set_text_in"], "azul_layout::widgets::text_input::TextInput");
        assert_eq!(
            arg_list(&f),
            pairs(&[("info", "CallbackInfo"), ("container", "DomNodeId"), ("text", "String")])
        );
        assert_eq!(
            f.fn_body.as_deref(),
            Some(
                "{ let mut info = info; \
                 azul_layout::widgets::text_input::TextInput::set_text_in(&mut info, container, text) }"
            )
        );

        let found = diffs(
            source,
            r#"{"functions": {"update_progress": {
                "fn_args": [{"callback_info": "CallbackInfo"}, {"node_id": "DomNodeId"},
                            {"percent_done": "f32"}],
                "returns": {"type": "bool"},
                "fn_body": "{ let mut callback_info = callback_info; azul_layout::widgets::progressbar::ProgressBar::update_progress(&mut callback_info, node_id, percent_done) }"}}}"#,
        );
        assert!(found.is_empty(), "{found:?}");
    }

    /// The only body `autofix add` wrote with a bare `object` - which the
    /// codegen does not rewrite - was a `destroy*` method's
    /// `core::mem::drop(object)`, and it did not even call the method. A
    /// `destroy*` method is called like any other.
    #[test]
    fn a_destroy_method_is_called_and_no_body_passes_a_bare_object() {
        let source = r#"
            impl T {
                pub fn destroy(&mut self) {}
                pub fn destroy_child(&mut self, index: usize) -> bool { true }
            }
        "#;
        assert_eq!(added(source, "destroy").fn_body.as_deref(), Some("object.destroy()"));
        assert_eq!(
            added(source, "destroy_child").fn_body.as_deref(),
            Some("object.destroy_child(index)")
        );
    }
}

