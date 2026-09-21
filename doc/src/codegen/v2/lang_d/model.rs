//! What every IR type becomes in D.
//!
//! The raw layer (`types.rs`, `functions.rs`) declares the C ABI one to one
//! (`struct AzDom`, `extern(C) AzDom AzDom_createBody()`). This module decides
//! the SHAPE each api.json type has on the idiomatic side, derived from the IR's
//! structure rather than from class names:
//!
//! | IR shape                                     | D                                        |
//! |----------------------------------------------|------------------------------------------|
//! | `String`                                     | `string`                                 |
//! | `RefAny`                                     | any class object / `RefAny`              |
//! | union `None` + `Some(T)`                     | `Nullable!T`                             |
//! | struct `{ptr, len, cap, destructor}` Vec     | `T[]` (`ubyte[]` for `U8Vec`)            |
//! | union `Ok(T)` + `Err(E)` (return position)   | `T`, throwing `ResultException!E`        |
//! | fieldless enum                               | D `enum` (`ButtonType.primary`)          |
//! | `Copy` struct or tagged union                | `struct` holding the C value (a copy)    |
//! | any other struct or tagged union             | `struct` handle (refcounted, `_delete`)  |
//!
//! A tagged union is a struct with a nested `Tag` enum, so `final switch
//! (value.tag)` matches it exhaustively, plus a static factory and an accessor
//! per variant. A container is only mapped when every conversion it needs
//! exists (a Vec needs `_copyFromPtr`, an element read out of borrowed memory
//! needs Copy or `_clone`); otherwise that position keeps the type's own struct.

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
    ir_builder::variant_constructor_method_name,
};

/// A primitive, spelled the way the raw layer declares it.
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
    CLong,
    CULong,
}

impl Prim {
    pub fn from_rust(t: &str) -> Option<Prim> {
        // The C spellings of the machine primitives; api.json declares the GL
        // ones as aliases of a Rust primitive, so each is another spelling of
        // one table entry, not a decision about one API type.
        Some(match t.trim() { // allow-api-name: a primitive-spelling table
            // `GLboolean` is `uint8_t` in azul.h; D's `bool` has the same size.
            "bool" | "GLboolean" => Prim::Bool,
            "u8" | "c_uchar" => Prim::U8,
            "i8" | "c_char" => Prim::I8,
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
            "usize" | "size_t" | "uintptr_t" => Prim::USize,
            "isize" | "ssize_t" | "intptr_t" | "GLsizeiptr" | "GLintptr" => Prim::ISize,
            _ => return None,
        })
    }

    pub fn d(self) -> &'static str {
        match self {
            Prim::Bool => "bool",
            Prim::U8 => "ubyte",
            Prim::I8 => "byte",
            Prim::U16 => "ushort",
            Prim::I16 => "short",
            Prim::U32 => "uint",
            Prim::I32 => "int",
            Prim::U64 => "ulong",
            Prim::I64 => "long",
            Prim::F32 => "float",
            Prim::F64 => "double",
            Prim::USize => "size_t",
            Prim::ISize => "ptrdiff_t",
            // `core.stdc.config`: 4 bytes on Windows, 8 on LP64.
            Prim::CLong => "c_long",
            Prim::CULong => "c_ulong",
        }
    }
}

/// The D-side shape of one type in one position.
#[derive(Clone, Debug)]
pub enum Ty {
    Void,
    Prim(Prim),
    /// A pointer, passed through as the raw layer spells it (`AzDom*`, `void*`).
    RawPtr(String),
    Str,
    RefAny,
    /// Fieldless enum.
    Enum(String),
    /// `Copy` struct or union: a D struct over the C value.
    Plain(String),
    /// Everything else: a refcounted handle struct.
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
    /// The member name in the raw D declaration (keywords get a `_`).
    pub c_name: String,
    pub type_name: String,
    pub ref_kind: FieldRefKind,
    pub doc: Option<String>,
}

impl Field {
    fn from_def(f: &FieldDef) -> Field {
        Field {
            name: f.name.clone(),
            c_name: super::raw_identifier(&f.name),
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
    /// The raw union member (and `Az{T}Variant_{member}` struct) name.
    pub member: String,
    pub index: usize,
    /// Tuple payloads are `payload` / `payload_<i>` in the raw layer.
    pub fields: Vec<Field>,
    pub doc: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Shape {
    Struct(Vec<Field>),
    Union {
        /// The raw `tag` field's type (`ubyte` for `#[repr(C, u8)]`).
        tag: &'static str,
        variants: Vec<Variant>,
    },
}

/// What D declaration a class gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Refcounted handle struct.
    Class,
    /// Copyable struct over the C value.
    Plain,
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
    /// The raw enum's backing integer.
    pub backing: &'static str,
}

pub struct Model<'a> {
    pub ir: &'a CodegenIR,
    pub config: &'a CodegenConfig,
    /// Every C symbol the raw layer declares.
    pub funs: BTreeSet<String>,
    pub classes: BTreeMap<String, ClassInfo>,
    pub enums: BTreeMap<String, EnumInfo>,
    /// Simple aliases (`GLuint = u32`), name -> target type name.
    pub aliases: BTreeMap<String, String>,
    pub callbacks: BTreeMap<String, &'a CallbackTypedefDef>,
    /// Functions per class, in IR order.
    pub functions: BTreeMap<String, Vec<&'a FunctionDef>>,
    /// The `Option<RefAny>` of the API, by shape: a callback wrapper's context
    /// slot. Resolved once, because the emitter asks for it per method.
    option_refany: Option<String>,
    /// Every `TypeCategory` exactly one class carries, with that class. The
    /// IR singles the Rust string and the opaque callback data out that way,
    /// and the emitter needs their C symbols (`_delete`) by name.
    sole_of_category: Vec<(TypeCategory, String)>,
    cache: RefCell<BTreeMap<String, Ty>>,
    in_progress: RefCell<BTreeSet<String>>,
    provisional: Cell<usize>,
}

impl<'a> Model<'a> {
    pub fn new(ir: &'a CodegenIR, config: &'a CodegenConfig) -> Model<'a> {
        let mut funs = BTreeSet::new();
        let mut functions: BTreeMap<String, Vec<&FunctionDef>> = BTreeMap::new();
        for f in &ir.functions {
            if super::should_emit_function(f, ir, config) {
                if funs.insert(f.c_name.clone()) {
                    functions.entry(f.class_name.clone()).or_default().push(f);
                }
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
            if !super::include_struct(s, config) || classes.contains_key(&s.name) {
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
            if !super::include_enum(e, config)
                || classes.contains_key(&e.name)
                || enums.contains_key(&e.name)
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
                        backing: super::types::enum_backing(e.repr.as_deref()),
                    },
                );
                continue;
            }
            let variants = e
                .variants
                .iter()
                .enumerate()
                .map(|(index, v)| {
                    let fields = match &v.kind {
                        EnumVariantKind::Unit => Vec::new(),
                        EnumVariantKind::Tuple(types) => types
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
                        EnumVariantKind::Struct(fields) => {
                            fields.iter().map(Field::from_def).collect()
                        }
                    };
                    Variant {
                        name: v.name.clone(),
                        member: super::raw_identifier(&v.name),
                        index,
                        fields,
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
                        tag: super::types::tag_type(e.repr.as_deref()),
                        variants,
                    },
                    traits: e.traits.clone(),
                    callback_wrapper: None,
                    category: e.category,
                    kind: Kind::Class,
                },
            );
        }
        for ta in &ir.type_aliases {
            if !config.should_include_type(&ta.name)
                || classes.contains_key(&ta.name)
                || enums.contains_key(&ta.name)
            {
                continue;
            }
            let module = module_of(&ta.name, &ta.module);
            match &ta.monomorphized_def {
                None => {
                    aliases.insert(ta.name.clone(), ta.target.clone());
                }
                Some(mono) => match &mono.kind {
                    MonomorphizedKind::SimpleEnum { repr, variants } => {
                        enums.insert(
                            ta.name.clone(),
                            EnumInfo {
                                name: ta.name.clone(),
                                module,
                                doc: ta.doc.clone(),
                                variants: variants.clone(),
                                backing: super::types::enum_backing(repr.as_deref()),
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
                                member: super::raw_identifier(&v.name),
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
                                    tag: super::types::tag_type(repr.as_deref()),
                                    variants,
                                },
                                traits: ta.traits.clone(),
                                callback_wrapper: None,
                                category: TypeCategory::Regular,
                                kind: Kind::Class,
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

        let mut per_category: Vec<(TypeCategory, String, usize)> = Vec::new();
        for c in classes.values() {
            match per_category.iter_mut().find(|(k, _, _)| *k == c.category) {
                Some(seen) => seen.2 += 1,
                None => per_category.push((c.category, c.name.clone(), 1)),
            }
        }
        let sole_of_category = per_category
            .into_iter()
            .filter(|(_, _, n)| *n == 1)
            .map(|(k, name, _)| (k, name))
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
            option_refany: None,
            sole_of_category,
            cache: RefCell::new(BTreeMap::new()),
            in_progress: RefCell::new(BTreeSet::new()),
            provisional: Cell::new(0),
        };
        m.classify();
        // `Kind` decides `Ty`, so nothing cached before `classify` is valid.
        m.cache.borrow_mut().clear();
        // Resolved once: the emitter asks for the callback context type per
        // method, and answering it walks every class.
        let option_refany = m
            .classes
            .keys()
            .find(|n| match m.owned(n.as_str()) {
                Ty::Option { payload, .. } => matches!(*payload, Ty::RefAny),
                _ => false,
            })
            .cloned();
        m.option_refany = option_refany;
        m
    }

    /// Settle `Kind`: a `Copy` type whose fields are all plain data (a
    /// fixpoint, since plainness is recursive) is a plain struct; everything
    /// else is a handle.
    fn classify(&mut self) {
        let names: Vec<String> = self.classes.keys().cloned().collect();
        loop {
            let mut changed = false;
            for n in &names {
                let c = &self.classes[n];
                if c.kind != Kind::Class || !c.traits.is_copy {
                    continue;
                }
                let ok = match &c.shape {
                    Shape::Struct(fields) => fields.iter().all(|f| self.plain_field(f)),
                    Shape::Union { variants, .. } => {
                        !variants.is_empty()
                            && variants
                                .iter()
                                .all(|v| v.fields.iter().all(|f| self.plain_field(f)))
                    }
                };
                if ok {
                    self.classes.get_mut(n).unwrap().kind = Kind::Plain;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// A field of a plain type: copying its bytes is a real copy.
    fn plain_field(&self, f: &Field) -> bool {
        match f.ref_kind {
            FieldRefKind::Boxed | FieldRefKind::OptionBoxed => false,
            FieldRefKind::Ref | FieldRefKind::RefMut | FieldRefKind::Ptr | FieldRefKind::PtrMut => {
                true
            }
            FieldRefKind::Owned => {
                let t = f.type_name.trim();
                if let Some(elem) = array_elem(t) {
                    return Prim::from_rust(self.unalias(elem)).is_some();
                }
                if t.starts_with("*const ") || t.starts_with("*mut ") {
                    return true;
                }
                let t = self.unalias(t);
                Prim::from_rust(t).is_some()
                    || self.enums.contains_key(t)
                    || self.callbacks.contains_key(t)
                    || self.classes.get(t).is_some_and(|c| c.kind == Kind::Plain)
            }
        }
    }

    /// The C symbol `Az{class}_{suffix}` if the raw layer declares it.
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

    /// The trait entry point of `kind` for `class`, if declared.
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

    /// The shape of an owned value, before containers are recognized.
    fn owned_pre(&self, type_name: &str) -> Ty {
        let t = type_name.trim();
        if t.starts_with("*const ") || t.starts_with("*mut ") {
            return Ty::RawPtr(super::map_type_to_d(t, self.ir));
        }
        if array_elem(t).is_some() {
            return Ty::Unsupported(t.to_string());
        }
        if let Some(p) = Prim::from_rust(t) {
            return Ty::Prim(p);
        }
        let name = self.unalias(t);
        if name.starts_with("*const ") || name.starts_with("*mut ") {
            return Ty::RawPtr(super::map_type_to_d(name, self.ir));
        }
        if matches!(name, "void" | "c_void" | "()") {
            return Ty::Void;
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
            // Asked for itself (a Vec of a union that holds that Vec): answer
            // with the type's own declaration.
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
                let class = &self.classes[&name];
                // The two types the IR itself singles out by category: the
                // Rust string and the opaque callback data.
                match class.category {
                    TypeCategory::String => return Ty::Str,
                    TypeCategory::RefAny => return Ty::RefAny,
                    _ => {}
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

    /// The type's own D declaration (never a native container).
    pub fn declared(&self, name: &str) -> Ty {
        if self.enums.contains_key(name) {
            return Ty::Enum(name.to_string());
        }
        match self.classes.get(name) {
            Some(c) if c.category == TypeCategory::String => Ty::Str,
            Some(c) if c.category == TypeCategory::RefAny => Ty::RefAny,
            Some(c) if c.kind == Kind::Plain => Ty::Plain(name.to_string()),
            Some(_) => Ty::Class(name.to_string()),
            None => Ty::Unsupported(name.to_string()),
        }
    }

    /// The api.json class the IR gives `category`, if exactly that one type
    /// carries it: how the emitter names the `String` and `RefAny` types
    /// without spelling them out.
    pub fn class_of_category(&self, category: TypeCategory) -> Option<&str> {
        self.sole_of_category
            .iter()
            .find(|(k, _)| *k == category)
            .map(|(_, name)| name.as_str())
    }

    /// The `Option<RefAny>` of the API: the slot a callback wrapper carries
    /// its context in, and the only Option the callback plumbing needs by
    /// itself. Found by shape, not by name.
    pub fn option_of_refany(&self) -> Option<&str> {
        self.option_refany.as_deref()
    }

    /// The exported constructor of one variant of a tagged union.
    ///
    /// `ir_builder` reserves the C name `Az{class}_{method}` for the variant
    /// and synthesises the function only when api.json has not already
    /// declared one under that exact name - in which case THAT function is the
    /// variant's constructor, and going through it runs whatever the Rust
    /// associated function does on the way (`HttpError::connection_failed`,
    /// `PixelValueOrSystem::value`). So match the reserved name and the shape
    /// the job requires, not the `FunctionKind` the IR ended up giving it.
    pub fn variant_fn(&self, class: &str, variant: &Variant) -> Option<&'a FunctionDef> {
        let c_name = format!(
            "Az{}_{}",
            class,
            variant_constructor_method_name(&variant.name)
        );
        let f = self
            .functions_of(class)
            .iter()
            .copied()
            .find(|f| f.c_name == c_name && f.return_type.as_deref() == Some(class))?;
        // It builds this variant only if it takes exactly its payload:
        // api.json may declare a differently shaped factory under the reserved
        // name (`HttpError::http_status(code, message)` for a one-field
        // variant), and that one is an ordinary static method instead.
        if f.args.len() != variant.fields.len() {
            return None;
        }
        f.args
            .iter()
            .zip(&variant.fields)
            .all(|(a, field)| {
                a.type_name.trim() == field.type_name.trim()
                    && (a.ref_kind == ArgRefKind::Owned) == (field.ref_kind == FieldRefKind::Owned)
            })
            .then_some(f)
    }

    /// The shape of a field.
    pub fn field(&self, f: &Field) -> Ty {
        match f.ref_kind {
            FieldRefKind::Owned => self.owned(&f.type_name),
            FieldRefKind::Ptr | FieldRefKind::PtrMut | FieldRefKind::Ref | FieldRefKind::RefMut => {
                Ty::RawPtr(super::field_type_for_ref_kind(
                    &f.type_name,
                    &f.ref_kind,
                    self.ir,
                ))
            }
            _ => Ty::Unsupported(f.type_name.clone()),
        }
    }

    /// Does this class cross every member boundary as a native D type
    /// (`string`, `Nullable!T`, `T[]`)? A `Result` is native only as a return
    /// value. The type still gets its own declaration (see `wrappers`), except
    /// for the string, which D already has.
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
        // Reading elements out needs an independent copy of each; writing them
        // in borrows them (`_copyFromPtr` clones what it is given).
        let ok = match &elem {
            Ty::Prim(_) | Ty::Enum(_) | Ty::Str | Ty::Plain(_) | Ty::RefAny => true,
            Ty::Class(c) => self.class_copyable(c),
            Ty::Option { .. } | Ty::Vec { .. } => {
                self.convertible(&elem) && self.borrow_supported(&elem)
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

    fn borrow_supported(&self, t: &Ty) -> bool {
        match t {
            Ty::Option { payload, .. } => self.convertible(payload),
            Ty::Vec { .. } => true,
            _ => true,
        }
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
    /// own struct.
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
            Ty::Class(c) => self.class_copyable(c),
            Ty::Option { .. } | Ty::Vec { .. } => true,
            Ty::Result { .. } => false,
            Ty::Void | Ty::RawPtr(_) | Ty::Callback(_) | Ty::Unsupported(_) => false,
        }
    }

    /// Does converting a native value of this shape into an owned C value
    /// consume (move) a handle the caller still holds?
    pub fn moves(&self, t: &Ty) -> bool {
        match t {
            Ty::Class(c) => !self.is_copy(c),
            Ty::Option { payload, .. } => self.moves(payload),
            _ => false,
        }
    }

    /// The context accessor of an info type, if it has one: the borrowing
    /// no-argument method that hands back the callback's `Option<RefAny>`.
    /// That is where a D function stored in a callback's ctx is read back.
    pub fn ctx_getter(&self, info_type: &str) -> Option<String> {
        let ctx = self.option_of_refany()?;
        self.functions_of(info_type)
            .iter()
            .find(|f| {
                f.args.len() == 1
                    && matches!(f.args[0].ref_kind, ArgRefKind::Ref | ArgRefKind::Ptr)
                    && f.return_type.as_deref() == Some(ctx)
            })
            .map(|f| f.c_name.clone())
    }
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
