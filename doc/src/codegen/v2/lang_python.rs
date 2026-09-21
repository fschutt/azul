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
//! 2. **Different trait implementations**: Python uses transmute to azul_core, C-API generates
//!    C-ABI functions
//! 3. **Type filtering**: Python skips recursive types and VecRef types
//! 4. **Callback handling**: Python needs trampolines to route Python callables to Rust callbacks,
//!    which C doesn't need
//!
//! The Python generator does NOT share any generated code with the C-API generator.
//! They both read from the same IR but produce completely independent output.
//!
//! # Type Classification
//!
//! Types are now classified via TypeCategory in the IR, not ad-hoc constants here.
//! See ir.rs TypeCategory enum for the central classification system.

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    config::{CodegenConfig, PythonConfig},
    generator::{CodeBuilder, LanguageGenerator},
    ir::{
        ArgRefKind, CallbackArgInfo, CallbackTypedefDef, CodegenIR, EnumDef, EnumVariantKind,
        FunctionDef, FunctionKind, StructDef, TypeCategory,
    },
    lang_rust::RustGenerator,
};
use crate::utils::analyze::analyze_type;

// ============================================================================
// Constants
// ============================================================================

/// The Vec classes this emitter hands to Python as a BUILTIN instead of a
/// pyclass: the C type itself carries the `FromPyObject`/`IntoPyObject` impls
/// that `generate_pyo3_traits` writes out, so a Python `bytes` / `list[int]` /
/// `list[str]` flows straight through the FFI struct and no `.inner` wrapper
/// exists to route it through.
///
/// This is not a decision keyed on a name, it IS the index of the
/// hand-written conversion block in `generate_pyo3_traits` -- the two must
/// name the same classes or the binding stops compiling. Every OTHER Vec goes
/// through the ordinary pyclass path, which is why the IR's
/// `TypeCategory::Vec` cannot stand in for this list.
const PY_BUILTIN_VEC_CLASSES: &[&str] = &[ // allow-api-name: the index of the conversion block
    "U8Vec",
    "StringVec",
    "GLuintVec",
    "GLintVec",
];

/// Classes with no Python shape at all: they are neither a builtin nor a
/// pyclass, so a method that mentions one is not emitted.
///
/// A class is listed here because of what it HOLDS, and the IR does not model
/// that yet: one carries its own clone/destructor function pointers next to an
/// opaque `*const c_void` (there is nothing for Python to hold onto), the
/// other is the recursive knot of the menu tree (a menu item holds the vector
/// of its own children). Both want an IR flag; until one exists the names have
/// to be written down.
const PY_UNMODELLED_CLASSES: &[&str] = &[ // allow-api-name: no IR flag for these shapes yet
    "InstantPtr",
    "StringMenuItem",
];

/// Whether Python reaches this class through a builtin rather than a pyclass
/// wrapper: the two fundamental types every binding maps natively (the IR
/// classifies them, so no name appears here) plus the hand-bridged vectors.
///
/// A method taking or returning one of these is still emitted -- the value is
/// converted in the body -- which is why this is separate from
/// [`PythonGenerator::type_is_excluded`].
fn is_py_builtin_class(type_name: &str, ir: &CodegenIR) -> bool {
    if PY_BUILTIN_VEC_CLASSES.contains(&type_name) {
        return true;
    }
    matches!(
        category_of(type_name, ir),
        Some(TypeCategory::String) | Some(TypeCategory::RefAny)
    )
}

/// Whether this class gets no `#[pyclass]` wrapper: it is either reached
/// through a Python builtin or has no Python shape at all.
fn has_no_pyclass(type_name: &str, category: TypeCategory) -> bool {
    matches!(category, TypeCategory::String | TypeCategory::RefAny)
        || PY_BUILTIN_VEC_CLASSES.contains(&type_name)
        || PY_UNMODELLED_CLASSES.contains(&type_name)
}

/// The IR's classification of a class, or `None` when the name is not one
/// (a primitive, a generic parameter, a pointer spelling).
fn category_of(type_name: &str, ir: &CodegenIR) -> Option<TypeCategory> {
    ir.find_struct(type_name)
        .map(|s| s.category)
        .or_else(|| ir.find_enum(type_name).map(|e| e.category))
}

/// The application's own opaque data, which crosses as the Python object
/// itself rather than as a wrapper. The IR classifies it, so the decision
/// survives a rename.
fn is_refany(type_name: &str, ir: &CodegenIR) -> bool {
    category_of(type_name, ir) == Some(TypeCategory::RefAny)
}

/// The string class, which crosses as a Python `str`.
fn is_string_class(type_name: &str, ir: &CodegenIR) -> bool {
    category_of(type_name, ir) == Some(TypeCategory::String)
}

/// A bare function-pointer typedef: the IR keeps these in their own list, and
/// nothing Python can hold has that shape (a callable travels in the WRAPPER
/// that pairs the pointer with a context).
fn is_callback_typedef(type_name: &str, ir: &CodegenIR) -> bool {
    ir.callback_typedefs.iter().any(|c| c.name == type_name)
}

/// The type of a callback wrapper's context slot, read off the first wrapper
/// the IR linked (they all use the same optional-object type -- that is what
/// makes them wrappers). Used to recognise the accessor that hands the
/// context back, without naming either the type or the accessor.
fn context_slot_type(ir: &CodegenIR) -> Option<&str> {
    ir.structs.iter().find_map(|s| {
        let info = s.callback_wrapper_info.as_ref()?;
        s.fields
            .iter()
            .find(|f| f.name == info.context_field_name)
            .map(|f| f.type_name.as_str())
    })
}

/// The method of `class_name` that hands the stored context back to a
/// callback, if it has one.
///
/// A callback trampoline has to find the Python callable the binding put in
/// the wrapper's context slot, and it can only do that through an argument
/// whose class exposes that slot. The shape is unambiguous: an instance
/// method taking nothing but the receiver and returning the context type.
/// (Keying on the accessor's NAME instead meant the trampoline broke the day a
/// callback typedef gained a plain-data argument.)
fn context_accessor_name(class_name: &str, ir: &CodegenIR) -> Option<String> {
    let slot = context_slot_type(ir)?;
    ir.functions_for_class(class_name)
        .find(|f| {
            f.kind == FunctionKind::Method
                && f.return_type.as_deref() == Some(slot)
                && f.args.iter().all(|a| f.is_receiver_arg(a))
        })
        .map(|f| f.method_name.clone())
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
        let prev_is_ident = i > 0 && {
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
// Monomorphized generic aliases
// ============================================================================

/// The classes api.json spells as a generic instantiation, presented as the
/// struct or enum they instantiate.
///
/// WHY THIS EXISTS
/// ---------------
/// The entire CSS property-value surface is declared as a type alias:
/// `BoxDecorationBreakValue: { type_alias: { target: CssPropertyValue,
/// generic_args: [BoxDecorationBreak] } }`. `ir.find_struct` and
/// `ir.find_enum` return None for such a name, so every pass that walks
/// `ir.structs` / `ir.enums` alone skips 180 REAL types -- the C header emits
/// `union AzBoxDecorationBreakValue` for each of them, and the mirror in
/// `__dll_api_inner::dll` emits the instantiation as a `pub type`. Python had
/// no class for a single CSS value, and no way to build one.
///
/// The IR does carry the definition: `TypeAliasDef::monomorphized_def` is the
/// instantiated shape with the type parameter already substituted. Turning it
/// back into the `EnumDef` / `StructDef` it instantiates means the wrapper,
/// clone, debug, pymethods, dunder and registration passes treat an alias as
/// any other class, instead of each of them growing an alias branch that
/// drifts.
struct AliasClasses {
    /// Instantiations of a generic enum (`CssPropertyValue<T>`, `BoxOrStatic<T>`).
    enums: Vec<EnumDef>,
    /// Instantiations of a generic struct (`PhysicalSize<u32>`).
    structs: Vec<StructDef>,
}

/// Rebuild the `derive` list from the trait flags, so the helpers that ask a
/// def "were you declared `Clone`?" (`struct_supports_clone`,
/// `enum_supports_clone`) answer for a synthesized class exactly what they
/// answer for a declared one. The flags themselves come from api.json:
/// `ir_builder::inherited_alias_traits` keeps a target's derive only when
/// every type argument implements it too, which is the same bound the mirror's
/// `#[derive]` imposes on the instantiation.
fn derives_from_traits(traits: &super::ir::TypeTraits) -> Vec<String> {
    let mut out = Vec::new();
    for (has, name) in [
        (traits.is_copy, "Copy"),
        (traits.is_clone, "Clone"),
        (traits.is_debug, "Debug"),
        (traits.is_partial_eq, "PartialEq"),
        (traits.is_eq, "Eq"),
        (traits.is_partial_ord, "PartialOrd"),
        (traits.is_ord, "Ord"),
        (traits.is_hash, "Hash"),
        (traits.is_default, "Default"),
    ] {
        if has {
            out.push(name.to_string());
        }
    }
    out
}

/// The traits the MIRROR really implements for a monomorphized alias.
///
/// api.json's `derive` list for an alias says what the REAL type implements;
/// the pyclass wrapper can only delegate to what the mirror's INSTANTIATION
/// implements, and those two differ. The Rust emitter writes generic trait
/// impls for a generic ENUM template (`impl<T: PartialEq> PartialEq for
/// AzCssPropertyValue<T>`, and the same for Debug/Eq/Ord/Hash) but not for a
/// generic STRUCT template, which keeps only what `#[derive]` put on it. A
/// dunder that delegates to a trait the mirror does not have is a build
/// break, so the flags are narrowed to the instantiation's reality here --
/// once, rather than in each of the passes that reads them.
///
/// `Default` is deliberately left alone: the wrapper's default value is built
/// from the REAL type behind `external_path` (see
/// [`PythonGenerator::default_inner_expr`]), never from the mirror.
fn alias_mirror_traits(alias: &super::ir::TypeAliasDef, ir: &CodegenIR) -> super::ir::TypeTraits {
    let mut traits = alias.traits.clone();
    if ir.find_enum(&alias.target).is_some() {
        return traits;
    }
    // A generic struct template: `#[derive(Copy)] #[derive(Clone)]` and
    // nothing else reaches the instantiation.
    traits.is_debug = false;
    traits.is_partial_eq = false;
    traits.is_eq = false;
    traits.is_partial_ord = false;
    traits.is_ord = false;
    traits.is_hash = false;
    // ... and even Clone only when the template DERIVED it; a hand-written
    // `impl Clone` is not generic, so the emitter skips it for a template.
    if let Some(target) = ir.find_struct(&alias.target) {
        traits.is_clone = target.traits.is_clone && target.traits.clone_is_derived;
    }
    traits
}

/// A variant payload's type as the rest of this file must read it: a payload
/// held behind a raw pointer (`BoxOrStatic::Boxed(*mut T)`) keeps the pointer
/// IN THE NAME, because that is the spelling every compatibility check in this
/// file already rejects. Python cannot be handed a `*mut StyleBoxShadow`, and
/// silently dropping the pointer would emit `Boxed(v.inner)` against a
/// pointer-typed variant: a type error in generated code.
fn payload_type_name(type_name: &str, ref_kind: super::ir::FieldRefKind) -> String {
    use super::ir::FieldRefKind;
    match ref_kind {
        FieldRefKind::Ptr => format!("*const {}", type_name),
        FieldRefKind::PtrMut => format!("*mut {}", type_name),
        _ => type_name.to_string(),
    }
}

/// Synthesize the class definitions of every monomorphized alias.
fn alias_classes(ir: &CodegenIR) -> AliasClasses {
    use super::ir::{EnumVariantDef, MonomorphizedKind};

    let mut out = AliasClasses {
        enums: Vec::new(),
        structs: Vec::new(),
    };

    for alias in &ir.type_aliases {
        let Some(mono) = alias.monomorphized_def.as_ref() else {
            continue;
        };
        let traits = alias_mirror_traits(alias, ir);
        let derives = derives_from_traits(&traits);
        match &mono.kind {
            MonomorphizedKind::TaggedUnion { repr, variants } => {
                let variants = variants
                    .iter()
                    .map(|v| EnumVariantDef {
                        name: v.name.clone(),
                        doc: None,
                        kind: match &v.payload_type {
                            None => EnumVariantKind::Unit,
                            Some(t) => EnumVariantKind::Tuple(vec![(
                                payload_type_name(t, v.payload_ref_kind),
                                v.payload_ref_kind,
                            )]),
                        },
                    })
                    .collect();
                out.enums.push(EnumDef {
                    name: alias.name.clone(),
                    doc: alias.doc.clone(),
                    variants,
                    external_path: alias.external_path.clone(),
                    module: alias.module.clone(),
                    derives,
                    has_explicit_derive: true,
                    // A tagged union by construction: that is what this arm means.
                    is_union: true,
                    repr: repr.clone(),
                    // Send-ness is decided structurally from the payloads by
                    // `enum_needs_unsendable`, never asserted here.
                    is_send_safe: false,
                    traits: traits.clone(),
                    generic_params: Vec::new(),
                    // The instantiation is concrete; only the TEMPLATE it
                    // instantiates is a `GenericTemplate` (and stays skipped).
                    category: TypeCategory::Regular,
                    dependencies: Vec::new(),
                    sort_order: 0,
                    needs_forward_decl: false,
                });
            }
            MonomorphizedKind::SimpleEnum { repr, variants } => {
                out.enums.push(EnumDef {
                    name: alias.name.clone(),
                    doc: alias.doc.clone(),
                    variants: variants
                        .iter()
                        .map(|name| EnumVariantDef {
                            name: name.clone(),
                            doc: None,
                            kind: EnumVariantKind::Unit,
                        })
                        .collect(),
                    external_path: alias.external_path.clone(),
                    module: alias.module.clone(),
                    derives,
                    has_explicit_derive: true,
                    is_union: false,
                    repr: repr.clone(),
                    is_send_safe: false,
                    traits: traits.clone(),
                    generic_params: Vec::new(),
                    category: TypeCategory::Regular,
                    dependencies: Vec::new(),
                    sort_order: 0,
                    needs_forward_decl: false,
                });
            }
            MonomorphizedKind::Struct { fields } => {
                out.structs.push(StructDef {
                    name: alias.name.clone(),
                    doc: alias.doc.clone(),
                    fields: fields.clone(),
                    external_path: alias.external_path.clone(),
                    module: alias.module.clone(),
                    derives,
                    has_explicit_derive: true,
                    custom_impls: Vec::new(),
                    is_boxed: false,
                    repr: Some("C".to_string()),
                    is_send_safe: false,
                    generic_params: Vec::new(),
                    traits: traits.clone(),
                    category: TypeCategory::Regular,
                    dependencies: Vec::new(),
                    sort_order: 0,
                    needs_forward_decl: false,
                    callback_wrapper_info: None,
                });
            }
        }
    }

    out
}

/// Whether `type_name` is a monomorphized alias that therefore HAS a Python
/// class (see [`alias_classes`]).
fn is_alias_class(type_name: &str, ir: &CodegenIR) -> bool {
    ir.find_type_alias(type_name)
        .is_some_and(|a| a.monomorphized_def.is_some())
}

/// Whether the class is `Clone`, whichever of the three lists declares it.
///
/// It decides whether a value of that class can be handed OUT by value (a
/// field getter, an `as_<variant>()`), and a monomorphized alias answers here
/// like any other class -- without this it silently answered "no", which is
/// why the CSS property values could be built but never read back.
fn class_is_clone(type_name: &str, ir: &CodegenIR) -> bool {
    ir.find_struct(type_name)
        .map(|s| s.traits.is_clone)
        .or_else(|| ir.find_enum(type_name).map(|e| e.traits.is_clone))
        .or_else(|| ir.find_type_alias(type_name).map(|a| alias_mirror_traits(a, ir).is_clone))
        .unwrap_or(false)
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
        // A `PyRefMut` guard is how a method borrows ANOTHER pyclass mutably
        // for the length of a call: the argument shape for a C function that
        // writes through a pointer into the caller's own object.
        builder.line("#[allow(unused_imports)]");
        builder.line("use pyo3::PyRefMut;");
        builder.line(
            "use pyo3::types::{PyAny, PyAnyMethods, PyBytes, PyList, PyModule, PyModuleMethods, \
             PyString};",
        );
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

        // A Send/Sync ASSERTION about mirrors the IR cannot make: each of
        // these holds a `*const`/`*mut c_void` that is really a `&` or a
        // `Box`, so the shape says "not Send" while the value is. The IR's own
        // `is_send_safe` covers only the `vec` module (the loop below), so
        // until it can carry this the claim has to be written out. It is a
        // claim, not a fallback: getting it wrong is a data race, which is why
        // each entry says what it wraps.
        // allow-api-name: a per-class safety claim the IR has no flag for.
        const PYTHON_SEND_SAFE_TYPES: &[&str] = &[ // allow-api-name: see above
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

// The two GL vectors are `list[int]` on the Python side. Without these impls
// the whole `gen_*` / `get_*_iv` family of the OpenGL surface had no shape to
// return into and was dropped from the binding.
//
// Ownership: extracting ALLOCATES (the Rust buffer is handed over with the
// DefaultRust destructor, so the library frees it), and returning COPIES into
// the Python list and then drops `self`, whose `Drop` frees the C buffer. No
// buffer is ever shared between the two runtimes.
impl FromPyObject<'_, '_> for AzGLuintVec {
    type Error = PyErr;

    fn extract(ob: Borrowed<'_, '_, PyAny>) -> Result<Self, Self::Error> {
        let v: Vec<u32> = ob.extract()?;
        let ptr = v.as_ptr();
        let len = v.len();
        let cap = v.capacity();
        core::mem::forget(v);

        Ok(AzGLuintVec {
            ptr,
            len,
            cap,
            destructor: AzGLuintVecDestructor::DefaultRust,
        })
    }
}

impl<'py> IntoPyObject<'py> for AzGLuintVec {
    type Target = PyList;
    type Output = Bound<'py, PyList>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        PyList::new(py, az_gluintvec_to_py_vecu32(self))
    }
}

impl FromPyObject<'_, '_> for AzGLintVec {
    type Error = PyErr;

    fn extract(ob: Borrowed<'_, '_, PyAny>) -> Result<Self, Self::Error> {
        let v: Vec<i32> = ob.extract()?;
        let ptr = v.as_ptr();
        let len = v.len();
        let cap = v.capacity();
        core::mem::forget(v);

        Ok(AzGLintVec {
            ptr,
            len,
            cap,
            destructor: AzGLintVecDestructor::DefaultRust,
        })
    }
}

impl<'py> IntoPyObject<'py> for AzGLintVec {
    type Target = PyList;
    type Output = Bound<'py, PyList>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        PyList::new(py, az_glintvec_to_py_veci32(self))
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

        // One trampoline per callback KIND - every typedef a callback wrapper
        // struct holds (by structure): a Python callable is registered as the
        // wrapper's context, so a typedef without a wrapper (a destructor, a
        // clone function) has nothing to bridge. The layout callback is not
        // special: it reaches its callable through the context like every
        // other kind.
        for callback in super::managed_host_invoker::host_invoker_kinds(ir) {
            if trampoline_bridges(callback, ir) {
                self.generate_callback_trampoline(builder, callback, ir, prefix);
            }
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
        let wrapper = super::managed_host_invoker::wrapper_name(callback);
        let trampoline_name = format!("invoke_py_{}", to_snake_case(wrapper));

        // Build argument signature
        let mut args_sig = String::new();
        let mut ctx_source_type = String::new();
        let mut ctx_source_arg_name = String::new();
        let mut ctx_source_getter = String::new();

        // Find the first argument whose class can hand the stored context
        // back (the info object of the kind: a CallbackInfo, a
        // TimerCallbackInfo, ...). Only a class that actually EXPOSES the
        // context slot can carry the Python callable -- the older "first
        // non-data, non-primitive argument" assumption broke as soon as a
        // callback typedef gained a plain-data argument (an op name, a tween
        // description).
        for (i, arg) in callback.args.iter().enumerate() {
            if !ctx_source_type.is_empty() || is_refany(&arg.type_name, ir) {
                continue;
            }
            if let Some(getter) = context_accessor_name(&arg.type_name, ir) {
                ctx_source_type = arg.type_name.clone();
                ctx_source_getter = getter;
                ctx_source_arg_name = trampoline_arg_name(i);
            }
        }

        for (i, arg) in callback.args.iter().enumerate() {
            let arg_name = trampoline_arg_name(i);

            let arg_type_external = if is_primitive_type(&arg.type_name) {
                arg.type_name.clone()
            } else if let Some(ext) = self.find_external_path(&arg.type_name, ir) {
                ext
            } else {
                format!("__dll_api_inner::dll::{}{}", prefix, arg.type_name)
            };
            // The C signature of the typedef, exactly: a `&mut RefAny` data
            // argument (MarginBoxCallback) is a pointer to the ENGINE's RefAny,
            // which a by-value parameter would drop on return.
            let arg_type_external = match arg.ref_kind {
                ArgRefKind::Owned => arg_type_external,
                ArgRefKind::Ref => format!("&{}", arg_type_external),
                ArgRefKind::RefMut => format!("&mut {}", arg_type_external),
                ArgRefKind::Ptr => format!("*const {}", arg_type_external),
                ArgRefKind::PtrMut => format!("*mut {}", arg_type_external),
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

        // The kind's fallback (no callable, a Python exception, a wrong return
        // type) is the engine's own - the one its host-invoker thunk returns -
        // so every binding degrades the same way (a merge keeps the fresh
        // dataset, a caret tween renders the current caret, ...).
        let wrapper_external = self
            .find_external_path(wrapper, ir)
            .unwrap_or_else(|| format!("__dll_api_inner::dll::{}{}", prefix, wrapper));
        let fallback_args: Vec<String> = (0..callback.args.len())
            .map(|i| match i {
                0 => "&data".to_string(),
                1 => "&info".to_string(),
                i => format!("&arg{}", i),
            })
            .collect();
        let default_expr = format!(
            "{}::fallback_return({})",
            wrapper_external,
            fallback_args.join(", ")
        );

        builder.line(&format!(
            "/// Trampoline for {} - bridges Python to Rust",
            callback.name
        ));
        if args_sig.is_empty() {
            builder.line(&format!(
                "extern \"C\" fn {}() -> {} {{",
                trampoline_name, return_type_external
            ));
        } else {
            builder.line(&format!("extern \"C\" fn {}(", trampoline_name));
            builder.line(&format!("    {}", args_sig));
            builder.line(&format!(") -> {} {{", return_type_external));
        }
        builder.indent();

        builder.line(&format!("let default = {};", default_expr));
        builder.blank();

        // WHERE A FAILING CALLBACK IS REPORTED
        // ------------------------------------
        // An exception must never unwind out of this `extern "C"` frame and
        // into the engine -- that is undefined behaviour, not a crash with a
        // traceback -- so the boundary catches it and returns the kind's own
        // fallback. Catching it is not enough: a failure nobody sees is a
        // blank window with no explanation. It goes to stderr AND, when one
        // of this kind's arguments carries the application's log sink, to
        // that sink, which is what reaches whatever the app pipes its logs
        // into. The sink is found by SHAPE (see `log_sink_method`), so a
        // kind gains one the moment its info object declares one.
        //
        // The handle is taken here, before anything is handed to Python: the
        // argument itself is moved into the call further down. A clone is as
        // good as the original -- the sink writes to the process-wide log,
        // not into the value.
        let log_sink = callback.args.iter().enumerate().find_map(|(i, arg)| {
            let (method, level) = log_sink_method(&arg.type_name, ir)?;
            let arg_external = self
                .find_external_path(&arg.type_name, ir)
                .unwrap_or_else(|| format!("crate::{}", arg.type_name));
            let level_external = self
                .find_external_path(&level.name, ir)
                .unwrap_or_else(|| format!("crate::{}", level.name));
            Some((
                trampoline_arg_name(i),
                arg_external,
                method.method_name.clone(),
                level_external,
            ))
        });
        if let Some((arg_name, arg_external, _, _)) = &log_sink {
            builder.line(&format!(
                "let mut __log_sink: {} = {}.clone();",
                arg_external, arg_name
            ));
            builder.blank();
        }

        // The application's own object, unwrapped back into the Python object
        // it holds. A kind that takes no arguments has none to unwrap: nothing
        // is passed to the callable, and the context comes from the
        // invocation slot below.
        let carries_data = callback_carries_data(callback, ir);
        if carries_data {
            builder.line("let mut data_core = data;");
            builder
                .line("let py_data_wrapper = match data_core.downcast_ref::<PyDataWrapper>() {");
            builder.line("    Some(s) => s,");
            builder.line("    None => return default,");
            builder.line("};");
            builder.line("let py_data = match py_data_wrapper._py_data.as_ref() {");
            builder.line("    Some(s) => s,");
            builder.line("    None => return default,");
            builder.line("};");
            builder.blank();
        }

        {
            if ctx_source_type.is_empty() {
                // No argument carries the context: libazul hands it over
                // through the invocation slot, keyed by this function's
                // address (the wrapper's `cb`).
                builder.line(&format!(
                    "let callable_opt = azul_core::host_invoker::invocation_ctx({} as usize);",
                    trampoline_name
                ));
            } else {
                let ctx_external = self
                    .find_external_path(&ctx_source_type, ir)
                    .unwrap_or_else(|| {
                        format!("__dll_api_inner::dll::{}{}", prefix, ctx_source_type)
                    });
                // Clone the source to avoid move issues when it's also used for Python wrapper
                builder.line(&format!(
                    "let ctx_source_ffi: __dll_api_inner::dll::{}{} = unsafe {{ \
                     mem::transmute({}.clone()) }};",
                    prefix, ctx_source_type, ctx_source_arg_name
                ));
                builder.line(&format!(
                    "let ctx_source_rust: &{} = unsafe {{ mem::transmute(&ctx_source_ffi) }};",
                    ctx_external
                ));
                builder.line(&format!(
                    "let callable_opt = ctx_source_rust.{}();",
                    ctx_source_getter
                ));
            }
            builder.line("let callable_refany = match callable_opt {");
            builder.line("    azul_core::refany::OptionRefAny::Some(r) => r,");
            builder.line("    azul_core::refany::OptionRefAny::None => return default,");
            builder.line("};");
            builder.line("let mut callable_core = callable_refany;");
            builder.line(
                "let py_callable_wrapper = match \
                 callable_core.downcast_ref::<PyCallableWrapper>() {",
            );
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

        // The argument that carries the callback's context is its info object
        // (`CallbackInfo`, `LayoutCallbackInfo`, ...); Python receives it as the
        // second positional argument. Every other argument follows, converted.
        let info_type_for_python = Some(ctx_source_type.clone()).filter(|t| !t.is_empty());

        if let Some(ref info_type) = info_type_for_python {
            let info_arg_idx = callback
                .args
                .iter()
                .position(|arg| &arg.type_name == info_type)
                .unwrap();
            let info_arg_name = trampoline_arg_name(info_arg_idx);
            builder.line(&format!(
                "let info_ffi_py: __dll_api_inner::dll::{}{} = unsafe {{ mem::transmute({}) }};",
                prefix, info_type, info_arg_name
            ));
            builder.line(&format!(
                "let info_py = {}{} {{ inner: info_ffi_py }};",
                prefix, info_type
            ));
        }

        // Pass every argument, not just (data, info). A kind with no
        // arguments calls the callable with an empty tuple.
        let mut call_args: Vec<String> = Vec::new();
        if carries_data {
            call_args.push("py_data.clone_ref(py)".to_string());
        }
        if info_type_for_python.is_some() {
            call_args.push("info_py".to_string());
        }
        for (i, arg) in callback.args.iter().enumerate() {
            if i == 0 || info_type_for_python.as_deref() == Some(arg.type_name.as_str()) {
                continue; // `data` and the info object are already in the tuple
            }
            let arg_name = trampoline_arg_name(i);
            let py_name = format!("extra_arg{}_py", i);
            if is_refany(&arg.type_name, ir) {
                // A write-back's incoming data: the Python object inside, if any.
                builder.line(&format!(
                    "let {}: Option<Py<PyAny>> = {{ let mut refany = {}; \
                     refany.downcast_ref::<PyDataWrapper>().and_then(|w| \
                     w._py_data.as_ref().map(|o| o.clone_ref(py))) }};",
                    py_name, arg_name
                ));
            } else if is_primitive_type(&arg.type_name) {
                builder.line(&format!("let {} = {};", py_name, arg_name));
            } else if is_string_class(&arg.type_name, ir) {
                builder.line(&format!(
                    "let {}: String = {{ let s: azul_css::corety::AzString = unsafe {{ \
                     mem::transmute({}) }}; s.as_str().to_string() }};",
                    py_name, arg_name
                ));
            } else if self.is_python_compatible_type(&arg.type_name, ir)
                && !is_direct_ffi_type(&arg.type_name, ir)
            {
                builder.line(&format!(
                    "let {} = {}{} {{ inner: unsafe {{ mem::transmute({}) }} }};",
                    py_name, prefix, arg.type_name, arg_name
                ));
            } else {
                continue;
            }
            call_args.push(py_name);
        }
        let call_args = if call_args.len() == 1 {
            format!("{},", call_args[0]) // single-element tuple needs a trailing comma
        } else {
            call_args.join(", ")
        };

        builder.line(&format!("match py_callable.call1(py, ({})) {{", call_args));
        builder.indent();
        builder.line("Ok(result) => {");
        builder.indent();

        if return_type == "()" {
            builder.line("()");
        } else if is_refany(return_type, ir) {
            // A RefAny result (a merged dataset): the Python object the
            // callable returned, in a fresh RefAny; `None` keeps the fallback.
            builder.line("if result.is_none(py) {");
            builder.line("    default");
            builder.line("} else {");
            builder.line(
                "    create_py_refany_with_json(PyDataWrapper { _py_data: Some(result) })",
            );
            builder.line("}");
        } else if is_string_class(return_type, ir) {
            builder.line("match result.extract::<String>(py) {");
            builder.line("    Ok(s) => azul_css::corety::AzString::from(s),");
            builder.line("    Err(e) => {");
            builder.line("        if !result.is_none(py) {");
            builder.line(&format!(
                "            eprintln!(\"azul: {} callback returned an unexpected type (expected \
                 str), using default return value:\");",
                callback.name
            ));
            builder.line("            pyo3::PyErr::from(e).print(py);");
            builder.line("        }");
            builder.line("        default");
            builder.line("    }");
            builder.line("}");
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
                "            eprintln!(\"azul: {} callback returned an unexpected type (expected \
                 {}), using default return value:\");",
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
        if let Some((_, _, method, level_external)) = &log_sink {
            // ... and into the application's log, where a monitored app can
            // see it. `PyErr`'s Display is `TypeName: message`, the same
            // shape every binding reports. The traceback stays on stderr:
            // the sink takes one line, not a stack.
            builder.line(&format!(
                "__log_sink.{method}({level}::{severity}, \
                 azul_css::corety::AzString::from(format!(\"azul: unhandled Python exception in \
                 {kind} callback: {{}}\", e)));",
                method = method,
                level = level_external,
                severity = SEVERITY_ERROR,
                kind = callback.name,
            ));
        }
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

        builder.line(
            "// ============================================================================",
        );
        builder.line("// MONOMORPHIZED GENERIC ALIASES");
        builder.line(
            "// ============================================================================",
        );
        builder.blank();

        // The mirror emits each of these as `pub type AzFooValue =
        // AzCssPropertyValue<AzFoo>;`, so the wrapper below is a pyclass over a
        // concrete instantiation and needs no generic machinery of its own.
        let aliases = alias_classes(ir);
        for struct_def in &aliases.structs {
            if !self.should_include_struct(struct_def, config) {
                continue;
            }
            self.generate_struct_wrapper(builder, struct_def, prefix, ir);
        }
        for enum_def in &aliases.enums {
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

        // Monomorphized aliases. The mirror's `#[derive(Clone)]` on the
        // TEMPLATE carries a `T: Clone` bound, and the alias only claims
        // `Clone` when every type argument declares it (see
        // `derives_from_traits`), so these two conditions are the same one.
        let aliases = alias_classes(ir);
        for def in aliases
            .structs
            .iter()
            .filter(|s| self.should_include_struct(s, config) && self.struct_supports_clone(s))
            .map(|s| &s.name)
            .chain(
                aliases
                    .enums
                    .iter()
                    .filter(|e| self.should_include_enum(e, config) && self.enum_supports_clone(e))
                    .map(|e| &e.name),
            )
        {
            let name = format!("{}{}", prefix, def);
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

        // Monomorphized aliases. Unlike a declared class, the mirror does NOT
        // implement `Debug` for every instantiation: the template's impl reads
        // `impl<T: Debug> Debug for AzCssPropertyValue<T>`, so delegating is
        // only legal when the type argument is `Debug` -- which is exactly what
        // the alias's own `is_debug` records. The impl is still emitted in the
        // other case (printing the class name), because `__str__` and
        // `__repr__` format the wrapper with `{:?}` and a class without Debug
        // would not compile at all.
        let aliases = alias_classes(ir);
        for (class, is_debug) in aliases
            .structs
            .iter()
            .filter(|s| self.should_include_struct(s, config))
            .map(|s| (&s.name, s.traits.is_debug))
            .chain(
                aliases
                    .enums
                    .iter()
                    .filter(|e| self.should_include_enum(e, config))
                    .map(|e| (&e.name, e.traits.is_debug)),
            )
        {
            let name = format!("{}{}", prefix, class);
            builder.line(&format!("impl core::fmt::Debug for {} {{", name));
            builder.line("    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {");
            if is_debug {
                builder.line("        core::fmt::Debug::fmt(&self.inner, f)");
            } else {
                builder.line(&format!("        f.write_str(\"{}\")", class));
            }
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

        // Monomorphized aliases get the SAME treatment, which is the point of
        // synthesizing them: variant constructors (`StyleCursorValue.Exact(c)`),
        // variant tests, the derive dunders, `clone` and `createDefault`.
        let aliases = alias_classes(ir);
        for struct_def in &aliases.structs {
            if !self.should_include_struct(struct_def, config) {
                continue;
            }
            self.generate_struct_pymethods(builder, struct_def, ir, prefix, config);
        }
        for enum_def in &aliases.enums {
            if !self.should_include_enum(enum_def, config) {
                continue;
            }
            self.generate_enum_pymethods(builder, enum_def, ir, prefix, config);
        }

        Ok(())
    }

    /// The Python-visible forms of the api.json `derive` list.
    ///
    /// WHY THIS IS NEEDED AT ALL
    /// -------------------------
    /// The embedded mirror in `__dll_api_inner::dll` already carries the real
    /// `impl PartialEq` / `impl Ord` / `impl core::hash::Hash` for every class
    /// whose api.json `derive` list asks for them, and this file already emits
    /// `impl core::fmt::Debug` for the pyclass on top of that. NONE of it was
    /// reachable from Python: a Rust trait impl is not a dunder, so
    /// `a == b` fell back to identity comparison (silently wrong, never an
    /// error), `hash(a)` hashed the address, `sorted(xs)` raised TypeError and
    /// `copy.copy(a)` could not work. 4671 declared derives, of which 1321
    /// equalities.
    ///
    /// WHAT IS EMITTED, AND UNDER EXACTLY WHICH CONDITION
    /// -------------------------------------------------
    /// Each dunder delegates to `self.inner`, so it is emitted under precisely
    /// the condition that makes the corresponding trait impl exist on the
    /// mirror -- the same expressions `generate_capi_derived_trait_impls` uses,
    /// including the two supertrait closures (`PartialEq` also when only
    /// `PartialOrd` was declared, `PartialOrd` also when only `Ord` was). If
    /// those two ever disagree this stops compiling, which is the right failure:
    /// a dunder that names a trait the mirror does not implement is not a
    /// binding, it is a build break waiting for someone else.
    ///
    /// `taken` is the set of Python-visible names already emitted into this
    /// `#[pymethods]` block from api.json. It matters for exactly one name:
    /// several classes declare their own `default` method, and pyo3 rejects two
    /// methods with the same Python name.
    ///
    /// `default_inner` is the expression that produces a default INNER value
    /// (see [`PythonGenerator::default_inner_expr`]); it differs between a
    /// declared class and a monomorphized alias.
    fn generate_derive_dunders(
        &self,
        builder: &mut CodeBuilder,
        traits: &super::ir::TypeTraits,
        taken: &BTreeSet<String>,
        default_inner: &str,
    ) {
        // Mirrors `generate_capi_derived_trait_impls`: `PartialOrd: PartialEq`
        // and `Ord: Eq + PartialOrd`, so a class declaring only the stronger
        // trait still has the weaker impl on the mirror.
        let has_eq = traits.is_partial_eq || traits.is_partial_ord;
        let has_ord = traits.is_partial_ord || traits.is_ord;

        if has_eq {
            // `&Self` is pyo3's documented shape for the comparison slots; a
            // comparison against an unrelated Python object cannot extract and
            // is answered by pyo3 itself, not by this body.
            builder.line("fn __eq__(&self, other: &Self) -> bool {");
            builder.line("    self.inner == other.inner");
            builder.line("}");
            builder.blank();
            builder.line("fn __ne__(&self, other: &Self) -> bool {");
            builder.line("    !(self.inner == other.inner)");
            builder.line("}");
            builder.blank();
        }

        if has_ord && has_eq {
            for (dunder, op) in [
                ("__lt__", "<"),
                ("__le__", "<="),
                ("__gt__", ">"),
                ("__ge__", ">="),
            ] {
                builder.line(&format!("fn {}(&self, other: &Self) -> bool {{", dunder));
                builder.line(&format!("    self.inner {} other.inner", op));
                builder.line("}");
                builder.blank();
            }
        }

        if traits.is_hash {
            // Hashes the mirror through the SAME `Hash` impl the mirror
            // exposes, so `a == b` implies `hash(a) == hash(b)`. The value is
            // not stable across builds (`DefaultHasher` is unspecified), which
            // is also true of Python's own `hash` for str/bytes, so nothing may
            // persist it.
            builder.line("fn __hash__(&self) -> u64 {");
            builder.line("    use core::hash::{Hash, Hasher};");
            builder.line("    let mut h = std::collections::hash_map::DefaultHasher::new();");
            builder.line("    self.inner.hash(&mut h);");
            builder.line("    h.finish()");
            builder.line("}");
            builder.blank();
        }

        if traits.is_clone {
            // `__copy__` and `__deepcopy__` are BOTH real deep copies: the
            // mirror's `Clone` is `Az{T}_clone`, which deep-copies every owned
            // buffer. There is no shallow copy to offer -- two mirrors sharing
            // one heap buffer would double-free -- so `copy.copy` doing what
            // `copy.deepcopy` does is the honest mapping, not a shortcut.
            builder.line("fn __copy__(&self) -> Self {");
            builder.line("    Self { inner: self.inner.clone() }");
            builder.line("}");
            builder.blank();
            builder.line("fn __deepcopy__(&self, _memo: Bound<'_, PyAny>) -> Self {");
            builder.line("    Self { inner: self.inner.clone() }");
            builder.line("}");
            builder.blank();

            // The api.json trait function itself (`Az{T}_clone`, the
            // `DeepCopy` kind), under the name every other binding gives it.
            // It carries no fn_body -- the IR synthesises it from the derive
            // list -- so `generate_pymethod` can only leave a comment behind,
            // and Python was left with a deep copy it could reach from
            // `copy.deepcopy` but not call. A class that declares its own
            // `clone` in api.json keeps that one.
            if !taken.contains("clone") {
                builder.line("fn clone(&self) -> Self {");
                builder.line("    Self { inner: self.inner.clone() }");
                builder.line("}");
                builder.blank();
            }
        }

        if traits.is_default {
            if !taken.contains("default") {
                builder.line("#[staticmethod]");
                builder.line("fn default() -> Self {");
                builder.line(&format!("    Self {{ inner: {} }}", default_inner));
                builder.line("}");
                builder.blank();
            }
            // Same value under the api.json name of the `Default` trait
            // function (`Az{T}_createDefault`), for the same reason as
            // `clone` above: it is a declared entry point of the API, and
            // `default` alone leaves it unreachable for anyone following the
            // api.json or another binding.
            if !taken.contains("createDefault") {
                builder.line("#[staticmethod]");
                builder.line("fn createDefault() -> Self {");
                builder.line(&format!("    Self {{ inner: {} }}", default_inner));
                builder.line("}");
                builder.blank();
            }
        }
    }

    /// api.json's constants, as class attributes of the class that owns them.
    ///
    /// WHAT WAS MISSING
    /// ----------------
    /// All 1436 constants are the OpenGL enum values (`ACCUM_ALPHA_BITS =
    /// 0x0D5B`), and Python reached NONE of them: every GL call from Python
    /// had to be handed a magic number typed out by hand, with no way to
    /// check it against the header. C emits them as `#define`s, Zig as
    /// `pub const`s; this is the same surface.
    ///
    /// HOW THEY ARE PLACED
    /// -------------------
    /// The IR names a constant `<Class>_<NAME>` (`build_constants`), which is
    /// how it records which class owns it -- so the owner is read back the
    /// same way, not matched against any name. pyo3 turns an associated const
    /// carrying `#[classattr]` into a class attribute, so the value reads
    /// `azul.GlContextPtr.ACCUM_ALPHA_BITS`: the constant's own spelling, on
    /// the class whose methods take it.
    ///
    /// A constant never shadows a method: `taken` holds the Python-visible
    /// names already emitted into this block, and a Rust `impl` cannot carry
    /// a `const` and a `fn` of the same name either.
    fn generate_class_constants(
        &self,
        builder: &mut CodeBuilder,
        class_name: &str,
        ir: &CodegenIR,
        taken: &mut BTreeSet<String>,
    ) {
        for constant in &ir.constants {
            let Some((owner, _)) = constant.name.split_once('_') else {
                continue;
            };
            let name = &constant.member_name();
            if owner != class_name || !taken.insert(name.to_string()) {
                continue;
            }
            builder.line("#[classattr]");
            builder.line(&format!(
                "const {}: {} = {};",
                name, constant.type_name, constant.value
            ));
            builder.blank();
        }
    }

    /// The expression that builds a default INNER value for `class_name`.
    ///
    /// A declared class has `impl Default` on its mirror (the transmute impl
    /// the Rust emitter writes next to the type), so `Default::default()`
    /// resolves. A monomorphized alias does NOT: its mirror is an
    /// instantiation of a generic (`AzCssPropertyValue<AzStyleCursor>`), and
    /// the mirror carries no `Default` impl for the template -- only the real
    /// type behind `external_path` implements it. So build the real value and
    /// transmute it into the mirror, which is character-for-character what the
    /// C API's `Az{T}_createDefault` body does for the same alias.
    fn default_inner_expr(&self, class_name: &str, ir: &CodegenIR, prefix: &str) -> String {
        if !is_alias_class(class_name, ir) {
            return "Default::default()".to_string();
        }
        let external = self
            .find_external_path(class_name, ir)
            .unwrap_or_else(|| format!("crate::{}", class_name));
        format!(
            "unsafe {{ core::mem::transmute::<{ext}, __dll_api_inner::dll::{prefix}{class}>(<{ext} \
             as Default>::default()) }}",
            ext = external,
            prefix = prefix,
            class = class_name,
        )
    }

    /// `#[getter]`/`#[setter]` for public fields Python can represent.
    fn generate_field_accessors(
        &self,
        builder: &mut CodeBuilder,
        struct_def: &StructDef,
        ir: &CodegenIR,
        prefix: &str,
        taken: &BTreeSet<String>,
    ) {
        const RUST_KEYWORDS: &[&str] = &[
            "as", "async", "await", "box", "break", "const", "continue", "crate", "dyn",
            "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let",
            "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "static",
            "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while",
        ];
        // Python keywords cannot be attribute names written as `obj.name`.
        const PYTHON_KEYWORDS: &[&str] = &[
            "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
            "del", "elif", "else", "except", "finally", "for", "from", "global", "if",
            "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise",
            "return", "try", "while", "with", "yield", "None", "True", "False",
        ];
        let is_clone = |type_name: &str| class_is_clone(type_name, ir);

        for field in &struct_def.fields {
            let n = field.name.as_str();
            if !field.is_public
                || field.ref_kind != crate::codegen::v2::ir::FieldRefKind::Owned
                || RUST_KEYWORDS.contains(&n)
                || PYTHON_KEYWORDS.contains(&n)
                || taken.contains(n)
            {
                continue;
            }
            let t = field.type_name.as_str();
            if is_primitive_type(t) {
                builder.line(&format!("#[getter({})]", n));
                builder.line(&format!("fn __get_{}(&self) -> {} {{ self.inner.{} }}", n, t, n));
                builder.blank();
                builder.line(&format!("#[setter({})]", n));
                builder.line(&format!(
                    "fn __set_{}(&mut self, value: {}) {{ self.inner.{} = value; }}",
                    n, t, n
                ));
                builder.blank();
            } else if is_string_class(t, ir) {
                builder.line(&format!("#[getter({})]", n));
                builder.line(&format!(
                    "fn __get_{}(&self) -> String {{ let s: &azul_css::corety::AzString = \
                     unsafe {{ mem::transmute(&self.inner.{}) }}; s.as_str().to_string() }}",
                    n, n
                ));
                builder.blank();
                builder.line(&format!("#[setter({})]", n));
                builder.line(&format!(
                    "fn __set_{}(&mut self, value: String) {{ self.inner.{} = unsafe {{ \
                     mem::transmute(azul_css::corety::AzString::from(value)) }}; }}",
                    n, n
                ));
                builder.blank();
            } else if self.is_python_compatible_type(t, ir)
                && (!is_direct_ffi_type(t, ir)
                    // A Vec-typed FIELD is not a Vec ARGUMENT. The direct-FFI
                    // exclusion exists because a Vec crosses a CALL as a Python
                    // list; as a field it is an attribute, and the wrapper class
                    // for it already exists. Excluding it here silently dropped
                    // the accessor for every Vec field in the API
                    // (`ComponentLibrary.components`, `Dom.children`, ...) -
                    // the field was simply unreachable from Python. Strings keep
                    // the exclusion: they are handled by the branch above.
                    || (category_of(t, ir) == Some(TypeCategory::Vec)
                        // ... but only a Vec that HAS a wrapper. A Vec Python
                        // reaches through a builtin (`bytes`, `list`) is the
                        // raw C struct with no `inner`, so a wrapper accessor
                        // would not compile.
                        && !has_no_pyclass(t, TypeCategory::Vec)))
                && is_clone(t)
            {
                builder.line(&format!("#[getter({})]", n));
                builder.line(&format!(
                    "fn __get_{}(&self) -> {}{} {{ {}{} {{ inner: self.inner.{}.clone() }} }}",
                    n, prefix, t, prefix, t, n
                ));
                builder.blank();
                builder.line(&format!("#[setter({})]", n));
                builder.line(&format!(
                    "fn __set_{}(&mut self, value: {}{}) {{ self.inner.{} = value.inner; }}",
                    n, prefix, t, n
                ));
                builder.blank();
            }
        }
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

        // Only the functions api.json DECLARES: the trait functions the IR
        // synthesises (`clone`, `createDefault`) carry no fn_body, so
        // `generate_pymethod` could only leave a comment where a method
        // belongs. They are emitted from the derive flags instead, next to the
        // dunders that share their meaning.
        let class_functions: Vec<_> = ir
            .functions
            .iter()
            .filter(|f| f.class_name == struct_def.name)
            .filter(|f| f.kind.is_api_function())
            .collect();

        // Check if this struct is a callback wrapper type
        let is_callback_type = is_callback_wrapper_type(&struct_def.name, ir);

        let mut taken: BTreeSet<String> = BTreeSet::new();
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
            // Only an emitted method (one with an fn_body) reserves its name.
            if func.fn_body.is_some() {
                taken.insert(func.method_name.clone());
            }
            self.generate_pymethod(builder, func, ir, prefix);
        }

        self.generate_field_accessors(builder, struct_def, ir, prefix, &taken);
        self.generate_class_constants(builder, &struct_def.name, ir, &mut taken);
        let default_inner = self.default_inner_expr(&struct_def.name, ir, prefix);
        self.generate_derive_dunders(builder, &struct_def.traits, &taken, &default_inner);

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

    /// `is_<variant>()` and, where Python can hold the payload, `as_<variant>()` (None if another variant).
    fn generate_variant_accessors(
        &self,
        builder: &mut CodeBuilder,
        enum_def: &EnumDef,
        ir: &CodegenIR,
        prefix: &str,
        config: &PythonConfig,
        c_api_type: &str,
        taken: &mut BTreeSet<String>,
    ) {
        let is_clone = |type_name: &str| class_is_clone(type_name, ir);
        for variant in &enum_def.variants {
            let snake = to_snake_case(&variant.name);
            let is_name = format!("is_{}", snake);
            let as_name = format!("as_{}", snake);
            match &variant.kind {
                EnumVariantKind::Unit => {
                    if !taken.insert(is_name.clone()) {
                        continue;
                    }
                    builder.line(&format!("fn {}(&self) -> bool {{", is_name));
                    builder.line(&format!("    matches!(&self.inner, {}::{})", c_api_type, variant.name));
                    builder.line("}");
                    builder.blank();
                }
                EnumVariantKind::Tuple(types) => {
                    if taken.insert(is_name.clone()) {
                        builder.line(&format!("fn {}(&self) -> bool {{", is_name));
                        builder.line(&format!("    matches!(&self.inner, {}::{}(..))", c_api_type, variant.name));
                        builder.line("}");
                        builder.blank();
                    }
                    if taken.contains(&as_name) {
                        continue;
                    }
                    let Some((ty, _)) = types.first() else { continue };
                    let (ret, conv) = if is_primitive_type(ty) {
                        (ty.clone(), "*v".to_string())
                    } else if is_string_class(ty, ir) {
                        (
                            // allow-api-name: the Rust type written into the signature
                            "String".to_string(),
                            "{ let s: &azul_css::corety::AzString = unsafe { mem::transmute(v) }; s.as_str().to_string() }"
                                .to_string(),
                        )
                    } else if self.is_python_compatible_type(ty, ir)
                        && !self.type_is_excluded(ty, ir, config)
                        && !is_direct_ffi_type(ty, ir)
                        && !is_callback_wrapper_type(ty, ir)
                        && is_clone(ty)
                    {
                        (format!("{}{}", prefix, ty), format!("{}{} {{ inner: v.clone() }}", prefix, ty))
                    } else {
                        continue;
                    };
                    taken.insert(as_name.clone());
                    builder.line(&format!("fn {}(&self) -> Option<{}> {{", as_name, ret));
                    builder.line("    match &self.inner {");
                    builder.line(&format!("        {}::{}(v) => Some({}),", c_api_type, variant.name, conv));
                    builder.line("        #[allow(unreachable_patterns)]");
                    builder.line("        _ => None,");
                    builder.line("    }");
                    builder.line("}");
                    builder.blank();
                }
                EnumVariantKind::Struct(_) => {}
            }
        }
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
                        } else if is_string_class(ty, ir) {
                            // String needs to be converted to AzString and transmuted
                            builder.line(&format!(
                                "    unsafe {{ Self {{ inner: \
                                 {}::{}(core::mem::transmute(azul_css::corety::AzString::from(v))) \
                                 }} }}",
                                c_api_type, variant.name
                            ));
                        } else if is_callback_wrapper_type(ty, ir) {
                            // Callback types use Py<PyAny> which has no .inner field
                            // For now, skip these - callbacks in Option<Callback> require more
                            // complex handling
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

        // Variant constructors occupy the Python name space of this class
        // too. Raw names, not snake_case: a variant is emitted as
        // `fn Default()`, which does NOT collide with `fn default()` in
        // either Rust or Python -- lowercasing here would suppress the
        // `Default` derive on the six enums that have such a variant.
        let mut taken: BTreeSet<String> =
            enum_def.variants.iter().map(|v| v.name.clone()).collect();

        // Variants, `default` and `clone` are already covered above and by the derive dunders.
        for func in ir.functions.iter().filter(|f| f.class_name == enum_def.name).filter(|f| {
            matches!(
                f.kind,
                FunctionKind::Constructor
                    | FunctionKind::StaticMethod
                    | FunctionKind::Method
                    | FunctionKind::MethodMut
            )
        }) {
            if self.function_has_unsupported_args(func, ir)
                || self.function_refs_excluded_type(func, ir, config)
            {
                continue;
            }
            if func.fn_body.is_some() {
                taken.insert(func.method_name.clone());
            }
            self.generate_pymethod(builder, func, ir, prefix);
        }

        if enum_def.is_union {
            self.generate_variant_accessors(builder, enum_def, ir, prefix, config, &c_api_type, &mut taken);
        }

        self.generate_class_constants(builder, &enum_def.name, ir, &mut taken);
        let default_inner = self.default_inner_expr(&enum_def.name, ir, prefix);
        self.generate_derive_dunders(builder, &enum_def.traits, &taken, &default_inner);

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
                // A borrowed slice is taken as the sequence itself: pyo3
                // extracts `bytes`/`list[int]`/`list[float]`/`list[str]`/
                // `list[Node]` into a `Vec<T>` that this function then LENDS
                // to the callee (see below), and an optional one as
                // `Optional[...]`. A value the callee MUTATES through a
                // pointer is taken as a `PyRefMut` guard -- the caller's own
                // object, borrowed for the call -- so the write lands where
                // the caller can see it. A shared borrow is taken as the
                // object itself and needs no adjustment.
                let py_type = if let Some(slice) = borrowed_slice_arg(&a.type_name, ir) {
                    format!("Vec<{}>", slice.py_element(prefix))
                } else if let Some(opt) = optional_slice(&a.type_name, ir) {
                    format!("Option<Vec<{}>>", opt.slice.py_element(prefix))
                } else if pointer_borrow(func, a) == Some(PointerBorrow::Mutable) {
                    format!("PyRefMut<'_, {}{}>", prefix, a.type_name)
                } else {
                    self.rust_type_to_python(&a.type_name, prefix, ir)
                };
                format!("{}: {}", a.name, py_type)
            })
            .collect::<Vec<_>>()
            .join(", ");

        // An optional borrowed slice comes back as the bytes themselves (pyo3
        // maps `Vec<u8>` to `bytes`, any other element to a list), copied out
        // of the borrow before it dies.
        let return_type = match func.return_type.as_ref() {
            Some(t) => match optional_slice(t, ir) {
                Some(opt) => format!("Option<Vec<{}>>", opt.slice.py_element(prefix)),
                None => self.rust_type_to_python(t, prefix, ir),
            },
            None => "()".to_string(),
        };

        // Functions with RefAny or Callback args need access to Python GIL for clone_ref
        let has_refany_arg = args.iter().any(|a| is_refany(&a.type_name, ir));
        let has_callback_arg = args.iter().any(|a| a.callback_info.is_some());
        let needs_py_param = has_refany_arg || has_callback_arg;

        // A by-value-consuming method on a NON-`Clone` type must MOVE the receiver
        // out (`ptr::read`) and then NEUTRALIZE the original so the pyclass isn't
        // Dropped a SECOND time at dealloc (RefAny double-free). That write is only
        // legal through `&mut self`, so take a mutable receiver for exactly those
        // methods. `Clone` types deep-clone via `&self` and never need this.
        let self_is_by_value = func
            .args
            .iter()
            .find(|a| a.name == self_arg_name)
            .map(|a| a.ref_kind == ArgRefKind::Owned)
            .unwrap_or(false);
        let class_is_clone_recv = ir
            .find_struct(&func.class_name)
            .map(|s| self.struct_supports_clone(s))
            .or_else(|| {
                ir.find_enum(&func.class_name)
                    .map(|e| self.enum_supports_clone(e))
            })
            .unwrap_or(true);
        let needs_consume_self = takes_self && self_is_by_value && !class_is_clone_recv;
        // A `&mut self` method MUTATES the object the caller is holding, so it
        // must reach that object and not a copy of it. Both cases below need a
        // mutable receiver for that; pyo3 turns it into a `PyRefMut` borrow of
        // the pyclass for the duration of the call.
        let mutates_self = func.kind == FunctionKind::MethodMut;
        let self_recv = if needs_consume_self || mutates_self {
            "&mut self"
        } else {
            "&self"
        };

        // Generate function signature
        if takes_self {
            if args.is_empty() {
                if needs_py_param {
                    builder.line(&format!(
                        "fn {}({}, py: Python<'_>) -> {} {{",
                        func.method_name, self_recv, return_type
                    ));
                } else {
                    builder.line(&format!(
                        "fn {}({}) -> {} {{",
                        func.method_name, self_recv, return_type
                    ));
                }
            } else if needs_py_param {
                builder.line(&format!(
                    "fn {}({}, py: Python<'_>, {}) -> {} {{",
                    func.method_name, self_recv, args_str, return_type
                ));
            } else {
                builder.line(&format!(
                    "fn {}({}, {}) -> {} {{",
                    func.method_name, self_recv, args_str, return_type
                ));
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
            } else if needs_py_param {
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

        builder.indent();
        builder.line("#[allow(unused_mut)]");
        builder.line("unsafe {");
        builder.indent();

        // Transform the fn_body for Python bindings:
        // 1. Replace "azul_dll::" with "crate::" (we're in azul-dll crate)
        // 2. Replace "Self " and "Self::" with the external path (since Self in Python wrapper is
        //    AzXxx)
        // 3. Replace self references with transmuted variable
        // 4. Replace parameter names with transmuted versions
        // 5. Replace type constructors (TypeName::method) with fully qualified paths
        let mut transformed_body = fn_body.replace("azul_dll::", "crate::");

        // Replace Self with external path (Self in fn_body refers to the Rust type, not the Python
        // wrapper) Handle both "Self::" (associated functions) and "Self {" or "Self "
        // (struct construction)
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
                ctor_replacements
                    .push((format!("{}::", struct_def.name), format!("{}::", ext_path)));
            }
        }
        for enum_def in &ir.enums {
            if let Some(ref ext_path) = enum_def.external_path {
                ctor_replacements.push((format!("{}::", enum_def.name), format!("{}::", ext_path)));
            }
        }
        // Longest pattern first so `DomIdVec::` wins over `Dom::`.
        ctor_replacements.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        transformed_body = replace_type_paths(&transformed_body, &ctor_replacements);

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
        self_vars.sort_by_key(|b| std::cmp::Reverse(b.len()));
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
            if is_method_mut {
                // A `&mut self` method exists to CHANGE the receiver, so
                // `__cloned` is bound to the real value, not to a copy of it:
                // every `object.set_x(v)` in a fn_body becomes
                // `__cloned.set_x(v)` and auto-derefs through this `&mut`, and
                // every body that passes the receiver on
                // (`register_dom_icon(iconproviderhandle, ..)`) receives the
                // same `&mut`. Cloning here instead -- which is what this used
                // to do for the handful of such methods that were emitted --
                // mutated a temporary that was then dropped: every setter was
                // a silent no-op, except on the few classes whose value is
                // itself a handle to something else.
                builder.line(&format!(
                    "let __cloned: &mut {} = core::mem::transmute(&mut self.inner);",
                    external_path
                ));
            } else if needs_consume_self {
                // By-value consume on a NON-Clone type: MOVE the owned inner out
                // into `__cloned`, then ZERO the original so the pyclass's later
                // Drop at dealloc is a harmless no-op (was a RefAny double-free /
                // refcount underflow — the ~10 camera/screencap/mic/video widget
                // sites). Drop-safe: all owned fields route through RefAny, whose
                // RefCount::drop early-returns a no-op on a null/zeroed pointer.
                // Legal because the receiver is `&mut self` (see self_recv above).
                builder.line(&format!(
                    "let _self: &mut {} = core::mem::transmute(&mut self.inner);",
                    external_path
                ));
                builder.line(&format!(
                    "let mut __cloned: {} = core::ptr::read(_self as *const {});",
                    external_path, external_path
                ));
                builder.line(&format!(
                    "core::ptr::write(_self as *mut {}, core::mem::zeroed());",
                    external_path
                ));
            } else {
                builder.line(&format!(
                    "let _self: &{} = core::mem::transmute(&self.inner);",
                    external_path
                ));
                // Clone self so methods can consume it (mut for methods that
                // mutate). For non-Clone types, bitwise-move the owned inner out.
                if class_is_clone {
                    builder.line("let mut __cloned = _self.clone();");
                } else {
                    builder.line(&format!(
                        "let mut __cloned: {} = core::ptr::read(_self as *const {});",
                        external_path, external_path
                    ));
                }
            }

            // Replace self references in fn_body
            // First replace method calls (with dot)
            transformed_body = transformed_body
                .replace("self.", "__cloned.")
                .replace("object.", "__cloned.");
            for sv in &self_vars {
                transformed_body = transformed_body.replace(&format!("{}.", sv), "__cloned.");
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
                // Already a `&mut` binding (see above), so it is passed on as
                // it stands; `&mut __cloned` would be a `&mut &mut T`.
                "__cloned"
            } else if self_is_by_value {
                "__cloned"
            } else {
                "&__cloned"
            };
            // An explicit `&mut self` / `&self` in the body reborrows rather
            // than re-references when `__cloned` is ALREADY a `&mut` (the
            // MethodMut binding above): `&mut __cloned` would be `&mut &mut T`.
            let (explicit_mut, explicit_ref) = if is_method_mut {
                ("__cloned", "&*__cloned")
            } else {
                ("&mut __cloned", "&__cloned")
            };
            for sv in &self_vars {
                transformed_body = transformed_body
                    .replace(&format!("({},", sv), &format!("({},", self_ref))
                    .replace(&format!("({}, ", sv), &format!("({}, ", self_ref))
                    .replace(&format!("({})", sv), &format!("({})", self_ref))
                    .replace(&format!(", {},", sv), ", __cloned,")
                    .replace(&format!(", {}, ", sv), ", __cloned, ")
                    .replace(&format!(", {})", sv), ", __cloned)")
                    .replace(&format!("&mut {},", sv), &format!("{},", explicit_mut))
                    .replace(&format!("&mut {})", sv), &format!("{})", explicit_mut))
                    .replace(&format!("&{},", sv), &format!("{},", explicit_ref))
                    .replace(&format!("&{})", sv), &format!("{})", explicit_ref));
            }
            // Any other use of the receiver name (`&earlier < instant`) gets a
            // binding. A `&mut` receiver is REBORROWED into it rather than
            // moved, so the body may still use `__cloned` itself.
            let fallback_binding = if is_method_mut { "&mut *__cloned" } else { self_ref };
            for sv in &self_vars {
                let word = regex::Regex::new(&format!(r"(^|[^\w.:]){}($|[^\w:])", regex::escape(sv)))
                    .unwrap();
                if word.is_match(&transformed_body) {
                    builder.line(&format!("let {} = {};", sv, fallback_binding));
                    break;
                }
            }
        }

        // Convert arguments to external types
        for arg in &args {
            // A borrowed slice: the callee gets a `{ptr, len}` pair pointing
            // into a buffer that lives in THIS function. The buffer is bound
            // before the call and dropped at the end of the surrounding block,
            // i.e. strictly after the call returns, which is the whole of the
            // borrow the C signature asks for. Nothing may be retained: a
            // callee that stored the pointer would be reading freed memory the
            // moment this method returns, so only the read-only slices are
            // bridged (`function_has_unsupported_args` drops the rest).
            if let Some(slice) = borrowed_slice_arg(&arg.type_name, ir) {
                let external = self
                    .find_external_path(&arg.type_name, ir)
                    .unwrap_or_else(|| format!("crate::{}", arg.type_name));
                let ffi = format!("__dll_api_inner::dll::{}{}", prefix, arg.type_name);
                builder.line(&format!(
                    "let __slice_{n}: Vec<{el}> = {n};",
                    n = arg.name,
                    el = slice.py_element(prefix)
                ));
                // A slice of borrowed STRINGS needs a second buffer: the
                // views themselves, each pointing into a string of the first.
                // Both are locals of this call, so both outlive it and die
                // with it.
                let buffer = if slice.kind == SliceElement::BorrowedStr {
                    let element_path = slice_element_path(&arg.type_name, &external);
                    builder.line(&format!(
                        "let __views_{n}: Vec<{el}> = __slice_{n}.iter().map(|s| \
                         {el}::from(s.as_str())).collect();",
                        n = arg.name,
                        el = element_path
                    ));
                    format!("__views_{}", arg.name)
                } else {
                    format!("__slice_{}", arg.name)
                };
                builder.line(&format!(
                    "let {n}: {ext} = core::mem::transmute({ffi} {{ {ptr}: {buf}.as_ptr() as \
                     *const c_void, {len}: {buf}.len() }});",
                    n = arg.name,
                    ext = external,
                    ffi = ffi,
                    ptr = slice.ptr_field,
                    len = slice.len_field,
                    buf = buffer,
                ));
                continue;
            }

            // An optional borrowed slice: the same lend, or the option's own
            // empty variant when Python passed nothing.
            if let Some(opt) = optional_slice(&arg.type_name, ir) {
                let external = self
                    .find_external_path(&arg.type_name, ir)
                    .unwrap_or_else(|| format!("crate::{}", arg.type_name));
                let ffi_opt = format!("__dll_api_inner::dll::{}{}", prefix, arg.type_name);
                let ffi_slice = format!("__dll_api_inner::dll::{}{}", prefix, opt.slice_type);
                builder.line(&format!(
                    "let __slice_{n}: Option<Vec<{el}>> = {n};",
                    n = arg.name,
                    el = opt.slice.py_element(prefix)
                ));
                builder.line(&format!("let __opt_{n} = match &__slice_{n} {{", n = arg.name));
                builder.line(&format!(
                    "    Some(v) => {ffi_opt}::{some}({ffi_slice} {{ {ptr}: v.as_ptr() as *const \
                     c_void, {len}: v.len() }}),",
                    ffi_opt = ffi_opt,
                    some = opt.some_variant,
                    ffi_slice = ffi_slice,
                    ptr = opt.slice.ptr_field,
                    len = opt.slice.len_field,
                ));
                builder.line(&format!(
                    "    None => {ffi_opt}::{none},",
                    ffi_opt = ffi_opt,
                    none = opt.none_variant
                ));
                builder.line("};");
                builder.line(&format!(
                    "let {n}: {ext} = core::mem::transmute(__opt_{n});",
                    n = arg.name,
                    ext = external
                ));
                continue;
            }

            // A borrowed object: same contract as the slice above, one value
            // instead of many.
            match pointer_borrow(func, arg) {
                // Shared: the value is bound to a local -- alive for the whole
                // call, dropped when this method returns -- and the callee
                // gets its address, which the fn_body dereferences
                // (`unsafe { &*info }`) exactly as the C entry point does.
                Some(PointerBorrow::Shared) => {
                    let external = self
                        .find_external_path(&arg.type_name, ir)
                        .unwrap_or_else(|| format!("crate::{}", arg.type_name));
                    builder.line(&format!(
                        "let __borrowed_{n}: {ext} = core::mem::transmute({n}.inner.clone());",
                        n = arg.name,
                        ext = external
                    ));
                    builder.line(&format!(
                        "let {n}: *const {ext} = &__borrowed_{n};",
                        n = arg.name,
                        ext = external
                    ));
                    continue;
                }
                // Mutable: the callee WRITES through this pointer, so it must
                // point at the caller's own object and not at a copy -- a copy
                // would make the method a silent no-op. The `PyRefMut` guard
                // is that object, borrowed mutably from Python for exactly the
                // length of this call (pyo3 raises instead of aliasing it),
                // and the pointer is taken through the guard.
                Some(PointerBorrow::Mutable) => {
                    let external = self
                        .find_external_path(&arg.type_name, ir)
                        .unwrap_or_else(|| format!("crate::{}", arg.type_name));
                    builder.line(&format!("let mut __guard_{n} = {n};", n = arg.name));
                    builder.line(&format!(
                        "let {n}: *mut {ext} = &mut __guard_{n}.inner as *mut \
                         __dll_api_inner::dll::{prefix}{ty} as *mut {ext};",
                        n = arg.name,
                        ext = external,
                        prefix = prefix,
                        ty = arg.type_name,
                    ));
                    continue;
                }
                None => {}
            }

            // RefAny is ALWAYS converted from Py<PyAny> to RefAny with JSON support
            if is_refany(&arg.type_name, ir) {
                // Wrap Python data in RefAny via PyDataWrapper with JSON serialization
                // Use the SAME name as the parameter so fn_body can use it unchanged
                builder.line(&format!(
                    "let __py_{}_wrapper = PyDataWrapper {{ _py_data: Some({}.clone_ref(py)) }};",
                    arg.name, arg.name
                ));
                builder.line(&format!(
                    "let {}: azul_core::refany::RefAny = \
                     create_py_refany_with_json(__py_{}_wrapper);",
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
                    "let __py_{}_wrapper = PyCallableWrapper {{ _py_callable: \
                     Some({}.clone_ref(py)) }};",
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

                // The wrapper's real field names, as the IR recorded them when
                // it linked a wrapper to its typedef BY SHAPE
                // (`link_callback_wrappers`): the function-pointer field is
                // usually `cb` but not always (one spells it `resolver`), and
                // the context slot is `ctx` on some wrappers and `callable` on
                // others. Reading them from the IR also gets the answer right
                // for a wrapper that carries a third field, where "the first
                // field that is not the context" picked whichever came first.
                let (fn_ptr_field, callable_field) =
                    match get_callback_wrapper_info(&cb_info.callback_wrapper_name, ir) {
                        Some(info) => (
                            info.callback_field_name.clone(),
                            Some(info.context_field_name.clone()),
                        ),
                        // Unreachable: `detect_callback_arg_info` only ever
                        // names a struct the IR linked. Left as an empty field
                        // name so that a future change which breaks that
                        // invariant fails loudly in the generated file rather
                        // than filling the wrong slot.
                        None => (String::new(), None),
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
                builder.line(&format!(
                    "let __{}_ffi: {} = {} {{",
                    arg.name, wrapper_ffi, wrapper_ffi
                ));
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
                    } else if is_string_class(&arg.type_name, ir) {
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
            } else if is_string_class(&arg.type_name, ir) {
                // String args: convert to AzString, shadow the parameter
                builder.line(&format!(
                    "let {}: {} = azul_css::corety::AzString::from({}.clone());",
                    arg.name, arg_external, arg.name
                ));
            } else if is_direct_ffi_type(&arg.type_name, ir) {
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
                builder.line(&transformed_body.to_string());
            } else {
                builder.line(&format!("let _: () = {};", transformed_body));
            }
        } else if let Some(ret_type) = &func.return_type {
            if let Some(opt) = optional_slice(ret_type, ir) {
                // An optional borrowed slice: the pointer belongs to the
                // callee and stays there. COPY the bytes out while the borrow
                // is still alive -- the value returned to Python owns itself,
                // so nothing of the borrow escapes this block, and pyo3 turns
                // it into `bytes` (or a list, for a wider element).
                let ret_external = self
                    .find_external_path(ret_type, ir)
                    .unwrap_or_else(|| format!("crate::{}", ret_type));
                let ffi_opt = format!("__dll_api_inner::dll::{}{}", prefix, ret_type);
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
                builder.line(&format!(
                    "let __result: {ffi} = core::mem::transmute(__result);",
                    ffi = ffi_opt
                ));
                builder.line("match __result {");
                builder.line(&format!(
                    "    {ffi}::{some}(__borrowed) => Some(",
                    ffi = ffi_opt,
                    some = opt.some_variant
                ));
                // A null or empty `{ptr, len}` is what an absent buffer looks
                // like inside a present variant, and `from_raw_parts` is UB on
                // a null pointer even for length zero.
                builder.line(&format!(
                    "        if __borrowed.{len} == 0 || __borrowed.{ptr}.is_null() {{ Vec::new() \
                     }}",
                    len = opt.slice.len_field,
                    ptr = opt.slice.ptr_field
                ));
                builder.line(&format!(
                    "        else {{ core::slice::from_raw_parts(__borrowed.{ptr} as *const \
                     {el}, __borrowed.{len}).to_vec() }}",
                    ptr = opt.slice.ptr_field,
                    el = opt.slice.py_element(prefix),
                    len = opt.slice.len_field
                ));
                builder.line("    ),");
                builder.line(&format!(
                    "    {ffi}::{none} => None,",
                    ffi = ffi_opt,
                    none = opt.none_variant
                ));
                builder.line("}");
            } else if is_primitive_type(ret_type) {
                // Primitive return types
                if has_statements {
                    builder.line(&format!("{{ {} }}", transformed_body));
                } else {
                    builder.line(&transformed_body);
                }
            } else if is_string_class(ret_type, ir) {
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
            } else if is_py_builtin_class(ret_type, ir) {
                // A builtin-bridged class has no `.inner` wrapper to fill: the
                // C type carries the IntoPyObject impl, so the result only has
                // to be transmuted from the external type into it.
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

        // A monomorphized alias is a class like any other, so it is importable
        // from `azul` like any other; leaving it unregistered would hide the
        // whole CSS property-value surface behind types Python could receive
        // but never name.
        let aliases = alias_classes(ir);
        for class in aliases
            .structs
            .iter()
            .filter(|s| self.should_include_struct(s, config))
            .map(|s| &s.name)
            .chain(
                aliases
                    .enums
                    .iter()
                    .filter(|e| self.should_include_enum(e, config))
                    .map(|e| &e.name),
            )
        {
            builder.line(&format!("m.add_class::<{}{}>()?;", prefix, class));
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

        // A class Python reaches through a builtin (`str`, `bytes`, `list`)
        // gets no wrapper struct: the PyO3 conversions sit on the C type
        // itself. Note this is NOT the IR's `Vec` category -- every Vec is
        // `TypeCategory::Vec` and all but the hand-bridged few do get a
        // pyclass -- but `String` and `RefAny` ARE recognised by category.
        if has_no_pyclass(&struct_def.name, struct_def.category) {
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

        // The same Send claim as in `generate_send_sync_impls`, applied to
        // the pyclass: a class that is Send needs no `unsendable`. The two
        // lists must agree -- a class marked sendable here whose mirror has
        // no `unsafe impl Send` there does not compile.
        const PYTHON_SEND_SAFE_TYPES: &[&str] = &[ // allow-api-name: a per-class safety claim, see generate_send_sync_impls
            "CssPropertyCachePtr",          // wraps Box<CssPropertyCache>
            "VirtualViewCallbackInfo",      // wraps &VirtualViewCallbackInfoInternal
            "VirtualViewReturn",            /* contains OptionDom which may have callbacks with
                                             * raw pointers */
            "StyledDom",                    // contains CssPropertyCachePtr
            "LayoutCallbackInfo",           // wraps & to internal data
            "CallbackInfo",                 // wraps & to internal data
            "RenderImageCallbackInfo",      // wraps & to internal data
            "RefCount",                     // refcounted pointer, semantically Send
            "OptionRefAny",                 // Option<RefAny>
            "GlVoidPtrMut",                 // GL pointer wrapper
            "ParsedSvg",                    // SVG data structure
            "ResultParsedSvgSvgParseError", // Result type containing ParsedSvg
            "GridMinMax",                   // CSS grid layout type
            "GridTrackSizing",              // CSS grid layout type
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

        // The reverse claim: these enums DO carry a raw pointer through to
        // Python, so the pyclass must be thread-bound even though the
        // structural walk below would clear them (their payloads are the
        // handle structs the list above vouches for).
        const PYTHON_FORCE_UNSENDABLE_ENUMS: &[&str] = &[ // allow-api-name: a per-class safety claim, see above
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

        // The same Send claim once more, for a class seen as the FIELD of
        // another: reaching a pointer here would make every holder
        // thread-bound too. Wider than the list above because a class can be
        // safe to hold without being safe to hand out on its own.
        const PYTHON_SEND_SAFE_TYPES: &[&str] = &[ // allow-api-name: a per-class safety claim, see generate_send_sync_impls
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

        // A function pointer is not sendable, and the IR knows both shapes it
        // takes: the bare typedef, and the wrapper struct that pairs one with
        // its context. A `*Callback`-named struct the IR did NOT link is
        // caught by the field walk below, whose function-pointer field IS a
        // typedef.
        if is_callback_typedef(type_name, ir) || is_callback_wrapper_type(type_name, ir) {
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
        // NOTE: a `&mut self` method used to be dropped whenever its class was
        // `unsendable`, which took out 247 methods -- every widget setter, on
        // classes that are unsendable only because they hold a callback. The
        // two have nothing to do with each other: pyo3 takes a `PyRefMut` for
        // a `&mut self` receiver on an unsendable pyclass exactly as it does
        // on a sendable one. `generate_pymethod` binds the receiver IN PLACE
        // for these (see the MethodMut arm), so the mutation reaches the
        // object the caller holds.
        for arg in &func.args {
            // Callback types with callback_info are ALWAYS allowed - become Py<PyAny>
            // This check MUST come before is_python_compatible_type to allow CallbackType args
            if arg.callback_info.is_some() {
                continue;
            }

            // The application's own data crosses as the Python object itself.
            if is_refany(&arg.type_name, ir) {
                continue;
            }

            // A pointer the callee dereferences as ONE value is a borrow for
            // the duration of the call, which Python can honour (see
            // `generate_pymethod`): a shared borrow lends a value bound here,
            // a mutable one lends the caller's own object through a
            // `PyRefMut` guard, so the write lands where the caller can see
            // it. Both need the pointee to be a class Python holds.
            if let Some(borrow) = pointer_borrow(func, arg) {
                if !self.is_python_compatible_type(&arg.type_name, ir) {
                    return true;
                }
                // The shared form copies the value to lend it; the mutable
                // form must NOT copy, so it needs no `Clone`.
                if borrow == PointerBorrow::Shared && !class_is_clone(&arg.type_name, ir) {
                    return true;
                }
                continue;
            }
            // An optional borrowed slice: the same lend, with the absent case
            // carried by the option's own empty variant.
            if let Some(opt) = optional_slice(&arg.type_name, ir) {
                if opt.slice.is_mutable || !is_primitive_type(&opt.slice.element) {
                    return true;
                }
                continue;
            }
            // Every other raw pointer stays out, and there the skip is the
            // honest answer rather than a gap: an array pointer is a promise
            // about memory the caller owns and keeps alive for `len`
            // elements, and a `*mut` is a promise to write into the caller's
            // own object -- neither of which a Python value can be. The
            // pointer-ness is carried in `ref_kind` (parse_type_ref_kind
            // strips `*const`/`*mut` from `type_name`), so a string check
            // alone would miss `copy_from_ptr`'s `ptr: *const ListViewRow`.
            // Such a method always has a by-value sibling (`from_item`,
            // `create`) that Python does get.
            if matches!(arg.ref_kind, ArgRefKind::Ptr | ArgRefKind::PtrMut) {
                return true;
            }
            if arg.type_name.contains("*const") || arg.type_name.contains("*mut") {
                return true;
            }
            // A borrowed slice argument is bridged when Python can build a
            // buffer with the C element layout: a primitive, or a class whose
            // `repr(transparent)` wrapper makes `Vec<AzT>` the same bytes as
            // the C array. Anything else (a slice of borrowed strings, a slice
            // the callee WRITES into) has no such bridge.
            if let Some(slice) = borrowed_slice_arg(&arg.type_name, ir) {
                let element_ok = match slice.kind {
                    SliceElement::Primitive | SliceElement::BorrowedStr => true,
                    SliceElement::Class => {
                        self.is_python_compatible_type(&slice.element, ir)
                            && class_is_clone(&slice.element, ir)
                    }
                };
                if slice.is_mutable || !element_ok {
                    return true;
                }
                continue;
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

            // A bare function-pointer typedef the IR could not pair with a
            // wrapper (no `callback_info`, handled above): there is nowhere to
            // put the Python callable, so the method cannot be bridged.
            if is_callback_typedef(&arg.type_name, ir) {
                return true;
            }
        }

        if let Some(ret) = &func.return_type {
            // An optional borrowed slice is the one borrow that may be
            // RETURNED: nothing of it escapes, because `generate_pymethod`
            // copies the bytes out while the borrow is still alive and hands
            // Python the copy. Every other borrow-shaped return would hand
            // out a pointer whose owner Python cannot see.
            if let Some(opt) = optional_slice(ret, ir) {
                return opt.slice.is_mutable || !is_primitive_type(&opt.slice.element);
            }
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
        // A monomorphized alias is a class too, and it answers the same way:
        // whichever synthesized def carries it decides (see `alias_classes`).
        if is_alias_class(type_name, ir) {
            let aliases = alias_classes(ir);
            if let Some(s) = aliases.structs.iter().find(|s| s.name == type_name) {
                return !self.should_include_struct(s, config);
            }
            if let Some(e) = aliases.enums.iter().find(|e| e.name == type_name) {
                return !self.should_include_enum(e, config);
            }
        }
        false
    }

    /// Returns true if a Python callable passed for this callback arg can be
    /// bridged to Rust. This requires BOTH:
    /// 1. A trampoline `extern "C"` fn is generated for the callback typedef (mirrors the gating in
    ///    `generate_callback_trampolines`), and
    /// 2. The wrapper struct exists and has an `OptionRefAny` field to store the Python callable.
    ///    When either is false there is no way to store/invoke the Python callable,
    ///    so the consuming method must be skipped.
    fn callback_arg_is_bridgeable(&self, cb_info: &CallbackArgInfo, ir: &CodegenIR) -> bool {
        // `callback_info` is only set for a callback wrapper (by structure) or
        // the typedef one holds, so the wrapper and its context slot exist.
        ir.callback_typedefs
            .iter()
            .find(|c| c.name == cb_info.callback_typedef_name)
            .is_some_and(|c| trampoline_bridges(c, ir))
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
            // A borrowed slice is bridged too: the Python sequence becomes a
            // buffer this call lends to the callee, so the slice class needs
            // no pyclass of its own. Its ELEMENT does, when the element is a
            // class -- that is the type the signature names. (The slices that
            // cannot be bridged are already gone --
            // `function_has_unsupported_args` drops them.)
            if let Some(slice) = borrowed_slice_arg(&arg.type_name, ir) {
                if slice.kind == SliceElement::Class
                    && self.type_is_excluded(&slice.element, ir, config)
                {
                    return true;
                }
                continue;
            }
            // An optional borrowed slice is bridged the same way.
            if optional_slice(&arg.type_name, ir).is_some() {
                continue;
            }
            // A builtin-bridged class is bridged, not wrapped: a Python `str`,
            // `bytes` or `list` flows in through the C type's own
            // FromPyObject and `generate_pymethod` converts it in the body.
            // Those classes have no pyclass ON PURPOSE, so `type_is_excluded`
            // would otherwise drop every method that takes one -- which is
            // every String-taking constructor (Button::create,
            // Css::from_string, ...) and every bytes-taking one.
            if is_py_builtin_class(&arg.type_name, ir) {
                continue;
            }
            if self.type_is_excluded(&arg.type_name, ir, config) {
                return true;
            }
        }
        if let Some(ret) = &func.return_type {
            // Same on the way out: the value is converted back to the builtin
            // in the body, so the missing pyclass must not drop the method.
            if !is_py_builtin_class(ret, ir) && self.type_is_excluded(ret, ir, config) {
                return true;
            }
        }
        false
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

        // The application object and the untyped pointer are not values.
        if is_refany(type_name, ir) || type_name == "c_void" {
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

        // A class Python reaches through a builtin IS representable: `bytes`,
        // `list[str]` and `list[int]` flow through the C type's own
        // FromPyObject/IntoPyObject impls, and the body converts. (`RefAny`
        // took the early return above: it is a Python object, not a value.)
        if is_py_builtin_class(type_name, ir) {
            return true;
        }

        // A class with no Python shape at all.
        if PY_UNMODELLED_CLASSES.contains(&type_name) {
            return false;
        }

        // Categories the emitter deliberately does not wrap. Every one of them
        // is the IR's own classification, not a name: a destructor or clone
        // callback is a bare function pointer, a generic template cannot be
        // instantiated from Python, and a recursive type has no sized mirror.
        if let Some(category) = category_of(type_name, ir) {
            if category.skip_in_python() {
                return false;
            }
            return true;
        }

        // Type aliases. A MONOMORPHIZED one has a real class (see
        // `alias_classes`) and can cross the boundary as long as pyo3 can
        // extract it, which for a pyclass means `Clone`. Every other alias --
        // to a raw pointer (`X11Visual`), to a generic template, to anything
        // the mirror spells as a bare typedef -- has neither a `.inner`
        // wrapper nor a FromPyObject impl, so a signature naming it would not
        // compile.
        if let Some(alias) = ir.find_type_alias(type_name) {
            if alias.target.contains('*') {
                return false;
            }
            // An alias to a primitive is a number with a name: Python passes
            // the number, and `generate_pymethod` transmutes it into whatever
            // newtype the real signature asks for.
            if is_primitive_type(&alias.target) {
                return true;
            }
            return alias.monomorphized_def.is_some() && alias_mirror_traits(alias, ir).is_clone;
        }

        // Not a class at all (a bare `Refstr`, a generic parameter, a spelling
        // the IR never saw): nothing to generate against.
        false
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
        if is_string_class(rust_type, ir) {
            // allow-api-name: the Rust type written into the signature, not a decision
            return "String".to_string();
        }

        // Handle array types: [PixelValue; 2] -> [AzPixelValue; 2]
        let (ptr_prefix, base_type, array_suffix) = analyze_type(rust_type);

        // RefAny → Py<PyAny> (Python object that gets wrapped internally)
        if is_refany(&base_type, ir) {
            return "Py<PyAny>".to_string();
        }

        // A raw function-pointer typedef is not something Python can hold: the
        // signature takes a callable instead, and a trampoline invokes it.
        // Only the kinds a trampoline bridges -- a destructor or a clone
        // function is internal cleanup with no callable and no wrapper to
        // carry one, and never reaches Python.
        if ir
            .callback_typedefs
            .iter()
            .any(|c| c.name == base_type && trampoline_bridges(c, ir))
        {
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

/// The severity a failure at the callback boundary is reported at.
///
/// This names a VALUE the binding chooses -- the "something went wrong" level
/// of whatever log-level enum the API declares -- not an API type: the enum
/// itself, the method and the message type are all found by shape in
/// [`log_sink_method`].
const SEVERITY_ERROR: &str = "Error";

/// The log-sink method `class_name` declares, with the level enum it takes.
///
/// A log sink is recognised by SHAPE, so this follows api.json instead of a
/// list of callback kinds: an instance method that returns nothing and takes
/// exactly two arguments besides the receiver -- a severity (a fieldless
/// variant of an enum that has one saying something failed) and a message
/// (the string class). Exactly one method in the API has that shape, and a
/// second one could only be another log sink.
fn log_sink_method<'a>(
    class_name: &str,
    ir: &'a CodegenIR,
) -> Option<(&'a FunctionDef, &'a EnumDef)> {
    // Not `functions_for_class`: it ties the class name's lifetime to the IR's.
    ir.functions.iter().find_map(|f| {
        if f.class_name != class_name
            || !matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
            || f.return_type.is_some()
        {
            return None;
        }
        let args: Vec<_> = f.args.iter().filter(|a| !f.is_receiver_arg(a)).collect();
        let [level, message] = args.as_slice() else {
            return None;
        };
        if !is_string_class(&message.type_name, ir) {
            return None;
        }
        let level_enum = ir.find_enum(&level.type_name)?;
        let reports_failure = level_enum
            .variants
            .iter()
            .any(|v| v.name == SEVERITY_ERROR && matches!(v.kind, EnumVariantKind::Unit));
        reports_failure.then_some((f, level_enum))
    })
}

/// The name a trampoline gives its `i`-th argument. The data object and the
/// info object are named for what they are; the rest are positional.
fn trampoline_arg_name(i: usize) -> String {
    match i {
        0 => "data".to_string(),
        1 => "info".to_string(),
        i => format!("arg{}", i),
    }
}

/// Whether this callback kind hands the callback the application's own data
/// object as its first argument. A kind that does not (because it takes no
/// arguments at all) is still bridgeable -- see [`trampoline_bridges`].
fn callback_carries_data(callback: &CallbackTypedefDef, ir: &CodegenIR) -> bool {
    callback
        .args
        .first()
        .is_some_and(|a| is_refany(&a.type_name, ir))
}

/// Whether a Python trampoline bridges this callback kind.
///
/// It needs a wrapper, because that is where the Python callable lives (in
/// the wrapper's context slot), and no argument may be a raw pointer, because
/// the trampoline hands its arguments to Python.
///
/// The ARGUMENTS may take either of two shapes. Usually the first one is the
/// application's data object, which Python receives; the context then comes
/// from the info argument's accessor, or -- when no argument can hand it back
/// -- from libazul's invocation slot. A kind with NO arguments at all
/// (`fn() -> ComponentLibrary`, the component-library registration) has
/// neither, and the invocation slot is exactly what the engine provides for
/// it: `impl_managed_callback!`'s Form 5 keys the context on the callee's
/// own address, which a trampoline knows. So that shape bridges too, and
/// calls Python with an empty argument tuple.
fn trampoline_bridges(callback: &CallbackTypedefDef, ir: &CodegenIR) -> bool {
    callback.wrapper.is_some()
        && (callback_carries_data(callback, ir) || callback.args.is_empty())
        && !callback.args.iter().any(|a| a.type_name.contains('*'))
}

/// A type the generated file writes into a signature AS ITSELF: a Rust
/// primitive, a `core::ffi` numeric alias, or one of the GL numeric typedefs.
///
/// The GL names are not a classification decision, they are the SPELLING the
/// generated code uses: the mirror does not re-export them under an `Az`
/// prefix, so `fn bind_texture(&self, target: GLenum, texture: GLuint)` names
/// the typedefs the including crate has in scope. Resolving
/// "alias whose target is a primitive" from the IR instead would also catch
/// the handful of newtype-ish aliases whose bare name is NOT in scope there,
/// and emit signatures that do not compile.
fn is_primitive_type(name: &str) -> bool {
    matches!( // allow-api-name: the spelling written into generated signatures, see above
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

/// Check if a type is a callback wrapper struct (a function pointer paired
/// with the context its caller passes back).
///
/// These types need special handling: Python receives Py<PyAny>, and we
/// construct the callback with a trampoline function that invokes the Python
/// callable.
///
/// The IR decides this BY SHAPE in `link_callback_wrappers` -- exactly one
/// field whose type is a callback typedef, and exactly one field that is the
/// optional context slot -- so no name is consulted here or there.
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

/// Whether a value of this class is handed to the C layer AS ITSELF rather
/// than through a `.inner` field: the two categories the IR gives the
/// `ptr`/`len`/`cap`/`destructor` layout and the string layout. The wrapper
/// for such a class is `repr(transparent)` when it exists at all, so a
/// transmute of the whole value is the conversion.
fn is_direct_ffi_type(type_name: &str, ir: &CodegenIR) -> bool {
    matches!(
        category_of(type_name, ir),
        Some(TypeCategory::Vec) | Some(TypeCategory::String)
    )
}

/// The element of a borrowed STRING slice.
///
/// api.json declares neither this type nor an element for the slice that
/// holds it, and the C header has no `AzRefstr` either -- only the slice. So
/// this is the one element name the IR cannot supply. The type itself is real
/// and public (`azul_core::gl::Refstr`: a `{ptr, len}` view of UTF-8 with a
/// `From<&str>`), and the bridge builds it from the Python strings it is
/// handed; its PATH is derived from the slice's own external path, not
/// written down.
const PY_BORROWED_STR_ELEMENT: &str = "Refstr";

/// What a borrowed slice's elements are, which decides what Python lends.
#[derive(PartialEq, Eq, Clone, Copy)]
enum SliceElement {
    /// A number: the Python sequence IS the buffer (`bytes`, `list[int]`).
    Primitive,
    /// A class: its `repr(transparent)` wrapper makes a `Vec` of wrappers the
    /// same bytes as the C array, so that `Vec` is the buffer.
    Class,
    /// A borrowed string view: the strings live in one buffer and a second
    /// buffer of views points into them. Both live on this call's stack.
    BorrowedStr,
}

/// A borrowed `{ptr, len}` slice parameter -- what the IR classifies as
/// [`TypeCategory::VecRef`].
struct BorrowedSlice {
    /// Element type as the IR spells it (`u8`, `GLuint`, `TessellatedSvgNode`).
    element: String,
    /// The callee WRITES through the pointer (the `RefMut` half of the family).
    is_mutable: bool,
    /// What the elements are.
    kind: SliceElement,
    /// Field names of the mirror struct, read from the IR rather than assumed.
    ptr_field: String,
    len_field: String,
}

impl BorrowedSlice {
    /// The element type as the generated signature spells it.
    fn py_element(&self, prefix: &str) -> String {
        match self.kind {
            SliceElement::Primitive => self.element.clone(),
            SliceElement::Class => format!("{}{}", prefix, self.element),
            // A Python `str` owns its bytes; the view is built from it below.
            // The spelling is Rust's own String, the type written into the
            // signature -- not the API class that shares the name.
            // allow-api-name: the Rust type written into the signature
            SliceElement::BorrowedStr => "String".to_string(),
        }
    }
}

/// Describe `type_name` as a borrowed slice, if that is what it is.
///
/// The IR classifies the type (`TypeCategory::VecRef`) but does not keep the
/// element -- api.json declares the pointer as `c_void`, and the element is
/// recovered from the name the same way `ir_builder::vecref_layout_element`
/// recovers it for the classification itself: the part before the suffix
/// either names a class or is a primitive spelled in CamelCase (`U8` -> `u8`).
fn borrowed_slice_arg(type_name: &str, ir: &CodegenIR) -> Option<BorrowedSlice> {
    use super::ir::FieldRefKind;

    if category_of(type_name, ir) != Some(TypeCategory::VecRef) {
        return None;
    }
    let (prefix, is_mutable) = match type_name.strip_suffix("VecRefMut") {
        Some(p) => (p, true),
        None => (type_name.strip_suffix("VecRef")?, false),
    };
    let lowered = prefix.to_ascii_lowercase();
    let (element, kind) = if ir.find_struct(prefix).is_some() || ir.find_enum(prefix).is_some() {
        (prefix.to_string(), SliceElement::Class)
    } else if ir.find_type_alias(prefix).is_some() && is_primitive_type(prefix) {
        // A GL scalar typedef: a number under another name.
        (prefix.to_string(), SliceElement::Primitive)
    } else if is_primitive_type(&lowered) {
        (lowered, SliceElement::Primitive)
    } else if prefix == PY_BORROWED_STR_ELEMENT {
        (prefix.to_string(), SliceElement::BorrowedStr)
    } else {
        // An element shape nothing here can build.
        return None;
    };

    // The two fields, told apart by shape: one is the pointer, the other is
    // the length.
    let def = ir.find_struct(type_name)?;
    let ptr_field = def
        .fields
        .iter()
        .find(|f| matches!(f.ref_kind, FieldRefKind::Ptr | FieldRefKind::PtrMut))?;
    let len_field = def.fields.iter().find(|f| f.name != ptr_field.name)?;

    Some(BorrowedSlice {
        element,
        is_mutable,
        kind,
        ptr_field: ptr_field.name.clone(),
        len_field: len_field.name.clone(),
    })
}

/// The external path of a borrowed slice's ELEMENT: the slice's own external
/// path without the suffix that makes it a slice (`azul_core::gl::RefstrVecRef`
/// -> `azul_core::gl::Refstr`), which is the same relationship the element
/// name has to the slice name.
fn slice_element_path(slice_type: &str, external: &str) -> String {
    let suffix = if slice_type.ends_with("VecRefMut") {
        "VecRefMut"
    } else {
        "VecRef"
    };
    external.strip_suffix(suffix).unwrap_or(external).to_string()
}

/// What a raw-pointer argument points AT, as the body itself states it.
#[derive(PartialEq, Eq, Clone, Copy)]
enum PointerBorrow {
    /// `&*name`: one value, borrowed for the call and only read.
    Shared,
    /// `&mut *name`: one value, borrowed for the call and WRITTEN THROUGH.
    Mutable,
}

/// Whether a raw-pointer argument is ONE borrowed object rather than the head
/// of an array, and if so how it is borrowed.
///
/// Python can lend a value for the duration of a call -- the same contract
/// the borrowed-slice bridge honours -- but a value has one address, not
/// `len` of them, so the array form cannot be bridged at all. api.json states
/// which one it is in the BODY, the only place that knows: a single borrowed
/// object is dereferenced there (`&*info`, `&mut *model`), while an array
/// pointer is passed straight on next to its length
/// (`copy_from_ptr(ptr, len)`). Keying on the body keeps the 124
/// `copy_from_ptr`s out by construction instead of by a name test -- handing
/// one of those a single value and a caller-chosen `len` would read past the
/// end of it.
fn pointer_borrow(func: &FunctionDef, arg: &super::ir::FunctionArg) -> Option<PointerBorrow> {
    let body = func.fn_body.as_deref()?;
    // The deref at a word boundary, so `&*dom` does not match `&*dom_id`.
    // `&\*` cannot match the `&mut *` form, so the two are never confused.
    let derefs = |head: &str| {
        regex::Regex::new(&format!(r"{}\*{}($|[^\w])", head, regex::escape(&arg.name)))
            .map(|re| re.is_match(body))
            .unwrap_or(false)
    };
    match arg.ref_kind {
        ArgRefKind::Ptr if derefs("&") => Some(PointerBorrow::Shared),
        ArgRefKind::PtrMut if derefs("&mut ") => Some(PointerBorrow::Mutable),
        _ => None,
    }
}

/// An `Option<borrowed slice>` -- the optional form of [`BorrowedSlice`].
struct OptionalSlice {
    slice: BorrowedSlice,
    /// The payload's own class name, for naming its mirror struct.
    slice_type: String,
    /// Variant names, read from the IR so neither spelling is assumed.
    some_variant: String,
    none_variant: String,
}

/// Describe `type_name` as an optional borrowed slice, if that is its shape.
///
/// Structural, not the IR's `Option` category: exactly two variants, one
/// carrying a borrowed slice and one carrying nothing. That is the whole of
/// what the bridge needs to know -- which variant to build for a Python
/// value, and which for `None`.
fn optional_slice(type_name: &str, ir: &CodegenIR) -> Option<OptionalSlice> {
    let def = ir.find_enum(type_name)?;
    if def.variants.len() != 2 {
        return None;
    }
    let mut payload: Option<(String, String, BorrowedSlice)> = None;
    let mut empty: Option<String> = None;
    for variant in &def.variants {
        match &variant.kind {
            EnumVariantKind::Unit => empty = Some(variant.name.clone()),
            EnumVariantKind::Tuple(types) => {
                let (ty, _) = types.first()?;
                payload = Some((variant.name.clone(), ty.clone(), borrowed_slice_arg(ty, ir)?));
            }
            EnumVariantKind::Struct(_) => return None,
        }
    }
    let (some_variant, slice_type, slice) = payload?;
    Some(OptionalSlice {
        slice,
        slice_type,
        some_variant,
        none_variant: empty?,
    })
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
