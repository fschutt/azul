//! What every IR type becomes in Swift.
//!
//! Swift reads the C ABI itself (the `CAzul` Clang module over `azul.h`), so
//! nothing here re-declares a C layout. This module decides the Swift-side
//! SHAPE of each api.json type, derived from the IR's structure rather than
//! from class names:
//!
//! | IR shape                                     | Swift                                  |
//! |----------------------------------------------|----------------------------------------|
//! | `String`                                     | `Swift.String`                         |
//! | `RefAny`                                     | any class instance / `RefAny`          |
//! | union `None` + `Some(T)`                     | `T?`                                   |
//! | struct `{ptr, len, cap, destructor}` Vec     | `[T]` (`[UInt8]` for `U8Vec`)          |
//! | union `Ok(T)` + `Err(E)` (return position)   | `T`, `throws AzulError<E>`             |
//! | fieldless enum                               | `enum` (`.primary`)                    |
//! | tagged union                                 | `enum` with associated values          |
//! | `Copy` struct without pointers, recursively  | `struct` over the C value              |
//! | any other struct                             | `final class` owning (or viewing) it   |
//!
//! A container is only mapped when every conversion it needs exists (a Vec
//! needs `_copyFromPtr`, an element read out of borrowed memory needs Copy or
//! `_clone`); otherwise that position keeps the type's own Swift declaration.

use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
};

use super::super::{
    config::CodegenConfig,
    ir::{
        ArgRefKind, CallbackTypedefDef, CallbackWrapperInfo, CodegenIR, EnumVariantKind, FieldDef,
        FieldRefKind, FunctionDef, FunctionKind, MonomorphizedKind, TypeCategory, TypeTraits,
    },
};

/// A Swift-visible primitive, spelled the way Swift imports the C type.
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
    /// `size_t` and `ssize_t` both import as `Int`.
    Int,
    CChar,
    CLong,
    CULong,
}

impl Prim {
    pub fn from_rust(t: &str) -> Option<Prim> {
        Some(match t {
            "bool" => Prim::Bool,
            // `typedef uint8_t AzGLboolean;`
            "u8" | "c_uchar" | "GLboolean" => Prim::U8,
            "i8" => Prim::I8,
            "c_char" => Prim::CChar,
            "u16" | "c_ushort" => Prim::U16,
            "i16" | "c_short" => Prim::I16,
            "u32" | "c_uint" | "GLuint" | "GLenum" | "GLbitfield" => Prim::U32,
            "i32" | "c_int" | "GLint" | "GLsizei" => Prim::I32,
            "u64" | "c_ulonglong" | "GLuint64" => Prim::U64,
            "i64" | "c_longlong" | "GLint64" => Prim::I64,
            "c_long" => Prim::CLong,
            "c_ulong" => Prim::CULong,
            "f32" | "c_float" | "GLfloat" | "GLclampf" => Prim::F32,
            "f64" | "c_double" | "GLdouble" | "GLclampd" => Prim::F64,
            "usize" | "size_t" | "uintptr_t" | "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr"
            | "GLintptr" => Prim::Int,
            _ => return None,
        })
    }

    pub fn swift(self) -> &'static str {
        match self {
            Prim::Bool => "Bool",
            Prim::U8 => "UInt8",
            Prim::I8 => "Int8",
            Prim::U16 => "UInt16",
            Prim::I16 => "Int16",
            Prim::U32 => "UInt32",
            Prim::I32 => "Int32",
            Prim::U64 => "UInt64",
            Prim::I64 => "Int64",
            Prim::F32 => "Float",
            Prim::F64 => "Double",
            Prim::Int => "Int",
            Prim::CChar => "CChar",
            Prim::CLong => "CLong",
            Prim::CULong => "CUnsignedLong",
        }
    }
}

/// The Swift-side shape of one type in one position.
#[derive(Clone, Debug)]
pub enum Ty {
    Void,
    Prim(Prim),
    /// A pointer passed through as Swift imports it (`UnsafeRawPointer?`, ...).
    RawPtr(String),
    Str,
    RefAny,
    /// Fieldless enum.
    Enum(String),
    /// `Copy` struct without pointers: a Swift struct over the C value.
    Plain(String),
    /// Tagged union: a Swift enum with associated values.
    Union(String),
    /// Everything else: a `final class`.
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
    /// A callback typedef (C function pointer), passed through.
    Callback(String),
    Unsupported(String),
}

/// One field of a struct or of a union variant.
#[derive(Clone, Debug)]
pub struct Field {
    /// The api.json name.
    pub name: String,
    /// The member name in the C declaration (C++ keywords get a `_`).
    pub c_name: String,
    pub type_name: String,
    pub ref_kind: FieldRefKind,
    pub doc: Option<String>,
}

impl Field {
    fn from_def(f: &FieldDef) -> Field {
        Field {
            name: f.name.clone(),
            c_name: super::c_member_name(&f.name),
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
    /// Tuple payloads are `payload` / `payload_<i>` in C.
    pub fields: Vec<Field>,
    /// Named (struct-like) payload.
    pub named: bool,
    pub doc: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Shape {
    Struct(Vec<Field>),
    Union {
        /// The C variant structs carry `uint8_t tag` (else an `Az{T}_Tag`).
        tag_u8: bool,
        variants: Vec<Variant>,
    },
}

/// What Swift declaration a class gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `final class`.
    Class,
    /// `struct` over the C value.
    Plain,
    /// `enum` with associated values.
    Union,
}

/// Everything the emitter needs to know about one api.json type with fields
/// or variants.
#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub name: String,
    pub module: String,
    pub doc: Vec<String>,
    pub shape: Shape,
    pub traits: TypeTraits,
    pub callback_wrapper: Option<CallbackWrapperInfo>,
    pub category: TypeCategory,
    pub kind: Kind,
}

/// A fieldless enum.
#[derive(Clone, Debug)]
pub struct EnumInfo {
    pub name: String,
    pub module: String,
    pub doc: Vec<String>,
    pub variants: Vec<String>,
}

pub struct Model<'a> {
    pub ir: &'a CodegenIR,
    pub config: &'a CodegenConfig,
    /// Every C symbol a function of an included type has.
    pub funs: BTreeSet<String>,
    pub classes: BTreeMap<String, ClassInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    /// Simple aliases (`GLuint = u32`), name -> target type name.
    pub aliases: BTreeMap<String, String>,
    pub callbacks: BTreeMap<String, &'a CallbackTypedefDef>,
    /// Functions per class, in IR order.
    pub functions: BTreeMap<String, Vec<&'a FunctionDef>>,
    /// Settled `owned` answers.
    cache: RefCell<BTreeMap<String, Ty>>,
    /// Types whose `owned` answer is being computed (a Vec of a union that
    /// holds that Vec asks for itself).
    in_progress: RefCell<BTreeSet<String>>,
    /// How many provisional answers were handed out; an answer that used one
    /// is not cached.
    provisional: Cell<usize>,
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
        let module_of = |n: &str, m: &str| {
            if m.is_empty() {
                ir.type_to_module.get(n).cloned().unwrap_or_default()
            } else {
                m.to_string()
            }
        };

        for s in &ir.structs {
            if !super::include_type(&s.name, &s.generic_params, s.category, config) {
                continue;
            }
            classes.insert(
                s.name.clone(),
                ClassInfo {
                    name: s.name.clone(),
                    module: module_of(&s.name, &s.module),
                    doc: s.doc.clone(),
                    shape: Shape::Struct(s.fields.iter().map(Field::from_def).collect()),
                    traits: s.traits.clone(),
                    callback_wrapper: s.callback_wrapper_info.clone(),
                    category: s.category,
                    kind: Kind::Class,
                },
            );
        }
        for e in &ir.enums {
            if !super::include_type(&e.name, &e.generic_params, e.category, config)
                || classes.contains_key(&e.name)
            {
                continue;
            }
            if !e.is_union {
                enums.insert(
                    e.name.clone(),
                    EnumInfo {
                        name: e.name.clone(),
                        module: module_of(&e.name, &e.module),
                        doc: e.doc.clone(),
                        variants: e.variants.iter().map(|v| v.name.clone()).collect(),
                    },
                );
                continue;
            }
            let variants = e
                .variants
                .iter()
                .enumerate()
                .map(|(index, v)| {
                    let (fields, named) = match &v.kind {
                        EnumVariantKind::Unit => (Vec::new(), false),
                        EnumVariantKind::Tuple(types) => (
                            types
                                .iter()
                                .enumerate()
                                .map(|(j, (ty, rk))| {
                                    let n = if types.len() == 1 {
                                        "payload".to_string()
                                    } else {
                                        format!("payload_{}", j)
                                    };
                                    Field {
                                        c_name: n.clone(),
                                        name: n,
                                        type_name: ty.clone(),
                                        ref_kind: *rk,
                                        doc: None,
                                    }
                                })
                                .collect(),
                            false,
                        ),
                        EnumVariantKind::Struct(fields) => {
                            (fields.iter().map(Field::from_def).collect(), true)
                        }
                    };
                    Variant {
                        name: v.name.clone(),
                        index,
                        fields,
                        named,
                        doc: v.doc.clone(),
                    }
                })
                .collect();
            classes.insert(
                e.name.clone(),
                ClassInfo {
                    name: e.name.clone(),
                    module: module_of(&e.name, &e.module),
                    doc: e.doc.clone(),
                    shape: Shape::Union {
                        tag_u8: repr_is_u8(e.repr.as_deref()),
                        variants,
                    },
                    traits: e.traits.clone(),
                    callback_wrapper: None,
                    category: e.category,
                    kind: Kind::Union,
                },
            );
        }
        for ta in &ir.type_aliases {
            if !config.should_include_type(&ta.name) || classes.contains_key(&ta.name) {
                continue;
            }
            let module = module_of(&ta.name, &ta.module);
            match &ta.monomorphized_def {
                None => {
                    aliases.insert(ta.name.clone(), ta.target.clone());
                }
                Some(mono) => match &mono.kind {
                    MonomorphizedKind::SimpleEnum { variants, .. } => {
                        enums.insert(
                            ta.name.clone(),
                            EnumInfo {
                                name: ta.name.clone(),
                                module,
                                doc: ta.doc.clone(),
                                variants: variants.clone(),
                            },
                        );
                    }
                    MonomorphizedKind::Struct { fields } => {
                        classes.insert(
                            ta.name.clone(),
                            ClassInfo {
                                name: ta.name.clone(),
                                module,
                                doc: ta.doc.clone(),
                                shape: Shape::Struct(fields.iter().map(Field::from_def).collect()),
                                traits: ta.traits.clone(),
                                callback_wrapper: None,
                                category: TypeCategory::Regular,
                                kind: Kind::Class,
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
                                fields: v
                                    .payload_type
                                    .iter()
                                    .map(|p| Field {
                                        name: "payload".to_string(),
                                        c_name: "payload".to_string(),
                                        type_name: p.clone(),
                                        ref_kind: v.payload_ref_kind,
                                        doc: None,
                                    })
                                    .collect(),
                                named: false,
                                doc: None,
                            })
                            .collect();
                        classes.insert(
                            ta.name.clone(),
                            ClassInfo {
                                name: ta.name.clone(),
                                module,
                                doc: ta.doc.clone(),
                                shape: Shape::Union {
                                    tag_u8: repr_is_u8(repr.as_deref()),
                                    variants,
                                },
                                traits: ta.traits.clone(),
                                callback_wrapper: None,
                                category: TypeCategory::Regular,
                                kind: Kind::Union,
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

        let mut m = Model {
            ir,
            config,
            funs,
            classes,
            enums,
            aliases,
            callbacks,
            functions,
            cache: RefCell::new(BTreeMap::new()),
            in_progress: RefCell::new(BTreeSet::new()),
            provisional: Cell::new(0),
        };
        m.classify();
        m.cache.borrow_mut().clear();
        m
    }

    /// Settle `Kind` for every class: plain structs first (a fixpoint, since
    /// plainness is recursive), then the unions a Swift enum cannot hold fall
    /// back to a class.
    fn classify(&mut self) {
        let names: Vec<String> = self.classes.keys().cloned().collect();
        // A tagged union whose payloads Swift cannot hold is a class instead.
        for n in &names {
            if self.classes[n].kind != Kind::Union {
                continue;
            }
            let ok = match &self.classes[n].shape {
                Shape::Union { variants, .. } => {
                    !variants.is_empty()
                        && variants
                            .iter()
                            .all(|v| v.fields.iter().all(|f| self.payload_ok(f)))
                }
                Shape::Struct(_) => false,
            };
            if !ok {
                self.classes.get_mut(n).unwrap().kind = Kind::Class;
            }
        }
        loop {
            let mut changed = false;
            for n in &names {
                let c = &self.classes[n];
                if c.kind != Kind::Class || !c.traits.is_copy {
                    continue;
                }
                let Shape::Struct(fields) = &c.shape else {
                    continue;
                };
                if fields.iter().all(|f| self.plain_field(f)) {
                    self.classes.get_mut(n).unwrap().kind = Kind::Plain;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Can a variant payload field live in a Swift enum case?
    fn payload_ok(&self, f: &Field) -> bool {
        if f.ref_kind != FieldRefKind::Owned {
            return false;
        }
        !matches!(
            self.owned_pre(&f.type_name),
            Ty::Void | Ty::Unsupported(_) | Ty::RawPtr(_)
        )
    }

    /// A field of a plain struct: owned data with no pointer anywhere inside.
    fn plain_field(&self, f: &Field) -> bool {
        if f.ref_kind != FieldRefKind::Owned {
            return false;
        }
        let t = f.type_name.trim();
        if let Some(elem) = array_elem(t) {
            return Prim::from_rust(self.unalias(elem)).is_some();
        }
        self.plain_type(t, 0)
    }

    fn plain_type(&self, t: &str, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        let t = self.unalias(t.trim());
        if Prim::from_rust(t).is_some() || self.enums.contains_key(t) {
            return true;
        }
        let Some(c) = self.classes.get(t) else {
            return false;
        };
        if !c.traits.is_copy {
            return false;
        }
        match (&c.shape, c.kind) {
            (_, Kind::Plain) => true,
            (Shape::Union { variants, .. }, Kind::Union) => variants.iter().all(|v| {
                v.fields.iter().all(|f| {
                    f.ref_kind == FieldRefKind::Owned && self.plain_type(&f.type_name, depth + 1)
                })
            }),
            _ => false,
        }
    }

    /// The C symbol `Az{class}_{suffix}` if it is bound.
    pub fn fun(&self, class: &str, suffix: &str) -> Option<String> {
        let c = format!("Az{}_{}", class, suffix);
        self.funs.contains(&c).then_some(c)
    }

    pub fn functions_of(&self, class: &str) -> &[&'a FunctionDef] {
        self.functions
            .get(class)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
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

    /// Can a borrowed value of this type be turned into an independent one?
    pub fn class_copyable(&self, class: &str) -> bool {
        self.is_copy(class) || self.clone_fn(class).is_some()
    }

    /// Resolve a simple alias chain (`GLuint -> u32`).
    pub fn unalias<'n>(&'n self, mut name: &'n str) -> &'n str {
        for _ in 0..16 {
            match self.aliases.get(name) {
                Some(t) => name = t.as_str(),
                None => break,
            }
        }
        name
    }

    /// The shape of an owned value, before class kinds are final: every
    /// class-like type reads as `Class`.
    fn owned_pre(&self, type_name: &str) -> Ty {
        let t = type_name.trim();
        if let Some(p) = pointer_swift(t, self) {
            return Ty::RawPtr(p);
        }
        if array_elem(t).is_some() {
            return Ty::Unsupported(t.to_string());
        }
        if let Some(p) = Prim::from_rust(t) {
            return Ty::Prim(p);
        }
        let name = self.unalias(t);
        if let Some(p) = pointer_swift(name, self) {
            return Ty::RawPtr(p);
        }
        match name {
            "void" | "c_void" | "()" => return Ty::Void,
            _ => {}
        }
        if let Some(p) = Prim::from_rust(name) {
            return Ty::Prim(p);
        }
        if self.enums.contains_key(name) {
            return Ty::Enum(name.to_string());
        }
        if self.callbacks.contains_key(name) {
            return Ty::Callback(name.to_string());
        }
        if self.classes.contains_key(name) {
            return Ty::Class(name.to_string());
        }
        Ty::Unsupported(name.to_string())
    }

    /// The shape of an owned (by-value) value of `type_name`.
    pub fn owned(&self, type_name: &str) -> Ty {
        let key = type_name.trim().to_string();
        if let Some(t) = self.cache.borrow().get(&key) {
            return t.clone();
        }
        if self.in_progress.borrow().contains(&key) {
            // Asked for itself: answer with the type's own declaration.
            self.provisional.set(self.provisional.get() + 1);
            return match self.owned_pre(&key) {
                Ty::Class(name) => self.declared(&name),
                other => other,
            };
        }
        self.in_progress.borrow_mut().insert(key.clone());
        let before = self.provisional.get();
        let t = self.owned_uncached(&key);
        self.in_progress.borrow_mut().remove(&key);
        if self.provisional.get() == before {
            self.cache.borrow_mut().insert(key, t.clone());
        }
        if self.in_progress.borrow().is_empty() {
            self.provisional.set(0);
        }
        t
    }

    fn owned_uncached(&self, type_name: &str) -> Ty {
        match self.owned_pre(type_name) {
            Ty::Class(name) => {
                if name == "String" {
                    return Ty::Str;
                }
                let class = &self.classes[&name];
                if class.category == TypeCategory::RefAny || name == "RefAny" {
                    return Ty::RefAny;
                }
                if let Some(t) = self.option_shape(class) {
                    return t;
                }
                if let Some(t) = self.vec_shape(class) {
                    return t;
                }
                if let Some(t) = self.result_shape(class) {
                    return t;
                }
                self.declared(&name)
            }
            other => other,
        }
    }

    /// The type's own Swift declaration (never a native container).
    pub fn declared(&self, name: &str) -> Ty {
        if name == "String" {
            return Ty::Str;
        }
        if self.enums.contains_key(name) {
            return Ty::Enum(name.to_string());
        }
        match self.classes.get(name).map(|c| c.kind) {
            Some(Kind::Plain) => Ty::Plain(name.to_string()),
            Some(Kind::Union) => Ty::Union(name.to_string()),
            Some(Kind::Class) if name == "RefAny" => Ty::RefAny,
            Some(Kind::Class) => Ty::Class(name.to_string()),
            None => Ty::Unsupported(name.to_string()),
        }
    }

    /// The shape of a field.
    pub fn field(&self, f: &Field) -> Ty {
        match f.ref_kind {
            FieldRefKind::Owned => self.owned(&f.type_name),
            _ => Ty::Unsupported(f.type_name.clone()),
        }
    }

    /// Is this class exposed through a native Swift type everywhere (so it
    /// gets no declaration of its own)? A `Result` is native only as a return
    /// value and keeps its enum for every other position.
    pub fn is_native(&self, name: &str) -> bool {
        matches!(
            self.owned(name),
            Ty::Str | Ty::Option { .. } | Ty::Vec { .. }
        )
    }

    fn option_shape(&self, class: &ClassInfo) -> Option<Ty> {
        let Shape::Union { variants, .. } = &class.shape else {
            return None;
        };
        if variants.len() != 2 || class.kind != Kind::Union {
            return None;
        }
        let none = variants.iter().find(|v| v.name == "None")?;
        let some = variants.iter().find(|v| v.name == "Some")?;
        if !none.fields.is_empty() || some.fields.len() != 1 || some.named {
            return None;
        }
        if some.fields[0].ref_kind != FieldRefKind::Owned {
            return None;
        }
        let payload = self.not_result(self.owned(&some.fields[0].type_name));
        if !self.convertible(&payload) || matches!(payload, Ty::Option { .. }) {
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
        let elem = self.not_result(self.owned(&ptr.type_name));
        // Reading elements out of the vec needs an independent copy of each.
        // Reading elements out needs an independent copy of each; writing
        // them in needs a copy that leaves the caller's objects usable
        // (`_copyFromPtr` clones what it is given).
        let ok = match &elem {
            Ty::Prim(_) | Ty::Enum(_) | Ty::Str | Ty::Plain(_) | Ty::RefAny => true,
            Ty::Class(c) => self.class_copyable(c),
            Ty::Union(c) => self.class_copyable(c),
            Ty::Option { .. } | Ty::Vec { .. } => {
                self.convertible(&elem) && self.borrow_expr(&elem, "x").is_some()
            }
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
        if variants.len() != 2 || !class.name.starts_with("Result") || class.kind != Kind::Union {
            return None;
        }
        let ok = variants.iter().find(|v| v.name == "Ok")?;
        let err = variants.iter().find(|v| v.name == "Err")?;
        if ok.fields.len() != 1 || err.fields.len() != 1 || ok.named || err.named {
            return None;
        }
        if ok.fields[0].ref_kind != FieldRefKind::Owned
            || err.fields[0].ref_kind != FieldRefKind::Owned
        {
            return None;
        }
        let okt = self.not_result(self.owned(&ok.fields[0].type_name));
        let errt = self.not_result(self.owned(&err.fields[0].type_name));
        if !self.convertible(&okt) || !self.convertible(&errt) {
            return None;
        }
        Some(Ty::Result {
            name: class.name.clone(),
            ok: Box::new(okt),
            err: Box::new(errt),
        })
    }

    /// A `Result` is native only as a RETURN value; everywhere else it is its
    /// own Swift enum.
    pub fn not_result(&self, t: Ty) -> Ty {
        match t {
            Ty::Result { name, .. } => self.declared(&name),
            other => other,
        }
    }

    /// Can a value of this shape cross the boundary in both directions
    /// (native -> owned C value, and borrowed C value -> native copy)?
    pub fn convertible(&self, t: &Ty) -> bool {
        match t {
            Ty::Prim(_) | Ty::Str | Ty::RefAny | Ty::Enum(_) | Ty::Plain(_) => true,
            Ty::Class(c) | Ty::Union(c) => self.class_copyable(c),
            Ty::Option { .. } | Ty::Vec { .. } => true,
            Ty::Result { .. } => false,
            Ty::Void | Ty::RawPtr(_) | Ty::Callback(_) | Ty::Unsupported(_) => false,
        }
    }

    /// Does converting a native value of this shape into an owned C value
    /// consume (move) some wrapper object the caller still holds?
    pub fn moves(&self, t: &Ty) -> bool {
        match t {
            Ty::Class(c) => !self.is_copy(c),
            Ty::Union(c) => self.union_moves(c, 0),
            Ty::Option { payload, .. } => self.moves(payload),
            _ => false,
        }
    }

    fn union_moves(&self, name: &str, depth: usize) -> bool {
        if depth > 32 {
            return true;
        }
        let Some(Shape::Union { variants, .. }) = self.classes.get(name).map(|c| &c.shape) else {
            return false;
        };
        variants.iter().any(|v| {
            v.fields.iter().any(|f| match self.field(f) {
                Ty::Union(n) => self.union_moves(&n, depth + 1),
                Ty::Option { payload, .. } => match *payload {
                    Ty::Union(n) => self.union_moves(&n, depth + 1),
                    other => self.moves(&other),
                },
                other => self.moves(&other),
            })
        })
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

fn repr_is_u8(repr: Option<&str>) -> bool {
    repr.is_some_and(|r| r.contains("u8"))
}

/// `[T; N]` -> `T`.
pub fn array_elem(t: &str) -> Option<&str> {
    let t = t.trim();
    if t.starts_with('[') && t.ends_with(']') {
        let inner = &t[1..t.len() - 1];
        if let Some(semi) = inner.rfind(';') {
            if inner[semi + 1..].trim().parse::<usize>().is_ok() {
                return Some(inner[..semi].trim());
            }
        }
    }
    None
}

/// How Swift imports a pointer TYPE NAME (`*const c_void` -> `UnsafeRawPointer?`).
/// Only one level of pointer to void, a primitive or an azul type is spelled.
pub fn pointer_swift(t: &str, m: &Model) -> Option<String> {
    let t = t.trim();
    let (mutable, inner) = if let Some(r) = t.strip_prefix("*const ") {
        (false, r)
    } else if let Some(r) = t.strip_prefix("*mut ") {
        (true, r)
    } else {
        return None;
    };
    Some(pointee_swift(inner.trim(), mutable, m)?)
}

/// The Swift spelling of a pointer to `inner`.
pub fn pointee_swift(inner: &str, mutable: bool, m: &Model) -> Option<String> {
    let inner = inner.trim();
    if matches!(inner, "c_void" | "void" | "()") {
        return Some(if mutable {
            "UnsafeMutableRawPointer?".to_string()
        } else {
            "UnsafeRawPointer?".to_string()
        });
    }
    if inner.starts_with('*') {
        return None;
    }
    let base = if let Some(p) = Prim::from_rust(inner) {
        p.swift().to_string()
    } else if m.classes.contains_key(inner)
        || m.enums.contains_key(inner)
        || m.aliases.contains_key(inner)
        || m.callbacks.contains_key(inner)
    {
        if let Some(p) = Prim::from_rust(m.unalias(inner)) {
            p.swift().to_string()
        } else if m.aliases.contains_key(inner) {
            return None;
        } else {
            format!("Az{}", inner)
        }
    } else {
        return None;
    };
    Some(if mutable {
        format!("UnsafeMutablePointer<{}>?", base)
    } else {
        format!("UnsafePointer<{}>?", base)
    })
}
