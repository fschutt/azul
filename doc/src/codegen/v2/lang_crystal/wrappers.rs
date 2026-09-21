//! The idiomatic `Azul::` layer: one wrapper class per api.json struct or
//! tagged union, Crystal enums for fieldless enums, and native Crystal values
//! (`String`, `T?`, `Array(T)`, exceptions) at every method boundary the IR
//! shape allows. See `model.rs` for the type mapping and `runtime.rs` for the
//! ownership base class.
//!
//! # Ownership, in one paragraph
//!
//! A wrapper owns its value (finalizer calls `_delete`) or is a view into a
//! field of an owner. Passing a non-Copy wrapper BY VALUE moves it, exactly
//! like Rust: the owner is marked moved and later use raises
//! `Azul::MovedError`; a view is cloned instead, since a field cannot be moved
//! out of. A `with_*` builder that consumes `self` and returns `Self` rebuilds
//! the value in place and returns `self`, so chains read naturally. Values
//! passed BY REFERENCE are borrowed (`to_unsafe`). Anything converted from a
//! native value (a `String`, an `Array`) is a fresh FFI value the call either
//! consumes or the wrapper frees right after it.
//!
//! # Callbacks
//!
//! A callback argument accepts a Crystal `Proc` (or a block): a closure that
//! may capture anything. The C side is always a non-capturing trampoline per
//! callback typedef (`Azul::Trampolines`); the closure travels in a `RefAny`
//! handle, either in the callback's `ctx` (read back through the info type's
//! `get_ctx`) or in the data `RefAny` passed next to it. The data argument
//! itself is any Crystal object, handed back to the closure with its static
//! type, so application state is an ordinary Crystal class.
//!
//! # Crystal 1.21 codegen rule
//!
//! Every value handed to a `LibAzul` fun is first bound to a local. Passing a
//! struct returned by a call straight into another call's argument list is
//! miscompiled by Crystal 1.21 (either an LLVM "instruction in another
//! function" validation error or, silently, a corrupt value).

use std::collections::{BTreeMap, BTreeSet};

use super::{
    super::{
        ir::{ArgRefKind, CallbackTypedefDef, FieldRefKind, FunctionDef, FunctionKind, TypeCategory},
        managed_host_invoker::{
            callback_ctx_field, has_callback_wrapper_arg, layout_callback_factory_info,
        },
    },
    enum_member_name,
    model::{ClassInfo, Field, Model, Prim, Shape, Ty, Variant},
    sanitize_identifier,
};

// ============================================================================
// Small text helpers
// ============================================================================

/// Accumulates Crystal source with explicit indentation.
pub struct W {
    pub out: String,
}

impl Default for W {
    fn default() -> Self {
        Self::new()
    }
}

impl W {
    pub fn new() -> W {
        W { out: String::new() }
    }

    fn l(&mut self, depth: usize, text: &str) {
        if !text.is_empty() {
            for _ in 0..depth {
                self.out.push_str("  ");
            }
            self.out.push_str(text);
        }
        self.out.push('\n');
    }

    fn doc(&mut self, depth: usize, lines: &[String]) {
        for d in lines {
            for part in d.split('\n') {
                let part = part.trim_end();
                if part.is_empty() {
                    self.l(depth, "#");
                } else {
                    self.l(depth, &format!("# {}", part.replace('\r', "")));
                }
            }
        }
    }
}

/// `LibAzul`-scope type spelling -> fully qualified spelling usable outside
/// the lib block (`AzDom*` -> `::Pointer(LibAzul::AzDom)`).
pub fn qualify(t: &str) -> String {
    let t = t.trim();
    if let Some(inner) = t.strip_suffix('*') {
        return format!("::Pointer({})", qualify(inner));
    }
    if t.ends_with(']') {
        if let Some(open) = t.rfind('[') {
            return format!(
                "::StaticArray({}, {})",
                qualify(&t[..open]),
                &t[open + 1..t.len() - 1]
            );
        }
    }
    if t.starts_with("Az") {
        return format!("LibAzul::{}", t);
    }
    format!("::{}", t)
}

fn cls(name: &str) -> String {
    format!("Azul::{}", name)
}

fn lib(name: &str) -> String {
    format!("LibAzul::Az{}", name)
}

fn fun(c_name: &str) -> String {
    format!("LibAzul.{}", super::crystal_fun_name(c_name))
}

fn snake(name: &str) -> String {
    // `mouseUp` (variant constructor) and `CreateBody` style -> snake_case.
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i != 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Method names a wrapper must not define (Object's contract, Crystal
/// pseudo-methods, and the runtime's own hooks).
fn reserved_method(name: &str) -> bool {
    matches!(
        name,
        "as" | "as?"
            | "is_a?"
            | "nil?"
            | "responds_to?"
            | "class"
            | "hash"
            | "clone"
            | "dup"
            | "to_s"
            | "inspect"
            | "to_unsafe"
            | "finalize"
            | "initialize"
            | "allocate"
            | "object_id"
            | "same?"
            | "not_nil!"
            | "try"
            | "tap"
            | "itself"
            | "crystal_type_id"
            | "unsafe_as"
            | "in?"
            | "default"
            | "pretty_print"
    ) || name.starts_with("__")
}

// ============================================================================
// Conversions
// ============================================================================

impl<'a> Model<'a> {
    /// Result is native only as a RETURN value; everywhere else it is its
    /// wrapper class.
    fn not_result(&self, t: Ty) -> Ty {
        match t {
            Ty::Result { name, .. } => Ty::Class(name),
            other => other,
        }
    }

    /// The qualified lib type of an owned value of `type_name`.
    fn lib_owned(&self, type_name: &str) -> String {
        qualify(&super::map_type_to_crystal(type_name, self.ir))
    }

    /// Native value `x` -> owned FFI value, bound to `out`.
    fn in_setup(&self, t: &Ty, x: &str, out: &str) -> Option<String> {
        Some(match t {
            Ty::Prim(p) => format!("{} = {}", out, p.convert(x)),
            Ty::Enum(_) | Ty::Callback(_) | Ty::RawPtr(_) => format!("{} = {}", out, x),
            Ty::Str => format!("{} = Azul::Native.az_string({})", out, x),
            Ty::RefAny => format!("{} = Azul::Native.refany({})", out, x),
            Ty::Class(_) | Ty::Result { .. } => format!("{} = {}.__take", out, x),
            Ty::Option { name, .. } | Ty::Vec { name, .. } => {
                format!("{} = Azul::Conv.in_{}({})", out, name, x)
            }
            Ty::Void | Ty::Unsupported(_) => return None,
        })
    }

    /// Owned FFI value `v` (a local) -> native value.
    fn take_expr(&self, t: &Ty, v: &str) -> Option<String> {
        Some(match t {
            Ty::Prim(_) | Ty::Enum(_) | Ty::Callback(_) | Ty::RawPtr(_) => v.to_string(),
            Ty::Str => format!("Azul::Native.take_string({})", v),
            Ty::RefAny => format!("Azul::RefAny.__own({})", v),
            Ty::Class(n) => format!("{}.__own({})", cls(n), v),
            Ty::Option { name, .. } | Ty::Vec { name, .. } | Ty::Result { name, .. } => {
                format!("Azul::Conv.take_{}({})", name, v)
            }
            Ty::Void | Ty::Unsupported(_) => return None,
        })
    }

    /// Borrowed FFI value behind pointer `p` -> independent native value.
    fn out_expr(&self, t: &Ty, p: &str) -> Option<String> {
        Some(match t {
            Ty::Prim(_) | Ty::Enum(_) | Ty::Callback(_) | Ty::RawPtr(_) => format!("{}.value", p),
            Ty::Str => format!("Azul::Native.string({})", p),
            Ty::RefAny => format!("Azul::RefAny.__copy_from({})", p),
            Ty::Class(n) if self.class_copyable(n) => format!("{}.__copy_from({})", cls(n), p),
            Ty::Option { name, .. } | Ty::Vec { name, .. } => {
                format!("Azul::Conv.out_{}({})", name, p)
            }
            _ => return None,
        })
    }

    /// After a fresh FFI value was copied (by `_copyFromPtr`) or only
    /// borrowed by a call, free it.
    fn cleanup(&self, t: &Ty, ptr: &str) -> Option<String> {
        // allow-api-name: `Ty::Str`/`Ty::RefAny` ARE those two API classes -
        // this maps the shape back to the class whose `_delete` frees it.
        let name = match t {
            Ty::Str => "String",
            Ty::RefAny => "RefAny",
            Ty::Option { name, .. } | Ty::Vec { name, .. } => name.as_str(),
            _ => return None,
        };
        self.fun(name, "delete")
            .map(|f| format!("{}({})", fun(&f), ptr))
    }

    /// Type of a returned value: `*const T` / `&T` returns stay raw pointers.
    fn ret_ty(&self, ret: Option<&str>, own_class: &str) -> Ty {
        let Some(r) = ret else { return Ty::Void };
        let r = r.trim();
        for prefix in ["*const ", "*mut ", "&mut ", "&"] {
            if let Some(inner) = r.strip_prefix(prefix) {
                return Ty::RawPtr(qualify(&super::ptr_to_crystal(inner, self.ir)));
            }
        }
        if r == own_class && self.classes.contains_key(r) {
            return Ty::Class(r.to_string());
        }
        self.owned(r)
    }
}

/// What an argument accepts.
fn restriction(t: &Ty) -> Option<String> {
    Some(match t {
        Ty::Prim(p) => p.restriction().to_string(),
        Ty::Str => "::String".to_string(),
        Ty::RefAny => "::Reference".to_string(),
        Ty::Enum(n) | Ty::Class(n) | Ty::Result { name: n, .. } => cls(n),
        Ty::Option { payload, .. } => format!("({} | ::Nil)", restriction(payload)?),
        Ty::Vec { elem, .. } => match elem.as_ref() {
            Ty::Prim(Prim::U8) => "::Bytes".to_string(),
            Ty::Str => "::Enumerable(::String)".to_string(),
            Ty::Enum(n) | Ty::Class(n) => format!("::Enumerable({})", cls(n)),
            _ => "::Enumerable".to_string(),
        },
        Ty::Callback(td) => lib(td),
        Ty::RawPtr(p) => p.clone(),
        Ty::Void | Ty::Unsupported(_) => return None,
    })
}

/// The exact Crystal type of a returned / read value.
fn exact(t: &Ty) -> Option<String> {
    Some(match t {
        Ty::Void => "::Nil".to_string(),
        Ty::Prim(p) => p.crystal().to_string(),
        Ty::Str => "::String".to_string(),
        // allow-api-name: `Ty::RefAny` is the RefAny class by definition.
        Ty::RefAny => cls("RefAny"),
        Ty::Enum(n) | Ty::Class(n) => cls(n),
        Ty::Option { payload, .. } => format!("({} | ::Nil)", exact(payload)?),
        Ty::Vec { elem, .. } => match elem.as_ref() {
            Ty::Prim(Prim::U8) => "::Bytes".to_string(),
            other => format!("::Array({})", exact(other)?),
        },
        Ty::Result { ok, .. } => exact(ok)?,
        Ty::Callback(td) => lib(td),
        Ty::RawPtr(p) => p.clone(),
        Ty::Unsupported(_) => return None,
    })
}


// ============================================================================
// Top level
// ============================================================================

pub struct Emitter<'m, 'a> {
    m: &'m Model<'a>,
    /// Callback typedefs some registration dispatches through.
    trampolines: BTreeSet<String>,
    /// C symbols of api.json functions with no Crystal-side shape.
    pub skipped: Vec<String>,
}

pub fn generate(m: &Model, api_version: &str) -> (String, Vec<String>) {
    let mut e = Emitter {
        m,
        trampolines: BTreeSet::new(),
        skipped: Vec::new(),
    };
    let mut w = W::new();
    e.emit_aliases(&mut w, api_version);
    e.emit_enums(&mut w);
    for class in m.classes.values() {
        e.emit_class(&mut w, class);
    }
    e.emit_conversions(&mut w);
    e.emit_trampolines(&mut w);
    let skipped = e.skipped;
    (w.out, skipped)
}

impl<'m, 'a> Emitter<'m, 'a> {
    fn emit_aliases(&mut self, w: &mut W, api_version: &str) {
        let m = self.m;
        w.l(0, "# ----------------------------------------------------------------------------");
        w.l(0, "# Idiomatic layer: Azul::* classes over the lib layer.");
        w.l(0, "# ----------------------------------------------------------------------------");
        w.l(0, "");
        w.l(0, "module Azul");
        w.l(1, &format!("VERSION = \"{}\"", api_version));
        w.l(0, "");
        for (name, variants) in &m.enums {
            if variants.is_empty() {
                w.l(1, &format!("alias {} = ::{}", name, "Int32"));
            } else {
                w.l(1, &format!("alias {} = {}", name, lib(name)));
            }
        }
        for name in m.callbacks.keys() {
            if !m.classes.contains_key(name) && !m.enums.contains_key(name) {
                w.l(1, &format!("alias {} = {}", name, lib(name)));
            }
        }
        for name in m.aliases.keys() {
            if m.classes.contains_key(name) || m.enums.contains_key(name) {
                continue;
            }
            let t = m.owned(name);
            let target = match &t {
                Ty::Prim(p) => p.crystal().to_string(),
                Ty::Enum(n) | Ty::Class(n) if n != name => cls(n),
                _ => continue,
            };
            w.l(1, &format!("alias {} = {}", name, target));
        }
        w.l(0, "end");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // Enums: reopen the lib enum and add the derived traits.
    // ------------------------------------------------------------------------

    fn emit_enums(&mut self, w: &mut W) {
        let m = self.m;
        for (name, variants) in &m.enums {
            if variants.is_empty() {
                continue;
            }
            let fns = |k| m.trait_fn(name, k);
            w.l(0, &format!("enum {}", lib(name)));
            if let Some(f) = fns(FunctionKind::Default) {
                w.l(1, "# Rust `Default`.");
                w.l(1, "def self.create_default : self");
                w.l(2, &fun(&f));
                w.l(1, "end");
                w.l(0, "");
            }
            if let Some(f) = fns(FunctionKind::DebugToString) {
                w.l(1, "# Rust `Debug` (`to_s` is the variant name, as for every Crystal enum).");
                w.l(1, "def inspect(io : ::IO) : ::Nil");
                w.l(2, "__v = self");
                w.l(2, &format!("__s = {}(pointerof(__v))", fun(&f)));
                w.l(2, "io << Azul::Native.take_string(__s)");
                w.l(1, "end");
                w.l(0, "");
            }
            if fns(FunctionKind::DeepCopy).is_some() {
                w.l(1, "# Rust `Clone`: an enum value is copied by assignment.");
                w.l(1, "def clone : self");
                w.l(2, "self");
                w.l(1, "end");
                w.l(0, "");
            }
            if fns(FunctionKind::PartialEq).is_some() {
                w.l(1, "# Rust `PartialEq`: variants compare by discriminant.");
                w.l(1, "def ==(other : self) : ::Bool");
                w.l(2, "value == other.value");
                w.l(1, "end");
                w.l(0, "");
            }
            if fns(FunctionKind::PartialCmp).is_some() || fns(FunctionKind::Cmp).is_some() {
                w.l(1, "# Rust `PartialOrd` / `Ord`: declaration order.");
                w.l(1, "def <=>(other : self) : ::Int32");
                w.l(2, "value <=> other.value");
                w.l(1, "end");
                w.l(0, "");
            }
            if fns(FunctionKind::Hash).is_some() {
                w.l(1, "# Rust `Hash`.");
                w.l(1, "def hash(hasher)");
                w.l(2, "value.hash(hasher)");
                w.l(1, "end");
                w.l(0, "");
            }
            w.l(0, "end");
            w.l(0, "");
        }
    }

    // ------------------------------------------------------------------------
    // Classes
    // ------------------------------------------------------------------------

    /// The api.json constants this class owns, as Crystal constants in its
    /// body (`Azul::GlContextPtr::ACCUM_ALPHA_BITS`).
    ///
    /// These are the OpenGL enum values - 1436 of them, all on `GlContextPtr`.
    /// The C header has always had them; Crystal had none, so a caller could
    /// not name a single argument of the GL surface.
    fn emit_constants(&self, w: &mut W, class: &str) {
        let prefix = format!("{class}_");
        let mut any = false;
        for c in self.m.ir.constants.iter() {
            if !c.name.starts_with(&prefix) {
                continue;
            }
            let bare = c.member_name();
            let suffix = match c.type_name.trim() {
                "u8" => "_u8",
                "u16" => "_u16",
                "u64" => "_u64",
                "i8" => "_i8",
                "i16" => "_i16",
                "i32" => "_i32",
                "i64" => "_i64",
                "f32" => "_f32",
                "f64" => "_f64",
                _ => "_u32",
            };
            w.l(1, &format!("{} = {}{}", bare, c.value, suffix));
            any = true;
        }
        if any {
            w.l(0, "");
        }
    }

    fn emit_class(&mut self, w: &mut W, c: &ClassInfo) {
        let m = self.m;
        let name = &c.name;
        let delete = m.delete_fn(name);
        let clone = m.clone_fn(name);
        let copy = m.is_copy(name);
        let is_union = matches!(c.shape, Shape::Union { .. });

        w.doc(0, &c.doc);
        w.l(
            0,
            &format!(
                "{}class {} < Azul::Storage({})",
                if is_union { "abstract " } else { "" },
                cls(name),
                lib(name)
            ),
        );
        self.emit_constants(w, name);

        // --- storage hooks ---------------------------------------------------
        if let Shape::Union { variants, .. } = &c.shape {
            let first = &variants[0];
            let v0 = variant_struct(name, first);
            w.l(1, "# :nodoc:");
            w.l(1, &format!("def self.__alloc(tag : ::Int) : {}", cls(name)));
            w.l(2, "case tag");
            for v in variants {
                w.l(
                    2,
                    &format!("when {} then {}.allocate", v.index, variant_class(name, v)),
                );
            }
            w.l(
                2,
                &format!("else raise \"azul: invalid {} discriminant #{{tag}}\"", name),
            );
            w.l(2, "end");
            w.l(1, "end");
            w.l(0, "");
            w.l(1, "# :nodoc:");
            w.l(1, &format!("def self.__own(raw : {}) : {}", lib(name), cls(name)));
            w.l(2, &format!("__o = __alloc(raw.{}.tag)", first.member));
            w.l(2, &format!("__o.__init(::Pointer({}).malloc(1, raw))", lib(name)));
            if delete.is_some() {
                w.l(2, "::GC.add_finalizer(__o)");
            }
            w.l(2, "__o");
            w.l(1, "end");
            w.l(0, "");
            w.l(1, "# :nodoc:");
            w.l(
                1,
                &format!(
                    "def self.__view(ptr : ::Pointer({}), root : Azul::Wrapper) : {}",
                    lib(name),
                    cls(name)
                ),
            );
            w.l(
                2,
                &format!("__o = __alloc(ptr.as(::Pointer({})).value.tag)", v0),
            );
            w.l(2, "__o.__init(ptr)");
            w.l(2, "__o.__set_root(root)");
            w.l(2, "__o");
            w.l(1, "end");
            w.l(0, "");
        } else {
            w.l(1, "# :nodoc:");
            w.l(1, &format!("def self.__own(raw : {}) : {}", lib(name), cls(name)));
            w.l(2, "__o = allocate");
            w.l(2, &format!("__o.__init(::Pointer({}).malloc(1, raw))", lib(name)));
            if delete.is_some() {
                w.l(2, "::GC.add_finalizer(__o)");
            }
            w.l(2, "__o");
            w.l(1, "end");
            w.l(0, "");
            w.l(1, "# :nodoc:");
            w.l(
                1,
                &format!(
                    "def self.__view(ptr : ::Pointer({}), root : Azul::Wrapper) : {}",
                    lib(name),
                    cls(name)
                ),
            );
            w.l(2, "__o = allocate");
            w.l(2, "__o.__init(ptr)");
            w.l(2, "__o.__set_root(root)");
            w.l(2, "__o");
            w.l(1, "end");
            w.l(0, "");
        }
        if copy || clone.is_some() {
            w.l(1, "# :nodoc:");
            w.l(
                1,
                &format!(
                    "def self.__copy_from(ptr : ::Pointer({})) : {}",
                    lib(name),
                    cls(name)
                ),
            );
            match (&clone, copy) {
                (_, true) => w.l(2, "__r = ptr.value"),
                (Some(f), false) => w.l(2, &format!("__r = {}(ptr)", fun(f))),
                (None, false) => unreachable!(),
            }
            w.l(2, &format!("{}.__own(__r)", cls(name)));
            w.l(1, "end");
            w.l(0, "");
        }
        w.l(1, "# :nodoc:");
        w.l(
            1,
            "# Moves the value out: an owner is marked moved; a view is cloned; a Copy",
        );
        w.l(1, "# value is copied.");
        w.l(1, &format!("def __take : {}", lib(name)));
        w.l(2, "__check_alive");
        if copy {
            w.l(2, "@__ptr.value");
        } else {
            w.l(2, "if @__root");
            match &clone {
                Some(f) => w.l(3, &format!("{}(@__ptr)", fun(f))),
                None => w.l(
                    3,
                    &format!(
                        "raise Azul::MovedError.new(\"{} has no clone, so it cannot be moved out of the field it is borrowed from\")",
                        cls(name)
                    ),
                ),
            }
            w.l(2, "else");
            w.l(3, "__mark_moved");
            w.l(3, "@__ptr.value");
            w.l(2, "end");
        }
        w.l(1, "end");
        w.l(0, "");
        if let Some(f) = &delete {
            w.l(1, "# Rust `Drop`: frees the value unless it was moved or is a view.");
            w.l(1, "def finalize");
            w.l(2, "return if @__root || @__moved");
            w.l(2, &format!("{}(@__ptr)", fun(f)));
            w.l(1, "end");
            w.l(0, "");
        }

        // --- traits ------------------------------------------------------------
        self.emit_traits(w, c, copy);

        // --- RefAny: any Crystal object in, typed downcast out ----------------
        if c.category == TypeCategory::RefAny {
            w.l(1, "# Wraps any Crystal object; libazul keeps it alive while it holds this RefAny.");
            w.l(1, &format!("def self.new(object : ::Reference) : {}", cls(name)));
            w.l(2, "__r = Azul::Native.refany(object)");
            w.l(2, &format!("{}.__own(__r)", cls(name)));
            w.l(1, "end");
            w.l(0, "");
            w.l(1, "# The Crystal object this RefAny holds, if it is a `T`.");
            w.l(1, "def downcast(type : T.class) : T? forall T");
            w.l(2, "if __e = Azul::Handles.entry(to_unsafe)");
            w.l(3, "if __h = __e[0]");
            w.l(4, "return __h.get(T)");
            w.l(3, "end");
            w.l(2, "end");
            w.l(2, "nil");
            w.l(1, "end");
            w.l(0, "");
        }

        // --- methods, then fields (a method wins a name clash) ------------------
        let mut taken: BTreeSet<(bool, String, usize)> = BTreeSet::new();
        self.emit_methods(w, c, &mut taken);
        if let Shape::Struct(fields) = &c.shape {
            self.emit_fields(w, &lib(name), "to_unsafe", fields, &taken);
        }
        w.l(0, "end");
        w.l(0, "");

        // --- tagged union variants ------------------------------------------------
        if let Shape::Union { tag, variants } = &c.shape {
            for v in variants {
                self.emit_variant(w, c, tag, v, &taken);
            }
        }
    }

    fn emit_traits(&mut self, w: &mut W, c: &ClassInfo, copy: bool) {
        let m = self.m;
        let name = &c.name;
        let t = |k| m.trait_fn(name, k);
        let is_union = matches!(c.shape, Shape::Union { .. });

        if let Some(f) = t(FunctionKind::DebugToString) {
            w.l(1, "# Rust `Debug`.");
            w.l(1, "def to_s(io : ::IO) : ::Nil");
            w.l(2, &format!("__s = {}(to_unsafe)", fun(&f)));
            w.l(2, "io << Azul::Native.take_string(__s)");
            w.l(1, "end");
            w.l(0, "");
            w.l(1, "# :ditto:");
            w.l(1, "def inspect(io : ::IO) : ::Nil");
            w.l(2, "to_s(io)");
            w.l(1, "end");
            w.l(0, "");
        }
        if copy || t(FunctionKind::DeepCopy).is_some() {
            w.l(1, "# Rust `Clone`: an independent copy.");
            w.l(1, &format!("def clone : {}", cls(name)));
            w.l(2, &format!("{}.__copy_from(to_unsafe)", cls(name)));
            w.l(1, "end");
            w.l(0, "");
            w.l(1, "# :ditto:");
            w.l(1, &format!("def dup : {}", cls(name)));
            w.l(2, "clone");
            w.l(1, "end");
            w.l(0, "");
        }
        if let Some(f) = t(FunctionKind::PartialEq) {
            w.l(1, "# Rust `PartialEq`.");
            w.l(1, &format!("def ==(other : {}) : ::Bool", cls(name)));
            w.l(2, &format!("{}(to_unsafe, other.to_unsafe)", fun(&f)));
            w.l(1, "end");
            w.l(0, "");
        }
        let cmp = t(FunctionKind::Cmp);
        let pcmp = t(FunctionKind::PartialCmp);
        if cmp.is_some() || pcmp.is_some() {
            w.l(1, &format!("include ::Comparable({})", cls(name)));
            w.l(0, "");
        }
        if let Some(f) = &cmp {
            w.l(1, "# Rust `Ord`.");
            w.l(1, &format!("def <=>(other : {}) : ::Int32", cls(name)));
            w.l(2, &format!("{}(to_unsafe, other.to_unsafe).to_i32 - 1", fun(f)));
            w.l(1, "end");
            w.l(0, "");
        } else if let Some(f) = &pcmp {
            w.l(1, "# Rust `PartialOrd`: nil when the two are not comparable.");
            w.l(1, &format!("def <=>(other : {}) : ::Int32?", cls(name)));
            w.l(2, &format!("__r = {}(to_unsafe, other.to_unsafe)", fun(f)));
            w.l(2, "__r == 255 ? nil : __r.to_i32 - 1");
            w.l(1, "end");
            w.l(0, "");
        }
        if let Some(f) = t(FunctionKind::Hash) {
            w.l(1, "# Rust `Hash`.");
            w.l(1, "def hash(hasher)");
            w.l(2, &format!("{}(to_unsafe).hash(hasher)", fun(&f)));
            w.l(1, "end");
            w.l(0, "");
        }
        if let Some(f) = t(FunctionKind::Default) {
            w.l(1, "# Rust `Default`.");
            w.l(1, &format!("def self.create_default : {}", cls(name)));
            w.l(2, &format!("__r = {}", fun(&f)));
            w.l(2, &format!("{}.__own(__r)", cls(name)));
            w.l(1, "end");
            w.l(0, "");
            let has_zero_arg_new = m.functions_of(name).iter().any(|f| {
                matches!(f.kind, FunctionKind::Constructor | FunctionKind::StaticMethod)
                    && f.method_name == "create"
                    && f.args.is_empty()
            });
            if !is_union && !has_zero_arg_new {
                w.l(1, "# Same as `.create_default`.");
                w.l(1, &format!("def self.new : {}", cls(name)));
                w.l(2, "create_default");
                w.l(1, "end");
                w.l(0, "");
            }
        }
    }

    // ------------------------------------------------------------------------
    // Fields
    // ------------------------------------------------------------------------

    fn emit_fields(
        &mut self,
        w: &mut W,
        lib_struct: &str,
        base: &str,
        fields: &[Field],
        taken: &BTreeSet<(bool, String, usize)>,
    ) {
        let m = self.m;
        for f in fields {
            if f.name.starts_with('_') {
                continue;
            }
            let getter = if reserved_method(&f.name) {
                format!("{}_", f.name)
            } else {
                f.name.clone()
            };
            let lib_field = sanitize_identifier(&f.name);
            let ty = m.not_result(m.field(f));
            let lib_ty = match f.ref_kind {
                FieldRefKind::Owned => m.lib_owned(&f.type_name),
                _ => qualify(&super::field_type_for_ref_kind(&f.type_name, &f.ref_kind, m.ir)),
            };
            let fptr = format!(
                "({}.as(::Pointer(::UInt8)) + offsetof({}, @{})).as(::Pointer({}))",
                base, lib_struct, lib_field, lib_ty
            );
            let get = match &ty {
                Ty::Class(n) => Some(format!("{}.__view(__f, self)", cls(n))),
                // allow-api-name: a RefAny field is viewed through the RefAny class.
                Ty::RefAny => Some("Azul::RefAny.__view(__f, self)".to_string()),
                other => m.out_expr(other, "__f"),
            };
            let get_ty = match &ty {
                // allow-api-name: as above - the RefAny class names itself.
                Ty::RefAny => Some(cls("RefAny")),
                other => exact(other),
            };
            if let (Some(get), Some(get_ty)) = (get, get_ty) {
                if !taken.contains(&(false, getter.clone(), 0)) {
                    if let Some(d) = &f.doc {
                        w.doc(1, std::slice::from_ref(d));
                    }
                    w.l(1, &format!("def {} : {}", getter, get_ty));
                    w.l(2, &format!("__f = {}", fptr));
                    w.l(2, &get);
                    w.l(1, "end");
                    w.l(0, "");
                }
            }
            let setter = format!("{}=", getter);
            if taken.contains(&(false, setter.clone(), 1)) {
                continue;
            }
            let (Some(restr), Some(setup)) = (restriction(&ty), m.in_setup(&ty, "value", "__v"))
            else {
                continue;
            };
            let drop_old = match &ty {
                Ty::Str | Ty::RefAny | Ty::Option { .. } | Ty::Vec { .. } => m.cleanup(&ty, "__f"),
                Ty::Class(n) => m.delete_fn(n).map(|d| format!("{}(__f)", fun(&d))),
                _ => None,
            };
            w.l(1, &format!("def {}(value : {}) : ::Nil", setter, restr));
            w.l(2, &setup);
            w.l(2, &format!("__f = {}", fptr));
            if let Some(d) = drop_old {
                w.l(2, &d);
            }
            w.l(2, "__f.value = __v");
            w.l(1, "end");
            w.l(0, "");
        }
    }

    // ------------------------------------------------------------------------
    // Tagged-union variants: one subclass each, pattern-matchable with `case`.
    // ------------------------------------------------------------------------

    fn emit_variant(
        &mut self,
        w: &mut W,
        c: &ClassInfo,
        tag: &str,
        v: &Variant,
        taken: &BTreeSet<(bool, String, usize)>,
    ) {
        let m = self.m;
        let vclass = variant_class(&c.name, v);
        let vstruct = variant_struct(&c.name, v);
        if let Some(d) = &v.doc {
            w.doc(0, std::slice::from_ref(d));
        }
        w.l(0, &format!("class {} < {}", vclass, cls(&c.name)));

        // Constructor from the payload.
        let mut params = Vec::new();
        let mut setups = Vec::new();
        let mut ok = true;
        for (i, f) in v.fields.iter().enumerate() {
            let ty = m.not_result(m.field(f));
            let local = format!("__p{}", i);
            let pname = arg_name(&f.name);
            match (restriction(&ty), m.in_setup(&ty, &pname, &local)) {
                (Some(r), Some(s)) if f.ref_kind == FieldRefKind::Owned => {
                    params.push(format!("{} : {}", pname, r));
                    setups.push((s, local, sanitize_identifier(&f.name)));
                }
                _ => ok = false,
            }
        }
        if ok {
            if params.is_empty() {
                w.l(1, &format!("def self.new : {}", vclass));
            } else {
                w.l(1, &format!("def self.new({}) : {}", params.join(", "), vclass));
            }
            for (s, _, _) in &setups {
                w.l(2, s);
            }
            w.l(2, &format!("__v = {}.new", vstruct));
            w.l(2, &format!("__v.tag = {}.new({})", qualify(tag), v.index));
            for (_, local, field) in &setups {
                w.l(2, &format!("__v.{} = {}", field, local));
            }
            w.l(2, &format!("__raw = {}.new", lib(&c.name)));
            w.l(2, &format!("__raw.{} = __v", v.member));
            w.l(2, &format!("{}.__own(__raw).as({})", cls(&c.name), vclass));
            w.l(1, "end");
            w.l(0, "");
        }
        let base = format!("to_unsafe.as(::Pointer({}))", vstruct);
        self.emit_fields(w, &vstruct, &base, &v.fields, taken);
        w.l(0, "end");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // Methods
    // ------------------------------------------------------------------------

    fn emit_methods(
        &mut self,
        w: &mut W,
        c: &ClassInfo,
        taken: &mut BTreeSet<(bool, String, usize)>,
    ) {
        let m = self.m;
        let funcs: Vec<&FunctionDef> = m
            .functions_of(&c.name)
            .iter()
            .copied()
            .filter(|f| match f.kind {
                FunctionKind::Constructor
                | FunctionKind::StaticMethod
                | FunctionKind::Method
                | FunctionKind::MethodMut => true,
                FunctionKind::EnumVariantConstructor => {
                    matches!(c.shape, Shape::Union { .. })
                }
                _ => false,
            })
            .collect();

        // Pick Crystal names; a clash (same level, name and arity) keeps the
        // api.json names for every function involved.
        let mut names: Vec<(String, bool, usize)> = funcs
            .iter()
            .map(|f| {
                let inst = is_instance(f);
                let arity = f.args.len().saturating_sub(usize::from(inst));
                (self.method_name(f, inst, arity), inst, arity)
            })
            .collect();
        let mut count: BTreeMap<(bool, String, usize), usize> = BTreeMap::new();
        for (n, inst, a) in &names {
            *count.entry((*inst, n.clone(), *a)).or_default() += 1;
        }
        for (i, f) in funcs.iter().enumerate() {
            let (n, inst, a) = &names[i];
            if count[&(*inst, n.clone(), *a)] > 1 {
                let raw = snake(&f.method_name);
                names[i].0 = if reserved_method(&raw) {
                    format!("{}_", raw)
                } else {
                    raw
                };
            }
        }

        for (i, f) in funcs.iter().enumerate() {
            let (name, inst, arity) = names[i].clone();
            if self.emit_method(w, c, f, &name, inst) {
                taken.insert((!inst, name, arity));
            } else {
                self.skipped.push(f.c_name.clone());
            }
        }
    }

    fn method_name(&self, f: &FunctionDef, inst: bool, arity: usize) -> String {
        let raw = snake(&f.method_name);
        let ret_bool = f.return_type.as_deref() == Some("bool");
        let name = if !inst {
            if raw == "create" {
                "new".to_string()
            } else if let Some(rest) = raw.strip_prefix("create_") {
                rest.to_string()
            } else {
                raw.clone()
            }
        } else if let (Some(rest), 0) = (raw.strip_prefix("get_"), arity) {
            rest.to_string()
        } else if let (Some(rest), 0, true) = (raw.strip_prefix("is_"), arity, ret_bool) {
            format!("{}?", rest)
        } else if arity == 0 && ret_bool && (raw.starts_with("has_") || raw.starts_with("can_")) {
            format!("{}?", raw)
        } else if let (Some(rest), 1, None) = (raw.strip_prefix("set_"), arity, &f.return_type) {
            let only_arg = &f.args[f.args.len() - 1];
            if self.callback_of(only_arg).is_some() {
                raw.clone()
            } else {
                format!("{}=", rest)
            }
        } else {
            raw.clone()
        };
        let valid = name
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase() || ch == '_')
            && !name[..name.len() - usize::from(name.ends_with('?') || name.ends_with('='))]
                .chars()
                .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'));
        if reserved_method(name.trim_end_matches(['?', '='])) || !valid {
            format!("{}_", raw)
        } else {
            name
        }
    }

    /// Is `arg` a callback? Returns (typedef, Some((wrapper struct, cb field,
    /// ctx field))) for a wrapper-struct argument.
    fn callback_of(&self, arg: &super::super::ir::FunctionArg) -> Option<CallbackArg<'a>> {
        let m = self.m;
        if arg.ref_kind != ArgRefKind::Owned {
            return None;
        }
        let t = arg.type_name.trim();
        if let Some(td) = m.callbacks.get(t) {
            return Some(CallbackArg {
                td,
                wrapper: None,
            });
        }
        let c = m.classes.get(t)?;
        let info = c.callback_wrapper.as_ref()?;
        let td = m.callbacks.get(&info.callback_typedef_name)?;
        let ctx = callback_ctx_field(t, m.ir)?;
        Some(CallbackArg {
            td,
            wrapper: Some((t.to_string(), info.callback_field_name.clone(), ctx)),
        })
    }

    /// Emit one method; false if some argument or the return value has no
    /// Crystal-side shape (the raw lib fun stays available).
    fn emit_method(&mut self, w: &mut W, c: &ClassInfo, f: &FunctionDef, name: &str, inst: bool) -> bool {
        let m = self.m;
        let own = &c.name;
        let args: Vec<&super::super::ir::FunctionArg> =
            f.args.iter().skip(usize::from(inst)).collect();

        // ---- callbacks -------------------------------------------------------
        let cb_args: Vec<(usize, CallbackArg)> = args
            .iter()
            .enumerate()
            .filter_map(|(i, a)| self.callback_of(a).map(|cb| (i, cb)))
            .collect();
        let refany_args: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| a.ref_kind == ArgRefKind::Owned && m.owned(&a.type_name).is_ref_any())
            .map(|(i, _)| i)
            .collect();
        let layout_factory_candidate = if !inst && cb_args.len() == 1 && args.len() == 1 {
            m.ir
                .find_struct(own)
                .and_then(|s| layout_callback_factory_info(s, m.ir))
                .filter(|info| {
                    f.return_type.as_deref() == Some(own.as_str())
                        && info.callback_wrapper
                            == cb_args[0].1.td.name.trim_end_matches("Type")
                })
        } else {
            None
        };

        struct CbPlan {
            arg: usize,
            closure: bool,
            ctx_arg: Option<(usize, String)>,
            data_first: bool,
            carry_in_data: Option<usize>,
        }
        let mut plans: Vec<CbPlan> = Vec::new();
        let mut generic = false;
        for (i, cb) in &cb_args {
            let td = cb.td;
            let ctx_arg = td.args.iter().enumerate().find_map(|(j, a)| {
                (a.ref_kind == ArgRefKind::Owned)
                    .then(|| m.ctx_getter(a.type_name.trim()))
                    .flatten()
                    .map(|g| (j, g))
            });
            let data_first = td
                .args
                .first()
                .is_some_and(|a| a.ref_kind == ArgRefKind::Owned && m.owned(&a.type_name).is_ref_any());
            // allow-api-name: the callback context slot IS an OptionRefAny;
            // a closure can only be carried if that type can be built and freed.
            let ctx_usable = ctx_arg.is_some()
                // allow-api-name: the context slot IS an OptionRefAny.
                && m.fun("OptionRefAny", "delete").is_some()
                && (cb.wrapper.is_some() || layout_factory_candidate.is_some());
            let carry_in_data =
                (data_first && cb_args.len() == 1 && refany_args.len() == 1).then(|| refany_args[0]);
            let closure = self.callback_convertible(td)
                && (ctx_usable || carry_in_data.is_some())
                // allow-api-name: as above - the context type.
                && matches!(m.owned("OptionRefAny"), Ty::Option { .. });
            if closure && data_first {
                generic = true;
            }
            plans.push(CbPlan {
                arg: *i,
                closure,
                ctx_arg: if ctx_usable { ctx_arg } else { None },
                data_first,
                carry_in_data: if closure { carry_in_data } else { None },
            });
        }

        let layout_factory = layout_factory_candidate.filter(|_| plans.first().is_some_and(|p| p.closure));

        // ---- signature + argument setup ---------------------------------------
        let mut params: Vec<String> = Vec::new();
        let mut setups_first: Vec<String> = Vec::new();
        let mut setups: Vec<String> = Vec::new();
        let mut call_args: Vec<String> = Vec::new();
        let mut cleanups: Vec<String> = Vec::new();
        let mut pre_closures: Vec<String> = Vec::new();

        for (i, a) in args.iter().enumerate() {
            let pname = arg_name(&a.name);
            let local = format!("__a{}", i);
            if let Some(plan) = plans.iter().find(|p| p.arg == i) {
                let cb = &cb_args.iter().find(|(j, _)| *j == i).unwrap().1;
                let td = cb.td;
                if plan.closure {
                    let Some(proc_ty) = self.user_proc_type(td, plan.data_first) else {
                        return false;
                    };
                    params.push(format!("{} : {}", pname, proc_ty));
                    pre_closures.extend(self.erased_closure(td, &pname, &format!("__erased{}", i), plan.data_first));
                    self.trampolines.insert(td.name.clone());
                    let tramp = format!("Azul::Trampolines::{}", td.name);
                    match &cb.wrapper {
                        Some((wstruct, cb_field, ctx_field)) => {
                            setups.push(format!("{} = {}.new", local, lib(wstruct)));
                            setups.push(format!("{}.{} = {}", local, sanitize_identifier(cb_field), tramp));
                            if plan.ctx_arg.is_some() {
                                setups.push(format!(
                                    "__h{} = Azul::Handles.refany(nil, Azul::Handles.hold(__erased{}))",
                                    i, i
                                ));
                                setups.push(format!("__o{} = Azul::Conv.wrap_OptionRefAny(__h{})", i, i));
                            } else {
                                setups.push(format!("__o{} = Azul::Conv.in_OptionRefAny(nil)", i));
                            }
                            setups.push(format!("{}.{} = __o{}", local, sanitize_identifier(ctx_field), i));
                            call_args.push(local.clone());
                        }
                        None => {
                            call_args.push(tramp);
                        }
                    }
                } else {
                    // Raw: a non-capturing proc with the C signature.
                    params.push(format!("{} : {}", pname, lib(&td.name)));
                    match &cb.wrapper {
                        Some((wstruct, cb_field, ctx_field)) => {
                            setups.push(format!("{} = {}.new", local, lib(wstruct)));
                            setups.push(format!("{}.{} = {}", local, sanitize_identifier(cb_field), pname));
                            // allow-api-name: as above - the context type.
                            if !matches!(m.owned("OptionRefAny"), Ty::Option { .. }) {
                                return false;
                            }
                            setups.push(format!("__o{} = Azul::Conv.in_OptionRefAny(nil)", i));
                            setups.push(format!("{}.{} = __o{}", local, sanitize_identifier(ctx_field), i));
                            call_args.push(local.clone());
                        }
                        None => call_args.push(pname.clone()),
                    }
                }
                continue;
            }

            // A data RefAny that also carries a closure.
            if let Some(plan) = plans.iter().find(|p| p.carry_in_data == Some(i)) {
                params.push(format!("{} : T", pname));
                setups.push(format!(
                    "{} = Azul::Handles.refany(Azul::Handles.hold({}), Azul::Handles.hold(__erased{}))",
                    local, pname, plan.arg
                ));
                call_args.push(local.clone());
                continue;
            }
            // A plain data RefAny next to a closure registered through ctx: it
            // is still handed back to the closure as `T`.
            if generic && a.ref_kind == ArgRefKind::Owned && refany_args.len() == 1 && refany_args[0] == i {
                params.push(format!("{} : T", pname));
                setups.push(format!(
                    "{} = Azul::Handles.refany(Azul::Handles.hold({}), nil)",
                    local, pname
                ));
                call_args.push(local.clone());
                continue;
            }

            let ty = if a.type_name.trim() == own.as_str() {
                Ty::Class(own.clone())
            } else {
                m.not_result(m.owned(&a.type_name))
            };
            match a.ref_kind {
                ArgRefKind::Owned => {
                    let (Some(r), Some(s)) = (restriction(&ty), m.in_setup(&ty, &pname, &local)) else {
                        return false;
                    };
                    if matches!(ty, Ty::Callback(_)) {
                        return false;
                    }
                    params.push(format!("{} : {}", pname, r));
                    if matches!(ty, Ty::Prim(_)) {
                        setups_first.push(s);
                    } else {
                        setups.push(s);
                    }
                    call_args.push(local);
                }
                ArgRefKind::Ref | ArgRefKind::RefMut => match &ty {
                    Ty::Class(n) => {
                        params.push(format!("{} : {}", pname, cls(n)));
                        call_args.push(format!("{}.to_unsafe", pname));
                    }
                    Ty::RefAny => {
                        params.push(format!("{} : Azul::RefAny", pname));
                        call_args.push(format!("{}.to_unsafe", pname));
                    }
                    Ty::Prim(_) | Ty::Enum(_) | Ty::Str | Ty::Option { .. } | Ty::Vec { .. } => {
                        let (Some(r), Some(s)) = (restriction(&ty), m.in_setup(&ty, &pname, &local)) else {
                            return false;
                        };
                        params.push(format!("{} : {}", pname, r));
                        setups.push(s);
                        call_args.push(format!("pointerof({})", local));
                        if let Some(cl) = m.cleanup(&ty, &format!("pointerof({})", local)) {
                            cleanups.push(cl);
                        }
                    }
                    _ => return false,
                },
                ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                    let p = qualify(&super::ptr_to_crystal(&a.type_name, m.ir));
                    params.push(format!("{} : {}", pname, p));
                    call_args.push(pname);
                }
            }
        }

        // ---- receiver ------------------------------------------------------------
        let recv = if inst { f.args.first() } else { None };
        let ret = m.ret_ty(f.return_type.as_deref(), own);
        let builder = matches!(recv, Some(r) if r.ref_kind == ArgRefKind::Owned)
            && matches!(&ret, Ty::Class(n) if n == own);
        if let Some(r) = recv {
            match r.ref_kind {
                ArgRefKind::Owned => {
                    let take = if builder { "__value" } else { "__take" };
                    setups.push(format!("__self = {}", take));
                    call_args.insert(0, "__self".to_string());
                }
                _ => call_args.insert(0, "to_unsafe".to_string()),
            }
        }

        // ---- return ----------------------------------------------------------------
        let (ret_ann, ret_line) = if builder {
            ("self".to_string(), None)
        } else if layout_factory.is_some() {
            (cls(own), None)
        } else {
            match &ret {
                Ty::Void => ("::Nil".to_string(), None),
                t => {
                    let (Some(a), Some(e)) = (exact(t), m.take_expr(t, "__r")) else {
                        return false;
                    };
                    (a, Some(e))
                }
            }
        };

        // ---- emit ------------------------------------------------------------------
        let def_head = format!(
            "def {}{}({}) : {}{}",
            if inst { "" } else { "self." },
            name,
            params.join(", "),
            ret_ann,
            if generic { " forall T" } else { "" }
        );
        w.doc(1, &f.doc);
        w.l(1, &def_head.replace("() :", " :"));
        for s in pre_closures.iter().chain(setups_first.iter()).chain(setups.iter()) {
            w.l(2, s);
        }
        let call = format!("{}({})", fun(&self.symbol(f, &cb_args)), call_args.join(", "));
        if builder {
            w.l(2, &format!("__r = {}", call));
            for cl in &cleanups {
                w.l(2, cl);
            }
            w.l(2, "__replace(__r)");
            w.l(2, "self");
        } else if let Some(fac) = &layout_factory {
            w.l(2, &format!("__r = {}", call));
            w.l(2, &format!("__obj = {}.__own(__r)", cls(own)));
            // Splice the closure handle into the nested callback's ctx: the
            // raw constructor only takes the bare function pointer.
            let mut offsets = Vec::new();
            let mut parent = own.clone();
            for (seg, ty) in fac.field_path.iter().zip(fac.field_types.iter()) {
                offsets.push(format!("offsetof({}, @{})", lib(&parent), sanitize_identifier(seg)));
                parent = ty.clone();
            }
            let ctx_field = callback_ctx_field(&fac.callback_wrapper, m.ir).unwrap_or_else(|| "ctx".to_string());
            offsets.push(format!("offsetof({}, @{})", lib(&parent), sanitize_identifier(&ctx_field)));
            w.l(2, "__h0 = Azul::Handles.refany(nil, Azul::Handles.hold(__erased0))");
            w.l(2, "__o0 = Azul::Conv.wrap_OptionRefAny(__h0)");
            w.l(
                2,
                &format!(
                    "__ctx = (__obj.to_unsafe.as(::Pointer(::UInt8)) + ({})).as(::Pointer(LibAzul::AzOptionRefAny))",
                    offsets.join(" + ")
                ),
            );
            w.l(2, "LibAzul.azOptionRefAny_delete(__ctx)");
            w.l(2, "__ctx.value = __o0");
            w.l(2, "__obj");
        } else if let Some(line) = ret_line {
            w.l(2, &format!("__r = {}", call));
            for cl in &cleanups {
                w.l(2, cl);
            }
            w.l(2, &line);
        } else {
            w.l(2, &call);
            for cl in &cleanups {
                w.l(2, cl);
            }
        }
        w.l(1, "end");
        w.l(0, "");

        // Block form: the single callback argument as a block.
        if cb_args.len() == 1 && plans[0].closure {
            let plan = &plans[0];
            let td = cb_args[0].1.td;
            if let Some(block_ty) = self.user_block_type(td, plan.data_first) {
                let cb_name = arg_name(&args[plan.arg].name);
                let mut bparams: Vec<String> = Vec::new();
                let mut forward: Vec<String> = Vec::new();
                let needs_type_param = plan.data_first && refany_args.is_empty();
                if needs_type_param {
                    bparams.push("data_type : T.class".to_string());
                }
                for (i, a) in args.iter().enumerate() {
                    if i == plan.arg {
                        forward.push(cb_name.clone());
                        continue;
                    }
                    let p = params[i].clone();
                    bparams.push(p);
                    forward.push(arg_name(&a.name));
                }
                bparams.push(format!("&{} : {}", cb_name, block_ty));
                w.l(1, "# :ditto:");
                w.l(
                    1,
                    &format!(
                        "def {}{}({}) : {}{}",
                        if inst { "" } else { "self." },
                        name,
                        bparams.join(", "),
                        ret_ann,
                        if generic { " forall T" } else { "" }
                    ),
                );
                w.l(2, &format!("{}({})", name.trim_end_matches('='), forward.join(", ")));
                w.l(1, "end");
                w.l(0, "");
            }
        }
        true
    }

    /// The C symbol a method binds: the `Struct` variant when a wrapper-struct
    /// callback is passed to an allowlisted function (its plain symbol takes
    /// the bare function pointer).
    fn symbol(&self, f: &FunctionDef, cb_args: &[(usize, CallbackArg)]) -> String {
        if cb_args.iter().any(|(_, cb)| cb.wrapper.is_some()) && has_callback_wrapper_arg(f) {
            format!("{}Struct", f.c_name)
        } else {
            f.c_name.clone()
        }
    }

    // ------------------------------------------------------------------------
    // Callback plumbing
    // ------------------------------------------------------------------------

    /// The lib-level type of callback arg `a` (pointer args to azul types
    /// are `Void*`, see `types::emit_callback_typedef`).
    fn cb_lib_arg(&self, a: &super::super::ir::FunctionArg) -> String {
        let t = super::arg_type_for_ref_kind(&a.type_name, &a.ref_kind, self.m.ir);
        if t.starts_with("Az") && t.ends_with('*') {
            "::Pointer(::Void)".to_string()
        } else {
            qualify(&t)
        }
    }

    fn cb_lib_ret(&self, td: &CallbackTypedefDef) -> String {
        match td.return_type.as_deref() {
            None => "::Nil".to_string(),
            Some(r) => match self.m.owned(r) {
                Ty::Void => "::Nil".to_string(),
                _ => self.m.lib_owned(r),
            },
        }
    }

    /// The Crystal-side type of callback arg `j`.
    fn cb_user_arg(&self, td: &CallbackTypedefDef, j: usize, data_first: bool) -> Option<(String, Ty)> {
        let a = &td.args[j];
        if j == 0 && data_first {
            return Some(("T".to_string(), Ty::RefAny));
        }
        if a.ref_kind != ArgRefKind::Owned {
            let t = self.cb_lib_arg(a);
            return Some((t.clone(), Ty::RawPtr(t)));
        }
        let ty = self.m.not_result(self.m.owned(&a.type_name));
        let ex = exact(&ty)?;
        self.m.take_expr(&ty, "x")?;
        Some((ex, ty))
    }

    fn cb_user_ret(&self, td: &CallbackTypedefDef) -> Option<(String, Ty)> {
        let ty = match td.return_type.as_deref() {
            None => Ty::Void,
            Some(r) => self.m.not_result(self.m.owned(r)),
        };
        if matches!(ty, Ty::Void) {
            return Some(("::Nil".to_string(), ty));
        }
        let ex = match &ty {
            Ty::RefAny => "::Reference".to_string(),
            other => exact(other)?,
        };
        self.m.in_setup(&ty, "x", "y")?;
        Some((ex, ty))
    }

    fn callback_convertible(&self, td: &CallbackTypedefDef) -> bool {
        (0..td.args.len()).all(|j| self.cb_user_arg(td, j, true).is_some())
            && self.cb_user_ret(td).is_some()
    }

    fn user_proc_type(&self, td: &CallbackTypedefDef, data_first: bool) -> Option<String> {
        let mut parts = Vec::new();
        for j in 0..td.args.len() {
            parts.push(self.cb_user_arg(td, j, data_first)?.0);
        }
        parts.push(self.cb_user_ret(td)?.0);
        Some(format!("::Proc({})", parts.join(", ")))
    }

    fn user_block_type(&self, td: &CallbackTypedefDef, data_first: bool) -> Option<String> {
        let mut parts = Vec::new();
        for j in 0..td.args.len() {
            parts.push(self.cb_user_arg(td, j, data_first)?.0);
        }
        let (ret, _) = self.cb_user_ret(td)?;
        Some(format!("{} -> {}", parts.join(", "), ret))
    }

    /// The closure libazul's trampoline calls: raw C values in, the user's
    /// proc in the middle, a raw C value out.
    fn erased_closure(&self, td: &CallbackTypedefDef, user: &str, var: &str, data_first: bool) -> Vec<String> {
        let mut out = Vec::new();
        let params: Vec<String> = td
            .args
            .iter()
            .enumerate()
            .map(|(j, a)| format!("__c{} : {}", j, self.cb_lib_arg(a)))
            .collect();
        out.push(format!(
            "{} = ->({}) : {} {{",
            var,
            params.join(", "),
            self.cb_lib_ret(td)
        ));
        let mut call = Vec::new();
        for j in 0..td.args.len() {
            if j == 0 && data_first {
                out.push("  __p0 = __c0".to_string());
                out.push("  __u0 = Azul::Handles.object(pointerof(__p0), T)".to_string());
            } else {
                let (_, ty) = self.cb_user_arg(td, j, data_first).expect("checked");
                let e = self.m.take_expr(&ty, &format!("__c{}", j)).expect("checked");
                out.push(format!("  __u{} = {}", j, e));
            }
            call.push(format!("__u{}", j));
        }
        let (_, rty) = self.cb_user_ret(td).expect("checked");
        match rty {
            Ty::Void => {
                out.push(format!("  {}.call({})", user, call.join(", ")));
                out.push("  nil".to_string());
            }
            ty => {
                out.push(format!("  __ur = {}.call({})", user, call.join(", ")));
                out.push(format!("  {}", self.m.in_setup(&ty, "__ur", "__rr").expect("checked")));
                out.push("  __rr".to_string());
            }
        }
        out.push("}".to_string());
        out
    }

    /// The log sink among a callback's arguments: the argument index, the
    /// `LibAzul` fun to call, and the level constant to report at.
    ///
    /// Structural, never a list of kinds: the first argument whose class
    /// declares an instance method taking an enum level and a string
    /// message. A kind without one reports to STDERR instead.
    fn log_sink(&self, td: &CallbackTypedefDef) -> Option<(usize, String, String)> {
        let m = self.m;
        for (j, a) in td.args.iter().enumerate() {
            // Owned arguments arrive BY VALUE, so a local copy can be
            // addressed for the `self` pointer the method takes; an argument
            // passed by reference is an untyped `::Pointer(::Void)` here
            // (see `cb_lib_arg`) and carries nothing to call through.
            if a.ref_kind != ArgRefKind::Owned {
                continue;
            }
            let Some(fns) = m.functions.get(a.type_name.trim()) else {
                continue;
            };
            let Some(f) = fns.iter().find(|f| {
                matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
                    // api.json flags no capability as "the diagnostic
                    // channel", and the shape alone (an enum level plus a
                    // string message) also matches ordinary methods, so this
                    // one entry point has to be named. The symbol, the level
                    // constant and the argument all come from the IR.
                    // allow-api-name: the diagnostic channel's api.json name.
                    && f.method_name == "log"
                    && f.args.len() == 3
                    && m.ir
                        .find_struct(f.args[2].type_name.trim())
                        .is_some_and(|s| s.category == TypeCategory::String)
            }) else {
                continue;
            };
            let level_ty = f.args[1].type_name.trim();
            let Some(e) = m.ir.find_enum(level_ty) else { continue };
            if e.is_union || e.variants.is_empty() {
                continue;
            }
            // A failed callback is an error; the first variant is the only
            // other thing the IR can offer if the level enum ever loses it.
            let variant = e
                .variants
                .iter()
                .find(|v| v.name == "Error")
                .unwrap_or(&e.variants[0]);
            return Some((
                j,
                fun(&f.c_name),
                format!("{}::{}", lib(level_ty), enum_member_name(&variant.name)),
            ));
        }
        None
    }

    /// The statement that puts this kind's FALLBACK return value in
    /// `__result`, for the trampoline arm where the callback raised: the
    /// return type's default constructor when it has one, else all-zero
    /// bytes (variant 0 of a fieldless enum, an empty POD).
    ///
    /// Before this, that arm returned the `uninitialized` slot - whatever
    /// was on the stack, read by Rust as a live value of the type, a fresh
    /// bug class on top of the one being reported.
    fn ret_fallback(&self, td: &CallbackTypedefDef) -> String {
        let ctor = td.return_type.as_deref().map(str::trim).and_then(|rt| {
            self.m
                .functions
                .get(rt)?
                .iter()
                .find(|f| f.kind == FunctionKind::Default && f.args.is_empty())
                .map(|f| fun(&f.c_name))
        });
        match ctor {
            Some(c) => format!("__result = {}", c),
            // `Pointer#clear` zeroes the slot in place, whatever the type -
            // no need to know whether it is an enum, a POD or a union.
            None => "pointerof(__result).clear(1)".to_string(),
        }
    }

    fn emit_trampolines(&mut self, w: &mut W) {
        let m = self.m;
        w.l(0, "# :nodoc:");
        w.l(0, "# One non-capturing C entry point per callback typedef. It finds the");
        w.l(0, "# registered Crystal closure (in the callback's ctx, or in the data RefAny)");
        w.l(0, "# and calls it, catching everything it may raise: an exception crossing");
        w.l(0, "# back into libazul's frames is undefined behaviour, so each entry point");
        w.l(0, "# reports it (through the callback's log sink when it has one) and");
        w.l(0, "# returns its kind's fallback value instead.");
        w.l(0, "module Azul::Trampolines");
        for tdn in self.trampolines.clone() {
            let td = m.callbacks[&tdn];
            let params: Vec<String> = td
                .args
                .iter()
                .enumerate()
                .map(|(j, a)| format!("__c{} : {}", j, self.cb_lib_arg(a)))
                .collect();
            let names: Vec<String> = (0..td.args.len()).map(|j| format!("__c{}", j)).collect();
            let ret = self.cb_lib_ret(td);
            let erased = {
                let mut p: Vec<String> = td.args.iter().map(|a| self.cb_lib_arg(a)).collect();
                p.push(ret.clone());
                format!("::Proc({})", p.join(", "))
            };
            let data_first = td
                .args
                .first()
                .is_some_and(|a| a.ref_kind == ArgRefKind::Owned && m.owned(&a.type_name).is_ref_any());
            let ctx = td.args.iter().enumerate().find_map(|(j, a)| {
                (a.ref_kind == ArgRefKind::Owned)
                    .then(|| m.ctx_getter(a.type_name.trim()))
                    .flatten()
                    .map(|g| (j, g))
            });
            w.l(1, &format!("{} = ->({}) {{", tdn, params.join(", ")));
            w.l(2, &format!("Azul::Trampolines.call_{}({})", tdn, names.join(", ")));
            w.l(1, "}");
            w.l(0, "");
            w.l(1, &format!("def self.call_{}({}) : {}", tdn, params.join(", "), ret));
            let void = ret == "::Nil";
            let sink = self.log_sink(td);
            if !void {
                w.l(2, &format!("__result = uninitialized {}", ret));
            }
            w.l(2, "begin");
            w.l(3, &format!("__f = nil.as({}?)", erased));
            if let Some((j, getter)) = &ctx {
                w.l(3, &format!("__i = __c{}", j));
                w.l(3, &format!("__ctx = {}(pointerof(__i))", fun(getter)));
                w.l(3, "if __ctxr = Azul::Conv.take_OptionRefAny(__ctx)");
                w.l(4, &format!("__f = Azul::Handles.closure(__ctxr.to_unsafe, {})", erased));
                w.l(3, "end");
            }
            if data_first {
                w.l(3, "__d = __c0");
                w.l(3, &format!("__f ||= Azul::Handles.closure(pointerof(__d), {})", erased));
            }
            w.l(
                3,
                &format!("raise \"no Crystal closure is registered for this {}\" unless __f", tdn),
            );
            if void {
                w.l(3, &format!("__f.call({})", names.join(", ")));
            } else {
                w.l(3, &format!("__result = __f.call({})", names.join(", ")));
            }
            // A Crystal exception unwinding into libazul's frames is
            // undefined behaviour across the C ABI, so EVERYTHING the
            // callback (and the downcast of its data RefAny) can raise stops
            // here: it is reported and the fallback value above is returned.
            w.l(2, "rescue __ex : ::Exception");
            if !void {
                w.l(3, "# The body never ran to its assignment, so `__result` is still");
                w.l(3, "# uninitialized: give Rust this kind's fallback value, not");
                w.l(3, "# whatever was on the stack. (Built HERE and not before the");
                w.l(3, "# call: a default that the callback then replaces would be an");
                w.l(3, "# owned value nobody ever frees.)");
                w.l(3, &self.ret_fallback(td));
            }
            match &sink {
                Some((j, log_fn, level)) => {
                    w.l(3, "# Through the callback's own log sink, so the failure");
                    w.l(3, "# reaches the host's log pipeline and not just a terminal.");
                    w.l(3, &format!("__le = __c{}", j));
                    w.l(3, &format!("Azul::Native.callback_raised(\"{}\", __ex) do |__msg|", tdn));
                    // The Crystal 1.21 rule at the top of this file: the
                    // AzString goes through a local, never straight into the
                    // argument list of another LibAzul call.
                    w.l(4, "__ls = Azul::Native.az_string(__msg)");
                    w.l(4, &format!("{}(pointerof(__le), {}, __ls)", log_fn, level));
                    w.l(4, "true");
                    w.l(3, "end");
                }
                None => {
                    w.l(3, "# No argument of this kind offers a log sink: STDERR it is.");
                    w.l(3, &format!("Azul::Native.callback_raised(\"{}\", __ex) {{ false }}", tdn));
                }
            }
            w.l(2, "end");
            if data_first {
                w.l(2, "__drop = __c0");
                w.l(2, "LibAzul.azRefAny_delete(pointerof(__drop))");
            }
            if !void {
                w.l(2, "__result");
            }
            w.l(1, "end");
            w.l(0, "");
        }
        w.l(0, "end");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // Container conversions
    // ------------------------------------------------------------------------

    fn emit_conversions(&mut self, w: &mut W) {
        let m = self.m;
        w.l(0, "# :nodoc:");
        w.l(0, "# Native <-> FFI conversions for every Option / Vec / Result shape.");
        w.l(0, "module Azul::Conv");
        for c in m.classes.values() {
            match m.owned(&c.name) {
                t @ Ty::Option { .. } => self.emit_option_conv(w, c, &t),
                t @ Ty::Vec { .. } => self.emit_vec_conv(w, &t),
                t @ Ty::Result { .. } => self.emit_result_conv(w, c, &t),
                _ => {}
            }
        }
        w.l(0, "end");
        w.l(0, "");
    }

    fn emit_option_conv(&mut self, w: &mut W, c: &ClassInfo, t: &Ty) {
        let m = self.m;
        let (Ty::Option { name, payload }, Shape::Union { tag, variants }) = (t, &c.shape) else {
            return;
        };
        let none = variants.iter().find(|v| v.name == "None").unwrap();
        let some = variants.iter().find(|v| v.name == "Some").unwrap();
        let payload_lib = m.lib_owned(&some.fields[0].type_name);
        let tag = qualify(tag);
        let (Some(restr), Some(exact), Some(in_s), Some(out_e), Some(take_e)) = (
            restriction(payload),
            exact(payload),
            m.in_setup(payload, "x", "__p"),
            m.out_expr(payload, "__pp"),
            m.take_expr(payload, "__p"),
        ) else {
            return;
        };
        w.l(1, &format!("def self.wrap_{}(payload : {}) : {}", name, payload_lib, lib(name)));
        w.l(2, &format!("__v = {}.new", variant_struct(name, some)));
        w.l(2, &format!("__v.tag = {}.new({})", tag, some.index));
        w.l(2, "__v.payload = payload");
        w.l(2, &format!("__r = {}.new", lib(name)));
        w.l(2, &format!("__r.{} = __v", some.member));
        w.l(2, "__r");
        w.l(1, "end");
        w.l(0, "");
        w.l(1, &format!("def self.in_{}(x : ({} | ::Nil)) : {}", name, restr, lib(name)));
        w.l(2, "if x.nil?");
        w.l(3, &format!("__v = {}.new", variant_struct(name, none)));
        w.l(3, &format!("__v.tag = {}.new({})", tag, none.index));
        w.l(3, &format!("__r = {}.new", lib(name)));
        w.l(3, &format!("__r.{} = __v", none.member));
        w.l(3, "__r");
        w.l(2, "else");
        w.l(3, &in_s);
        w.l(3, &format!("wrap_{}(__p)", name));
        w.l(2, "end");
        w.l(1, "end");
        w.l(0, "");
        w.l(1, &format!("def self.out_{}(p : ::Pointer({})) : {}?", name, lib(name), exact));
        w.l(
            2,
            &format!(
                "return nil if p.as(::Pointer({})).value.tag == {}",
                variant_struct(name, none),
                none.index
            ),
        );
        w.l(
            2,
            &format!(
                "__pp = (p.as(::Pointer(::UInt8)) + offsetof({}, @payload)).as(::Pointer({}))",
                variant_struct(name, some),
                payload_lib
            ),
        );
        w.l(2, &out_e);
        w.l(1, "end");
        w.l(0, "");
        w.l(1, &format!("def self.take_{}(v : {}) : {}?", name, lib(name), exact));
        w.l(2, "__v = v");
        w.l(2, &format!("return nil if __v.{}.tag == {}", none.member, none.index));
        w.l(2, &format!("__p = __v.{}.payload", some.member));
        w.l(2, &take_e);
        w.l(1, "end");
        w.l(0, "");
    }

    fn emit_vec_conv(&mut self, w: &mut W, t: &Ty) {
        let m = self.m;
        let Ty::Vec { name, elem } = t else { return };
        let Some(Shape::Struct(fields)) = m.classes.get(name).map(|c| &c.shape) else {
            return;
        };
        let elem_name = &fields.iter().find(|f| f.name == "ptr").unwrap().type_name;
        let elem_lib = m.lib_owned(elem_name);
        let copy_fn = m.fun(name, "copyFromPtr").unwrap();
        let (Some(restr), Some(exact)) = (restriction(t), exact(t)) else {
            return;
        };
        let in_setup = match elem.as_ref() {
            Ty::Prim(Prim::U8) => Some(String::new()),
            Ty::Class(_) => Some("__t = __e.to_unsafe.value".to_string()),
            other => m.in_setup(other, "__e", "__t"),
        };
        let out_e = match elem.as_ref() {
            Ty::Prim(Prim::U8) => Some(String::new()),
            other => m.out_expr(other, "(__v.ptr + __i)"),
        };
        let (Some(in_setup), Some(out_e)) = (in_setup, out_e) else {
            return;
        };
        // native -> FFI
        w.l(1, &format!("def self.in_{}(x : {}) : {}", name, restr, lib(name)));
        if matches!(elem.as_ref(), Ty::Prim(Prim::U8)) {
            w.l(2, &format!("{}(x.to_unsafe, ::LibC::SizeT.new(x.size))", fun(&copy_fn)));
        } else {
            w.l(2, "__items = x.to_a");
            w.l(2, "__n = __items.size");
            w.l(2, &format!("__buf = ::Pointer({}).malloc(__n)", elem_lib));
            w.l(2, "__items.each_with_index do |__e, __i|");
            w.l(3, &in_setup);
            w.l(3, "__buf[__i] = __t");
            w.l(2, "end");
            w.l(2, &format!("__r = {}(__buf, ::LibC::SizeT.new(__n))", fun(&copy_fn)));
            if !matches!(elem.as_ref(), Ty::Class(_)) {
                if let Some(cl) = m.cleanup(elem, "__buf + __j") {
                    w.l(2, &format!("__n.times {{ |__j| {} }}", cl));
                }
            }
            w.l(2, "__r");
        }
        w.l(1, "end");
        w.l(0, "");
        // FFI (borrowed) -> native
        w.l(1, &format!("def self.out_{}(p : ::Pointer({})) : {}", name, lib(name), exact));
        w.l(2, "__v = p.value");
        w.l(2, "__n = __v.len.to_i");
        if matches!(elem.as_ref(), Ty::Prim(Prim::U8)) {
            w.l(2, "::Bytes.new(__n) { |__i| __v.ptr[__i] }");
        } else {
            w.l(2, &format!("{}.new(__n) {{ |__i| {} }}", exact, out_e));
        }
        w.l(1, "end");
        w.l(0, "");
        // FFI (owned) -> native
        w.l(1, &format!("def self.take_{}(v : {}) : {}", name, lib(name), exact));
        w.l(2, "__v = v");
        w.l(2, &format!("__r = out_{}(pointerof(__v))", name));
        if let Some(d) = m.delete_fn(name) {
            w.l(2, &format!("{}(pointerof(__v))", fun(&d)));
        }
        w.l(2, "__r");
        w.l(1, "end");
        w.l(0, "");
    }

    fn emit_result_conv(&mut self, w: &mut W, c: &ClassInfo, t: &Ty) {
        let m = self.m;
        let (Ty::Result { name, ok, err }, Shape::Union { variants, .. }) = (t, &c.shape) else {
            return;
        };
        let okv = variants.iter().find(|v| v.name == "Ok").unwrap();
        let errv = variants.iter().find(|v| v.name == "Err").unwrap();
        let (Some(okx), Some(errx), Some(take_ok), Some(take_err)) = (
            exact(ok),
            exact(err),
            m.take_expr(ok, "__p"),
            m.take_expr(err, "__e"),
        ) else {
            return;
        };
        w.l(1, &format!("def self.take_{}(v : {}) : {}", name, lib(name), okx));
        w.l(2, "__v = v");
        w.l(2, &format!("if __v.{}.tag == {}", okv.member, okv.index));
        w.l(3, &format!("__p = __v.{}.payload", okv.member));
        w.l(3, &take_ok);
        w.l(2, "else");
        w.l(3, &format!("__e = __v.{}.payload", errv.member));
        w.l(3, &format!("raise Azul::ResultError({}).new({})", errx, take_err));
        w.l(2, "end");
        w.l(1, "end");
        w.l(0, "");
    }
}

#[derive(Clone)]
struct CallbackArg<'a> {
    td: &'a CallbackTypedefDef,
    wrapper: Option<(String, String, String)>,
}

impl Ty {
    fn is_ref_any(&self) -> bool {
        matches!(self, Ty::RefAny)
    }
}

/// The IR puts `self` first for exactly these two kinds.
fn is_instance(f: &FunctionDef) -> bool {
    matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut) && !f.args.is_empty()
}

fn arg_name(n: &str) -> String {
    let s = sanitize_identifier(n);
    if s.starts_with(|c: char| c.is_ascii_uppercase()) {
        format!("_{}", s)
    } else {
        s
    }
}

fn variant_struct(union: &str, v: &Variant) -> String {
    format!("LibAzul::Az{}Variant_{}", union, enum_member_name(&v.name))
}

fn variant_class(union: &str, v: &Variant) -> String {
    format!("Azul::{}::{}", union, enum_member_name(&v.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualify_spells_lib_types_outside_the_lib_block() {
        assert_eq!(qualify("AzDom"), "LibAzul::AzDom");
        assert_eq!(qualify("AzDom*"), "::Pointer(LibAzul::AzDom)");
        assert_eq!(qualify("Void*"), "::Pointer(::Void)");
        assert_eq!(qualify("UInt8**"), "::Pointer(::Pointer(::UInt8))");
        assert_eq!(qualify("LibC::SizeT"), "::LibC::SizeT");
        assert_eq!(qualify("Float32[4]"), "::StaticArray(::Float32, 4)");
    }

    #[test]
    fn snake_case_of_variant_constructor_names() {
        assert_eq!(snake("mouseUp"), "mouse_up");
        assert_eq!(snake("create_body"), "create_body");
        assert_eq!(snake("px"), "px");
    }

    #[test]
    fn object_contract_names_are_never_generated() {
        for n in ["hash", "class", "clone", "to_s", "as", "nil?", "__take", "default"] {
            assert!(reserved_method(n), "{n} must be reserved");
        }
        assert!(!reserved_method("with_css"));
        assert!(!reserved_method("select"));
    }

    #[test]
    fn native_types_at_the_boundary() {
        let opt = Ty::Option {
            name: "OptionString".into(),
            payload: Box::new(Ty::Str),
        };
        assert_eq!(restriction(&opt).as_deref(), Some("(::String | ::Nil)"));
        let bytes = Ty::Vec {
            name: "U8Vec".into(),
            elem: Box::new(Ty::Prim(Prim::U8)),
        };
        assert_eq!(exact(&bytes).as_deref(), Some("::Bytes"));
        let dom_vec = Ty::Vec {
            name: "DomVec".into(),
            elem: Box::new(Ty::Class("Dom".into())),
        };
        assert_eq!(exact(&dom_vec).as_deref(), Some("::Array(Azul::Dom)"));
        assert_eq!(restriction(&dom_vec).as_deref(), Some("::Enumerable(Azul::Dom)"));
        let res = Ty::Result {
            name: "ResultXmlXmlError".into(),
            ok: Box::new(Ty::Class("Xml".into())),
            err: Box::new(Ty::Class("XmlError".into())),
        };
        assert_eq!(exact(&res).as_deref(), Some("Azul::Xml"));
        assert_eq!(restriction(&Ty::Prim(Prim::U32)).as_deref(), Some("::Int"));
        assert_eq!(exact(&Ty::Prim(Prim::U32)).as_deref(), Some("::UInt32"));
    }
}
