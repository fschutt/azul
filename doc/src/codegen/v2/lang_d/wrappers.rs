//! The D declarations: a struct per api.json type (a refcounted handle, or a
//! plain struct for `Copy` types), a D `enum` per fieldless enum, with native
//! values (`string`, `Nullable!T`, `T[]`, exceptions) at every member boundary
//! the IR shape allows. See `model.rs` for the type mapping and `runtime.rs` for
//! the handle boxes.
//!
//! # Ownership, in one paragraph
//!
//! A handle owns its value (the last copy runs `_delete`) or is a view into a
//! field of another value, which it keeps alive. Copying a handle shares the
//! value. Passing a handle BY VALUE moves the value into libazul, like Rust:
//! every copy of it throws `AzulMovedError` afterwards; a view is copied with
//! `_clone` instead, since a field cannot be moved out of. A `withX` builder that
//! consumes `self` and returns `Self` rebuilds the value in place and returns
//! `this`, so chains read naturally and the variable stays valid. Anything
//! converted from a native value (a `string`, an array) is a fresh C value the
//! call either consumes or the wrapper frees right after it.
//!
//! # Callbacks
//!
//! A callback parameter takes any callable with typed parameters (function,
//! delegate, lambda). The C side is one non-capturing `extern(C)` trampoline per
//! callback typedef (`azul.trampolines`); the callable travels, type-erased to a
//! delegate over the C arguments, in a `RefAny` handle - in the callback's `ctx`
//! (read back through the info type's `getCtx`) and/or in the data `RefAny`
//! passed next to it. The data argument itself is any class object, handed back
//! to the callable with its static type `T`.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    super::{
        ir::{
            ArgRefKind, CallbackTypedefDef, FunctionArg, FunctionDef, FunctionKind, TypeCategory,
        },
        managed_host_invoker::{
            callback_ctx_field, has_callback_wrapper_arg, layout_callback_factory_info,
        },
    },
    camel, d_type_name,
    model::{ClassInfo, EnumInfo, Field, Kind, Model, Prim, Shape, Ty, Variant},
    sanitize_identifier,
};

/// Suffix of a skipped function whose D name and parameter types another
/// member already has.
pub const TAKEN: &str = "(same name and parameter types as another member)";
/// Suffix of a function of a natively mapped type.
pub const NATIVE: &str = "(native D type)";

// ============================================================================
// Text accumulation
// ============================================================================

#[derive(Default)]
pub struct W {
    pub out: String,
}

impl W {
    fn l(&mut self, depth: usize, text: &str) {
        if !text.is_empty() {
            for _ in 0..depth {
                self.out.push_str("    ");
            }
            self.out.push_str(text);
        }
        self.out.push('\n');
    }

    fn doc(&mut self, depth: usize, lines: &[String]) {
        for d in lines {
            for part in d.split('\n') {
                let part = part.trim_end().replace('\r', "");
                if part.is_empty() {
                    self.l(depth, "///");
                } else {
                    self.l(depth, &format!("/// {}", part));
                }
            }
        }
    }
}

/// Member names a generated type must not declare: what the runtime and the
/// traits use, and D's own.
fn reserved_member(name: &str) -> bool {
    matches!(
        name,
        "init"
            | "sizeof"
            | "alignof"
            | "mangleof"
            | "stringof"
            | "tupleof"
            | "toString"
            | "toHash"
            | "dup"
            | "idup"
            | "defaultValue"
            | "tag"
            | "Tag"
            | "downcast"
            | "ctor"
            | "dtor"
    ) || name.starts_with('_')
        || (name.len() > 2
            && name.starts_with("op")
            && name[2..].starts_with(|c: char| c.is_ascii_uppercase()))
}

/// D member name for an api.json field / method / variant name.
fn member_name(raw: &str) -> String {
    let mut n = camel(raw);
    if n.is_empty() || n.starts_with(|c: char| c.is_ascii_digit()) {
        n = format!("v{}", n);
    }
    if reserved_member(&n) {
        n.push('_');
    }
    sanitize_identifier(&n)
}

/// The spelling of an owned C value of shape `t`.
fn c_type(t: &Ty) -> Option<String> {
    Some(match t {
        Ty::Void => "void".to_string(),
        Ty::Prim(p) => p.d().to_string(),
        Ty::RawPtr(p) => p.clone(),
        Ty::Str => "AzString".to_string(),
        Ty::RefAny => "AzRefAny".to_string(),
        Ty::Enum(n)
        | Ty::Plain(n)
        | Ty::Class(n)
        | Ty::Callback(n)
        | Ty::Option { name: n, .. }
        | Ty::Vec { name: n, .. }
        | Ty::Result { name: n, .. } => format!("Az{}", n),
        Ty::Unsupported(_) => return None,
    })
}

/// The D type a value of shape `t` has when read.
fn exact(t: &Ty) -> Option<String> {
    Some(match t {
        Ty::Void => "void".to_string(),
        Ty::Prim(p) => p.d().to_string(),
        Ty::RawPtr(p) => p.clone(),
        Ty::Str => "string".to_string(),
        Ty::RefAny => "RefAny".to_string(),
        Ty::Enum(n) | Ty::Plain(n) | Ty::Class(n) => d_type_name(n),
        Ty::Option { payload, .. } => format!("Nullable!({})", exact(payload)?),
        Ty::Vec { elem, .. } => format!("{}[]", exact(elem)?),
        Ty::Result { ok, .. } => exact(ok)?,
        Ty::Callback(n) => format!("Az{}", n),
        Ty::Unsupported(_) => return None,
    })
}

/// The D type an argument of shape `t` accepts.
fn restriction(t: &Ty) -> Option<String> {
    Some(match t {
        Ty::RefAny => "Object".to_string(),
        Ty::Option { payload, .. } => format!("Nullable!({})", restriction(payload)?),
        Ty::Vec { elem, .. } => match elem.as_ref() {
            Ty::RefAny => "Object[]".to_string(),
            Ty::Prim(_) | Ty::Enum(_) | Ty::Str | Ty::Plain(_) | Ty::Class(_) => {
                format!("const({})[]", restriction(elem)?)
            }
            e => format!("{}[]", restriction(e)?),
        },
        Ty::Result { .. } => return None,
        other => exact(other)?,
    })
}

// ============================================================================
// Conversions
// ============================================================================

impl<'a> Model<'a> {
    /// Native value `x` -> owned C value. Consumes (moves) handles.
    pub(super) fn in_expr(&self, t: &Ty, x: &str) -> Option<String> {
        Some(match t {
            Ty::Prim(_) | Ty::Callback(_) | Ty::RawPtr(_) => x.to_string(),
            Ty::Enum(n) => format!("cast(Az{}) {}", n, x),
            Ty::Plain(n) => format!("cast(Az{}) {}._raw", n, x),
            Ty::Class(_) => format!("{}._take()", x),
            Ty::Str => format!("_azulString({})", x),
            Ty::RefAny => format!("_azulRefAny(cast(Object) {})", x),
            Ty::Option { name, .. } | Ty::Vec { name, .. } => format!("_in_{}({})", name, x),
            Ty::Result { .. } | Ty::Void | Ty::Unsupported(_) => return None,
        })
    }

    /// Native value `x` -> a C value for a call that only BORROWS it: handles
    /// are read bitwise (not moved, not copied) and everything converted is
    /// fresh; `release` frees exactly the fresh parts afterwards.
    pub(super) fn borrow_expr(&self, t: &Ty, x: &str) -> Option<String> {
        Some(match t {
            Ty::Class(n) => format!("*cast(Az{}*) {}._ptr()", n, x),
            Ty::Option { name, payload } if self.moves(payload) => {
                format!("_borrow_{}({})", name, x)
            }
            other => self.in_expr(other, x)?,
        })
    }

    /// Frees the fresh parts of a value built by `borrow_expr`, behind `p`.
    pub(super) fn release(&self, t: &Ty, p: &str) -> Option<String> {
        match t {
            Ty::Class(_)
            | Ty::Prim(_)
            | Ty::Enum(_)
            | Ty::Plain(_)
            | Ty::Callback(_)
            | Ty::RawPtr(_) => None,
            Ty::Option { name, payload } if self.moves(payload) => {
                Some(format!("_release_{}({});", name, p))
            }
            other => self.cleanup(other, p),
        }
    }

    /// Owned C value `v` -> native value.
    pub(super) fn take_expr(&self, t: &Ty, v: &str) -> Option<String> {
        Some(match t {
            Ty::Prim(_) | Ty::Callback(_) | Ty::RawPtr(_) => v.to_string(),
            Ty::Enum(n) => format!("cast({}) {}", d_type_name(n), v),
            Ty::Plain(n) | Ty::Class(n) => format!("{}._own({})", d_type_name(n), v),
            Ty::Str => format!("_azulTakeString({})", v),
            Ty::RefAny => format!("RefAny._own({})", v),
            Ty::Option { name, .. } | Ty::Vec { name, .. } | Ty::Result { name, .. } => {
                format!("_take_{}({})", name, v)
            }
            Ty::Void | Ty::Unsupported(_) => return None,
        })
    }

    /// Borrowed C value behind pointer `p` (possibly const) -> independent
    /// native value.
    pub(super) fn out_expr(&self, t: &Ty, p: &str) -> Option<String> {
        Some(match t {
            Ty::Prim(_) => format!("*{}", p),
            Ty::Callback(n) => format!("cast(Az{}) *{}", n, p),
            Ty::RawPtr(r) => format!("cast({}) *{}", r, p),
            Ty::Enum(n) => format!("cast({}) *{}", d_type_name(n), p),
            Ty::Plain(n) => format!("{}._own(cast(Az{}) *{})", d_type_name(n), n, p),
            Ty::Str => format!("_azulStringOf({})", p),
            Ty::RefAny => format!("RefAny._own(AzRefAny_clone(cast(AzRefAny*) {}))", p),
            Ty::Class(n) if self.is_copy(n) => {
                format!("{}._own(cast(Az{}) *{})", d_type_name(n), n, p)
            }
            Ty::Class(n) => {
                let f = self.clone_fn(n)?;
                format!("{}._own({}(cast(Az{}*) {}))", d_type_name(n), f, n, p)
            }
            Ty::Option { name, .. } | Ty::Vec { name, .. } => format!("_out_{}({})", name, p),
            _ => return None,
        })
    }

    /// Frees a fresh C value (behind pointer `p`).
    pub(super) fn cleanup(&self, t: &Ty, p: &str) -> Option<String> {
        let name = match t {
            Ty::Str => "String",
            Ty::RefAny => "RefAny",
            Ty::Option { name, .. } | Ty::Vec { name, .. } => name.as_str(),
            Ty::Class(n) if !self.is_copy(n) => n.as_str(),
            _ => return None,
        };
        self.fun(name, "delete").map(|f| format!("{}({});", f, p))
    }

    /// Type of a returned value: `*const T` / `&T` returns stay pointers.
    pub(super) fn ret_ty(&self, ret: Option<&str>) -> Ty {
        let Some(r) = ret else { return Ty::Void };
        let r = r.trim();
        for prefix in ["*const ", "*mut ", "&mut ", "&"] {
            if let Some(inner) = r.strip_prefix(prefix) {
                return Ty::RawPtr(super::ptr_to_d(inner, self.ir));
            }
        }
        self.owned(r)
    }
}

// ============================================================================
// Member identity
// ============================================================================

/// D rejects two members with the same name and parameter types (static or
/// not, template or not).
#[derive(Default)]
struct Taken {
    set: BTreeSet<(String, String)>,
}

impl Taken {
    fn is_free(&self, name: &str, sig: &str) -> bool {
        !self.set.contains(&(name.to_string(), sig.to_string()))
    }

    fn take(&mut self, name: &str, sig: &str) {
        self.set.insert((name.to_string(), sig.to_string()));
    }
}

/// One planned parameter.
#[derive(Clone)]
struct Param {
    name: String,
    /// The D spelling, `ref ` included.
    ty: String,
    /// How the compile check builds an argument for it.
    check: CheckArg,
}

#[derive(Clone)]
enum CheckArg {
    /// A default-initialized local of this type.
    Local(String),
    /// A lambda literal.
    Callable(String),
}

fn sig_of(params: &[Param]) -> String {
    params
        .iter()
        .map(|p| p.ty.trim_start_matches("ref ").to_string())
        .collect::<Vec<_>>()
        .join(",")
}

// ============================================================================
// Emitter
// ============================================================================

pub struct Output {
    /// D source per api.json module (declarations + their conversions).
    pub modules: BTreeMap<String, String>,
    /// The callback trampolines.
    pub trampolines: String,
    /// The body of the compile check.
    pub check: String,
    /// api.json functions with no D-side shape.
    pub skipped: Vec<String>,
    pub stats: Stats,
}

#[derive(Default, Debug, Clone)]
pub struct Stats {
    pub options_native: usize,
    pub options_total: usize,
    pub vecs_native: usize,
    pub vecs_total: usize,
    pub results_native: usize,
    pub results_total: usize,
    pub unions: usize,
    pub classes: usize,
    pub structs: usize,
    pub enums: usize,
    pub members: usize,
}

struct Emitter<'m, 'a> {
    m: &'m Model<'a>,
    native_names: BTreeMap<String, String>,
    trampolines: BTreeSet<String>,
    skipped: Vec<String>,
    stats: Stats,
    check: W,
}

pub fn generate(m: &Model) -> Output {
    let native_names = m
        .classes
        .keys()
        .filter(|n| m.is_native(n))
        .filter_map(|n| exact(&m.owned(n)).map(|x| (n.clone(), x)))
        .collect();
    let mut e = Emitter {
        m,
        native_names,
        trampolines: BTreeSet::new(),
        skipped: Vec::new(),
        stats: Stats::default(),
        check: W::default(),
    };
    e.check
        .l(0, "/// Stands in for application data in the calls below.");
    e.check.l(0, "final class _CheckData");
    e.check.l(0, "{");
    e.check.l(0, "}");
    e.check.l(0, "");
    e.check.l(0, "void main()");
    e.check.l(0, "{");
    e.check.l(0, "}");
    e.check.l(0, "");
    let mut modules: BTreeMap<String, W> = BTreeMap::new();

    for info in m.enums.values() {
        let w = modules.entry(module_key(&info.module)).or_default();
        e.emit_enum(w, info);
    }
    for c in m.classes.values() {
        e.count(c);
        let w = modules.entry(module_key(&c.module)).or_default();
        let owned = m.owned(&c.name);
        if m.is_native(&c.name) || matches!(owned, Ty::Result { .. }) {
            e.emit_native(w, c);
            if m.is_native(&c.name) {
                continue;
            }
        }
        e.emit_struct(w, c);
    }
    let mut tramp = W::default();
    e.emit_trampolines(&mut tramp);
    Output {
        modules: modules.into_iter().map(|(k, v)| (k, v.out)).collect(),
        trampolines: tramp.out,
        check: e.check.out,
        skipped: e.skipped,
        stats: e.stats,
    }
}

fn module_key(m: &str) -> String {
    if m.is_empty() {
        "misc".to_string()
    } else {
        m.to_string()
    }
}

/// What a method's receiver is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Recv {
    Handle,
    Plain,
    Enum,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MethodKind {
    /// `static T opCall(...)`.
    Ctor,
    Static,
    Instance,
    /// A fieldless enum's instance method: `Ret name(Enum self, ...)` (UFCS).
    FreeUfcs,
    /// A fieldless enum's static method: `Ret name(E0 : Enum)(...)`.
    FreeStatic,
}

struct Plan {
    c_name: String,
    doc: Vec<String>,
    kind: MethodKind,
    name: String,
    raw_name: String,
    tparams: Vec<String>,
    constraint: Vec<String>,
    params: Vec<Param>,
    self_param: Option<String>,
    ret_type: String,
    body: Vec<String>,
    trampolines: Vec<String>,
    /// The D type the member belongs to.
    owner: String,
}

impl Plan {
    fn sig(&self) -> String {
        let mut s = sig_of(&self.params);
        if let Some(sp) = &self.self_param {
            s = format!("{}|{}", sp.trim_start_matches("ref "), s);
        }
        if self.kind == MethodKind::FreeStatic {
            s = format!("{}!{}", self.owner, s);
        }
        s
    }

    fn fallback_name(&mut self) {
        if self.kind == MethodKind::Ctor {
            self.kind = MethodKind::Static;
        }
        self.name = self.raw_name.clone();
    }

    fn header(&self) -> String {
        let mut tparams = self.tparams.clone();
        if self.kind == MethodKind::FreeStatic {
            tparams.insert(0, format!("E0 : {}", self.owner));
        }
        let tp = if tparams.is_empty() {
            String::new()
        } else {
            format!("({})", tparams.join(", "))
        };
        let mut params: Vec<String> = Vec::new();
        if let Some(sp) = &self.self_param {
            params.push(sp.clone());
        }
        params.extend(self.params.iter().map(|p| format!("{} {}", p.ty, p.name)));
        let constraint = if self.constraint.is_empty() {
            String::new()
        } else {
            format!(" if ({})", self.constraint.join(" && "))
        };
        match self.kind {
            MethodKind::Ctor => format!(
                "static {} opCall{}({}){}",
                self.owner,
                tp,
                params.join(", "),
                constraint
            ),
            MethodKind::Static => format!(
                "static {} {}{}({}){}",
                self.ret_type,
                self.name,
                tp,
                params.join(", "),
                constraint
            ),
            MethodKind::Instance | MethodKind::FreeUfcs | MethodKind::FreeStatic => format!(
                "{} {}{}({}){}",
                self.ret_type,
                self.name,
                tp,
                params.join(", "),
                constraint
            ),
        }
    }

    /// The call the compile check makes.
    fn check_call(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut args = Vec::new();
        for (i, p) in self.params.iter().enumerate() {
            match &p.check {
                CheckArg::Local(ty) => {
                    out.push(format!("{} __p{};", ty, i));
                    args.push(format!("__p{}", i));
                }
                CheckArg::Callable(lambda) => args.push(lambda.clone()),
            }
        }
        let call = match self.kind {
            MethodKind::Ctor => format!("{}({})", self.owner, args.join(", ")),
            MethodKind::Static => format!("{}.{}({})", self.owner, self.name, args.join(", ")),
            MethodKind::Instance | MethodKind::FreeUfcs => {
                format!("__s.{}({})", self.name, args.join(", "))
            }
            MethodKind::FreeStatic => {
                format!("{}!({})({})", self.name, self.owner, args.join(", "))
            }
        };
        out.push(format!("{};", call));
        out
    }
}

#[derive(Clone)]
struct CallbackArg<'a> {
    td: &'a CallbackTypedefDef,
    /// (wrapper struct, cb field, ctx field) for a wrapper-struct argument.
    wrapper: Option<(String, String, String)>,
}

impl<'m, 'a> Emitter<'m, 'a> {
    /// Doc text with every natively mapped type name in its D spelling
    /// (`OptionDom` -> `Nullable!(Dom)`): those names do not exist in D.
    fn rw(&self, lines: &[String]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                let mut out = String::with_capacity(line.len());
                let mut word = String::new();
                let flush = |word: &mut String, out: &mut String| {
                    match self.native_names.get(word.as_str()) {
                        Some(x) => out.push_str(x),
                        None => out.push_str(word),
                    }
                    word.clear();
                };
                for ch in line.chars() {
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        word.push(ch);
                    } else {
                        flush(&mut word, &mut out);
                        out.push(ch);
                    }
                }
                flush(&mut word, &mut out);
                out
            })
            .collect()
    }

    fn count(&mut self, c: &ClassInfo) {
        let m = self.m;
        let native = m.owned(&c.name);
        let is_union = matches!(c.shape, Shape::Union { .. });
        let looks_option = is_union
            && c.name.starts_with("Option")
            && matches!(&c.shape, Shape::Union { variants, .. } if variants.len() == 2 && variants.iter().any(|v| v.name == "None"));
        let looks_result = is_union && c.name.starts_with("Result");
        let looks_vec =
            !is_union && c.name.ends_with("Vec") && m.fun(&c.name, "copyFromPtr").is_some();
        if looks_option {
            self.stats.options_total += 1;
            if matches!(native, Ty::Option { .. }) {
                self.stats.options_native += 1;
            }
        } else if looks_result {
            self.stats.results_total += 1;
            if matches!(native, Ty::Result { .. }) {
                self.stats.results_native += 1;
            }
        } else if looks_vec {
            self.stats.vecs_total += 1;
            if matches!(native, Ty::Vec { .. }) {
                self.stats.vecs_native += 1;
            }
        }
        if is_union && !m.is_native(&c.name) {
            self.stats.unions += 1;
        }
    }

    fn check_block(&mut self, fn_name: &str, subject: &str, lines: Vec<Vec<String>>) {
        let w = &mut self.check;
        w.l(0, &format!("void check_{}()", fn_name));
        w.l(0, "{");
        w.l(1, "if (false)");
        w.l(1, "{");
        w.l(2, &format!("{} __s;", subject));
        for block in lines {
            w.l(2, "{");
            for l in block {
                w.l(3, &l);
            }
            w.l(2, "}");
        }
        w.l(1, "}");
        w.l(0, "}");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // Native types: no declaration, only conversions.
    // ------------------------------------------------------------------------

    fn emit_native(&mut self, w: &mut W, c: &ClassInfo) {
        let m = self.m;
        match m.owned(&c.name) {
            Ty::Str => {
                w.l(
                    0,
                    "/// Rust `String` is D `string` at every boundary; `string` has its own",
                );
                w.l(0, "/// `==`, `<`, hashing, copies and `.init`.");
                w.l(0, &format!("alias {} = string;", c.name));
                w.l(0, "");
            }
            t @ Ty::Option { .. } => self.emit_option_conv(w, c, &t),
            t @ Ty::Vec { .. } => self.emit_vec_conv(w, c, &t),
            t @ Ty::Result { .. } => self.emit_result_conv(w, c, &t),
            _ => {}
        }
        if matches!(m.owned(&c.name), Ty::Result { .. }) {
            return;
        }
        for f in m.functions_of(&c.name) {
            if matches!(
                f.kind,
                FunctionKind::Constructor
                    | FunctionKind::StaticMethod
                    | FunctionKind::Method
                    | FunctionKind::MethodMut
            ) {
                self.skipped.push(format!("{} {}", f.c_name, NATIVE));
            }
        }
    }

    // ------------------------------------------------------------------------
    // Fieldless enums
    // ------------------------------------------------------------------------

    fn emit_enum(&mut self, w: &mut W, info: &EnumInfo) {
        if info.variants.is_empty() {
            return;
        }
        let m = self.m;
        self.stats.enums += 1;
        let name = &info.name;
        let dname = d_type_name(name);
        let cases = case_names(&info.variants);
        w.doc(0, &self.rw(&info.doc));
        w.l(0, &format!("enum {} : {}", dname, info.backing));
        w.l(0, "{");
        for (i, c) in cases.iter().enumerate() {
            w.l(1, &format!("{} = {},", c, i));
        }
        w.l(0, "}");
        w.l(0, "");
        let mut checks: Vec<Vec<String>> = Vec::new();
        if let Some(f) = m.trait_fn(name, FunctionKind::Default) {
            w.l(
                0,
                &format!(
                    "/// Rust `Default` for `{}`: `defaultValue!{}()`.",
                    dname, dname
                ),
            );
            w.l(0, &format!("{} defaultValue(T : {})()", dname, dname));
            w.l(0, "{");
            w.l(1, &format!("return cast({}) {}();", dname, f));
            w.l(0, "}");
            w.l(0, "");
            self.stats.members += 1;
            checks.push(vec![format!("auto __d = defaultValue!({})();", dname)]);
        }
        let mut taken = Taken::default();
        checks.extend(self.emit_methods(w, name, Recv::Enum, &mut taken));
        self.check_block(name, &dname, checks);
    }

    // ------------------------------------------------------------------------
    // Structs and tagged unions
    // ------------------------------------------------------------------------

    fn emit_struct(&mut self, w: &mut W, c: &ClassInfo) {
        let m = self.m;
        let name = &c.name;
        let dname = d_type_name(name);
        let raw = format!("Az{}", name);
        let recv = match c.kind {
            Kind::Class => {
                self.stats.classes += 1;
                Recv::Handle
            }
            Kind::Plain => {
                self.stats.structs += 1;
                Recv::Plain
            }
        };
        let mut checks: Vec<Vec<String>> = Vec::new();
        w.doc(0, &self.rw(&c.doc));
        w.l(0, &format!("struct {}", dname));
        w.l(0, "{");
        if let Shape::Union { variants, tag } = &c.shape {
            w.l(
                1,
                &format!(
                    "/// The variant a `{}` holds: `final switch (value.tag)` matches it.",
                    dname
                ),
            );
            w.l(1, &format!("enum Tag : {}", tag));
            w.l(1, "{");
            for (v, case) in variants.iter().zip(case_names(
                &variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>(),
            )) {
                if let Some(d) = &v.doc {
                    w.doc(2, &self.rw(std::slice::from_ref(d)));
                }
                w.l(2, &format!("{} = {},", case, v.index));
            }
            w.l(1, "}");
            w.l(0, "");
        }
        match recv {
            Recv::Handle => self.emit_handle_plumbing(w, c),
            _ => self.emit_plain_plumbing(w, c),
        }
        let mut taken = Taken::default();
        if c.category == TypeCategory::RefAny && recv == Recv::Handle {
            w.l(
                1,
                "/// Wraps any class object; libazul keeps it alive while it holds this RefAny.",
            );
            w.l(1, &format!("static {} opCall(Object object)", dname));
            w.l(1, "{");
            w.l(2, "return _own(_azulRefAny(object));");
            w.l(1, "}");
            w.l(0, "");
            w.l(
                1,
                "/// The object this RefAny holds, if it is a `T` (else null).",
            );
            w.l(1, "T downcast(T)() if (is(T == class))");
            w.l(1, "{");
            w.l(2, &format!("auto __h = _azulHeld(cast({}*) _ptr());", raw));
            w.l(2, "return __h is null ? null : cast(T) __h.object;");
            w.l(1, "}");
            w.l(0, "");
            taken.take("opCall", "Object");
            taken.take("downcast", "");
            self.stats.members += 2;
            checks.push(vec![
                "Object __o;".to_string(),
                format!("auto __r = {}(__o);", dname),
                "auto __d = __s.downcast!_CheckData();".to_string(),
            ]);
        }
        if let Shape::Union { variants, .. } = &c.shape {
            checks.extend(self.emit_variants(w, c, variants, recv, &mut taken));
        }
        checks.extend(self.emit_traits(w, c, recv, &mut taken));
        checks.extend(self.emit_methods(w, name, recv, &mut taken));
        if let Shape::Struct(fields) = &c.shape {
            checks.extend(self.emit_fields(w, c, fields, recv, &mut taken));
            if recv == Recv::Plain {
                checks.extend(self.emit_memberwise(w, c, fields, &mut taken));
            }
        }
        w.l(0, "}");
        w.l(0, "");
        self.check_block(name, &dname, checks);
    }

    fn emit_handle_plumbing(&mut self, w: &mut W, c: &ClassInfo) {
        let m = self.m;
        let name = &c.name;
        let dname = d_type_name(name);
        let raw = format!("Az{}", name);
        w.l(1, "package _AzulRc* _rc;");
        w.l(0, "");
        // A postblit, not a copy constructor: D hides every `static opCall` of a
        // struct that declares any constructor, and `Dom()` / `Button("x")`
        // are static opCalls.
        w.l(1, "/// Copies share the value.");
        w.l(1, "this(this) nothrow @nogc");
        w.l(1, "{");
        w.l(2, "_azulRetain(_rc);");
        w.l(1, "}");
        w.l(0, "");
        w.l(
            1,
            "/// Rust `Drop`: the last copy runs the type's destructor.",
        );
        w.l(1, "~this() nothrow @nogc");
        w.l(1, "{");
        w.l(2, "_azulRelease(_rc);");
        w.l(1, "}");
        w.l(0, "");
        let drop = match m.delete_fn(name) {
            Some(f) => {
                w.l(1, "package static void _drop(void* p) nothrow @nogc");
                w.l(1, "{");
                w.l(2, &format!("{}(cast({}*) p);", f, raw));
                w.l(1, "}");
                w.l(0, "");
                "&_drop"
            }
            None => "null",
        };
        w.l(
            1,
            &format!(
                "package static {} _own({} raw) nothrow @nogc @trusted",
                dname, raw
            ),
        );
        w.l(1, "{");
        w.l(2, &format!("{} r;", dname));
        w.l(
            2,
            &format!("r._rc = _azulOwn(&raw, {}.sizeof, {});", raw, drop),
        );
        w.l(2, "return r;");
        w.l(1, "}");
        w.l(0, "");
        w.l(
            1,
            &format!(
                "package static {} _viewOf({}* field, const(_AzulRc)* owner) nothrow @nogc",
                dname, raw
            ),
        );
        w.l(1, "{");
        w.l(2, &format!("{} r;", dname));
        w.l(2, "r._rc = _azulView(field, owner);");
        w.l(2, "return r;");
        w.l(1, "}");
        w.l(0, "");
        w.l(
            1,
            &format!("package inout({})* _ptr() inout nothrow @trusted", raw),
        );
        w.l(1, "{");
        w.l(
            2,
            &format!("return cast(inout({})*) _azulPtr(_rc, \"{}\");", raw, dname),
        );
        w.l(1, "}");
        w.l(0, "");
        w.l(1, &format!("package {} _take() nothrow", raw));
        w.l(1, "{");
        w.l(2, "auto __p = _ptr();");
        if m.is_copy(name) {
            w.l(2, "return *__p;");
        } else {
            w.l(2, "if (_rc.root is null)");
            w.l(2, "{");
            w.l(3, "_azulMarkMoved(_rc);");
            w.l(3, "return *__p;");
            w.l(2, "}");
            match m.clone_fn(name) {
                Some(f) => w.l(2, &format!("return {}(__p);", f)),
                None => w.l(2, &format!("_azulNotMovable(\"{}\");", dname)),
            }
        }
        w.l(1, "}");
        w.l(0, "");
        w.l(1, &format!("package {} _value() nothrow", raw));
        w.l(1, "{");
        w.l(2, "return *_ptr();");
        w.l(1, "}");
        w.l(0, "");
        w.l(1, &format!("package void _replace({} raw) nothrow", raw));
        w.l(1, "{");
        w.l(2, "*_ptr() = raw;");
        w.l(1, "}");
        w.l(0, "");
    }

    fn emit_plain_plumbing(&mut self, w: &mut W, c: &ClassInfo) {
        let dname = d_type_name(&c.name);
        let raw = format!("Az{}", c.name);
        w.l(1, &format!("package {} _raw;", raw));
        w.l(0, "");
        w.l(
            1,
            &format!("package static {} _own({} raw) nothrow @nogc", dname, raw),
        );
        w.l(1, "{");
        w.l(2, &format!("{} r = void;", dname));
        w.l(2, "r._raw = raw;");
        w.l(2, "return r;");
        w.l(1, "}");
        w.l(0, "");
        w.l(
            1,
            &format!("package inout({})* _ptr() inout return nothrow @nogc", raw),
        );
        w.l(1, "{");
        w.l(2, "return &_raw;");
        w.l(1, "}");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // Tagged union variants
    // ------------------------------------------------------------------------

    fn emit_variants(
        &mut self,
        w: &mut W,
        c: &ClassInfo,
        variants: &[Variant],
        recv: Recv,
        taken: &mut Taken,
    ) -> Vec<Vec<String>> {
        let m = self.m;
        let dname = d_type_name(&c.name);
        let raw = format!("Az{}", c.name);
        let cases = case_names(&variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>());
        let mut checks = Vec::new();
        let first = &variants[0];
        w.l(1, "/// The variant this value holds.");
        w.l(1, "Tag tag() const nothrow");
        w.l(1, "{");
        w.l(2, &format!("return cast(Tag) _ptr().{}.tag;", first.member));
        w.l(1, "}");
        w.l(0, "");
        taken.take("tag", "");
        self.stats.members += 1;
        checks.push(vec!["auto __t = __s.tag;".to_string()]);

        for (v, case) in variants.iter().zip(&cases) {
            // `isX`
            let is_name = format!("is{}", upper_first(case.trim_end_matches('_')));
            if taken.is_free(&is_name, "") {
                taken.take(&is_name, "");
                w.l(1, &format!("/// Whether this holds `{}`.", v.name));
                w.l(1, &format!("bool {}() const nothrow", is_name));
                w.l(1, "{");
                w.l(2, &format!("return tag == Tag.{};", case));
                w.l(1, "}");
                w.l(0, "");
                self.stats.members += 1;
                checks.push(vec![format!("auto __b = __s.{};", is_name)]);
            }

            // `static T x(payload)`
            let tys: Vec<Option<(Ty, String)>> = v
                .fields
                .iter()
                .map(|f| {
                    let t = m.not_result(m.field(f));
                    let r = restriction(&t)?;
                    m.in_expr(&t, "x")?;
                    Some((t, r))
                })
                .collect();
            if tys.iter().all(|t| t.is_some()) {
                let tys: Vec<(Ty, String)> = tys.into_iter().map(|t| t.unwrap()).collect();
                let params: Vec<Param> = v
                    .fields
                    .iter()
                    .zip(&tys)
                    .enumerate()
                    .map(|(i, (f, (_, r)))| Param {
                        name: if v.fields.len() == 1 {
                            "payload".to_string()
                        } else {
                            member_name(&f.name).replace("payload", &format!("payload{}", i))
                        },
                        ty: r.clone(),
                        check: CheckArg::Local(r.clone()),
                    })
                    .collect();
                let sig = sig_of(&params);
                if taken.is_free(case, &sig) {
                    taken.take(case, &sig);
                    w.l(1, &format!("/// A `{}` holding `{}`.", dname, v.name));
                    if let Some(d) = &v.doc {
                        w.doc(1, &self.rw(std::slice::from_ref(d)));
                    }
                    w.l(
                        1,
                        &format!(
                            "static {} {}({})",
                            dname,
                            case,
                            params
                                .iter()
                                .map(|p| format!("{} {}", p.ty, p.name))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    );
                    w.l(1, "{");
                    w.l(2, &format!("{} __v;", raw));
                    w.l(2, &format!("__v.{}.tag = {};", v.member, v.index));
                    for ((f, (t, _)), p) in v.fields.iter().zip(&tys).zip(&params) {
                        w.l(
                            2,
                            &format!(
                                "__v.{}.{} = {};",
                                v.member,
                                f.c_name,
                                m.in_expr(t, &p.name).unwrap()
                            ),
                        );
                    }
                    w.l(2, "return _own(__v);");
                    w.l(1, "}");
                    w.l(0, "");
                    self.stats.members += 1;
                    let mut block = Vec::new();
                    let mut args = Vec::new();
                    for (i, p) in params.iter().enumerate() {
                        block.push(format!("{} __p{};", p.ty, i));
                        args.push(format!("__p{}", i));
                    }
                    block.push(format!(
                        "auto __r = {}.{}({});",
                        dname,
                        case,
                        args.join(", ")
                    ));
                    checks.push(block);
                }
            }

            // payload accessor
            if v.fields.len() == 1 {
                let f = &v.fields[0];
                let path = format!("{}.{}", v.member, f.c_name);
                let guard = format!(
                    "if (tag != Tag.{}) throw new AzulVariantError(\"{}\", \"{}\");",
                    case, dname, case
                );
                if let Some(block) =
                    self.emit_accessor(w, c, recv, case, &path, f, Some(&guard), false, taken)
                {
                    checks.extend(block);
                }
            }
        }
        checks
    }

    // ------------------------------------------------------------------------
    // Fields
    // ------------------------------------------------------------------------

    fn emit_fields(
        &mut self,
        w: &mut W,
        c: &ClassInfo,
        fields: &[Field],
        recv: Recv,
        taken: &mut Taken,
    ) -> Vec<Vec<String>> {
        let mut checks = Vec::new();
        for f in fields {
            if f.name.starts_with('_') {
                continue;
            }
            let prop = member_name(&f.name);
            if let Some(block) =
                self.emit_accessor(w, c, recv, &prop, &f.c_name, f, None, true, taken)
            {
                checks.extend(block);
            }
        }
        checks
    }

    /// A getter (and for struct fields a setter) for the C member at `path`.
    #[allow(clippy::too_many_arguments)]
    fn emit_accessor(
        &mut self,
        w: &mut W,
        c: &ClassInfo,
        recv: Recv,
        prop: &str,
        path: &str,
        f: &Field,
        guard: Option<&str>,
        setter: bool,
        taken: &mut Taken,
    ) -> Option<Vec<Vec<String>>> {
        let m = self.m;
        let t = m.not_result(m.field(f));
        let access = format!("_ptr().{}", path);
        let mut checks = Vec::new();
        // (return type, attributes after the parameter list, return expression)
        let getter: Option<(String, &str, String)> = match &t {
            Ty::Class(n) if recv == Recv::Handle => Some((
                d_type_name(n),
                "",
                format!("{}._viewOf(&{}, _rc)", d_type_name(n), access),
            )),
            Ty::RefAny if recv == Recv::Handle => Some((
                "RefAny".to_string(),
                "",
                format!("RefAny._viewOf(&{}, _rc)", access),
            )),
            Ty::Prim(p) => Some((p.d().to_string(), " const nothrow", access.clone())),
            Ty::RawPtr(r) => Some((r.clone(), " nothrow", access.clone())),
            Ty::Callback(n) => Some((format!("Az{}", n), " nothrow", access.clone())),
            Ty::Enum(n) => Some((
                d_type_name(n),
                " const nothrow",
                format!("cast({}) {}", d_type_name(n), access),
            )),
            Ty::Plain(n) => Some((
                format!("ref {}", d_type_name(n)),
                " return",
                format!("*cast({}*) &{}", d_type_name(n), access),
            )),
            Ty::Str | Ty::Option { .. } | Ty::Vec { .. } => {
                let ex = exact(&t)?;
                Some((ex, " const", m.out_expr(&t, &format!("&{}", access))?))
            }
            _ => None,
        };
        let (ret, attrs, expr) = getter?;
        if !taken.is_free(prop, "") {
            return None;
        }
        taken.take(prop, "");
        if let Some(d) = &f.doc {
            w.doc(1, &self.rw(std::slice::from_ref(d)));
        }
        w.l(1, &format!("{} {}(){}", ret, prop, attrs));
        w.l(1, "{");
        if let Some(g) = guard {
            w.l(2, g);
        }
        w.l(2, &format!("return {};", expr));
        w.l(1, "}");
        w.l(0, "");
        self.stats.members += 1;
        checks.push(vec![format!("auto __g = __s.{};", prop)]);

        if !setter {
            return Some(checks);
        }
        let Some(r) = restriction(&t) else {
            return Some(checks);
        };
        let body: Vec<String> = match &t {
            Ty::Prim(_) | Ty::RawPtr(_) | Ty::Callback(_) => {
                vec![format!("{} = v;", access)]
            }
            Ty::Enum(n) => vec![format!("{} = cast(Az{}) v;", access, n)],
            Ty::Plain(n) => vec![format!("{} = cast(Az{}) v._raw;", access, n)],
            Ty::Class(_) | Ty::RefAny | Ty::Str | Ty::Option { .. } | Ty::Vec { .. }
                if recv == Recv::Handle || !matches!(t, Ty::Class(_) | Ty::RefAny) =>
            {
                let Some(e) = m.in_expr(&t, "v") else {
                    return Some(checks);
                };
                let ct = c_type(&t)?;
                let mut b = vec![format!("{} __v = {};", ct, e)];
                b.push(format!("auto __f = &{};", access));
                if let Some(d) = m.cleanup(&t, "__f") {
                    b.push(d);
                }
                b.push("*__f = __v;".to_string());
                b
            }
            _ => return Some(checks),
        };
        if !taken.is_free(prop, &r) {
            return Some(checks);
        }
        taken.take(prop, &r);
        w.l(1, &format!("void {}({} v)", prop, r));
        w.l(1, "{");
        for l in body {
            w.l(2, &l);
        }
        w.l(1, "}");
        w.l(0, "");
        self.stats.members += 1;
        checks.push(vec![format!("{} __p;", r), format!("__s.{} = __p;", prop)]);
        let _ = c;
        Some(checks)
    }

    /// `static T opCall(field, ...)` over every field of a plain struct.
    fn emit_memberwise(
        &mut self,
        w: &mut W,
        c: &ClassInfo,
        fields: &[Field],
        taken: &mut Taken,
    ) -> Vec<Vec<String>> {
        let m = self.m;
        let dname = d_type_name(&c.name);
        let raw = format!("Az{}", c.name);
        let mut params = Vec::new();
        let mut assigns = Vec::new();
        for f in fields {
            let t = m.not_result(m.field(f));
            let (Some(r), Some(_)) = (restriction(&t), m.in_expr(&t, "x")) else {
                return Vec::new();
            };
            let mut pname = member_name(&f.name);
            if pname == "v" {
                pname.push('_');
            }
            assigns.push(format!(
                "__v.{} = {};",
                f.c_name,
                m.in_expr(&t, &pname).unwrap()
            ));
            params.push(Param {
                name: pname,
                ty: r.clone(),
                check: CheckArg::Local(r),
            });
        }
        if params.is_empty() {
            return Vec::new();
        }
        let sig = sig_of(&params);
        if !taken.is_free("opCall", &sig) {
            return Vec::new();
        }
        taken.take("opCall", &sig);
        w.l(1, &format!("/// A `{}` from every field.", dname));
        w.l(
            1,
            &format!(
                "static {} opCall({})",
                dname,
                params
                    .iter()
                    .map(|p| format!("{} {}", p.ty, p.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
        w.l(1, "{");
        w.l(2, &format!("{} __v;", raw));
        for a in assigns {
            w.l(2, &a);
        }
        w.l(2, "return _own(__v);");
        w.l(1, "}");
        w.l(0, "");
        self.stats.members += 1;
        let mut block = Vec::new();
        let mut args = Vec::new();
        for (i, p) in params.iter().enumerate() {
            block.push(format!("{} __p{};", p.ty, i));
            args.push(format!("__p{}", i));
        }
        block.push(format!("auto __r = {}({});", dname, args.join(", ")));
        vec![block]
    }

    // ------------------------------------------------------------------------
    // Traits
    // ------------------------------------------------------------------------

    fn emit_traits(
        &mut self,
        w: &mut W,
        c: &ClassInfo,
        recv: Recv,
        taken: &mut Taken,
    ) -> Vec<Vec<String>> {
        let m = self.m;
        let name = &c.name;
        let dname = d_type_name(name);
        let raw = format!("Az{}", name);
        let t = |k| m.trait_fn(name, k);
        let this_ptr = format!("cast({}*) _ptr()", raw);
        let rhs_ptr = format!("cast({}*) rhs._ptr()", raw);
        let mut checks = Vec::new();

        if let Some(f) = t(FunctionKind::DebugToString) {
            w.l(1, "/// Rust `Debug`.");
            w.l(1, "string toString() const");
            w.l(1, "{");
            w.l(2, &format!("return _azulTakeString({}({}));", f, this_ptr));
            w.l(1, "}");
            w.l(0, "");
            taken.take("toString", "");
            self.stats.members += 1;
            checks.push(vec!["auto __t = __s.toString();".to_string()]);
        }
        if m.is_copy(name) || t(FunctionKind::DeepCopy).is_some() {
            w.l(1, "/// Rust `Clone`: an independent copy.");
            w.l(1, &format!("{} dup() const", dname));
            w.l(1, "{");
            if m.is_copy(name) {
                w.l(2, &format!("return _own(cast({}) *_ptr());", raw));
            } else {
                w.l(
                    2,
                    &format!(
                        "return _own({}({}));",
                        t(FunctionKind::DeepCopy).unwrap(),
                        this_ptr
                    ),
                );
            }
            w.l(1, "}");
            w.l(0, "");
            taken.take("dup", "");
            self.stats.members += 1;
            checks.push(vec!["auto __d = __s.dup();".to_string()]);
        }
        if let Some(f) = t(FunctionKind::PartialEq) {
            w.l(1, "/// Rust `PartialEq`.");
            w.l(1, &format!("bool opEquals(ref const {} rhs) const", dname));
            w.l(1, "{");
            w.l(2, &format!("return {}({}, {});", f, this_ptr, rhs_ptr));
            w.l(1, "}");
            w.l(0, "");
            w.l(1, &format!("/// ditto"));
            w.l(1, &format!("bool opEquals()(const {} rhs) const", dname));
            w.l(1, "{");
            w.l(2, "return opEquals(rhs);");
            w.l(1, "}");
            w.l(0, "");
            self.stats.members += 1;
            checks.push(vec![format!("bool __e = __s == {}.init;", dname)]);
        }
        let cmp = t(FunctionKind::Cmp);
        let pcmp = t(FunctionKind::PartialCmp);
        if let Some(f) = cmp.clone().or(pcmp) {
            let (ret, conv) = if cmp.is_some() {
                w.l(1, "/// Rust `Ord`.");
                ("int", "_azulOrdering")
            } else {
                w.l(
                    1,
                    "/// Rust `PartialOrd`: NaN when the two are not comparable.",
                );
                ("float", "_azulPartialOrdering")
            };
            w.l(1, &format!("{} opCmp(ref const {} rhs) const", ret, dname));
            w.l(1, "{");
            w.l(
                2,
                &format!("return {}({}({}, {}));", conv, f, this_ptr, rhs_ptr),
            );
            w.l(1, "}");
            w.l(0, "");
            w.l(1, "/// ditto");
            w.l(1, &format!("{} opCmp()(const {} rhs) const", ret, dname));
            w.l(1, "{");
            w.l(2, "return opCmp(rhs);");
            w.l(1, "}");
            w.l(0, "");
            self.stats.members += 1;
            checks.push(vec![format!("bool __l = __s < {}.init;", dname)]);
        }
        if let Some(f) = t(FunctionKind::Hash) {
            w.l(1, "/// Rust `Hash`.");
            w.l(1, "size_t toHash() const nothrow @trusted");
            w.l(1, "{");
            w.l(2, &format!("return cast(size_t) {}({});", f, this_ptr));
            w.l(1, "}");
            w.l(0, "");
            taken.take("toHash", "");
            self.stats.members += 1;
            checks.push(vec!["size_t __h = __s.toHash();".to_string()]);
        }
        if let Some(f) = t(FunctionKind::Default) {
            w.l(1, "/// Rust `Default`.");
            w.l(1, &format!("static {} defaultValue()", dname));
            w.l(1, "{");
            w.l(2, &format!("return _own({}());", f));
            w.l(1, "}");
            w.l(0, "");
            taken.take("defaultValue", "");
            self.stats.members += 1;
            checks.push(vec![
                format!("auto __d = {}.defaultValue();", dname),
                format!("auto __g = defaultValue!({})();", dname),
            ]);
            let zero_arg_create = m.functions_of(name).iter().any(|g| {
                matches!(
                    g.kind,
                    FunctionKind::Constructor | FunctionKind::StaticMethod
                ) && g.method_name == "create"
                    && g.args.is_empty()
            });
            if !zero_arg_create && taken.is_free("opCall", "") {
                taken.take("opCall", "");
                w.l(1, &format!("/// `{}()` is Rust `Default` too.", dname));
                w.l(1, &format!("static {} opCall()", dname));
                w.l(1, "{");
                w.l(2, "return defaultValue();");
                w.l(1, "}");
                w.l(0, "");
                self.stats.members += 1;
                checks.push(vec![format!("auto __o = {}();", dname)]);
            }
        }
        let _ = recv;
        checks
    }

    // ------------------------------------------------------------------------
    // Methods
    // ------------------------------------------------------------------------

    fn emit_methods(
        &mut self,
        w: &mut W,
        class: &str,
        recv: Recv,
        taken: &mut Taken,
    ) -> Vec<Vec<String>> {
        let m = self.m;
        let funcs: Vec<&FunctionDef> = m
            .functions_of(class)
            .iter()
            .copied()
            .filter(|f| {
                matches!(
                    f.kind,
                    FunctionKind::Constructor
                        | FunctionKind::StaticMethod
                        | FunctionKind::Method
                        | FunctionKind::MethodMut
                )
            })
            .collect();

        let mut plans: Vec<Plan> = Vec::new();
        for f in &funcs {
            match self.plan_method(class, recv, f) {
                Some(p) => plans.push(p),
                None => self.skipped.push(f.c_name.clone()),
            }
        }

        // A clash puts every claimant back to its api.json name.
        let mut claims: BTreeMap<(String, String), usize> = BTreeMap::new();
        for p in &plans {
            *claims.entry((p.name.clone(), p.sig())).or_default() += 1;
        }
        for p in plans.iter_mut() {
            let key = (p.name.clone(), p.sig());
            if claims[&key] > 1 || !taken.is_free(&key.0, &key.1) {
                p.fallback_name();
            }
        }

        let mut checks = Vec::new();
        for p in plans {
            if self.emit_func(w, &p, recv, taken) {
                checks.push(p.check_call());
                continue;
            }
            let mut p = p;
            let before = p.name.clone();
            p.fallback_name();
            if p.name != before && self.emit_func(w, &p, recv, taken) {
                checks.push(p.check_call());
            } else {
                self.skipped.push(format!("{} {}", p.c_name, TAKEN));
            }
        }
        checks
    }

    /// Emits a planned method if its name and parameter types are free.
    fn emit_func(&mut self, w: &mut W, p: &Plan, recv: Recv, taken: &mut Taken) -> bool {
        let sig = p.sig();
        let name = if p.kind == MethodKind::Ctor {
            "opCall"
        } else {
            p.name.as_str()
        };
        if !taken.is_free(name, &sig) {
            return false;
        }
        taken.take(name, &sig);
        for t in &p.trampolines {
            self.trampolines.insert(t.clone());
        }
        let depth = if recv == Recv::Enum { 0 } else { 1 };
        w.doc(depth, &self.rw(&p.doc));
        w.l(depth, &p.header());
        w.l(depth, "{");
        for s in &p.body {
            w.l(depth + 1, s);
        }
        w.l(depth, "}");
        w.l(0, "");
        self.stats.members += 1;
        true
    }

    /// Plan one method; None if some argument or the return value has no
    /// D-side shape.
    fn plan_method(&self, class: &str, recv: Recv, f: &FunctionDef) -> Option<Plan> {
        let m = self.m;
        let inst =
            matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut) && !f.args.is_empty();
        let args: Vec<&FunctionArg> = f.args.iter().skip(usize::from(inst)).collect();
        let dname = d_type_name(class);
        let raw = format!("Az{}", class);

        // ---- callbacks ---------------------------------------------------------
        let cb_args: Vec<(usize, CallbackArg)> = args
            .iter()
            .enumerate()
            .filter_map(|(i, a)| self.callback_of(a).map(|cb| (i, cb)))
            .collect();
        let refany_args: Vec<usize> = args
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.ref_kind == ArgRefKind::Owned && matches!(m.owned(&a.type_name), Ty::RefAny)
            })
            .map(|(i, _)| i)
            .collect();
        let layout_factory =
            if !inst && cb_args.len() == 1 && args.len() == 1 && recv == Recv::Handle {
                m.ir.find_struct(class)
                    .and_then(|s| layout_callback_factory_info(s, m.ir))
                    .filter(|li| {
                        f.return_type.as_deref() == Some(class)
                            && li.callback_wrapper == cb_args[0].1.td.name.trim_end_matches("Type")
                    })
            } else {
                None
            };

        struct CbPlan {
            arg: usize,
            closure: bool,
            data_first: bool,
            carry_in_data: Option<usize>,
            ctx_usable: bool,
        }
        let option_refany = matches!(m.owned("OptionRefAny"), Ty::Option { .. })
            && m.fun("OptionRefAny", "delete").is_some();
        let mut cplans: Vec<CbPlan> = Vec::new();
        let mut generic = false;
        for (i, cb) in &cb_args {
            let td = cb.td;
            let ctx_arg = td.args.iter().any(|a| {
                a.ref_kind == ArgRefKind::Owned && m.ctx_getter(a.type_name.trim()).is_some()
            });
            let data_first = td.args.first().is_some_and(|a| {
                a.ref_kind == ArgRefKind::Owned && matches!(m.owned(&a.type_name), Ty::RefAny)
            });
            let ctx_usable =
                ctx_arg && option_refany && (cb.wrapper.is_some() || layout_factory.is_some());
            let carry_in_data = (data_first && cb_args.len() == 1 && refany_args.len() == 1)
                .then(|| refany_args[0]);
            let closure = self.callback_convertible(td)
                && (ctx_usable || carry_in_data.is_some())
                && option_refany;
            if closure && data_first {
                generic = true;
            }
            cplans.push(CbPlan {
                arg: *i,
                closure,
                data_first,
                carry_in_data: if closure { carry_in_data } else { None },
                ctx_usable,
            });
        }
        let layout_factory = layout_factory.filter(|_| cplans.first().is_some_and(|p| p.closure));

        // The template parameter `T` (the application data's class): deduced
        // from the data argument when there is one, else from the callable.
        let data_arg: Option<usize> = cplans
            .iter()
            .find_map(|p| p.carry_in_data)
            .or_else(|| (generic && refany_args.len() == 1).then(|| refany_args[0]));
        let mut tparams: Vec<String> = Vec::new();
        let mut constraint: Vec<String> = Vec::new();
        if generic {
            match data_arg {
                Some(_) => {
                    tparams.push("T".to_string());
                    constraint.push("is(T == class)".to_string());
                }
                None => {}
            }
        }

        // ---- parameters + setup ----------------------------------------------------
        let mut params: Vec<Param> = Vec::new();
        let mut pre: Vec<String> = Vec::new();
        let mut body: Vec<String> = Vec::new();
        let mut post: Vec<String> = Vec::new();
        let mut call_args: Vec<String> = Vec::new();
        let mut trampolines: Vec<String> = Vec::new();
        let mut used_names: BTreeSet<String> = BTreeSet::new();
        used_names.insert("self".to_string());
        let mut t_from_callable = false;

        for (i, a) in args.iter().enumerate() {
            let mut pname = member_name(&a.name);
            if pname.is_empty() {
                pname = format!("arg{}", i);
            }
            while !used_names.insert(pname.clone()) {
                pname.push('_');
            }
            let local = format!("__a{}", i);

            if let Some(plan) = cplans.iter().find(|p| p.arg == i) {
                let cb = &cb_args.iter().find(|(j, _)| *j == i).unwrap().1;
                let td = cb.td;
                if plan.closure {
                    let fparam = format!("F{}", i);
                    let t_name = if plan.data_first && data_arg.is_none() && !t_from_callable {
                        // T comes from the callable's first parameter.
                        t_from_callable = true;
                        tparams.push(fparam.clone());
                        tparams.push(format!("T = Parameters!({})[0]", fparam));
                        constraint.push(format!("isCallable!({})", fparam));
                        constraint.push(format!(
                            "Parameters!({}).length == {}",
                            fparam,
                            td.args.len()
                        ));
                        constraint.push("is(T == class)".to_string());
                        "T"
                    } else {
                        tparams.push(fparam.clone());
                        "T"
                    };
                    let (user_args, user_ret) = self.user_signature(td, plan.data_first)?;
                    let inits: Vec<String> = user_args
                        .iter()
                        .map(|u| {
                            if plan.data_first && u == t_name {
                                "T.init".to_string()
                            } else {
                                format!("{}.init", u)
                            }
                        })
                        .collect();
                    let call = format!("{}.init({})", fparam, inits.join(", "));
                    if user_ret == "void" {
                        constraint.push(format!("is(typeof({}) == void)", call));
                    } else {
                        constraint.push(format!("is(typeof({}) : {})", call, user_ret));
                    }
                    let lambda_params: Vec<String> = user_args
                        .iter()
                        .enumerate()
                        .map(|(j, u)| {
                            let u = if plan.data_first && j == 0 {
                                "_CheckData".to_string()
                            } else {
                                u.clone()
                            };
                            format!("{} __x{}", u, j)
                        })
                        .collect();
                    let lambda = if user_ret == "void" {
                        format!("({}) {{ }}", lambda_params.join(", "))
                    } else {
                        format!("({}) => {}.init", lambda_params.join(", "), user_ret)
                    };
                    params.push(Param {
                        name: pname.clone(),
                        ty: fparam.clone(),
                        check: CheckArg::Callable(lambda),
                    });
                    let dg = self.erased_type(td)?;
                    pre.extend(self.erased_closure(
                        td,
                        &pname,
                        &format!("__erased{}", i),
                        plan.data_first,
                    )?);
                    trampolines.push(td.name.clone());
                    let tramp = format!("&_azulTrampoline_{}", td.name);
                    match &cb.wrapper {
                        Some((wstruct, cb_field, ctx_field)) => {
                            body.push(format!("Az{} {};", wstruct, local));
                            body.push(format!(
                                "{}.{} = {};",
                                local,
                                super::raw_identifier(cb_field),
                                tramp
                            ));
                            if plan.ctx_usable {
                                body.push(format!(
                                    "{}.{} = _wrap_OptionRefAny(_azulRefAny(null, new _AzulClosure!({})(__erased{})));",
                                    local,
                                    super::raw_identifier(ctx_field),
                                    dg,
                                    i
                                ));
                            } else {
                                body.push(format!(
                                    "{}.{} = _none_OptionRefAny();",
                                    local,
                                    super::raw_identifier(ctx_field)
                                ));
                            }
                            call_args.push(local.clone());
                        }
                        None => call_args.push(tramp),
                    }
                } else {
                    // Raw: a C function pointer.
                    let ty = format!("Az{}", td.name);
                    params.push(Param {
                        name: pname.clone(),
                        ty: ty.clone(),
                        check: CheckArg::Local(ty),
                    });
                    match &cb.wrapper {
                        Some((wstruct, cb_field, ctx_field)) => {
                            if !option_refany {
                                return None;
                            }
                            body.push(format!("Az{} {};", wstruct, local));
                            body.push(format!(
                                "{}.{} = {};",
                                local,
                                super::raw_identifier(cb_field),
                                pname
                            ));
                            body.push(format!(
                                "{}.{} = _none_OptionRefAny();",
                                local,
                                super::raw_identifier(ctx_field)
                            ));
                            call_args.push(local.clone());
                        }
                        None => call_args.push(pname.clone()),
                    }
                }
                continue;
            }

            // The application data, handed back to the callable as `T`.
            if data_arg == Some(i) {
                params.push(Param {
                    name: pname.clone(),
                    ty: "T".to_string(),
                    check: CheckArg::Local("_CheckData".to_string()),
                });
                let closure = cplans
                    .iter()
                    .find(|p| p.carry_in_data == Some(i))
                    .map(|p| {
                        let td = cb_args.iter().find(|(j, _)| *j == p.arg).unwrap().1.td;
                        format!(
                            "new _AzulClosure!({})(__erased{})",
                            self.erased_type(td).unwrap_or_default(),
                            p.arg
                        )
                    })
                    .unwrap_or_else(|| "null".to_string());
                body.push(format!(
                    "AzRefAny {} = _azulRefAny({}, {});",
                    local, pname, closure
                ));
                call_args.push(local.clone());
                continue;
            }

            let ty = m.not_result(m.owned(&a.type_name));
            if matches!(ty, Ty::Callback(_)) {
                return None;
            }
            match a.ref_kind {
                ArgRefKind::Owned => {
                    let r = restriction(&ty)?;
                    let e = m.in_expr(&ty, &pname)?;
                    params.push(Param {
                        name: pname.clone(),
                        ty: r.clone(),
                        check: CheckArg::Local(r),
                    });
                    body.push(format!("{} {} = {};", c_type(&ty)?, local, e));
                    call_args.push(local);
                }
                ArgRefKind::Ref | ArgRefKind::Ptr | ArgRefKind::RefMut | ArgRefKind::PtrMut
                    if matches!(ty, Ty::Class(_) | Ty::RefAny) =>
                {
                    let ex = exact(&ty)?;
                    params.push(Param {
                        name: pname.clone(),
                        ty: ex.clone(),
                        check: CheckArg::Local(ex),
                    });
                    call_args.push(format!("cast({}*) {}._ptr()", c_type(&ty)?, pname));
                }
                ArgRefKind::Ref => match &ty {
                    Ty::RawPtr(_)
                    | Ty::Void
                    | Ty::Unsupported(_)
                    | Ty::Callback(_)
                    | Ty::Result { .. } => return None,
                    _ => {
                        let e = m.borrow_expr(&ty, &pname)?;
                        let r = restriction(&ty)?;
                        params.push(Param {
                            name: pname.clone(),
                            ty: r.clone(),
                            check: CheckArg::Local(r),
                        });
                        body.push(format!("{} {} = {};", c_type(&ty)?, local, e));
                        call_args.push(format!("&{}", local));
                        if let Some(c) = m.release(&ty, &format!("&{}", local)) {
                            post.push(c);
                        }
                    }
                },
                ArgRefKind::RefMut => match &ty {
                    Ty::Prim(p) => {
                        params.push(Param {
                            name: pname.clone(),
                            ty: format!("ref {}", p.d()),
                            check: CheckArg::Local(p.d().to_string()),
                        });
                        call_args.push(format!("&{}", pname));
                    }
                    Ty::Plain(n) => {
                        params.push(Param {
                            name: pname.clone(),
                            ty: format!("ref {}", d_type_name(n)),
                            check: CheckArg::Local(d_type_name(n)),
                        });
                        call_args.push(format!("&{}._raw", pname));
                    }
                    Ty::Enum(_) | Ty::Str | Ty::Option { .. } | Ty::Vec { .. } => {
                        let ex = exact(&ty)?;
                        if restriction(&ty)? != ex && !matches!(ty, Ty::Vec { .. }) {
                            return None;
                        }
                        let e = m.in_expr(&ty, &pname)?;
                        let back = m.take_expr(&ty, &local)?;
                        params.push(Param {
                            name: pname.clone(),
                            ty: format!("ref {}", ex),
                            check: CheckArg::Local(ex),
                        });
                        body.push(format!("{} {} = {};", c_type(&ty)?, local, e));
                        call_args.push(format!("&{}", local));
                        post.push(format!("{} = {};", pname, back));
                    }
                    _ => return None,
                },
                ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                    let p = super::arg_type_for_ref_kind(&a.type_name, &a.ref_kind, m.ir);
                    params.push(Param {
                        name: pname.clone(),
                        ty: p.clone(),
                        check: CheckArg::Local(p),
                    });
                    call_args.push(pname.clone());
                }
            }
        }

        pre.append(&mut body);
        let mut body = pre;

        // ---- receiver --------------------------------------------------------------
        let recv_arg = if inst { f.args.first() } else { None };
        let ret = m.ret_ty(f.return_type.as_deref());
        let returns_self = matches!(&ret, Ty::Class(n) | Ty::Plain(n) if n == class);
        let builder = matches!(recv_arg, Some(r) if r.ref_kind == ArgRefKind::Owned)
            && returns_self
            && recv != Recv::Enum;
        let mut self_param = None;
        if let Some(r) = recv_arg {
            let own = r.ref_kind == ArgRefKind::Owned;
            let is_mut = matches!(r.ref_kind, ArgRefKind::RefMut | ArgRefKind::PtrMut);
            let s = match recv {
                Recv::Handle => {
                    if own {
                        body.push(format!(
                            "{} __self = {};",
                            raw,
                            if builder { "_value()" } else { "_take()" }
                        ));
                        "__self".to_string()
                    } else {
                        "_ptr()".to_string()
                    }
                }
                Recv::Plain => {
                    if own {
                        "_raw".to_string()
                    } else {
                        "&_raw".to_string()
                    }
                }
                Recv::Enum => {
                    self_param = Some(format!(
                        "{}{} self",
                        if is_mut { "ref " } else { "" },
                        dname
                    ));
                    if own {
                        format!("cast({}) self", raw)
                    } else {
                        body.push(format!("{} __self = cast({}) self;", raw, raw));
                        if is_mut {
                            post.push(format!("self = cast({}) __self;", dname));
                        }
                        "&__self".to_string()
                    }
                }
            };
            call_args.insert(0, s);
        }

        // ---- return + naming ---------------------------------------------------------
        let symbol = self.symbol(f, &cb_args);
        let call = format!("{}({})", symbol, call_args.join(", "));
        let throws = matches!(ret, Ty::Result { .. });
        let mut kind = if inst {
            if recv == Recv::Enum {
                MethodKind::FreeUfcs
            } else {
                MethodKind::Instance
            }
        } else if recv == Recv::Enum {
            MethodKind::FreeStatic
        } else {
            MethodKind::Static
        };
        let raw_name = member_name(&f.method_name);
        let mut name = raw_name.clone();
        let is_create = !inst && f.method_name == "create";
        let ctor = recv != Recv::Enum && is_create && returns_self && !throws;
        let arity = params.len();
        let ret_type: String;
        let mut doc = f.doc.clone();

        if layout_factory.is_some() || ctor {
            kind = MethodKind::Ctor;
            name = "opCall".to_string();
            ret_type = dname.clone();
            body.push(format!("{} __r = {};", raw, call));
            body.append(&mut post);
            if let Some(fac) = &layout_factory {
                let mut path: Vec<String> = fac
                    .field_path
                    .iter()
                    .map(|seg| super::raw_identifier(seg))
                    .collect();
                let ctx_field = callback_ctx_field(&fac.callback_wrapper, m.ir)
                    .unwrap_or_else(|| "ctx".to_string());
                path.push(super::raw_identifier(&ctx_field));
                let td = cb_args[0].1.td;
                body.push(format!("auto __o = {}._own(__r);", dname));
                body.push(format!("auto __ctx = &__o._ptr().{};", path.join(".")));
                body.push("AzOptionRefAny_delete(__ctx);".to_string());
                body.push(format!(
                    "*__ctx = _wrap_OptionRefAny(_azulRefAny(null, new _AzulClosure!({})(__erased0)));",
                    self.erased_type(td)?
                ));
                body.push("return __o;".to_string());
            } else {
                body.push("return _own(__r);".to_string());
            }
        } else {
            if builder {
                body.push(format!("{} __r = {};", raw, call));
                body.append(&mut post);
                match recv {
                    Recv::Handle => body.push("_replace(__r);".to_string()),
                    _ => body.push("_raw = __r;".to_string()),
                }
                body.push("return this;".to_string());
                ret_type = dname.clone();
            } else {
                match &ret {
                    Ty::Void => {
                        body.push(format!("{};", call));
                        body.append(&mut post);
                        ret_type = "void".to_string();
                    }
                    t => {
                        let a = exact(t)?;
                        let e = m.take_expr(t, "__r")?;
                        body.push(format!("{} __r = {};", c_type(t)?, call));
                        body.append(&mut post);
                        body.push(format!("return {};", e));
                        ret_type = a;
                        if let Ty::Result { err, .. } = t {
                            doc.push(format!(
                                "Throws: `ResultException!({})` when the Rust `Result` is `Err`.",
                                exact(err).unwrap_or_default()
                            ));
                        }
                    }
                }
            }
            if !inst {
                if let Some(rest) = f.method_name.strip_prefix("create_") {
                    name = member_name(rest);
                }
            } else {
                let raw_snake = &f.method_name;
                let getter_ok = arity == 0
                    && !throws
                    && !generic
                    && ret_type != "void"
                    && !matches!(recv_arg, Some(r) if r.ref_kind == ArgRefKind::Owned);
                if let (Some(rest), true) = (raw_snake.strip_prefix("get_"), getter_ok) {
                    name = member_name(rest);
                } else if let (Some(rest), 1, "void", false) = (
                    raw_snake.strip_prefix("set_"),
                    arity,
                    ret_type.as_str(),
                    generic,
                ) {
                    if cb_args.is_empty() && !params[0].ty.starts_with("ref ") {
                        name = member_name(rest);
                    }
                }
            }
        }

        Some(Plan {
            c_name: f.c_name.clone(),
            doc,
            kind,
            name,
            raw_name,
            tparams,
            constraint,
            params,
            self_param,
            ret_type,
            body,
            trampolines,
            owner: dname,
        })
    }

    /// The C symbol a method binds: the `Struct` variant when a wrapper-struct
    /// callback is passed to a function the header pairs.
    fn symbol(&self, f: &FunctionDef, cb_args: &[(usize, CallbackArg)]) -> String {
        if cb_args.iter().any(|(_, cb)| cb.wrapper.is_some()) && has_callback_wrapper_arg(f) {
            format!("{}Struct", f.c_name)
        } else {
            f.c_name.clone()
        }
    }

    /// Is `arg` a callback? Returns the typedef, and for a wrapper-struct
    /// argument (wrapper struct, cb field, ctx field).
    fn callback_of(&self, arg: &FunctionArg) -> Option<CallbackArg<'a>> {
        let m = self.m;
        if arg.ref_kind != ArgRefKind::Owned {
            return None;
        }
        let t = arg.type_name.trim();
        if let Some(td) = m.callbacks.get(t) {
            return Some(CallbackArg { td, wrapper: None });
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

    // ------------------------------------------------------------------------
    // Callback plumbing
    // ------------------------------------------------------------------------

    /// The C-level D type of callback arg `a`.
    fn cb_c_arg(&self, a: &FunctionArg) -> Option<String> {
        let m = self.m;
        match a.ref_kind {
            ArgRefKind::Owned => c_type(&m.owned(&a.type_name)),
            _ => Some(super::arg_type_for_ref_kind(
                &a.type_name,
                &a.ref_kind,
                m.ir,
            )),
        }
    }

    fn cb_c_ret(&self, td: &CallbackTypedefDef) -> Option<String> {
        match td.return_type.as_deref() {
            None => Some("void".to_string()),
            Some(r) => match self.m.owned(r) {
                Ty::Void => Some("void".to_string()),
                t => c_type(&t),
            },
        }
    }

    /// The D-side type of callback arg `j`.
    fn cb_user_arg(
        &self,
        td: &CallbackTypedefDef,
        j: usize,
        data_first: bool,
    ) -> Option<(String, Option<Ty>)> {
        let a = &td.args[j];
        if j == 0 && data_first {
            return Some(("T".to_string(), Some(Ty::RefAny)));
        }
        if a.ref_kind != ArgRefKind::Owned {
            return Some((self.cb_c_arg(a)?, None));
        }
        let ty = self.m.not_result(self.m.owned(&a.type_name));
        let ex = exact(&ty)?;
        self.m.take_expr(&ty, "x")?;
        Some((ex, Some(ty)))
    }

    fn cb_user_ret(&self, td: &CallbackTypedefDef) -> Option<(String, Ty)> {
        let ty = match td.return_type.as_deref() {
            None => Ty::Void,
            Some(r) => self.m.not_result(self.m.owned(r)),
        };
        if matches!(ty, Ty::Void) {
            return Some(("void".to_string(), ty));
        }
        let ex = restriction(&ty)?;
        self.m.in_expr(&ty, "x")?;
        Some((ex, ty))
    }

    fn callback_convertible(&self, td: &CallbackTypedefDef) -> bool {
        (0..td.args.len()).all(|j| {
            self.cb_user_arg(td, j, true).is_some() && self.cb_c_arg(&td.args[j]).is_some()
        }) && self.cb_user_ret(td).is_some()
            && self.cb_c_ret(td).is_some()
    }

    fn user_signature(
        &self,
        td: &CallbackTypedefDef,
        data_first: bool,
    ) -> Option<(Vec<String>, String)> {
        let mut parts = Vec::new();
        for j in 0..td.args.len() {
            parts.push(self.cb_user_arg(td, j, data_first)?.0);
        }
        Some((parts, self.cb_user_ret(td)?.0))
    }

    /// `AzUpdate delegate(AzRefAny, AzCallbackInfo)`: what a trampoline calls.
    fn erased_type(&self, td: &CallbackTypedefDef) -> Option<String> {
        let args: Vec<String> = td
            .args
            .iter()
            .map(|a| self.cb_c_arg(a))
            .collect::<Option<_>>()?;
        Some(format!(
            "{} delegate({})",
            self.cb_c_ret(td)?,
            args.join(", ")
        ))
    }

    /// The delegate the trampoline calls: C values in, the user's callable in
    /// the middle, a C value out.
    fn erased_closure(
        &self,
        td: &CallbackTypedefDef,
        user: &str,
        var: &str,
        data_first: bool,
    ) -> Option<Vec<String>> {
        let mut out = Vec::new();
        let params: Vec<String> = td
            .args
            .iter()
            .enumerate()
            .map(|(j, a)| Some(format!("{} __c{}", self.cb_c_arg(a)?, j)))
            .collect::<Option<_>>()?;
        let ret = self.cb_c_ret(td)?;
        let cb = format!("__cb_{}", var.trim_start_matches('_'));
        out.push(format!("auto {} = {};", cb, user));
        out.push(format!(
            "{} {} = delegate {}({}) {{",
            self.erased_type(td)?,
            var,
            ret,
            params.join(", ")
        ));
        let mut call = Vec::new();
        for j in 0..td.args.len() {
            if j == 0 && data_first {
                out.push("    AzRefAny __p0 = __c0;".to_string());
                out.push("    T __u0 = _azulObject!T(&__p0);".to_string());
            } else {
                let (_, ty) = self.cb_user_arg(td, j, data_first)?;
                let e = match ty {
                    Some(ty) => self.m.take_expr(&ty, &format!("__c{}", j))?,
                    None => format!("__c{}", j),
                };
                out.push(format!("    auto __u{} = {};", j, e));
            }
            call.push(format!("__u{}", j));
        }
        let (rex, rty) = self.cb_user_ret(td)?;
        match rty {
            Ty::Void => {
                out.push(format!("    {}({});", cb, call.join(", ")));
            }
            ty => {
                out.push(format!("    {} __ur = {}({});", rex, cb, call.join(", ")));
                out.push(format!("    return {};", self.m.in_expr(&ty, "__ur")?));
            }
        }
        out.push("};".to_string());
        Some(out)
    }

    fn emit_trampolines(&mut self, w: &mut W) {
        let m = self.m;
        let some_none = option_parts(m, "OptionRefAny");
        for tdn in self.trampolines.clone() {
            let td = m.callbacks[&tdn];
            let (Some(erased), Some(ret)) = (self.erased_type(td), self.cb_c_ret(td)) else {
                continue;
            };
            let params: Vec<String> = td
                .args
                .iter()
                .enumerate()
                .map(|(j, a)| format!("{} __c{}", self.cb_c_arg(a).unwrap(), j))
                .collect();
            let names: Vec<String> = (0..td.args.len()).map(|j| format!("__c{}", j)).collect();
            let data_first = td.args.first().is_some_and(|a| {
                a.ref_kind == ArgRefKind::Owned && matches!(m.owned(&a.type_name), Ty::RefAny)
            });
            let ctx = td.args.iter().enumerate().find_map(|(j, a)| {
                (a.ref_kind == ArgRefKind::Owned)
                    .then(|| m.ctx_getter(a.type_name.trim()))
                    .flatten()
                    .map(|g| (j, g, a.type_name.trim().to_string()))
            });
            w.l(
                0,
                &format!(
                    "/// The C entry point of every `{}` this binding registers.",
                    tdn
                ),
            );
            w.l(
                0,
                &format!(
                    "package extern (C) {} _azulTrampoline_{}({}) nothrow",
                    ret,
                    tdn,
                    params.join(", ")
                ),
            );
            w.l(0, "{");
            w.l(1, "try");
            w.l(1, "{");
            w.l(2, "_azulAttachThread();");
            w.l(2, &format!("{} __f;", erased));
            if let (Some((j, getter, info)), Some((none_member, none_index, some_member))) =
                (&ctx, &some_none)
            {
                w.l(2, &format!("Az{} __i = __c{};", info, j));
                w.l(2, &format!("AzOptionRefAny __ctx = {}(&__i);", getter));
                w.l(2, "scope (exit) AzOptionRefAny_delete(&__ctx);");
                w.l(
                    2,
                    &format!("if (__ctx.{}.tag != {})", none_member, none_index),
                );
                w.l(
                    3,
                    &format!(
                        "__f = _azulClosure!({})(&__ctx.{}.payload);",
                        erased, some_member
                    ),
                );
            }
            if data_first {
                w.l(2, "AzRefAny __d = __c0;");
                w.l(2, "scope (exit) AzRefAny_delete(&__d);");
                w.l(2, "if (__f is null)");
                w.l(3, &format!("__f = _azulClosure!({})(&__d);", erased));
            }
            w.l(2, "if (__f is null)");
            w.l(3, &format!("_azulNoClosure(\"{}\");", tdn));
            if ret == "void" {
                w.l(2, &format!("__f({});", names.join(", ")));
            } else {
                w.l(2, &format!("return __f({});", names.join(", ")));
            }
            w.l(1, "}");
            w.l(1, "catch (Throwable __t)");
            w.l(1, "{");
            w.l(2, &format!("_azulUncaught(__t, \"{}\");", tdn));
            w.l(1, "}");
            w.l(0, "}");
            w.l(0, "");
        }
    }

    // ------------------------------------------------------------------------
    // Container conversions
    // ------------------------------------------------------------------------

    fn emit_option_conv(&mut self, w: &mut W, c: &ClassInfo, t: &Ty) {
        let m = self.m;
        let (Ty::Option { name, payload }, Shape::Union { variants, .. }) = (t, &c.shape) else {
            return;
        };
        let none = variants.iter().find(|v| v.name == "None").unwrap();
        let some = variants.iter().find(|v| v.name == "Some").unwrap();
        let raw = format!("Az{}", name);
        let pmember = &some.fields[0].c_name;
        let (Some(restr), Some(ex), Some(pc), Some(in_e), Some(out_e), Some(take_e)) = (
            restriction(payload),
            exact(payload),
            c_type(payload),
            m.in_expr(payload, "x.get"),
            m.out_expr(payload, "__q"),
            m.take_expr(payload, &format!("v.{}.{}", some.member, pmember)),
        ) else {
            return;
        };
        w.l(
            0,
            &format!(
                "package {} _wrap_{}({} payload) nothrow @nogc",
                raw, name, pc
            ),
        );
        w.l(0, "{");
        w.l(1, &format!("{} __r;", raw));
        w.l(1, &format!("__r.{}.tag = {};", some.member, some.index));
        w.l(1, &format!("__r.{}.{} = payload;", some.member, pmember));
        w.l(1, "return __r;");
        w.l(0, "}");
        w.l(0, "");
        w.l(
            0,
            &format!("package {} _none_{}() nothrow @nogc", raw, name),
        );
        w.l(0, "{");
        w.l(1, &format!("{} __r;", raw));
        w.l(1, &format!("__r.{}.tag = {};", none.member, none.index));
        w.l(1, "return __r;");
        w.l(0, "}");
        w.l(0, "");
        w.l(
            0,
            &format!("package {} _in_{}(Nullable!({}) x)", raw, name, restr),
        );
        w.l(0, "{");
        w.l(1, "if (x.isNull)");
        w.l(2, &format!("return _none_{}();", name));
        w.l(1, &format!("return _wrap_{}({});", name, in_e));
        w.l(0, "}");
        w.l(0, "");
        if m.moves(payload) {
            if let Some(borrow_e) = m.borrow_expr(payload, "x.get") {
                w.l(
                    0,
                    &format!(
                        "package {} _borrow_{}(ref Nullable!({}) x)",
                        raw, name, restr
                    ),
                );
                w.l(0, "{");
                w.l(1, "if (x.isNull)");
                w.l(2, &format!("return _none_{}();", name));
                w.l(1, &format!("return _wrap_{}({});", name, borrow_e));
                w.l(0, "}");
                w.l(0, "");
                w.l(0, &format!("package void _release_{}({}* p)", name, raw));
                w.l(0, "{");
                w.l(1, &format!("if (p.{}.tag == {})", none.member, none.index));
                w.l(2, "return;");
                if let Some(r) = m.release(payload, "__q") {
                    w.l(1, &format!("auto __q = &p.{}.{};", some.member, pmember));
                    w.l(1, &r);
                }
                w.l(0, "}");
                w.l(0, "");
            }
        }
        w.l(
            0,
            &format!("package Nullable!({}) _out_{}(const({})* p)", ex, name, raw),
        );
        w.l(0, "{");
        w.l(1, &format!("if (p.{}.tag == {})", none.member, none.index));
        w.l(2, "return typeof(return).init;");
        w.l(
            1,
            &format!("auto __q = cast({}*) &p.{}.{};", pc, some.member, pmember),
        );
        w.l(1, &format!("return typeof(return)({});", out_e));
        w.l(0, "}");
        w.l(0, "");
        w.l(
            0,
            &format!("package Nullable!({}) _take_{}({} v)", ex, name, raw),
        );
        w.l(0, "{");
        w.l(1, &format!("if (v.{}.tag == {})", none.member, none.index));
        w.l(2, "return typeof(return).init;");
        w.l(1, &format!("return typeof(return)({});", take_e));
        w.l(0, "}");
        w.l(0, "");
    }

    fn emit_vec_conv(&mut self, w: &mut W, c: &ClassInfo, t: &Ty) {
        let m = self.m;
        let Ty::Vec { name, elem } = t else { return };
        let Shape::Struct(_) = &c.shape else {
            return;
        };
        let raw = format!("Az{}", name);
        let copy_fn = m.fun(name, "copyFromPtr").unwrap();
        let (Some(restr), Some(ex), Some(ec)) = (restriction(t), exact(elem), c_type(elem)) else {
            return;
        };
        let bytes = matches!(elem.as_ref(), Ty::Prim(Prim::U8));
        let (Some(in_e), Some(out_e)) = (m.borrow_expr(elem, "__e"), m.out_expr(elem, "__q"))
        else {
            return;
        };
        let in_cleanup = m.release(elem, "__q");
        // native -> C
        w.l(0, &format!("package {} _in_{}({} x)", raw, name, restr));
        w.l(0, "{");
        w.l(1, "immutable size_t __n = x.length;");
        w.l(
            1,
            &format!(
                "auto __buf = cast({}*) malloc({}.sizeof * (__n ? __n : 1));",
                ec, ec
            ),
        );
        w.l(1, "if (__buf is null)");
        w.l(2, "abort();");
        w.l(1, "scope (exit) free(__buf);");
        if bytes {
            w.l(1, "if (__n)");
            w.l(2, "memcpy(__buf, x.ptr, __n);");
        } else {
            w.l(1, "foreach (__i, ref __e; x)");
            w.l(2, &format!("__buf[__i] = {};", in_e));
        }
        w.l(1, &format!("{} __r = {}(__buf, __n);", raw, copy_fn));
        if let Some(cl) = in_cleanup {
            w.l(1, "foreach (__i; 0 .. __n)");
            w.l(1, "{");
            w.l(2, "auto __q = &__buf[__i];");
            w.l(2, &cl);
            w.l(1, "}");
        }
        w.l(1, "return __r;");
        w.l(0, "}");
        w.l(0, "");
        // C (borrowed) -> native
        w.l(
            0,
            &format!("package {}[] _out_{}(const({})* p)", ex, name, raw),
        );
        w.l(0, "{");
        w.l(1, "immutable size_t __n = p.len;");
        w.l(1, "if (__n == 0 || p.ptr is null)");
        w.l(2, "return null;");
        if bytes {
            w.l(1, "return (cast(ubyte*) p.ptr)[0 .. __n].dup;");
        } else {
            w.l(1, &format!("auto __out = new {}[](__n);", ex));
            w.l(1, "foreach (__i; 0 .. __n)");
            w.l(1, "{");
            w.l(2, &format!("auto __q = cast({}*) &p.ptr[__i];", ec));
            w.l(2, &format!("__out[__i] = {};", out_e));
            w.l(1, "}");
            w.l(1, "return __out;");
        }
        w.l(0, "}");
        w.l(0, "");
        // C (owned) -> native
        w.l(0, &format!("package {}[] _take_{}({} v)", ex, name, raw));
        w.l(0, "{");
        w.l(1, &format!("auto __r = _out_{}(&v);", name));
        if let Some(d) = m.delete_fn(name) {
            w.l(1, &format!("{}(&v);", d));
        }
        w.l(1, "return __r;");
        w.l(0, "}");
        w.l(0, "");
    }

    fn emit_result_conv(&mut self, w: &mut W, c: &ClassInfo, t: &Ty) {
        let m = self.m;
        let (Ty::Result { name, ok, err }, Shape::Union { variants, .. }) = (t, &c.shape) else {
            return;
        };
        let okv = variants.iter().find(|v| v.name == "Ok").unwrap();
        let errv = variants.iter().find(|v| v.name == "Err").unwrap();
        let raw = format!("Az{}", name);
        let (Some(okx), Some(errx), Some(take_ok), Some(take_err)) = (
            exact(ok),
            exact(err),
            m.take_expr(ok, &format!("v.{}.{}", okv.member, okv.fields[0].c_name)),
            m.take_expr(err, &format!("v.{}.{}", errv.member, errv.fields[0].c_name)),
        ) else {
            return;
        };
        w.l(0, &format!("package {} _take_{}({} v)", okx, name, raw));
        w.l(0, "{");
        w.l(1, &format!("if (v.{}.tag == {})", okv.member, okv.index));
        w.l(2, &format!("return {};", take_ok));
        w.l(
            1,
            &format!("throw new ResultException!({})({});", errx, take_err),
        );
        w.l(0, "}");
        w.l(0, "");
    }
}

/// (None member, None index, Some member) of an Option union.
fn option_parts(m: &Model, name: &str) -> Option<(String, usize, String)> {
    let c = m.classes.get(name)?;
    let Shape::Union { variants, .. } = &c.shape else {
        return None;
    };
    let none = variants.iter().find(|v| v.name == "None")?;
    let some = variants.iter().find(|v| v.name == "Some")?;
    Some((none.member.clone(), none.index, some.member.clone()))
}

/// D member names for variant names, unique within the type.
fn case_names(variants: &[String]) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    variants
        .iter()
        .map(|v| {
            let mut n = member_name(v);
            while !seen.insert(n.clone()) {
                n.push('_');
            }
            n
        })
        .collect()
}

fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_names_are_lower_camel_unique_and_never_keywords() {
        let v: Vec<String> = [
            "DoNothing",
            "RefreshDom",
            "URL",
            "Em",
            "EM",
            "Default",
            "Tag",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            case_names(&v),
            vec![
                "doNothing",
                "refreshDom",
                "url",
                "em",
                "em_",
                "default_",
                "tag_"
            ]
        );
    }

    #[test]
    fn native_types_at_the_boundary() {
        let opt = Ty::Option {
            name: "OptionString".into(),
            payload: Box::new(Ty::Str),
        };
        assert_eq!(exact(&opt).as_deref(), Some("Nullable!(string)"));
        let bytes = Ty::Vec {
            name: "U8Vec".into(),
            elem: Box::new(Ty::Prim(Prim::U8)),
        };
        assert_eq!(exact(&bytes).as_deref(), Some("ubyte[]"));
        assert_eq!(restriction(&bytes).as_deref(), Some("const(ubyte)[]"));
        let any = Ty::Option {
            name: "OptionRefAny".into(),
            payload: Box::new(Ty::RefAny),
        };
        assert_eq!(restriction(&any).as_deref(), Some("Nullable!(Object)"));
        assert_eq!(exact(&any).as_deref(), Some("Nullable!(RefAny)"));
        let res = Ty::Result {
            name: "ResultXmlXmlError".into(),
            ok: Box::new(Ty::Class("Xml".into())),
            err: Box::new(Ty::Class("XmlError".into())),
        };
        assert_eq!(exact(&res).as_deref(), Some("Xml"));
        assert_eq!(restriction(&res), None);
    }

    #[test]
    fn runtime_and_trait_names_are_never_generated_as_members() {
        for n in [
            "toString", "dup", "opEquals", "opCall", "_take", "tag", "init",
        ] {
            assert!(reserved_member(n), "{n} must be reserved");
        }
        assert!(!reserved_member("withCss"));
        assert!(!reserved_member("open"));
    }
}
