use indexmap::IndexMap;

use crate::{
    api::{ApiData, ClassData, EnumVariantData, FieldData, FunctionData, RefKind, VersionData},
    utils::{
        analyze::{
            analyze_type, enum_is_union, get_class, is_primitive_arg, replace_primitive_ctype,
            search_for_class_by_class_name,
        },
        string::snake_case_to_lower_camel,
    },
};

const PREFIX: &str = "Az";

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

                // Skip if type is a callback type (contains function pointers)
                if let Some((module, _)) = search_for_class_by_class_name(version_data, type_str) {
                    if let Some(field_class) = get_class(version_data, module, type_str) {
                        if is_callback_typedef(field_class) {
                            return false;
                        }
                    }
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

/// Check if a struct field contains a type that is forbidden for Python
/// (callbacks, RefAny, raw function pointers, internal types, destructor callbacks)
fn field_has_forbidden_type(field_type: &str, version_data: &VersionData) -> bool {
    let (_, base_type, _) = analyze_type(field_type);

    // Direct RefAny reference
    if base_type == "RefAny" || base_type == "RefCount" {
        return true;
    }

    // Option<RefAny> is also forbidden
    if base_type == "OptionRefAny" {
        return true;
    }

    // Internal types that shouldn't be exposed
    if base_type.ends_with("Inner") {
        return true;
    }

    // Callback types (function pointers)
    if base_type.ends_with("Callback") || base_type.ends_with("CallbackType") {
        return true;
    }

    // Destructor types (enum with function pointer variants)
    if base_type.ends_with("Destructor") {
        return true;
    }

    // Look up the type in api.json
    if let Some((module, _)) = search_for_class_by_class_name(version_data, &base_type) {
        if let Some(class_data) = get_class(version_data, module, &base_type) {
            // Is it a callback typedef?
            if is_callback_typedef(class_data) {
                return true;
            }
            // Is it a type alias to a callback or destructor?
            if let Some(ref type_alias) = class_data.type_alias {
                let target = &type_alias.target;
                if target.ends_with("Callback")
                    || target.ends_with("Type")
                    || target.ends_with("Destructor")
                {
                    return true;
                }
            }
        }
    } else {
        // Type not found in api.json - might be internal
        // Skip types that look like callbacks or internal types
        if base_type.contains("Callback")
            || (base_type.contains("Data") && base_type != "StyleFontFamiliesValue")
        {
            return true;
        }
    }

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

/// Check if a struct has any fields with forbidden types
/// For Vec types, we ignore the destructor field
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

/// Check if a function has arguments with types that can't be used in Python
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

            // RefAny can't be passed from Python directly
            if base_type == "RefAny" || base_type == "RefCount" {
                return true;
            }

            // Check hardcoded VecRef types by name
            if is_vec_ref_type_by_name(&base_type) {
                return true;
            }

            // Look up the type
            if let Some((module, _)) = search_for_class_by_class_name(version_data, &base_type) {
                if let Some(class_data) = get_class(version_data, module, &base_type) {
                    // Callback typedefs can't be passed
                    if is_callback_typedef(class_data) {
                        return true;
                    }
                    // VecRef types need special conversion
                    if is_vec_ref_type(class_data) {
                        return true;
                    }
                }
            }
        }
    }

    // Also check return type
    if let Some(ref ret) = fn_data.returns {
        if ret.r#type.contains('*') {
            return true;
        }
        let (_, base_type, _) = analyze_type(&ret.r#type);
        if base_type == "RefAny" || base_type == "RefCount" {
            return true;
        }
    }

    false
}

/// Types that should be wrapped as opaque types (no field getters/setters)
/// These contain callbacks or raw pointers that can't be exposed to Python
/// but need to exist as pyclass types so they can be used in enums
const OPAQUE_WRAPPER_TYPES: &[&str] = &[
    "StringMenuItem",   // Contains OptionCoreMenuCallback
    "InstantPtr",       // Contains callback function pointers and *const c_void
];

/// Check if a type should be generated as an opaque wrapper
fn is_opaque_wrapper_type(class_name: &str) -> bool {
    OPAQUE_WRAPPER_TYPES.contains(&class_name)
}

/// Should this type be completely skipped for Python binding generation?
fn should_skip_type(class_name: &str, class_data: &ClassData, version_data: &VersionData) -> bool {
    // Opaque wrapper types are NOT skipped - they get special generation
    if is_opaque_wrapper_type(class_name) {
        return false;
    }
    
    // Skip types that are manually implemented in python-patch/api.rs
    // These have complex Python integration (callbacks, GC, etc.)
    const MANUAL_TYPES: &[&str] = &[
        "App",
        "LayoutCallbackInfo",
        "WindowCreateOptions",
        // These have manual FromPyObject/IntoPyObject impls for Python str/bytes/list conversion
        // If we also generate #[pyclass] for them, PyO3 creates conflicting trait impls
        "String",    // AzString <-> Python str
        "U8Vec",     // AzU8Vec <-> Python bytes
        "StringVec", // AzStringVec <-> Python list[str]
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
    code.push_str("\r\n");

    // Imports
    code.push_str("use core::ffi::c_void;\r\n");
    code.push_str("use core::mem;\r\n");
    code.push_str("use pyo3::prelude::*;\r\n");
    code.push_str("use pyo3::types::*;\r\n");
    code.push_str("use pyo3::exceptions::PyException;\r\n");
    code.push_str("\r\n");

    // Import VecDestructor types and other internal types from C-API
    // These are needed for Vec types which have destructor fields
    // NOTE: AzU8VecDestructor and AzStringVecDestructor are imported in python-patch/api.rs
    // NOTE: Callback+data pair types (IFrameNode, ButtonOnClick, etc.) are NOT imported
    //       because we generate Python wrapper types for them
    // NOTE: AzStringMenuItem and AzInstantPtr are generated as opaque wrapper types
    code.push_str("// Import internal types from C-API (destructors, NodeData, etc.)\r\n");
    code.push_str("use crate::ffi::dll::{\r\n");
    code.push_str("    // Types used in struct fields that are not generated\r\n");
    code.push_str("    AzNodeData,\r\n");
    code.push_str("    AzCoreCallbackData,\r\n");
    // Removed: AzStringMenuItem - generated as opaque wrapper
    // Removed: AzInstantPtr - generated as opaque wrapper
    code.push_str("    // GL VecRef types used in Gl methods\r\n");
    code.push_str("    AzGLenumVecRef,\r\n");
    code.push_str("    AzI32VecRef,\r\n");
    code.push_str("    AzOptionU8VecRef,\r\n");
    code.push_str("    // VecDestructor types for owned Vec memory management\r\n");
    code.push_str("    AzAccessibilityActionVecDestructor,\r\n");
    code.push_str("    AzAccessibilityStateVecDestructor,\r\n");
    code.push_str("    AzAttributeVecDestructor,\r\n");
    code.push_str("    AzCascadeInfoVecDestructor,\r\n");
    code.push_str("    AzCoreCallbackDataVecDestructor,\r\n");
    code.push_str("    AzCssDeclarationVecDestructor,\r\n");
    code.push_str("    AzCssPathSelectorVecDestructor,\r\n");
    code.push_str("    AzCssRuleBlockVecDestructor,\r\n");
    code.push_str("    AzDebugMessageVecDestructor,\r\n");
    code.push_str("    AzDomVecDestructor,\r\n");
    code.push_str("    AzF32VecDestructor,\r\n");
    code.push_str("    AzGLintVecDestructor,\r\n");
    code.push_str("    AzGLuintVecDestructor,\r\n");
    code.push_str("    AzGridTrackSizingVecDestructor,\r\n");
    code.push_str("    AzIdOrClassVecDestructor,\r\n");
    code.push_str("    AzListViewRowVecDestructor,\r\n");
    code.push_str("    AzMenuItemVecDestructor,\r\n");
    code.push_str("    AzMonitorVecDestructor,\r\n");
    code.push_str("    AzNodeDataInlineCssPropertyVecDestructor,\r\n");
    code.push_str("    AzNodeDataVecDestructor,\r\n");
    code.push_str("    AzNodeHierarchyItemVecDestructor,\r\n");
    code.push_str("    AzNodeIdVecDestructor,\r\n");
    code.push_str("    AzNormalizedLinearColorStopVecDestructor,\r\n");
    code.push_str("    AzNormalizedRadialColorStopVecDestructor,\r\n");
    code.push_str("    AzParentWithNodeDepthVecDestructor,\r\n");
    code.push_str("    AzScanCodeVecDestructor,\r\n");
    code.push_str("    AzShapePointVecDestructor,\r\n");
    code.push_str("    AzStringPairVecDestructor,\r\n");
    // Removed: AzStringVecDestructor - imported in python-patch/api.rs
    code.push_str("    AzStyleBackgroundContentVecDestructor,\r\n");
    code.push_str("    AzStyleBackgroundPositionVecDestructor,\r\n");
    code.push_str("    AzStyleBackgroundRepeatVecDestructor,\r\n");
    code.push_str("    AzStyleBackgroundSizeVecDestructor,\r\n");
    code.push_str("    AzStyledNodeVecDestructor,\r\n");
    code.push_str("    AzStyleFilterVecDestructor,\r\n");
    code.push_str("    AzStyleFontFamilyVecDestructor,\r\n");
    code.push_str("    AzStylesheetVecDestructor,\r\n");
    code.push_str("    AzStyleTransformVecDestructor,\r\n");
    code.push_str("    AzSvgMultiPolygonVecDestructor,\r\n");
    code.push_str("    AzSvgPathElementVecDestructor,\r\n");
    code.push_str("    AzSvgPathVecDestructor,\r\n");
    code.push_str("    AzSvgSimpleNodeVecDestructor,\r\n");
    code.push_str("    AzSvgVertexVecDestructor,\r\n");
    // Removed: AzTabVecDestructor - does not exist
    code.push_str("    AzTagIdToNodeIdMappingVecDestructor,\r\n");
    // Removed: AzTessellatedSvgNodeVecDestructor - does not exist
    code.push_str("    AzU16VecDestructor,\r\n");
    code.push_str("    AzU32VecDestructor,\r\n");
    // Removed: AzU8VecDestructor - imported in python-patch/api.rs
    code.push_str("    AzVertexAttributeVecDestructor,\r\n");
    code.push_str("    AzVideoModeVecDestructor,\r\n");
    code.push_str("    AzVirtualKeyCodeVecDestructor,\r\n");
    // Removed: AzXmlNodeVecDestructor - does not exist
    code.push_str("    AzXmlNodeChildVecDestructor,\r\n");
    code.push_str("    AzXWindowTypeVecDestructor,\r\n");
    code.push_str("};\r\n");
    code.push_str("\r\n");

    // GL type definitions
    code.push_str("// GL type definitions\r\n");
    code.push_str("type GLuint = u32; type AzGLuint = GLuint;\r\n");
    code.push_str("type GLint = i32; type AzGLint = GLint;\r\n");
    code.push_str("type GLint64 = i64; type AzGLint64 = GLint64;\r\n");
    code.push_str("type GLuint64 = u64; type AzGLuint64 = GLuint64;\r\n");
    code.push_str("type GLenum = u32; type AzGLenum = GLenum;\r\n");
    code.push_str("type GLintptr = isize; type AzGLintptr = GLintptr;\r\n");
    code.push_str("type GLboolean = u8; type AzGLboolean = GLboolean;\r\n");
    code.push_str("type GLsizeiptr = isize; type AzGLsizeiptr = GLsizeiptr;\r\n");
    code.push_str("type GLvoid = c_void; type AzGLvoid = GLvoid;\r\n");
    code.push_str("type GLbitfield = u32; type AzGLbitfield = GLbitfield;\r\n");
    code.push_str("type GLsizei = i32; type AzGLsizei = GLsizei;\r\n");
    code.push_str("type GLclampf = f32; type AzGLclampf = GLclampf;\r\n");
    code.push_str("type GLfloat = f32; type AzGLfloat = GLfloat;\r\n");
    code.push_str("type AzF32 = f32;\r\n");
    code.push_str("type AzU16 = u16;\r\n");
    code.push_str("type AzU32 = u32;\r\n");
    code.push_str("type AzScanCode = u32;\r\n");
    code.push_str("\r\n");

    // Manual patches for callbacks and complex types
    // TODO: Eventually minimize this by generating more automatically
    code.push_str("// === Manual API patches for callbacks and complex types ===\r\n");
    code.push_str(include_str!("./python-patch/api.rs"));
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
    // 1. Simple enums already have #[derive(Copy)]
    // 2. Structs may contain fields without Copy, so we can't safely impl Copy
    // 3. Clone via transmute to C-API types is sufficient for PyO3

    // Generate Clone implementations
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
    for (class_name, class_data) in &enums {
        let is_union = class_data
            .enum_fields
            .as_ref()
            .map(|f| enum_is_union(f))
            .unwrap_or(false);
        if is_union {
            code.push_str(&generate_clone_impl(class_name, class_data, &prefix));
        }
    }

    // Generate Debug implementations for all types that don't have derive(Debug)
    // - Structs: need Debug for __repr__
    // - Union enums: need Debug (simple enums already have derive(Debug))
    code.push_str(
        "// ============================================================================\r\n",
    );
    code.push_str("// DEBUG IMPLEMENTATIONS\r\n");
    code.push_str(
        "// ============================================================================\r\n\r\n",
    );

    for (class_name, class_data) in &structs {
        // Callback+data pairs have their own Debug in wrapper
        let is_callback_pair = callback_pair_info.iter().any(|(n, _, _)| n == class_name);
        if !is_callback_pair {
            code.push_str(&generate_debug_impl(class_name, class_data, &prefix));
        }
    }
    for (class_name, class_data) in &enums {
        let is_union = class_data
            .enum_fields
            .as_ref()
            .map(|f| enum_is_union(f))
            .unwrap_or(false);
        // Only generate Debug for union enums - simple enums already have derive(Debug)
        if is_union {
            code.push_str(&generate_debug_impl(class_name, class_data, &prefix));
        }
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
            ));
        }
    }
    for (class_name, class_data) in &enums {
        code.push_str(&generate_enum_pymethods(
            class_name,
            class_data,
            &prefix,
            version_data,
        ));
    }

    // Generate Python module
    code.push_str(&generate_python_module(
        &structs,
        &enums,
        &prefix,
        version_data,
    ));

    code
}

/// Generate an opaque wrapper struct for types with callbacks or raw pointers
/// These types wrap the C-API type directly without exposing fields
fn generate_opaque_wrapper_struct(class_name: &str, prefix: &str) -> String {
    let struct_name = format!("{}{}", prefix, class_name);
    let c_api_type = format!("crate::ffi::dll::Az{}", class_name);
    
    let mut code = String::new();
    code.push_str(&format!("// Opaque wrapper for {} (contains callbacks/pointers)\r\n", class_name));
    code.push_str(&format!(
        "#[pyclass(name = \"{}\", module = \"azul\", unsendable)]\r\n",
        class_name
    ));
    code.push_str("#[repr(transparent)]\r\n");
    code.push_str(&format!("pub struct {} {{\r\n", struct_name));
    code.push_str(&format!("    pub inner: {},\r\n", c_api_type));
    code.push_str("}\r\n\r\n");
    
    code
}

// struct generation
fn generate_struct_definition(
    class_name: &str,
    class_data: &ClassData,
    prefix: &str,
    version_data: &VersionData,
) -> String {
    let mut code = String::new();
    let struct_name = format!("{}{}", prefix, class_name);

    // Check if this is an opaque wrapper type
    if is_opaque_wrapper_type(class_name) {
        return generate_opaque_wrapper_struct(class_name, prefix);
    }

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

    // Don't derive anything - nested C-API types don't implement Debug/Clone
    // Debug is implemented via __repr__ method in pymethods
    // Clone is implemented via C-API deepCopy function if available

    code.push_str("#[repr(C)]\r\n");
    code.push_str(&format!("pub struct {} {{\r\n", struct_name));

    // Fields
    if let Some(struct_fields) = &class_data.struct_fields {
        // Collect field names that have setter methods to avoid duplicate pyo3(get, set)
        let mut fields_with_setters: std::collections::HashSet<&str> =
            std::collections::HashSet::new();
        if let Some(functions) = &class_data.functions {
            for (fn_name, _) in functions {
                if fn_name.starts_with("set_") {
                    fields_with_setters.insert(&fn_name[4..]);
                }
            }
        }

        let is_boxed = class_data.is_boxed_object;

        for field_map in struct_fields {
            for (field_name, field_data) in field_map {
                let is_raw_ptr = field_data.ref_kind == RefKind::ConstPtr
                    || field_data.ref_kind == RefKind::MutPtr;
                let is_destructor = field_name == "destructor"
                    || field_data.r#type.ends_with("DestructorCallbackType");

                // For boxed objects, use usize for pointer and destructor fields
                // This maintains repr(C) compatibility while hiding internal pointers from Python
                let field_type = if is_boxed && (is_raw_ptr || is_destructor) {
                    "usize".to_string()
                } else {
                    // Apply ref_kind to get the actual field type using the helper function
                    rust_type_to_python_type_with_ref(
                        &field_data.r#type,
                        field_data.ref_kind.clone(),
                        prefix,
                        version_data,
                    )
                };

                // Only add pyo3(get, set) for simple types without explicit setters
                let has_setter = fields_with_setters.contains(field_name.as_str());
                let is_simple = is_python_compatible_primitive(&field_data.r#type);

                if !is_raw_ptr && !is_destructor && !has_setter && is_simple {
                    code.push_str("    #[pyo3(get, set)]\r\n");
                }
                code.push_str(&format!("    pub {}: {},\r\n", field_name, field_type));
            }
        }
    }

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
    version_data: &VersionData,
) -> String {
    let mut code = String::new();
    let enum_name = format!("{}{}", prefix, class_name);

    let is_union = class_data
        .enum_fields
        .as_ref()
        .map(|f| enum_is_union(f))
        .unwrap_or(false);

    // Determine unsendable
    let unsendable = if needs_unsendable(class_data) {
        ", unsendable"
    } else {
        ""
    };

    if is_union {
        // Tagged union - complex enum
        // First, count how many variants will actually be generated
        let mut variant_count = 0;
        if let Some(enum_fields) = &class_data.enum_fields {
            for variant_map in enum_fields {
                for (_, variant_data) in variant_map {
                    if let Some(ref variant_type) = variant_data.r#type {
                        // Check if this variant type should be skipped
                        if should_skip_enum_variant_type(variant_type, version_data) {
                            continue;
                        }
                        variant_count += 1;
                    } else {
                        // Unit variant always counts
                        variant_count += 1;
                    }
                }
            }
        }

        // If no variants will be generated, skip the entire enum
        if variant_count == 0 {
            return String::new();
        }

        code.push_str(&format!(
            "#[pyclass(name = \"{}\", module = \"azul\"{})]\r\n",
            class_name, unsendable
        ));
        // Don't derive Debug - nested types might be C-API types without Debug
        code.push_str("#[repr(C, u8)]\r\n");
        code.push_str(&format!("pub enum {} {{\r\n", enum_name));

        if let Some(enum_fields) = &class_data.enum_fields {
            for variant_map in enum_fields {
                for (variant_name, variant_data) in variant_map {
                    if let Some(ref variant_type) = variant_data.r#type {
                        if should_skip_enum_variant_type(variant_type, version_data) {
                            continue;
                        }
                        let py_type = rust_type_to_python_type(variant_type, prefix, version_data);
                        code.push_str(&format!("    {}({}),\r\n", variant_name, py_type));
                    } else {
                        // Unit variant in tagged union needs empty tuple for PyO3
                        code.push_str(&format!("    {}(),\r\n", variant_name));
                    }
                }
            }
        }
    } else {
        // Simple C-style enum
        code.push_str(&format!(
            "#[pyclass(name = \"{}\", module = \"azul\", eq, eq_int{})]\r\n",
            class_name, unsendable
        ));
        code.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\r\n");
        code.push_str("#[repr(C)]\r\n");
        code.push_str(&format!("pub enum {} {{\r\n", enum_name));

        if let Some(enum_fields) = &class_data.enum_fields {
            for variant_map in enum_fields {
                for (variant_name, _) in variant_map {
                    code.push_str(&format!("    {},\r\n", variant_name));
                }
            }
        }
    }

    code.push_str("}\r\n\r\n");
    code
}

// // clone/drop implementations
//
fn generate_clone_impl(class_name: &str, class_data: &ClassData, prefix: &str) -> String {
    let mut code = String::new();
    let type_name = format!("{}{}", prefix, class_name);

    // Handle opaque wrapper types specially
    if is_opaque_wrapper_type(class_name) {
        // For opaque wrappers, clone the inner C-API type
        code.push_str(&format!("impl Clone for {} {{\r\n", type_name));
        code.push_str("    fn clone(&self) -> Self {\r\n");
        code.push_str("        Self { inner: self.inner.clone() }\r\n");
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");
        return code;
    }

    // Check if type has custom Clone (needs C-API deepCopy function)
    let has_custom_clone = class_data.has_custom_clone();

    // Get external path for transmute to core type
    // Skip if external path contains generics (< or >)
    let external_path = class_data.external.as_deref().unwrap_or("");
    let has_valid_external = !external_path.is_empty() && !external_path.contains('<');

    // For Python bindings, we always need Clone implementation
    // because PyO3 requires Clone for extract()
    if has_custom_clone {
        // Use C-API deepCopy function
        code.push_str(&format!("impl Clone for {} {{\r\n", type_name));
        code.push_str("    fn clone(&self) -> Self {\r\n");
        code.push_str(&format!(
            "        unsafe {{ \
             mem::transmute(crate::ffi::dll::{}{}_deepCopy(mem::transmute(self))) }}\r\n",
            prefix, class_name
        ));
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");
    } else if has_valid_external {
        // Use the same pattern as dll_api.rs: transmute to core type, call clone, transmute back
        // This works because the core type implements Clone
        code.push_str(&format!("impl Clone for {} {{\r\n", type_name));
        code.push_str("    fn clone(&self) -> Self {\r\n");
        code.push_str(&format!(
            "        unsafe {{ core::mem::transmute::<{}, {}>((*(self as *const {} as *const \
             {})).clone()) }}\r\n",
            external_path, type_name, type_name, external_path
        ));
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");
    } else {
        // No valid external path - use byte-copy via ptr::read
        // This is safe for repr(C) types without Drop
        code.push_str(&format!("impl Clone for {} {{\r\n", type_name));
        code.push_str("    fn clone(&self) -> Self {\r\n");
        code.push_str("        unsafe { core::ptr::read(self) }\r\n");
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");
    }

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
/// - If type has Debug in derive or custom_impl in api.json: transmute to C-API and use that
/// - Otherwise: generate a simple implementation that just prints the type name
fn generate_debug_impl(class_name: &str, class_data: &ClassData, prefix: &str) -> String {
    let mut code = String::new();
    let type_name = format!("{}{}", prefix, class_name);

    // Handle opaque wrapper types specially
    if is_opaque_wrapper_type(class_name) {
        code.push_str(&format!("impl core::fmt::Debug for {} {{\r\n", type_name));
        code.push_str("    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {\r\n");
        code.push_str(&format!("        write!(f, \"{}(opaque)\")\r\n", class_name));
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");
        return code;
    }

    // Check if Debug is derived in api.json
    let has_derive_debug = class_data
        .derive
        .as_ref()
        .map(|d| d.iter().any(|t| t == "Debug"))
        .unwrap_or(false);

    // Check if Debug is in custom_impls
    let has_custom_debug = class_data
        .custom_impls
        .as_ref()
        .map(|c| c.iter().any(|t| t == "Debug"))
        .unwrap_or(false);

    // If type has Debug via derive or custom_impl, use transmute to C-API type
    if has_derive_debug || has_custom_debug {
        code.push_str(&format!("impl core::fmt::Debug for {} {{\r\n", type_name));
        code.push_str("    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {\r\n");
        code.push_str(&format!(
            "        let capi: &crate::ffi::dll::{} = unsafe {{ mem::transmute(self) }};\r\n",
            type_name
        ));
        code.push_str("        core::fmt::Debug::fmt(capi, f)\r\n");
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");
    } else {
        // Generate a simple Debug impl that just prints the type name
        code.push_str(&format!("impl core::fmt::Debug for {} {{\r\n", type_name));
        code.push_str("    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {\r\n");
        code.push_str(&format!("        write!(f, \"{}\")\r\n", class_name));
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");
    }

    code
}

fn generate_drop_impl(class_name: &str, class_data: &ClassData, prefix: &str) -> String {
    let mut code = String::new();
    let type_name = format!("{}{}", prefix, class_name);

    // Handle opaque wrapper types specially - delegate to inner's Drop
    if is_opaque_wrapper_type(class_name) {
        // No explicit drop needed - inner type handles its own drop
        return code;
    }

    // Check if type has custom destructor
    let has_custom_destructor = class_data.has_custom_drop();

    // Check if type has Copy derive (no Drop needed)
    let has_copy = class_data
        .derive
        .as_ref()
        .map(|d| d.iter().any(|t| t == "Copy"))
        .unwrap_or(false);

    if has_copy || !has_custom_destructor {
        return code;
    }

    code.push_str(&format!("impl Drop for {} {{\r\n", type_name));
    code.push_str("    fn drop(&mut self) {\r\n");
    code.push_str(&format!(
        "        unsafe {{ crate::ffi::dll::{}{}_delete(mem::transmute(self)); }}\r\n",
        prefix, class_name
    ));
    code.push_str("    }\r\n");
    code.push_str("}\r\n\r\n");

    code
}

// pymethods generation
fn generate_struct_pymethods(
    class_name: &str,
    class_data: &ClassData,
    prefix: &str,
    version_data: &VersionData,
) -> String {
    let mut code = String::new();
    let struct_name = format!("{}{}", prefix, class_name);

    // Handle opaque wrapper types - only generate __str__ and __repr__
    if is_opaque_wrapper_type(class_name) {
        code.push_str(&format!("#[pymethods]\r\nimpl {} {{\r\n", struct_name));
        code.push_str("    fn __str__(&self) -> String {\r\n");
        code.push_str(&format!("        format!(\"{}(opaque)\")\r\n", class_name));
        code.push_str("    }\r\n\r\n");
        code.push_str("    fn __repr__(&self) -> String {\r\n");
        code.push_str("        self.__str__()\r\n");
        code.push_str("    }\r\n");
        code.push_str("}\r\n\r\n");
        return code;
    }

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
) -> String {
    let mut code = String::new();
    let enum_name = format!("{}{}", prefix, class_name);

    let is_union = class_data
        .enum_fields
        .as_ref()
        .map(|f| enum_is_union(f))
        .unwrap_or(false);

    code.push_str(&format!("#[pymethods]\r\nimpl {} {{\r\n", enum_name));

    // For tagged unions, generate variant constructors
    if is_union {
        if let Some(enum_fields) = &class_data.enum_fields {
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
                        // Check if the variant type is a callback, VecRef, or type_alias with
                        // pointer
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

                        let py_type = rust_type_to_python_type(variant_type, prefix, version_data);
                        code.push_str("    #[staticmethod]\r\n");
                        code.push_str(&format!(
                            "    fn {}(v: {}) -> Self {{\r\n",
                            variant_name, py_type
                        ));
                        code.push_str(&format!("        Self::{}(v)\r\n", variant_name));
                        code.push_str("    }\r\n\r\n");
                    } else {
                        code.push_str("    #[staticmethod]\r\n");
                        code.push_str(&format!("    fn {}() -> Self {{\r\n", variant_name));
                        code.push_str(&format!("        Self::{}()\r\n", variant_name));
                        code.push_str("    }\r\n\r\n");
                    }
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

    // Parameters - use ref_kind to get proper pointer types
    for field_map in struct_fields {
        for (field_name, field_data) in field_map {
            let field_type = rust_type_to_python_type_with_ref(
                &field_data.r#type,
                field_data.ref_kind.clone(),
                prefix,
                version_data,
            );
            code.push_str(&format!("        {}: {},\r\n", field_name, field_type));
        }
    }

    code.push_str("    ) -> Self {\r\n");
    code.push_str("        Self {\r\n");

    for field_map in struct_fields {
        for (field_name, _) in field_map {
            code.push_str(&format!("            {},\r\n", field_name));
        }
    }

    code.push_str("        }\r\n");
    code.push_str("    }\r\n\r\n");

    code
}

fn generate_function(
    class_name: &str,
    fn_name: &str,
    fn_data: &FunctionData,
    prefix: &str,
    version_data: &VersionData,
    is_constructor: bool,
) -> String {
    let mut code = String::new();

    // Get self type
    let (self_param, self_call) = get_self_type(fn_data);
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

    // Other parameters
    let mut first_param = is_static;
    for arg_map in &fn_data.fn_args {
        for (arg_name, arg_type) in arg_map {
            if arg_name == "self" {
                continue;
            }
            if !first_param {
                code.push_str(", ");
            }
            first_param = false;
            let py_type = rust_type_to_python_type(arg_type, prefix, version_data);
            code.push_str(&format!("{}: {}", arg_name, py_type));
        }
    }

    code.push_str(")");

    // Return type
    if let Some(ret) = &fn_data.returns {
        let ret_type = rust_type_to_python_type(&ret.r#type, prefix, version_data);
        code.push_str(&format!(" -> {}", ret_type));
    } else if is_constructor {
        code.push_str(" -> Self");
    }

    code.push_str(" {\r\n");

    // Function body - call C-API
    // Note: C-API uses camelCase for function names (e.g., setNodeType not set_node_type)
    let c_fn_name = format!(
        "{}{}_{}",
        prefix,
        class_name,
        snake_case_to_lower_camel(fn_name)
    );

    code.push_str("        unsafe {\r\n");

    // Build call expression
    let mut call = format!("crate::ffi::dll::{}(", c_fn_name);

    // Self argument
    if !is_static {
        call.push_str(&self_call);
    }

    // Other arguments
    let mut first_arg = is_static;
    for arg_map in &fn_data.fn_args {
        for (arg_name, _) in arg_map {
            if arg_name == "self" {
                continue;
            }
            if !first_arg {
                call.push_str(", ");
            }
            first_arg = false;
            call.push_str(&format!("mem::transmute({})", arg_name));
        }
    }

    call.push_str(")");

    // Transmute result back
    if fn_data.returns.is_some() || is_constructor {
        code.push_str(&format!("            mem::transmute({})\r\n", call));
    } else {
        code.push_str(&format!("            {};\r\n", call));
    }

    code.push_str("        }\r\n");
    code.push_str("    }\r\n\r\n");

    code
}

fn get_self_type(fn_data: &FunctionData) -> (String, String) {
    for arg_map in &fn_data.fn_args {
        if let Some(self_type) = arg_map.get("self") {
            return match self_type.as_str() {
                "ref" => ("&self".to_string(), "mem::transmute(self)".to_string()),
                "refmut" => ("&mut self".to_string(), "mem::transmute(self)".to_string()),
                // For by-value self, we need to clone and transmute the cloned value
                "value" => (
                    "&self".to_string(),
                    "mem::transmute(self.clone())".to_string(),
                ),
                "mut value" => (
                    "&self".to_string(),
                    "mem::transmute(self.clone())".to_string(),
                ),
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
    // Use C-API types (crate::ffi::dll::Az*) for the signature so it matches
    // what the C-API callback type expects
    let info_type_az = format!("{}{}", prefix, cb_sig.info_type);
    let return_type_az = format!("{}{}", prefix, cb_sig.return_type);

    // C-API types for the function signature
    let capi_return_type = format!("crate::ffi::dll::{}{}", prefix, cb_sig.return_type);
    let capi_info_type = format!("crate::ffi::dll::{}{}", prefix, cb_sig.info_type);

    // Also need C-API RefAny type
    let capi_refany_type = "crate::ffi::dll::AzRefAny";

    // Build extra args for signature (using C-API types)
    let mut extra_args_sig = String::new();
    for (name, type_name, _ref_kind, _ext_path) in cb_sig.extra_args.iter() {
        // All callback args are now by-value
        let capi_arg_type = if is_primitive_arg(type_name) {
            type_name.clone()
        } else {
            format!("crate::ffi::dll::{}{}", prefix, type_name)
        };
        extra_args_sig.push_str(&format!(", {}: {}", name, capi_arg_type));
    }

    code.push_str(&format!(
        "/// Trampoline for {} - called by C-API, invokes Python\r\n",
        class_name
    ));
    // Callbacks now take by-value arguments using C-API types
    code.push_str(&format!(
        "extern \"C\" fn {}(\r\n    data: {},\r\n    info: {}{}\r\n) -> {} {{\r\n",
        trampoline_name, capi_refany_type, capi_info_type, extra_args_sig, capi_return_type
    ));

    // Default value - construct C-API types
    let default_expr = match cb_sig.return_type.as_str() {
        "Update" => format!("{}::DoNothing", capi_return_type),
        "OnTextInputReturn" => format!(
            "{} {{ update: crate::ffi::dll::AzUpdate::DoNothing, valid: \
             crate::ffi::dll::AzTextInputValid::Yes }}",
            capi_return_type
        ),
        "()" => "()".to_string(),
        _ => format!("unsafe {{ mem::zeroed() }}",),
    };
    code.push_str(&format!("    let default = {};\r\n\r\n", default_expr));

    // Transmute C-API RefAny to core RefAny to use downcast_mut
    code.push_str(
        "    let mut data_core: azul_core::refany::RefAny = unsafe { mem::transmute(data) };\r\n",
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
    // info is now by-value, so we transmute the value directly
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
        // All args are by-value now
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
    code.push_str("                    Ok(ret) => unsafe { mem::transmute(ret) },\r\n");
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
    let cb_struct_name = format!("{}{}", prefix, cb_type);

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
        "    pub inner: crate::ffi::dll::{},\r\n",
        struct_name
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
    code.push_str("        Ok(Self {\r\n");
    code.push_str(&format!(
        "            inner: crate::ffi::dll::{} {{\r\n",
        struct_name
    ));
    code.push_str(&format!(
        "                {}: crate::ffi::dll::{} {{\r\n",
        cb_field_name, cb_struct_name
    ));
    code.push_str(&format!("                    cb: {},\r\n", trampoline_name));
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

    // Add manually implemented classes
    code.push_str("    // Manual implementations\r\n");
    code.push_str("    m.add_class::<AzApp>()?;\r\n");
    code.push_str("    m.add_class::<AzLayoutCallbackInfo>()?;\r\n");
    code.push_str("    m.add_class::<AzWindowCreateOptions>()?;\r\n");
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
/// Types that are imported from C-API but have manual IntoPyObject/FromPyObject impls
/// They should use just Az{type} without path since they're already imported
const IMPORTED_CAPI_TYPES: &[&str] = &[
    "String",    // AzString - imported, has IntoPyObject impl
    "U8Vec",     // AzU8Vec - imported, has IntoPyObject impl
    "StringVec", // AzStringVec - imported, has IntoPyObject impl
];

/// Convert a Rust type to Python-compatible type name with explicit ref_kind
fn rust_type_to_python_type_with_ref(
    rust_type: &str,
    ref_kind: RefKind,
    prefix: &str,
    version_data: &VersionData,
) -> String {
    let base = rust_type_to_python_type(rust_type, prefix, version_data);
    match ref_kind {
        RefKind::ConstPtr => format!("*const {}", base),
        RefKind::MutPtr => format!("*mut {}", base),
        RefKind::Ref => format!("&{}", base),
        RefKind::RefMut => format!("&mut {}", base),
        RefKind::Boxed => format!("Box<{}>", base),
        RefKind::OptionBoxed => format!("Option<Box<{}>>", base),
        RefKind::Value => base,
    }
}

/// Convert a Rust type to Python-compatible type name
/// Resolves type aliases to their underlying types
fn rust_type_to_python_type(rust_type: &str, prefix: &str, version_data: &VersionData) -> String {
    // Convert *const c_void and *mut c_void to usize for Python compatibility
    let trimmed = rust_type.trim();
    if trimmed == "*const c_void" || trimmed == "* const c_void" 
        || trimmed == "*mut c_void" || trimmed == "* mut c_void" {
        return "usize".to_string();
    }

    let (ptr_prefix, base_type, array_suffix) = analyze_type(rust_type);

    // If the base type is c_void with a pointer prefix, convert to usize
    if base_type == "c_void" && (ptr_prefix.contains("const") || ptr_prefix.contains("mut")) {
        return "usize".to_string();
    }

    if is_primitive_arg(&base_type) {
        return format!("{}{}{}", ptr_prefix, base_type, array_suffix);
    }

    // For types that are imported from C-API with IntoPyObject impls, use just Az{type}
    // without the crate::ffi::dll:: path since they're already imported
    if IMPORTED_CAPI_TYPES.contains(&base_type.as_str()) {
        return format!(
            "{}Az{}{}",
            ptr_prefix, base_type, array_suffix
        );
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
