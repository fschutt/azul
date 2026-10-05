//! Which fields of an api.json struct get a getter / setter in the bindings
//! that expose structs as raw C records (Julia, Nim, Odin, V, OCaml), and
//! how each one has to be read and written.
//!
//! The field-access contract (the same in every binding):
//!
//! - a GETTER returns an independent value: a primitive as a native scalar,
//!   the String class decoded to a native string WITHOUT consuming the
//!   field, a heap-owning type (one with `Az<T>_delete`) as a deep copy
//!   through `Az<T>_clone` (no `_clone` -> no getter, never a shallow copy),
//!   plain data as a plain copy;
//! - a SETTER releases the field's old value (`Az<T>_delete` on the field's
//!   address, only when the type has one), then moves the new value in - a
//!   native string becomes a fresh AzString, a record argument is taken over
//!   (the caller hands its ownership in, like any by-value C argument);
//! - callbacks, callback wrappers, `RefAny`, pointers and generics are
//!   skipped.
//!
//! The emitters only decide the spelling; this module decides the shape.

use super::{
    config::CodegenConfig,
    ir::{
        CodegenIR, EnumVariantKind, FieldDef, FieldRefKind, FunctionDef, FunctionKind,
        MonomorphizedKind, StructDef, TypeCategory,
    },
    managed_host_invoker::is_callback_wrapper,
    managed_lang_helpers::is_refany_type,
};

/// How one field is read and written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawFieldKind {
    /// A C primitive, read and written in place; `is_bool` marks the 1-byte
    /// C `bool`.
    Prim { is_bool: bool },
    /// The String class: read as a native string without consuming the
    /// field; written as a fresh AzString after `delete` (the C symbol)
    /// released the old one.
    Str { delete: String },
    /// A heap-owning type: the getter deep-copies through `clone` (the C
    /// symbol; `None` -> no getter), the setter calls `delete` on the old
    /// value, then moves the new one in.
    Heap { delete: String, clone: Option<String> },
    /// Plain data (no `_delete` anywhere inside): copied both ways.
    Pod,
}

impl RawFieldKind {
    /// Whether a getter may be emitted (a heap-owning field without a deep
    /// copy has none: a shallow copy would be freed twice).
    pub fn has_getter(&self) -> bool {
        !matches!(self, RawFieldKind::Heap { clone: None, .. })
    }
}

/// One accessible field of a struct.
#[derive(Debug, Clone)]
pub struct RawField<'a> {
    /// 0-based position of the field in the struct.
    pub index: usize,
    pub def: &'a FieldDef,
    pub kind: RawFieldKind,
}

const C_PRIMITIVES: &[&str] = &[
    "u8", "u16", "u32", "u64", "usize", "i8", "i16", "i32", "i64", "isize", "f32", "f64",
    "bool", "c_char", "c_uchar", "c_int", "c_uint", "c_long", "c_ulong", "c_float", "c_double",
];

/// `t` (or the type alias it names) is a C primitive.
fn is_primitive(t: &str, ir: &CodegenIR) -> bool {
    let t = t.trim();
    if C_PRIMITIVES.contains(&t) {
        return true;
    }
    ir.find_type_alias(t).is_some_and(|a| {
        a.monomorphized_def.is_none()
            && a.generic_args.is_empty()
            && C_PRIMITIVES.contains(&a.target.trim())
    })
}

fn kind_fn<'a>(
    ir: &'a CodegenIR,
    class: &str,
    kind: FunctionKind,
    emitted: &dyn Fn(&FunctionDef) -> bool,
) -> Option<&'a FunctionDef> {
    ir.functions
        .iter()
        .find(|f| f.class_name == class && f.kind == kind && emitted(f))
}

fn has_kind_fn(ir: &CodegenIR, class: &str, kind: FunctionKind) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == class && f.kind == kind)
}

/// Does a value of `t` own heap memory - does it, or anything it holds by
/// value, have a `_delete`? Unknown types count as owning (conservative:
/// the field is then skipped rather than shallow-copied).
pub fn owns_heap(t: &str, ir: &CodegenIR) -> bool {
    owns_heap_depth(t, ir, 0)
}

fn owns_heap_depth(t: &str, ir: &CodegenIR, depth: usize) -> bool {
    let t = t.trim();
    if depth > 32 {
        return true;
    }
    if is_primitive(t, ir) || matches!(t, "c_void" | "void" | "()") {
        return false;
    }
    if t.starts_with('[') && t.ends_with(']') {
        let inner = &t[1..t.len() - 1];
        return match inner.rfind(';') {
            Some(semi) => owns_heap_depth(&inner[..semi], ir, depth + 1),
            None => true,
        };
    }
    if has_kind_fn(ir, t, FunctionKind::Delete) {
        return true;
    }
    let field_owns = |f: &FieldDef| match f.ref_kind {
        FieldRefKind::Owned => owns_heap_depth(&f.type_name, ir, depth + 1),
        FieldRefKind::Boxed | FieldRefKind::OptionBoxed => true,
        // A borrowed pointer is not released by its holder.
        FieldRefKind::Ref | FieldRefKind::RefMut | FieldRefKind::Ptr | FieldRefKind::PtrMut => {
            false
        }
    };
    if let Some(s) = ir.find_struct(t) {
        if matches!(
            s.category,
            TypeCategory::String | TypeCategory::Vec | TypeCategory::RefAny | TypeCategory::Boxed
        ) {
            return true;
        }
        return s.fields.iter().any(field_owns);
    }
    if let Some(e) = ir.find_enum(t) {
        return e.variants.iter().any(|v| match &v.kind {
            EnumVariantKind::Unit => false,
            EnumVariantKind::Tuple(items) => items.iter().any(|(ty, rk)| match rk {
                FieldRefKind::Owned => owns_heap_depth(ty, ir, depth + 1),
                FieldRefKind::Boxed | FieldRefKind::OptionBoxed => true,
                _ => false,
            }),
            EnumVariantKind::Struct(fields) => fields.iter().any(field_owns),
        });
    }
    if let Some(a) = ir.find_type_alias(t) {
        return match &a.monomorphized_def {
            Some(m) => match &m.kind {
                MonomorphizedKind::SimpleEnum { .. } => false,
                MonomorphizedKind::Struct { fields } => fields.iter().any(field_owns),
                MonomorphizedKind::TaggedUnion { variants, .. } => variants.iter().any(|v| {
                    match (&v.payload_type, v.payload_ref_kind) {
                        (None, _) => false,
                        (Some(p), FieldRefKind::Owned) => owns_heap_depth(p, ir, depth + 1),
                        (Some(_), FieldRefKind::Boxed | FieldRefKind::OptionBoxed) => true,
                        _ => false,
                    }
                }),
            },
            None if a.generic_args.is_empty() => owns_heap_depth(&a.target, ir, depth + 1),
            None => true,
        };
    }
    true
}

/// Does `t` carry a callback, a callback wrapper or a `RefAny` (directly,
/// or as the payload of an option-like enum)? Those are wired by the
/// closure plumbing, never by poking bytes into a field.
fn is_callback_or_refany(t: &str, ir: &CodegenIR, depth: usize) -> bool {
    let t = t.trim();
    if depth > 4 {
        return false;
    }
    if is_callback_wrapper(ir, t)
        || is_refany_type(t, ir)
        || ir.callback_typedefs.iter().any(|c| c.name.trim() == t)
    {
        return true;
    }
    if let Some(s) = ir.find_struct(t) {
        if matches!(
            s.category,
            TypeCategory::CallbackDataPair
                | TypeCategory::CallbackTypedef
                | TypeCategory::RefAny
                | TypeCategory::DestructorOrClone
        ) {
            return true;
        }
    }
    if let Some(e) = ir.find_enum(t) {
        if matches!(e.category, TypeCategory::DestructorOrClone) {
            return true;
        }
        return e.variants.iter().any(|v| match &v.kind {
            EnumVariantKind::Tuple(items) => items
                .iter()
                .any(|(ty, _)| is_callback_or_refany(ty, ir, depth + 1)),
            _ => false,
        });
    }
    false
}

/// How `f` is read and written, or `None` when it gets no accessor.
/// `emitted` says whether the binding declares a C function (the `_delete`
/// / `_clone` the accessor would call).
pub fn classify_field(
    f: &FieldDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
    emitted: &dyn Fn(&FunctionDef) -> bool,
) -> Option<RawFieldKind> {
    if !f.is_public || f.ref_kind != FieldRefKind::Owned {
        return None;
    }
    let t = f.type_name.trim();
    if t.is_empty() || t.contains('<') || t.starts_with('[') || t.starts_with('*') || t.starts_with('&') {
        return None;
    }
    if matches!(t, "c_void" | "void" | "()") {
        return None;
    }
    if is_primitive(t, ir) {
        return Some(RawFieldKind::Prim { is_bool: t == "bool" });
    }
    if !config.should_include_type(t) || is_callback_or_refany(t, ir, 0) {
        return None;
    }
    if ir
        .find_struct(t)
        .is_some_and(|s| s.category == TypeCategory::String)
    {
        let delete = kind_fn(ir, t, FunctionKind::Delete, emitted)?;
        return Some(RawFieldKind::Str {
            delete: delete.c_name.clone(),
        });
    }
    if has_kind_fn(ir, t, FunctionKind::Delete) {
        let delete = kind_fn(ir, t, FunctionKind::Delete, emitted)?;
        let clone = kind_fn(ir, t, FunctionKind::DeepCopy, emitted).map(|c| c.c_name.clone());
        return Some(RawFieldKind::Heap {
            delete: delete.c_name.clone(),
            clone,
        });
    }
    if owns_heap(t, ir) {
        // Heap memory inside, but no `_delete` to release it with: neither
        // a copy nor an overwrite is safe.
        return None;
    }
    Some(RawFieldKind::Pod)
}

/// Does `s` get field accessors at all? The String / Vec / slice types,
/// callback wrappers, `RefAny` and the internal destructor types are
/// handled by their own helpers.
pub fn struct_gets_accessors(s: &StructDef, config: &CodegenConfig) -> bool {
    config.should_include_type(&s.name)
        && s.generic_params.is_empty()
        && s.callback_wrapper_info.is_none()
        && !matches!(
            s.category,
            TypeCategory::String
                | TypeCategory::Vec
                | TypeCategory::VecRef
                | TypeCategory::RefAny
                | TypeCategory::CallbackTypedef
                | TypeCategory::CallbackDataPair
                | TypeCategory::DestructorOrClone
                | TypeCategory::Recursive
                | TypeCategory::GenericTemplate
                | TypeCategory::Boxed
        )
}

/// The accessible fields of `s`, in declaration order (empty when the
/// struct gets no accessors).
pub fn accessible_fields<'a>(
    s: &'a StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
    emitted: &dyn Fn(&FunctionDef) -> bool,
) -> Vec<RawField<'a>> {
    if !struct_gets_accessors(s, config) {
        return Vec::new();
    }
    s.fields
        .iter()
        .enumerate()
        .filter_map(|(index, def)| {
            classify_field(def, ir, config, emitted).map(|kind| RawField { index, def, kind })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ir() -> &'static CodegenIR {
        super::super::bug_classes::ir()
    }

    fn kind_of(class: &str, field: &str) -> Option<RawFieldKind> {
        let s = ir().find_struct(class).expect(class);
        let f = s.fields.iter().find(|f| f.name == field).expect(field);
        classify_field(f, ir(), &CodegenConfig::c_header(), &|_| true)
    }

    #[test]
    fn the_window_title_is_a_string_released_through_its_delete() {
        assert_eq!(
            kind_of("FullWindowState", "title"),
            Some(RawFieldKind::Str {
                delete: "AzString_delete".into()
            })
        );
    }

    #[test]
    fn the_window_state_is_deep_copied_and_released_through_its_delete() {
        assert_eq!(
            kind_of("WindowCreateOptions", "window_state"),
            Some(RawFieldKind::Heap {
                delete: "AzFullWindowState_delete".into(),
                clone: Some("AzFullWindowState_clone".into()),
            })
        );
    }

    #[test]
    fn a_checkbox_flag_is_a_bool_and_a_window_size_is_plain_data() {
        assert_eq!(
            kind_of("CheckBoxState", "checked"),
            Some(RawFieldKind::Prim { is_bool: true })
        );
        assert_eq!(kind_of("FullWindowState", "size"), Some(RawFieldKind::Pod));
        assert_eq!(kind_of("WindowSize", "dimensions"), Some(RawFieldKind::Pod));
    }

    #[test]
    fn the_text_input_text_is_heap_owned_even_though_get_text_exists() {
        assert!(matches!(
            kind_of("TextInputState", "text"),
            Some(RawFieldKind::Heap { .. })
        ));
    }

    #[test]
    fn a_callback_field_gets_no_accessor() {
        let s = ir().find_struct("WindowCreateOptions").unwrap();
        for f in &s.fields {
            if is_callback_or_refany(&f.type_name, ir(), 0) {
                assert_eq!(
                    classify_field(f, ir(), &CodegenConfig::c_header(), &|_| true),
                    None,
                    "{}",
                    f.name
                );
            }
        }
    }
}
