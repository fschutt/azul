//! Python extension generator v2
//!
//! Generates Python extension module code using PyO3, including:
//! - #[pyclass] wrapper structs (SEPARATE from C-API structs!)
//! - #[pymethods] impl blocks
//! - Callback trampolines for Python→Rust calls
//! - Type conversions for Python-specific types
//!
//! # Important Design Decision
//!
//! Python extension structs are generated **completely separately** from C-API structs.
//! This is intentional because:
//!
//! 1. **Different attributes**: Python uses `#[pyclass]`, C-API uses `#[repr(C)]`
//! 2. **Different trait implementations**: Python uses transmute to azul_core,
//!    C-API generates C-ABI functions
//! 3. **Type filtering**: Python skips recursive types and VecRef types
//! 4. **Callback handling**: Python needs trampolines to route Python callables
//!    to Rust callbacks, which C doesn't need
//!
//! The Python generator does NOT share any generated code with the C-API generator.
//! They both read from the same IR but produce completely independent output.
//!
//! # Type Classification
//!
//! Types are now classified via TypeCategory in the IR, not ad-hoc constants here.
//! See ir.rs TypeCategory enum for the central classification system.

use anyhow::Result;
use std::collections::BTreeSet;

use super::config::{CodegenConfig, PythonConfig};
use super::generator::CodeBuilder;
use super::generator::LanguageGenerator;
use super::ir::{
    ArgRefKind, CallbackArgInfo, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind,
    FunctionDef, FunctionKind,
    StructDef, TypeCategory,
};
use super::lang_rust::RustGenerator;
use crate::utils::analyze::analyze_type;

// ============================================================================
// Constants
// ============================================================================

/// Types that are C-API type aliases (not structs with .inner field)
/// These types use `type AzFoo = dll::AzFoo;` not `struct AzFoo { inner: ... }`
const CAPI_TYPE_ALIASES: &[&str] = &[
    "String",
    "U8Vec",
    "StringVec",
    "GLuintVec",
    "GLintVec",
    "RefAny",
    "U8VecDestructor",
    "StringVecDestructor",
    "InstantPtr",
    "StringMenuItem",
    "ParsedSvgXmlNode",
];

/// Check if a type is a C-API type alias (no .inner field)
fn is_capi_type_alias(type_name: &str) -> bool {
    CAPI_TYPE_ALIASES.contains(&type_name)
}

/// Replace `Name::` path prefixes inside a fn_body with their fully-qualified
/// external paths. Only replaces an occurrence when the `Name` is not preceded
/// by an identifier character (so `DomId::` is not corrupted while replacing
/// `Dom::`). `replacements` should be sorted longest-pattern-first.
fn replace_type_paths(body: &str, replacements: &[(String, String)]) -> String {
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let mut matched = false;
        // Only attempt a match at an identifier boundary.
        let prev_is_ident = i > 0
            && {
                let c = bytes[i - 1];
                // `:` guards against re-qualifying a name already part of a
                // fully-qualified path (e.g. `azul_core::icon::IconHandle::`).
                c == b'_' || c == b':' || c.is_ascii_alphanumeric()
            };
        if !prev_is_ident {
            for (pat, rep) in replacements {
                if body[i..].starts_with(pat.as_str()) {
                    out.push_str(rep);
                    i += pat.len();
                    matched = true;
                    break;
                }
            }
        }
        if !matched {
            // Push one full UTF-8 char to keep boundaries valid.
            let ch = body[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

// ============================================================================
// Python Generator
// ============================================================================

pub struct PythonGenerator;

impl PythonGenerator {
    /// Generate complete Python extension module code
    pub fn generate_python(&self, ir: &CodegenIR, config: &PythonConfig) -> Result<String> {
        let mut builder = CodeBuilder::new(&config.base.indent);

        // File header
        self.generate_header(&mut builder);

        // Generate inner DLL API module (C-API types for transmute)
        self.generate_inner_dll_module(&mut builder, ir, config)?;

        // Generate unsafe Send + Sync impls for Vec-like types
        self.generate_send_sync_impls(&mut builder, ir, config);

        // PyO3 imports
        self.generate_imports(&mut builder);

        // Note: GL type aliases (AzGLuint etc.) are already defined in __dll_api_inner::dll
        // and exported via `pub use __dll_api_inner::dll::*;`

        // Python patches (helper functions, conversions, trampolines)
        self.generate_python_patches(&mut builder, ir, config)?;

        // Python wrapper types
        self.generate_wrapper_types(&mut builder, ir, config)?;

        // Clone implementations
        self.generate_clone_impls(&mut builder, ir, config)?;

        // Debug implementations
        self.generate_debug_impls(&mut builder, ir, config)?;

        // Pymethods implementations
        self.generate_pymethods(&mut builder, ir, config)?;

        // Module registration
        self.generate_module_registration(&mut builder, ir, config)?;

        Ok(builder.finish())
    }

    fn generate_header(&self, builder: &mut CodeBuilder) {
        builder.line("// WARNING: autogenerated Python bindings for azul api");
        builder.line("// Generated for PyO3 v0.27.2 by azul-doc codegen v2");
        builder.line("// This file is included via include!() in dll/src/lib.rs");
        builder.line("// This file is STANDALONE and does NOT depend on the c-api feature.");
        builder.blank();
    }

    fn generate_imports(&self, builder: &mut CodeBuilder) {
        builder.line("use core::ffi::c_void;");
        // C numeric aliases from core::ffi. Kept AS aliases (not mapped to
        // i32/u32/f32/f64) so the binding stays ABI-correct on 32-bit and riscv
        // targets, where the GL shim's `c_int` width follows the platform C ABI.
        builder.line("#[allow(unused_imports)]");
        builder.line("use core::ffi::{c_int, c_uint, c_float, c_double};");
        builder.line("use core::mem;");
        builder.line("use pyo3::{pyclass, pymethods, pymodule, Bound, Py, PyResult};");
        builder.line("use pyo3::{Python, PyErr, FromPyObject};");
        builder.line("use pyo3::types::{PyAny, PyAnyMethods, PyBytes, PyList, PyModule, PyModuleMethods, PyString};");
        builder.line("use pyo3::exceptions::PyException;");
        builder.line("use pyo3::gc::{PyVisit, PyTraverseError};");
        builder.line("use pyo3::conversion::IntoPyObject;");
        builder.line("use pyo3::Borrowed;");
        // Bring extension traits into scope so fn_body calls that dispatch to
        // trait methods (e.g. `SvgMultiPolygon::tessellate_fill`, defined on the
        // `SvgMultiPolygonTessellation` trait rather than the type itself)
        // resolve. The `#[allow(unused_imports)]` keeps builds warning-clean when
        // no method in this file uses the trait.
        builder.line("#[allow(unused_imports)]");
        builder.line("use azul_layout::xml::svg::SvgMultiPolygonTessellation;");
        builder.blank();
    }

    /// Generate unsafe Send + Sync implementations for Vec-like types
    ///
    /// Vec types (U8Vec, StringVec, etc.) use internal pointers (*const T)
    /// but are semantically safe like Rust's Vec<T>. PyO3 requires Send
    /// for types used in pyclass, so we implement it manually.
    fn generate_send_sync_impls(
        &self,
        builder: &mut CodeBuilder,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) {
        let prefix = &config.base.type_prefix;

        builder.line(
            "// ============================================================================",
        );
        builder.line("// SEND + SYNC IMPLEMENTATIONS FOR SEND-SAFE TYPES");
        builder.line(
            "// ============================================================================",
        );
        builder.line("// These types use internal pointers but are semantically safe to Send");
        builder.blank();

        // Python-specific: Types that wrap & or Box<> and are semantically Send
        const PYTHON_SEND_SAFE_TYPES: &[&str] = &[
            "CssPropertyCachePtr",
            "VirtualViewCallbackInfo",
            "VirtualViewReturn",
            "StyledDom",
            "LayoutCallbackInfo",
            "CallbackInfo",
            "RenderImageCallbackInfo",
            "RefCount",
            "OptionRefAny",
            "GlVoidPtrMut",
            "ParsedSvg",
            "ResultParsedSvgSvgParseError",
            "GridMinMax",
            "GridTrackSizing",
        ];

        // Generate for Python-specific send-safe types
        for type_name in PYTHON_SEND_SAFE_TYPES {
            let full_type = format!("__dll_api_inner::dll::{}{}", prefix, type_name);
            builder.line(&format!("unsafe impl Send for {} {{}}", full_type));
            builder.line(&format!("unsafe impl Sync for {} {{}}", full_type));
        }

        // Generate for IR-marked send-safe types (vec module)
        for struct_def in &ir.structs {
            if struct_def.is_send_safe {
                let type_name = format!("__dll_api_inner::dll::{}{}", prefix, struct_def.name);
                builder.line(&format!("unsafe impl Send for {} {{}}", type_name));
                builder.line(&format!("unsafe impl Sync for {} {{}}", type_name));
            }
        }

        for enum_def in &ir.enums {
            if enum_def.is_send_safe {
                let type_name = format!("__dll_api_inner::dll::{}{}", prefix, enum_def.name);
                builder.line(&format!("unsafe impl Send for {} {{}}", type_name));
                builder.line(&format!("unsafe impl Sync for {} {{}}", type_name));
            }
        }

        builder.blank();
    }

    fn generate_inner_dll_module(
        &self,
        builder: &mut CodeBuilder,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) -> Result<()> {
        builder.line(
            "// ============================================================================",
        );
        builder.line("// GENERATED C-API TYPES (standalone, not imported from crate::ffi::dll)");
        builder.line(
            "// ============================================================================",
        );
        builder.blank();

        // Generate DLL API using RustGenerator with dll_types_only config
        // This includes types and trait impls (with transmute), but NO C-ABI functions
        // Python extension is standalone and doesn't need extern "C" fn AzFoo_bar() functions
        let dll_config = CodegenConfig::dll_types_only();
        let rust_gen = RustGenerator;
        let dll_code = rust_gen.generate(ir, &dll_config)?;

        builder.raw(&dll_code);
        builder.blank();

        Ok(())
    }

    fn generate_python_patches(
        &self,
        builder: &mut CodeBuilder,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) -> Result<()> {
        builder.line(
            "// ============================================================================",
        );
        builder.line("// AUTO-GENERATED PYTHON PATCHES");
        builder.line(
            "// ============================================================================",
        );
        builder.blank();

        // Note: No type aliases needed here - all types are available through
        // `pub use __dll_api_inner::dll::*;` (e.g., AzString, AzU8Vec, AzStringVec, etc.)

        self.generate_helper_functions(builder);
        self.generate_from_into_impls(builder);
        self.generate_pyo3_traits(builder);
        self.generate_callback_wrapper_types(builder);
        self.generate_callback_trampolines(builder, ir, config)?;

        Ok(())
    }

    fn generate_helper_functions(&self, builder: &mut CodeBuilder) {
        builder.line("// --- Helper functions for type conversion ---");
        builder.blank();

        builder.raw(
            r#"fn az_string_to_py_string(input: AzString) -> String {
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
    slice.iter().map(|s| {
        let bytes = unsafe { core::slice::from_raw_parts(s.vec.ptr, s.vec.len) };
        String::from_utf8_lossy(bytes).into_owned()
    }).collect()
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

"#,
        );
    }

    fn generate_from_into_impls(&self, builder: &mut CodeBuilder) {
        builder.line("// --- From/Into implementations for string/bytes types ---");
        builder.blank();

        builder.raw(
            r#"impl From<String> for AzString {
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

"#,
        );
    }

    fn generate_pyo3_traits(&self, builder: &mut CodeBuilder) {
        builder.line("// --- PyO3 conversion traits (FromPyObject, IntoPyObject) ---");
        builder.blank();

        builder.raw(
            r#"impl FromPyObject<'_, '_> for AzString {
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

"#,
        );
    }

    fn generate_callback_wrapper_types(&self, builder: &mut CodeBuilder) {
        builder.line("// --- Python Wrapper Types for RefAny ---");
        builder.blank();

        builder.raw(
            r#"/// Generic wrapper for Python user data stored in RefAny
#[repr(C)]
pub struct PyDataWrapper {
    pub _py_data: Option<Py<PyAny>>,
}

/// Wrapper for Python callable stored in the callback's `callable` field
#[repr(C)]
pub struct PyCallableWrapper {
    pub _py_callable: Option<Py<PyAny>>,
}

/// Generic wrapper for any Python object stored in RefAny
#[repr(C)]
pub struct PyObjectWrapper {
    pub py_obj: Py<PyAny>,
}

// --- Python JSON Serialization Support for RefAny ---

/// Trampoline for Python object serialization to JSON
/// 
/// This is called when `RefAny.serialize_to_json()` is invoked.
/// It checks for a custom `__az_to_json__` method first, then falls back
/// to using Python's `json.dumps()`.
extern "C" fn py_serialize_refany_trampoline(
    mut refany: azul_core::refany::RefAny
) -> azul_layout::json::Json {
    use azul_layout::json::Json;

    // Get the PyDataWrapper from RefAny
    let wrapper_opt = refany.downcast_ref::<PyDataWrapper>();
    let wrapper = match wrapper_opt {
        Some(w) => w,
        None => return Json::null(),
    };
    
    let py_data = match &wrapper._py_data {
        Some(d) => d,
        None => return Json::null(),
    };
    
    Python::with_gil(|py| {
        let py_obj = py_data.bind(py);

        // Try custom __az_to_json__ method first
        if let Ok(method) = py_obj.getattr("__az_to_json__") {
            if let Ok(result) = method.call0() {
                if let Ok(json_str) = result.extract::<String>() {
                    if let Ok(json) = Json::parse(&json_str) {
                        return json;
                    }
                }
            }
        }
        
        // Fallback to json.dumps
        if let Ok(json_module) = py.import("json") {
            if let Ok(json_str_obj) = json_module.call_method1("dumps", (py_obj,)) {
                if let Ok(json_str) = json_str_obj.extract::<String>() {
                    if let Ok(json) = Json::parse(&json_str) {
                        return json;
                    }
                }
            }
        }
        
        Json::null()
    })
}

/// Trampoline for Python object deserialization from JSON
/// 
/// This is called when `Json.deserialize_to_refany()` is invoked.
/// It uses Python's `json.loads()` to convert JSON to a Python dict,
/// then checks if the user's type has a `__az_from_json__` classmethod.
extern "C" fn py_deserialize_refany_trampoline(
    json: azul_layout::json::Json
) -> azul_layout::json::ResultRefAnyString {
    use azul_layout::json::ResultRefAnyString;
    use azul_css::AzString;
    
    Python::with_gil(|py| {
        let json_string = json.to_json_string();
        
        // Parse JSON using Python's json module
        let json_module = match py.import("json") {
            Ok(m) => m,
            Err(e) => return ResultRefAnyString::Err(
                AzString::from(format!("Failed to import json module: {}", e))
            ),
        };
        
        let py_obj = match json_module.call_method1("loads", (json_string.as_str(),)) {
            Ok(obj) => obj,
            Err(e) => return ResultRefAnyString::Err(
                AzString::from(format!("Failed to parse JSON: {}", e))
            ),
        };
        
        // Wrap the parsed Python object in PyDataWrapper
        let wrapper = PyDataWrapper {
            _py_data: Some(py_obj.unbind()),
        };
        
        // Create RefAny with JSON callbacks
        let refany = create_py_refany_with_json(wrapper);
        ResultRefAnyString::Ok(refany)
    })
}

/// Create a RefAny for a Python object with JSON serialization support
fn create_py_refany_with_json(wrapper: PyDataWrapper) -> azul_core::refany::RefAny {
    azul_core::refany::RefAny::new(wrapper)
}

"#,
        );
    }

    fn generate_callback_trampolines(
        &self,
        builder: &mut CodeBuilder,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) -> Result<()> {
        builder.line("// --- Callback Trampolines (extern \"C\" functions) ---");
        builder.blank();

        let prefix = &config.base.type_prefix;

        // Generate trampolines for ALL callback typedefs — including
        // LayoutCallbackType. The layout callback is NOT special: like every
        // other callback it reaches its Python callable via the ctx stored on
        // the wrapper (info.get_ctx()), so it uses the same trampoline path.
        // Generate trampolines for other callback typedefs
        for callback in &ir.callback_typedefs {
            if callback.name.ends_with("DestructorType")
                || callback.name.ends_with("CloneCallbackType")
                || callback.name.ends_with("DestructorCallbackType")
            {
                continue;
            }
            if callback.args.is_empty() || callback.args[0].type_name != "RefAny" {
                continue;
            }
            if callback.args.iter().any(|a| a.type_name.contains("*")) {
                continue;
            }

            // The trampoline reaches the stored Python callable via `get_ctx()` on
            // a non-RefAny, non-primitive arg. Without such an arg (e.g.
            // DatasetMergeCallback `(RefAny, RefAny) -> RefAny`) the callable is
            // unreachable, so emitting the trampoline would reference an undefined
            // `py_callable`. Skip these (the consuming method is skipped too).
            if !callback
                .args
                .iter()
                .any(|a| a.type_name != "RefAny" && !is_primitive_type(&a.type_name))
            {
                continue;
            }

            // Skip callbacks with return types that don't have Python wrappers or Default impl
            let return_type = callback.return_type.as_deref().unwrap_or("()");
            if return_type == "ImageRef" {
                // ImageRef is a special type that needs manual handling
                continue;
            }

            self.generate_callback_trampoline(builder, callback, ir, prefix);
        }

        Ok(())
    }


    fn generate_callback_trampoline(
        &self,
        builder: &mut CodeBuilder,
        callback: &CallbackTypedefDef,
        ir: &CodegenIR,
        prefix: &str,
    ) {
        let trampoline_name = format!(
            "invoke_py_{}",
            to_snake_case(&callback.name.replace("Type", ""))
        );

        // Build argument signature
        let mut args_sig = String::new();
        let mut ctx_source_type = String::new();
        let mut ctx_source_arg_name = String::new();

        // Find the first non-RefAny argument that can provide get_ctx()
        // All non-RefAny, non-primitive types have get_ctx() method
        // For callbacks like (RefAny, RefAny, CallbackInfo) -> Update, we want CallbackInfo
        // For callbacks like (RefAny, TimerCallbackInfo) -> Update, we want TimerCallbackInfo
        // For callbacks like (RefAny, ThreadSender, ThreadReceiver) -> (), we want ThreadSender
        for (i, arg) in callback.args.iter().enumerate() {
            if arg.type_name != "RefAny"
                && !is_primitive_type(&arg.type_name)
                && ctx_source_type.is_empty()
            {
                ctx_source_type = arg.type_name.clone();
                ctx_source_arg_name = if i == 0 {
                    "data".to_string()
                } else if i == 1 {
                    "info".to_string()
                } else {
                    format!("arg{}", i)
                };
            }
        }

        for (i, arg) in callback.args.iter().enumerate() {
            let arg_name = if i == 0 {
                "data".to_string()
            } else if i == 1 {
                "info".to_string()
            } else {
                format!("arg{}", i)
            };

            let arg_type_external = if is_primitive_type(&arg.type_name) {
                arg.type_name.clone()
            } else if let Some(ext) = self.find_external_path(&arg.type_name, ir) {
                ext
            } else {
                format!("__dll_api_inner::dll::{}{}", prefix, arg.type_name)
            };

            if i > 0 {
                args_sig.push_str(",\n    ");
            }
            args_sig.push_str(&format!("{}: {}", arg_name, arg_type_external));
        }

        let return_type = callback.return_type.as_deref().unwrap_or("()");
        let return_type_external = if is_primitive_type(return_type) || return_type == "()" {
            return_type.to_string()
        } else if let Some(ext) = self.find_external_path(return_type, ir) {
            ext
        } else {
            format!("__dll_api_inner::dll::{}{}", prefix, return_type)
        };

        let default_expr = match return_type {
            "()" => "()".to_string(),
            "Update" => format!("{}::DoNothing", return_type_external),
            "OnTextInputReturn" => format!(
                "{} {{ update: azul_core::callbacks::Update::DoNothing, valid: azul_layout::widgets::text_input::TextInputValid::Yes }}",
                return_type_external
            ),
            _ => format!("{}::default()", return_type_external),
        };

        builder.line(&format!(
            "/// Trampoline for {} - bridges Python to Rust",
            callback.name
        ));
        builder.line(&format!("extern \"C\" fn {}(", trampoline_name));
        builder.line(&format!("    {}", args_sig));
        builder.line(&format!(") -> {} {{", return_type_external));
        builder.indent();

        builder.line(&format!("let default = {};", default_expr));
        builder.blank();

        builder.line("let mut data_core = data;");
        builder.line("let py_data_wrapper = match data_core.downcast_ref::<PyDataWrapper>() {");
        builder.line("    Some(s) => s,");
        builder.line("    None => return default,");
        builder.line("};");
        builder.line("let py_data = match py_data_wrapper._py_data.as_ref() {");
        builder.line("    Some(s) => s,");
        builder.line("    None => return default,");
        builder.line("};");
        builder.blank();

        if !ctx_source_type.is_empty() {
            let ctx_external = self
                .find_external_path(&ctx_source_type, ir)
                .unwrap_or_else(|| format!("__dll_api_inner::dll::{}{}", prefix, ctx_source_type));
            // Clone the source to avoid move issues when it's also used for Python wrapper
            builder.line(&format!("let ctx_source_ffi: __dll_api_inner::dll::{}{} = unsafe {{ mem::transmute({}.clone()) }};", prefix, ctx_source_type, ctx_source_arg_name));
            builder.line(&format!(
                "let ctx_source_rust: &{} = unsafe {{ mem::transmute(&ctx_source_ffi) }};",
                ctx_external
            ));
            builder.line("let callable_opt = ctx_source_rust.get_ctx();");
            builder.line("let callable_refany = match callable_opt {");
            builder.line("    azul_core::refany::OptionRefAny::Some(r) => r,");
            builder.line("    azul_core::refany::OptionRefAny::None => return default,");
            builder.line("};");
            builder.line("let mut callable_core = callable_refany;");
            builder.line("let py_callable_wrapper = match callable_core.downcast_ref::<PyCallableWrapper>() {");
            builder.line("    Some(s) => s,");
            builder.line("    None => return default,");
            builder.line("};");
            builder.line("let py_callable = match py_callable_wrapper._py_callable.as_ref() {");
            builder.line("    Some(s) => s,");
            builder.line("    None => return default,");
            builder.line("};");
            builder.blank();
        }

        builder.line("Python::attach(|py| {");
        builder.indent();

        // For *Info types, we wrap and pass to Python
        // Find the info type for passing to Python (if any)
        let info_type_for_python = callback
            .args
            .iter()
            .find(|arg| arg.type_name.ends_with("Info") && arg.type_name != "RefAny")
            .map(|arg| arg.type_name.clone());

        if let Some(ref info_type) = info_type_for_python {
            let info_arg_idx = callback
                .args
                .iter()
                .position(|arg| &arg.type_name == info_type)
                .unwrap();
            let info_arg_name = if info_arg_idx == 0 {
                "data".to_string()
            } else if info_arg_idx == 1 {
                "info".to_string()
            } else {
                format!("arg{}", info_arg_idx)
            };
            builder.line(&format!(
                "let info_ffi_py: __dll_api_inner::dll::{}{} = unsafe {{ mem::transmute({}) }};",
                prefix, info_type, info_arg_name
            ));
            builder.line(&format!(
                "let info_py = {}{} {{ inner: info_ffi_py }};",
                prefix, info_type
            ));
        }

        let call_args = if info_type_for_python.is_none() {
            "py_data.clone_ref(py),".to_string() // Single-element tuple needs trailing comma
        } else {
            "py_data.clone_ref(py), info_py".to_string()
        };

        builder.line(&format!("match py_callable.call1(py, ({})) {{", call_args));
        builder.indent();
        builder.line("Ok(result) => {");
        builder.indent();

        if return_type == "()" {
            builder.line("()");
        } else {
            builder.line(&format!(
                "match result.extract::<{}{}>(py) {{",
                prefix, return_type
            ));
            builder.line("    Ok(ret) => unsafe { mem::transmute(ret.inner) },");
            builder.line("    Err(e) => {");
            builder.line("        // `return None` (or falling off the end of the callback)");
            builder.line("        // intentionally maps to the default return value; any OTHER");
            builder.line("        // wrong return type is a user bug that must be surfaced on");
            builder.line("        // stderr, not silently swallowed.");
            builder.line("        if !result.is_none(py) {");
            builder.line(&format!(
                "            eprintln!(\"azul: {} callback returned an unexpected type (expected {}), using default return value:\");",
                callback.name, return_type
            ));
            // pyo3 0.27: extract() on a pyclass fails with PyClassGuardError
            // (not PyErr) — convert before printing the traceback.
            builder.line("            pyo3::PyErr::from(e).print(py);");
            builder.line("        }");
            builder.line("        default");
            builder.line("    }");
            builder.line("}");
        }

        builder.dedent();
        builder.line("}");
        builder.line("Err(e) => {");
        builder.indent();
        builder.line("// ALWAYS surface the Python exception on sys.stderr. The");
        builder.line("// python-extension build installs no logger (fern is disabled");
        builder.line("// under the pyo3 build and pyo3_log::init is never called), so");
        builder.line("// a feature-gated log::error! silently swallows the traceback");
        builder.line("// and the user only sees a blank window / dead button.");
        builder.line(&format!(
            "eprintln!(\"azul: unhandled Python exception in {} callback:\");",
            callback.name
        ));
        builder.line("e.print(py);");
        builder.line("default");
        builder.dedent();
        builder.line("}");
        builder.dedent();
        builder.line("}");
        builder.dedent();
        builder.line("})");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    fn generate_wrapper_types(
        &self,
        builder: &mut CodeBuilder,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) -> Result<()> {
        builder.line(
            "// ============================================================================",
        );
        builder.line("// STRUCT DEFINITIONS");
        builder.line(
            "// ============================================================================",
        );
        builder.blank();

        let prefix = &config.base.type_prefix;

        for struct_def in &ir.structs {
            if !self.should_include_struct(struct_def, config) {
                continue;
            }
            self.generate_struct_wrapper(builder, struct_def, prefix, ir);
        }

        builder.line(
            "// ============================================================================",
        );
        builder.line("// ENUM DEFINITIONS");
        builder.line(
            "// ============================================================================",
        );
        builder.blank();

        for enum_def in &ir.enums {
            if !self.should_include_enum(enum_def, config) {
                continue;
            }
            self.generate_enum_wrapper(builder, enum_def, prefix, ir);
        }

        Ok(())
    }

    fn generate_struct_wrapper(
        &self,
        builder: &mut CodeBuilder,
        struct_def: &StructDef,
        prefix: &str,
        ir: &CodegenIR,
    ) {
        let name = format!("{}{}", prefix, struct_def.name);
        let c_api_type = format!("__dll_api_inner::dll::{}{}", prefix, struct_def.name);

        for doc in &struct_def.doc {
            builder.line(&format!("/// {}", doc));
        }

        // Determine if this type needs unsendable marker
        // Most types are sendable! Only types with raw pointers need unsendable
        let unsendable = if self.type_needs_unsendable(struct_def, ir) {
            ", unsendable"
        } else {
            ""
        };

        builder.line(&format!(
            "#[pyclass(name = \"{}\", module = \"azul\"{})]",
            struct_def.name, unsendable
        ));
        builder.line("#[repr(transparent)]");
        builder.line(&format!("pub struct {} {{", name));
        builder.line(&format!("    pub inner: {},", c_api_type));
        builder.line("}");
        builder.blank();

        builder.line(&format!("impl From<{}> for {} {{", c_api_type, name));
        builder.line(&format!(
            "    fn from(inner: {}) -> Self {{ Self {{ inner }} }}",
            c_api_type
        ));
        builder.line("}");
        builder.blank();

        builder.line(&format!("impl From<{}> for {} {{", name, c_api_type));
        builder.line(&format!(
            "    fn from(wrapper: {}) -> Self {{ wrapper.inner }}",
            name
        ));
        builder.line("}");
        builder.blank();
    }

    fn generate_enum_wrapper(
        &self,
        builder: &mut CodeBuilder,
        enum_def: &EnumDef,
        prefix: &str,
        ir: &CodegenIR,
    ) {
        let name = format!("{}{}", prefix, enum_def.name);
        let c_api_type = format!("__dll_api_inner::dll::{}{}", prefix, enum_def.name);

        for doc in &enum_def.doc {
            builder.line(&format!("/// {}", doc));
        }

        // Determine if this enum needs unsendable marker
        // Most enums are sendable! Only enums with variants containing raw pointers need unsendable
        let unsendable = if self.enum_needs_unsendable(enum_def, ir) {
            ", unsendable"
        } else {
            ""
        };

        builder.line(&format!(
            "#[pyclass(name = \"{}\", module = \"azul\"{})]",
            enum_def.name, unsendable
        ));
        builder.line("#[repr(transparent)]");
        builder.line(&format!("pub struct {} {{", name));
        builder.line(&format!("    pub inner: {},", c_api_type));
        builder.line("}");
        builder.blank();

        builder.line(&format!("impl From<{}> for {} {{", c_api_type, name));
        builder.line(&format!(
            "    fn from(inner: {}) -> Self {{ Self {{ inner }} }}",
            c_api_type
        ));
        builder.line("}");
        builder.blank();

        builder.line(&format!("impl From<{}> for {} {{", name, c_api_type));
        builder.line(&format!(
            "    fn from(wrapper: {}) -> Self {{ wrapper.inner }}",
            name
        ));
        builder.line("}");
        builder.blank();

        if !enum_def.is_union {
            builder.line(&format!("impl PartialEq for {} {{", name));
            builder.line("    fn eq(&self, other: &Self) -> bool {");
            builder.line("        unsafe {");
            builder.line("            let a: u8 = core::mem::transmute_copy(&self.inner);");
            builder.line("            let b: u8 = core::mem::transmute_copy(&other.inner);");
            builder.line("            a == b");
            builder.line("        }");
            builder.line("    }");
            builder.line("}");
            builder.blank();

            builder.line(&format!("impl Eq for {} {{}}", name));
            builder.blank();

            builder.line(&format!("impl core::hash::Hash for {} {{", name));
            builder.line("    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {");
            builder.line("        unsafe {");
            builder.line("            let disc: u8 = core::mem::transmute_copy(&self.inner);");
            builder.line("            disc.hash(state);");
            builder.line("        }");
            builder.line("    }");
            builder.line("}");
            builder.blank();
        }
    }

    fn generate_clone_impls(
        &self,
        builder: &mut CodeBuilder,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) -> Result<()> {
        builder.line(
            "// ============================================================================",
        );
        builder.line("// CLONE IMPLEMENTATIONS");
        builder.line(
            "// ============================================================================",
        );
        builder.blank();

        let prefix = &config.base.type_prefix;

        for struct_def in &ir.structs {
            if !self.should_include_struct(struct_def, config) {
                continue;
            }
            // Skip structs that can't be cloned (contain callbacks or other non-Clone types)
            if !self.struct_supports_clone(struct_def) {
                continue;
            }
            let name = format!("{}{}", prefix, struct_def.name);
            builder.line(&format!("impl Clone for {} {{", name));
            builder.line("    fn clone(&self) -> Self {");
            builder.line("        Self { inner: self.inner.clone() }");
            builder.line("    }");
            builder.line("}");
            builder.blank();
        }

        for enum_def in &ir.enums {
            if !self.should_include_enum(enum_def, config) {
                continue;
            }
            // Skip enums that can't be cloned
            if !self.enum_supports_clone(enum_def) {
                continue;
            }
            let name = format!("{}{}", prefix, enum_def.name);
            builder.line(&format!("impl Clone for {} {{", name));
            builder.line("    fn clone(&self) -> Self {");
            builder.line("        Self { inner: self.inner.clone() }");
            builder.line("    }");
            builder.line("}");
            builder.blank();
        }

        Ok(())
    }

    fn generate_debug_impls(
        &self,
        builder: &mut CodeBuilder,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) -> Result<()> {
        builder.line(
            "// ============================================================================",
        );
        builder.line("// DEBUG IMPLEMENTATIONS");
        builder.line(
            "// ============================================================================",
        );
        builder.blank();

        let prefix = &config.base.type_prefix;

        for struct_def in &ir.structs {
            if !self.should_include_struct(struct_def, config) {
                continue;
            }
            let name = format!("{}{}", prefix, struct_def.name);
            builder.line(&format!("impl core::fmt::Debug for {} {{", name));
            builder.line("    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {");
            builder.line("        core::fmt::Debug::fmt(&self.inner, f)");
            builder.line("    }");
            builder.line("}");
            builder.blank();
        }

        for enum_def in &ir.enums {
            if !self.should_include_enum(enum_def, config) {
                continue;
            }
            let name = format!("{}{}", prefix, enum_def.name);
            builder.line(&format!("impl core::fmt::Debug for {} {{", name));
            builder.line("    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {");
            builder.line("        core::fmt::Debug::fmt(&self.inner, f)");
            builder.line("    }");
            builder.line("}");
            builder.blank();
        }

        Ok(())
    }

    fn generate_pymethods(
        &self,
        builder: &mut CodeBuilder,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) -> Result<()> {
        builder.line(
            "// ============================================================================",
        );
        builder.line("// PYMETHODS IMPLEMENTATIONS");
        builder.line(
            "// ============================================================================",
        );
        builder.blank();

        let prefix = &config.base.type_prefix;

        for struct_def in &ir.structs {
            if !self.should_include_struct(struct_def, config) {
                continue;
            }
            self.generate_struct_pymethods(builder, struct_def, ir, prefix, config);
        }

        for enum_def in &ir.enums {
            if !self.should_include_enum(enum_def, config) {
                continue;
            }
            self.generate_enum_pymethods(builder, enum_def, ir, prefix, config);
        }

        Ok(())
    }

    fn generate_struct_pymethods(
        &self,
        builder: &mut CodeBuilder,
        struct_def: &StructDef,
        ir: &CodegenIR,
        prefix: &str,
        config: &PythonConfig,
    ) {
        let name = format!("{}{}", prefix, struct_def.name);
        let c_api_type = format!("__dll_api_inner::dll::{}{}", prefix, struct_def.name);

        builder.line("#[pymethods]");
        builder.line(&format!("impl {} {{", name));
        builder.indent();

        let class_functions: Vec<_> = ir
            .functions
            .iter()
            .filter(|f| f.class_name == struct_def.name)
            .filter(|f| !f.kind.is_trait_function())
            .collect();

        // Check if this struct is a callback wrapper type
        let is_callback_type = is_callback_wrapper_type(&struct_def.name, ir);

        for func in class_functions {
            if self.function_has_unsupported_args(func, ir) {
                continue;
            }
            if self.function_refs_excluded_type(func, ir, config) {
                continue;
            }
            // Skip constructors for callback types - Python uses PyAny + trampoline instead
            if is_callback_type && func.kind == FunctionKind::Constructor {
                continue;
            }
            self.generate_pymethod(builder, func, ir, prefix);
        }

        builder.line("fn __str__(&self) -> String {");
        builder.line("    format!(\"{:?}\", self)");
        builder.line("}");
        builder.blank();
        builder.line("fn __repr__(&self) -> String {");
        builder.line("    self.__str__()");
        builder.line("}");

        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    fn generate_enum_pymethods(
        &self,
        builder: &mut CodeBuilder,
        enum_def: &EnumDef,
        ir: &CodegenIR,
        prefix: &str,
        config: &PythonConfig,
    ) {
        let name = format!("{}{}", prefix, enum_def.name);
        let c_api_type = format!("__dll_api_inner::dll::{}{}", prefix, enum_def.name);

        builder.line("#[pymethods]");
        builder.line(&format!("impl {} {{", name));
        builder.indent();

        for variant in &enum_def.variants {
            match &variant.kind {
                EnumVariantKind::Unit => {
                    if enum_def.is_union {
                        builder.line("#[staticmethod]");
                        builder.line(&format!("fn {}() -> Self {{", variant.name));
                        builder.line(&format!(
                            "    Self {{ inner: {}::{} }}",
                            c_api_type, variant.name
                        ));
                        builder.line("}");
                    } else {
                        builder.line("#[classattr]");
                        builder.line(&format!("fn {}() -> Self {{", variant.name));
                        builder.line(&format!(
                            "    Self {{ inner: {}::{} }}",
                            c_api_type, variant.name
                        ));
                        builder.line("}");
                    }
                    builder.blank();
                }
                EnumVariantKind::Tuple(types) => {
                    if let Some((ty, _ref_kind)) = types.first() {
                        if !self.is_python_compatible_type(ty, ir) {
                            continue;
                        }
                        if self.type_is_excluded(ty, ir, config) {
                            continue;
                        }
                        let py_type = self.rust_type_to_python(ty, prefix, ir);
                        builder.line("#[staticmethod]");
                        builder.line(&format!("fn {}(v: {}) -> Self {{", variant.name, py_type));
                        if is_primitive_type(ty) {
                            builder.line(&format!(
                                "    Self {{ inner: {}::{}(v) }}",
                                c_api_type, variant.name
                            ));
                        } else if ty == "String" {
                            // String needs to be converted to AzString and transmuted
                            builder.line(&format!("    unsafe {{ Self {{ inner: {}::{}(core::mem::transmute(azul_css::corety::AzString::from(v))) }} }}", c_api_type, variant.name));
                        } else if is_callback_wrapper_type(ty, ir) {
                            // Callback types use Py<PyAny> which has no .inner field
                            // For now, skip these - callbacks in Option<Callback> require more complex handling
                            builder.line("    // TODO: callback type conversion");
                            builder.line(&format!(
                                "    unimplemented!(\"Option<{}> not yet supported in Python\")",
                                ty
                            ));
                        } else {
                            builder.line(&format!(
                                "    Self {{ inner: {}::{}(v.inner) }}",
                                c_api_type, variant.name
                            ));
                        }
                        builder.line("}");
                        builder.blank();
                    }
                }
                EnumVariantKind::Struct(_) => {
                    // Struct variants not yet supported
                }
            }
        }

        if !enum_def.is_union {
            // Unit variants of C-like enums are exposed as #[classattr]
            // INSTANCES (`Update.RefreshDom`), but docs and examples have
            // historically also used the constructor-call spelling
            // (`Update.RefreshDom()`). Make every instance callable and
            // return itself, so BOTH spellings work instead of the parens
            // form raising a TypeError that the callback trampoline then
            // maps to a default return value.
            builder.line("fn __call__(&self) -> Self {");
            builder.line("    // Fieldless repr(C) enum: a bitwise copy is a valid clone.");
            builder.line("    Self { inner: unsafe { core::mem::transmute_copy(&self.inner) } }");
            builder.line("}");
            builder.blank();
        }

        builder.line("fn __str__(&self) -> String {");
        builder.line("    format!(\"{:?}\", self)");
        builder.line("}");
        builder.blank();
        builder.line("fn __repr__(&self) -> String {");
        builder.line("    self.__str__()");
        builder.line("}");

        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    fn generate_pymethod(
        &self,
        builder: &mut CodeBuilder,
        func: &FunctionDef,
        ir: &CodegenIR,
        prefix: &str,
    ) {
        // Skip functions without fn_body - they can't be called directly
        // This is the key insight from the old generator: methods like intersect/tessellate_stroke
        // are implemented via fn_body calling free functions, not actual methods on the type
        let fn_body = match &func.fn_body {
            Some(body) => body.clone(),
            None => {
                // No fn_body means this function can't be implemented
                builder.line(&format!(
                    "// fn {}(...) - skipped: no fn_body in api.json",
                    func.method_name
                ));
                builder.blank();
                return;
            }
        };

        // Find the external path for the class
        let external_path = ir
            .structs
            .iter()
            .find(|s| s.name == func.class_name)
            .and_then(|s| s.external_path.clone())
            .or_else(|| {
                ir.enums
                    .iter()
                    .find(|e| e.name == func.class_name)
                    .and_then(|e| e.external_path.clone())
            })
            .unwrap_or_else(|| format!("crate::{}", func.class_name))
            .replace("azul_dll::", "crate::");

        let ffi_type = format!("__dll_api_inner::dll::{}{}", prefix, func.class_name);

        let is_constructor = func.kind == FunctionKind::Constructor;
        let is_static = func.kind == FunctionKind::StaticMethod;
        let takes_self = matches!(func.kind, FunctionKind::Method | FunctionKind::MethodMut);

        // Check if this function has a callback pattern (RefAny + CallbackType)
        // This needs to be early because we need it for function signature
        let has_callback_pattern = self.has_callback_pattern(func);

        if is_constructor && func.method_name == "new" {
            builder.line("#[new]");
        } else if is_constructor || is_static {
            builder.line("#[staticmethod]");
        }

        // Drop the implicit self-arg — `ir_builder.rs` synthesises an
        // entry named `to_snake_case(class_name)` (e.g. `list_view_row_vec`
        // for `ListViewRowVec`) whenever fn_args carries `{ "self": "..." }`.
        // The previous `to_lowercase()` filter only matched single-word
        // class names (`Dom` → `dom`); compound names like `DomVec`
        // (snake `dom_vec`) slipped through and produced
        // `fn len(&self, dom_vec: AzDomVec)` — both receiver-shadowed
        // and unusable from Python.
        let self_arg_name = to_snake_case(&func.class_name);
        let args: Vec<_> = func
            .args
            .iter()
            .filter(|a| a.name != self_arg_name)
            .collect();

        let args_str: String = args
            .iter()
            .map(|a| {
                let py_type = self.rust_type_to_python(&a.type_name, prefix, ir);
                format!("{}: {}", a.name, py_type)
            })
            .collect::<Vec<_>>()
            .join(", ");

        let return_type = func
            .return_type
            .as_ref()
            .map(|t| self.rust_type_to_python(t, prefix, ir))
            .unwrap_or_else(|| "()".to_string());

        // Functions with RefAny or Callback args need access to Python GIL for clone_ref
        let has_refany_arg = args.iter().any(|a| a.type_name == "RefAny");
        let has_callback_arg = args.iter().any(|a| a.callback_info.is_some());
        let needs_py_param = has_refany_arg || has_callback_arg;

        // Generate function signature
        if takes_self {
            if args.is_empty() {
                if needs_py_param {
                    builder.line(&format!(
                        "fn {}(&self, py: Python<'_>) -> {} {{",
                        func.method_name, return_type
                    ));
                } else {
                    builder.line(&format!(
                        "fn {}(&self) -> {} {{",
                        func.method_name, return_type
                    ));
                }
            } else {
                if needs_py_param {
                    builder.line(&format!(
                        "fn {}(&self, py: Python<'_>, {}) -> {} {{",
                        func.method_name, args_str, return_type
                    ));
                } else {
                    builder.line(&format!(
                        "fn {}(&self, {}) -> {} {{",
                        func.method_name, args_str, return_type
                    ));
                }
            }
        } else {
            if args.is_empty() {
                if needs_py_param {
                    builder.line(&format!(
                        "fn {}(py: Python<'_>) -> {} {{",
                        func.method_name, return_type
                    ));
                } else {
                    builder.line(&format!("fn {}() -> {} {{", func.method_name, return_type));
                }
            } else {
                if needs_py_param {
                    builder.line(&format!(
                        "fn {}(py: Python<'_>, {}) -> {} {{",
                        func.method_name, args_str, return_type
                    ));
                } else {
                    builder.line(&format!(
                        "fn {}({}) -> {} {{",
                        func.method_name, args_str, return_type
                    ));
                }
            }
        }

        builder.indent();
        builder.line("#[allow(unused_mut)]");
        builder.line("unsafe {");
        builder.indent();

        // Transform the fn_body for Python bindings:
        // 1. Replace "azul_dll::" with "crate::" (we're in azul-dll crate)
        // 2. Replace "Self " and "Self::" with the external path (since Self in Python wrapper is AzXxx)
        // 3. Replace self references with transmuted variable
        // 4. Replace parameter names with transmuted versions
        // 5. Replace type constructors (TypeName::method) with fully qualified paths
        let mut transformed_body = fn_body.replace("azul_dll::", "crate::");

        // Replace Self with external path (Self in fn_body refers to the Rust type, not the Python wrapper)
        // Handle both "Self::" (associated functions) and "Self {" or "Self " (struct construction)
        transformed_body = transformed_body
            .replace("Self::", &format!("{}::", external_path))
            .replace("Self {", &format!("{} {{", external_path))
            .replace("Self(", &format!("{}(", external_path));

        // Replace type constructors with fully qualified paths wherever they
        // appear as a path prefix (e.g. "TypeName::method"), not just at the
        // start of fn_body. Bodies like `unsafe { U32Vec::copy_from_ptr(...) }`
        // reference the type after an `unsafe {` / `(` token, so a `starts_with`
        // check missed them and left the bare (undeclared) type name.
        //
        // To avoid clobbering substrings (e.g. `Dom::` inside `DomId::`) we
        // process longer type names first and only replace occurrences where
        // the `Name::` is not preceded by an identifier character.
        let mut ctor_replacements: Vec<(String, String)> = Vec::new();
        for struct_def in &ir.structs {
            if let Some(ref ext_path) = struct_def.external_path {
                ctor_replacements.push((
                    format!("{}::", struct_def.name),
                    format!("{}::", ext_path),
                ));
            }
        }
        for enum_def in &ir.enums {
            if let Some(ref ext_path) = enum_def.external_path {
                ctor_replacements
                    .push((format!("{}::", enum_def.name), format!("{}::", ext_path)));
            }
        }
        // Longest pattern first so `DomIdVec::` wins over `Dom::`.
        ctor_replacements.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        transformed_body =
            replace_type_paths(&transformed_body, &ctor_replacements);

        // Convert self to external type if needed.
        // Use `to_snake_case` so compound class names (e.g. `DomVec`)
        // match the IR-synthesised arg name `dom_vec` rather than
        // `domvec` — otherwise the fn_body substitutions below would
        // leave `dom_vec.len()` untouched and a stale `dom_vec` would
        // shadow `__cloned` in the generated body.
        // The fn_body refers to the receiver by an arg name that api.json
        // derives from the class name. Two conventions exist in api.json:
        // the snake_case form (`raw_image`, `dom_vec`) and the all-lowercase
        // no-underscore form (`rawimage`, `domvec`). Build candidates for both
        // (longest first) so whichever the body uses gets rewritten to
        // `__cloned`.
        let self_var = to_snake_case(&func.class_name);
        let self_var_lower = func.class_name.to_lowercase();
        let mut self_vars: Vec<String> = vec![self_var.clone()];
        if self_var_lower != self_var {
            self_vars.push(self_var_lower);
        }
        self_vars.sort_by(|a, b| b.len().cmp(&a.len()));
        let is_method_mut = func.kind == FunctionKind::MethodMut;

        // Does the receiver type implement Clone? If not, `_self.clone()` would
        // silently resolve to `Clone for &T` (yielding `&T`), which then fails
        // to compile for by-value consuming methods like `CameraWidget::dom(self)`
        // ("cannot move out of `*__cloned`"). For such types we must move the
        // owned inner value out with `core::ptr::read` instead of cloning.
        let class_is_clone = ir
            .find_struct(&func.class_name)
            .map(|s| self.struct_supports_clone(s))
            .or_else(|| {
                ir.find_enum(&func.class_name)
                    .map(|e| self.enum_supports_clone(e))
            })
            .unwrap_or(true);
        if takes_self {
            builder.line(&format!(
                "let _self: &{} = core::mem::transmute(&self.inner);",
                external_path
            ));
            // Clone self so methods can consume it (mut for methods that mutate).
            // For non-Clone types, bitwise-move the owned inner out instead.
            if class_is_clone {
                builder.line("let mut __cloned = _self.clone();");
            } else {
                builder.line(&format!(
                    "let mut __cloned: {} = core::ptr::read(_self as *const {});",
                    external_path, external_path
                ));
            }

            // Replace self references in fn_body
            // First replace method calls (with dot)
            transformed_body = transformed_body
                .replace("self.", "__cloned.")
                .replace("object.", "__cloned.");
            for sv in &self_vars {
                transformed_body =
                    transformed_body.replace(&format!("{}.", sv), "__cloned.");
            }

            // Then replace standalone variable references (as function arguments)
            // Handle various contexts: (var, ...), (var), var, ..., etc.
            // The self arg (named after the class) records its ref_kind: a
            // by-value `self` (ArgRefKind::Owned) must be moved (`__cloned`),
            // while `&self`/`&mut self` are passed by reference. Free functions
            // like `json_deserialize_to_refany(json: Json, ...)` consume self by
            // value, so `&__cloned` would be a type error.
            let self_is_by_value = func
                .args
                .iter()
                .find(|a| self_vars.iter().any(|sv| sv == &a.name))
                .map(|a| a.ref_kind == ArgRefKind::Owned)
                .unwrap_or(false);
            let self_ref = if is_method_mut {
                "&mut __cloned"
            } else if self_is_by_value {
                "__cloned"
            } else {
                "&__cloned"
            };
            for sv in &self_vars {
                transformed_body = transformed_body
                    .replace(&format!("({},", sv), &format!("({},", self_ref))
                    .replace(&format!("({}, ", sv), &format!("({}, ", self_ref))
                    .replace(&format!("({})", sv), &format!("({})", self_ref))
                    .replace(&format!(", {},", sv), ", __cloned,")
                    .replace(&format!(", {}, ", sv), ", __cloned, ")
                    .replace(&format!(", {})", sv), ", __cloned)")
                    .replace(&format!("&mut {},", sv), "&mut __cloned,")
                    .replace(&format!("&mut {})", sv), "&mut __cloned)")
                    .replace(&format!("&{},", sv), "&__cloned,")
                    .replace(&format!("&{})", sv), "&__cloned)");
            }
        }

        // Convert arguments to external types
        for arg in &args {
            // RefAny is ALWAYS converted from Py<PyAny> to RefAny with JSON support
            if arg.type_name == "RefAny" {
                // Wrap Python data in RefAny via PyDataWrapper with JSON serialization
                // Use the SAME name as the parameter so fn_body can use it unchanged
                builder.line(&format!(
                    "let __py_{}_wrapper = PyDataWrapper {{ _py_data: Some({}.clone_ref(py)) }};",
                    arg.name, arg.name
                ));
                builder.line(&format!(
                    "let {}: azul_core::refany::RefAny = create_py_refany_with_json(__py_{}_wrapper);",
                    arg.name, arg.name
                ));
                // No fn_body replacement needed - we used the same variable name as the parameter
                continue;
            }

            // Callback types with callback_info are converted from Py<PyAny> to Callback struct
            if let Some(ref cb_info) = arg.callback_info {
                // Wrap Python callable in the callback wrapper struct with trampoline.
                //
                // IMPORTANT: the ctx RefAny must store the PyCallableWrapper ITSELF —
                // every generated trampoline extracts it via
                // `callable_core.downcast_ref::<PyCallableWrapper>()` (see
                // generate_trampoline). RefAny::downcast_ref compares TypeIds, so
                // re-wrapping the callable into a PyDataWrapper here (as was done
                // historically to reuse create_py_refany_with_json) makes the
                // downcast fail and EVERY Python callback silently degrades to its
                // default return value: blank window, dead buttons, no traceback.
                builder.line(&format!(
                    "let __py_{}_wrapper = PyCallableWrapper {{ _py_callable: Some({}.clone_ref(py)) }};",
                    arg.name, arg.name
                ));
                builder.line(&format!(
                    "let __py_{}_refany = azul_core::refany::RefAny::new(__py_{}_wrapper);",
                    arg.name, arg.name
                ));

                // Find the external path for the callback wrapper (the internal Rust type)
                let wrapper_external = self
                    .find_external_path(&cb_info.callback_wrapper_name, ir)
                    .unwrap_or_else(|| format!("crate::{}", cb_info.callback_wrapper_name));

                // Look up the actual wrapper struct definition to discover its real
                // field names. The function-pointer field is usually "cb" but may be
                // named differently (e.g. "resolver"); the foreign-callable storage
                // field is the OptionRefAny field, named "ctx" or "callable".
                let wrapper_struct =
                    ir.structs.iter().find(|s| s.name == cb_info.callback_wrapper_name);
                let (fn_ptr_field, callable_field) = match wrapper_struct {
                    Some(sd) => {
                        let callable = sd
                            .fields
                            .iter()
                            .find(|f| f.type_name == "OptionRefAny")
                            .map(|f| f.name.clone());
                        let fn_ptr = sd
                            .fields
                            .iter()
                            .find(|f| f.type_name != "OptionRefAny")
                            .map(|f| f.name.clone())
                            .unwrap_or_else(|| "cb".to_string());
                        (fn_ptr, callable)
                    }
                    None => ("cb".to_string(), Some("ctx".to_string())),
                };

                // Build the wrapper using the generated FFI inner type
                // (`__dll_api_inner::dll::Az...`), whose field names are derived
                // from the same api.json as the IR, so the field literal always
                // matches. The real external struct may name its OptionRefAny
                // field differently (e.g. `ctx` vs `callable`) but is ABI/layout
                // compatible, so we transmute the FFI value to the external type.
                let wrapper_ffi = format!(
                    "__dll_api_inner::dll::{}{}",
                    prefix, cb_info.callback_wrapper_name
                );
                builder.line(&format!("let __{}_ffi: {} = {} {{", arg.name, wrapper_ffi, wrapper_ffi));
                builder.line(&format!(
                    "    {}: core::mem::transmute({} as usize),",
                    fn_ptr_field, cb_info.trampoline_name
                ));
                if let Some(ref cf) = callable_field {
                    builder.line(&format!(
                        "    {}: core::mem::transmute(azul_core::refany::OptionRefAny::Some(__py_{}_refany)),",
                        cf, arg.name
                    ));
                }
                builder.line("};");
                // Use the SAME name as the parameter so fn_body can use it unchanged.
                builder.line(&format!(
                    "let {}: {} = core::mem::transmute(__{}_ffi);",
                    arg.name, wrapper_external, arg.name
                ));
                // No fn_body replacement needed - we used the same variable name as the parameter
                continue;
            }

            // Normal argument handling
            // Use the SAME name as the parameter so fn_body can use it unchanged
            let arg_external = self
                .find_external_path(&arg.type_name, ir)
                .unwrap_or_else(|| {
                    if is_primitive_type(&arg.type_name) {
                        arg.type_name.clone()
                    } else if arg.type_name == "String" {
                        "azul_css::corety::AzString".to_string()
                    } else {
                        format!("crate::{}", arg.type_name)
                    }
                });

            // A type alias to a primitive (e.g. `ScanCode = u32`) is emitted as a
            // bare primitive in the FFI layer, so the pyfn arg has no `.inner`
            // field. Transmute the value directly to the external newtype.
            let prim_alias_target = ir
                .type_aliases
                .iter()
                .find(|ta| ta.name == arg.type_name && ta.generic_args.is_empty())
                .map(|ta| ta.target.clone())
                .filter(|t| is_primitive_type(t));

            if is_primitive_type(&arg.type_name) {
                // Primitive types - use directly, no conversion needed
                // The parameter already has the correct type
                // No shadowing needed for primitives
            } else if let Some(_target) = prim_alias_target {
                // Alias-to-primitive: the param is the bare primitive, transmute it.
                builder.line(&format!(
                    "let {}: {} = core::mem::transmute({});",
                    arg.name, arg_external, arg.name
                ));
            } else if arg.type_name == "String" {
                // String args: convert to AzString, shadow the parameter
                builder.line(&format!(
                    "let {}: {} = azul_css::corety::AzString::from({}.clone());",
                    arg.name, arg_external, arg.name
                ));
            } else if is_direct_ffi_type(&arg.type_name) {
                // Direct FFI types (StringVec, U8Vec, etc.) - no .inner wrapper
                // These are type-aliased directly to the C-API types
                builder.line(&format!(
                    "let {}: {} = core::mem::transmute({}.clone());",
                    arg.name, arg_external, arg.name
                ));
            } else {
                // Other types use transmute with .inner, shadow the parameter
                builder.line(&format!(
                    "let {}: {} = core::mem::transmute({}.inner.clone());",
                    arg.name, arg_external, arg.name
                ));
            }
            // No fn_body replacement needed - we used the same variable name as the parameter
        }

        // Check if fn_body contains statements (has `;` which means multiple statements)
        let has_statements = transformed_body.contains(';');

        // Determine return type handling
        let ret_type_str = func
            .return_type
            .as_ref()
            .map(|t| format!("{}{}", prefix, t))
            .unwrap_or_default();

        if ret_type_str.is_empty() || ret_type_str == format!("{}()", prefix) {
            // Void return
            if has_statements {
                builder.line(&format!("{}", transformed_body));
            } else {
                builder.line(&format!("let _: () = {};", transformed_body));
            }
        } else if let Some(ret_type) = &func.return_type {
            if is_primitive_type(ret_type) {
                // Primitive return types
                if has_statements {
                    builder.line(&format!("{{ {} }}", transformed_body));
                } else {
                    builder.line(&transformed_body);
                }
            } else if ret_type == "String" {
                // String return type: external methods return AzString, convert to Rust String
                // Use into_library_owned_string() to convert AzString to String
                if has_statements {
                    builder.line(&format!(
                        "let __result: azul_css::corety::AzString = {{ {} }};",
                        transformed_body
                    ));
                } else {
                    builder.line(&format!(
                        "let __result: azul_css::corety::AzString = {};",
                        transformed_body
                    ));
                }
                builder.line("__result.into_library_owned_string()");
            } else if is_capi_type_alias(ret_type) {
                // C-API type aliases (like U8Vec, StringVec) - transmute directly without wrapper
                let ret_external = self
                    .find_external_path(ret_type, ir)
                    .unwrap_or_else(|| format!("crate::{}", ret_type));
                if has_statements {
                    builder.line(&format!(
                        "let __result: {} = {{ {} }};",
                        ret_external, transformed_body
                    ));
                } else {
                    builder.line(&format!(
                        "let __result: {} = {};",
                        ret_external, transformed_body
                    ));
                }
                builder.line("core::mem::transmute(__result)");
            } else {
                // Need to wrap result in Python wrapper
                let ret_external = self
                    .find_external_path(ret_type, ir)
                    .unwrap_or_else(|| format!("crate::{}", ret_type));

                if has_statements {
                    // fn_body has statements - wrap in block
                    builder.line(&format!(
                        "let __result: {} = {{ {} }};",
                        ret_external, transformed_body
                    ));
                } else {
                    builder.line(&format!(
                        "let __result: {} = {};",
                        ret_external, transformed_body
                    ));
                }

                // Only use Self { inner } if the return type matches the class
                // For constructors that return Result types, use the return type wrapper
                if ret_type == &func.class_name {
                    builder.line("Self { inner: core::mem::transmute(__result) }");
                } else {
                    builder.line(&format!(
                        "{}{} {{ inner: core::mem::transmute(__result) }}",
                        prefix, ret_type
                    ));
                }
            }
        } else {
            builder.line(&transformed_body);
        }

        builder.dedent();
        builder.line("}");
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    fn generate_module_registration(
        &self,
        builder: &mut CodeBuilder,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) -> Result<()> {
        builder.line("// MODULE REGISTRATION");
        builder.blank();

        let prefix = &config.base.type_prefix;

        builder.line("/// Register all Python types with the module");
        builder.line(
            "pub fn register_types(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {",
        );
        builder.indent();

        for struct_def in &ir.structs {
            if !self.should_include_struct(struct_def, config) {
                continue;
            }
            builder.line(&format!("m.add_class::<{}{}>()?;", prefix, struct_def.name));
        }

        for enum_def in &ir.enums {
            if !self.should_include_enum(enum_def, config) {
                continue;
            }
            builder.line(&format!("m.add_class::<{}{}>()?;", prefix, enum_def.name));
        }

        builder.line("Ok(())");
        builder.dedent();
        builder.line("}");
        builder.blank();

        // Generate the #[pymodule] function that PyO3 needs for PyInit_azul
        builder.line("/// PyO3 module definition - generates PyInit_azul");
        builder.line("#[pymodule]");
        builder.line("pub fn azul(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {");
        builder.indent();
        builder.line("register_types(py, m)");
        builder.dedent();
        builder.line("}");
        builder.blank();

        Ok(())
    }

    // Helper methods

    /// Check if a struct should be included in Python bindings
    /// Uses TypeCategory from the IR for classification
    fn should_include_struct(&self, struct_def: &StructDef, config: &PythonConfig) -> bool {
        // Use TypeCategory for primary classification
        if struct_def.category.skip_in_python() {
            return false;
        }

        // Types that use C-API directly don't get wrapper structs
        if struct_def.category.uses_capi_directly() {
            return false;
        }

        // Check config overrides
        if config.skip_types.contains(&struct_def.name) {
            return false;
        }

        config.base.should_include_type(&struct_def.name)
    }

    /// Check if an enum should be included in Python bindings
    /// Uses TypeCategory from the IR for classification
    fn should_include_enum(&self, enum_def: &EnumDef, config: &PythonConfig) -> bool {
        // Use TypeCategory for primary classification
        if enum_def.category.skip_in_python() {
            return false;
        }

        // Check config overrides
        if config.skip_types.contains(&enum_def.name) {
            return false;
        }

        config.base.should_include_type(&enum_def.name)
    }

    /// Check if a struct can be cloned (has Clone derive or custom_impl)
    /// Types without Clone cannot be cloned
    fn struct_supports_clone(&self, struct_def: &StructDef) -> bool {
        // Check if struct has Clone in derive list or custom_impls
        struct_def.derives.contains(&"Clone".to_string())
            || struct_def.custom_impls.contains(&"Clone".to_string())
    }

    /// Check if an enum can be cloned
    fn enum_supports_clone(&self, enum_def: &EnumDef) -> bool {
        // Check if enum has Clone in derive list (enums don't have custom_impls)
        enum_def.derives.contains(&"Clone".to_string())
    }

    /// Check if a struct type needs the `unsendable` marker in PyO3
    ///
    /// A type is sendable if:
    /// - It has `is_send_safe: true` in the IR (vec module types), OR
    /// - It's in the PYTHON_SEND_SAFE_TYPES list (types that wrap & or Box), OR
    /// - All its fields are transitively sendable
    ///
    /// A type needs unsendable if it contains:
    /// - Raw pointers (*const, *mut) that are NOT in a send_safe type
    /// - Function pointers (extern "C" fn)
    /// - Boxed types
    /// - Callback wrappers
    fn type_needs_unsendable(&self, struct_def: &StructDef, ir: &CodegenIR) -> bool {
        // Types marked as send_safe in the IR don't need unsendable
        if struct_def.is_send_safe {
            return false;
        }

        // Python-specific: These types use *const/*mut c_void but are semantically Send
        // because they wrap references (&) or Box<> which are Send
        const PYTHON_SEND_SAFE_TYPES: &[&str] = &[
            "CssPropertyCachePtr",          // wraps Box<CssPropertyCache>
            "VirtualViewCallbackInfo",           // wraps &VirtualViewCallbackInfoInternal
            "VirtualViewReturn", // contains OptionDom which may have callbacks with raw pointers
            "StyledDom",            // contains CssPropertyCachePtr
            "LayoutCallbackInfo",   // wraps & to internal data
            "CallbackInfo",         // wraps & to internal data
            "RenderImageCallbackInfo", // wraps & to internal data
            "RefCount",             // refcounted pointer, semantically Send
            "OptionRefAny",         // Option<RefAny>
            "GlVoidPtrMut",         // GL pointer wrapper
            "ParsedSvg",            // SVG data structure
            "ResultParsedSvgSvgParseError", // Result type containing ParsedSvg
            "GridMinMax",           // CSS grid layout type
            "GridTrackSizing",      // CSS grid layout type
            // Window/Thread types - Send but not Sync
            "RawWindowHandle",
            "OptionThread",
            "ThreadSendMsg",
            "OptionThreadSendMsg",
            "OptionTimer",
            "OptionThreadReceiveMsg",
        ];
        if PYTHON_SEND_SAFE_TYPES.contains(&struct_def.name.as_str()) {
            return false;
        }

        // Boxed types definitely need unsendable
        if struct_def.is_boxed {
            return true;
        }

        // Types with callback wrappers contain function pointers
        if struct_def.callback_wrapper_info.is_some() {
            return true;
        }

        // Check all fields for unsendable types
        for field in &struct_def.fields {
            // Fields with pointer ref_kind are not sendable
            if matches!(
                field.ref_kind,
                crate::codegen::v2::ir::FieldRefKind::Ptr
                    | crate::codegen::v2::ir::FieldRefKind::PtrMut
            ) {
                return true;
            }
            if self.field_type_needs_unsendable(&field.type_name, ir) {
                return true;
            }
        }

        false
    }

    /// Check if an enum type needs the `unsendable` marker
    ///
    /// An enum is sendable if:
    /// - It has `is_send_safe: true` in the IR, OR
    /// - It's in the PYTHON_SEND_SAFE_TYPES list, OR
    /// - All its variant payloads are transitively sendable
    fn enum_needs_unsendable(&self, enum_def: &EnumDef, ir: &CodegenIR) -> bool {
        // Types marked as send_safe in the IR don't need unsendable
        if enum_def.is_send_safe {
            return false;
        }

        // Python-specific: These enum types NEED unsendable because they contain raw pointers
        // but we want to use them in Python anyway
        const PYTHON_FORCE_UNSENDABLE_ENUMS: &[&str] = &[
            "RawWindowHandle",
            "OptionRawWindowHandle",
            "OptionThread",
            "OptionThreadSendMsg",
            "OptionTimer",
            "OptionThreadReceiveMsg",
            "ThreadSendMsg",
        ];
        if PYTHON_FORCE_UNSENDABLE_ENUMS.contains(&enum_def.name.as_str()) {
            return true; // Force unsendable for these types
        }

        // Check all variant payload types
        for variant in &enum_def.variants {
            match &variant.kind {
                EnumVariantKind::Tuple(types) => {
                    for (ty, _ref_kind) in types {
                        if self.field_type_needs_unsendable(ty, ir) {
                            return true;
                        }
                    }
                }
                EnumVariantKind::Struct(fields) => {
                    for field in fields {
                        if self.field_type_needs_unsendable(&field.type_name, ir) {
                            return true;
                        }
                    }
                }
                EnumVariantKind::Unit => {}
            }
        }

        false
    }

    /// Check if a field type (by name) requires unsendable
    ///
    /// This checks whether a type is NOT sendable.
    /// A type is sendable if:
    /// - It's a primitive
    /// - It has `is_send_safe: true` in the IR
    /// - It's in the PYTHON_SEND_SAFE_TYPES list
    /// - All its fields are transitively sendable
    fn field_type_needs_unsendable(&self, type_name: &str, ir: &CodegenIR) -> bool {
        // Primitives are always sendable
        if is_primitive_type(type_name) {
            return false;
        }

        // Python-specific: These types use *const/*mut c_void but are semantically Send
        const PYTHON_SEND_SAFE_TYPES: &[&str] = &[
            "CssPropertyCachePtr",
            "VirtualViewCallbackInfo",
            "VirtualViewReturn",
            "StyledDom",
            "LayoutCallbackInfo",
            "CallbackInfo",
            "RenderImageCallbackInfo",
            "RefCount",
            "OptionRefAny",
            "GlVoidPtrMut",
            "ParsedSvg",
            "ResultParsedSvgSvgParseError",
            "GridMinMax",
            "GridTrackSizing",
            // Window handle types - contain *mut c_void but are conceptually sendable
            "RawWindowHandle",
            "IOSHandle",
            "MacOSHandle",
            "XlibHandle",
            "XcbHandle",
            "WaylandHandle",
            "WindowsHandle",
            "WebHandle",
            "AndroidHandle",
            "OptionRawWindowHandle",
            // Thread types - contain Arc<Mutex<...>> which are Send
            "Thread",
            "OptionThread",
            "ThreadSender",
            "ThreadReceiver",
            "ThreadInner",
            "ThreadSendMsg",
            "OptionThreadSendMsg",
            "ThreadReceiveMsg",
            "OptionThreadReceiveMsg",
            // Timer types
            "Timer",
            "OptionTimer",
            "TimerCallbackInfo",
            "TimerCallbackReturn",
            // Callback types that have ctx (function pointers are usize internally)
            "GetSystemTimeCallback",
            "CheckThreadFinishedCallback",
            "LibrarySendThreadMsgCallback",
            "ThreadSenderInner",
            "ThreadReceiverInner",
        ];
        if PYTHON_SEND_SAFE_TYPES.contains(&type_name) {
            return false;
        }

        // Raw pointers in the type name itself - NOT sendable
        if type_name.contains("*const") || type_name.contains("*mut") {
            return true;
        }

        // Function pointers - NOT sendable
        if type_name.contains("extern") || type_name.contains("fn(") {
            return true;
        }

        // Box types - NOT sendable
        if type_name.starts_with("Box<") {
            return true;
        }

        // Types ending with "Callback" contain function pointers - NOT sendable
        if type_name.ends_with("Callback") || type_name.ends_with("CallbackType") {
            return true;
        }

        // Check if this is a type alias to a pointer type
        if let Some(type_alias) = ir.find_type_alias(type_name) {
            if type_alias.target.contains("*const") || type_alias.target.contains("*mut") {
                return true;
            }
            // A type alias to a generic instantiation (e.g.
            // `BoxOrStaticStyleBoxShadow` => `BoxOrStatic<StyleBoxShadow>`)
            // inherits sendability from its underlying generic type. The
            // generic `BoxOrStatic<T>` stores `*const T` / `*mut T`, so it is
            // NOT sendable. Resolve through to the target type so that any
            // pyclass embedding such an alias is correctly marked unsendable.
            if type_alias.target != type_name
                && self.field_type_needs_unsendable(&type_alias.target, ir)
            {
                return true;
            }
            // The generic arguments of the instantiation also contribute to
            // sendability: `StyleBoxShadowValue` =>
            // `CssPropertyValue<BoxOrStaticStyleBoxShadow>` is unsendable
            // because the *argument* (`BoxOrStatic<StyleBoxShadow>`) holds raw
            // pointers, even though `CssPropertyValue<T>` itself only sees the
            // generic parameter `T`.
            for arg in &type_alias.generic_args {
                if arg != type_name && self.field_type_needs_unsendable(arg, ir) {
                    return true;
                }
            }
        }

        // Check if it's a struct - use is_send_safe flag
        if let Some(struct_def) = ir.find_struct(type_name) {
            // If struct is marked send_safe, it's sendable
            if struct_def.is_send_safe {
                return false;
            }
            // Boxed types and callback wrappers are not sendable
            if struct_def.is_boxed || struct_def.callback_wrapper_info.is_some() {
                return true;
            }
            // Recursively check fields
            for field in &struct_def.fields {
                // A field stored behind a raw pointer (e.g. the `ptr: *mut
                // c_void` of a Box wrapper like `ComponentFieldTypeBox`) makes
                // the struct unsendable even when the pointee type name is a
                // primitive — the ref_kind, not the type_name, carries the
                // pointer-ness.
                if matches!(
                    field.ref_kind,
                    crate::codegen::v2::ir::FieldRefKind::Ptr
                        | crate::codegen::v2::ir::FieldRefKind::PtrMut
                ) {
                    return true;
                }
                if self.field_type_needs_unsendable(&field.type_name, ir) {
                    return true;
                }
            }
            // All fields are sendable, so this struct is sendable
            return false;
        }

        // Check if it's an enum - use is_send_safe flag
        if let Some(enum_def) = ir.find_enum(type_name) {
            // If enum is marked send_safe, it's sendable
            if enum_def.is_send_safe {
                return false;
            }
            // Recursively check variant payloads
            for variant in &enum_def.variants {
                match &variant.kind {
                    crate::codegen::v2::ir::EnumVariantKind::Tuple(types) => {
                        for (ty, ref_kind) in types {
                            // A variant that holds the payload behind a raw
                            // pointer (e.g. `BoxOrStatic::Boxed(*mut T)`) is
                            // not sendable, regardless of the payload type
                            // name (which may be a bare generic param `T`).
                            if matches!(
                                ref_kind,
                                crate::codegen::v2::ir::FieldRefKind::Ptr
                                    | crate::codegen::v2::ir::FieldRefKind::PtrMut
                            ) {
                                return true;
                            }
                            if self.field_type_needs_unsendable(ty, ir) {
                                return true;
                            }
                        }
                    }
                    crate::codegen::v2::ir::EnumVariantKind::Struct(fields) => {
                        for field in fields {
                            if self.field_type_needs_unsendable(&field.type_name, ir) {
                                return true;
                            }
                        }
                    }
                    crate::codegen::v2::ir::EnumVariantKind::Unit => {}
                }
            }
            // All variants are sendable
            return false;
        }

        // Unknown types - assume sendable (will fail at compile time if wrong)
        false
    }

    /// Check if a class (by name) needs unsendable
    /// Used for determining if &mut self methods should be skipped
    fn class_needs_unsendable(&self, class_name: &str, ir: &CodegenIR) -> bool {
        // Check struct
        if let Some(struct_def) = ir.find_struct(class_name) {
            return self.type_needs_unsendable(struct_def, ir);
        }
        // Check enum
        if let Some(enum_def) = ir.find_enum(class_name) {
            return self.enum_needs_unsendable(enum_def, ir);
        }
        // Unknown types default to unsendable for safety
        true
    }

    fn function_has_unsupported_args(&self, func: &FunctionDef, ir: &CodegenIR) -> bool {
        // For &mut self methods, only skip if the class is unsendable
        // Sendable classes can have mutable methods!
        if func.kind == FunctionKind::MethodMut {
            if self.class_needs_unsendable(&func.class_name, ir) {
                return true;
            }
            // Sendable class - &mut self is allowed, continue checking args
        }

        for arg in &func.args {
            // RefAny is ALWAYS allowed - becomes Py<PyAny>
            if arg.type_name == "RefAny" {
                continue;
            }

            // Callback types with callback_info are ALWAYS allowed - become Py<PyAny>
            // This check MUST come before is_python_compatible_type to allow CallbackType args
            if arg.callback_info.is_some() {
                continue;
            }

            // Skip raw pointer types. The pointer-ness is carried in `ref_kind`
            // (parse_type_ref_kind strips `*const`/`*mut` from `type_name`), so
            // a string `.contains` check alone misses e.g. `copy_from_ptr`'s
            // `ptr: *const ListViewRow` — Python cannot supply a raw pointer.
            if matches!(arg.ref_kind, ArgRefKind::Ptr | ArgRefKind::PtrMut) {
                return true;
            }
            if arg.type_name.contains("*const") || arg.type_name.contains("*mut") {
                return true;
            }
            // Skip VecRef types
            if arg.type_name.contains("VecRef") || arg.type_name == "Refstr" {
                return true;
            }
            // Skip generic instantiations (e.g., CssPropertyValue<StyleBoxShadow>)
            if arg.type_name.contains('<') && arg.type_name.contains('>') {
                return true;
            }
            // Skip array types
            if arg.type_name.starts_with('[') && arg.type_name.contains(';') {
                return true;
            }

            // Skip type aliases to generic types
            if !self.is_python_compatible_type(&arg.type_name, ir) {
                return true;
            }

            // Unrecognized CallbackType (no callback_info) - skip
            if arg.type_name.ends_with("CallbackType") {
                return true;
            }
        }

        if let Some(ret) = &func.return_type {
            if ret.contains("*const") || ret.contains("*mut") {
                return true;
            }
            if ret.contains("VecRef") || ret == "Refstr" {
                return true;
            }
            // Skip generic instantiations in return types
            if ret.contains('<') && ret.contains('>') {
                return true;
            }
            // Skip incompatible return types
            if !self.is_python_compatible_type(ret, ir) {
                return true;
            }
        }

        false
    }

    /// Returns true if `type_name` names a struct/enum that is NOT emitted as
    /// a pyclass wrapper (because `should_include_*` excludes it, e.g. the
    /// `Xml`/`XmlNodeChild` family in `config.skip_types`). Methods that take or
    /// return such a type cannot compile, because the type only exists in the
    /// raw `__dll_api_inner` module and lacks the `PyClass`/`FromPyObject`
    /// impls the pymethod signature needs.
    fn type_is_excluded(&self, type_name: &str, ir: &CodegenIR, config: &PythonConfig) -> bool {
        if let Some(struct_def) = ir.find_struct(type_name) {
            return !self.should_include_struct(struct_def, config);
        }
        if let Some(enum_def) = ir.find_enum(type_name) {
            return !self.should_include_enum(enum_def, config);
        }
        false
    }

    /// Returns true if a Python callable passed for this callback arg can be
    /// bridged to Rust. This requires BOTH:
    /// 1. A trampoline `extern "C"` fn is generated for the callback typedef
    ///    (mirrors the gating in `generate_callback_trampolines`), and
    /// 2. The wrapper struct exists and has an `OptionRefAny` field to store the
    ///    Python callable.
    /// When either is false there is no way to store/invoke the Python callable,
    /// so the consuming method must be skipped.
    fn callback_arg_is_bridgeable(&self, cb_info: &CallbackArgInfo, ir: &CodegenIR) -> bool {
        // (2) wrapper struct must exist and have an OptionRefAny callable slot.
        let wrapper = match ir.structs.iter().find(|s| s.name == cb_info.callback_wrapper_name) {
            Some(s) => s,
            None => return false,
        };
        if !wrapper.fields.iter().any(|f| f.type_name == "OptionRefAny") {
            return false;
        }

        // (1) a trampoline must be generated for the callback typedef. The
        // typedef name is the wrapper name + "Type" by convention; verify it
        // satisfies the same gating used in `generate_callback_trampolines`.
        let typedef = ir.callback_typedefs.iter().find(|c| {
            c.name == format!("{}Type", cb_info.callback_wrapper_name)
        });
        let typedef = match typedef {
            Some(t) => t,
            None => return false,
        };
        if typedef.name.ends_with("DestructorType")
            || typedef.name.ends_with("CloneCallbackType")
            || typedef.name.ends_with("DestructorCallbackType")
        {
            return false;
        }
        if typedef.args.is_empty() || typedef.args[0].type_name != "RefAny" {
            return false;
        }
        if typedef.args.iter().any(|a| a.type_name.contains("*")) {
            return false;
        }
        let return_type = typedef.return_type.as_deref().unwrap_or("()");
        if return_type == "ImageRef" {
            return false;
        }
        // The trampoline locates the Python callable by calling `get_ctx()` on a
        // non-RefAny, non-primitive argument (e.g. CallbackInfo). If every arg is
        // RefAny/primitive (e.g. DatasetMergeCallback `(RefAny, RefAny) -> RefAny`)
        // there is no way for the extern "C" trampoline to reach the stored
        // callable, so the method cannot be bridged.
        let has_ctx_source = typedef
            .args
            .iter()
            .any(|a| a.type_name != "RefAny" && !is_primitive_type(&a.type_name));
        if !has_ctx_source {
            return false;
        }
        true
    }

    /// Skip a function if any of its (non-callback, non-primitive) arg types or
    /// its return type is an excluded pyclass type. See `type_is_excluded`.
    fn function_refs_excluded_type(
        &self,
        func: &FunctionDef,
        ir: &CodegenIR,
        config: &PythonConfig,
    ) -> bool {
        for arg in &func.args {
            // Callback args become Py<PyAny>, but only if we can actually bridge
            // them: a trampoline must be generated AND the wrapper struct must have
            // an OptionRefAny field to store the Python callable. Callbacks like
            // GetSystemTimeCallback (no callable storage) or IconResolverCallback
            // (no wrapper struct / no RefAny-first-arg trampoline) cannot be
            // bridged, so the method must be skipped entirely.
            if let Some(ref cb_info) = arg.callback_info {
                if !self.callback_arg_is_bridgeable(cb_info, ir) {
                    return true;
                }
                continue;
            }
            // RefAny args become Py<PyAny>, never a pyclass arg.
            if arg.type_name == "RefAny" {
                continue;
            }
            // String args are bridged, not wrapped: Python `str` flows in via
            // pyo3's built-in FromPyObject, and `generate_pymethod` converts it
            // to AzString in the body (the `arg.type_name == "String"` arm).
            // `String` IS a struct in the IR but is emitted C-API-direct (no
            // pyclass wrapper), so `type_is_excluded` would otherwise drop EVERY
            // String-taking method (Button::create, Button::with_type,
            // Css::from_string, …). Mirror the RefAny case above.
            if arg.type_name == "String" {
                continue;
            }
            if self.type_is_excluded(&arg.type_name, ir, config) {
                return true;
            }
        }
        if let Some(ret) = &func.return_type {
            // String return is converted back to a Python `str` in the body
            // (into_library_owned_string), so the C-API-direct String struct's
            // lack of a pyclass wrapper must not drop the method.
            if ret != "String" && self.type_is_excluded(ret, ir, config) {
                return true;
            }
        }
        false
    }

    /// Check if a function has a recognized callback pattern:
    /// - Has an argument named "data" with type "RefAny"
    /// - Has an argument named "callback" with a recognized CallbackType (has callback_info)
    /// - Or has any argument that is a callback type (Py<PyAny>)
    fn has_callback_pattern(&self, func: &FunctionDef) -> bool {
        let has_refany = func
            .args
            .iter()
            .any(|a| a.name == "data" && a.type_name == "RefAny");
        let has_callback = func
            .args
            .iter()
            .any(|a| a.name == "callback" && a.callback_info.is_some());

        // Also check for any argument that has callback_info (for cases like layout_callback)
        let has_any_callback = func.args.iter().any(|a| a.callback_info.is_some());

        (has_refany && has_callback) || has_any_callback
    }

    /// Check if a type is compatible with Python bindings
    /// Uses structural analysis rather than hardcoded lists
    fn is_python_compatible_type(&self, type_name: &str, ir: &CodegenIR) -> bool {
        if is_primitive_type(type_name) {
            return true;
        }

        // Skip pointer types
        if type_name.contains('*') {
            return false;
        }

        // Skip RefAny and c_void
        if type_name == "RefAny" || type_name == "c_void" {
            return false;
        }

        // Skip VecRef types (by name pattern)
        if type_name.contains("VecRef") || type_name == "Refstr" {
            return false;
        }

        // Skip array types like [PixelValue; 2]
        if type_name.starts_with('[') && type_name.contains(';') {
            return false;
        }

        // Skip generic type parameters (single uppercase letters like T, U, V)
        if type_name.len() == 1
            && type_name
                .chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false)
        {
            return false;
        }

        // Skip generic instantiations like PhysicalPosition<i32>
        if type_name.contains('<') && type_name.contains('>') {
            return false;
        }

        // Skip callback wrapper types - these need special Py<PyAny> handling
        if is_callback_wrapper_type(type_name, ir) {
            return false;
        }

        // Skip C-API direct types that don't have .inner field.
        //
        // `String` is deliberately *not* in this list: AzString has
        // FromPyObject/IntoPyObject impls so Python `str` flows in
        // directly, and `generate_pymethod` knows how to shadow the
        // String-typed parameter with the AzString conversion.
        const CAPI_DIRECT: &[&str] = &[
            "U8Vec",
            "StringVec",
            "GLuintVec",
            "GLintVec",
            "RefAny",
            "U8VecDestructor",
            "StringVecDestructor",
            "InstantPtr",
            "StringMenuItem",
        ];
        if CAPI_DIRECT.contains(&type_name) {
            return false;
        }

        // Skip type_alias types that resolve to raw pointers (c_void with pointer)
        // These are platform-specific handles like HwndHandle, X11Visual, etc.
        const POINTER_TYPE_ALIASES: &[&str] = &[
            "HwndHandle",
            "X11Visual",
            "XWindowType",
            "XConnection",
            "WaylandHandle",
            "IOSHandle",
            "MacOSHandle",
            "AndroidHandle",
            // Add any other type_alias to c_void here
        ];
        if POINTER_TYPE_ALIASES.contains(&type_name) {
            return false;
        }

        // Skip type_aliases to generic types (like PhysicalPositionI32 → PhysicalPosition<i32>)
        // These resolve to generic FFI types which don't have FromPyObject impl
        const GENERIC_TYPE_ALIASES: &[&str] = &[
            "PhysicalPositionI32",
            "PhysicalPositionU32",
            "PhysicalPositionF32",
            "PhysicalPositionF64",
            "PhysicalSizeI32",
            "PhysicalSizeU32",
            "PhysicalSizeF32",
            "PhysicalSizeF64",
            "LogicalPositionI32",
            "LogicalPositionF32",
            "LogicalSizeI32",
            "LogicalSizeF32",
        ];
        if GENERIC_TYPE_ALIASES.contains(&type_name) {
            return false;
        }

        // Generalized version of GENERIC_TYPE_ALIASES: any type alias whose
        // target is a generic instantiation (e.g. `BoxOrStaticString` =>
        // `BoxOrStatic<AzString>`) resolves to a generic FFI type that has
        // neither a `.inner`-bearing pyclass wrapper nor a FromPyObject impl.
        // Treat these the same as the hardcoded generic aliases above so the
        // variants / arguments that reference them are skipped structurally
        // rather than emitting broken `v.inner` / pyo3-arg code.
        if let Some(type_alias) = ir.find_type_alias(type_name) {
            if !type_alias.generic_args.is_empty()
                || (type_alias.target.contains('<') && type_alias.target.contains('>'))
            {
                return false;
            }
        }

        // Skip type aliases for CssPropertyValue<T> (they end with "Value" and are not "PixelValue")
        // These can't be used as Python arguments because they resolve to generic types
        if type_name.ends_with("Value")
            && ![
                "PixelValue",
                "PixelValueNoPercent",
                "FloatValue",
                "PercentageValue",
                "AngleValue",
            ]
            .contains(&type_name)
        {
            return false;
        }

        // Skip destructor types (extern "C" fn types)
        if type_name.ends_with("Destructor") || type_name.ends_with("DestructorType") {
            return false;
        }

        // Note: U8Vec, StringVec, ImageRef, FontRef, Callback types etc. are NOT skipped here.
        // They ARE Python-compatible and have proper wrapper implementations.
        // Only truly incompatible types (raw pointers, VecRef, destructors) are skipped.
        true
    }

    /// Lookup the TypeCategory for a type name
    fn get_type_category(&self, type_name: &str, ir: &CodegenIR) -> TypeCategory {
        // Check structs first
        if let Some(s) = ir.find_struct(type_name) {
            return s.category;
        }
        // Check enums
        if let Some(e) = ir.find_enum(type_name) {
            return e.category;
        }
        // Check callback typedefs
        for cb in &ir.callback_typedefs {
            if cb.name == type_name {
                return TypeCategory::CallbackTypedef;
            }
        }
        // Check type aliases
        for ta in &ir.type_aliases {
            if ta.name == type_name {
                return TypeCategory::TypeAlias;
            }
        }
        // Default to Regular for unknown types
        TypeCategory::Regular
    }

    fn rust_type_to_python(&self, rust_type: &str, prefix: &str, ir: &CodegenIR) -> String {
        // Handle primitives directly
        if is_primitive_type(rust_type) {
            return rust_type.to_string();
        }
        if rust_type == "String" {
            return "String".to_string();
        }

        // Handle array types: [PixelValue; 2] -> [AzPixelValue; 2]
        let (ptr_prefix, base_type, array_suffix) = analyze_type(rust_type);

        // RefAny → Py<PyAny> (Python object that gets wrapped internally)
        if base_type == "RefAny" {
            return "Py<PyAny>".to_string();
        }

        // Callback typedef types (e.g., CallbackType, ButtonOnClickCallbackType) → Py<PyAny>
        // These are raw function pointer types that Python can't use directly
        // We accept a Python callable and use a trampoline to invoke it
        // EXCEPTION: Destructor callback types are internal and should NOT be exposed to Python
        // as Py<PyAny> - they are low-level function pointers for cleanup, not user callbacks
        if base_type.ends_with("CallbackType") && !base_type.contains("Destructor") {
            return "Py<PyAny>".to_string();
        }

        // Callback wrapper types (Callback, VirtualViewCallback, etc.) → Py<PyAny>
        // These get converted to a callback struct with a trampoline in the function body
        if is_callback_wrapper_type(&base_type, ir) {
            return "Py<PyAny>".to_string();
        }

        // Skip generic type parameters
        if base_type.len() == 1
            && base_type
                .chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false)
        {
            return rust_type.to_string(); // Return as-is, caller should skip
        }

        // For primitives in the base, don't prefix
        if is_primitive_type(&base_type) {
            return format!("{}{}{}", ptr_prefix, base_type, array_suffix);
        }

        // For complex types, add prefix to base type
        format!("{}{}{}{}", ptr_prefix, prefix, base_type, array_suffix)
    }

    fn find_external_path(&self, type_name: &str, ir: &CodegenIR) -> Option<String> {
        let path = if let Some(s) = ir.find_struct(type_name) {
            s.external_path.clone()
        } else if let Some(e) = ir.find_enum(type_name) {
            e.external_path.clone()
        } else if let Some(ta) = ir.type_aliases.iter().find(|ta| ta.name == type_name) {
            ta.external_path.clone()
        } else {
            for cb in &ir.callback_typedefs {
                if cb.name == type_name {
                    return cb
                        .external_path
                        .clone()
                        .map(|p| p.replace("azul_dll::", "crate::"));
                }
            }
            return None;
        };
        path.map(|p| p.replace("azul_dll::", "crate::"))
    }
}

fn is_primitive_type(name: &str) -> bool {
    matches!(
        name,
        "bool" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" |
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" |
        "f32" | "f64" | "char" | "()" | "c_void" |
        // core::ffi C numeric aliases — kept as aliases (imported in
        // generate_imports) for 32-bit/riscv ABI correctness, handled directly
        // as primitives rather than through the .inner wrapper path.
        "c_int" | "c_uint" | "c_float" | "c_double" |
        // GL type aliases (these are type aliases for primitive types)
        "GLuint" | "GLint" | "GLenum" | "GLint64" | "GLuint64" | "GLsizei" |
        "GLfloat" | "GLboolean" | "GLbitfield" | "GLclampf" | "GLsizeiptr" | "GLintptr"
    )
}

/// Check if a type is a callback wrapper struct (contains a function pointer + RefAny data)
/// These types need special handling: Python receives Py<PyAny>, and we construct
/// the callback with a trampoline function that invokes the Python callable.
///
/// Detection criteria (all must be true):
/// 1. Type is a struct (not enum, not type_alias)
/// 2. Type name ends with "Callback" but NOT "CallbackType" or "CallbackInfo"
/// 3. Type contains a field with a callback_typedef type as direct child
/// 4. Type has a "callable" field with type "OptionRefAny"
///
/// This information is pre-computed in the IR during the link_callback_wrappers phase.
fn is_callback_wrapper_type(type_name: &str, ir: &CodegenIR) -> bool {
    // Use the pre-computed callback_wrapper_info from the IR
    if let Some(struct_def) = ir.find_struct(type_name) {
        return struct_def.callback_wrapper_info.is_some();
    }
    false
}

/// Get the callback wrapper info for a type, if it is a callback wrapper
fn get_callback_wrapper_info<'a>(
    type_name: &str,
    ir: &'a CodegenIR,
) -> Option<&'a crate::codegen::v2::ir::CallbackWrapperInfo> {
    ir.find_struct(type_name)
        .and_then(|s| s.callback_wrapper_info.as_ref())
}

/// Check if a type is a direct FFI type (not wrapped in a struct with .inner)
/// These types are type-aliased directly to the C-API types in generate_python_patches_prefix()
fn is_direct_ffi_type(type_name: &str) -> bool {
    // Vec types that have direct type aliases (no .inner wrapper)
    const DIRECT_FFI_TYPES: &[&str] = &[
        // Core Vec types
        "StringVec",
        "U8Vec",
        "U16Vec",
        "U32Vec",
        "I32Vec",
        "F32Vec",
        // GL Vec types
        "GLuintVec",
        "GLintVec",
        // These might have FromPyObject/IntoPyObject implementations
        "String", // Already handled separately, but include for completeness
    ];

    DIRECT_FFI_TYPES.contains(&type_name) || type_name.ends_with("Vec")
}

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
