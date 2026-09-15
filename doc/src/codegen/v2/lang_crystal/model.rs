//! What every IR type becomes on the idiomatic Crystal side.
//!
//! The raw `lib LibAzul` layer mirrors the C ABI one to one. The `Azul::`
//! layer on top of it takes and returns native Crystal values wherever the
//! IR shape allows it, and this module is the single place that decides the
//! shape: a [`Ty`] per (type name, reference kind), derived from the IR's
//! structure rather than from class names.
//!
//! | IR shape                                   | Crystal                          |
//! |--------------------------------------------|----------------------------------|
//! | `String`                                   | `::String`                       |
//! | `RefAny`                                   | any Crystal object / `Azul::RefAny` |
//! | union `None` + `Some(T)`                   | `T?`                             |
//! | struct `{ptr, len, cap, destructor}` Vec   | `::Array(T)` (`::Bytes` for u8)  |
//! | union `Ok(T)` + `Err(E)` (return position) | `T`, raising `Azul::ResultError(E)` |
//! | fieldless enum                             | Crystal `enum` (symbols autocast)|
//! | other struct / tagged union                | `Azul::Name` wrapper class       |
//!
//! A container is only mapped when every conversion it needs exists (a Vec
//! needs `_copyFromPtr`, an element read out of borrowed memory needs Copy or
//! `_clone`); otherwise that position falls back to the wrapper class, which
//! is always available.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{
    config::CodegenConfig,
    ir::{
        ArgRefKind, CallbackTypedefDef, CallbackWrapperInfo, CodegenIR, EnumVariantKind, FieldDef,
        FieldRefKind, FunctionDef, FunctionKind, MonomorphizedKind, TypeCategory, TypeTraits,
    },
};

/// A Crystal-visible primitive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prim {
    Bool,
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
    USize,
    ISize,
}

impl Prim {
    pub fn from_rust(t: &str) -> Option<Prim> {
        Some(match t {
            "bool" | "GLboolean" => Prim::Bool,
            "u8" | "c_uchar" => Prim::U8,
            "i8" | "c_char" => Prim::I8,
            "u16" | "c_ushort" => Prim::U16,
            "i16" | "c_short" => Prim::I16,
            "u32" | "c_uint" | "GLuint" | "GLenum" | "GLbitfield" => Prim::U32,
            "i32" | "c_int" | "GLint" | "GLsizei" => Prim::I32,
            "u64" | "c_ulong" | "c_ulonglong" | "GLuint64" => Prim::U64,
            "i64" | "c_long" | "c_longlong" | "GLint64" => Prim::I64,
            "f32" | "c_float" | "GLfloat" | "GLclampf" => Prim::F32,
            "f64" | "c_double" | "GLdouble" | "GLclampd" => Prim::F64,
            "usize" | "size_t" | "uintptr_t" => Prim::USize,
            "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr" | "GLintptr" => Prim::ISize,
            _ => return None,
        })
    }

    /// The exact Crystal type (as the raw layer spells it).
    pub fn crystal(self) -> &'static str {
        match self {
            Prim::Bool => "::Bool",
            Prim::U8 => "::UInt8",
            Prim::I8 => "::Int8",
            Prim::U16 => "::UInt16",
            Prim::I16 => "::Int16",
            Prim::U32 => "::UInt32",
            Prim::I32 => "::Int32",
            Prim::U64 => "::UInt64",
            Prim::I64 => "::Int64",
            Prim::F32 => "::Float32",
            Prim::F64 => "::Float64",
            Prim::USize => "::LibC::SizeT",
            Prim::ISize => "::LibC::SSizeT",
        }
    }

    /// What an argument of this type accepts: any integer for the integer
    /// kinds (range-checked on conversion), any number for floats.
    pub fn restriction(self) -> &'static str {
        match self {
            Prim::Bool => "::Bool",
            Prim::F32 | Prim::F64 => "::Number",
            _ => "::Int",
        }
    }

    /// Convert a Crystal value `x` (satisfying [`Prim::restriction`]) to the
    /// exact type.
    pub fn convert(self, x: &str) -> String {
        match self {
            Prim::Bool => x.to_string(),
            Prim::F32 => format!("{}.to_f32", x),
            Prim::F64 => format!("{}.to_f64", x),
            _ => format!("{}.new({})", self.crystal(), x),
        }
    }
}

/// The Crystal-side shape of one type in one position.
#[derive(Clone, Debug)]
pub enum Ty {
    Void,
    Prim(Prim),
    /// Passed through as the raw pointer the lib layer declares.
    RawPtr(String),
    Str,
    RefAny,
    /// Fieldless enum: `Azul::Name`, an alias of the lib enum.
    Enum(String),
    /// Wrapper class `Azul::Name` (struct or tagged union).
    Class(String),
    Option {
        name: String,
        payload: Box<Ty>,
    },
    Vec {
        name: String,
        elem: Box<Ty>,
    },
    Result {
        name: String,
        ok: Box<Ty>,
        err: Box<Ty>,
    },
    /// A callback typedef (C function pointer).
    Callback(String),
    Unsupported(String),
}

/// One field of a struct or of a union variant.
#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub type_name: String,
    pub ref_kind: FieldRefKind,
    pub doc: Option<String>,
}

impl Field {
    fn from_def(f: &FieldDef) -> Field {
        Field {
            name: f.name.clone(),
            type_name: f.type_name.clone(),
            ref_kind: f.ref_kind,
            doc: f.doc.clone(),
        }
    }
}

/// One variant of a tagged union.
#[derive(Clone, Debug)]
pub struct Variant {
    pub name: String,
    pub index: usize,
    /// The lib-layer union member name (`none`, `some`, ...).
    pub member: String,
    /// Tuple payloads are named `payload` / `payload_<i>` in the lib layer.
    pub fields: Vec<Field>,
    pub doc: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Shape {
    Struct(Vec<Field>),
    Union { tag: &'static str, variants: Vec<Variant> },
}

/// Everything the emitter needs to know about one wrapper class.
#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub name: String,
    pub doc: Vec<String>,
    pub shape: Shape,
    pub traits: TypeTraits,
    pub callback_wrapper: Option<CallbackWrapperInfo>,
    pub category: TypeCategory,
}

pub struct Model<'a> {
    pub ir: &'a CodegenIR,
    pub config: &'a CodegenConfig,
    /// Every C symbol the lib layer binds.
    pub funs: BTreeSet<String>,
    pub classes: BTreeMap<String, ClassInfo>,
    /// Fieldless enums (incl. monomorphized ones), name -> variant names.
    pub enums: BTreeMap<String, Vec<String>>,
    /// Simple aliases (`GLuint = u32`), name -> target type name.
    pub aliases: BTreeMap<String, String>,
    pub callbacks: BTreeMap<String, &'a CallbackTypedefDef>,
    /// Functions per class, in IR order.
    pub functions: BTreeMap<String, Vec<&'a FunctionDef>>,
}

impl<'a> Model<'a> {
    pub fn new(ir: &'a CodegenIR, config: &'a CodegenConfig) -> Model<'a> {
        let mut funs = BTreeSet::new();
        let mut functions: BTreeMap<String, Vec<&FunctionDef>> = BTreeMap::new();
        for f in &ir.functions {
            if super::should_emit_function(f, ir, config) {
                funs.insert(f.c_name.clone());
                functions.entry(f.class_name.clone()).or_default().push(f);
            }
        }

        let mut classes = BTreeMap::new();
        let mut enums = BTreeMap::new();
        let mut aliases = BTreeMap::new();

        for s in &ir.structs {
            if !super::include_struct(s, config) {
                continue;
            }
            classes.insert(
                s.name.clone(),
                ClassInfo {
                    name: s.name.clone(),
                    doc: s.doc.clone(),
                    shape: Shape::Struct(s.fields.iter().map(Field::from_def).collect()),
                    traits: s.traits.clone(),
                    callback_wrapper: s.callback_wrapper_info.clone(),
                    category: s.category,
                },
            );
        }
        for e in &ir.enums {
            if !super::include_enum(e, config) || classes.contains_key(&e.name) {
                continue;
            }
            if !e.is_union {
                enums.insert(
                    e.name.clone(),
                    e.variants.iter().map(|v| v.name.clone()).collect(),
                );
                continue;
            }
            let variants = e
                .variants
                .iter()
                .enumerate()
                .map(|(index, v)| Variant {
                    name: v.name.clone(),
                    index,
                    member: super::sanitize_identifier(&super::types::union_field_name(&v.name)),
                    fields: match &v.kind {
                        EnumVariantKind::Unit => Vec::new(),
                        EnumVariantKind::Tuple(types) => types
                            .iter()
                            .enumerate()
                            .map(|(j, (ty, rk))| Field {
                                name: if types.len() == 1 {
                                    "payload".to_string()
                                } else {
                                    format!("payload_{}", j)
                                },
                                type_name: ty.clone(),
                                ref_kind: *rk,
                                doc: None,
                            })
                            .collect(),
                        EnumVariantKind::Struct(fields) => {
                            fields.iter().map(Field::from_def).collect()
                        }
                    },
                    doc: v.doc.clone(),
                })
                .collect();
            classes.insert(
                e.name.clone(),
                ClassInfo {
                    name: e.name.clone(),
                    doc: e.doc.clone(),
                    shape: Shape::Union {
                        tag: super::types::tag_type(e.repr.as_deref()),
                        variants,
                    },
                    traits: e.traits.clone(),
                    callback_wrapper: None,
                    category: e.category,
                },
            );
        }
        for ta in &ir.type_aliases {
            if !config.should_include_type(&ta.name) || classes.contains_key(&ta.name) {
                continue;
            }
            match &ta.monomorphized_def {
                None => {
                    aliases.insert(ta.name.clone(), ta.target.clone());
                }
                Some(mono) => match &mono.kind {
                    MonomorphizedKind::SimpleEnum { variants, .. } => {
                        enums.insert(ta.name.clone(), variants.clone());
                    }
                    MonomorphizedKind::Struct { fields } => {
                        classes.insert(
                            ta.name.clone(),
                            ClassInfo {
                                name: ta.name.clone(),
                                doc: ta.doc.clone(),
                                shape: Shape::Struct(fields.iter().map(Field::from_def).collect()),
                                traits: ta.traits.clone(),
                                callback_wrapper: None,
                                category: TypeCategory::Regular,
                            },
                        );
                    }
                    MonomorphizedKind::TaggedUnion { repr, variants } => {
                        let variants = variants
                            .iter()
                            .enumerate()
                            .map(|(index, v)| Variant {
                                name: v.name.clone(),
                                index,
                                member: super::sanitize_identifier(&super::types::union_field_name(&v.name)),
                                fields: v
                                    .payload_type
                                    .iter()
                                    .map(|p| Field {
                                        name: "payload".to_string(),
                                        type_name: p.clone(),
                                        ref_kind: v.payload_ref_kind,
                                        doc: None,
                                    })
                                    .collect(),
                                doc: None,
                            })
                            .collect();
                        classes.insert(
                            ta.name.clone(),
                            ClassInfo {
                                name: ta.name.clone(),
                                doc: ta.doc.clone(),
                                shape: Shape::Union {
                                    tag: super::types::tag_type(repr.as_deref()),
                                    variants,
                                },
                                traits: ta.traits.clone(),
                                callback_wrapper: None,
                                category: TypeCategory::Regular,
                            },
                        );
                    }
                },
            }
        }

        let callbacks = ir
            .callback_typedefs
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        Model {
            ir,
            config,
            funs,
            classes,
            enums,
            aliases,
            callbacks,
            functions,
        }
    }

    /// The C symbol `Az{class}_{suffix}` if the lib layer binds it.
    pub fn fun(&self, class: &str, suffix: &str) -> Option<String> {
        let c = format!("Az{}_{}", class, suffix);
        self.funs.contains(&c).then_some(c)
    }

    pub fn functions_of(&self, class: &str) -> &[&'a FunctionDef] {
        self.functions.get(class).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// The trait entry point of `kind` for `class`, if bound.
    pub fn trait_fn(&self, class: &str, kind: FunctionKind) -> Option<String> {
        self.functions_of(class)
            .iter()
            .find(|f| f.kind == kind)
            .map(|f| f.c_name.clone())
    }

    pub fn is_copy(&self, class: &str) -> bool {
        self.classes.get(class).is_some_and(|c| c.traits.is_copy)
    }

    pub fn delete_fn(&self, class: &str) -> Option<String> {
        self.trait_fn(class, FunctionKind::Delete)
    }

    pub fn clone_fn(&self, class: &str) -> Option<String> {
        self.trait_fn(class, FunctionKind::DeepCopy)
    }

    /// Can a borrowed value of this class be turned into an independent one?
    pub fn class_copyable(&self, class: &str) -> bool {
        self.is_copy(class) || self.clone_fn(class).is_some()
    }

    /// Resolve a simple alias chain (`GLuint -> u32`).
    fn unalias<'n>(&'n self, mut name: &'n str) -> &'n str {
        for _ in 0..16 {
            match self.aliases.get(name) {
                Some(t) => name = t.as_str(),
                None => break,
            }
        }
        name
    }

    /// The shape of an owned (by-value) value of `type_name`.
    pub fn owned(&self, type_name: &str) -> Ty {
        // The lib layer maps a primitive-looking alias (`GLboolean` -> Bool)
        // before any alias resolution, so ask the same question first.
        if let Some(p) = Prim::from_rust(type_name.trim()) {
            return Ty::Prim(p);
        }
        let name = self.unalias(type_name.trim());
        match name {
            "void" | "c_void" | "()" => return Ty::Void,
            _ => {}
        }
        if let Some(p) = Prim::from_rust(name) {
            return Ty::Prim(p);
        }
        if name == "String" && self.classes.contains_key("String") {
            return Ty::Str;
        }
        if name == "RefAny" && self.classes.contains_key("RefAny") {
            return Ty::RefAny;
        }
        if self.enums.contains_key(name) {
            return Ty::Enum(name.to_string());
        }
        if self.callbacks.contains_key(name) {
            return Ty::Callback(name.to_string());
        }
        let Some(class) = self.classes.get(name) else {
            return Ty::Unsupported(name.to_string());
        };
        if let Some(t) = self.option_shape(class) {
            return t;
        }
        if let Some(t) = self.vec_shape(class) {
            return t;
        }
        if let Some(t) = self.result_shape(class) {
            return t;
        }
        Ty::Class(name.to_string())
    }

    /// The shape of a field.
    pub fn field(&self, f: &Field) -> Ty {
        match f.ref_kind {
            FieldRefKind::Owned => self.owned(&f.type_name),
            _ => Ty::RawPtr(super::wrappers::qualify(&super::field_type_for_ref_kind(
                &f.type_name,
                &f.ref_kind,
                self.ir,
            ))),
        }
    }

    fn option_shape(&self, class: &ClassInfo) -> Option<Ty> {
        let Shape::Union { variants, .. } = &class.shape else {
            return None;
        };
        if variants.len() != 2 {
            return None;
        }
        let none = variants.iter().find(|v| v.name == "None")?;
        let some = variants.iter().find(|v| v.name == "Some")?;
        if !none.fields.is_empty() || some.fields.len() != 1 {
            return None;
        }
        if some.fields[0].ref_kind != FieldRefKind::Owned {
            return None;
        }
        let payload = self.owned(&some.fields[0].type_name);
        if !self.convertible(&payload) {
            return None;
        }
        Some(Ty::Option {
            name: class.name.clone(),
            payload: Box::new(payload),
        })
    }

    fn vec_shape(&self, class: &ClassInfo) -> Option<Ty> {
        let Shape::Struct(fields) = &class.shape else {
            return None;
        };
        if !class.name.ends_with("Vec") || fields.len() != 4 {
            return None;
        }
        let ptr = fields.iter().find(|f| f.name == "ptr")?;
        let len = fields.iter().find(|f| f.name == "len")?;
        let cap = fields.iter().find(|f| f.name == "cap")?;
        fields.iter().find(|f| f.name == "destructor")?;
        if !matches!(ptr.ref_kind, FieldRefKind::Ptr | FieldRefKind::PtrMut)
            || len.type_name != "usize"
            || cap.type_name != "usize"
        {
            return None;
        }
        self.fun(&class.name, "copyFromPtr")?;
        let elem = self.owned(&ptr.type_name);
        // Reading elements out of the vec needs an independent copy of each.
        let ok = match &elem {
            Ty::Prim(_) | Ty::Enum(_) | Ty::Str => true,
            Ty::Class(c) => self.class_copyable(c),
            Ty::RefAny => true,
            Ty::Option { .. } | Ty::Vec { .. } => self.convertible(&elem),
            _ => false,
        };
        if !ok {
            return None;
        }
        Some(Ty::Vec {
            name: class.name.clone(),
            elem: Box::new(elem),
        })
    }

    fn result_shape(&self, class: &ClassInfo) -> Option<Ty> {
        let Shape::Union { variants, .. } = &class.shape else {
            return None;
        };
        if variants.len() != 2 || !class.name.starts_with("Result") {
            return None;
        }
        let ok = variants.iter().find(|v| v.name == "Ok")?;
        let err = variants.iter().find(|v| v.name == "Err")?;
        if ok.fields.len() != 1 || err.fields.len() != 1 {
            return None;
        }
        if ok.fields[0].ref_kind != FieldRefKind::Owned
            || err.fields[0].ref_kind != FieldRefKind::Owned
        {
            return None;
        }
        let okt = self.owned(&ok.fields[0].type_name);
        let errt = self.owned(&err.fields[0].type_name);
        if !self.convertible(&okt) || !self.convertible(&errt) {
            return None;
        }
        Some(Ty::Result {
            name: class.name.clone(),
            ok: Box::new(okt),
            err: Box::new(errt),
        })
    }

    /// Can a value of this shape cross the boundary in both directions
    /// (native -> owned FFI value, and borrowed FFI value -> native copy)?
    pub fn convertible(&self, t: &Ty) -> bool {
        match t {
            Ty::Prim(_) | Ty::Str | Ty::RefAny | Ty::Enum(_) => true,
            Ty::Class(c) => self.class_copyable(c),
            Ty::Option { .. } | Ty::Vec { .. } => true,
            Ty::Result { .. } => false,
            Ty::Void | Ty::RawPtr(_) | Ty::Callback(_) | Ty::Unsupported(_) => false,
        }
    }

    /// The `get_ctx` of an info type, if it has one: that is where a closure
    /// stored in a callback's ctx is read back.
    pub fn ctx_getter(&self, info_type: &str) -> Option<String> {
        self.functions_of(info_type)
            .iter()
            .find(|f| {
                f.method_name == "get_ctx"
                    && f.args.len() == 1
                    && matches!(f.args[0].ref_kind, ArgRefKind::Ref | ArgRefKind::Ptr)
                    && f.return_type.as_deref() == Some("OptionRefAny")
            })
            .map(|f| f.c_name.clone())
    }
}
