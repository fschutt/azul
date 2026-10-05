//! Which fields of an api.json struct a "classic" binding (Ada, Fortran,
//! FreeBASIC, VB6, Pascal) surfaces as getter/setter pairs on its wrapper
//! type, and what each one has to do to stay memory-safe.
//!
//! The contract every binding implements (2026-10-05 field-access wave):
//!
//! - a GETTER returns an independent value: a scalar (the 1-byte C bool as
//!   the language's boolean), the string class decoded WITHOUT consuming the
//!   field, a heap-owning value deep-copied through its `_clone` (no `_clone`
//!   -> no getter: a shallow copy would be freed twice), a plain copy of a POD;
//! - a SETTER releases the field's old value through its `_delete` (when the
//!   type has one), then MOVES the new value in (a wrapper argument is
//!   consumed: its destructor is disarmed);
//! - callback, callback-wrapper, RefAny, pointer and generic fields are never
//!   accessors: the closure plumbing owns them.
//!
//! Everything here is derived from the IR (type categories, function kinds),
//! never from the spelling of one API type.

use super::{
    config::CodegenConfig,
    ir::{CodegenIR, FieldDef, FieldRefKind, FunctionDef, FunctionKind, StructDef, TypeCategory},
    managed_host_invoker, managed_lang_helpers,
};

/// The shape of one accessible field.
#[derive(Clone, Copy, Debug)]
pub enum FieldKind<'a> {
    /// A C primitive (`ty` is the primitive after resolving simple type
    /// aliases). `is_bool` marks the 1-byte C bool.
    Prim { is_bool: bool },
    /// A unit enum (a C `int`).
    UnitEnum,
    /// The API's string class.
    Str {
        delete: &'a FunctionDef,
    },
    /// Any other struct / tagged union, carried by value.
    Value {
        delete: Option<&'a FunctionDef>,
        clone: Option<&'a FunctionDef>,
    },
}

impl FieldKind<'_> {
    /// A getter exists unless the field owns heap memory it cannot deep-copy.
    pub fn has_getter(&self) -> bool {
        !matches!(
            self,
            FieldKind::Value {
                delete: Some(_),
                clone: None
            }
        )
    }
}

/// One field a binding surfaces as accessors.
#[derive(Clone, Copy, Debug)]
pub struct AccessField<'a> {
    pub field: &'a FieldDef,
    /// The field's type, trimmed; for a simple alias of a primitive, the
    /// primitive it resolves to.
    pub ty: &'a str,
    pub kind: FieldKind<'a>,
}

/// `Az<T>_delete` of a type, if the C API has one.
pub fn delete_fn<'a>(ir: &'a CodegenIR, ty: &str) -> Option<&'a FunctionDef> {
    let ty = ty.trim();
    ir.functions
        .iter()
        .find(|f| f.class_name == ty && f.kind == FunctionKind::Delete)
}

/// `Az<T>_clone` (the deep copy) of a type, if the C API has one.
pub fn clone_fn<'a>(ir: &'a CodegenIR, ty: &str) -> Option<&'a FunctionDef> {
    let ty = ty.trim();
    ir.functions
        .iter()
        .find(|f| f.class_name == ty && f.kind == FunctionKind::DeepCopy)
}

/// The API's string class, if the IR has one.
pub fn string_class(ir: &CodegenIR) -> Option<&StructDef> {
    ir.structs
        .iter()
        .find(|s| s.category == TypeCategory::String)
}

/// Is `ty` the API's string class?
pub fn is_string(ir: &CodegenIR, ty: &str) -> bool {
    ir.find_struct(ty.trim())
        .is_some_and(|s| s.category == TypeCategory::String)
}

/// The string class's byte-copying constructor: the one shaped
/// `(ptr, start: usize, len: usize) -> String`. It copies the bytes, so a
/// host string's buffer never has to outlive the call.
pub fn string_copy_fn(ir: &CodegenIR) -> Option<&FunctionDef> {
    let st = string_class(ir)?;
    ir.functions.iter().find(|f| {
        f.class_name == st.name
            && f.args.len() == 3
            && f.args[1].type_name.trim() == "usize"
            && f.args[2].type_name.trim() == "usize"
            && f.return_type.as_deref().map(str::trim) == Some(st.name.as_str())
    })
}

/// The raw field names along which a string's bytes are reached:
/// `(vec, ptr, len)` as in `s.vec.ptr` / `s.vec.len` - read off the IR
/// (the string class wraps one byte vector; the vector's first pointer
/// field is the data, its first `usize` field after that the length).
pub fn string_layout(ir: &CodegenIR) -> Option<(String, String, String)> {
    let st = string_class(ir)?;
    let vec_field = st.fields.first()?;
    let vec = ir.find_struct(vec_field.type_name.trim())?;
    let ptr_idx = vec
        .fields
        .iter()
        .position(|f| matches!(f.ref_kind, FieldRefKind::Ptr | FieldRefKind::PtrMut))?;
    let len = vec.fields[ptr_idx + 1..]
        .iter()
        .find(|f| f.ref_kind == FieldRefKind::Owned && f.type_name.trim() == "usize")?;
    Some((
        vec_field.name.clone(),
        vec.fields[ptr_idx].name.clone(),
        len.name.clone(),
    ))
}

const PRIMITIVES: &[&str] = &[
    "bool", "u8", "i8", "u16", "i16", "u32", "i32", "u64", "i64", "f32", "f64", "usize", "isize",
    "c_char", "c_uchar", "c_int", "c_uint", "char",
];

/// Classify one field; `None` = not an accessor.
pub fn classify<'a>(
    f: &'a FieldDef,
    ir: &'a CodegenIR,
    config: &CodegenConfig,
) -> Option<AccessField<'a>> {
    if !f.is_public || f.ref_kind != FieldRefKind::Owned {
        return None;
    }
    let mut ty: &'a str = f.type_name.trim();
    if ty.is_empty() || ty.contains('<') || ty.starts_with('[') || ty.contains('*') || ty.starts_with('&') {
        return None;
    }
    // A simple alias of a primitive (`ScanCode = u32`) is that primitive.
    if let Some(ta) = ir.find_type_alias(ty) {
        if ta.monomorphized_def.is_none() && ta.generic_args.is_empty() {
            let target = ta.target.trim();
            if PRIMITIVES.contains(&target) {
                ty = target;
            } else {
                return None;
            }
        }
    }
    if PRIMITIVES.contains(&ty) {
        return Some(AccessField {
            field: f,
            ty,
            kind: FieldKind::Prim {
                is_bool: ty == "bool",
            },
        });
    }
    if !config.should_include_type(ty) {
        return None;
    }
    // Callbacks, their wrappers and the type-erased handle are wired up by
    // the closure plumbing, never by poking bytes into a field.
    if ir.callback_typedefs.iter().any(|c| c.name == ty)
        || managed_host_invoker::is_callback_wrapper(ir, ty)
        || managed_lang_helpers::is_refany_type(ty, ir)
    {
        return None;
    }
    if is_string(ir, ty) {
        let delete = delete_fn(ir, ty)?;
        return Some(AccessField {
            field: f,
            ty,
            kind: FieldKind::Str { delete },
        });
    }
    if let Some(s) = ir.find_struct(ty) {
        if !s.generic_params.is_empty()
            || matches!(
                s.category,
                TypeCategory::Recursive
                    | TypeCategory::VecRef
                    | TypeCategory::DestructorOrClone
                    | TypeCategory::GenericTemplate
                    | TypeCategory::CallbackTypedef
            )
        {
            return None;
        }
    } else if let Some(e) = ir.find_enum(ty) {
        if !e.generic_params.is_empty()
            || matches!(
                e.category,
                TypeCategory::Recursive
                    | TypeCategory::DestructorOrClone
                    | TypeCategory::GenericTemplate
            )
        {
            return None;
        }
        if !e.is_union {
            return Some(AccessField {
                field: f,
                ty,
                kind: FieldKind::UnitEnum,
            });
        }
    } else if ir.find_type_alias(ty).is_none() {
        return None;
    }
    Some(AccessField {
        field: f,
        ty,
        kind: FieldKind::Value {
            delete: delete_fn(ir, ty),
            clone: clone_fn(ir, ty),
        },
    })
}

/// Every accessible field of `s`, in declaration order.
pub fn accessible_fields<'a>(
    s: &'a StructDef,
    ir: &'a CodegenIR,
    config: &CodegenConfig,
) -> Vec<AccessField<'a>> {
    s.fields
        .iter()
        .filter_map(|f| classify(f, ir, config))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(ir: &'a CodegenIR, s: &str, f: &str) -> Option<AccessField<'a>> {
        let st = ir.find_struct(s).expect("struct");
        let fd = st.fields.iter().find(|x| x.name == f).expect("field");
        classify(fd, ir, &CodegenConfig::c_header())
    }

    #[test]
    fn the_window_title_is_a_string_field_released_through_the_string_delete() {
        let ir = super::super::bug_classes::ir();
        let f = field(ir, "FullWindowState", "title").expect("title is accessible");
        assert!(matches!(f.kind, FieldKind::Str { .. }), "{:?}", f.kind);
    }

    #[test]
    fn the_window_state_is_a_value_field_with_a_deep_copy_and_a_delete() {
        let ir = super::super::bug_classes::ir();
        let f = field(ir, "WindowCreateOptions", "window_state").expect("window_state");
        assert!(
            matches!(f.kind, FieldKind::Value { delete: Some(_), clone: Some(_) }),
            "{:?}",
            f.kind
        );
    }

    #[test]
    fn a_checkbox_checked_flag_is_a_bool_primitive() {
        let ir = super::super::bug_classes::ir();
        let f = field(ir, "CheckBoxState", "checked").expect("checked");
        assert!(matches!(f.kind, FieldKind::Prim { is_bool: true }));
    }
}
