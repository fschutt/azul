use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use anyhow::Context;
use indexmap::IndexMap; // Use IndexMap for ordered fields where necessary
use serde_derive::{Deserialize, Serialize}; // Use BTreeMap for sorted keys (versions)

// Helper function to check if a bool is false (for skip_serializing_if)
fn is_false(b: &bool) -> bool {
    !b
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ApiData(
    // BTreeMap ensures versions are sorted alphabetically/numerically by key.
    pub BTreeMap<String, VersionData>,
);

impl ApiData {
    // Helper to get sorted version strings
    pub fn get_sorted_versions(&self) -> Vec<String> {
        self.0.keys().cloned().collect() // BTreeMap keys are already sorted
    }

    // Helper to get data for a specific version by its string name
    pub fn get_version(&self, version: &str) -> Option<&VersionData> {
        self.0.get(version)
    }

    // Helper to get the latest version string (assuming BTreeMap sorting works)
    pub fn get_latest_version_str(&self) -> Option<&str> {
        self.0.keys().last().map(|s| s.as_str())
    }

    /// Get versions sorted by date (oldest first)
    /// Returns list of (version_name, version_index) where index is 1-based
    /// Oldest version gets index 1 (Az1), second oldest gets index 2 (Az2), etc.
    pub fn get_versions_by_date(&self) -> Vec<(String, usize)> {
        let mut versions: Vec<_> = self.0.iter().collect();

        // Sort by date (oldest first)
        versions.sort_by(|a, b| a.1.date.cmp(&b.1.date));

        // Return with 1-based index
        versions
            .into_iter()
            .enumerate()
            .map(|(idx, (name, _))| (name.clone(), idx + 1))
            .collect()
    }

    /// Get the prefix for a specific version (e.g., "Az1", "Az2", etc.)
    /// Based on the version's position when sorted by date
    pub fn get_version_prefix(&self, version_name: &str) -> Option<String> {
        // Always return "Az" without version number
        if self.0.contains_key(version_name) {
            Some("Az".to_string())
        } else {
            None
        }
    }

    /// Get the default prefix (uses "Az" without version number)
    pub fn get_default_prefix() -> &'static str {
        "Az"
    }

    // Search across all versions and modules for a class definition by name.
    // Returns Option<(version_str, module_name, class_name, &ClassData)>
    pub fn find_class_definition<'a>(
        &'a self,
        search_class_name: &str,
    ) -> Option<(&'a str, &'a str, &'a str, &'a ClassData)> {
        for (version_str, version_data) in &self.0 {
            if let Some((module_name, class_name, class_data)) =
                version_data.find_class(search_class_name)
            {
                return Some((version_str, module_name, class_name, class_data));
            }
        }
        None
    }

    /// Create ApiData from a JSON string
    pub fn from_str(json_str: &str) -> anyhow::Result<Self> {
        serde_json::from_str(json_str).map_err(|e| anyhow::anyhow!("JSON parse error: {}", e))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VersionData {
    /// Required: all API calls specific to this version are going to be prefixed with
    /// "Az[version]"
    pub apiversion: usize,
    /// Required: git revision hash, so that we know which tag this version was deployed from
    pub git: String,
    /// Required: release date
    pub date: String,
    /// Examples to view on the frontpage
    #[serde(default)]
    pub examples: Vec<Example>,
    /// Release notes as GitHub Markdown (used both on the website and on the GitHub release page)
    #[serde(default)]
    pub notes: Vec<String>,
    // Using IndexMap to preserve module order as read from JSON
    pub api: IndexMap<String, ModuleData>,
}

pub type OsId = String;
pub type ImageFilePathRelative = String;
pub type ExampleSrcFileRelative = String;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Example {
    pub name: String,
    pub alt: String,
    pub code: LangDepFilesPaths,
    pub screenshot: OsDepFilesPaths,
    pub description: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LangDepFilesPaths {
    pub c: String,
    pub cpp: String,
    pub rust: String,
    pub python: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OsDepFilesPaths {
    pub windows: String,
    pub linux: String,
    pub mac: String,
}

impl Example {
    pub fn load(
        &self,
        filerelativepath: &str,
        imagerelativepath: &str,
    ) -> anyhow::Result<LoadedExample> {
        Ok(LoadedExample {
            name: self.name.clone(),
            alt: self.alt.clone(),
            description: self.description.clone(),
            code: LangDepFiles {
                c: std::fs::read(&Path::new(filerelativepath).join(&self.code.c)).context(
                    format!("failed to load c code for example {}", self.name.clone()),
                )?,
                cpp: std::fs::read(&Path::new(filerelativepath).join(&self.code.cpp)).context(
                    format!("failed to load cpp code for example {}", self.name.clone()),
                )?,
                rust: std::fs::read(&Path::new(filerelativepath).join(&self.code.rust)).context(
                    format!("failed to load rust code for example {}", self.name.clone()),
                )?,
                python: std::fs::read(&Path::new(filerelativepath).join(&self.code.python))
                    .context(format!(
                        "failed to load python code for example {}",
                        self.name.clone()
                    ))?,
            },
            screenshot: OsDepFiles {
                windows: std::fs::read(
                    &Path::new(imagerelativepath).join(&self.screenshot.windows),
                )
                .context(format!(
                    "failed to load windows screenshot for example {}",
                    self.name.clone()
                ))?,
                linux: std::fs::read(&Path::new(imagerelativepath).join(&self.screenshot.linux))
                    .context(format!(
                        "failed to load linux screenshot for example {}",
                        self.name.clone()
                    ))?,
                mac: std::fs::read(&Path::new(imagerelativepath).join(&self.screenshot.mac))
                    .context(format!(
                        "failed to load mac screenshot for example {}",
                        self.name.clone()
                    ))?,
            },
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoadedExample {
    /// Id of the examples
    pub name: String,
    /// Short description of the image
    pub alt: String,
    /// Markdown description of the example
    pub description: Vec<String>,
    /// Code example loaded to string
    pub code: LangDepFiles,
    /// Image file loaded to string
    pub screenshot: OsDepFiles,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OsDepFiles {
    pub windows: Vec<u8>,
    pub linux: Vec<u8>,
    pub mac: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LangDepFiles {
    pub c: Vec<u8>,
    pub cpp: Vec<u8>,
    pub rust: Vec<u8>,
    pub python: Vec<u8>,
}

impl VersionData {
    // Find a class definition within this specific version.
    // Returns Option<(module_name, class_name, &ClassData)>
    pub fn find_class<'a>(
        &'a self,
        search_class_name: &str,
    ) -> Option<(&'a str, &'a str, &'a ClassData)> {
        for (module_name, module_data) in &self.api {
            if let Some((class_name, class_data)) = module_data.find_class(search_class_name) {
                return Some((module_name.as_str(), class_name, class_data));
            }
        }
        None
    }

    // Get a specific class if module and class name are known for this version.
    pub fn get_class(&self, module_name: &str, class_name: &str) -> Option<&ClassData> {
        self.api.get(module_name)?.classes.get(class_name)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModuleData {
    pub doc: Option<String>,
    // Using IndexMap to preserve class order within a module
    pub classes: IndexMap<String, ClassData>,
}

impl ModuleData {
    // Find a class within this specific module.
    // Returns Option<(class_name, &ClassData)>
    pub fn find_class<'a>(&'a self, search_class_name: &str) -> Option<(&'a str, &'a ClassData)> {
        self.classes
            .iter()
            .find(|(name, _)| *name == search_class_name)
            .map(|(name, data)| (name.as_str(), data))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ClassData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")] // Skip if false
    pub is_boxed_object: bool,
    /// Traits with manual `impl Trait for Type` blocks (e.g., ["Clone", "Drop"])
    /// These require DLL functions like `AzTypeName_deepCopy` and `AzTypeName_delete`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_impls: Option<Vec<String>>,
    // DEPRECATED: Use custom_impls: ["Clone"] instead. Kept for backwards compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone: Option<bool>,
    // DEPRECATED: Use custom_impls: ["Drop"] instead. Kept for backwards compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_destructor: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derive: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serde: Option<String>, // Serde attributes like "transparent"
    // Renamed from "const" which is a keyword
    #[serde(rename = "const", default, skip_serializing_if = "Option::is_none")]
    pub const_value_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constants: Option<Vec<IndexMap<String, ConstantData>>>, /* Use IndexMap if field order
                                                                 * matters */
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub struct_fields: Option<Vec<IndexMap<String, FieldData>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enum_fields: Option<Vec<IndexMap<String, EnumVariantData>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub callback_typedef: Option<CallbackDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    // Using IndexMap to preserve function/constructor order
    pub constructors: Option<IndexMap<String, FunctionData>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub functions: Option<IndexMap<String, FunctionData>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_patches: Option<Vec<String>>, // For conditional patch application
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repr: Option<String>, // For things like #[repr(transparent)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic_params: Option<Vec<String>>, // e.g., ["T", "U"] for generic types
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_alias: Option<TypeAliasInfo>, // Information about type aliases
}

impl ClassData {
    /// Check if this type has a custom `impl Clone` (not #[derive(Clone)])
    /// Returns true if custom_impls contains "Clone" OR legacy clone field is Some(false)
    pub fn has_custom_clone(&self) -> bool {
        if let Some(ref impls) = self.custom_impls {
            if impls.iter().any(|s| s == "Clone") {
                return true;
            }
        }
        // Legacy: clone: false means "don't derive Clone" which implies custom impl needed
        // clone: true or None means "derive Clone automatically"
        false
    }

    /// Check if this type has a custom `impl Drop` (not automatic)
    /// Returns true if custom_impls contains "Drop" OR legacy custom_destructor is true
    pub fn has_custom_drop(&self) -> bool {
        if let Some(ref impls) = self.custom_impls {
            if impls.iter().any(|s| s == "Drop") {
                return true;
            }
        }
        // Legacy: custom_destructor: true means custom Drop impl
        self.custom_destructor.unwrap_or(false)
    }

    /// Check if Clone can be derived automatically (not custom impl)
    /// Returns true if no custom Clone impl exists
    pub fn can_derive_clone(&self) -> bool {
        !self.has_custom_clone() && self.clone.unwrap_or(true)
    }

    /// Check if a specific trait has a custom implementation
    pub fn has_custom_impl(&self, trait_name: &str) -> bool {
        if let Some(ref impls) = self.custom_impls {
            return impls.iter().any(|s| s == trait_name);
        }
        // Handle legacy fields
        match trait_name {
            "Drop" => self.custom_destructor.unwrap_or(false),
            "Clone" => false, // Legacy clone field doesn't indicate custom impl
            _ => false,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConstantData {
    pub r#type: String, // r# to allow "type" as field name
    pub value: String,  // Keep value as string, parsing depends on type context
}

/// Information about a type alias, including generic instantiation
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct TypeAliasInfo {
    /// The target generic type (e.g., "CssPropertyValue")
    pub target: String,
    /// Generic arguments for instantiation (e.g., ["LayoutZIndex"])
    pub generic_args: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct FieldData {
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub derive: Option<Vec<String>>, // For field-level derives like #[pyo3(get, set)]
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct EnumVariantData {
    // Variants might not have an associated type (e.g., simple enums like MsgBoxIcon)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct FunctionData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
    // Arguments are a list where each item is a map like {"arg_name": "type"}
    // Using IndexMap here preserves argument order.
    #[serde(default, rename = "fn_args")]
    pub fn_args: Vec<IndexMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<ReturnTypeData>,
    #[serde(rename = "fn_body", default, skip_serializing_if = "Option::is_none")]
    pub fn_body: Option<String>, // Present in api.json for DLL generation
    #[serde(
        default,
        rename = "use_patches",
        skip_serializing_if = "Option::is_none"
    )]
    pub use_patches: Option<Vec<String>>, // Which languages this patch applies to
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReturnTypeData {
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CallbackDefinition {
    #[serde(default, rename = "fn_args")]
    pub fn_args: Vec<CallbackArgData>,
    pub returns: Option<ReturnTypeData>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CallbackArgData {
    #[serde(rename = "type")]
    pub r#type: String,
    #[serde(rename = "ref")]
    pub ref_kind: String, // "ref", "refmut", "value"
    pub doc: Option<String>,
}

// --- HELPER FUNCTIONS BELOW ---
//
// Helper functions to traverse complex API structures and extract type references
//
// The API structures are deeply nested with Vec<IndexMap<>>, Option<>, etc.
// These helpers make it easy to extract all type references for recursive discovery.

/// Extract all type references from a ClassData
pub fn extract_types_from_class_data(class_data: &ClassData) -> HashSet<String> {
    let mut types = HashSet::new();

    // Extract from struct fields
    if let Some(struct_fields) = &class_data.struct_fields {
        for field_map in struct_fields {
            for (_field_name, field_data) in field_map {
                types.extend(extract_types_from_field_data(field_data));
            }
        }
    }

    // Extract from enum variants
    if let Some(enum_fields) = &class_data.enum_fields {
        for variant_map in enum_fields {
            for (_variant_name, variant_data) in variant_map {
                types.extend(extract_types_from_enum_variant(variant_data));
            }
        }
    }

    // Extract from functions
    if let Some(functions) = &class_data.functions {
        for (_fn_name, fn_data) in functions {
            types.extend(extract_types_from_function_data(fn_data));
        }
    }

    // Extract from callback_typedef
    if let Some(callback_def) = &class_data.callback_typedef {
        types.extend(extract_types_from_callback_definition(callback_def));
    }

    // Extract from type_alias generic arguments
    if let Some(type_alias) = &class_data.type_alias {
        // Add the target type (e.g., CssPropertyValue)
        if let Some(base_type) = extract_base_type_if_not_opaque(&type_alias.target) {
            types.insert(base_type);
        }
        // Add all generic arguments (e.g., LayoutZIndex from CssPropertyValue<LayoutZIndex>)
        for generic_arg in &type_alias.generic_args {
            if let Some(base_type) = extract_base_type_if_not_opaque(generic_arg) {
                types.insert(base_type);
            }
        }
    }

    types
}

/// Extract type from FieldData
/// Skips types behind pointers (they don't need to be in the API)
pub fn extract_types_from_field_data(field_data: &FieldData) -> HashSet<String> {
    let mut types = HashSet::new();

    // Skip types behind pointers - they're opaque and don't need to be exposed
    if let Some(base_type) = extract_base_type_if_not_opaque(&field_data.r#type) {
        types.insert(base_type);
    }

    types
}

/// Extract types from EnumVariantData
/// Skips types behind pointers
pub fn extract_types_from_enum_variant(variant_data: &EnumVariantData) -> HashSet<String> {
    let mut types = HashSet::new();

    if let Some(variant_type) = &variant_data.r#type {
        if let Some(base_type) = extract_base_type_if_not_opaque(variant_type) {
            types.insert(base_type);
        }
    }

    types
}

/// Extract types from FunctionData
/// Skips types behind pointers
pub fn extract_types_from_function_data(fn_data: &FunctionData) -> HashSet<String> {
    let mut types = HashSet::new();

    // Extract return type
    if let Some(return_data) = &fn_data.returns {
        types.extend(extract_types_from_return_data(return_data));
    }

    // Extract parameter types
    // fn_args is Vec<IndexMap<String, String>> where key=name, value=type
    for arg_map in &fn_data.fn_args {
        for (_param_name, param_type) in arg_map {
            if let Some(base_type) = extract_base_type_if_not_opaque(param_type) {
                types.insert(base_type);
            }
        }
    }

    types
}

/// Extract types from CallbackDefinition
/// Skips types behind pointers
pub fn extract_types_from_callback_definition(
    callback_def: &CallbackDefinition,
) -> HashSet<String> {
    let mut types = HashSet::new();

    // Extract return type
    if let Some(return_data) = &callback_def.returns {
        types.extend(extract_types_from_return_data(return_data));
    }

    // Extract parameter types
    for arg_data in &callback_def.fn_args {
        if let Some(base_type) = extract_base_type_if_not_opaque(&arg_data.r#type) {
            types.insert(base_type);
        }
    }

    types
}

/// Extract type from ReturnTypeData
/// Skips types behind pointers
pub fn extract_types_from_return_data(return_data: &ReturnTypeData) -> HashSet<String> {
    let mut types = HashSet::new();

    if let Some(base_type) = extract_base_type_if_not_opaque(&return_data.r#type) {
        types.insert(base_type);
    }

    types
}

/// Extract base type from a type string (removes Vec, Option, Box, etc.)
///
/// Examples:
/// - "Vec<Foo>" -> "Foo"
/// - "Option<Bar>" -> "Bar"
/// - "*const Baz" -> "Baz"
/// - "&mut Qux" -> "Qux"
pub fn extract_base_type(type_str: &str) -> String {
    let trimmed = type_str.trim();

    // Handle generic types like Vec<T>, Option<T>, Box<T>, etc.
    if let Some(start) = trimmed.find('<') {
        if let Some(end) = trimmed.rfind('>') {
            let inner = &trimmed[start + 1..end];
            // Recursively extract from inner type
            return extract_base_type(inner);
        }
    }

    // Handle pointer types
    if let Some(rest) = trimmed.strip_prefix("*const ") {
        return extract_base_type(rest);
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return extract_base_type(rest);
    }

    // Handle reference types
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return extract_base_type(rest);
    }
    if let Some(rest) = trimmed.strip_prefix('&') {
        return extract_base_type(rest);
    }

    // Handle tuple types - extract first element
    if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if let Some(comma_pos) = inner.find(',') {
            return extract_base_type(&inner[..comma_pos]);
        }
        return extract_base_type(inner);
    }

    trimmed.to_string()
}

/// Check if a type is behind a pointer or smart pointer wrapper
/// These types don't need to be exposed in the API because they're opaque
pub fn is_behind_pointer(type_str: &str) -> bool {
    let trimmed = type_str.trim();

    // Raw pointers
    if trimmed.starts_with("*const ") || trimmed.starts_with("*mut ") {
        return true;
    }

    // References (usually opaque in FFI)
    if trimmed.starts_with("&") {
        return true;
    }

    // Smart pointers that make types opaque
    let opaque_wrappers = [
        "Box<", "Arc<", "Rc<", "Weak<", "Mutex<", "RwLock<", "RefCell<", "Cell<",
    ];

    for wrapper in &opaque_wrappers {
        if trimmed.starts_with(wrapper) {
            return true;
        }
    }

    false
}

/// Primitive types that should never be added to the API as classes
/// These are built-in language types that don't need Az prefix
const PRIMITIVE_TYPES: &[&str] = &[
    "bool", "f32", "f64", "fn", "i128", "i16", "i32", "i64", "i8", "isize", 
    "slice", "u128", "u16", "u32", "u64", "u8", "()", "usize", "c_void",
    "str", "char", "c_char", "c_schar", "c_uchar",
];

/// Single-letter types are usually generic type parameters
fn is_generic_type_param(type_name: &str) -> bool {
    type_name.len() == 1 && type_name.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
}

/// Extract base type from a type string (removes Vec, Option, Box, etc.)
/// BUT: If the type is behind a pointer/smart pointer, return None
/// Also returns None for primitive types (they shouldn't be added to API)
pub fn extract_base_type_if_not_opaque(type_str: &str) -> Option<String> {
    if is_behind_pointer(type_str) {
        return None; // Don't follow types behind pointers
    }

    let base_type = extract_base_type(type_str);
    
    // Don't return primitive types - they're built-in and shouldn't be in API
    if PRIMITIVE_TYPES.contains(&base_type.as_str()) {
        return None;
    }
    
    // Don't return single-letter generic type parameters (T, U, V, etc.)
    if is_generic_type_param(&base_type) {
        return None;
    }
    
    Some(base_type)
}

/// Collect all type references from the entire API
pub fn collect_all_referenced_types_from_api(api_data: &crate::api::ApiData) -> HashSet<String> {
    let mut types = HashSet::new();

    // Include callback_typedefs - they can be referenced and need patches
    // (e.g. FooDestructorType is referenced from FooDestructor enum)
    for (_version_name, version_data) in &api_data.0 {
        for (_module_name, module_data) in &version_data.api {
            for (_class_name, class_data) in &module_data.classes {
                types.extend(extract_types_from_class_data(class_data));
            }
        }
    }

    types
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_base_type_simple() {
        assert_eq!(extract_base_type("Foo"), "Foo");
        assert_eq!(extract_base_type("  Bar  "), "Bar");
    }

    #[test]
    fn test_extract_base_type_generic() {
        assert_eq!(extract_base_type("Vec<Foo>"), "Foo");
        assert_eq!(extract_base_type("Option<Bar>"), "Bar");
        assert_eq!(extract_base_type("Box<Baz>"), "Baz");
    }

    #[test]
    fn test_extract_base_type_nested() {
        assert_eq!(extract_base_type("Vec<Option<Foo>>"), "Foo");
        assert_eq!(extract_base_type("Option<Box<Bar>>"), "Bar");
    }

    #[test]
    fn test_extract_base_type_pointers() {
        assert_eq!(extract_base_type("*const Foo"), "Foo");
        assert_eq!(extract_base_type("*mut Bar"), "Bar");
        assert_eq!(extract_base_type("&Baz"), "Baz");
        assert_eq!(extract_base_type("&mut Qux"), "Qux");
    }

    #[test]
    fn test_extract_base_type_complex() {
        assert_eq!(extract_base_type("*const Vec<Foo>"), "Foo");
        assert_eq!(extract_base_type("&Option<Bar>"), "Bar");
    }
}
