use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

use crate::{
    api::{ApiData, ClassData, EnumVariantData, FieldData, FunctionData, RefKind, VersionData},
    utils::{
        analyze::{
            analyze_type, enum_is_union, get_class, is_primitive_arg,
            search_for_class_by_class_name,
        },
        string::snake_case_to_lower_camel,
    },
};

use super::memtest::{
    build_type_to_external_map, generate_generated_rs, generate_transmuted_fn_body,
    MemtestConfig, TypeReplacements,
};

const PREFIX: &str = "Az";

/// Capitalize the first character of a string (e.g., "u8" → "U8", "f32" → "F32")
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().to_string() + chars.as_str(),
    }
}

// Recursive types - they cause "infinite size" errors in PyO3
// These would need Box<> indirection which the C-API doesn't have
const RECURSIVE_TYPES: &[&str] = &[
    "XmlNode",
    "XmlNodeChild",
    "XmlNodeChildVec",
    "Xml",               // Uses XmlNodeChildVec which is recursive
    "ResultXmlXmlError", // Uses Xml which is skipped
];

// // property-based type classification
// instead of hardcoding type names, we detect properties from api.json
//
/// Check if a type is a callback typedef (function pointer type)
/// These have `callback_typedef` field in api.json
fn is_callback_typedef(class_data: &ClassData) -> bool {
    class_data.callback_typedef.is_some()
}

// TODO: These VecRef/Refstr types need proper trampolines to convert Python lists/strings
// to C-API slice references. For now, we hardcode them to be excluded from Python bindings.
const VECREF_TYPES: &[&str] = &[
    // VecRef types (immutable slices)
    "GLuintVecRef",
    "GLintVecRef",
    "GLenumVecRef",
    "U8VecRef",
    "U16VecRef",
    "U32VecRef",
    "I32VecRef",
    "F32VecRef",
    "Refstr",
    "RefstrVecRef",
    "TessellatedSvgNodeVecRef",
    "TessellatedColoredSvgNodeVecRef",
    "OptionU8VecRef",
    "OptionI16VecRef",
    "OptionI32VecRef",
    "OptionF32VecRef",
    "OptionFloatVecRef",
    // VecRefMut types (mutable slices)
    "GLintVecRefMut",
    "GLint64VecRefMut",
    "GLbooleanVecRefMut",
    "GLfloatVecRefMut",
    "U8VecRefMut",
    "F32VecRefMut",
];

/// Check if a type name is a callback type that needs Python→Rust routing
/// Returns the callback wrapper struct name if it is, None otherwise
fn get_callback_info_for_type(type_name: &str, version_data: &VersionData) -> Option<CallbackTypeInfo> {
    let (_, base_type, _) = analyze_type(type_name);
    
    // Direct callback types (e.g., "Callback", "IFrameCallback")
    // Look up in api.json to find the callback_typedef
    if let Some((module, _)) = search_for_class_by_class_name(version_data, &base_type) {
        if let Some(class_data) = get_class(version_data, module, &base_type) {
            // Check if this type has struct_fields that contain a callback
            // e.g., Callback has { cb: CallbackType, data: RefAny }
            if let Some(ref struct_fields) = class_data.struct_fields {
                let mut has_callback = false;
                let mut has_refany = false;
                let mut callback_field_type = String::new();
                
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        if field_data.r#type == "RefAny" {
                            has_refany = true;
                        }
                        // Check if this field is a callback typedef
                        if let Some((mod2, _)) = search_for_class_by_class_name(version_data, &field_data.r#type) {
                            if let Some(field_class) = get_class(version_data, mod2, &field_data.r#type) {
                                if field_class.callback_typedef.is_some() {
                                    has_callback = true;
                                    callback_field_type = field_data.r#type.clone();
                                }
                            }
                        }
                    }
                }
                
                if has_callback && has_refany {
                    // This is a callback+data pair struct
                    let wrapper_name = format!("{}Wrapper", callback_field_type.replace("Type", ""));
                    let trampoline_name = format!("invoke_py_{}", to_snake_case(&callback_field_type.replace("Type", "")));
                    return Some(CallbackTypeInfo {
                        original_type: base_type.clone(),
                        callback_type: callback_field_type,
                        wrapper_name,
                        trampoline_name,
                    });
                }
            }
        }
    }
    
    None
}

/// Information about a callback type for Python routing
#[derive(Debug, Clone)]
struct CallbackTypeInfo {
    /// Original type name (e.g., "Callback", "IFrameCallback")
    original_type: String,
    /// The inner callback typedef (e.g., "CallbackType")
    callback_type: String,
    /// Wrapper struct name for holding Python objects (e.g., "CallbackWrapper")
    wrapper_name: String,
    /// Trampoline function name (e.g., "invoke_py_callback")
    trampoline_name: String,
}

/// Check if a type is RefAny - needs Python PyObject wrapping
fn is_refany_type(type_name: &str) -> bool {
    let (_, base_type, _) = analyze_type(type_name);
    base_type == "RefAny"
}

/// Check if a type name is a callback_typedef (raw function pointer type like LayoutCallbackType)
/// These are different from Callback structs which pair a function pointer with RefAny data
fn is_callback_typedef_by_name(type_name: &str, version_data: &VersionData) -> bool {
    let (_, base_type, _) = analyze_type(type_name);
    
    if let Some((module, _)) = search_for_class_by_class_name(version_data, &base_type) {
        if let Some(class_data) = get_class(version_data, module, &base_type) {
            return class_data.callback_typedef.is_some();
        }
    }
    false
}

/// Get the trampoline name for a callback_typedef type
fn get_callback_typedef_trampoline(type_name: &str) -> String {
    // LayoutCallbackType → invoke_py_layout_callback
    // CallbackType → invoke_py_callback
    let base = type_name.replace("Type", "");
    format!("invoke_py_{}", to_snake_case(&base))
}

/// Check if a type is a VecRef type (raw pointer slice wrapper)
/// These have `vec_ref_element_type` field in api.json or are in the hardcoded list
fn is_vec_ref_type(class_data: &ClassData) -> bool {
    class_data.vec_ref_element_type.is_some()
}

/// Check if a type name is a VecRef type by name
fn is_vec_ref_type_by_name(class_name: &str) -> bool {
    VECREF_TYPES.contains(&class_name)
}

/// Check if a type is a boxed object (heap-allocated pointer wrapper)
/// These have `is_boxed_object: true` in api.json
fn is_boxed_object(class_data: &ClassData) -> bool {
    class_data.is_boxed_object
}

/// Check if a type is a generic template (has generic_params)
/// These cannot be instantiated directly in Python
fn is_generic_template(class_data: &ClassData) -> bool {
    class_data.generic_params.is_some()
}

/// Check if a type is a type alias for a primitive or c_void
/// These are skipped because they don't need Python wrappers
fn is_primitive_or_void_alias(class_data: &ClassData) -> bool {
    if let Some(ref type_alias) = class_data.type_alias {
        let target = type_alias.target.as_str();
        matches!(
            target,
            "c_void"
                | "usize"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "f32"
                | "f64"
                | "bool"
        )
    } else {
        false
    }
}

/// Check if a type is a type alias for CssPropertyValue (CSS value types)
/// These need special handling - we instantiate the generic
fn is_css_property_value_alias(class_data: &ClassData) -> bool {
    if let Some(ref type_alias) = class_data.type_alias {
        type_alias.target == "CssPropertyValue" && !type_alias.generic_args.is_empty()
    } else {
        false
    }
}

/// Check if a type is a simple type alias (non-generic, like XmlTagName = String)
fn is_simple_type_alias(class_data: &ClassData) -> bool {
    if let Some(ref type_alias) = class_data.type_alias {
        type_alias.generic_args.is_empty()
            && !matches!(type_alias.target.as_str(), "c_void" | "usize")
    } else {
        false
    }
}

/// Check if a type has fields containing raw pointers
fn has_pointer_fields(class_data: &ClassData) -> bool {
    if let Some(ref struct_fields) = class_data.struct_fields {
        for field_map in struct_fields {
            for (_, field_data) in field_map {
                // Check ref_kind for pointer types
                if matches!(field_data.ref_kind, RefKind::ConstPtr | RefKind::MutPtr) {
                    return true;
                }
                // Also check the type string for legacy compatibility
                if field_data.r#type.contains("*const")
                    || field_data.r#type.contains("*mut")
                    || field_data.r#type.contains('*')
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if a type can have an auto-generated `new` constructor from Python.
/// Types with pointers, Box, Ref-types, etc. cannot be constructed from Python
/// because Python has no concept of raw pointers or references.
///
/// NOTE: This only affects the auto-generated `new(field1, field2, ...)` constructor.
/// Other constructors from api.json (like `Foo::bar(x, y) -> Foo`) are still generated.
///
/// TODO: In the future, we may want to generate "trampoline" constructors that
/// convert Python types to the required Rust pointer types. For now, we skip
/// these types entirely for the default constructor.
fn can_have_python_constructor(class_data: &ClassData, version_data: &VersionData) -> bool {
    if let Some(ref struct_fields) = class_data.struct_fields {
        for field_map in struct_fields {
            for (_, field_data) in field_map {
                // Skip if ref_kind indicates a pointer
                if matches!(field_data.ref_kind, RefKind::ConstPtr | RefKind::MutPtr) {
                    return false;
                }

                // Skip if type contains pointer syntax
                let type_str = &field_data.r#type;
                if type_str.contains("*const")
                    || type_str.contains("*mut")
                    || type_str.contains('*')
                {
                    return false;
                }

                // Skip if type is a Ref/RefMut type (borrowing types not constructable from Python)
                if type_str.ends_with("Ref")
                    || type_str.ends_with("RefMut")
                    || type_str.contains("VecRef")
                    || type_str.contains("Refstr")
                {
                    return false;
                }

                // Skip if type is c_void (opaque pointer)
                if type_str == "c_void" {
                    return false;
                }

                // Skip if type name ends with "CallbackType" (function pointer types)
                // These are stored as usize but are actually function pointers
                if type_str.ends_with("CallbackType") {
                    return false;
                }
                
                // Skip if type is RefAny (requires special handling)
                if type_str == "RefAny" {
                    return false;
                }

                // Skip if type is a callback type (contains function pointers)
                if let Some((module, _)) = search_for_class_by_class_name(version_data, type_str) {
                    if let Some(field_class) = get_class(version_data, module, type_str) {
                        if is_callback_typedef(field_class) {
                            return false;
                        }
                    }
                }
                
                // Skip if type is a Vec type (requires complex conversion)
                // Vec types have suffix "Vec" but are not primitives
                if type_str.ends_with("Vec") && type_str.len() > 3 && !type_str.contains("Destructor") {
                    return false;
                }
            }
        }
    }
    true
}

/// Check if a type has Send trait (explicit derive or custom_impl)
fn has_send_trait(class_data: &ClassData) -> bool {
    if let Some(ref derive) = class_data.derive {
        if derive.iter().any(|d| d == "Send") {
            return true;
        }
    }
    if let Some(ref custom_impls) = class_data.custom_impls {
        if custom_impls.iter().any(|d| d == "Send") {
            return true;
        }
    }
    false
}

/// Check if a type has Sync trait
fn has_sync_trait(class_data: &ClassData) -> bool {
    if let Some(ref derive) = class_data.derive {
        if derive.iter().any(|d| d == "Sync") {
            return true;
        }
    }
    if let Some(ref custom_impls) = class_data.custom_impls {
        if custom_impls.iter().any(|d| d == "Sync") {
            return true;
        }
    }
    false
}

/// Check if a type is a primitive type or a type alias that resolves to a primitive.
/// For example, GLint -> i32, GLuint -> u32, GLfloat -> f32, etc.
fn is_primitive_or_alias_to_primitive(type_name: &str, version_data: &VersionData) -> bool {
    // First check if it's directly a primitive
    if is_primitive_arg(type_name) {
        return true;
    }
    
    // Then check if it's a type alias to a primitive
    if let Some((module, _)) = search_for_class_by_class_name(version_data, type_name) {
        if let Some(class_data) = get_class(version_data, module, type_name) {
            if let Some(ref type_alias) = class_data.type_alias {
                // Recursively check the target type
                return is_primitive_or_alias_to_primitive(&type_alias.target, version_data);
            }
        }
    }
    
    false
}

/// Check if a class has any mutable methods (&mut self)
/// In PyO3 0.27+, unsendable implies frozen which forbids &mut self methods
fn class_has_mutable_methods(class_data: &ClassData) -> bool {
    if let Some(functions) = &class_data.functions {
        for func in functions.values() {
            if func.fn_args.iter().any(|arg| {
                arg.get("self").map(|s| s.contains("mut")).unwrap_or(false)
            }) {
                return true;
            }
        }
    }
    false
}

/// Check if a type needs #[pyclass(unsendable)]
/// All Azul types need this because they contain nested types with raw pointers
/// (AzString contains AzU8Vec which has *const u8, etc.)
/// In PyO3 0.27+, unsendable implies frozen - we must skip &mut self methods
fn needs_unsendable(_class_data: &ClassData) -> bool {
    // All types must be unsendable because they transitively contain pointers
    // The &mut self methods will be skipped in method generation
    true
}

/// Check if a struct is a callback+data pair (has a callback field + data: RefAny field)
/// These structs need special Python wrappers that accept PyObject for both fields
/// Returns Some((callback_field_name, callback_type, callback_info_type, return_type)) if it's a
/// pair
fn is_callback_data_pair_struct(
    class_data: &ClassData,
    version_data: &VersionData,
) -> Option<(String, String, CallbackSignature)> {
    let struct_fields = class_data.struct_fields.as_ref()?;

    // Collect all fields from all field maps
    let mut all_fields: Vec<(&str, &str)> = Vec::new();
    for field_map in struct_fields {
        for (name, field_data) in field_map {
            all_fields.push((name.as_str(), field_data.r#type.as_str()));
        }
    }

    // Check if we have both a callback-like field and a RefAny data field
    let mut callback_field: Option<(&str, &str)> = None;
    let mut has_refany = false;

    for (name, ty) in &all_fields {
        if ty.contains("Callback") && !ty.contains("Destructor") {
            callback_field = Some((*name, *ty));
        }
        if *ty == "RefAny" {
            has_refany = true;
        }
    }

    // Must have both callback and RefAny
    let (cb_field_name, cb_type) = callback_field?;
    if !has_refany {
        return None;
    }

    // Get the callback signature from the CallbackType definition
    let callback_sig = get_callback_signature(cb_type, version_data)?;

    Some((cb_field_name.to_string(), cb_type.to_string(), callback_sig))
}

/// Information about a callback's function signature
#[derive(Clone, Debug)]
pub struct CallbackSignature {
    /// The inner callback type (e.g., "IFrameCallbackType")
    pub callback_type: String,
    /// The info type passed to the callback (e.g., "CallbackInfo")
    pub info_type: String,
    /// Full external path for the info type (e.g., "azul_layout::callbacks::CallbackInfo")
    pub info_type_external: String,
    /// Additional arguments beyond RefAny and info (e.g., "&CheckBoxState")
    /// Tuple: (name, type_name, ref_kind, external_path)
    pub extra_args: Vec<(String, String, RefKind, String)>,
    /// Return type (e.g., "Update")
    pub return_type: String,
    /// Full external path for the return type (e.g., "azul_core::callbacks::Update")
    pub return_type_external: String,
}

/// Generate trampolines for all callback_typedef types in api.json
/// This generates:
/// 1. Wrapper types that hold Py<PyAny> objects (stored in RefAny)
/// 2. extern "C" trampoline functions that bridge Python and Rust
/// Generate all Python-specific code for bindings integration.
/// This includes:
/// - Helper functions for type conversion (AzString <-> String, AzU8Vec <-> bytes)
/// - From/Into implementations for string/bytes types
/// - PyO3 conversion traits (FromPyObject, IntoPyObject)
/// - Callback wrapper types (AppDataTy, etc.)
/// - Callback trampolines (extern "C" fn invoke_py_*)
///
/// NOTE: This generates the PREFIX part only. The App class is generated separately
/// AFTER wrapper types are defined, since App uses AzWindowCreateOptions which is a wrapper.
fn generate_python_patches_prefix(version_data: &VersionData, prefix: &str) -> String {
    let mut code = String::new();
    
    code.push_str("// ============================================================================\r\n");
    code.push_str("// AUTO-GENERATED PYTHON PATCHES\r\n");
    code.push_str("// Generated from api.json - Python-specific integration code\r\n");
    code.push_str("// ============================================================================\r\n\r\n");

    // --- PyO3 imports ---
    code.push_str("use pyo3::gc::{PyVisit, PyTraverseError};\r\n");
    code.push_str("use pyo3::conversion::IntoPyObject;\r\n");
    code.push_str("use pyo3::Borrowed;\r\n");
    code.push_str("\r\n");
    
    // --- Type aliases for C-API types used in patches ---
    // These types are skipped from Python wrapper generation (MANUAL_TYPES),
    // so we use the C-API types directly via full path aliases.
    code.push_str("// Type aliases for C-API types that have custom Python integration\r\n");
    code.push_str("type AzString = __dll_api_inner::dll::AzString;\r\n");
    code.push_str("type AzU8Vec = __dll_api_inner::dll::AzU8Vec;\r\n");
    code.push_str("type AzStringVec = __dll_api_inner::dll::AzStringVec;\r\n");
    code.push_str("type AzU8VecDestructor = __dll_api_inner::dll::AzU8VecDestructor;\r\n");
    code.push_str("type AzStringVecDestructor = __dll_api_inner::dll::AzStringVecDestructor;\r\n");
    code.push_str("type AzRefAny = __dll_api_inner::dll::AzRefAny;\r\n");
    // GL Vec types for return value conversion
    code.push_str("type AzGLuintVec = __dll_api_inner::dll::AzGLuintVec;\r\n");
    code.push_str("type AzGLintVec = __dll_api_inner::dll::AzGLintVec;\r\n");
    // Note: GLint64Vec does not exist in api.json, only GLint64VecRefMut
    // Types used in enum variants that need full paths (C-API types, not Python wrappers)
    code.push_str("type AzStringMenuItem = __dll_api_inner::dll::AzStringMenuItem;\r\n");
    code.push_str("type AzInstantPtr = __dll_api_inner::dll::AzInstantPtr;\r\n");
    // FFI module alias for trampolines (cleaner than __dll_api_inner::dll)
    code.push_str("\r\n// FFI module alias for cleaner trampoline code\r\n");
    code.push_str("mod ffi { pub use super::__dll_api_inner::dll::*; }\r\n");
    code.push_str("\r\n");
    
    // --- Helper functions ---
    code.push_str("// --- Helper functions for type conversion ---\r\n\r\n");
    
    code.push_str(r#"fn az_string_to_py_string(input: AzString) -> String {
    let bytes = unsafe {
        core::slice::from_raw_parts(input.vec.ptr, input.vec.len)
    };
    String::from_utf8_lossy(bytes).into_owned()
}

fn az_vecu8_to_py_vecu8(input: AzU8Vec) -> Vec<u8> {
    let slice = unsafe {
        core::slice::from_raw_parts(input.ptr, input.len)
    };
    slice.to_vec()
}

fn az_stringvec_to_py_stringvec(input: AzStringVec) -> Vec<String> {
    let slice = unsafe {
        core::slice::from_raw_parts(input.ptr, input.len)
    };
    slice.iter().map(|s| az_string_to_py_string(s.clone())).collect()
}

fn az_gluintvec_to_py_vecu32(input: AzGLuintVec) -> Vec<u32> {
    let slice = unsafe {
        core::slice::from_raw_parts(input.ptr, input.len)
    };
    slice.to_vec()
}

fn az_glintvec_to_py_veci32(input: AzGLintVec) -> Vec<i32> {
    let slice = unsafe {
        core::slice::from_raw_parts(input.ptr, input.len)
    };
    slice.to_vec()
}

"#);

    // --- From/Into implementations ---
    code.push_str("// --- From/Into implementations for string/bytes types ---\r\n\r\n");
    
    code.push_str(r#"impl From<String> for AzString {
    fn from(s: String) -> AzString {
        let bytes = s.into_bytes();
        let ptr = bytes.as_ptr();
        let len = bytes.len();
        let cap = bytes.capacity();
        core::mem::forget(bytes);
        
        AzString {
            vec: AzU8Vec {
                ptr,
                len,
                cap,
                destructor: AzU8VecDestructor::DefaultRust,
            }
        }
    }
}

impl From<AzString> for String {
    fn from(s: AzString) -> String {
        az_string_to_py_string(s)
    }
}

impl From<AzU8Vec> for Vec<u8> {
    fn from(input: AzU8Vec) -> Vec<u8> {
        az_vecu8_to_py_vecu8(input)
    }
}

impl From<Vec<u8>> for AzU8Vec {
    fn from(input: Vec<u8>) -> AzU8Vec {
        let ptr = input.as_ptr();
        let len = input.len();
        let cap = input.capacity();
        core::mem::forget(input);
        
        AzU8Vec {
            ptr,
            len,
            cap,
            destructor: AzU8VecDestructor::DefaultRust,
        }
    }
}

"#);

    // --- PyO3 conversion traits ---
    code.push_str("// --- PyO3 conversion traits (FromPyObject, IntoPyObject) ---\r\n\r\n");
    
    code.push_str(r#"impl FromPyObject<'_, '_> for AzString {
    type Error = PyErr;
    
    fn extract(ob: Borrowed<'_, '_, PyAny>) -> Result<Self, Self::Error> {
        let s: String = ob.extract()?;
        Ok(s.into())
    }
}

impl<'py> IntoPyObject<'py> for AzString {
    type Target = PyString;
    type Output = Bound<'py, PyString>;
    type Error = std::convert::Infallible;
    
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let s: String = self.into();
        Ok(PyString::new(py, &s))
    }
}

impl FromPyObject<'_, '_> for AzU8Vec {
    type Error = PyErr;
    
    fn extract(ob: Borrowed<'_, '_, PyAny>) -> Result<Self, Self::Error> {
        let v: Vec<u8> = ob.extract()?;
        Ok(v.into())
    }
}

impl<'py> IntoPyObject<'py> for AzU8Vec {
    type Target = PyBytes;
    type Output = Bound<'py, PyBytes>;
    type Error = std::convert::Infallible;
    
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let slice = unsafe { core::slice::from_raw_parts(self.ptr, self.len) };
        Ok(PyBytes::new(py, slice))
    }
}

impl FromPyObject<'_, '_> for AzStringVec {
    type Error = PyErr;
    
    fn extract(ob: Borrowed<'_, '_, PyAny>) -> Result<Self, Self::Error> {
        let v: Vec<String> = ob.extract()?;
        let az_strings: Vec<AzString> = v.into_iter().map(|s| s.into()).collect();
        Ok(AzStringVec::from_vec(az_strings))
    }
}

impl<'py> IntoPyObject<'py> for AzStringVec {
    type Target = PyList;
    type Output = Bound<'py, PyList>;
    type Error = PyErr;
    
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let strings: Vec<String> = self.into_rust_vec();
        PyList::new(py, strings)
    }
}

impl AzStringVec {
    fn from_vec(v: Vec<AzString>) -> Self {
        let ptr = v.as_ptr();
        let len = v.len();
        let cap = v.capacity();
        core::mem::forget(v);
        
        AzStringVec {
            ptr,
            len,
            cap,
            destructor: AzStringVecDestructor::DefaultRust,
        }
    }
    
    fn into_rust_vec(self) -> Vec<String> {
        let slice = unsafe { core::slice::from_raw_parts(self.ptr, self.len) };
        slice.iter().map(|s| {
            let bytes = unsafe { core::slice::from_raw_parts(s.vec.ptr, s.vec.len) };
            String::from_utf8_lossy(bytes).into_owned()
        }).collect()
    }
}

"#);

    // --- Callback wrapper types and trampolines ---
    code.push_str(&generate_callback_trampolines(version_data, prefix));
    
    // NOTE: App class is NOT generated here!
    // It's generated separately after wrapper types, since it uses AzWindowCreateOptions.
    // See generate_app_class() which is called in generate_python_api().
    
    code
}

fn generate_callback_trampolines(version_data: &VersionData, prefix: &str) -> String {
    let mut code = String::new();
    
    code.push_str("// --- Python Wrapper Types for RefAny ---\r\n\r\n");
    
    // Special wrapper for App (holds both data and layout callback)
    code.push_str(r#"/// Holds Python objects for the main App (data + layout callback)
/// Layout callback is stored in WindowState.layout_callback,
/// but for App we store both data and callback together.
#[repr(C)]
pub struct AppDataTy {
    pub _py_app_data: Option<Py<PyAny>>,
    pub _py_layout_callback: Option<Py<PyAny>>,
}

/// Generic wrapper for Python user data stored in RefAny
/// Used by all callbacks (except layout) to wrap Python objects.
/// The callable is retrieved separately via info.get_callable().
#[repr(C)]
pub struct PyDataWrapper {
    pub _py_data: Option<Py<PyAny>>,
}

/// Wrapper for Python callable stored in the callback's `callable` field.
/// Retrieved via info.get_callable() in trampolines.
#[repr(C)]
pub struct PyCallableWrapper {
    pub _py_callable: Option<Py<PyAny>>,
}

/// Generic wrapper for any Python object stored in RefAny.
/// Used when we just need to wrap a Py<PyAny> without specific semantics.
#[repr(C)]
pub struct PyObjectWrapper {
    pub py_obj: Py<PyAny>,
}

"#);
    
    // Collect all callback_typedef types (excluding destructor types)
    let mut callback_types: Vec<(String, crate::api::CallbackDefinition)> = Vec::new();
    
    for (_module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            if let Some(ref callback_def) = class_data.callback_typedef {
                // Skip destructor types - they're not Python callbacks
                if class_name.ends_with("DestructorType") {
                    continue;
                }
                // Skip clone callback types
                if class_name.ends_with("CloneCallbackType") {
                    continue;
                }
                // Skip destructor callbacks (don't start with RefAny)
                if class_name.ends_with("DestructorCallbackType") {
                    continue;
                }
                // Skip callbacks that don't have RefAny as first argument (not user callbacks)
                if callback_def.fn_args.is_empty() || callback_def.fn_args[0].r#type != "RefAny" {
                    continue;
                }
                // Skip callbacks that have pointer arguments (not supported in Python yet)
                let has_pointer_args = callback_def.fn_args.iter().any(|arg| {
                    arg.r#type.starts_with("*mut ") || arg.r#type.starts_with("*const ")
                });
                if has_pointer_args {
                    continue;
                }
                callback_types.push((class_name.clone(), callback_def.clone()));
            }
        }
    }
    
    // No per-callback wrapper types needed anymore!
    // We use PyDataWrapper for data and PyCallableWrapper for callables.
    
    // Generate trampoline functions
    code.push_str("// --- Callback Trampolines (extern \"C\" functions) ---\r\n\r\n");
    
    // Special trampoline for layout callback (uses AppDataTy)
    // Since callback_typedef_use_external is enabled, we must use EXTERNAL types in signature
    code.push_str(&format!(r#"/// Trampoline for layout callbacks - uses AppDataTy wrapper
/// Layout callback is special: both data and callback are in AppDataTy
/// Signature uses external types to match azul_core::callbacks::LayoutCallbackType
extern "C" fn invoke_py_layout_callback(
    app_data: azul_core::refany::RefAny,
    info: azul_core::callbacks::LayoutCallbackInfo
) -> azul_core::styled_dom::StyledDom {{
    let default = azul_core::styled_dom::StyledDom::default();
    
    let mut app_data_core = app_data;
    
    let app = match app_data_core.downcast_ref::<AppDataTy>() {{
        Some(s) => s,
        None => return default,
    }};

    let py_callback = match app._py_layout_callback.as_ref() {{
        Some(s) => s,
        None => return default,
    }};

    let py_data = match app._py_app_data.as_ref() {{
        Some(s) => s,
        None => return default,
    }};

    Python::attach(|py| {{
        // Transmute external type to Python wrapper struct
        let info_py: {prefix}LayoutCallbackInfo = unsafe {{ mem::transmute(info) }};
        
        match py_callback.call1(py, (py_data.clone_ref(py), info_py)) {{
            Ok(result) => {{
                match result.extract::<{prefix}StyledDom>(py) {{
                    Ok(styled_dom) => unsafe {{ mem::transmute(styled_dom) }},
                    Err(e) => {{
                        #[cfg(feature = "logging")]
                        log::error!("Layout callback must return StyledDom: {{:?}}", e);
                        default
                    }}
                }}
            }}
            Err(e) => {{
                #[cfg(feature = "logging")]
                log::error!("Exception in layout callback: {{:?}}", e);
                default
            }}
        }}
    }})
}}

"#, prefix = prefix));
    
    // Generate trampolines for other callbacks
    // These use PyDataWrapper for data and get_callable() for the callable
    // IMPORTANT: Signatures must use EXTERNAL types (azul_core::...) to match CallbackType definitions
    for (callback_name, callback_def) in &callback_types {
        // Skip LayoutCallbackType - handled above
        if callback_name == "LayoutCallbackType" {
            continue;
        }
        
        let trampoline_name = format!("invoke_py_{}", to_snake_case(&callback_name.replace("Type", "")));
        
        // Parse function arguments - use EXTERNAL types for signature
        let mut args_sig = String::new();
        let mut args_sig_ffi = String::new();  // For converting to FFI inside function
        let mut first_arg = true;
        let mut info_type = String::new();
        let mut info_arg_name = String::new();
        let mut extra_args: Vec<(String, String)> = Vec::new();
        
        for (i, arg) in callback_def.fn_args.iter().enumerate() {
            let arg_name = if i == 0 { 
                "data".to_string() 
            } else if i == 1 { 
                "info".to_string() 
            } else { 
                format!("arg{}", i) 
            };
            
            let type_str = &arg.r#type;
            // Use EXTERNAL types for signature (to match the callback type definition)
            let arg_type_external = if is_primitive_arg(type_str) {
                arg.r#type.clone()
            } else {
                get_type_external_path(&arg.r#type, version_data)
            };
            
            // FFI type for internal use
            let arg_type_ffi = if is_primitive_arg(type_str) {
                arg.r#type.clone()
            } else if arg.r#type == "RefAny" {
                "ffi::AzRefAny".to_string()
            } else {
                format!("ffi::{}{}", prefix, arg.r#type)
            };
            
            if i == 1 {
                info_type = arg.r#type.clone();
                info_arg_name = arg_name.clone();
            }
            if i >= 2 {
                extra_args.push((arg_name.clone(), arg.r#type.clone()));
            }
            
            if !first_arg {
                args_sig.push_str(",\r\n    ");
            }
            args_sig.push_str(&format!("{}: {}", arg_name, arg_type_external));
            first_arg = false;
        }
        
        // Return type - use EXTERNAL type for signature
        let return_type = callback_def.returns.as_ref()
            .map(|r| r.r#type.clone())
            .unwrap_or_else(|| "()".to_string());
        let return_type_external = if is_primitive_arg(&return_type) || return_type == "()" {
            return_type.clone()
        } else {
            get_type_external_path(&return_type, version_data)
        };
        let capi_return_type = if is_primitive_arg(&return_type) || return_type == "()" {
            return_type.clone()
        } else {
            format!("ffi::{}{}", prefix, return_type)
        };
        
        // Default value for return - use external type
        let default_expr = match return_type.as_str() {
            "()" => "()".to_string(),
            "Update" => format!("{}::DoNothing", return_type_external),
            "OnTextInputReturn" => format!(
                "{} {{ update: azul_core::callbacks::Update::DoNothing, valid: azul_layout::widgets::text_input::TextInputValid::Yes }}",
                return_type_external
            ),
            "ImageRef" => {
                // ImageRef doesn't implement Default, use null_image() with empty values instead
                format!("{}::null_image(0, 0, azul_core::resources::RawImageFormat::BGRA8, Vec::new())", return_type_external)
            }
            _ => {
                format!("{}::default()", return_type_external)
            }
        };
        
        // Generate trampoline with EXTERNAL types in signature
        code.push_str(&format!("/// Trampoline for {} - bridges Python to Rust\r\n", callback_name));
        code.push_str(&format!("/// Data is in RefAny (PyDataWrapper), callable is retrieved via {}.get_callable()\r\n", info_type));
        code.push_str(&format!("/// Signature uses external types to match the callback type definition\r\n"));
        code.push_str(&format!("extern \"C\" fn {}(\r\n    {}\r\n) -> {} {{\r\n", 
            trampoline_name, args_sig, return_type_external));
        
        code.push_str(&format!("    let default = {};\r\n\r\n", default_expr));
        
        // Convert external types to FFI for Python wrapper interaction
        if !info_type.is_empty() {
            let info_ffi_type = format!("ffi::{}{}", prefix, info_type);
            code.push_str(&format!("    // Convert external info type to FFI for Python wrapper\r\n"));
            code.push_str(&format!("    let {}_ffi: {} = unsafe {{ mem::transmute({}) }};\r\n\r\n", 
                info_arg_name, info_ffi_type, info_arg_name));
        }
        
        // Get user data from RefAny (PyDataWrapper) - data is already external type
        code.push_str("    // Extract Python user data from RefAny\r\n");
        code.push_str("    let mut data_core = data;\r\n");
        code.push_str("    let py_data_wrapper = match data_core.downcast_ref::<PyDataWrapper>() {\r\n");
        code.push_str("        Some(s) => s,\r\n");
        code.push_str("        None => return default,\r\n");
        code.push_str("    };\r\n");
        code.push_str("    let py_data = match py_data_wrapper._py_data.as_ref() {\r\n");
        code.push_str("        Some(s) => s,\r\n");
        code.push_str("        None => return default,\r\n");
        code.push_str("    };\r\n\r\n");
        
        // Get callable from info.get_callable() (PyCallableWrapper)
        // Use the original external type (data is already external)
        let info_external_path = get_type_external_path(&info_type, version_data);
        code.push_str("    // Get Python callable from info.get_callable()\r\n");
        code.push_str(&format!("    let info_rust: &{} = unsafe {{ mem::transmute(&{}_ffi) }};\r\n", info_external_path, info_arg_name));
        code.push_str("    let callable_opt: azul_core::refany::OptionRefAny = info_rust.get_callable();\r\n");
        code.push_str("    let callable_refany = match callable_opt {\r\n");
        code.push_str("        azul_core::refany::OptionRefAny::Some(r) => r,\r\n");
        code.push_str("        azul_core::refany::OptionRefAny::None => return default,\r\n");
        code.push_str("    };\r\n");
        code.push_str("    let mut callable_core = callable_refany;\r\n");
        code.push_str("    let py_callable_wrapper = match callable_core.downcast_ref::<PyCallableWrapper>() {\r\n");
        code.push_str("        Some(s) => s,\r\n");
        code.push_str("        None => return default,\r\n");
        code.push_str("    };\r\n");
        code.push_str("    let py_callable = match py_callable_wrapper._py_callable.as_ref() {\r\n");
        code.push_str("        Some(s) => s,\r\n");
        code.push_str("        None => return default,\r\n");
        code.push_str("    };\r\n\r\n");
        
        // Call Python
        code.push_str("    Python::attach(|py| {\r\n");
        
        // Wrap FFI type in Python wrapper struct
        if !info_type.is_empty() {
            let info_py_type = format!("{}{}", prefix, info_type);
            code.push_str(&format!("        let info_py = {} {{ inner: {}_ffi }};\r\n", info_py_type, info_arg_name));
        }
        
        // Wrap extra args in Python wrappers - convert external to FFI first
        for (arg_name, arg_type) in &extra_args {
            if is_primitive_arg(arg_type) {
                code.push_str(&format!("        let {}_py = {};\r\n", arg_name, arg_name));
            } else {
                let py_type = format!("{}{}", prefix, arg_type);
                let ffi_type = format!("ffi::{}{}", prefix, arg_type);
                code.push_str(&format!("        let {}_ffi: {} = unsafe {{ mem::transmute({}) }};\r\n", 
                    arg_name, ffi_type, arg_name));
                code.push_str(&format!("        let {}_py = {} {{ inner: {}_ffi }};\r\n", 
                    arg_name, py_type, arg_name));
            }
        }
        
        // Build call args
        let mut call_args = "py_data.clone_ref(py)".to_string();
        if !info_type.is_empty() {
            call_args.push_str(", info_py");
        }
        for (arg_name, _) in &extra_args {
            call_args.push_str(&format!(", {}_py", arg_name));
        }
        
        // Make the call
        code.push_str(&format!("\r\n        match py_callable.call1(py, ({})) {{\r\n", call_args));
        
        let return_py_type = format!("{}{}", prefix, return_type);
        code.push_str("            Ok(result) => {\r\n");
        if return_type == "()" {
            code.push_str("                ()\r\n");
        } else {
            // Extract Python wrapper and transmute FFI result back to external type
            code.push_str(&format!("                match result.extract::<{}>(py) {{\r\n", return_py_type));
            code.push_str(&format!("                    Ok(ret) => unsafe {{ mem::transmute(ret.inner) }},\r\n"));
            code.push_str("                    Err(_) => default,\r\n");
            code.push_str("                }\r\n");
        }
        code.push_str("            }\r\n");
        code.push_str("            Err(e) => {\r\n");
        code.push_str("                #[cfg(feature = \"logging\")]\r\n");
        code.push_str(&format!("                log::error!(\"Exception in {} callback: {{:?}}\", e);\r\n", callback_name));
        code.push_str("                default\r\n");
        code.push_str("            }\r\n");
        code.push_str("        }\r\n");
        code.push_str("    })\r\n");
        code.push_str("}\r\n\r\n");
    }
    
    code
}

/// Generate the App class implementation
fn generate_app_class(prefix: &str) -> String {
    format!(r#"// --- App implementation ---

/// The main application - runs the event loop
#[pyclass(name = "App", module = "azul", unsendable)]
pub struct AzApp {{
    pub ptr: *const c_void,
    pub run_destructor: bool,
}}

#[pymethods]
impl AzApp {{
    /// Create a new App with user data and a layout callback
    #[new]
    fn new(data: Py<PyAny>, layout_callback: Py<PyAny>) -> PyResult<Self> {{
        Python::attach(|py| {{
            if !layout_callback.bind(py).is_callable() {{
                return Err(PyException::new_err("layout_callback must be callable"));
            }}
            Ok(())
        }})?;

        let app_data = AppDataTy {{
            _py_app_data: Some(data),
            _py_layout_callback: Some(layout_callback),
        }};

        let refany = azul_core::refany::RefAny::new(app_data);
        let app_config: azul_core::resources::AppConfig = Default::default();
        let app: crate::desktop::app::App = crate::desktop::app::App::new(refany, app_config);

        Ok(unsafe {{ core::mem::transmute(app) }})
    }}

    /// Run the application event loop with an initial window
    fn run(&mut self, mut window: {prefix}WindowCreateOptions) {{
        // Access the inner FFI type and set the layout callback
        window.inner.state.layout_callback = ffi::AzLayoutCallback {{
            cb: invoke_py_layout_callback,
            callable: ffi::AzOptionRefAny::None,
        }};
        
        let _self: &mut crate::desktop::app::App = unsafe {{ core::mem::transmute(self) }};
        let root_window: azul_layout::window_state::WindowCreateOptions = unsafe {{ core::mem::transmute(window.inner) }};
        _self.run(root_window);
    }}

    /// Add another window to the application
    fn add_window(&mut self, mut window: {prefix}WindowCreateOptions) {{
        // Access the inner FFI type and set the layout callback
        window.inner.state.layout_callback = ffi::AzLayoutCallback {{
            cb: invoke_py_layout_callback,
            callable: ffi::AzOptionRefAny::None,
        }};
        
        let _self: &mut crate::desktop::app::App = unsafe {{ core::mem::transmute(self) }};
        let create_options: azul_layout::window_state::WindowCreateOptions = unsafe {{ core::mem::transmute(window.inner) }};
        _self.add_window(create_options);
    }}

    /// Get the list of available monitors
    fn get_monitors(&self) -> {prefix}MonitorVec {{
        let _self: &crate::desktop::app::App = unsafe {{ core::mem::transmute(self) }};
        {prefix}MonitorVec {{ inner: unsafe {{ core::mem::transmute(_self.get_monitors()) }} }}
    }}

    fn __traverse__(&self, _visit: PyVisit<'_>) -> Result<(), PyTraverseError> {{
        Ok(())
    }}

    fn __clear__(&mut self) {{
    }}

    fn __str__(&self) -> String {{
        "App {{ ... }}".to_string()
    }}

    fn __repr__(&self) -> String {{
        self.__str__()
    }}
}}

impl Drop for AzApp {{
    fn drop(&mut self) {{
        if self.run_destructor {{
            unsafe {{
                core::ptr::drop_in_place(self as *mut AzApp as *mut crate::desktop::app::App);
            }}
        }}
    }}
}}

"#, prefix = prefix)
}

/// Get the external path for a type from api.json
/// Returns the full path like "azul_core::callbacks::Update"
fn get_type_external_path(type_name: &str, version_data: &VersionData) -> String {
    // Handle primitive types
    if matches!(
        type_name,
        "()" | "bool"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "isize"
            | "f32"
            | "f64"
    ) {
        return type_name.to_string();
    }

    // Look up in api.json
    if let Some((module, _)) = search_for_class_by_class_name(version_data, type_name) {
        if let Some(class_data) = get_class(version_data, module, type_name) {
            if let Some(ref external) = class_data.external {
                return external.clone();
            }
        }
    }

    // Fallback: guess based on naming convention
    if type_name.contains("Callback") {
        format!("azul_core::callbacks::{}", type_name)
    } else {
        format!("azul_layout::{}", type_name)
    }
}

/// Get the callback signature for a callback wrapper type
/// For example, IFrameCallback has a `cb` field of type IFrameCallbackType
/// We look up IFrameCallbackType to find the actual function signature
fn get_callback_signature(
    callback_wrapper_type: &str,
    version_data: &VersionData,
) -> Option<CallbackSignature> {
    // First, find the wrapper struct (e.g., IFrameCallback)
    let (module, _) = search_for_class_by_class_name(version_data, callback_wrapper_type)?;
    let wrapper_class = get_class(version_data, module, callback_wrapper_type)?;

    // The wrapper has a `cb` field pointing to the actual CallbackType
    let struct_fields = wrapper_class.struct_fields.as_ref()?;
    let mut callback_type_name: Option<&str> = None;

    for field_map in struct_fields {
        for (field_name, field_data) in field_map {
            if field_name == "cb" || field_name == "callback" {
                callback_type_name = Some(&field_data.r#type);
                break;
            }
        }
    }

    let cb_type_name = callback_type_name?;

    // Now look up the actual CallbackType to get the fn signature
    let (module2, _) = search_for_class_by_class_name(version_data, cb_type_name)?;
    let cb_type_class = get_class(version_data, module2, cb_type_name)?;

    let callback_typedef = cb_type_class.callback_typedef.as_ref()?;

    // Parse the function arguments
    // fn_args is Vec<CallbackArgData> where each has type and ref_kind
    // fn_args[0] is always &mut RefAny (data)
    // fn_args[1] is usually &mut SomeCallbackInfo
    // fn_args[2..] are extra args (like &CheckBoxState)
    let mut info_type = String::new();
    let mut info_type_external = String::new();
    let mut extra_args = Vec::new();

    for (i, arg_data) in callback_typedef.fn_args.iter().enumerate() {
        if i == 0 {
            // Skip RefAny (data) argument
            continue;
        } else if i == 1 {
            // This is the callback info type
            info_type = arg_data.r#type.clone();
            info_type_external = get_type_external_path(&arg_data.r#type, version_data);
        } else {
            // Extra arguments (e.g., &CheckBoxState, usize for ListView)
            let ext_path = get_type_external_path(&arg_data.r#type, version_data);
            extra_args.push((
                format!("arg{}", i), // We don't have names in CallbackArgData
                arg_data.r#type.clone(),
                arg_data.ref_kind.clone(),
                ext_path,
            ));
        }
    }

    // Get return type
    let return_type = callback_typedef
        .returns
        .as_ref()
        .map(|r| r.r#type.clone())
        .unwrap_or_else(|| "()".to_string());
    let return_type_external = get_type_external_path(&return_type, version_data);

    Some(CallbackSignature {
        callback_type: cb_type_name.to_string(),
        info_type,
        info_type_external,
        extra_args,
        return_type,
        return_type_external,
    })
}

/// Check if a struct field contains a type that CANNOT be wrapped for Python.
/// 
/// Most types CAN be wrapped:
/// - RefAny → Python takes PyObject, internally wrapped in RefAny
/// - Callbacks → Python takes Callable, routed through trampolines
/// - Regular structs/enums → Wrapped with { inner: ffi::AzFoo }
///
/// Only truly unwrappable types are forbidden:
/// - Raw pointers (*const, *mut) - unsafe, can't be safely exposed
/// - VecRef types - borrow semantics can't be expressed in Python
/// - Destructor types - internal implementation detail
fn field_has_forbidden_type(field_type: &str, version_data: &VersionData) -> bool {
    let (_, base_type, _) = analyze_type(field_type);

    // Raw pointers can't be safely exposed to Python
    if field_type.contains("*const") || field_type.contains("*mut") {
        return true;
    }

    // VecRef types have borrow semantics that can't be expressed in Python
    if base_type.ends_with("VecRef") || base_type.ends_with("VecRefMut") {
        return true;
    }

    // Refstr is a raw string pointer
    if base_type == "Refstr" {
        return true;
    }

    // Destructor types are internal implementation details
    if base_type.ends_with("Destructor") || base_type.ends_with("DestructorCallbackType") {
        return true;
    }

    // Look up the type in api.json to check for vec_ref_element_type
    if let Some((module, _)) = search_for_class_by_class_name(version_data, &base_type) {
        if let Some(class_data) = get_class(version_data, module, &base_type) {
            // VecRef types (detected by property, not name)
            if class_data.vec_ref_element_type.is_some() {
                return true;
            }
        }
    }

    // RefAny is NOT forbidden - it gets wrapped (Python PyObject → RefAny)
    // Callbacks are NOT forbidden - they go through trampolines
    // Regular types are NOT forbidden - they get wrapped

    false
}

/// Check if a type is a Vec type (has ptr, len, cap, destructor fields)
fn is_vec_type(class_data: &ClassData) -> bool {
    if let Some(ref struct_fields) = class_data.struct_fields {
        let field_names: Vec<&str> = struct_fields
            .iter()
            .flat_map(|m| m.keys())
            .map(|s| s.as_str())
            .collect();

        return field_names.contains(&"ptr")
            && field_names.contains(&"len")
            && field_names.contains(&"cap");
    }
    false
}

/// Check if a struct has any fields with truly forbidden types.
/// For Vec types and boxed objects, destructor fields are internal and ignored.
///
/// Most types are NOT forbidden - they get wrapped:
/// - RefAny → Python PyObject wrapped internally
/// - Callbacks → Routed through trampolines
/// - Regular types → Wrapped with { inner: ffi::AzFoo }
fn struct_has_forbidden_field(class_data: &ClassData, version_data: &VersionData) -> bool {
    let is_vec = is_vec_type(class_data);
    let is_boxed = class_data.is_boxed_object;

    if let Some(ref struct_fields) = class_data.struct_fields {
        for field_map in struct_fields {
            for (field_name, field_data) in field_map {
                // For Vec types and boxed objects, ignore destructor-related fields
                // (destructor is an internal implementation detail, not exposed to Python)
                if is_vec || is_boxed {
                    if field_name == "destructor" || field_name.ends_with("_destructor") {
                        continue;
                    }
                    // Also skip fields that are destructor callback types
                    if field_data.r#type.ends_with("DestructorCallbackType") {
                        continue;
                    }
                    // Skip raw pointer fields for boxed objects (they'll be converted to usize)
                    if is_boxed
                        && (field_data.ref_kind == RefKind::ConstPtr
                            || field_data.ref_kind == RefKind::MutPtr)
                    {
                        continue;
                    }
                }
                if field_has_forbidden_type(&field_data.r#type, version_data) {
                    return true;
                }
            }
        }
    }

    // Also check enum variants for forbidden types
    if let Some(ref enum_fields) = class_data.enum_fields {
        for variant_map in enum_fields {
            for (_, variant_data) in variant_map {
                if let Some(ref variant_type) = variant_data.r#type {
                    if field_has_forbidden_type(variant_type, version_data) {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Check if a function takes &mut self
/// In PyO3 0.27+, unsendable classes are frozen and cannot have &mut self methods
fn function_takes_mut_self(fn_data: &FunctionData) -> bool {
    for arg_map in &fn_data.fn_args {
        if let Some(self_type) = arg_map.get("self") {
            return self_type == "refmut" || self_type == "mut value";
        }
    }
    false
}

/// Check if a function has arguments with types that can't be used in Python.
///
/// Most types CAN be used - they get routed through wrappers:
/// - RefAny → Python takes PyObject, wrapped internally
/// - Callbacks → Python takes Callable, routed through trampolines
/// - Regular types → Wrapped with { inner: ffi::AzFoo }
///
/// Only truly unsupported types:
/// - Raw pointers (*const, *mut) - unsafe
/// - VecRef types - borrow semantics
/// - &mut self methods - PyO3 0.27+ frozen classes
fn function_has_unsupported_args(fn_data: &FunctionData, version_data: &VersionData) -> bool {
    // Skip &mut self methods - PyO3 0.27+ makes unsendable classes frozen
    if function_takes_mut_self(fn_data) {
        return true;
    }

    for arg_map in &fn_data.fn_args {
        for (name, arg_type) in arg_map {
            if name == "self" {
                continue;
            }

            // Raw pointers can't be passed from Python
            if arg_type.contains('*') {
                return true;
            }

            let (_, base_type, _) = analyze_type(arg_type);

            // VecRef types have borrow semantics that can't be expressed
            if is_vec_ref_type_by_name(&base_type) {
                return true;
            }

            // Look up the type to check properties
            if let Some((module, _)) = search_for_class_by_class_name(version_data, &base_type) {
                if let Some(class_data) = get_class(version_data, module, &base_type) {
                    if is_vec_ref_type(class_data) {
                        return true;
                    }
                    // Callback typedef types are NOW SUPPORTED - routed through trampolines
                    // RefAny is NOW SUPPORTED - wrapped (Python PyObject → RefAny)
                }
            }
        }
    }

    // Also check return type for raw pointers and VecRef
    if let Some(ref ret) = fn_data.returns {
        if ret.r#type.contains('*') {
            return true;
        }
        let (_, base_type, _) = analyze_type(&ret.r#type);
        if is_vec_ref_type_by_name(&base_type) {
            return true;
        }
        // RefAny return is OK - wrapped back to PyObject
    }

    false
}

/// Should this type be completely skipped for Python binding generation?
fn should_skip_type(class_name: &str, class_data: &ClassData, version_data: &VersionData) -> bool {
    // Skip types that have custom Python integration or are internal implementation details.
    // These types have type aliases in generate_python_patches_prefix() and should not
    // have wrapper structs generated (would cause duplicate definitions).
    const MANUAL_TYPES: &[&str] = &[
        "App", // Has custom constructor with Python callback (see generate_app_class)
        // These have manual FromPyObject/IntoPyObject impls for Python str/bytes/list conversion
        // If we also generate #[pyclass] for them, PyO3 creates conflicting trait impls
        "String",    // AzString <-> Python str
        "U8Vec",     // AzU8Vec <-> Python bytes  
        "StringVec", // AzStringVec <-> Python list[str]
        // GL Vec types - used internally for OpenGL, converted via helper functions
        "GLuintVec",    // AzGLuintVec <-> Vec<u32>
        "GLintVec",     // AzGLintVec <-> Vec<i32>
        // Note: GLint64Vec does not exist in api.json - only GLint64VecRefMut
        // Internal types - not exposed to Python, used internally for wrapping
        "RefAny",    // Internal: wraps Python PyObject for Rust callbacks
        "RefCount",  // Internal: reference counting for RefAny
        // Destructor types - internal implementation detail, not API types
        "U8VecDestructor",
        "StringVecDestructor",
        // Types with raw pointers that are used internally
        "StringMenuItem", // Has raw pointer fields
        "InstantPtr",     // Is a pointer type
    ];
    if MANUAL_TYPES.contains(&class_name) {
        return true;
    }

    // Skip recursive types
    if RECURSIVE_TYPES.contains(&class_name) {
        return true;
    }

    // Skip callback typedefs - they're function pointers, not types
    if is_callback_typedef(class_data) {
        return true;
    }

    // Skip VecRef types - they're raw pointer wrappers
    // Check both property and name pattern
    if is_vec_ref_type(class_data) {
        return true;
    }
    if class_name.ends_with("VecRef") || class_name.ends_with("VecRefMut") {
        return true;
    }

    // Skip Refstr type (raw string pointer)
    if class_name == "Refstr" {
        return true;
    }

    // Skip generic templates - can't instantiate directly
    if is_generic_template(class_data) {
        return true;
    }

    // Skip primitive/void type aliases
    if is_primitive_or_void_alias(class_data) {
        return true;
    }

    // Skip simple type aliases (like XmlTagName = String)
    // These should use the underlying type directly
    if is_simple_type_alias(class_data) {
        return true;
    }

    // DON'T skip callback+data pair structs - these get special Python wrappers
    // They have a callback field + data: RefAny field that we wrap with PyObject
    if is_callback_data_pair_struct(class_data, version_data).is_some() {
        return false;
    }

    // Skip structs that contain callbacks or RefAny (but not callback+data pairs)
    if struct_has_forbidden_field(class_data, version_data) {
        return true;
    }

    false
}

// TYPE ALIAS INSTANTIATION
// For type aliases like StyleCursorValue = CssPropertyValue<StyleCursor>

/// Instantiate a type alias by resolving its generic target
/// Resolve a type alias to its underlying type, following alias chains
/// e.g. GridAutoTracks -> GridTemplate
fn resolve_type_alias_chain<'a>(type_name: &'a str, version_data: &'a VersionData) -> &'a str {
    let mut current = type_name;
    for _ in 0..10 {
        // Limit recursion depth
        if let Some((mod_name, _)) = search_for_class_by_class_name(version_data, current) {
            if let Some(class) = get_class(version_data, mod_name, current) {
                // Check if it's a simple type alias (no generic args)
                if let Some(ref ta) = class.type_alias {
                    if ta.generic_args.is_empty() {
                        current = &ta.target;
                        continue;
                    }
                }
            }
        }
        break;
    }
    current
}

fn instantiate_type_alias(
    _class_name: &str,
    class_data: &ClassData,
    version_data: &VersionData,
) -> Option<ClassData> {
    let type_alias = class_data.type_alias.as_ref()?;

    // Only handle generic type aliases
    if type_alias.generic_args.is_empty() {
        return None;
    }

    // Find the target type
    let (module_name, _) = search_for_class_by_class_name(version_data, &type_alias.target)?;
    let target_class = get_class(version_data, module_name, &type_alias.target)?;

    // Get the generic parameters
    let generic_params = target_class.generic_params.as_ref()?;

    if generic_params.len() != type_alias.generic_args.len() {
        return None;
    }

    // Resolve generic args through type alias chains
    // e.g. if arg is "GridAutoTracks" and that's an alias for "GridTemplate",
    // use "GridTemplate" as the final type
    let resolved_args: Vec<&str> = type_alias
        .generic_args
        .iter()
        .map(|arg| resolve_type_alias_chain(arg.as_str(), version_data))
        .collect();

    // Check if any generic arg refers to a type that should be skipped
    for arg in &resolved_args {
        if let Some((mod_name, _)) = search_for_class_by_class_name(version_data, arg) {
            if let Some(arg_class) = get_class(version_data, mod_name, arg) {
                if should_skip_type(arg, arg_class, version_data) {
                    return None;
                }
            }
        }
    }

    // Build substitution map using resolved types
    let mut substitutions: IndexMap<&str, &str> = IndexMap::new();
    for (param, arg) in generic_params.iter().zip(resolved_args.iter()) {
        substitutions.insert(param.as_str(), *arg);
    }

    // Create instantiated class
    let mut new_class = target_class.clone();
    new_class.generic_params = None;
    new_class.type_alias = None;

    // Use the external path from the original alias class (e.g. StyleCursorValue),
    // not from the generic target (CssPropertyValue<T>)
    if class_data.external.is_some() {
        new_class.external = class_data.external.clone();
    }

    // Substitute in enum_fields
    if let Some(ref mut enum_fields) = new_class.enum_fields {
        for variant_map in enum_fields.iter_mut() {
            for (_, variant_data) in variant_map.iter_mut() {
                if let Some(ref mut ty) = variant_data.r#type {
                    if let Some(&concrete) = substitutions.get(ty.as_str()) {
                        *ty = concrete.to_string();
                    }
                }
            }
        }
    }

    // Substitute in struct_fields
    if let Some(ref mut struct_fields) = new_class.struct_fields {
        for field_map in struct_fields.iter_mut() {
            for (_, field_data) in field_map.iter_mut() {
                if let Some(&concrete) = substitutions.get(field_data.r#type.as_str()) {
                    field_data.r#type = concrete.to_string();
                }
            }
        }
    }

    Some(new_class)
}

// main generator function
/// Generate Python API code from API data using PyO3 0.27.2
pub fn generate_python_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();

    let version_data = api_data.get_version(version).unwrap();
    let prefix = api_data
        .get_version_prefix(version)
        .unwrap_or_else(|| PREFIX.to_string());

    // File header
    code.push_str(&format!(
        "// WARNING: autogenerated Python bindings for azul api version {}\r\n",
        version
    ));
    code.push_str("// Generated for PyO3 v0.27.2\r\n");
    code.push_str("// This file is included via include!() in dll/src/lib.rs\r\n");
    code.push_str("// This file is STANDALONE and does NOT depend on the c-api feature.\r\n");
    code.push_str("\r\n");

    // Build the type_to_external map for transmute operations
    // This maps Az-prefixed types to their internal Rust type paths
    let type_to_external = build_type_to_external_map(version_data, &prefix, true);

    // Generate the complete DLL API (structs, enums, functions) inline
    // This makes python-extension completely independent from c-api feature
    code.push_str("// ============================================================================\r\n");
    code.push_str("// GENERATED C-API TYPES (standalone, not imported from crate::ffi::dll)\r\n");
    code.push_str("// ============================================================================\r\n\r\n");
    
    // Generate the DLL API inline - but SKIP C-ABI functions
    // Python extension calls Rust functions directly, not via C-ABI
    let config = MemtestConfig {
        remove_serde: false,
        remove_optional_features: vec![],
        generate_fn_bodies: true,
        is_for_dll: true,
        generate_no_mangle: false,      // Not needed - we skip C-ABI functions
        skip_c_abi_functions: true,     // Skip C-ABI functions - we call Rust directly
        drop_via_external: true,        // Use transmute for Drop/Clone (no C-ABI functions)
        callback_typedef_use_external: true, // Use external types for callbacks (compatible signatures)
        extern_declarations_only: false, // We generate implementations, not extern decls
        link_library_name: None,         // No #[link] attribute needed
    };
    let replacements = TypeReplacements::new(version_data).unwrap();
    let dll_api_code = generate_generated_rs(api_data, &config, &replacements)
        .expect("Failed to generate DLL API for Python bindings");
    code.push_str(&dll_api_code);
    code.push_str("\r\n\r\n");

    // NOTE: We do NOT use `use __dll_api_inner::dll::*;` here!
    // Python wrappers have the same names (AzDom, AzString, etc.) as the C-API types,
    // so we use fully qualified paths (__dll_api_inner::dll::AzDom) instead.
    // This avoids naming conflicts between the wrapper and the wrapped type.

    // PyO3 imports
    code.push_str("use core::ffi::c_void;\r\n");
    code.push_str("use core::mem;\r\n");
    code.push_str("use pyo3::prelude::*;\r\n");
    code.push_str("use pyo3::types::*;\r\n");
    code.push_str("use pyo3::exceptions::PyException;\r\n");
    code.push_str("\r\n");

    // GL type aliases - use full path to avoid conflicts
    code.push_str("// GL type aliases for Python API\r\n");
    code.push_str("type AzGLuint = __dll_api_inner::dll::GLuint;\r\n");
    code.push_str("type AzGLint = __dll_api_inner::dll::GLint;\r\n");
    code.push_str("type AzGLint64 = __dll_api_inner::dll::GLint64;\r\n");
    code.push_str("type AzGLuint64 = __dll_api_inner::dll::GLuint64;\r\n");
    code.push_str("type AzGLenum = __dll_api_inner::dll::GLenum;\r\n");
    code.push_str("type AzGLintptr = __dll_api_inner::dll::GLintptr;\r\n");
    code.push_str("type AzGLboolean = __dll_api_inner::dll::GLboolean;\r\n");
    code.push_str("type AzGLsizeiptr = __dll_api_inner::dll::GLsizeiptr;\r\n");
    code.push_str("type AzGLvoid = __dll_api_inner::dll::GLvoid;\r\n");
    code.push_str("type AzGLbitfield = __dll_api_inner::dll::GLbitfield;\r\n");
    code.push_str("type AzGLsizei = __dll_api_inner::dll::GLsizei;\r\n");
    code.push_str("type AzGLclampf = __dll_api_inner::dll::GLclampf;\r\n");
    code.push_str("type AzGLfloat = __dll_api_inner::dll::GLfloat;\r\n");
    code.push_str("\r\n");

    // Generated Python patches PREFIX (helper functions, conversions, trampolines)
    // This includes helper functions, From/Into impls, PyO3 traits, and callback trampolines.
    // NOTE: App class is generated AFTER wrapper types, see end of this function.
    code.push_str(&generate_python_patches_prefix(version_data, &prefix));
    code.push_str("\r\n\r\n");

    // Collect all types
    let mut structs: Vec<(String, ClassData)> = Vec::new();
    let mut enums: Vec<(String, ClassData)> = Vec::new();

    for (_module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            // Handle type aliases with generic args - try to instantiate them
            if class_data.type_alias.is_some() {
                if let Some(instantiated) =
                    instantiate_type_alias(class_name, class_data, version_data)
                {
                    // Check if the instantiated type should be skipped
                    if should_skip_type(class_name, &instantiated, version_data) {
                        continue;
                    }

                    if instantiated.struct_fields.is_some() {
                        structs.push((class_name.to_string(), instantiated));
                    } else if instantiated.enum_fields.is_some() {
                        enums.push((class_name.to_string(), instantiated));
                    }
                }
                continue;
            }

            // Skip types based on properties
            if should_skip_type(class_name, class_data, version_data) {
                continue;
            }

            if class_data.struct_fields.is_some() {
                structs.push((class_name.to_string(), class_data.clone()));
            } else if class_data.enum_fields.is_some() {
                enums.push((class_name.to_string(), class_data.clone()));
            }
        }
    }

    // Generate struct definitions
    code.push_str(
        "// ============================================================================\r\n",
    );
    code.push_str("// STRUCT DEFINITIONS\r\n");
    code.push_str(
        "// ============================================================================\r\n\r\n",
    );

    // First generate callback+data pair wrapper types and trampolines
    code.push_str("// --- Callback+Data Pair Wrapper Types ---\r\n\r\n");
    let mut callback_pair_info: Vec<(String, String, String)> = Vec::new(); // (class_name, cb_field, cb_type)
    for (class_name, class_data) in &structs {
        if let Some((cb_field, cb_type, cb_sig)) =
            is_callback_data_pair_struct(class_data, version_data)
        {
            callback_pair_info.push((class_name.clone(), cb_field.clone(), cb_type.clone()));
            code.push_str(&generate_callback_data_pair_wrapper(
                class_name, &cb_field, &cb_type, &cb_sig, &prefix,
            ));
        }
    }
    code.push_str("\r\n");

    // Then generate regular struct definitions
    for (class_name, class_data) in &structs {
        // Skip callback+data pairs - they have their own generation
        if let Some((_, cb_field, cb_type)) =
            callback_pair_info.iter().find(|(n, _, _)| n == class_name)
        {
            code.push_str(&generate_callback_data_pair_struct(
                class_name, cb_field, cb_type, &prefix,
            ));
        } else {
            code.push_str(&generate_struct_definition(
                class_name,
                class_data,
                &prefix,
                version_data,
            ));
        }
    }

    // Generate enum definitions
    code.push_str(
        "// ============================================================================\r\n",
    );
    code.push_str("// ENUM DEFINITIONS\r\n");
    code.push_str(
        "// ============================================================================\r\n\r\n",
    );

    for (class_name, class_data) in &enums {
        code.push_str(&generate_enum_definition(
            class_name,
            class_data,
            &prefix,
            version_data,
        ));
    }

    // NOTE: We don't generate Copy implementations for Python types because:
    // 1. Simple enums already have Copy in generate_enum_definition
    // 2. Structs may contain fields without Copy, so we can't safely impl Copy
    // 3. Clone via wrapper to C-API types is sufficient for PyO3

    // Generate Clone implementations for STRUCTS only
    // (Enums already have Clone in generate_enum_definition)
    code.push_str(
        "// ============================================================================\r\n",
    );
    code.push_str("// CLONE IMPLEMENTATIONS\r\n");
    code.push_str(
        "// ============================================================================\r\n\r\n",
    );

    for (class_name, class_data) in &structs {
        code.push_str(&generate_clone_impl(class_name, class_data, &prefix));
    }
    // NOTE: Enums get Clone generated inline in generate_enum_definition

    // Generate Debug implementations for STRUCTS only
    // (Enums already have Debug in generate_enum_definition)
    code.push_str(
        "// ============================================================================\r\n",
    );
    code.push_str("// DEBUG IMPLEMENTATIONS\r\n");
    code.push_str(
        "// ============================================================================\r\n\r\n",
    );

    for (class_name, class_data) in &structs {
        code.push_str(&generate_debug_impl(class_name, class_data, &prefix));
    }

    // Generate Drop implementations
    code.push_str(
        "// ============================================================================\r\n",
    );
    code.push_str("// DROP IMPLEMENTATIONS\r\n");
    code.push_str(
        "// ============================================================================\r\n\r\n",
    );

    for (class_name, class_data) in &structs {
        // Callback+data pairs have custom drop in their wrapper
        let is_callback_pair = callback_pair_info.iter().any(|(n, _, _)| n == class_name);
        if !is_callback_pair {
            code.push_str(&generate_drop_impl(class_name, class_data, &prefix));
        }
    }
    for (class_name, class_data) in &enums {
        code.push_str(&generate_drop_impl(class_name, class_data, &prefix));
    }

    // Generate #[pymethods] implementations
    code.push_str(
        "// ============================================================================\r\n",
    );
    code.push_str("// PYMETHODS IMPLEMENTATIONS\r\n");
    code.push_str(
        "// ============================================================================\r\n\r\n",
    );

    for (class_name, class_data) in &structs {
        // Callback+data pairs have their pymethods generated with the struct
        let is_callback_pair = callback_pair_info.iter().any(|(n, _, _)| n == class_name);
        if !is_callback_pair {
            code.push_str(&generate_struct_pymethods(
                class_name,
                class_data,
                &prefix,
                version_data,
                &type_to_external,
            ));
        }
    }
    for (class_name, class_data) in &enums {
        code.push_str(&generate_enum_pymethods(
            class_name,
            class_data,
            &prefix,
            version_data,
            &type_to_external,
        ));
    }

    // Generate Python module
    code.push_str(&generate_python_module(
        &structs,
        &enums,
        &prefix,
        version_data,
    ));

    // Generate App class AFTER all wrapper types are defined
    // App uses AzWindowCreateOptions and AzMonitorVec which are wrapper types
    code.push_str("\r\n");
    code.push_str(&generate_app_class(&prefix));

    code
}

// struct generation
// 
// ALL Python structs are now wrappers around C-API types:
//   struct AzFoo { pub inner: __dll_api_inner::dll::AzFoo }
// 
// This avoids duplicating field definitions and ensures type compatibility.
// Traits (Clone, Debug, Drop) delegate to the inner C-API type.
// Methods call the C-API functions on `self.inner`.
//
fn generate_struct_definition(
    class_name: &str,
    class_data: &ClassData,
    prefix: &str,
    version_data: &VersionData,
) -> String {
    let mut code = String::new();
    let struct_name = format!("{}{}", prefix, class_name);
    // Full path to C-API type to avoid self-reference
    let c_api_type = format!("__dll_api_inner::dll::{}{}", prefix, class_name);

    // Determine pyclass attributes
    let unsendable = if needs_unsendable(class_data) {
        ", unsendable"
    } else {
        ""
    };

    code.push_str(&format!(
        "#[pyclass(name = \"{}\", module = \"azul\"{})]\r\n",
        class_name, unsendable
    ));
    // Use repr(transparent) so AzFoo and __dll_api_inner::dll::AzFoo have the same layout
    code.push_str("#[repr(transparent)]\r\n");
    code.push_str(&format!("pub struct {} {{\r\n", struct_name));
    code.push_str(&format!("    pub inner: {},\r\n", c_api_type));
    code.push_str("}\r\n\r\n");
    
    // From/Into conversions for easy interop
    code.push_str(&format!("impl From<{}> for {} {{\r\n", c_api_type, struct_name));
    code.push_str(&format!("    fn from(inner: {}) -> Self {{ Self {{ inner }} }}\r\n", c_api_type));
    code.push_str("}\r\n\r\n");
    
    code.push_str(&format!("impl From<{}> for {} {{\r\n", struct_name, c_api_type));
    code.push_str(&format!("    fn from(wrapper: {}) -> Self {{ wrapper.inner }}\r\n", struct_name));
    code.push_str("}\r\n\r\n");

    code
}

/// Check if an enum variant type should be skipped (callbacks, VecRef types, recursive types)
fn should_skip_enum_variant_type(variant_type: &str, version_data: &VersionData) -> bool {
    // Look up the variant type in api.json
    if let Some((module, _)) = search_for_class_by_class_name(version_data, variant_type) {
        if let Some(variant_class_data) = get_class(version_data, module, variant_type) {
            // Skip callback typedefs - can't be used in Python
            if is_callback_typedef(variant_class_data) {
                return true;
            }
            // Skip VecRef types - raw pointer wrappers
            if is_vec_ref_type(variant_class_data) {
                return true;
            }
        }
    }
    // Skip recursive types
    if RECURSIVE_TYPES.contains(&variant_type) {
        return true;
    }
    // Skip VecRef types by name pattern
    if is_vec_ref_type_by_name(variant_type) {
        return true;
    }
    false
}

fn generate_enum_definition(
    class_name: &str,
    class_data: &ClassData,
    prefix: &str,
    _version_data: &VersionData,
) -> String {
    let mut code = String::new();
    let enum_name = format!("{}{}", prefix, class_name);
    let c_api_type = format!("__dll_api_inner::dll::{}{}", prefix, class_name);

    let is_union = class_data
        .enum_fields
        .as_ref()
        .map(|f| enum_is_union(f))
        .unwrap_or(false);

    // Determine unsendable - all types need this due to nested pointers
    let unsendable = if needs_unsendable(class_data) {
        ", unsendable"
    } else {
        ""
    };

    // ALL enums are now wrappers around the C-API type
    // This is consistent with the struct approach and avoids trait conflicts
    code.push_str(&format!(
        "#[pyclass(name = \"{}\", module = \"azul\"{})]\r\n",
        class_name, unsendable
    ));
    code.push_str("#[repr(transparent)]\r\n");
    code.push_str(&format!("pub struct {} {{\r\n", enum_name));
    code.push_str(&format!("    pub inner: {},\r\n", c_api_type));
    code.push_str("}\r\n\r\n");

    // From/Into conversions
    code.push_str(&format!("impl From<{}> for {} {{\r\n", c_api_type, enum_name));
    code.push_str(&format!("    fn from(inner: {}) -> Self {{ Self {{ inner }} }}\r\n", c_api_type));
    code.push_str("}\r\n\r\n");

    code.push_str(&format!("impl From<{}> for {} {{\r\n", enum_name, c_api_type));
    code.push_str(&format!("    fn from(wrapper: {}) -> Self {{ wrapper.inner }}\r\n", enum_name));
    code.push_str("}\r\n\r\n");

    // Clone implementation - delegate to inner
    code.push_str(&format!("impl Clone for {} {{\r\n", enum_name));
    code.push_str("    fn clone(&self) -> Self {\r\n");
    code.push_str("        Self { inner: self.inner.clone() }\r\n");
    code.push_str("    }\r\n");
    code.push_str("}\r\n\r\n");

    // Debug implementation - delegate to inner
    code.push_str(&format!("impl core::fmt::Debug for {} {{\r\n", enum_name));
    code.push_str("    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {\r\n");
    code.push_str("        core::fmt::Debug::fmt(&self.inner, f)\r\n");
    code.push_str("    }\r\n");
    code.push_str("}\r\n\r\n");

    // For simple enums: implement PartialEq/Eq/Hash for Python comparison
    // NOTE: We DON'T implement Copy because the inner FFI type may not be Copy
    // (e.g., enums with String variants). Clone is sufficient for Python.
    if !is_union {
        // PartialEq via discriminant comparison
        code.push_str(&format!("impl PartialEq for {} {{\r\n", enum_name));
        code.push_str("    fn eq(&self, other: &Self) -> bool {\r\n");
        code.push_str("        // Compare discriminants via transmute to u8\r\n");
        code.push_str("        unsafe {\r\n");
        code.push_str(&format!("            let a: u8 = core::mem::transmute_copy(&self.inner);\r\n"));
        code.push_str(&format!("            let b: u8 = core::mem::transmute_copy(&other.inner);\r\n"));
        code.push_str("            a == b\r\n");
        code.push_str("        }\r\n");
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");

        code.push_str(&format!("impl Eq for {} {{}}\r\n\r\n", enum_name));

        // Hash via discriminant
        code.push_str(&format!("impl core::hash::Hash for {} {{\r\n", enum_name));
        code.push_str("    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {\r\n");
        code.push_str("        unsafe {\r\n");
        code.push_str(&format!("            let disc: u8 = core::mem::transmute_copy(&self.inner);\r\n"));
        code.push_str("            disc.hash(state);\r\n");
        code.push_str("        }\r\n");
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");
    }

    code
}

// // clone/drop implementations
// 
// Since all Python structs are now wrappers around C-API types,
// Clone/Debug/Drop simply delegate to the inner C-API type.
//
fn generate_clone_impl(class_name: &str, _class_data: &ClassData, prefix: &str) -> String {
    let mut code = String::new();
    let type_name = format!("{}{}", prefix, class_name);

    // All Python structs are wrappers: just clone inner
    code.push_str(&format!("impl Clone for {} {{\r\n", type_name));
    code.push_str("    fn clone(&self) -> Self {\r\n");
    code.push_str("        Self { inner: self.inner.clone() }\r\n");
    code.push_str("    }\r\n");
    code.push_str("}\r\n\r\n");

    code
}

/// Generate Copy impl for types that have Copy in api.json
fn generate_copy_impl(class_name: &str, class_data: &ClassData, prefix: &str) -> String {
    let mut code = String::new();
    let type_name = format!("{}{}", prefix, class_name);

    // Only generate Copy for types that have it in derive
    let has_derive_copy = class_data
        .derive
        .as_ref()
        .map(|d| d.iter().any(|t| t == "Copy"))
        .unwrap_or(false);

    if has_derive_copy {
        code.push_str(&format!("impl Copy for {} {{}}\r\n", type_name));
    }

    code
}

/// Generate Debug impl for types (needed for __repr__ in PyO3)
/// Delegates to the inner C-API type's Debug impl
fn generate_debug_impl(class_name: &str, _class_data: &ClassData, prefix: &str) -> String {
    let mut code = String::new();
    let type_name = format!("{}{}", prefix, class_name);

    // All Python structs are wrappers: delegate Debug to inner
    code.push_str(&format!("impl core::fmt::Debug for {} {{\r\n", type_name));
    code.push_str("    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {\r\n");
    code.push_str("        core::fmt::Debug::fmt(&self.inner, f)\r\n");
    code.push_str("    }\r\n");
    code.push_str("}\r\n\r\n");

    code
}

// Drop implementation for wrapper types
// Since all Python structs are wrappers, Drop is automatic (inner drops itself)
// We only need explicit Drop for types with custom destructors
fn generate_drop_impl(class_name: &str, class_data: &ClassData, prefix: &str) -> String {
    // Wrapper types don't need explicit Drop - inner handles it
    // The C-API type in __dll_api_inner::dll already has proper Drop impl
    let _ = (class_name, class_data, prefix);
    String::new()
}

// pymethods generation
fn generate_struct_pymethods(
    class_name: &str,
    class_data: &ClassData,
    prefix: &str,
    version_data: &VersionData,
    type_to_external: &HashMap<String, String>,
) -> String {
    let mut code = String::new();
    let struct_name = format!("{}{}", prefix, class_name);

    code.push_str(&format!("#[pymethods]\r\nimpl {} {{\r\n", struct_name));

    // Default constructor (if type has struct fields and no forbidden types)
    if class_data.struct_fields.is_some() {
        code.push_str(&generate_default_constructor(
            class_name,
            class_data,
            prefix,
            version_data,
        ));
    }

    // Constructors from api.json
    if let Some(constructors) = &class_data.constructors {
        for (ctor_name, ctor_data) in constructors {
            if function_has_unsupported_args(ctor_data, version_data) {
                continue;
            }
            code.push_str(&generate_function(
                class_name,
                ctor_name,
                ctor_data,
                prefix,
                version_data,
                true,
                type_to_external,
            ));
        }
    }

    // Methods from api.json
    if let Some(functions) = &class_data.functions {
        for (fn_name, fn_data) in functions {
            if function_has_unsupported_args(fn_data, version_data) {
                continue;
            }
            code.push_str(&generate_function(
                class_name,
                fn_name,
                fn_data,
                prefix,
                version_data,
                false,
                type_to_external,
            ));
        }
    }

    // __str__ and __repr__
    code.push_str("    fn __str__(&self) -> String {\r\n");
    code.push_str(&format!("        format!(\"{{:?}}\", self)\r\n"));
    code.push_str("    }\r\n\r\n");

    code.push_str("    fn __repr__(&self) -> String {\r\n");
    code.push_str("        self.__str__()\r\n");
    code.push_str("    }\r\n");

    code.push_str("}\r\n\r\n");
    code
}

fn generate_enum_pymethods(
    class_name: &str,
    class_data: &ClassData,
    prefix: &str,
    version_data: &VersionData,
    type_to_external: &HashMap<String, String>,
) -> String {
    let mut code = String::new();
    let enum_name = format!("{}{}", prefix, class_name);
    let c_api_type = format!("__dll_api_inner::dll::{}{}", prefix, class_name);

    let is_union = class_data
        .enum_fields
        .as_ref()
        .map(|f| enum_is_union(f))
        .unwrap_or(false);

    code.push_str(&format!("#[pymethods]\r\nimpl {} {{\r\n", enum_name));

    // For ALL enums (simple or union), generate variant constructors/accessors
    if let Some(enum_fields) = &class_data.enum_fields {
        if is_union {
            // Tagged union: variant constructors take an argument
            for variant_map in enum_fields {
                for (variant_name, variant_data) in variant_map {
                    if let Some(ref variant_type) = variant_data.r#type {
                        // Skip variant constructors for types that can't be passed from Python
                        // (raw pointers, VecRef, RefAny, callbacks, etc.)
                        if variant_type.contains('*') {
                            continue;
                        }
                        let (_, base_type, _) = analyze_type(variant_type);
                        if base_type == "RefAny" || base_type == "RefCount" {
                            continue;
                        }
                        // Check hardcoded VecRef types by name
                        if is_vec_ref_type_by_name(&base_type) {
                            continue;
                        }
                        // Check if the variant type is a callback, VecRef, or type_alias with pointer
                        if let Some((module, _)) =
                            search_for_class_by_class_name(version_data, &base_type)
                        {
                            if let Some(variant_class) = get_class(version_data, module, &base_type)
                            {
                                if is_callback_typedef(variant_class)
                                    || is_vec_ref_type(variant_class)
                                {
                                    continue;
                                }
                                // Skip types that shouldn't be constructed from Python
                                if should_skip_type(&base_type, variant_class, version_data) {
                                    continue;
                                }
                                // Skip boxed object types (they are internally managed)
                                if variant_class.is_boxed_object {
                                    continue;
                                }
                                // Skip type_alias that resolve to pointers
                                if let Some(ref type_alias) = variant_class.type_alias {
                                    use crate::autofix::types::ref_kind::RefKind;
                                    if type_alias.ref_kind == RefKind::MutPtr
                                        || type_alias.ref_kind == RefKind::ConstPtr
                                        || type_alias.target == "c_void"
                                    {
                                        continue;
                                    }
                                }
                            }
                        }

                        // Use wrapper type for parameter, but construct C-API enum
                        let py_type = rust_type_to_python_type(variant_type, prefix, version_data);
                        code.push_str("    #[staticmethod]\r\n");
                        code.push_str(&format!(
                            "    fn {}(v: {}) -> Self {{\r\n",
                            variant_name, py_type
                        ));
                        // For primitive types and aliases to primitives, use v directly
                        // For Vec types, we need to convert to the FFI Vec type
                        // For wrapper types, extract .inner
                        if is_primitive_or_alias_to_primitive(&base_type, version_data) {
                            code.push_str(&format!(
                                "        Self {{ inner: {}::{}(v) }}\r\n",
                                c_api_type, variant_name
                            ));
                        } else if let Some(element_type) = get_vec_element_type(&base_type) {
                            // Vec<AzFoo> needs conversion
                            let vec_type_name = format!("{}{}Vec", prefix, element_type);
                            let elem_type_name = format!("{}{}", prefix, element_type);
                            let external_vec_path = type_to_external.get(&vec_type_name)
                                .cloned()
                                .unwrap_or_else(|| format!("azul_core::dom::{}Vec", element_type));
                            let external_elem_path = type_to_external.get(&elem_type_name)
                                .cloned()
                                .unwrap_or_else(|| format!("azul_core::dom::{}", element_type));
                            
                            if is_primitive_or_alias_to_primitive(&element_type, version_data) {
                                // Vec<u8>, Vec<f32> etc.
                                // For FFI types: u8 → U8, f32 → F32
                                let capitalized = capitalize_first(&element_type);
                                let primitive_vec_type = format!("{}{}Vec", prefix, capitalized);
                                let external_primitive_vec = type_to_external.get(&primitive_vec_type)
                                    .cloned()
                                    .unwrap_or_else(|| format!("azul_css::corety::{}Vec", capitalized));
                                code.push_str(&format!(
                                    "        let converted: __dll_api_inner::dll::Az{}Vec = unsafe {{\r\n\
                                         let elem_vec: Vec<{}> = v;\r\n\
                                         let wrapped: {} = elem_vec.into();\r\n\
                                         core::mem::transmute(wrapped)\r\n\
                                     }};\r\n",
                                    capitalized,
                                    element_type,
                                    external_primitive_vec,
                                ));
                                code.push_str(&format!(
                                    "        Self {{ inner: {}::{}(converted) }}\r\n",
                                    c_api_type, variant_name
                                ));
                            } else {
                                code.push_str(&format!(
                                    "        let converted: __dll_api_inner::dll::Az{}Vec = unsafe {{\r\n\
                                         let inners: Vec<__dll_api_inner::dll::Az{}> = v.into_iter().map(|x| x.inner).collect();\r\n\
                                         let transmuted: Vec<{}> = core::mem::transmute(inners);\r\n\
                                         let wrapped: {} = transmuted.into();\r\n\
                                         core::mem::transmute(wrapped)\r\n\
                                     }};\r\n",
                                    element_type,
                                    element_type,
                                    external_elem_path,
                                    external_vec_path,
                                ));
                                code.push_str(&format!(
                                    "        Self {{ inner: {}::{}(converted) }}\r\n",
                                    c_api_type, variant_name
                                ));
                            }
                        } else {
                            // Check if it's an array type like [PixelValue; 2]
                            let (array_prefix, array_base, array_suffix) = analyze_type(variant_type);
                            if !array_suffix.is_empty() {
                                // Array type: need to convert each element's .inner
                                // For [AzPixelValue; 2] → [dll::AzPixelValue; 2]
                                // array_prefix contains "[", array_suffix contains "; 2]"
                                // The type should be [__dll_api_inner::dll::AzFoo; N] not __dll_api_inner::dll::[AzFoo; N]
                                code.push_str(&format!(
                                    "        let converted: {}__dll_api_inner::dll::{}{}{} = unsafe {{\r\n\
                                         let inners = v.map(|x| x.inner);\r\n\
                                         inners\r\n\
                                     }};\r\n",
                                    array_prefix, prefix, array_base, array_suffix
                                ));
                                code.push_str(&format!(
                                    "        Self {{ inner: {}::{}(converted) }}\r\n",
                                    c_api_type, variant_name
                                ));
                            } else {
                                code.push_str(&format!(
                                    "        Self {{ inner: {}::{}(v.inner) }}\r\n",
                                    c_api_type, variant_name
                                ));
                            }
                        }
                        code.push_str("    }\r\n\r\n");
                    } else {
                        // Unit variant in tagged union - no parentheses after variant name
                        code.push_str("    #[staticmethod]\r\n");
                        code.push_str(&format!("    fn {}() -> Self {{\r\n", variant_name));
                        code.push_str(&format!(
                            "        Self {{ inner: {}::{} }}\r\n",
                            c_api_type, variant_name
                        ));
                        code.push_str("    }\r\n\r\n");
                    }
                }
            }
        } else {
            // Simple C-style enum: generate #[classattr] for each variant
            for variant_map in enum_fields {
                for (variant_name, _) in variant_map {
                    code.push_str("    #[classattr]\r\n");
                    code.push_str(&format!(
                        "    fn {}() -> Self {{\r\n",
                        variant_name
                    ));
                    code.push_str(&format!(
                        "        Self {{ inner: {}::{} }}\r\n",
                        c_api_type, variant_name
                    ));
                    code.push_str("    }\r\n\r\n");
                }
            }
        }
    }

    // Constructors
    if let Some(constructors) = &class_data.constructors {
        for (ctor_name, ctor_data) in constructors {
            if function_has_unsupported_args(ctor_data, version_data) {
                continue;
            }
            code.push_str(&generate_function(
                class_name,
                ctor_name,
                ctor_data,
                prefix,
                version_data,
                true,
                type_to_external,
            ));
        }
    }

    // Methods
    if let Some(functions) = &class_data.functions {
        for (fn_name, fn_data) in functions {
            if function_has_unsupported_args(fn_data, version_data) {
                continue;
            }
            code.push_str(&generate_function(
                class_name,
                fn_name,
                fn_data,
                prefix,
                version_data,
                false,
                type_to_external,
            ));
        }
    }

    // __str__ and __repr__
    code.push_str("    fn __str__(&self) -> String {\r\n");
    code.push_str(&format!("        format!(\"{{:?}}\", self)\r\n"));
    code.push_str("    }\r\n\r\n");

    code.push_str("    fn __repr__(&self) -> String {\r\n");
    code.push_str("        self.__str__()\r\n");
    code.push_str("    }\r\n");

    code.push_str("}\r\n\r\n");
    code
}

fn generate_default_constructor(
    class_name: &str,
    class_data: &ClassData,
    prefix: &str,
    version_data: &VersionData,
) -> String {
    let mut code = String::new();
    let c_api_type = format!("__dll_api_inner::dll::{}{}", prefix, class_name);

    // Check if there's already a `new` or `default` constructor
    let has_new = class_data
        .constructors
        .as_ref()
        .map(|c| c.contains_key("new") || c.contains_key("default"))
        .unwrap_or(false);

    if has_new {
        return code;
    }

    // Check if type can have a Python constructor (no pointers, Ref types, etc.)
    // Other constructors from api.json are still generated even if this returns false
    if !can_have_python_constructor(class_data, version_data) {
        return code;
    }

    // Build constructor with all fields as arguments
    let struct_fields = match &class_data.struct_fields {
        Some(f) => f,
        None => return code,
    };

    code.push_str("    #[new]\r\n");
    code.push_str("    fn new(\r\n");

    // Parameters - use PyO3 wrapper types (AzFoo, not __dll_api_inner::dll::AzFoo)
    // because PyO3 needs to be able to extract them from Python objects
    for field_map in struct_fields {
        for (field_name, field_data) in field_map {
            // Use wrapper type for the parameter (PyO3 compatible)
            let field_type = rust_type_to_wrapper_type(
                &field_data.r#type,
                field_data.ref_kind.clone(),
                prefix,
                version_data,
            );
            code.push_str(&format!("        {}: {},\r\n", field_name, field_type));
        }
    }

    code.push_str("    ) -> Self {\r\n");
    // Construct the C-API type using .inner from wrapper types, then wrap it
    code.push_str(&format!("        Self {{ inner: {} {{\r\n", c_api_type));

    for field_map in struct_fields {
        for (field_name, field_data) in field_map {
            let field_type = &field_data.r#type;
            
            // For primitives (including type aliases to primitives like GLint -> i32) - use directly
            let value_expr = if is_primitive_or_alias_to_primitive(field_type, version_data) {
                field_name.clone()
            }
            // For String - transmute from Rust String
            else if field_type == "String" {
                format!("unsafe {{ core::mem::transmute(azul_css::corety::AzString::from({}.clone())) }}", field_name)
            }
            // For Vec types - convert appropriately
            else if let Some(element_type) = get_vec_element_type(field_type) {
                if is_primitive_or_alias_to_primitive(&element_type, version_data) {
                    format!("unsafe {{ core::mem::transmute({}.clone().into()) }}", field_name)
                } else if element_type == "String" {
                    format!("unsafe {{ core::mem::transmute(azul_css::corety::StringVec::from({}.clone().into_iter().map(|s| azul_css::corety::AzString::from(s)).collect::<Vec<_>>())) }}", field_name)
                } else {
                    format!("unsafe {{ core::mem::transmute({}.clone().into_iter().map(|x| x.inner).collect::<Vec<_>>()) }}", field_name)
                }
            }
            // For RefAny (Py<PyAny>) - create RefAny wrapper
            else if field_type == "RefAny" {
                format!("Python::with_gil(|py| unsafe {{ let wrapper = PyObjectWrapper {{ py_obj: {}.clone_ref(py) }}; core::mem::transmute::<_, __dll_api_inner::dll::AzRefAny>(azul_core::refany::RefAny::new(wrapper)) }})", field_name)
            }
            // For callback types and CallbackType aliases - skip constructor entirely
            // (this should not happen as can_have_python_constructor should have returned false)
            else if field_type.ends_with("CallbackType") {
                // Fallback - just use field_name, will likely cause compile error
                // but the constructor should have been skipped
                field_name.clone()
            }
            // For other non-primitives - extract .inner from wrapper
            else {
                format!("{}.inner", field_name)
            };
            code.push_str(&format!("            {}: {},\r\n", field_name, value_expr));
        }
    }

    code.push_str("        } }\r\n");
    code.push_str("    }\r\n\r\n");

    code
}

/// Convert a Rust type to PyO3 wrapper type (AzFoo, not __dll_api_inner::dll::AzFoo)
/// These are the #[pyclass] types that PyO3 can extract from Python objects
fn rust_type_to_wrapper_type(rust_type: &str, ref_kind: RefKind, prefix: &str, version_data: &VersionData) -> String {
    let trimmed = rust_type.trim();
    
    // Handle primitives - they don't need wrappers
    if is_primitive_arg(trimmed) {
        return trimmed.to_string();
    }
    
    // RefAny → Py<PyAny>
    if trimmed == "RefAny" {
        return "Py<PyAny>".to_string();
    }
    
    // Callback types → Py<PyAny>
    if let Some((module, _)) = search_for_class_by_class_name(version_data, trimmed) {
        if let Some(class_data) = get_class(version_data, module, trimmed) {
            if class_data.callback_typedef.is_some() {
                return "Py<PyAny>".to_string();
            }
        }
    }
    
    // CallbackType types (type aliases to function pointers) → Py<PyAny>
    if trimmed.ends_with("CallbackType") {
        return "Py<PyAny>".to_string();
    }
    
    // String → String (PyO3 auto-converts)
    if trimmed == "String" {
        return "String".to_string();
    }
    
    // Vec types with primitive elements → Vec<primitive>
    // Vec types with complex elements → keep as AzFooVec wrapper
    // This ensures the return type matches what transmute produces
    if let Some(element_type) = get_vec_element_type(trimmed) {
        if is_primitive_arg(&element_type) {
            return format!("Vec<{}>", element_type);
        } else if element_type == "String" {
            return "Vec<String>".to_string();
        } else {
            // Non-primitive element type: keep as AzFooVec wrapper (e.g., AzDebugMessageVec)
            // This matches the transmute output in the function body
            return format!("{}{}", prefix, trimmed);
        }
    }
    
    // For non-primitives, use the wrapper type (AzFoo)
    // Note: we don't handle pointers/refs here because constructors shouldn't have them
    format!("{}{}", prefix, trimmed)
}

/// Convert a Rust type to C-API inner type (__dll_api_inner::dll::AzFoo)
fn rust_type_to_c_api_inner_type(rust_type: &str, ref_kind: RefKind, prefix: &str) -> String {
    let trimmed = rust_type.trim();
    
    // Handle primitives
    if is_primitive_arg(trimmed) {
        return trimmed.to_string();
    }
    
    // Handle pointers and reference kinds
    match ref_kind {
        RefKind::ConstPtr => format!("*const __dll_api_inner::dll::{}{}", prefix, trimmed),
        RefKind::MutPtr => format!("*mut __dll_api_inner::dll::{}{}", prefix, trimmed),
        RefKind::Ref => format!("&__dll_api_inner::dll::{}{}", prefix, trimmed),
        RefKind::RefMut => format!("&mut __dll_api_inner::dll::{}{}", prefix, trimmed),
        RefKind::Value => format!("__dll_api_inner::dll::{}{}", prefix, trimmed),
        RefKind::Boxed => format!("Box<__dll_api_inner::dll::{}{}>", prefix, trimmed),
        RefKind::OptionBoxed => format!("Option<Box<__dll_api_inner::dll::{}{}>>", prefix, trimmed),
    }
}

fn generate_function(
    class_name: &str,
    fn_name: &str,
    fn_data: &FunctionData,
    prefix: &str,
    version_data: &VersionData,
    is_constructor: bool,
    type_to_external: &HashMap<String, String>,
) -> String {
    let mut code = String::new();

    // Skip functions without fn_body - they can't be called directly
    let fn_body = match &fn_data.fn_body {
        Some(body) => body.clone(),
        None => {
            // No fn_body means this function can't be implemented
            // Generate a comment instead
            code.push_str(&format!(
                "    // fn {}(...) - skipped: no fn_body in api.json\r\n\r\n",
                fn_name
            ));
            return code;
        }
    };

    // Get self type
    let (self_param, _self_call) = get_self_type(fn_data);
    let is_static = self_param.is_empty();

    // For constructors:
    // - "new" gets #[new] attribute and is special
    // - all others need #[staticmethod]
    let is_py_new = is_constructor && fn_name == "new";

    // Staticmethod attribute for static methods and non-new constructors
    if is_static && !is_py_new {
        code.push_str("    #[staticmethod]\r\n");
    }

    // #[new] attribute only for "new" constructor
    if is_py_new {
        code.push_str("    #[new]\r\n");
    }

    // Signature
    let py_fn_name = fn_name.to_string();

    code.push_str(&format!("    fn {}(", py_fn_name));

    // Self parameter
    if !is_static {
        code.push_str(&self_param);
    }

    // Build C-ABI style fn_args string for generate_transmuted_fn_body
    let mut c_api_args = Vec::new();
    
    // First pass: identify arguments that need special conversion and should be skipped in transmute
    let mut skip_transmute_args: HashSet<String> = HashSet::new();
    for arg_map in &fn_data.fn_args {
        for (arg_name, arg_type) in arg_map {
            if arg_name == "self" {
                continue;
            }
            let (ptr_prefix, base_type, _) = analyze_type(arg_type);
            // Skip primitives
            if is_primitive_arg(&base_type) || (!ptr_prefix.is_empty() && base_type == "c_void") {
                continue;
            }
            // These types are manually converted and should NOT be transmuted again
            if base_type == "RefAny" 
                || get_callback_info_for_type(&base_type, version_data).is_some()
                || is_callback_typedef_by_name(&base_type, version_data)
                || base_type == "String"
                || get_vec_element_type(&base_type).is_some() 
            {
                skip_transmute_args.insert(arg_name.clone());
            }
            // Non-primitive wrapper types also get .inner extracted
            // and need the converted name used
            else {
                skip_transmute_args.insert(arg_name.clone());
            }
        }
    }
    
    // Other parameters
    let mut first_param = is_static;
    let mut self_is_value = false;
    for arg_map in &fn_data.fn_args {
        for (arg_name, arg_type) in arg_map {
            if arg_name == "self" {
                // Track if self is by-value (needs clone in fn_body)
                self_is_value = arg_type == "value" || arg_type == "mut value";
                // Add self to c_api_args with proper type
                // In PyO3, the self parameter is named "self", not the class name
                // IMPORTANT: In PyO3, we ALWAYS receive &self or &mut self, never self by-value
                // So c_api_args must reflect what we actually receive from PyO3
                // The self_is_value flag is used later to tell generate_transmuted_fn_body to clone
                let self_c_type = format!("{}{}",  prefix, class_name);
                let self_c_arg = match self_param.as_str() {
                    "&self" => format!("self: &{}", self_c_type),
                    "&mut self" => format!("self: &mut {}", self_c_type),
                    _ => format!("self: &{}", self_c_type), // Default to & even if API says value
                };
                c_api_args.push(self_c_arg);
                continue;
            }
            if !first_param {
                code.push_str(", ");
            }
            first_param = false;
            let py_type = rust_type_to_python_type(arg_type, prefix, version_data);
            code.push_str(&format!("{}: {}", arg_name, py_type));
            
            // Add to C-API args with Az-prefixed type
            // All args go here, but some will be pre-converted with *_ffi suffix
            // generate_transmuted_fn_body uses these to determine what to transmute
            let c_api_type = rust_type_to_c_api_type(arg_type, prefix, version_data);
            c_api_args.push(format!("{}: {}", arg_name, c_api_type));
        }
    }

    code.push_str(")");

    // Return type - track both the Python return type and the C-API return type
    // so we can add conversion wrappers for types that need it (String, Vec<u8>, etc.)
    // Use rust_type_to_python_return_type for return types to keep Vec wrappers (matches transmute)
    let original_return_type = fn_data.returns.as_ref().map(|r| r.r#type.clone());
    let return_type_str = if let Some(ret) = &fn_data.returns {
        let ret_type = rust_type_to_python_return_type(&ret.r#type, prefix, version_data);
        code.push_str(&format!(" -> {}", ret_type));
        rust_type_to_c_api_type(&ret.r#type, prefix, version_data)
    } else if is_constructor {
        code.push_str(" -> Self");
        format!("{}{}", prefix, class_name)
    } else {
        String::new()
    };

    code.push_str(" {\r\n");

    // Generate conversion code for ALL non-primitive argument types
    // Python wrapper types (AzFoo) need to be converted to FFI types (ffi::AzFoo)
    // via .inner access or transmute
    let mut conversion_code = String::new();
    let mut converted_arg_names: HashMap<String, String> = HashMap::new();
    
    for arg_map in &fn_data.fn_args {
        for (arg_name, arg_type) in arg_map {
            if arg_name == "self" {
                continue;
            }
            
            let (ptr_prefix, base_type, _) = analyze_type(arg_type);
            
            // Skip primitives - they don't need conversion
            if is_primitive_arg(&base_type) {
                continue;
            }
            
            // Skip pointer types - they're passed as usize
            if !ptr_prefix.is_empty() && base_type == "c_void" {
                continue;
            }
            
            let converted_name = format!("{}_ffi", arg_name);
            
            // RefAny conversion: Py<PyAny> → azul_core::refany::RefAny (external type directly)
            // We produce the external type directly since fn_body expects external types
            if base_type == "RefAny" {
                conversion_code.push_str(&format!(
                    "        // Convert Python object to RefAny (external type)\r\n\
                     let {converted}: azul_core::refany::RefAny = Python::with_gil(|py| {{\r\n\
                         let wrapper = PyObjectWrapper {{ py_obj: {arg}.clone_ref(py) }};\r\n\
                         azul_core::refany::RefAny::new(wrapper)\r\n\
                     }});\r\n",
                    converted = converted_name,
                    arg = arg_name,
                ));
                converted_arg_names.insert(arg_name.clone(), converted_name);
            }
            // Callback conversion: Py<PyAny> → external callback type with trampoline
            // These are callback+data pair structs like IFrameCallback, Callback, etc.
            else if let Some(cb_info) = get_callback_info_for_type(&base_type, version_data) {
                // Get external type path
                let prefixed_type = format!("{}{}", prefix, cb_info.original_type);
                let external_type = type_to_external.get(&prefixed_type)
                    .cloned()
                    .unwrap_or_else(|| format!("azul_core::callbacks::{}", cb_info.original_type));
                let prefixed_cb_type = format!("{}{}", prefix, cb_info.callback_type);
                let external_cb_type = type_to_external.get(&prefixed_cb_type)
                    .cloned()
                    .unwrap_or_else(|| format!("azul_core::callbacks::{}", cb_info.callback_type));
                conversion_code.push_str(&format!(
                    "        // Convert Python callable to {} with trampoline (external type)\r\n\
                     let {converted}: {ext_type} = Python::with_gil(|py| {{\r\n\
                         let wrapper = {wrapper_name} {{\r\n\
                             _py_callback: Some({arg}.clone_ref(py)),\r\n\
                             _py_data: None,\r\n\
                         }};\r\n\
                         let refany = azul_core::refany::RefAny::new(wrapper);\r\n\
                         {ext_type} {{\r\n\
                             cb: {ext_cb_type} {{ cb: {trampoline} }},\r\n\
                             data: refany,\r\n\
                         }}\r\n\
                     }});\r\n",
                    cb_info.original_type,
                    converted = converted_name,
                    wrapper_name = cb_info.wrapper_name,
                    arg = arg_name,
                    ext_type = external_type,
                    ext_cb_type = external_cb_type,
                    trampoline = cb_info.trampoline_name,
                ));
                converted_arg_names.insert(arg_name.clone(), converted_name);
            }
            // callback_typedef conversion: Py<PyAny> → external callback type (raw function pointer with trampoline)
            // These are types like LayoutCallbackType that are direct function pointers (type aliases)
            // The Python argument is ignored here since these callback typedefs don't carry data.
            // The callable is expected to be stored separately (e.g., in AppDataTy._py_layout_callback).
            else if is_callback_typedef_by_name(&base_type, version_data) {
                let trampoline = get_callback_typedef_trampoline(&base_type);
                // Get external type from type_to_external
                let prefixed_type = format!("{}{}", prefix, base_type);
                let external_type = type_to_external.get(&prefixed_type)
                    .cloned()
                    .unwrap_or_else(|| format!("azul_core::callbacks::{}", base_type));
                // callback_typedef is just a function pointer type alias, not a struct
                // So we cast the trampoline function to the external type using 'as'
                conversion_code.push_str(&format!(
                    "        // callback_typedef: {base} - use trampoline directly (external type)\r\n\
                     let {converted}: {ext_type} = {trampoline} as {ext_type};\r\n",
                    base = base_type,
                    converted = converted_name,
                    ext_type = external_type,
                    trampoline = trampoline,
                ));
                converted_arg_names.insert(arg_name.clone(), converted_name);
            }
            // String conversion: Rust String → azul_css::corety::AzString (external type)
            // Use .clone() to preserve original value for use in fn_body
            else if base_type == "String" {
                conversion_code.push_str(&format!(
                    "        let {converted}: azul_css::corety::AzString = azul_css::corety::AzString::from({arg}.clone());\r\n",
                    converted = converted_name,
                    arg = arg_name,
                ));
                converted_arg_names.insert(arg_name.clone(), converted_name);
            }
            // Vec<AzFoo> conversion: collect .inner values and convert to external Vec type
            // Since fn_body expects external types, we produce external types directly
            else if let Some(element_type) = get_vec_element_type(&base_type) {
                // Check if element type is a GL primitive type alias (GLuint, GLint, etc.)
                let is_gl_primitive = matches!(element_type.as_str(), 
                    "GLuint" | "GLint" | "GLfloat" | "GLdouble" | "GLsizei" | "GLenum" |
                    "GLbitfield" | "GLbyte" | "GLubyte" | "GLshort" | "GLushort" |
                    "GLuint64" | "GLint64");
                
                // Get external Vec type path
                // For primitives, capitalize first letter (u8 -> U8Vec)
                let capitalized_elem = if is_primitive_arg(&element_type) {
                    element_type.chars().next().unwrap().to_uppercase().to_string() + &element_type[1..]
                } else {
                    element_type.clone()
                };
                let vec_type_name = format!("{}{}Vec", prefix, capitalized_elem);
                let external_vec_path = type_to_external.get(&vec_type_name)
                    .cloned()
                    .unwrap_or_else(|| format!("azul_css::corety::{}Vec", capitalized_elem));
                    
                if is_primitive_arg(&element_type) {
                    // Vec<u8> → external type (e.g., azul_css::corety::U8Vec)
                    conversion_code.push_str(&format!(
                        "        let {converted}: {ext_vec} = {arg}.into();\r\n",
                        converted = converted_name,
                        ext_vec = external_vec_path,
                        arg = arg_name,
                    ));
                } else if is_gl_primitive {
                    // Vec<GLuint> → external type (e.g., azul_core::gl::GLuintVec)
                    conversion_code.push_str(&format!(
                        "        let {converted}: {ext_vec} = {arg}.into();\r\n",
                        converted = converted_name,
                        ext_vec = external_vec_path,
                        arg = arg_name,
                    ));
                } else if element_type == "String" {
                    // Vec<String> → azul_css::corety::StringVec
                    conversion_code.push_str(&format!(
                        "        let {converted}: azul_css::corety::StringVec = {{\r\n\
                             let strings: Vec<azul_css::corety::AzString> = {arg}.into_iter().map(|s| azul_css::corety::AzString::from(s)).collect();\r\n\
                             azul_css::corety::StringVec::from(strings)\r\n\
                         }};\r\n",
                        converted = converted_name,
                        arg = arg_name,
                    ));
                } else {
                    // Vec<AzFoo> → external Vec type: collect .inner values, convert to external Vec type
                    let elem_type_name = format!("{}{}", prefix, element_type);
                    let external_elem_path = type_to_external.get(&elem_type_name)
                        .cloned()
                        .unwrap_or_else(|| format!("azul_core::dom::{}", element_type));
                    conversion_code.push_str(&format!(
                        "        let {converted}: {ext_vec} = unsafe {{\r\n\
                             let inners: Vec<__dll_api_inner::dll::Az{elem}> = {arg}.into_iter().map(|x| x.inner).collect();\r\n\
                             let transmuted_inners: Vec<{ext_elem}> = core::mem::transmute(inners);\r\n\
                             transmuted_inners.into()\r\n\
                         }};\r\n",
                        converted = converted_name,
                        elem = element_type,
                        arg = arg_name,
                        ext_elem = external_elem_path,
                        ext_vec = external_vec_path,
                    ));
                }
                converted_arg_names.insert(arg_name.clone(), converted_name);
            }
            // All other non-primitive types: extract .inner from wrapper and transmute to external type
            // The .inner field is the FFI type (dll::AzFoo), we need to transmute to external type
            else {
                let prefixed_type = format!("{}{}", prefix, base_type);
                let external_type = type_to_external.get(&prefixed_type)
                    .cloned()
                    .unwrap_or_else(|| format!("azul_core::dom::{}", base_type));
                conversion_code.push_str(&format!(
                    "        let {converted}: {ext_type} = unsafe {{ core::mem::transmute({arg}.inner.clone()) }};\r\n",
                    converted = converted_name,
                    ext_type = external_type,
                    arg = arg_name,
                ));
                converted_arg_names.insert(arg_name.clone(), converted_name);
            }
        }
    }
    
    if !conversion_code.is_empty() {
        code.push_str(&conversion_code);
        code.push_str("\r\n");
    }

    // Generate the function body using generate_transmuted_fn_body
    // Pass skip_transmute_args so already-converted args aren't transmuted again
    let c_api_args_str = c_api_args.join(", ");
    let mut body = generate_transmuted_fn_body(
        &fn_body,
        class_name,
        is_constructor,
        &return_type_str,
        prefix,
        type_to_external,
        &c_api_args_str,
        true, // is_for_dll
        true, // keep_self_name: PyO3 uses "self" as the parameter name
        self_is_value, // force_clone_self: clone if API says self by-value
        &skip_transmute_args, // Skip args that are already converted with _ffi suffix
    );
    
    // Replace original arg names with converted names in the function body
    // IMPORTANT: Sort by length descending to avoid partial replacements
    // (e.g., replace "dom_ffi" before "dom" to avoid "dom_ffi" → "dom_ffi_ffi")
    let mut sorted_replacements: Vec<_> = converted_arg_names.iter().collect();
    sorted_replacements.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    
    for (original, converted) in sorted_replacements {
        // Use word boundary matching to avoid partial replacements
        // Only replace when arg is a complete word (not part of another identifier)
        // Handle various contexts where the arg name might appear
        body = body.replace(&format!(" {}, ", original), &format!(" {}, ", converted));
        body = body.replace(&format!(" {})", original), &format!(" {})", converted));
        body = body.replace(&format!("({})", original), &format!("({})", converted));
        body = body.replace(&format!("({},", original), &format!("({},", converted));
        body = body.replace(&format!(", {})", original), &format!(", {})", converted));
        body = body.replace(&format!(", {},", original), &format!(", {},", converted));
        body = body.replace(&format!("({}, ", original), &format!("({}, ", converted));
        // Handle .into() pattern: arg.into()
        body = body.replace(&format!("{}.into()", original), &format!("{}.into()", converted));
        // Handle reference patterns: &arg and &mut arg (with word boundary after)
        body = body.replace(&format!("&mut {},", original), &format!("&mut {},", converted));
        body = body.replace(&format!("&mut {})", original), &format!("&mut {})", converted));
        body = body.replace(&format!("&{},", original), &format!("&{},", converted));
        body = body.replace(&format!("&{})", original), &format!("&{})", converted));
        // Handle comparison patterns: &arg < value, &arg > value
        body = body.replace(&format!("&{} <", original), &format!("&{} <", converted));
        body = body.replace(&format!("&{} >", original), &format!("&{} >", converted));
        body = body.replace(&format!("&{} ==", original), &format!("&{} ==", converted));
        body = body.replace(&format!("&{} !=", original), &format!("&{} !=", converted));
        // Handle assignment patterns: let mut arg = arg; and arg = value
        body = body.replace(&format!("let mut {} = {}", original, original), &format!("let mut {} = {}", converted, converted));
        body = body.replace(&format!("= {};", original), &format!("= {};", converted));
    }

    // Determine if we need to wrap the return value for Python type conversion
    // String/Vec<u8>/Vec<String>/etc. need conversion from AzString/AzU8Vec/etc.
    let return_conversion = original_return_type.as_ref().and_then(|rt| {
        let (_, base_type, _) = analyze_type(rt);
        if base_type == "String" {
            Some(("__ffi_result", "az_string_to_py_string(__ffi_result)".to_string()))
        } else if let Some(element_type) = get_vec_element_type(&base_type) {
            if element_type == "u8" {
                Some(("__ffi_result", "az_vecu8_to_py_vecu8(__ffi_result)".to_string()))
            } else if element_type == "String" {
                Some(("__ffi_result", "az_stringvec_to_py_stringvec(__ffi_result)".to_string()))
            } else if element_type == "GLuint" {
                // GLuintVec -> Vec<u32>
                Some(("__ffi_result", "az_gluintvec_to_py_vecu32(__ffi_result)".to_string()))
            } else if element_type == "GLint" {
                // GLintVec -> Vec<i32>
                Some(("__ffi_result", "az_glintvec_to_py_veci32(__ffi_result)".to_string()))
            } else if is_primitive_arg(&element_type) {
                // Vec<u32> etc. - just transmute to Vec<primitive>
                None
            } else {
                // Vec<AzFoo> - need to convert AzFooVec to Vec<AzFoo>
                // The transmute returns AzFooVec (Python wrapper with FFI inner), we need to convert to Vec<AzFoo>
                // First transmute .inner (FFI type) to external type, then call into_library_owned_vec()
                // Then wrap each external element in AzFoo wrapper
                let external_vec_type = type_to_external.get(&format!("{}{}Vec", prefix, element_type))
                    .cloned()
                    .unwrap_or_else(|| format!("azul_core::gl::{}Vec", element_type));
                Some(("__ffi_result", format!(
                    "{{ let ext_vec: {} = unsafe {{ core::mem::transmute(__ffi_result.inner) }}; ext_vec.into_library_owned_vec().into_iter().map(|x| Az{}{{ inner: unsafe {{ core::mem::transmute(x) }} }}).collect() }}",
                    external_vec_type,
                    element_type
                )))
            }
        } else {
            None
        }
    });

    // The body is already wrapped in { }, so we need to indent it properly
    // and add unsafe block
    if let Some((result_name, conversion)) = return_conversion {
        // Wrap the FFI result and convert it
        code.push_str(&format!("        let {} = unsafe {{\r\n", result_name));
        for line in body.lines() {
            if line.trim().is_empty() {
                code.push_str("\r\n");
            } else if line == "{" || line == "}" {
                // Skip outer braces from generate_transmuted_fn_body
                continue;
            } else {
                code.push_str(&format!("        {}\r\n", line));
            }
        }
        code.push_str("        };\r\n");
        code.push_str(&format!("        {}\r\n", conversion));
    } else {
        code.push_str("        unsafe {\r\n");
        for line in body.lines() {
            if line.trim().is_empty() {
                code.push_str("\r\n");
            } else if line == "{" || line == "}" {
                // Skip outer braces from generate_transmuted_fn_body
                continue;
            } else {
                code.push_str(&format!("        {}\r\n", line));
            }
        }
        code.push_str("        }\r\n");
    }
    code.push_str("    }\r\n\r\n");

    code
}

/// Convert a Rust type from api.json to C-API type (Az-prefixed)
/// For Python extension, we keep Rust primitive types instead of C types
fn rust_type_to_c_api_type(rust_type: &str, prefix: &str, version_data: &VersionData) -> String {
    let trimmed = rust_type.trim();
    
    // Handle references
    let (is_ref, is_mut, base) = if trimmed.starts_with("&mut ") {
        (true, true, trimmed.strip_prefix("&mut ").unwrap().trim())
    } else if trimmed.starts_with("&") {
        (true, false, trimmed.strip_prefix("&").unwrap().trim())
    } else {
        (false, false, trimmed)
    };
    
    // Convert the base type
    // For Python extension, keep Rust primitive types (f32, f64, etc.)
    // instead of converting to C types (float, double, etc.)
    let c_base = if is_primitive_arg(base) {
        // Keep Rust primitives as-is for Python extension
        base.to_string()
    } else {
        format!("{}{}", prefix, base)
    };
    
    // Reconstruct with references
    if is_ref && is_mut {
        format!("&mut {}", c_base)
    } else if is_ref {
        format!("&{}", c_base)
    } else {
        c_base
    }
}

fn get_self_type(fn_data: &FunctionData) -> (String, String) {
    for arg_map in &fn_data.fn_args {
        if let Some(self_type) = arg_map.get("self") {
            return match self_type.as_str() {
                "ref" => ("&self".to_string(), "mem::transmute(self)".to_string()),
                "refmut" => ("&mut self".to_string(), "mem::transmute(self)".to_string()),
                // PyO3 does NOT allow self by-value directly (Python objects are shared)
                // Use &self and clone() before consuming methods
                "value" => ("&self".to_string(), "mem::transmute(self.inner.clone())".to_string()),
                "mut value" => ("&self".to_string(), "mem::transmute(self.inner.clone())".to_string()),
                _ => ("&self".to_string(), "mem::transmute(self)".to_string()),
            };
        }
    }
    (String::new(), String::new())
}

// // callback+data pair generation
// unified generator for all callback+data structs (iframenode, buttononclick, etc.)
//
/// Generate wrapper type + trampoline for a callback+data pair
fn generate_callback_data_pair_wrapper(
    class_name: &str,
    _cb_field_name: &str,
    cb_wrapper_type: &str,
    cb_sig: &CallbackSignature,
    prefix: &str,
) -> String {
    let mut code = String::new();
    let wrapper_name = format!("{}Ty", class_name);
    let trampoline_name = format!("invoke_py_{}", to_snake_case(class_name));

    // 1. Generate the wrapper struct that holds Python objects
    code.push_str(&format!(
        "/// Python object wrapper for {} callback+data\r\n",
        class_name
    ));
    code.push_str("#[repr(C)]\r\n");
    code.push_str(&format!("pub struct {} {{\r\n", wrapper_name));
    code.push_str("    pub _py_callback: Option<Py<PyAny>>,\r\n");
    code.push_str("    pub _py_data: Option<Py<PyAny>>,\r\n");
    code.push_str("}\r\n\r\n");

    // 2. Generate the trampoline function
    // Since callback_typedef_use_external is enabled, the callback type is an alias
    // to the external type. So we must use EXTERNAL types in the signature, not C-API types.
    let info_type_az = format!("{}{}", prefix, cb_sig.info_type);
    let return_type_az = format!("{}{}", prefix, cb_sig.return_type);

    // Use external types for the function signature since callback_typedef_use_external=true
    // This makes the trampoline compatible with the callback type alias
    let external_return_type = if cb_sig.return_type_external.is_empty() {
        format!("__dll_api_inner::dll::{}{}", prefix, cb_sig.return_type)
    } else {
        cb_sig.return_type_external.clone()
    };
    let external_info_type = if cb_sig.info_type_external.is_empty() {
        format!("__dll_api_inner::dll::{}{}", prefix, cb_sig.info_type)
    } else {
        cb_sig.info_type_external.clone()
    };

    // External RefAny type
    let external_refany_type = "azul_core::refany::RefAny";

    // Build extra args for signature (using external types)
    let mut extra_args_sig = String::new();
    for (name, type_name, _ref_kind, ext_path) in cb_sig.extra_args.iter() {
        // All callback args are now by-value
        let arg_type = if is_primitive_arg(type_name) {
            type_name.clone()
        } else if !ext_path.is_empty() {
            ext_path.clone()
        } else {
            format!("__dll_api_inner::dll::{}{}", prefix, type_name)
        };
        extra_args_sig.push_str(&format!(", {}: {}", name, arg_type));
    }

    code.push_str(&format!(
        "/// Trampoline for {} - called by C-API, invokes Python\r\n",
        class_name
    ));
    // Callbacks now take by-value arguments using external types
    code.push_str(&format!(
        "extern \"C\" fn {}(\r\n    data: {},\r\n    info: {}{}\r\n) -> {} {{\r\n",
        trampoline_name, external_refany_type, external_info_type, extra_args_sig, external_return_type
    ));

    // Default value - using external types directly (no transmute needed)
    // CRITICAL: Use Default::default() of the external type, never mem::zeroed()
    // mem::zeroed() can cause UB for types with function pointers or non-null invariants
    let default_expr = match cb_sig.return_type.as_str() {
        "Update" => format!("{}::DoNothing", external_return_type),
        "OnTextInputReturn" => format!(
            "{} {{ update: azul_core::callbacks::Update::DoNothing, valid: \
             azul_layout::widgets::text_input::TextInputValid::Yes }}",
            external_return_type
        ),
        "()" => "()".to_string(),
        "ImageRef" => {
            // ImageRef doesn't implement Default, use null_image() instead
            format!("{}::null_image()", external_return_type)
        }
        _ => {
            // Use the external type's Default implementation directly
            format!("{}::default()", external_return_type)
        }
    };
    code.push_str(&format!("    let default = {};\r\n\r\n", default_expr));

    // data is already azul_core::refany::RefAny, no transmute needed
    code.push_str(
        "    let mut data_core = data;\r\n",
    );

    // Downcast RefAny to our wrapper - now using by-value mut binding
    code.push_str(&format!(
        "    let cb = match data_core.downcast_mut::<{}>() {{\r\n",
        wrapper_name
    ));
    code.push_str("        Some(s) => s,\r\n");
    code.push_str("        None => return default,\r\n");
    code.push_str("    };\r\n\r\n");

    // Get Python callback and data
    code.push_str("    let py_callback = match cb._py_callback.as_ref() {\r\n");
    code.push_str("        Some(s) => s,\r\n");
    code.push_str("        None => return default,\r\n");
    code.push_str("    };\r\n\r\n");

    code.push_str("    let py_data = match cb._py_data.as_ref() {\r\n");
    code.push_str("        Some(s) => s,\r\n");
    code.push_str("        None => return default,\r\n");
    code.push_str("    };\r\n\r\n");

    // Call Python with GIL
    code.push_str("    Python::attach(|py| {\r\n");
    // info is now by-value external type, transmute to Python wrapper type
    code.push_str(&format!(
        "        let info_py: {} = unsafe {{ mem::transmute(info) }};\r\n",
        info_type_az
    ));

    // Build call arguments - for extra args, all by-value now
    let mut call_args = String::from("py_data.clone_ref(py), info_py");
    for (name, type_name, ref_kind, _) in &cb_sig.extra_args {
        // For primitive types, don't add prefix
        let py_type = if is_primitive_arg(type_name) {
            type_name.clone()
        } else {
            format!("{}{}", prefix, type_name)
        };
        // All args are by-value now - transmute from external to Python wrapper type
        let transmute_type = py_type;
        code.push_str(&format!(
            "        let {}_py: {} = unsafe {{ mem::transmute({}) }};\r\n",
            name, transmute_type, name
        ));
        call_args.push_str(&format!(", {}_py", name));
    }

    code.push_str(&format!(
        "\r\n        match py_callback.call1(py, ({})) {{\r\n",
        call_args
    ));
    code.push_str(&format!("            Ok(result) => {{\r\n"));
    code.push_str(&format!(
        "                match result.extract::<{}>(py) {{\r\n",
        return_type_az
    ));
    // Transmute from Python wrapper type back to external type
    code.push_str(&format!(
        "                    Ok(ret) => unsafe {{ mem::transmute::<{}, {}>(ret) }},\r\n",
        return_type_az, external_return_type
    ));
    code.push_str("                    Err(_) => default,\r\n");
    code.push_str("                }\r\n");
    code.push_str("            }\r\n");
    code.push_str("            Err(e) => {\r\n");
    code.push_str("                #[cfg(feature = \"logging\")]\r\n");
    code.push_str(&format!(
        "                log::error!(\"Exception in {} callback: {{:?}}\", e);\r\n",
        class_name
    ));
    code.push_str("                default\r\n");
    code.push_str("            }\r\n");
    code.push_str("        }\r\n");
    code.push_str("    })\r\n");
    code.push_str("}\r\n\r\n");

    code
}

/// Generate the Python-facing struct for a callback+data pair
fn generate_callback_data_pair_struct(
    class_name: &str,
    cb_field_name: &str,
    cb_type: &str,
    prefix: &str,
) -> String {
    let mut code = String::new();
    let struct_name = format!("{}{}", prefix, class_name);
    let c_api_type = format!("__dll_api_inner::dll::{}{}", prefix, class_name);
    let cb_struct_name = format!("__dll_api_inner::dll::{}{}", prefix, cb_type);

    code.push_str(&format!(
        "/// {} - Python wrapper for callback+data pair\r\n",
        class_name
    ));
    code.push_str(&format!(
        "#[pyclass(name = \"{}\", module = \"azul\", unsendable)]\r\n",
        class_name
    ));
    code.push_str(&format!("pub struct {} {{\r\n", struct_name));
    code.push_str(&format!(
        "    pub inner: {},\r\n",
        c_api_type
    ));
    code.push_str("}\r\n\r\n");

    // Generate #[pymethods] inline
    let wrapper_name = format!("{}Ty", class_name);
    let trampoline_name = format!("invoke_py_{}", to_snake_case(class_name));

    code.push_str(&format!("#[pymethods]\r\nimpl {} {{\r\n", struct_name));

    // Constructor that takes Python data and callback
    code.push_str("    /// Create a new callback+data pair\r\n");
    code.push_str("    /// \r\n");
    code.push_str("    /// Args:\r\n");
    code.push_str("    ///     data: Any Python object to pass to the callback\r\n");
    code.push_str(
        "    ///     callback: A callable that receives (data, info) and returns the appropriate \
         type\r\n",
    );
    code.push_str("    #[new]\r\n");
    code.push_str("    fn new(data: Py<PyAny>, callback: Py<PyAny>) -> PyResult<Self> {\r\n");
    code.push_str("        // Verify callback is callable\r\n");
    code.push_str("        Python::attach(|py| {\r\n");
    code.push_str("            if !callback.bind(py).is_callable() {\r\n");
    code.push_str(
        "                return Err(PyException::new_err(\"callback must be callable\"));\r\n",
    );
    code.push_str("            }\r\n");
    code.push_str("            Ok(())\r\n");
    code.push_str("        })?;\r\n\r\n");

    // Create wrapper and RefAny
    code.push_str(&format!("        let wrapper = {} {{\r\n", wrapper_name));
    code.push_str("            _py_callback: Some(callback),\r\n");
    code.push_str("            _py_data: Some(data),\r\n");
    code.push_str("        };\r\n\r\n");

    code.push_str("        let ref_any = azul_core::refany::RefAny::new(wrapper);\r\n\r\n");

    // Create the C-API struct using the correct callback type name from api.json
    // NOTE: The callback struct has a `callable` field for Python callable storage,
    // but for widget callbacks we use None since the callable is stored in the RefAny wrapper
    code.push_str("        Ok(Self {\r\n");
    code.push_str(&format!(
        "            inner: {} {{\r\n",
        c_api_type
    ));
    code.push_str(&format!(
        "                {}: {} {{\r\n",
        cb_field_name, cb_struct_name
    ));
    code.push_str(&format!("                    cb: {},\r\n", trampoline_name));
    code.push_str("                    callable: __dll_api_inner::dll::AzOptionRefAny::None,\r\n");
    code.push_str("                },\r\n");
    code.push_str("                data: unsafe { mem::transmute(ref_any) },\r\n");
    code.push_str("            },\r\n");
    code.push_str("        })\r\n");
    code.push_str("    }\r\n\r\n");

    // __str__ and __repr__
    code.push_str("    fn __str__(&self) -> String {\r\n");
    code.push_str(&format!(
        "        \"{} {{ ... }}\".to_string()\r\n",
        class_name
    ));
    code.push_str("    }\r\n\r\n");

    code.push_str("    fn __repr__(&self) -> String {\r\n");
    code.push_str("        self.__str__()\r\n");
    code.push_str("    }\r\n");

    code.push_str("}\r\n\r\n");

    code
}

/// Convert CamelCase to snake_case
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

// python module generation
fn generate_python_module(
    structs: &[(String, ClassData)],
    enums: &[(String, ClassData)],
    prefix: &str,
    _version_data: &VersionData,
) -> String {
    let mut code = String::new();

    code.push_str(
        "// ============================================================================\r\n",
    );
    code.push_str("// PYTHON MODULE\r\n");
    code.push_str(
        "// ============================================================================\r\n\r\n",
    );

    code.push_str("#[pymodule]\r\n");
    code.push_str("fn azul(m: &Bound<'_, PyModule>) -> PyResult<()> {\r\n");
    code.push_str("\r\n");

    // Logging setup
    code.push_str("    // Configure logging\r\n");
    code.push_str(
        "    #[cfg(all(feature = \"use_pyo3_logger\", not(feature = \"use_fern_logger\")))] {\r\n",
    );
    code.push_str("        let _ = pyo3_log::init();\r\n");
    code.push_str("    }\r\n\r\n");

    // Add manually implemented classes (App has custom constructor)
    code.push_str("    // Manual implementations\r\n");
    code.push_str("    m.add_class::<AzApp>()?;\r\n");
    code.push_str("\r\n");

    // Add structs
    code.push_str("    // Structs\r\n");
    for (class_name, _) in structs {
        let struct_name = format!("{}{}", prefix, class_name);
        code.push_str(&format!("    m.add_class::<{}>()?;\r\n", struct_name));
    }
    code.push_str("\r\n");

    // Add enums
    code.push_str("    // Enums\r\n");
    for (class_name, _) in enums {
        let enum_name = format!("{}{}", prefix, class_name);
        code.push_str(&format!("    m.add_class::<{}>()?;\r\n", enum_name));
    }
    code.push_str("\r\n");

    code.push_str("    Ok(())\r\n");
    code.push_str("}\r\n");

    code
}

// helper functions

/// Get the inner element type for a Vec type (e.g., "DomVec" -> Some("Dom"))
/// Returns None if not a Vec type
/// Special cases: U8Vec -> "u8", U16Vec -> "u16", F32Vec -> "f32", etc.
fn get_vec_element_type(type_name: &str) -> Option<String> {
    if type_name.ends_with("Vec") && type_name.len() > 3 {
        let element = &type_name[..type_name.len() - 3];
        // Handle primitive type Vecs with uppercase names
        let lowercase = element.to_lowercase();
        if is_primitive_arg(&lowercase) {
            Some(lowercase)
        } else {
            Some(element.to_string())
        }
    } else {
        None
    }
}

/// Convert a Rust type to Python-compatible type name with explicit ref_kind
/// CRITICAL: All pointer types (*const T, *mut T, Box<T>) become usize in Python
/// because Python has no concept of raw pointers or Rust smart pointers.
fn rust_type_to_python_type_with_ref(
    rust_type: &str,
    ref_kind: RefKind,
    prefix: &str,
    version_data: &VersionData,
) -> String {
    // All pointer types become usize in Python
    // This includes *const T, *mut T, Box<T>, Option<Box<T>>
    match ref_kind {
        RefKind::ConstPtr | RefKind::MutPtr | RefKind::Boxed | RefKind::OptionBoxed => {
            return "usize".to_string();
        }
        RefKind::Ref | RefKind::RefMut => {
            // References shouldn't appear in field types for C-API structs
            // If they do, treat them as usize
            return "usize".to_string();
        }
        RefKind::Value => {
            // Value types are handled below
        }
    }
    rust_type_to_python_type(rust_type, prefix, version_data)
}

/// Convert a Rust type to Python-compatible return type name
/// For most types, this is the same as rust_type_to_python_type.
/// The key difference: the function body still calls az_*vec_to_py_vec* 
/// which converts to Vec<T>, so we must use the same types as input.
fn rust_type_to_python_return_type(rust_type: &str, prefix: &str, version_data: &VersionData) -> String {
    // Return types use the same conversion as input types
    // because the generated body calls conversion functions that produce Vec<AzFoo>
    rust_type_to_python_type(rust_type, prefix, version_data)
}

/// Convert a Rust type to Python-compatible type name
/// Resolves type aliases to their underlying types
/// 
/// Special handling:
/// - RefAny → Py<PyAny> (Python object wrapped in RefAny internally)
/// - Callback types → Py<PyAny> (Python callable routed through trampolines)
fn rust_type_to_python_type(rust_type: &str, prefix: &str, version_data: &VersionData) -> String {
    // Convert *const c_void and *mut c_void to usize for Python compatibility
    let trimmed = rust_type.trim();
    if trimmed == "*const c_void" || trimmed == "* const c_void" 
        || trimmed == "*mut c_void" || trimmed == "* mut c_void" {
        return "usize".to_string();
    }

    let (ptr_prefix, base_type, array_suffix) = analyze_type(rust_type);

    // RefAny → Py<PyAny> (Python object that gets wrapped internally)
    if base_type == "RefAny" {
        return "Py<PyAny>".to_string();
    }

    // Callback wrapper types (e.g., Callback, IFrameCallback) → Py<PyAny>
    if get_callback_info_for_type(&base_type, version_data).is_some() {
        return "Py<PyAny>".to_string();
    }
    
    // Raw callback_typedef types (e.g., CallbackType, LayoutCallbackType) → Py<PyAny>
    // These are function pointer types that can't be exposed to Python directly
    if let Some((module, _)) = search_for_class_by_class_name(version_data, &base_type) {
        if let Some(class_data) = get_class(version_data, module, &base_type) {
            if class_data.callback_typedef.is_some() {
                return "Py<PyAny>".to_string();
            }
        }
    }

    // If the base type is c_void with a pointer prefix, convert to usize
    if base_type == "c_void" && (ptr_prefix.contains("const") || ptr_prefix.contains("mut")) {
        return "usize".to_string();
    }

    if is_primitive_arg(&base_type) {
        return format!("{}{}{}", ptr_prefix, base_type, array_suffix);
    }

    // String → String (Rust String, PyO3 auto-converts from Python str)
    if base_type == "String" {
        return "String".to_string();
    }

    // FooVec types → Vec<AzFoo> (PyO3 auto-converts from Python list)
    // e.g., DomVec → Vec<AzDom>, U8Vec → Vec<u8>, StringVec → Vec<String>
    if let Some(element_type) = get_vec_element_type(&base_type) {
        if is_primitive_arg(&element_type) {
            // U8Vec → Vec<u8>, etc.
            return format!("Vec<{}>", element_type);
        } else if element_type == "String" {
            // StringVec → Vec<String>
            return "Vec<String>".to_string();
        } else {
            // DomVec → Vec<AzDom>, etc.
            return format!("Vec<{}{}>", prefix, element_type);
        }
    }

    // Look up if this type is a simple type alias
    if let Some((module, _)) = search_for_class_by_class_name(version_data, &base_type) {
        if let Some(class_data) = get_class(version_data, module, &base_type) {
            if let Some(ref type_alias) = class_data.type_alias {
                // Only resolve non-generic aliases
                if type_alias.generic_args.is_empty() {
                    // Check if this is an alias to c_void with pointer - convert to usize
                    if type_alias.target == "c_void" && 
                        (type_alias.ref_kind == RefKind::ConstPtr || type_alias.ref_kind == RefKind::MutPtr) {
                        return "usize".to_string();
                    }
                    
                    // Apply ref_kind from type_alias
                    let alias_ptr_prefix = match type_alias.ref_kind {
                        RefKind::ConstPtr => "*const ",
                        RefKind::MutPtr => "*mut ",
                        RefKind::Ref => "&",
                        RefKind::RefMut => "&mut ",
                        RefKind::Value => "",
                        RefKind::Boxed => "Box<",
                        RefKind::OptionBoxed => "Option<Box<",
                    };
                    let alias_ptr_suffix = match type_alias.ref_kind {
                        RefKind::Boxed => ">",
                        RefKind::OptionBoxed => ">>",
                        _ => "",
                    };
                    // Resolve to the target type with proper pointer prefix
                    let resolved =
                        rust_type_to_python_type(&type_alias.target, prefix, version_data);
                    return format!(
                        "{}{}{}{}",
                        alias_ptr_prefix, resolved, alias_ptr_suffix, array_suffix
                    );
                }
            }
        }
    }

    format!("{}{}{}{}", ptr_prefix, prefix, base_type, array_suffix)
}

/// Check if a type is a simple primitive that can have pyo3(get, set)
fn is_python_compatible_primitive(rust_type: &str) -> bool {
    let (_, base_type, _) = analyze_type(rust_type);
    matches!(
        base_type.as_str(),
        "bool"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "isize"
            | "f32"
            | "f64"
    )
}
