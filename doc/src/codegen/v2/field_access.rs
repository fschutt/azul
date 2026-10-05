//! Which fields of an api.json struct a binding exposes as typed accessors,
//! and how each one is read and written (the field-access contract shared by
//! the scripting bindings: Ruby, Lua, PHP, Perl).
//!
//! The contract, per field shape:
//!
//! - [`FieldShape::Prim`] / [`FieldShape::UnitEnum`]: a C scalar, read and
//!   written in place (`bool` is the 1-byte C bool);
//! - [`FieldShape::Str`]: the String class - read by DECODING the bytes (the
//!   field is never consumed), written by releasing the old string through
//!   `delete` and moving a fresh one in;
//! - [`FieldShape::Value`]: a struct or tagged union stored inline. Reading
//!   either gives a view into the parent (writes reach the parent) or an
//!   independent copy through `clone`; writing releases the old value through
//!   `delete` (when the type owns heap memory) and then MOVES the new value
//!   in (the binding consumes its wrapper, or deep-copies one it cannot
//!   consume).
//!
//! Never exposed (`None` from [`field_shape`]): private fields, pointer /
//! reference / boxed fields, generics and arrays, callback typedefs, callback
//! wrappers, `RefAny`, and unions carrying one of those (`OptionRefAny`,
//! `OptionButtonOnClick`): those are wired by the closure plumbing, never by
//! writing bytes into a field.

use super::{
    config::CodegenConfig,
    ir::{
        CodegenIR, EnumVariantKind, FieldDef, FieldRefKind, FunctionDef, FunctionKind, StructDef,
        TypeCategory,
    },
    managed_host_invoker::is_callback_wrapper,
    managed_lang_helpers::is_refany_type,
};

/// How one field of a struct is read and written.
#[derive(Debug, Clone)]
pub enum FieldShape<'a> {
    /// A C primitive (`u32`, `f32`, `bool`, ...), after resolving plain
    /// type aliases (`GLuint = u32`).
    Prim { ty: String, is_bool: bool },
    /// A unit (C-like) enum: an integer at the C ABI.
    UnitEnum { name: String },
    /// The String class (`TypeCategory::String`).
    Str { name: String, delete: &'a FunctionDef },
    /// A struct or tagged union stored inline.
    Value {
        name: String,
        /// `Az<T>_delete`: the type owns heap memory.
        delete: Option<&'a FunctionDef>,
        /// `Az<T>_clone`: the deep copy.
        clone: Option<&'a FunctionDef>,
    },
}

impl<'a> FieldShape<'a> {
    /// The `_delete` export to release the field's old value with, if any.
    pub fn delete(&self) -> Option<&'a FunctionDef> {
        match self {
            FieldShape::Str { delete, .. } => Some(delete),
            FieldShape::Value { delete, .. } => *delete,
            _ => None,
        }
    }
}

/// The export of `kind` on `class`.
pub fn class_fn<'a>(ir: &'a CodegenIR, class: &str, kind: FunctionKind) -> Option<&'a FunctionDef> {
    ir.functions
        .iter()
        .find(|f| f.class_name == class && f.kind == kind)
}

/// Does a struct of this category get field accessors at all? The
/// containers (`Vec`, the String class, `RefAny`, the destructor tags) keep
/// their own API: writing `len` or `ptr` by hand would corrupt them.
pub fn struct_has_field_accessors(s: &StructDef) -> bool {
    s.generic_params.is_empty()
        && !matches!(
            s.category,
            TypeCategory::Vec
                | TypeCategory::VecRef
                | TypeCategory::String
                | TypeCategory::RefAny
                | TypeCategory::DestructorOrClone
                | TypeCategory::Recursive
                | TypeCategory::GenericTemplate
                | TypeCategory::Boxed
                | TypeCategory::CallbackTypedef
        )
        && s.callback_wrapper_info.is_none()
}

/// The fields of `s` a binding exposes, with their shapes, in declaration
/// order. Empty when the struct itself gets no accessors.
pub fn accessible_fields<'a>(
    s: &'a StructDef,
    ir: &'a CodegenIR,
    config: &CodegenConfig,
) -> Vec<(&'a FieldDef, FieldShape<'a>)> {
    if !struct_has_field_accessors(s) {
        return Vec::new();
    }
    s.fields
        .iter()
        .filter_map(|f| field_shape(f, ir, config).map(|shape| (f, shape)))
        .collect()
}

/// C primitive type names (the IR spelling).
pub fn is_c_primitive(t: &str) -> bool {
    matches!(
        t.trim(),
        "u8" | "u16"
            | "u32"
            | "u64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "usize"
            | "isize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "c_char"
            | "c_uchar"
            | "c_short"
            | "c_ushort"
            | "c_int"
            | "c_uint"
            | "c_long"
            | "c_ulong"
            | "c_float"
            | "c_double"
    )
}

/// Is `t` (or, for a union, one of its payloads) something only the
/// closure plumbing may write: a callback typedef, a callback wrapper or
/// `RefAny`?
fn is_closure_plumbing(t: &str, ir: &CodegenIR, depth: usize) -> bool {
    let t = t.trim();
    if is_callback_wrapper(ir, t)
        || is_refany_type(t, ir)
        || ir.callback_typedefs.iter().any(|c| c.name.trim() == t)
    {
        return true;
    }
    if depth == 0 {
        return false;
    }
    if let Some(e) = ir.find_enum(t) {
        return e.variants.iter().any(|v| match &v.kind {
            EnumVariantKind::Unit => false,
            EnumVariantKind::Tuple(items) => items
                .iter()
                .any(|(ty, _)| is_closure_plumbing(ty, ir, depth - 1)),
            EnumVariantKind::Struct(fields) => fields
                .iter()
                .any(|f| is_closure_plumbing(&f.type_name, ir, depth - 1)),
        });
    }
    false
}

/// How `f` is read and written, or `None` when it is not exposed (see the
/// module docs for the list).
pub fn field_shape<'a>(
    f: &FieldDef,
    ir: &'a CodegenIR,
    config: &CodegenConfig,
) -> Option<FieldShape<'a>> {
    if !f.is_public || f.ref_kind != FieldRefKind::Owned {
        return None;
    }
    let mut t = f.type_name.trim().to_string();
    if t.contains('<') || t.contains('[') || t.starts_with('*') || t.starts_with('&') {
        return None;
    }
    // Plain aliases (`GLuint = u32`) resolve to their target; monomorphized
    // ones (`CaretColorValue`) are concrete types of their own.
    for _ in 0..8 {
        match ir.find_type_alias(&t) {
            Some(ta) if ta.monomorphized_def.is_none() && ta.generic_args.is_empty() => {
                t = ta.target.trim().to_string();
            }
            _ => break,
        }
    }
    if matches!(t.as_str(), "c_void" | "void" | "()") {
        return None;
    }
    if is_c_primitive(&t) {
        return Some(FieldShape::Prim {
            is_bool: t == "bool",
            ty: t,
        });
    }
    if t.contains('<') || !config.should_include_type(&t) {
        return None;
    }
    if is_closure_plumbing(&t, ir, 1) {
        return None;
    }
    if let Some(e) = ir.find_enum(&t) {
        if !e.generic_params.is_empty()
            || matches!(
                e.category,
                TypeCategory::Recursive
                    | TypeCategory::GenericTemplate
                    | TypeCategory::DestructorOrClone
            )
        {
            return None;
        }
        if !e.is_union {
            return Some(FieldShape::UnitEnum { name: t });
        }
    } else if let Some(s) = ir.find_struct(&t) {
        if !s.generic_params.is_empty()
            || matches!(
                s.category,
                TypeCategory::Recursive
                    | TypeCategory::GenericTemplate
                    | TypeCategory::DestructorOrClone
                    | TypeCategory::VecRef
            )
        {
            return None;
        }
        if matches!(s.category, TypeCategory::String) {
            let delete = class_fn(ir, &t, FunctionKind::Delete)?;
            return Some(FieldShape::Str { name: t, delete });
        }
    } else if ir
        .find_type_alias(&t)
        .map_or(true, |ta| ta.monomorphized_def.is_none())
    {
        // Not a type the IR defines.
        return None;
    }
    let delete = class_fn(ir, &t, FunctionKind::Delete);
    let clone = class_fn(ir, &t, FunctionKind::DeepCopy);
    Some(FieldShape::Value { name: t, delete, clone })
}
