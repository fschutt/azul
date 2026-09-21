//! The Swift declarations: one `final class`, `struct` or `enum` per api.json
//! type, with native Swift values (`String`, `T?`, `[T]`, `throws`) at every
//! member boundary the IR shape allows. See `model.rs` for the type mapping
//! and `runtime.rs` for the ownership base classes.
//!
//! # Ownership, in one paragraph
//!
//! A class instance owns its value (`deinit` runs `_delete`) or is a view into
//! a field of an owner. Passing a non-`Copy` instance BY VALUE moves it, like
//! Rust: later use stops the program with the type's name; a view is copied
//! instead, since a field cannot be moved out of. A `withX` builder that
//! consumes `self` and returns `Self` rebuilds the value in place and returns
//! `self`, so chains read naturally. Values passed BY REFERENCE are borrowed.
//! Anything converted from a native value (a `String`, an `Array`) is a fresh
//! C value the call either consumes or the wrapper frees right after it.
//! Structs and enums are Swift values and convert on every crossing.
//!
//! # Callbacks
//!
//! A callback parameter takes a Swift closure, capturing or not. The C side is
//! a non-capturing `@convention(c)` trampoline per callback typedef
//! (`_Trampolines`); the closure travels in a `RefAny` handle, either in the
//! callback's `ctx` (read back through the info type's `getCtx`) or in the data
//! `RefAny` passed next to it. The data argument itself is any class instance,
//! handed back to the closure with its static type `T`.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    super::{
        ir::{
            ArgRefKind, CallbackTypedefDef, ConstantDef, FunctionArg, FunctionDef, FunctionKind,
            TypeCategory,
        },
        managed_host_invoker::{
            callback_ctx_field, has_callback_wrapper_arg, layout_callback_factory_info,
        },
    },
    camel, escape,
    model::{pointee_swift, ClassInfo, EnumInfo, Field, Kind, Model, Prim, Shape, Ty, Variant},
    swift_type_name,
};

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

/// Member names a generated type must not declare: the protocol requirements
/// the traits fill in, and Swift's own.
fn reserved_member(name: &str) -> bool {
    matches!(
        name,
        "description"
            | "debugDescription"
            | "hashValue"
            | "hash"
            | "copy"
            | "init"
            | "deinit"
            | "self"
            | "Type"
            | "Protocol"
            | "default"
            | "downcast"
            | "subscript"
    ) || name.starts_with('_')
}

/// The spelling of an owned C value of shape `t`, for local annotations and
/// buffers.
fn c_type(t: &Ty) -> Option<String> {
    Some(match t {
        Ty::Void => "Void".to_string(),
        Ty::Prim(p) => p.swift().to_string(),
        Ty::RawPtr(p) => p.clone(),
        Ty::Str => "AzString".to_string(),
        Ty::RefAny => "AzRefAny".to_string(),
        Ty::Enum(n)
        | Ty::Plain(n)
        | Ty::Union(n)
        | Ty::Class(n)
        | Ty::Callback(n)
        | Ty::Option { name: n, .. }
        | Ty::Vec { name: n, .. }
        | Ty::Result { name: n, .. } => format!("Az{}", n),
        Ty::Unsupported(_) => return None,
    })
}

/// The Swift type a value of shape `t` has when read.
fn exact(t: &Ty) -> Option<String> {
    Some(match t {
        Ty::Void => "Void".to_string(),
        Ty::Prim(p) => p.swift().to_string(),
        Ty::RawPtr(p) => p.clone(),
        // `Ty::Str` and `Ty::RefAny` have dropped the class name, so these
        // two Swift spellings cannot be derived: they are the standard
        // library's string and the runtime's wrapper class, and the IR
        // singles the api.json types out as `TypeCategory::String` / `RefAny`.
        Ty::Str => "String".to_string(), // allow-api-name: Swift's own String
        Ty::RefAny => "RefAny".to_string(), // allow-api-name: the runtime wrapper class
        Ty::Enum(n) | Ty::Plain(n) | Ty::Union(n) | Ty::Class(n) => swift_type_name(n),
        Ty::Option { payload, .. } => format!("{}?", exact(payload)?),
        Ty::Vec { elem, .. } => format!("[{}]", exact(elem)?),
        Ty::Result { ok, .. } => exact(ok)?,
        Ty::Callback(n) => format!("Az{}", n),
        Ty::Unsupported(_) => return None,
    })
}

/// The Swift type an argument of shape `t` accepts.
fn restriction(t: &Ty) -> Option<String> {
    Some(match t {
        Ty::RefAny => "AnyObject".to_string(),
        Ty::Option { payload, .. } => format!("{}?", restriction(payload)?),
        Ty::Vec { elem, .. } => format!("[{}]", restriction(elem)?),
        Ty::Result { .. } => return None,
        other => exact(other)?,
    })
}

// ============================================================================
// Conversions
// ============================================================================

impl<'a> Model<'a> {
    /// Native value `x` -> owned C value. Consumes (moves) wrapper objects.
    pub(super) fn in_expr(&self, t: &Ty, x: &str) -> Option<String> {
        Some(match t {
            Ty::Prim(_) | Ty::Callback(_) | Ty::RawPtr(_) => x.to_string(),
            Ty::Enum(_) | Ty::Plain(_) => format!("{}._raw", x),
            Ty::Union(_) => format!("{}._toRaw()", x),
            Ty::Str => format!("_Native.azString({})", x),
            Ty::RefAny => format!("_Native.refany({})", x),
            Ty::Class(_) => format!("{}._take()", x),
            Ty::Option { name, .. } | Ty::Vec { name, .. } => format!("_Conv.in_{}({})", name, x),
            Ty::Result { .. } | Ty::Void | Ty::Unsupported(_) => return None,
        })
    }

    /// Native value `x` -> a C value for a call that only BORROWS it: the
    /// wrapper objects inside are read bitwise (not moved, not copied) and
    /// everything converted is fresh. `release` frees exactly the fresh parts
    /// once the call returns.
    pub(super) fn borrow_expr(&self, t: &Ty, x: &str) -> Option<String> {
        Some(match t {
            Ty::Class(_) => format!("{}._address.pointee", x),
            Ty::Union(n) if self.moves(t) => {
                let _ = n;
                format!("{}._toRawBorrowed()", x)
            }
            Ty::Option { name, payload } if self.moves(payload) => {
                format!("_Conv.borrow_{}({})", name, x)
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
            Ty::Union(n) if self.moves(t) => {
                Some(format!("{}._releaseBorrowed({})", swift_type_name(n), p))
            }
            Ty::Option { name, payload } if self.moves(payload) => {
                Some(format!("_Conv.release_{}({})", name, p))
            }
            other => self.cleanup(other, p),
        }
    }

    /// Owned C value `v` -> native value.
    pub(super) fn take_expr(&self, t: &Ty, v: &str) -> Option<String> {
        Some(match t {
            Ty::Prim(_) | Ty::Callback(_) | Ty::RawPtr(_) => v.to_string(),
            Ty::Enum(n) | Ty::Plain(n) => format!("{}(_raw: {})", swift_type_name(n), v),
            Ty::Union(n) => format!("{}(_take: {})", swift_type_name(n), v),
            Ty::Str => format!("_Native.takeString({})", v),
            Ty::RefAny => format!("RefAny(_own: {})", v),
            Ty::Class(n) => format!("{}(_own: {})", swift_type_name(n), v),
            Ty::Option { name, .. } | Ty::Vec { name, .. } => format!("_Conv.take_{}({})", name, v),
            Ty::Result { name, .. } => format!("try _Conv.take_{}({})", name, v),
            Ty::Void | Ty::Unsupported(_) => return None,
        })
    }

    /// Borrowed C value behind pointer `p` -> independent native value.
    pub(super) fn out_expr(&self, t: &Ty, p: &str) -> Option<String> {
        Some(match t {
            Ty::Prim(_) | Ty::Callback(_) | Ty::RawPtr(_) => format!("{}.pointee", p),
            Ty::Enum(n) | Ty::Plain(n) => format!("{}(_raw: {}.pointee)", swift_type_name(n), p),
            Ty::Str => format!("_Native.string({})", p),
            Ty::RefAny => format!("RefAny(_own: AzRefAny_clone({}))", p),
            Ty::Class(n) if self.is_copy(n) => {
                format!("{}(_own: {}.pointee)", swift_type_name(n), p)
            }
            Ty::Class(n) => {
                let f = self.clone_fn(n)?;
                format!("{}(_own: {}({}))", swift_type_name(n), f, p)
            }
            Ty::Union(n) if self.class_copyable(n) => {
                format!("{}._copy({})", swift_type_name(n), p)
            }
            Ty::Option { name, .. } | Ty::Vec { name, .. } => format!("_Conv.out_{}({})", name, p),
            _ => return None,
        })
    }

    /// Frees a fresh C value (behind pointer `p`) that a call only borrowed.
    pub(super) fn cleanup(&self, t: &Ty, p: &str) -> Option<String> {
        // `Ty::Str` and `Ty::RefAny` have dropped the class name, so the two
        // types the IR singles out are asked for by category.
        let name = match t {
            Ty::Str => self.string_class()?,
            Ty::RefAny => self.refany_class()?,
            Ty::Option { name, .. } | Ty::Vec { name, .. } => name.as_str(),
            Ty::Union(n) | Ty::Class(n) if !self.is_copy(n) => n.as_str(),
            _ => return None,
        };
        self.fun(name, "delete").map(|f| format!("{}({})", f, p))
    }

    /// Type of a returned value: `*const T` / `&T` returns stay pointers.
    pub(super) fn ret_ty(&self, ret: Option<&str>) -> Ty {
        let Some(r) = ret else { return Ty::Void };
        let r = r.trim();
        for (prefix, mutable) in [
            ("*const ", false),
            ("*mut ", true),
            ("&mut ", true),
            ("&", false),
        ] {
            if let Some(inner) = r.strip_prefix(prefix) {
                return match pointee_swift(inner, mutable, self) {
                    Some(p) => Ty::RawPtr(p),
                    None => Ty::Unsupported(r.to_string()),
                };
            }
        }
        self.owned(r)
    }
}

// ============================================================================
// Member naming
// ============================================================================

/// A Swift member's identity for redeclaration purposes: `var x` and
/// `func x()` clash, and so do `case y(Int)` and `static func y(_:)`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Selector {
    is_static: bool,
    base: String,
    labels: Vec<String>,
}

#[derive(Default)]
struct Taken {
    /// selector -> parameter type signatures (None for a non-function).
    members: BTreeMap<Selector, Vec<Option<String>>>,
}

impl Taken {
    fn is_free(&self, s: &Selector, sig: Option<&str>) -> bool {
        match self.members.get(s) {
            None => true,
            Some(existing) => match sig {
                None => false,
                Some(sig) => existing
                    .iter()
                    .all(|e| e.as_deref().is_some_and(|e| e != sig)),
            },
        }
    }

    fn take(&mut self, s: Selector, sig: Option<String>) {
        self.members.entry(s).or_default().push(sig);
    }
}

/// One planned parameter.
struct Param {
    label: String,
    name: String,
    ty: String,
}

fn param_list(params: &[Param]) -> String {
    params
        .iter()
        .map(|p| {
            if p.label == p.name {
                format!("{}: {}", p.name, p.ty)
            } else {
                format!("{} {}: {}", p.label, p.name, p.ty)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

// ============================================================================
// Emitter
// ============================================================================

pub struct Output {
    /// Swift source per api.json module (declarations + their conversions).
    pub modules: BTreeMap<String, String>,
    /// Trampolines and module-independent declarations.
    pub shared: String,
    /// api.json functions with no Swift-side shape.
    pub skipped: Vec<String>,
    /// Coverage counters, for the header and the commit log.
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
    pub unions_enum: usize,
    pub unions_total: usize,
    pub classes: usize,
    pub structs: usize,
    pub enums: usize,
}

pub struct Emitter<'m, 'a> {
    m: &'m Model<'a>,
    /// Natively mapped api.json names and their Swift spelling, for docs.
    native_names: BTreeMap<String, String>,
    trampolines: BTreeSet<String>,
    /// `Wrapped` spellings that already carry an `Optional` extension: two
    /// api.json options over the same Swift type would redeclare each other.
    option_extensions: BTreeSet<String>,
    skipped: Vec<String>,
    stats: Stats,
}

pub fn generate(m: &Model) -> Output {
    // Doc rewriting turns a natively mapped api.json name into its Swift
    // spelling (`OptionDom` -> `Dom?`). The string keeps its own name: it is
    // declared as a typealias, so the name does exist in Swift.
    let native_names = m
        .classes
        .values()
        .filter(|c| m.is_native(&c.name) && c.category != TypeCategory::String)
        .filter_map(|c| exact(&m.owned(&c.name)).map(|x| (c.name.clone(), x)))
        .collect();
    let mut e = Emitter {
        m,
        native_names,
        trampolines: BTreeSet::new(),
        option_extensions: BTreeSet::new(),
        skipped: Vec::new(),
        stats: Stats::default(),
    };
    let mut modules: BTreeMap<String, W> = BTreeMap::new();

    for info in m.enums.values() {
        let w = modules.entry(module_key(&info.module)).or_default();
        e.emit_enum(w, info);
    }
    for c in m.classes.values() {
        e.count(c);
        let w = modules.entry(module_key(&c.module)).or_default();
        if m.is_native(&c.name) || matches!(m.owned(&c.name), Ty::Result { .. }) {
            e.emit_native(w, c);
            if m.is_native(&c.name) {
                continue;
            }
        }
        match c.kind {
            Kind::Class => e.emit_class(w, c),
            Kind::Plain => e.emit_plain(w, c),
            Kind::Union => e.emit_union(w, c),
        }
    }
    e.emit_constants(&mut modules);
    let mut shared = W::default();
    e.emit_trampolines(&mut shared);
    Output {
        modules: modules.into_iter().map(|(k, v)| (k, v.out)).collect(),
        shared: shared.out,
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
    Class,
    Plain,
    Enum,
    Union,
}

impl<'m, 'a> Emitter<'m, 'a> {
    /// Doc text with every natively mapped type name in its Swift spelling
    /// (`OptionDom` -> `Dom?`, `U8Vec` -> `[UInt8]`): those names do not exist
    /// in Swift.
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
            self.stats.unions_total += 1;
            if c.kind == Kind::Union {
                self.stats.unions_enum += 1;
            }
        }
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
                    "/// Rust `String` is `Swift.String` at every boundary; it keeps its own",
                );
                w.l(
                    0,
                    "/// `description`, `==`, `<`, `hash(into:)`, value copies and `init()`.",
                );
                w.l(0, &format!("public typealias {} = Swift.String", c.name));
                w.l(0, "");
                // ... and everything api.json declares on it, which has no
                // class of its own to live on.
                self.emit_string_ext(w, c);
                return;
            }
            t @ Ty::Option { .. } => self.emit_option_conv(w, c, &t),
            t @ Ty::Vec { .. } => {
                self.emit_vec_conv(w, c, &t);
                // ... and the Rust container itself, whose members a Swift
                // `Array` cannot stand in for.
                self.emit_vec_class(w, c, &t);
                return;
            }
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
                self.skipped
                    .push(format!("{} (native Swift type)", f.c_name));
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
        self.stats.enums += 1;
        let name = &info.name;
        let sname = swift_type_name(name);
        let raw = format!("Az{}", name);
        let cases = case_names(&info.variants);
        let conformances = self.trait_conformances(name, Recv::Enum);
        w.doc(0, &self.rw(&info.doc));
        w.l(
            0,
            &format!(
                "public enum {}{} {{",
                sname,
                conformance_clause(&conformances)
            ),
        );
        for c in &cases {
            w.l(1, &format!("case {}", escape(c)));
        }
        w.l(0, "");
        w.l(1, &format!("internal init(_raw raw: {}) {{", raw));
        w.l(2, "switch raw.rawValue {");
        for (i, c) in cases.iter().enumerate() {
            w.l(2, &format!("case {}: self = .{}", i, escape(c)));
        }
        w.l(
            2,
            &format!(
                "default: preconditionFailure(\"azul: invalid {} discriminant \\(raw.rawValue)\")",
                name
            ),
        );
        w.l(2, "}");
        w.l(1, "}");
        w.l(0, "");
        w.l(1, &format!("internal var _raw: {} {{", raw));
        w.l(2, "switch self {");
        for (i, c) in cases.iter().enumerate() {
            w.l(
                2,
                &format!("case .{}: return {}(rawValue: {})", escape(c), raw, i),
            );
        }
        w.l(2, "}");
        w.l(1, "}");
        w.l(0, "");
        let mut taken = Taken::default();
        for c in &cases {
            taken.take(
                Selector {
                    is_static: true,
                    base: c.clone(),
                    labels: vec![],
                },
                None,
            );
        }
        self.emit_traits(w, name, Recv::Enum, &mut taken);
        self.emit_methods(w, name, Recv::Enum, &mut taken);
        w.l(0, "}");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // Plain structs
    // ------------------------------------------------------------------------

    fn emit_plain(&mut self, w: &mut W, c: &ClassInfo) {
        let m = self.m;
        self.stats.structs += 1;
        let name = &c.name;
        let sname = swift_type_name(name);
        let raw = format!("Az{}", name);
        let Shape::Struct(fields) = &c.shape else {
            return;
        };
        let conformances = self.trait_conformances(name, Recv::Plain);
        w.doc(0, &self.rw(&c.doc));
        w.l(
            0,
            &format!(
                "public struct {}{} {{",
                sname,
                conformance_clause(&conformances)
            ),
        );
        w.l(1, &format!("internal var _raw: {}", raw));
        w.l(0, "");
        w.l(1, &format!("internal init(_raw raw: {}) {{", raw));
        w.l(2, "_raw = raw");
        w.l(1, "}");
        w.l(0, "");
        let mut taken = Taken::default();
        // Memberwise initializer over every field.
        let exposed: Vec<(&Field, Ty, String)> = fields
            .iter()
            .filter_map(|f| {
                let t = m.not_result(m.field(f));
                let ex = exact(&t)?;
                m.in_expr(&t, "x")?;
                m.take_expr(&t, "x")?;
                Some((f, t, ex))
            })
            .collect();
        if !fields.is_empty() && exposed.len() == fields.len() {
            let params: Vec<Param> = exposed
                .iter()
                .enumerate()
                .map(|(i, (f, _, ex))| Param {
                    label: escape(&member_name(&f.name)),
                    name: format!("__f{}", i),
                    ty: ex.clone(),
                })
                .collect();
            let sel = Selector {
                is_static: true,
                base: "init".to_string(),
                labels: exposed
                    .iter()
                    .map(|(f, _, _)| member_name(&f.name))
                    .collect(),
            };
            let sig = params
                .iter()
                .map(|p| p.ty.clone())
                .collect::<Vec<_>>()
                .join(",");
            if taken.is_free(&sel, Some(&sig)) {
                taken.take(sel, Some(sig));
                w.l(1, &format!("public init({}) {{", param_list(&params)));
                w.l(2, &format!("var __v: {} = {}()", raw, raw));
                for (i, (f, t, _)) in exposed.iter().enumerate() {
                    w.l(
                        2,
                        &format!(
                            "__v.{} = {}",
                            escape(&f.c_name),
                            m.in_expr(t, &format!("__f{}", i)).unwrap()
                        ),
                    );
                }
                w.l(2, "_raw = __v");
                w.l(1, "}");
                w.l(0, "");
            }
        }
        self.emit_traits(w, name, Recv::Plain, &mut taken);
        self.emit_methods(w, name, Recv::Plain, &mut taken);
        for (f, t, ex) in &exposed {
            let prop = member_name(&f.name);
            let sel = Selector {
                is_static: false,
                base: prop.clone(),
                labels: vec![],
            };
            if !taken.is_free(&sel, None) {
                continue;
            }
            taken.take(sel, None);
            let access = format!("_raw.{}", escape(&f.c_name));
            if let Some(d) = &f.doc {
                w.doc(1, &self.rw(std::slice::from_ref(d)));
            }
            w.l(1, &format!("public var {}: {} {{", escape(&prop), ex));
            w.l(
                2,
                &format!("get {{ return {} }}", m.take_expr(t, &access).unwrap()),
            );
            w.l(
                2,
                &format!(
                    "set {{ {} = {} }}",
                    access,
                    m.in_expr(t, "newValue").unwrap()
                ),
            );
            w.l(1, "}");
            w.l(0, "");
        }
        w.l(0, "}");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // Tagged unions -> enums with associated values
    // ------------------------------------------------------------------------

    fn emit_union(&mut self, w: &mut W, c: &ClassInfo) {
        let m = self.m;
        let name = &c.name;
        let sname = swift_type_name(name);
        let raw = format!("Az{}", name);
        let Shape::Union { tag_u8, variants } = &c.shape else {
            return;
        };
        let conformances = self.trait_conformances(name, Recv::Union);
        let cases = case_names(&variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>());
        w.doc(0, &self.rw(&c.doc));
        w.l(
            0,
            &format!(
                "public enum {}{} {{",
                sname,
                conformance_clause(&conformances)
            ),
        );
        let mut taken = Taken::default();
        for (v, case) in variants.iter().zip(&cases) {
            let tys: Vec<String> = v
                .fields
                .iter()
                .map(|f| exact(&m.not_result(m.field(f))).unwrap_or_else(|| "Never".to_string()))
                .collect();
            if let Some(d) = &v.doc {
                w.doc(1, &self.rw(std::slice::from_ref(d)));
            }
            let labels: Vec<String> = if v.named {
                v.fields.iter().map(|f| member_name(&f.name)).collect()
            } else {
                v.fields.iter().map(|_| "_".to_string()).collect()
            };
            if v.fields.is_empty() {
                w.l(1, &format!("case {}", escape(case)));
            } else if v.named {
                let parts: Vec<String> = v
                    .fields
                    .iter()
                    .zip(&tys)
                    .map(|(f, t)| format!("{}: {}", escape(&member_name(&f.name)), t))
                    .collect();
                w.l(1, &format!("case {}({})", escape(case), parts.join(", ")));
            } else {
                w.l(1, &format!("case {}({})", escape(case), tys.join(", ")));
            }
            // `case y(V)` and `static func y(_: Int)` coexist; the same
            // payload types do not.
            taken.take(
                Selector {
                    is_static: true,
                    base: case.clone(),
                    labels: if v.fields.is_empty() { vec![] } else { labels },
                },
                if v.fields.is_empty() {
                    None
                } else {
                    Some(tys.join(","))
                },
            );
        }
        w.l(0, "");

        // owned C value -> enum
        w.l(1, &format!("internal init(_take raw: {}) {{", raw));
        w.l(
            2,
            &format!("switch {} {{", tag_read(&variants[0], "raw", *tag_u8)),
        );
        for (v, case) in variants.iter().zip(&cases) {
            let member = escape(&v.name);
            let vals: Vec<String> = v
                .fields
                .iter()
                .map(|f| {
                    let t = m.not_result(m.field(f));
                    m.take_expr(&t, &format!("raw.{}.{}", member, escape(&f.c_name)))
                        .unwrap_or_default()
                })
                .collect();
            let rhs = if v.fields.is_empty() {
                format!(".{}", escape(case))
            } else if v.named {
                let parts: Vec<String> = v
                    .fields
                    .iter()
                    .zip(&vals)
                    .map(|(f, x)| format!("{}: {}", escape(&member_name(&f.name)), x))
                    .collect();
                format!(".{}({})", escape(case), parts.join(", "))
            } else {
                format!(".{}({})", escape(case), vals.join(", "))
            };
            w.l(2, &format!("case {}: self = {}", v.index, rhs));
        }
        w.l(
            2,
            &format!(
                "default: preconditionFailure(\"azul: invalid {} discriminant\")",
                name
            ),
        );
        w.l(2, "}");
        w.l(1, "}");
        w.l(0, "");

        // enum -> owned C value (moves payload objects), and the borrowing
        // twin with its release (only a union holding objects needs one).
        let moves = m.moves(&Ty::Union(name.clone()));
        let copyable = m.class_copyable(name);
        self.emit_to_raw(w, c, &cases, "_toRaw", false);
        if moves {
            self.emit_to_raw(w, c, &cases, "_toRawBorrowed", true);
            self.emit_release_borrowed(w, c);
        }
        if copyable {
            w.l(
                1,
                &format!(
                    "internal static func _copy(_ p: UnsafeMutablePointer<{}>) -> {} {{",
                    raw, sname
                ),
            );
            if m.is_copy(name) {
                w.l(2, &format!("return {}(_take: p.pointee)", sname));
            } else {
                let f = m.clone_fn(name).unwrap();
                w.l(2, &format!("return {}(_take: {}(p))", sname, f));
            }
            w.l(1, "}");
            w.l(0, "");
        }
        self.emit_traits(w, name, Recv::Union, &mut taken);
        self.emit_methods(w, name, Recv::Union, &mut taken);
        w.l(0, "}");
        w.l(0, "");
    }

    /// The exported constructor of one union variant, if libazul exports it
    /// and it takes exactly the variant's fields by value. Building the C
    /// value by calling it keeps ONE authority on the tag and on where a
    /// payload sits - the union's layout is never restated in Swift.
    fn variant_ctor(&self, class: &str, v: &Variant) -> Option<String> {
        let f = self.m.ir.variant_constructor(class, &v.name)?;
        let matches_fields = f.args.len() == v.fields.len()
            && f.args.iter().zip(&v.fields).all(|(a, field)| {
                a.ref_kind == ArgRefKind::Owned && a.type_name.trim() == field.type_name.trim()
            });
        (self.m.funs.contains(&f.c_name) && matches_fields).then(|| f.c_name.clone())
    }

    fn emit_to_raw(&mut self, w: &mut W, c: &ClassInfo, cases: &[String], fname: &str, copy: bool) {
        let m = self.m;
        let raw = format!("Az{}", c.name);
        let Shape::Union { tag_u8, variants } = &c.shape else {
            return;
        };
        w.l(1, &format!("internal func {}() -> {} {{", fname, raw));
        w.l(2, "switch self {");
        for (v, case) in variants.iter().zip(cases) {
            let binds: Vec<String> = (0..v.fields.len()).map(|i| format!("__p{}", i)).collect();
            if v.fields.is_empty() {
                w.l(2, &format!("case .{}:", escape(case)));
            } else {
                w.l(
                    2,
                    &format!("case let .{}({}):", escape(case), binds.join(", ")),
                );
            }
            // Each payload as an owned C value (`_toRaw`) or as a bitwise
            // view of one the Swift value keeps (`_toRawBorrowed`); the
            // release twin frees whatever this built fresh.
            let args: Option<Vec<String>> = v
                .fields
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let t = m.not_result(m.field(f));
                    let x = format!("__p{}", i);
                    if copy {
                        m.borrow_expr(&t, &x)
                    } else {
                        m.in_expr(&t, &x)
                    }
                })
                .collect();
            match (self.variant_ctor(&c.name, v), &args) {
                (Some(ctor), Some(args)) => {
                    w.l(3, &format!("return {}({})", ctor, args.join(", ")));
                    continue;
                }
                // No constructor, or a payload with no Swift expression:
                // write the variant struct out and tag it here.
                _ => {}
            }
            let vstruct = format!("{}Variant_{}", raw, v.name);
            w.l(3, &format!("var __v: {} = {}()", vstruct, vstruct));
            w.l(3, &format!("__v.tag = {}", tag_value(c, v, *tag_u8)));
            for (i, f) in v.fields.iter().enumerate() {
                let t = m.not_result(m.field(f));
                let x = format!("__p{}", i);
                let e = if copy {
                    m.borrow_expr(&t, &x)
                } else {
                    m.in_expr(&t, &x)
                };
                w.l(
                    3,
                    &format!("__v.{} = {}", escape(&f.c_name), e.unwrap_or_default()),
                );
            }
            w.l(3, &format!("var __r: {} = {}()", raw, raw));
            w.l(3, &format!("__r.{} = __v", escape(&v.name)));
            w.l(3, "return __r");
        }
        w.l(2, "}");
        w.l(1, "}");
        w.l(0, "");
    }

    fn emit_release_borrowed(&mut self, w: &mut W, c: &ClassInfo) {
        let m = self.m;
        let raw = format!("Az{}", c.name);
        let Shape::Union { tag_u8, variants } = &c.shape else {
            return;
        };
        w.l(
            1,
            &format!(
                "internal static func _releaseBorrowed(_ p: UnsafeMutablePointer<{}>) {{",
                raw
            ),
        );
        w.l(
            2,
            &format!("switch {} {{", tag_read(&variants[0], "p.pointee", *tag_u8)),
        );
        for v in variants {
            let vstruct = format!("{}Variant_{}", raw, v.name);
            let mut lines = Vec::new();
            for f in &v.fields {
                let t = m.not_result(m.field(f));
                let Some(ct) = c_type(&t) else { continue };
                if let Some(r) = m.release(&t, "__q") {
                    lines.push(format!(
                        "do {{ let __q: UnsafeMutablePointer<{}> = UnsafeMutableRawPointer(p).advanced(by: MemoryLayout<{}>.offset(of: \\{}.{})!).assumingMemoryBound(to: {}.self); {} }}",
                        ct, vstruct, vstruct, escape(&f.c_name), ct, r
                    ));
                }
            }
            if !lines.is_empty() {
                w.l(2, &format!("case {}:", v.index));
                for l in lines {
                    w.l(3, &l);
                }
            }
        }
        w.l(2, "default:");
        w.l(3, "break");
        w.l(2, "}");
        w.l(1, "}");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // Classes
    // ------------------------------------------------------------------------

    fn emit_class(&mut self, w: &mut W, c: &ClassInfo) {
        let m = self.m;
        self.stats.classes += 1;
        let name = &c.name;
        let sname = swift_type_name(name);
        let raw = format!("Az{}", name);
        let mut conformances = vec![format!("AzulValue<{}>", raw)];
        conformances.extend(self.trait_conformances(name, Recv::Class));
        w.doc(0, &self.rw(&c.doc));
        w.l(
            0,
            &format!(
                "public final class {}{} {{",
                sname,
                conformance_clause(&conformances)
            ),
        );
        if let Some(f) = m.delete_fn(name) {
            w.l(
                1,
                &format!(
                    "override internal class func _drop(_ p: UnsafeMutablePointer<{}>) {{ {}(p) }}",
                    raw, f
                ),
            );
        }
        if m.is_copy(name) {
            w.l(
                1,
                "override internal class var _isCopy: Bool { return true }",
            );
        } else if let Some(f) = m.clone_fn(name) {
            w.l(
                1,
                &format!(
                    "override internal class func _copyRaw(_ p: UnsafeMutablePointer<{}>) -> {}? {{ return {}(p) }}",
                    raw, raw, f
                ),
            );
        }
        w.l(0, "");
        let mut taken = Taken::default();
        if c.category == TypeCategory::RefAny {
            w.l(
                1,
                "/// Wraps any class instance; libazul keeps it alive while it holds this RefAny.",
            );
            w.l(1, "public convenience init(_ object: AnyObject) {");
            w.l(2, "self.init(_own: _Native.refany(object))");
            w.l(1, "}");
            w.l(0, "");
            w.l(1, "/// The object this RefAny holds, if it is a `T`.");
            w.l(
                1,
                "public func downcast<T: AnyObject>(_ type: T.Type) -> T? {",
            );
            w.l(2, "return _Handles.held(_address)?.object as? T");
            w.l(1, "}");
            w.l(0, "");
            taken.take(
                Selector {
                    is_static: true,
                    base: "init".to_string(),
                    labels: vec!["_".to_string()],
                },
                Some("AnyObject".to_string()),
            );
        }
        self.emit_traits(w, name, Recv::Class, &mut taken);
        self.emit_methods(w, name, Recv::Class, &mut taken);
        match &c.shape {
            Shape::Struct(fields) => self.emit_class_fields(w, c, fields, &mut taken),
            // A tagged union Swift cannot hold as an enum (`classify`: a
            // payload is a raw pointer or has no Swift shape) is a class, and
            // then the exported variant constructors are the only way to
            // build one - the enum form gets them through `_toRaw`.
            Shape::Union { .. } => self.emit_union_ctors(w, c, &mut taken),
        }
        w.l(0, "}");
        w.l(0, "");
    }

    /// One static factory per variant of a union-shaped class, each calling
    /// the exported constructor. Emitted after the methods, so an api.json
    /// member of the same name keeps its spelling.
    fn emit_union_ctors(&mut self, w: &mut W, c: &ClassInfo, taken: &mut Taken) {
        let m = self.m;
        let Shape::Union { variants, .. } = &c.shape else {
            return;
        };
        let sname = swift_type_name(&c.name);
        let cases = case_names(&variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>());
        for (v, case) in variants.iter().zip(&cases) {
            let Some(ctor) = self.variant_ctor(&c.name, v) else {
                continue;
            };
            // One parameter per payload field, in the binding's convention:
            // the first unlabeled, the rest labeled.
            let names: Vec<String> = v
                .fields
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let n = camel(&f.name);
                    if n.is_empty() {
                        format!("arg{}", i)
                    } else {
                        n
                    }
                })
                .collect();
            let shapes: Vec<Ty> = v.fields.iter().map(|f| m.not_result(m.field(f))).collect();
            let (Some(tys), Some(args)): (Option<Vec<String>>, Option<Vec<String>>) = (
                shapes.iter().map(restriction).collect(),
                shapes
                    .iter()
                    .zip(&names)
                    .map(|(t, n)| m.in_expr(t, &escape(n)))
                    .collect(),
            ) else {
                self.skipped.push(ctor);
                continue;
            };
            let labels: Vec<String> = names
                .iter()
                .enumerate()
                .map(|(i, n)| if i == 0 { "_".to_string() } else { n.clone() })
                .collect();
            let sel = Selector {
                is_static: true,
                base: case.clone(),
                labels: labels.clone(),
            };
            let sig = tys.join(",");
            if !taken.is_free(&sel, Some(sig.as_str())) {
                self.skipped.push(format!("{} {}", ctor, TAKEN));
                continue;
            }
            taken.take(sel, Some(sig));
            let params: Vec<String> = labels
                .iter()
                .zip(names.iter().zip(&tys))
                .map(|(l, (n, t))| format!("{} {}: {}", l, escape(n), t))
                .collect();
            if let Some(d) = &v.doc {
                w.doc(1, &self.rw(std::slice::from_ref(d)));
            }
            w.l(
                1,
                &format!(
                    "public static func {}({}) -> {} {{",
                    escape(case),
                    params.join(", "),
                    sname
                ),
            );
            w.l(
                2,
                &format!("return {}(_own: {}({}))", sname, ctor, args.join(", ")),
            );
            w.l(1, "}");
            w.l(0, "");
        }
    }

    fn emit_class_fields(&mut self, w: &mut W, c: &ClassInfo, fields: &[Field], taken: &mut Taken) {
        let m = self.m;
        let raw = format!("Az{}", c.name);
        for f in fields {
            if f.name.starts_with('_') {
                continue;
            }
            let t = m.not_result(m.field(f));
            let Some(ex) = exact(&t) else { continue };
            let prop = member_name(&f.name);
            let sel = Selector {
                is_static: false,
                base: prop.clone(),
                labels: vec![],
            };
            if !taken.is_free(&sel, None) {
                continue;
            }
            let fptr = format!("_address.pointer(to: \\{}.{})!", raw, escape(&f.c_name));
            let get = match &t {
                Ty::Class(n) => Some(format!(
                    "{}(_view: {}, root: self)",
                    swift_type_name(n),
                    fptr
                )),
                Ty::RefAny => Some(format!("RefAny(_view: {}, root: self)", fptr)),
                Ty::Prim(_) | Ty::Enum(_) | Ty::Plain(_) => {
                    m.take_expr(&t, &format!("_address.pointee.{}", escape(&f.c_name)))
                }
                Ty::Callback(_) | Ty::RawPtr(_) => None,
                other => m.out_expr(other, &fptr),
            };
            let Some(get) = get else { continue };
            taken.take(sel, None);
            if let Some(d) = &f.doc {
                w.doc(1, &self.rw(std::slice::from_ref(d)));
            }
            w.l(1, &format!("public var {}: {} {{", escape(&prop), ex));
            w.l(2, "get {");
            w.l(3, &format!("return {}", get));
            w.l(2, "}");
            let setter = m.in_expr(&t, "newValue");
            if let Some(setter) = setter {
                w.l(2, "set {");
                w.l(3, &format!("let __v: {} = {}", c_type(&t).unwrap(), setter));
                match &t {
                    Ty::Prim(_) | Ty::Enum(_) | Ty::Plain(_) => {
                        w.l(3, &format!("_address.pointee.{} = __v", escape(&f.c_name)));
                    }
                    _ => {
                        w.l(
                            3,
                            &format!(
                                "let __f: UnsafeMutablePointer<{}> = {}",
                                c_type(&t).unwrap(),
                                fptr
                            ),
                        );
                        if let Some(d) = m.cleanup(&t, "__f") {
                            w.l(3, &d);
                        }
                        w.l(3, "__f.pointee = __v");
                    }
                }
                w.l(2, "}");
            }
            w.l(1, "}");
            w.l(0, "");
        }
    }

    // ------------------------------------------------------------------------
    // Traits
    // ------------------------------------------------------------------------

    fn trait_conformances(&self, name: &str, recv: Recv) -> Vec<String> {
        let m = self.m;
        let mut out = Vec::new();
        let t = |k| m.trait_fn(name, k).is_some();
        if t(FunctionKind::DebugToString) {
            out.push("CustomStringConvertible".to_string());
            out.push("CustomDebugStringConvertible".to_string());
        }
        let eq = t(FunctionKind::PartialEq);
        if eq {
            out.push("Equatable".to_string());
        }
        if eq && t(FunctionKind::Hash) {
            out.push("Hashable".to_string());
        }
        if eq && (t(FunctionKind::Cmp) || t(FunctionKind::PartialCmp)) {
            out.push("Comparable".to_string());
        }
        out
    }

    /// Emits the protocol requirements a type's derives provide.
    fn emit_traits(&mut self, w: &mut W, name: &str, recv: Recv, taken: &mut Taken) {
        let m = self.m;
        let t = |k| m.trait_fn(name, k);
        let sname = swift_type_name(name);
        let raw = format!("Az{}", name);
        // Borrow `x` (an expression of this type) as `var <local>`; returns
        // (setup lines, pointer expression, cleanup lines).
        let borrow = |x: &str, local: &str| -> (Vec<String>, String, Vec<String>) {
            match recv {
                Recv::Class => (vec![], format!("{}._address", x), vec![]),
                Recv::Plain | Recv::Enum => (
                    vec![format!("var {}: {} = {}._raw", local, raw, x)],
                    format!("&{}", local),
                    vec![],
                ),
                Recv::Union => {
                    let t = Ty::Union(name.to_string());
                    let cleanup: Vec<String> =
                        m.release(&t, &format!("&{}", local)).into_iter().collect();
                    (
                        vec![format!(
                            "var {}: {} = {}",
                            local,
                            raw,
                            m.borrow_expr(&t, x).unwrap()
                        )],
                        format!("&{}", local),
                        cleanup,
                    )
                }
            }
        };
        let copyable_union = true;

        if let (Some(f), true) = (t(FunctionKind::DebugToString), copyable_union) {
            let (setup, p, cleanup) = borrow("self", "__s");
            w.l(1, "/// Rust `Debug`.");
            w.l(1, "public var description: String {");
            for s in &setup {
                w.l(2, s);
            }
            w.l(2, &format!("let __r: AzString = {}({})", f, p));
            for s in &cleanup {
                w.l(2, s);
            }
            w.l(2, "return _Native.takeString(__r)");
            w.l(1, "}");
            w.l(0, "");
            w.l(1, "public var debugDescription: String {");
            w.l(2, "return description");
            w.l(1, "}");
            w.l(0, "");
        }
        let copy = m.is_copy(name);
        if copy || t(FunctionKind::DeepCopy).is_some() {
            w.l(1, "/// Rust `Clone`: an independent copy.");
            w.l(1, &format!("public func copy() -> {} {{", sname));
            match recv {
                Recv::Class if copy => w.l(2, &format!("return {}(_own: _address.pointee)", sname)),
                Recv::Class => w.l(
                    2,
                    &format!(
                        "return {}(_own: {}(_address))",
                        sname,
                        t(FunctionKind::DeepCopy).unwrap()
                    ),
                ),
                Recv::Plain | Recv::Enum => w.l(2, "return self"),
                Recv::Union if copy => w.l(2, "return self"),
                Recv::Union => {
                    let ut = Ty::Union(name.to_string());
                    w.l(
                        2,
                        &format!("var __b: {} = {}", raw, m.borrow_expr(&ut, "self").unwrap()),
                    );
                    w.l(
                        2,
                        &format!(
                            "let __c: {} = {}(&__b)",
                            raw,
                            t(FunctionKind::DeepCopy).unwrap()
                        ),
                    );
                    if let Some(r) = m.release(&ut, "&__b") {
                        w.l(2, &r);
                    }
                    w.l(2, &format!("return {}(_take: __c)", sname));
                }
            }
            w.l(1, "}");
            w.l(0, "");
            taken.take(
                Selector {
                    is_static: false,
                    base: "copy".to_string(),
                    labels: vec![],
                },
                None,
            );
        }
        if let (Some(f), true) = (t(FunctionKind::PartialEq), copyable_union) {
            let (s1, p1, c1) = borrow("lhs", "__l");
            let (s2, p2, c2) = borrow("rhs", "__r");
            w.l(1, "/// Rust `PartialEq`.");
            w.l(
                1,
                &format!(
                    "public static func == (lhs: {}, rhs: {}) -> Bool {{",
                    sname, sname
                ),
            );
            for s in s1.iter().chain(&s2) {
                w.l(2, s);
            }
            w.l(2, &format!("let __eq: Bool = {}({}, {})", f, p1, p2));
            for s in c1.iter().chain(&c2) {
                w.l(2, s);
            }
            w.l(2, "return __eq");
            w.l(1, "}");
            w.l(0, "");
        }
        let cmp = t(FunctionKind::Cmp);
        let pcmp = t(FunctionKind::PartialCmp);
        if let (Some(f), true) = (cmp.clone().or(pcmp.clone()), copyable_union) {
            let (s1, p1, c1) = borrow("lhs", "__l");
            let (s2, p2, c2) = borrow("rhs", "__r");
            if cmp.is_some() {
                w.l(1, "/// Rust `Ord`.");
            } else {
                w.l(
                    1,
                    "/// Rust `PartialOrd` (false when the two are not comparable).",
                );
            }
            w.l(
                1,
                &format!(
                    "public static func < (lhs: {}, rhs: {}) -> Bool {{",
                    sname, sname
                ),
            );
            for s in s1.iter().chain(&s2) {
                w.l(2, s);
            }
            w.l(2, &format!("let __o: UInt8 = {}({}, {})", f, p1, p2));
            for s in c1.iter().chain(&c2) {
                w.l(2, s);
            }
            w.l(2, "return __o == 0");
            w.l(1, "}");
            w.l(0, "");
        }
        if let (Some(f), true) = (t(FunctionKind::Hash), copyable_union) {
            let (setup, p, cleanup) = borrow("self", "__s");
            w.l(1, "/// Rust `Hash`.");
            w.l(1, "public func hash(into hasher: inout Hasher) {");
            for s in &setup {
                w.l(2, s);
            }
            w.l(2, &format!("let __h: UInt64 = {}({})", f, p));
            for s in &cleanup {
                w.l(2, s);
            }
            w.l(2, "hasher.combine(__h)");
            w.l(1, "}");
            w.l(0, "");
        }
        // A value type never OWNS a C value: an enum moves the payloads out of
        // one (`init(_take:)`) and writes a fresh one for each call, a struct
        // is `Copy`. Code that talks to `CAzul` directly does get owned
        // values handed to it, and this is their `Drop`. (A class has no need
        // for it: its `deinit` runs the same destructor.)
        if recv != Recv::Class {
            if let Some(f) = m.delete_fn(name) {
                let sel = Selector {
                    is_static: true,
                    base: "deleteRawValue".to_string(),
                    labels: vec!["_".to_string()],
                };
                let sig = format!("inout {}", raw);
                if taken.is_free(&sel, Some(sig.as_str())) {
                    taken.take(sel, Some(sig));
                    w.l(
                        1,
                        "/// Rust's `Drop` for an owned C value, payload included.",
                    );
                    w.l(
                        1,
                        "/// Only for a value code calling `CAzul` directly owns:",
                    );
                    w.l(1, "/// everything this binding hands out frees itself.");
                    w.l(
                        1,
                        &format!("public static func deleteRawValue(_ raw: inout {}) {{", raw),
                    );
                    w.l(2, &format!("{}(&raw)", f));
                    w.l(1, "}");
                    w.l(0, "");
                }
            }
        }
        if let Some(f) = t(FunctionKind::Default) {
            let zero_arg_create = m.functions_of(name).iter().any(|g| {
                matches!(
                    g.kind,
                    FunctionKind::Constructor | FunctionKind::StaticMethod
                ) && g.method_name == "create"
                    && g.args.is_empty()
            });
            let make = match recv {
                Recv::Class => format!("{}(_own: {}())", sname, f),
                Recv::Plain | Recv::Enum => format!("{}(_raw: {}())", sname, f),
                Recv::Union => format!("{}(_take: {}())", sname, f),
            };
            let init_sel = Selector {
                is_static: true,
                base: "init".to_string(),
                labels: vec![],
            };
            if !zero_arg_create && taken.is_free(&init_sel, Some("")) {
                taken.take(init_sel, Some(String::new()));
                w.l(1, "/// Rust `Default`.");
                match recv {
                    Recv::Class => {
                        w.l(1, "public convenience init() {");
                        w.l(2, &format!("self.init(_own: {}())", f));
                    }
                    Recv::Plain | Recv::Enum => {
                        w.l(1, "public init() {");
                        w.l(2, &format!("self.init(_raw: {}())", f));
                    }
                    Recv::Union => {
                        w.l(1, "public init() {");
                        w.l(2, &format!("self.init(_take: {}())", f));
                    }
                }
                w.l(1, "}");
                w.l(0, "");
            } else {
                w.l(1, "/// Rust `Default`.");
                w.l(
                    1,
                    &format!("public static func createDefault() -> {} {{", sname),
                );
                w.l(2, &format!("return {}", make));
                w.l(1, "}");
                w.l(0, "");
                taken.take(
                    Selector {
                        is_static: true,
                        base: "createDefault".to_string(),
                        labels: vec![],
                    },
                    Some(String::new()),
                );
            }
        }
    }

    // ------------------------------------------------------------------------
    // Methods
    // ------------------------------------------------------------------------

    fn emit_methods(&mut self, w: &mut W, class: &str, recv: Recv, taken: &mut Taken) {
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

        // Plan every method first, so getters and setters pair up.
        let mut plans: Vec<Plan> = Vec::new();
        for f in &funcs {
            match self.plan_method(class, recv, f) {
                Some(p) => plans.push(p),
                None => self.skipped.push(f.c_name.clone()),
            }
        }

        // Pair `get_x` / `set_x` into a property when both exist and agree.
        let mut props: BTreeMap<String, (Option<usize>, Option<usize>)> = BTreeMap::new();
        for (i, p) in plans.iter().enumerate() {
            match &p.role {
                Role::Getter(n) => props.entry(n.clone()).or_default().0 = Some(i),
                Role::Setter(n, _) => props.entry(n.clone()).or_default().1 = Some(i),
                Role::Func => {}
            }
        }
        // A setter whose getter is missing or of another type is a method.
        for (_, (g, s)) in props.iter() {
            if let Some(si) = s {
                let ok = g.is_some_and(|gi| match (&plans[gi].role, &plans[*si].role) {
                    (Role::Getter(_), Role::Setter(_, ty)) => plans[gi].ret_type == *ty,
                    _ => false,
                });
                if !ok {
                    plans[*si].role = Role::Func;
                }
            }
        }

        // Resolve names: a clash puts every claimant back to its api.json name.
        let mut claims: BTreeMap<(Selector, Option<String>), usize> = BTreeMap::new();
        for p in &plans {
            if matches!(p.role, Role::Setter(..)) {
                continue;
            }
            *claims.entry((p.selector(), p.sig())).or_default() += 1;
        }
        for p in plans.iter_mut() {
            if matches!(p.role, Role::Setter(..)) {
                continue;
            }
            let key = (p.selector(), p.sig());
            if claims[&key] > 1 || !taken.is_free(&key.0, key.1.as_deref()) {
                p.fallback_name();
            }
        }
        // A setter whose getter lost its property role is a method again.
        for i in 0..plans.len() {
            if let Role::Setter(n, _) = &plans[i].role {
                let getter = props.get(n).and_then(|(g, _)| *g);
                if !getter.is_some_and(|gi| matches!(&plans[gi].role, Role::Getter(g) if g == n)) {
                    plans[i].role = Role::Func;
                }
            }
        }

        let mut emitted_props: BTreeSet<String> = BTreeSet::new();
        for i in 0..plans.len() {
            match plans[i].role.clone() {
                Role::Setter(..) => continue,
                Role::Getter(n) => {
                    let sel = plans[i].selector();
                    if !taken.is_free(&sel, None) || emitted_props.contains(&n) {
                        plans[i].fallback_name();
                        if !self.emit_func(w, &plans[i], taken) {
                            self.skipped.push(format!("{} {}", plans[i].c_name, TAKEN));
                        }
                        continue;
                    }
                    taken.take(sel, None);
                    emitted_props.insert(n.clone());
                    let setter = props
                        .get(&n)
                        .and_then(|(_, s)| *s)
                        .filter(|si| matches!(plans[*si].role, Role::Setter(..)));
                    let p = &plans[i];
                    w.doc(1, &self.rw(&p.doc));
                    w.l(1, &format!("public var {}: {} {{", escape(&n), p.ret_type));
                    w.l(2, "get {");
                    for s in &p.body {
                        w.l(3, s);
                    }
                    w.l(2, "}");
                    if let Some(si) = setter {
                        let sp = &plans[si];
                        w.l(2, "set {");
                        w.l(
                            3,
                            &format!("let {}: {} = newValue", sp.params[0].name, sp.params[0].ty),
                        );
                        for s in &sp.body {
                            w.l(3, s);
                        }
                        w.l(2, "}");
                    }
                    w.l(1, "}");
                    w.l(0, "");
                }
                Role::Func => {
                    if self.emit_func(w, &plans[i], taken) {
                        continue;
                    }
                    let mut p = plans.remove(i);
                    let before = p.name.clone();
                    p.fallback_name();
                    if p.name == before || !self.emit_func(w, &p, taken) {
                        self.skipped.push(format!("{} {}", p.c_name, TAKEN));
                    }
                    plans.insert(i, p);
                }
            }
        }
    }

    /// Emits a planned method if its name is free; false otherwise.
    fn emit_func(&mut self, w: &mut W, p: &Plan, taken: &mut Taken) -> bool {
        let sel = p.selector();
        let sig = p.sig();
        if !taken.is_free(&sel, sig.as_deref()) {
            return false;
        }
        taken.take(sel, sig);
        for t in &p.trampolines {
            self.trampolines.insert(t.clone());
        }
        w.doc(1, &self.rw(&p.doc));
        for line in p.header() {
            w.l(1, &line);
        }
        for s in &p.body {
            w.l(2, s);
        }
        w.l(1, "}");
        w.l(0, "");
        true
    }

    /// Plan one method; None if some argument or the return value has no
    /// Swift-side shape.
    fn plan_method(&self, class: &str, recv: Recv, f: &FunctionDef) -> Option<Plan> {
        let m = self.m;
        let inst =
            matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut) && !f.args.is_empty();
        let args: Vec<&FunctionArg> = f.args.iter().skip(usize::from(inst)).collect();
        let sname = swift_type_name(class);
        let raw = format!("Az{}", class);

        // ---- callbacks -----------------------------------------------------------
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
            if !inst && cb_args.len() == 1 && args.len() == 1 && recv == Recv::Class {
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
        let mut cplans: Vec<CbPlan> = Vec::new();
        let mut generic = false;
        let ctx_slot = m.option_refany_class();
        let option_refany = ctx_slot.is_some_and(|n| matches!(m.owned(n), Ty::Option { .. }));
        // The ctx slot's api.json name, for the `_Conv` helpers and the C
        // symbols written into the generated code (only read once
        // `option_refany` holds, so the empty fallback never reaches Swift).
        let ctx_name = ctx_slot.unwrap_or("");
        for (i, cb) in &cb_args {
            let td = cb.td;
            let ctx_arg = td.args.iter().any(|a| {
                a.ref_kind == ArgRefKind::Owned && m.ctx_getter(a.type_name.trim()).is_some()
            });
            let data_first = td.args.first().is_some_and(|a| {
                a.ref_kind == ArgRefKind::Owned && matches!(m.owned(&a.type_name), Ty::RefAny)
            });
            let ctx_usable = ctx_arg
                && ctx_slot.and_then(|n| m.fun(n, "delete")).is_some()
                && (cb.wrapper.is_some() || layout_factory.is_some());
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
        if layout_factory.is_some() && !cplans.first().is_some_and(|p| p.closure) {
            // The raw constructor is still bound, just without a closure.
        }
        let layout_factory = layout_factory.filter(|_| cplans.first().is_some_and(|p| p.closure));

        // ---- parameters + setup ----------------------------------------------------
        let mut params: Vec<Param> = Vec::new();
        let mut pre: Vec<String> = Vec::new();
        let mut body: Vec<String> = Vec::new();
        let mut post: Vec<String> = Vec::new();
        let mut call_args: Vec<String> = Vec::new();
        let mut trampolines: Vec<String> = Vec::new();
        let mut used_names: BTreeSet<String> = BTreeSet::new();

        for (i, a) in args.iter().enumerate() {
            let mut pname = camel(&a.name);
            if pname.is_empty() || pname.starts_with(|c: char| c.is_ascii_digit()) {
                pname = format!("arg{}", i);
            }
            while !used_names.insert(pname.clone()) {
                pname.push('_');
            }
            let ename = escape(&pname);
            let local = format!("__a{}", i);
            let label_for = |ty_name: &str| -> String {
                if i == 0 || camel(&a.name).eq_ignore_ascii_case(ty_name) {
                    "_".to_string()
                } else {
                    escape(&pname)
                }
            };

            if let Some(plan) = cplans.iter().find(|p| p.arg == i) {
                let cb = &cb_args.iter().find(|(j, _)| *j == i).unwrap().1;
                let td = cb.td;
                if plan.closure {
                    let user = self.user_closure_type(td, plan.data_first)?;
                    params.push(Param {
                        label: label_for(""),
                        name: ename.clone(),
                        ty: format!("@escaping {}", user),
                    });
                    pre.extend(self.erased_closure(
                        td,
                        &ename,
                        &format!("__erased{}", i),
                        plan.data_first,
                    ));
                    trampolines.push(td.name.clone());
                    let tramp = format!("_Trampolines.{}", td.name);
                    match &cb.wrapper {
                        Some((wstruct, cb_field, ctx_field)) => {
                            body.push(format!("var {}: Az{} = Az{}()", local, wstruct, wstruct));
                            body.push(format!(
                                "{}.{} = {}",
                                local,
                                escape(&super::c_member_name(cb_field)),
                                tramp
                            ));
                            if plan.ctx_usable {
                                body.push(format!(
                                    "{}.{} = _Conv.wrap_{}(_Handles.refany(object: nil, closure: \
                                     _Closure(__erased{})))",
                                    local,
                                    escape(&super::c_member_name(ctx_field)),
                                    ctx_name,
                                    i
                                ));
                            } else {
                                body.push(format!(
                                    "{}.{} = _Conv.in_{}(nil)",
                                    local,
                                    escape(&super::c_member_name(ctx_field)),
                                    ctx_name
                                ));
                            }
                            call_args.push(local.clone());
                        }
                        None => call_args.push(tramp),
                    }
                } else {
                    // Raw: a C function pointer.
                    params.push(Param {
                        label: label_for(""),
                        name: ename.clone(),
                        ty: format!("Az{}", td.name),
                    });
                    match &cb.wrapper {
                        Some((wstruct, cb_field, ctx_field)) => {
                            if !option_refany {
                                return None;
                            }
                            body.push(format!("var {}: Az{} = Az{}()", local, wstruct, wstruct));
                            body.push(format!(
                                "{}.{} = {}",
                                local,
                                escape(&super::c_member_name(cb_field)),
                                ename
                            ));
                            body.push(format!(
                                "{}.{} = _Conv.in_{}(nil)",
                                local,
                                escape(&super::c_member_name(ctx_field)),
                                ctx_name
                            ));
                            call_args.push(local.clone());
                        }
                        None => call_args.push(ename.clone()),
                    }
                }
                continue;
            }

            // A data RefAny that also carries the closure.
            if let Some(plan) = cplans.iter().find(|p| p.carry_in_data == Some(i)) {
                params.push(Param {
                    label: label_for(""),
                    name: ename.clone(),
                    ty: "T".to_string(),
                });
                body.push(format!(
                    "let {}: AzRefAny = _Handles.refany(object: {}, closure: _Closure(__erased{}))",
                    local, ename, plan.arg
                ));
                call_args.push(local.clone());
                continue;
            }
            // A plain data RefAny next to a closure registered through ctx: it
            // is still handed back to the closure as `T`.
            if generic
                && a.ref_kind == ArgRefKind::Owned
                && refany_args.len() == 1
                && refany_args[0] == i
            {
                params.push(Param {
                    label: label_for(""),
                    name: ename.clone(),
                    ty: "T".to_string(),
                });
                body.push(format!(
                    "let {}: AzRefAny = _Handles.refany(object: {}, closure: nil)",
                    local, ename
                ));
                call_args.push(local.clone());
                continue;
            }

            let ty = m.not_result(m.owned(&a.type_name));
            if matches!(ty, Ty::Callback(_)) {
                return None;
            }
            let tyname = exact(&ty).unwrap_or_default();
            match a.ref_kind {
                ArgRefKind::Owned => {
                    let r = restriction(&ty)?;
                    let e = m.in_expr(&ty, &ename)?;
                    params.push(Param {
                        label: label_for(&tyname),
                        name: ename.clone(),
                        ty: r,
                    });
                    body.push(format!("let {}: {} = {}", local, c_type(&ty)?, e));
                    call_args.push(local);
                }
                ArgRefKind::Ref => match &ty {
                    Ty::Class(_) | Ty::RefAny => {
                        params.push(Param {
                            label: label_for(&tyname),
                            name: ename.clone(),
                            ty: exact(&ty)?,
                        });
                        call_args.push(format!("{}._address", ename));
                    }
                    Ty::RawPtr(_)
                    | Ty::Void
                    | Ty::Unsupported(_)
                    | Ty::Callback(_)
                    | Ty::Result { .. } => return None,
                    _ => {
                        let e = m.borrow_expr(&ty, &ename)?;
                        params.push(Param {
                            label: label_for(&tyname),
                            name: ename.clone(),
                            ty: restriction(&ty)?,
                        });
                        body.push(format!("var {}: {} = {}", local, c_type(&ty)?, e));
                        call_args.push(format!("&{}", local));
                        if let Some(c) = m.release(&ty, &format!("&{}", local)) {
                            post.push(c);
                        }
                    }
                },
                ArgRefKind::RefMut => match &ty {
                    Ty::Class(_) | Ty::RefAny => {
                        params.push(Param {
                            label: label_for(&tyname),
                            name: ename.clone(),
                            ty: exact(&ty)?,
                        });
                        call_args.push(format!("{}._address", ename));
                    }
                    Ty::Prim(_) => {
                        params.push(Param {
                            label: label_for(&tyname),
                            name: ename.clone(),
                            ty: format!("inout {}", exact(&ty)?),
                        });
                        call_args.push(format!("&{}", ename));
                    }
                    Ty::Plain(_) => {
                        params.push(Param {
                            label: label_for(&tyname),
                            name: ename.clone(),
                            ty: format!("inout {}", exact(&ty)?),
                        });
                        call_args.push(format!("&{}._raw", ename));
                    }
                    Ty::Enum(_) | Ty::Str | Ty::Union(_) | Ty::Option { .. } | Ty::Vec { .. } => {
                        let e = m.in_expr(&ty, &ename)?;
                        let back = m.take_expr(&ty, &local)?;
                        params.push(Param {
                            label: label_for(&tyname),
                            name: ename.clone(),
                            ty: format!("inout {}", exact(&ty)?),
                        });
                        body.push(format!("var {}: {} = {}", local, c_type(&ty)?, e));
                        call_args.push(format!("&{}", local));
                        post.push(format!("{} = {}", ename, back));
                    }
                    _ => return None,
                },
                // A pointer to a class instance is that instance, borrowed.
                ArgRefKind::Ptr | ArgRefKind::PtrMut if matches!(ty, Ty::Class(_) | Ty::RefAny) => {
                    params.push(Param {
                        label: label_for(&tyname),
                        name: ename.clone(),
                        ty: exact(&ty)?,
                    });
                    call_args.push(format!("{}._address", ename));
                }
                ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                    let p = pointee_swift(&a.type_name, a.ref_kind == ArgRefKind::PtrMut, m)?;
                    params.push(Param {
                        label: label_for(""),
                        name: ename.clone(),
                        ty: p,
                    });
                    call_args.push(ename.clone());
                }
            }
        }

        pre.append(&mut body);
        let mut body = pre;

        // ---- receiver --------------------------------------------------------------
        let recv_arg = if inst { f.args.first() } else { None };
        let ret = m.ret_ty(f.return_type.as_deref());
        let returns_self =
            matches!(&ret, Ty::Class(n) | Ty::Plain(n) | Ty::Union(n) | Ty::Enum(n) if n == class);
        let builder = matches!(recv_arg, Some(r) if r.ref_kind == ArgRefKind::Owned)
            && returns_self
            && recv == Recv::Class;
        let mut mutating = false;
        if let Some(r) = recv_arg {
            let own = r.ref_kind == ArgRefKind::Owned;
            let is_mut = matches!(r.ref_kind, ArgRefKind::RefMut | ArgRefKind::PtrMut);
            let s = match recv {
                Recv::Class => {
                    if own {
                        body.push(format!(
                            "let __self: {} = {}",
                            raw,
                            if builder { "_value()" } else { "_take()" }
                        ));
                        "__self".to_string()
                    } else {
                        "_address".to_string()
                    }
                }
                Recv::Plain => {
                    if own {
                        "_raw".to_string()
                    } else if is_mut {
                        mutating = true;
                        "&_raw".to_string()
                    } else {
                        body.push(format!("var __self: {} = _raw", raw));
                        "&__self".to_string()
                    }
                }
                Recv::Enum => {
                    if own {
                        "_raw".to_string()
                    } else {
                        body.push(format!("var __self: {} = _raw", raw));
                        if is_mut {
                            mutating = true;
                            post.push(format!("self = {}(_raw: __self)", sname));
                        }
                        "&__self".to_string()
                    }
                }
                Recv::Union => {
                    if own {
                        body.push(format!("let __self: {} = _toRaw()", raw));
                        "__self".to_string()
                    } else if is_mut {
                        mutating = true;
                        body.push(format!("var __self: {} = _toRaw()", raw));
                        post.push(format!("self = {}(_take: __self)", sname));
                        "&__self".to_string()
                    } else {
                        let t = Ty::Union(class.to_string());
                        body.push(format!(
                            "var __self: {} = {}",
                            raw,
                            m.borrow_expr(&t, "self")?
                        ));
                        if let Some(r) = m.release(&t, "&__self") {
                            post.insert(0, r);
                        }
                        "&__self".to_string()
                    }
                }
            };
            call_args.insert(0, s);
        }

        // ---- return + role -----------------------------------------------------------
        let symbol = self.symbol(f, &cb_args);
        let call = format!("{}({})", symbol, call_args.join(", "));
        let throws = matches!(ret, Ty::Result { .. });
        let mut kind = if inst {
            MethodKind::Instance
        } else {
            MethodKind::Static
        };
        let raw_name = camel(&f.method_name);
        let mut name = raw_name.clone();
        let mut role = Role::Func;
        let ret_type: String;
        let is_create = !inst && f.method_name == "create";
        let ctor = is_create && returns_self && !throws;
        let arity = params.len();

        if layout_factory.is_some() || ctor {
            kind = MethodKind::Init;
            name = "init".to_string();
            ret_type = String::new();
            let make = match recv {
                Recv::Class => "self.init(_own: __r)",
                Recv::Plain | Recv::Enum => "self.init(_raw: __r)",
                Recv::Union => "self.init(_take: __r)",
            };
            body.push(format!("let __r: {} = {}", raw, call));
            body.append(&mut post);
            body.push(make.to_string());
            if let Some(fac) = &layout_factory {
                let mut path: Vec<String> = fac
                    .field_path
                    .iter()
                    .map(|seg| escape(&super::c_member_name(seg)))
                    .collect();
                let ctx_field = callback_ctx_field(&fac.callback_wrapper, m.ir)
                    .unwrap_or_else(|| "ctx".to_string());
                path.push(escape(&super::c_member_name(&ctx_field)));
                body.push(format!(
                    "let __ctx: UnsafeMutablePointer<Az{}> = _ptr.pointer(to: \\{}.{})!",
                    ctx_name,
                    raw,
                    path.join(".")
                ));
                // The factory already filled the slot: free it before writing.
                if let Some(d) = m.delete_fn(ctx_name) {
                    body.push(format!("{}(__ctx)", d));
                }
                body.push(format!(
                    "__ctx.pointee = _Conv.wrap_{}(_Handles.refany(object: nil, closure: \
                     _Closure(__erased0)))",
                    ctx_name
                ));
            }
        } else {
            let (ann, line) = if builder {
                body.push(format!("let __r: {} = {}", raw, call));
                body.append(&mut post);
                body.push("_replace(__r)".to_string());
                body.push("return self".to_string());
                (sname.clone(), None)
            } else {
                match &ret {
                    Ty::Void => {
                        body.push(call.clone());
                        body.append(&mut post);
                        ("Void".to_string(), None)
                    }
                    t => {
                        let a = exact(t)?;
                        let e = m.take_expr(t, "__r")?;
                        body.push(format!("let __r: {} = {}", c_type(t)?, call));
                        body.append(&mut post);
                        (a, Some(format!("return {}", e)))
                    }
                }
            };
            if let Some(l) = line {
                body.push(l);
            }
            ret_type = ann;
            if !inst {
                if let Some(rest) = f.method_name.strip_prefix("create_") {
                    name = camel(rest);
                }
            } else {
                let raw_snake = &f.method_name;
                let getter_ok = arity == 0
                    && !mutating
                    && !throws
                    && !generic
                    && ret_type != "Void"
                    && !matches!(recv_arg, Some(r) if r.ref_kind == ArgRefKind::Owned);
                if let (Some(rest), true) = (raw_snake.strip_prefix("get_"), getter_ok) {
                    role = Role::Getter(camel(rest));
                } else if getter_ok
                    && ret_type == "Bool"
                    && (raw_snake.starts_with("is_")
                        || raw_snake.starts_with("has_")
                        || raw_snake.starts_with("can_"))
                {
                    role = Role::Getter(raw_name.clone());
                } else if let (Some(rest), 1, "Void", false) = (
                    raw_snake.strip_prefix("set_"),
                    arity,
                    ret_type.as_str(),
                    generic,
                ) {
                    let only = &params[0];
                    if cb_args.is_empty() && !only.ty.starts_with("inout ") {
                        role = Role::Setter(camel(rest), only.ty.clone());
                    }
                }
            }
        }
        if let Role::Getter(n) | Role::Setter(n, _) = &role {
            if reserved_member(n) || n.is_empty() {
                role = Role::Func;
            }
        }
        if reserved_member(&name) && kind != MethodKind::Init {
            name = format!("{}_", name);
        }

        Some(Plan {
            c_name: f.c_name.clone(),
            doc: f.doc.clone(),
            kind,
            name,
            raw_name: if reserved_member(&raw_name) {
                format!("{}_", raw_name)
            } else {
                raw_name
            },
            params,
            ret_type,
            throws,
            generic,
            mutating,
            discardable: builder,
            body,
            role,
            trampolines,
            convenience: recv == Recv::Class,
        })
    }

    /// The C symbol a method binds: the `Struct` variant when a wrapper-struct
    /// callback is passed to a function the header pairs (its plain symbol
    /// takes the bare function pointer).
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

    /// The C-level Swift type of callback arg `a`, as the importer spells it
    /// inside a `@convention(c)` function type.
    fn cb_c_arg(&self, a: &FunctionArg) -> Option<String> {
        let m = self.m;
        match a.ref_kind {
            ArgRefKind::Owned => c_type(&m.owned(&a.type_name)),
            ArgRefKind::Ref | ArgRefKind::Ptr => pointee_swift(&a.type_name, false, m),
            ArgRefKind::RefMut | ArgRefKind::PtrMut => pointee_swift(&a.type_name, true, m),
        }
    }

    fn cb_c_ret(&self, td: &CallbackTypedefDef) -> Option<String> {
        match td.return_type.as_deref() {
            None => Some("Void".to_string()),
            Some(r) => match self.m.owned(r) {
                Ty::Void => Some("Void".to_string()),
                t => c_type(&t),
            },
        }
    }

    /// The Swift-side type of callback arg `j`.
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
            return Some(("Void".to_string(), ty));
        }
        let ex = match &ty {
            Ty::RefAny => "AnyObject".to_string(),
            other => restriction(other)?,
        };
        self.m.in_expr(&ty, "x")?;
        Some((ex, ty))
    }

    fn callback_convertible(&self, td: &CallbackTypedefDef) -> bool {
        (0..td.args.len()).all(|j| {
            self.cb_user_arg(td, j, true).is_some() && self.cb_c_arg(&td.args[j]).is_some()
        }) && self.cb_user_ret(td).is_some()
            && self.cb_c_ret(td).is_some()
    }

    fn user_closure_type(&self, td: &CallbackTypedefDef, data_first: bool) -> Option<String> {
        let mut parts = Vec::new();
        for j in 0..td.args.len() {
            parts.push(self.cb_user_arg(td, j, data_first)?.0);
        }
        Some(format!(
            "({}) -> {}",
            parts.join(", "),
            self.cb_user_ret(td)?.0
        ))
    }

    fn erased_type(&self, td: &CallbackTypedefDef) -> Option<String> {
        let args: Vec<String> = td
            .args
            .iter()
            .map(|a| self.cb_c_arg(a))
            .collect::<Option<_>>()?;
        Some(format!("({}) -> {}", args.join(", "), self.cb_c_ret(td)?))
    }

    /// The closure the trampoline calls: C values in, the user's closure in
    /// the middle, a C value out.
    fn erased_closure(
        &self,
        td: &CallbackTypedefDef,
        user: &str,
        var: &str,
        data_first: bool,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let params: Vec<String> = td
            .args
            .iter()
            .enumerate()
            .map(|(j, a)| format!("__c{}: {}", j, self.cb_c_arg(a).unwrap()))
            .collect();
        let ret = self.cb_c_ret(td).unwrap();
        out.push(format!(
            "let {}: {} = {{ ({}) -> {} in",
            var,
            self.erased_type(td).unwrap(),
            params.join(", "),
            ret
        ));
        let mut call = Vec::new();
        for j in 0..td.args.len() {
            if j == 0 && data_first {
                out.push("    var __p0: AzRefAny = __c0".to_string());
                out.push("    let __u0: T = _Handles.object(&__p0, T.self)".to_string());
            } else {
                let (_, ty) = self.cb_user_arg(td, j, data_first).unwrap();
                let e = match ty {
                    Some(ty) => self.m.take_expr(&ty, &format!("__c{}", j)).unwrap(),
                    None => format!("__c{}", j),
                };
                out.push(format!("    let __u{} = {}", j, e));
            }
            call.push(format!("__u{}", j));
        }
        let (_, rty) = self.cb_user_ret(td).unwrap();
        match rty {
            Ty::Void => {
                out.push(format!("    {}({})", user, call.join(", ")));
            }
            ty => {
                out.push(format!("    let __ur = {}({})", user, call.join(", ")));
                out.push(format!(
                    "    return {}",
                    self.m.in_expr(&ty, "__ur").unwrap()
                ));
            }
        }
        out.push("}".to_string());
        out
    }

    fn emit_trampolines(&mut self, w: &mut W) {
        let m = self.m;
        for tdn in self.trampolines.clone() {
            let td = m.callbacks[&tdn];
            let (Some(erased), Some(ret)) = (self.erased_type(td), self.cb_c_ret(td)) else {
                continue;
            };
            let params: Vec<String> = td
                .args
                .iter()
                .enumerate()
                .map(|(j, a)| format!("__c{}: {}", j, self.cb_c_arg(a).unwrap()))
                .collect();
            let fparams: Vec<String> = params.iter().map(|p| format!("_ {}", p)).collect();
            let names: Vec<String> = (0..td.args.len()).map(|j| format!("__c{}", j)).collect();
            let data_first = td.args.first().is_some_and(|a| {
                a.ref_kind == ArgRefKind::Owned && matches!(m.owned(&a.type_name), Ty::RefAny)
            });
            // The data RefAny arrives by value: the trampoline owns it and
            // frees it again once the Swift closure has returned.
            let drop_data = m
                .refany_class()
                .and_then(|n| m.delete_fn(n))
                .map(|f| format!("{}(&__d)", f));
            let ctx = td.args.iter().enumerate().find_map(|(j, a)| {
                (a.ref_kind == ArgRefKind::Owned)
                    .then(|| m.ctx_getter(a.type_name.trim()))
                    .flatten()
                    .map(|g| (j, g, a.type_name.trim().to_string()))
            });
            w.l(0, "extension _Trampolines {");
            w.l(
                1,
                &format!(
                    "static let {}: Az{} = {{ ({}) -> {} in",
                    tdn,
                    tdn,
                    params.join(", "),
                    ret
                ),
            );
            w.l(
                2,
                &format!("return _Trampolines.call_{}({})", tdn, names.join(", ")),
            );
            w.l(1, "}");
            w.l(0, "");
            w.l(
                1,
                &format!(
                    "static func call_{}({}) -> {} {{",
                    tdn,
                    fparams.join(", "),
                    ret
                ),
            );
            w.l(2, &format!("var __f: ({})? = nil", erased));
            // The ctx getter answers with the `Option<RefAny>` slot; its
            // api.json name is what the `_Conv` helper is spelled with.
            let ctx_slot = m.option_refany_class().unwrap_or("");
            if let Some((j, getter, info)) = &ctx {
                w.l(2, &format!("var __i: Az{} = __c{}", info, j));
                w.l(
                    2,
                    &format!(
                        "if let __ctx = _Conv.take_{}({}(&__i)) {{",
                        ctx_slot, getter
                    ),
                );
                w.l(
                    3,
                    &format!("__f = _Handles.closure(__ctx._address, ({}).self)", erased),
                );
                w.l(2, "}");
            }
            if data_first {
                w.l(2, "var __d: AzRefAny = __c0");
                w.l(2, "if __f == nil {");
                w.l(
                    3,
                    &format!("__f = _Handles.closure(&__d, ({}).self)", erased),
                );
                w.l(2, "}");
            }
            w.l(2, "guard let __call = __f else {");
            w.l(
                3,
                &format!(
                    "preconditionFailure(\"azul: no Swift closure is registered for this {}\")",
                    tdn
                ),
            );
            w.l(2, "}");
            if ret == "Void" {
                w.l(2, &format!("__call({})", names.join(", ")));
                if let (true, Some(d)) = (data_first, &drop_data) {
                    w.l(2, d);
                }
            } else {
                w.l(
                    2,
                    &format!("let __result: {} = __call({})", ret, names.join(", ")),
                );
                if let (true, Some(d)) = (data_first, &drop_data) {
                    w.l(2, d);
                }
                w.l(2, "return __result");
            }
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
        let (Ty::Option { name, payload }, Shape::Union { tag_u8, variants }) = (t, &c.shape)
        else {
            return;
        };
        let none = variants.iter().find(|v| v.name == "None").unwrap();
        let some = variants.iter().find(|v| v.name == "Some").unwrap();
        let raw = format!("Az{}", name);
        let (Some(restr), Some(ex), Some(pc), Some(in_e), Some(out_e), Some(take_e)) = (
            restriction(payload),
            exact(payload),
            c_type(payload),
            m.in_expr(payload, "__x"),
            m.out_expr(payload, "__q"),
            m.take_expr(
                payload,
                &format!("v.Some.{}", escape(&some.fields[0].c_name)),
            ),
        ) else {
            return;
        };
        let some_struct = format!("{}Variant_Some", raw);
        let none_struct = format!("{}Variant_None", raw);
        let payload_member = escape(&some.fields[0].c_name);
        w.l(0, "extension _Conv {");
        w.l(
            1,
            &format!("static func wrap_{}(_ payload: {}) -> {} {{", name, pc, raw),
        );
        // The exported constructors own the tag; the union is only written
        // out here when libazul does not export them.
        match self.variant_ctor(name, some) {
            Some(ctor) => w.l(2, &format!("return {}(payload)", ctor)),
            None => {
                w.l(2, &format!("var __v: {} = {}()", some_struct, some_struct));
                w.l(2, &format!("__v.tag = {}", tag_value(c, some, *tag_u8)));
                w.l(2, &format!("__v.{} = payload", payload_member));
                w.l(2, &format!("var __r: {} = {}()", raw, raw));
                w.l(2, "__r.Some = __v");
                w.l(2, "return __r");
            }
        }
        w.l(1, "}");
        w.l(0, "");
        w.l(1, &format!("static func none_{}() -> {} {{", name, raw));
        match self.variant_ctor(name, none) {
            Some(ctor) => w.l(2, &format!("return {}()", ctor)),
            None => {
                w.l(2, &format!("var __v: {} = {}()", none_struct, none_struct));
                w.l(2, &format!("__v.tag = {}", tag_value(c, none, *tag_u8)));
                w.l(2, &format!("var __r: {} = {}()", raw, raw));
                w.l(2, "__r.None = __v");
                w.l(2, "return __r");
            }
        }
        w.l(1, "}");
        w.l(0, "");
        w.l(
            1,
            &format!("static func in_{}(_ x: {}?) -> {} {{", name, restr, raw),
        );
        w.l(2, "guard let __x = x else {");
        w.l(3, &format!("return none_{}()", name));
        w.l(2, "}");
        w.l(2, &format!("return wrap_{}({})", name, in_e));
        w.l(1, "}");
        w.l(0, "");
        let mut borrowable = !m.moves(payload);
        if m.moves(payload) {
            if let Some(borrow_e) = m.borrow_expr(payload, "__x") {
                borrowable = true;
                w.l(
                    1,
                    &format!("static func borrow_{}(_ x: {}?) -> {} {{", name, restr, raw),
                );
                w.l(2, "guard let __x = x else {");
                w.l(3, &format!("return none_{}()", name));
                w.l(2, "}");
                w.l(2, &format!("return wrap_{}({})", name, borrow_e));
                w.l(1, "}");
                w.l(0, "");
                w.l(
                    1,
                    &format!(
                        "static func release_{}(_ p: UnsafeMutablePointer<{}>) {{",
                        name, raw
                    ),
                );
                w.l(
                    2,
                    &format!(
                        "if {} == {} {{",
                        tag_read(none, "p.pointee", *tag_u8),
                        none.index
                    ),
                );
                w.l(3, "return");
                w.l(2, "}");
                if let Some(r) = m.release(payload, "__q") {
                    w.l(
                        2,
                        &format!(
                            "let __q: UnsafeMutablePointer<{}> = UnsafeMutableRawPointer(p).advanced(by: MemoryLayout<{}>.offset(of: \\{}.{})!).assumingMemoryBound(to: {}.self)",
                            pc, some_struct, some_struct, payload_member, pc
                        ),
                    );
                    w.l(2, &r);
                }
                w.l(1, "}");
                w.l(0, "");
            }
        }
        w.l(
            1,
            &format!(
                "static func out_{}(_ p: UnsafeMutablePointer<{}>) -> {}? {{",
                name, raw, ex
            ),
        );
        w.l(
            2,
            &format!(
                "if {} == {} {{",
                tag_read(none, "p.pointee", *tag_u8),
                none.index
            ),
        );
        w.l(3, "return nil");
        w.l(2, "}");
        w.l(
            2,
            &format!(
                "let __q: UnsafeMutablePointer<{}> = UnsafeMutableRawPointer(p).advanced(by: MemoryLayout<{}>.offset(of: \\{}.{})!).assumingMemoryBound(to: {}.self)",
                pc, some_struct, some_struct, payload_member, pc
            ),
        );
        w.l(2, &format!("return {}", out_e));
        w.l(1, "}");
        w.l(0, "");
        w.l(
            1,
            &format!("static func take_{}(_ v: {}) -> {}? {{", name, raw, ex),
        );
        w.l(
            2,
            &format!("if {} == {} {{", tag_read(none, "v", *tag_u8), none.index),
        );
        w.l(3, "return nil");
        w.l(2, "}");
        w.l(2, &format!("return {}", take_e));
        w.l(1, "}");
        w.l(0, "}");
        w.l(0, "");
        self.emit_option_traits(w, name, payload, &ex, borrowable);
    }

    /// What Rust derives for `Option<T>`, on Swift's `T?`.
    ///
    /// Swift's own `==`, `<`, `hashValue` and `description` on `Optional`
    /// need `T` to conform - which a libazul-backed type does only when the
    /// Rust type derives the trait - and even then Swift answers its own
    /// way, not the way `Option<T>` does in Rust. So these call the exported
    /// C functions on a temporary `AzOption{T}`, and they are `azul`-prefixed
    /// because a member of `Optional` must not shadow one the standard
    /// library already has.
    ///
    /// The temporary is a bitwise VIEW when the payload is a wrapper object
    /// the caller keeps (`borrow_` / `release_` free only what this built
    /// fresh), and a fresh C value otherwise (freed as a whole by the
    /// option's own `Drop`). Only one extension per `Wrapped` spelling: two
    /// api.json options over the same Swift type would redeclare each other.
    fn emit_option_traits(
        &mut self,
        w: &mut W,
        name: &str,
        payload: &Ty,
        ex: &str,
        borrowable: bool,
    ) {
        let m = self.m;
        if !borrowable || !m.convertible(payload) {
            return;
        }
        let raw = format!("Az{}", name);
        let t = |k| m.trait_fn(name, k);
        let moves = m.moves(payload);
        let make = |local: &str, from: &str| {
            format!(
                "var {}: {} = _Conv.{}_{}({})",
                local,
                raw,
                if moves { "borrow" } else { "in" },
                name,
                from
            )
        };
        let drop_it = |local: &str| -> Option<String> {
            if moves {
                Some(format!("_Conv.release_{}(&{})", name, local))
            } else {
                m.delete_fn(name).map(|d| format!("{}(&{})", d, local))
            }
        };
        let mut body = W::default();
        if let Some(f) = t(FunctionKind::DebugToString) {
            body.l(1, "/// Rust `Debug` for the `Option` this crosses as.");
            body.l(1, "public var azulDescription: String {");
            body.l(2, &make("__s", "self"));
            body.l(2, &format!("let __r: AzString = {}(&__s)", f));
            if let Some(d) = drop_it("__s") {
                body.l(2, &d);
            }
            body.l(2, "return _Native.takeString(__r)");
            body.l(1, "}");
            body.l(0, "");
        }
        if let Some(f) = t(FunctionKind::PartialEq) {
            body.l(1, "/// Rust `PartialEq` for the `Option` this crosses as.");
            body.l(
                1,
                &format!("public func azulEquals(_ other: {}?) -> Bool {{", ex),
            );
            body.l(2, &make("__l", "self"));
            body.l(2, &make("__r", "other"));
            body.l(2, &format!("let __eq: Bool = {}(&__l, &__r)", f));
            for l in ["__l", "__r"] {
                if let Some(d) = drop_it(l) {
                    body.l(2, &d);
                }
            }
            body.l(2, "return __eq");
            body.l(1, "}");
            body.l(0, "");
        }
        // Rust's C ordering: 0 Less, 1 Equal, 2 Greater, 255 incomparable.
        if let Some(f) = t(FunctionKind::Cmp) {
            body.l(
                1,
                "/// Rust `Ord` for the `Option` this crosses as: -1, 0 or 1.",
            );
            body.l(
                1,
                &format!("public func azulCompare(_ other: {}?) -> Int {{", ex),
            );
            body.l(2, &make("__l", "self"));
            body.l(2, &make("__r", "other"));
            body.l(2, &format!("let __o: UInt8 = {}(&__l, &__r)", f));
            for l in ["__l", "__r"] {
                if let Some(d) = drop_it(l) {
                    body.l(2, &d);
                }
            }
            body.l(2, "return Int(__o) - 1");
            body.l(1, "}");
            body.l(0, "");
        }
        if let Some(f) = t(FunctionKind::PartialCmp) {
            body.l(
                1,
                "/// Rust `PartialOrd` for the `Option` this crosses as: -1, 0",
            );
            body.l(1, "/// or 1, nil when the two do not compare.");
            body.l(
                1,
                &format!(
                    "public func azulPartialCompare(_ other: {}?) -> Int? {{",
                    ex
                ),
            );
            body.l(2, &make("__l", "self"));
            body.l(2, &make("__r", "other"));
            body.l(2, &format!("let __o: UInt8 = {}(&__l, &__r)", f));
            for l in ["__l", "__r"] {
                if let Some(d) = drop_it(l) {
                    body.l(2, &d);
                }
            }
            body.l(2, "if __o > 2 {");
            body.l(3, "return nil");
            body.l(2, "}");
            body.l(2, "return Int(__o) - 1");
            body.l(1, "}");
            body.l(0, "");
        }
        if let Some(f) = t(FunctionKind::Hash) {
            body.l(1, "/// Rust `Hash` for the `Option` this crosses as.");
            body.l(1, "public var azulHashValue: UInt64 {");
            body.l(2, &make("__s", "self"));
            body.l(2, &format!("let __h: UInt64 = {}(&__s)", f));
            if let Some(d) = drop_it("__s") {
                body.l(2, &d);
            }
            body.l(2, "return __h");
            body.l(1, "}");
            body.l(0, "");
        }
        if let Some(f) = t(FunctionKind::Default) {
            body.l(1, "/// Rust `Default` for the `Option` this crosses as.");
            body.l(1, &format!("public static func azulDefault() -> {}? {{", ex));
            body.l(2, &format!("return _Conv.take_{}({}())", name, f));
            body.l(1, "}");
            body.l(0, "");
        }
        match (t(FunctionKind::DeepCopy), m.delete_fn(name)) {
            (Some(f), Some(del)) => {
                body.l(1, "/// Rust `Clone` for the `Option` this crosses as: the");
                body.l(1, "/// C copy owns a deep copy of the payload, the Swift");
                body.l(1, "/// value is read out of it, and `Drop` frees it again.");
                body.l(1, &format!("public func azulCopy() -> {}? {{", ex));
                body.l(2, &make("__s", "self"));
                body.l(2, &format!("var __c: {} = {}(&__s)", raw, f));
                if let Some(d) = drop_it("__s") {
                    body.l(2, &d);
                }
                body.l(2, &format!("let __r: {}? = _Conv.out_{}(&__c)", ex, name));
                body.l(2, &format!("{}(&__c)", del));
                body.l(2, "return __r");
                body.l(1, "}");
                body.l(0, "");
            }
            // No `Clone`: nothing here ever owns a C option, so its `Drop` is
            // exposed the way a union's is - for code calling `CAzul`.
            (None, Some(del)) => {
                body.l(1, "/// Rust's `Drop` for an owned C option, payload included.");
                body.l(1, "/// Only for a value code calling `CAzul` directly owns:");
                body.l(1, "/// everything this binding hands out frees itself.");
                body.l(
                    1,
                    &format!("public static func deleteRawValue(_ raw: inout {}) {{", raw),
                );
                body.l(2, &format!("{}(&raw)", del));
                body.l(1, "}");
                body.l(0, "");
            }
            _ => {}
        }
        // Nothing derived, or another option over the same Swift type has
        // the extension already (a second one would redeclare it).
        if body.out.trim().is_empty() || !self.option_extensions.insert(ex.to_string()) {
            return;
        }
        w.l(0, &format!("extension Optional where Wrapped == {} {{", ex));
        w.out.push_str(&body.out);
        w.l(0, "}");
        w.l(0, "");
    }

    fn emit_vec_conv(&mut self, w: &mut W, c: &ClassInfo, t: &Ty) {
        let m = self.m;
        let Ty::Vec { name, elem } = t else { return };
        let Shape::Struct(fields) = &c.shape else {
            return;
        };
        let ptr_field = fields.iter().find(|f| f.name == "ptr").unwrap();
        let const_ptr = ptr_field.ref_kind == super::super::ir::FieldRefKind::Ptr;
        let raw = format!("Az{}", name);
        let copy_fn = m.fun(name, "copyFromPtr").unwrap();
        let (Some(restr), Some(ex), Some(ec)) = (restriction(elem), exact(elem), c_type(elem))
        else {
            return;
        };
        let bytes = matches!(elem.as_ref(), Ty::Prim(Prim::U8));
        // `_copyFromPtr` clones every element, so the buffer only borrows.
        let (in_e, in_cleanup) = (m.borrow_expr(elem, "__e"), m.release(elem, "__buf + __i"));
        let q = if const_ptr {
            "UnsafeMutablePointer(mutating: __base + __i)".to_string()
        } else {
            "(__base + __i)".to_string()
        };
        let out_e = m.out_expr(elem, "__q");
        let (Some(in_e), Some(out_e)) = (in_e, out_e) else {
            return;
        };
        w.l(0, "extension _Conv {");
        // native -> C
        w.l(
            1,
            &format!("static func in_{}(_ x: [{}]) -> {} {{", name, restr, raw),
        );
        w.l(2, "let __n: Int = x.count");
        w.l(2, &format!("let __buf: UnsafeMutablePointer<{}> = UnsafeMutablePointer<{}>.allocate(capacity: Swift.max(__n, 1))", ec, ec));
        w.l(2, "defer { __buf.deallocate() }");
        if bytes {
            w.l(
                2,
                "x.withUnsafeBufferPointer { (__b: UnsafeBufferPointer<UInt8>) -> Void in",
            );
            w.l(3, "if let __s = __b.baseAddress {");
            w.l(4, "__buf.initialize(from: __s, count: __n)");
            w.l(3, "}");
            w.l(2, "}");
        } else {
            w.l(2, "var __i: Int = 0");
            w.l(2, "for __e in x {");
            w.l(3, &format!("(__buf + __i).initialize(to: {})", in_e));
            w.l(3, "__i += 1");
            w.l(2, "}");
        }
        w.l(2, &format!("let __r: {} = {}(__buf, __n)", raw, copy_fn));
        if let Some(cl) = in_cleanup {
            w.l(2, "for __i in 0..<__n {");
            w.l(3, &cl);
            w.l(2, "}");
        }
        w.l(2, "return __r");
        w.l(1, "}");
        w.l(0, "");
        // C (borrowed) -> native
        w.l(
            1,
            &format!(
                "static func out_{}(_ p: UnsafeMutablePointer<{}>) -> [{}] {{",
                name, raw, ex
            ),
        );
        w.l(2, &format!("let __v: {} = p.pointee", raw));
        w.l(2, "let __n: Int = __v.len");
        w.l(2, "guard __n > 0, let __base = __v.ptr else {");
        w.l(3, "return []");
        w.l(2, "}");
        if bytes {
            w.l(
                2,
                "return Array(UnsafeBufferPointer<UInt8>(start: __base, count: __n))",
            );
        } else {
            w.l(2, &format!("var __out: [{}] = []", ex));
            w.l(2, "__out.reserveCapacity(__n)");
            w.l(2, "for __i in 0..<__n {");
            w.l(3, &format!("let __q: UnsafeMutablePointer<{}> = {}", ec, q));
            w.l(3, &format!("__out.append({})", out_e));
            w.l(2, "}");
            w.l(2, "return __out");
        }
        w.l(1, "}");
        w.l(0, "");
        // C (owned) -> native
        w.l(
            1,
            &format!("static func take_{}(_ v: {}) -> [{}] {{", name, raw, ex),
        );
        w.l(2, &format!("var __v: {} = v", raw));
        w.l(2, &format!("let __r: [{}] = out_{}(&__v)", ex, name));
        if let Some(d) = m.delete_fn(name) {
            w.l(2, &format!("{}(&__v)", d));
        }
        w.l(2, "return __r");
        w.l(1, "}");
        w.l(0, "}");
        w.l(0, "");
    }

    /// A natively mapped Vec also gets the Rust container as a class.
    ///
    /// `[T]` is an independent Swift array: it answers `count` and `==` its
    /// own way and has no capacity, no borrowed C slice and no Rust `Debug`.
    /// This class owns one `Az{T}Vec` (`deinit` runs its `Drop`) and is how
    /// those reach Swift; `init(_:)` and `elements` convert to and from the
    /// array that crosses every other boundary. Everything else is the
    /// api.json members, emitted exactly as for any other class.
    fn emit_vec_class(&mut self, w: &mut W, c: &ClassInfo, t: &Ty) {
        let m = self.m;
        let Ty::Vec { name, elem } = t else { return };
        let Some(ex) = exact(elem) else { return };
        let sname = swift_type_name(name);
        let raw = format!("Az{}", name);
        self.stats.classes += 1;
        let mut conformances = vec![format!("AzulValue<{}>", raw)];
        conformances.extend(self.trait_conformances(name, Recv::Class));
        w.doc(0, &self.rw(&c.doc));
        w.l(
            0,
            &format!(
                "/// The Rust vector itself. `[{}]` is what crosses the API; this is",
                ex
            ),
        );
        w.l(
            0,
            "/// the C container behind it, for the capacity Rust keeps, its own",
        );
        w.l(
            0,
            "/// `Debug` / `PartialEq` / `Ord` / `Hash`, the bounds-checked reads and",
        );
        w.l(0, "/// the borrowed C slices.");
        w.l(
            0,
            &format!(
                "public final class {}{} {{",
                sname,
                conformance_clause(&conformances)
            ),
        );
        if let Some(f) = m.delete_fn(name) {
            w.l(
                1,
                &format!(
                    "override internal class func _drop(_ p: UnsafeMutablePointer<{}>) {{ {}(p) }}",
                    raw, f
                ),
            );
        }
        if m.is_copy(name) {
            w.l(
                1,
                "override internal class var _isCopy: Bool { return true }",
            );
        } else if let Some(f) = m.clone_fn(name) {
            w.l(
                1,
                &format!(
                    "override internal class func _copyRaw(_ p: UnsafeMutablePointer<{}>) -> {}? \
                     {{ return {}(p) }}",
                    raw, raw, f
                ),
            );
        }
        w.l(0, "");
        let mut taken = Taken::default();
        w.l(1, "/// A new Rust vector holding a copy of every element.");
        w.l(
            1,
            &format!("public convenience init(_ elements: [{}]) {{", ex),
        );
        w.l(2, &format!("self.init(_own: _Conv.in_{}(elements))", name));
        w.l(1, "}");
        w.l(0, "");
        taken.take(
            Selector {
                is_static: true,
                base: "init".to_string(),
                labels: vec!["_".to_string()],
            },
            Some(format!("[{}]", ex)),
        );
        w.l(
            1,
            "/// The elements as a Swift array; each one is copied out.",
        );
        w.l(1, &format!("public var elements: [{}] {{", ex));
        w.l(2, &format!("return _Conv.out_{}(_address)", name));
        w.l(1, "}");
        w.l(0, "");
        taken.take(
            Selector {
                is_static: false,
                base: "elements".to_string(),
                labels: vec![],
            },
            None,
        );
        self.emit_traits(w, name, Recv::Class, &mut taken);
        self.emit_methods(w, name, Recv::Class, &mut taken);
        // No field accessors: a Vec's fields are its raw buffer, its capacity
        // and its destructor - writing any of them from Swift corrupts it.
        w.l(0, "}");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // The string type
    // ------------------------------------------------------------------------

    /// What api.json declares on the string, on Swift's own `String`.
    ///
    /// The string is the one api.json type with no declaration of its own -
    /// it IS `Swift.String` - so its constructors, its one method and its
    /// derives would otherwise be reachable only through `CAzul`. Building a
    /// string from UTF-16 or from a `char*` has no other route in the
    /// binding, and the derives answer the way Rust's string does rather
    /// than the way Swift's does. Every member is `azul`-prefixed: one added
    /// to a standard-library type must not shadow one it already has.
    fn emit_string_ext(&mut self, w: &mut W, c: &ClassInfo) {
        let m = self.m;
        let name = &c.name;
        let raw = format!("Az{}", name);
        let Some(ex) = exact(&Ty::Str) else { return };
        let del = m.delete_fn(name);
        // A Swift string as a fresh C one, and the free that balances it.
        let fresh = |local: &str, from: &str, mutable: bool| {
            format!(
                "{} {}: {} = _Native.azString({})",
                if mutable { "var" } else { "let" },
                local,
                raw,
                from
            )
        };
        let release = |local: &str| del.as_ref().map(|d| format!("{}(&{})", d, local));
        let mut body = W::default();
        let mut used: BTreeSet<String> = BTreeSet::new();

        for f in m.functions_of(name) {
            if !matches!(
                f.kind,
                FunctionKind::Constructor
                    | FunctionKind::StaticMethod
                    | FunctionKind::Method
                    | FunctionKind::MethodMut
            ) {
                continue;
            }
            let inst = matches!(f.kind, FunctionKind::Method | FunctionKind::MethodMut)
                && !f.args.is_empty();
            let mut params: Vec<String> = Vec::new();
            let mut pre: Vec<String> = Vec::new();
            let mut call: Vec<String> = Vec::new();
            let mut post: Vec<String> = Vec::new();
            let mut ok = true;
            for (i, a) in f.args.iter().skip(usize::from(inst)).enumerate() {
                let pname = {
                    let n = camel(&a.name);
                    if n.is_empty() {
                        format!("arg{}", i)
                    } else {
                        n
                    }
                };
                let en = escape(&pname);
                // The binding's convention: the first argument has no label.
                let label = if i == 0 { "_ " } else { "" };
                match a.ref_kind {
                    ArgRefKind::Owned => {
                        let t = m.not_result(m.owned(&a.type_name));
                        match (restriction(&t), c_type(&t), m.in_expr(&t, &en)) {
                            (Some(r), Some(ct), Some(e)) => {
                                params.push(format!("{}{}: {}", label, en, r));
                                pre.push(format!("let __a{}: {} = {}", i, ct, e));
                                call.push(format!("__a{}", i));
                            }
                            _ => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                        match pointee_swift(&a.type_name, a.ref_kind == ArgRefKind::PtrMut, m) {
                            Some(p) => {
                                params.push(format!("{}{}: {}", label, en, p));
                                call.push(en);
                            }
                            None => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            // The receiver is a fresh C string: a call that takes it by value
            // owns it, one that borrows it leaves it here to be freed.
            if inst && ok {
                match f.args[0].ref_kind {
                    ArgRefKind::Owned => {
                        pre.insert(0, fresh("__self", "self", false));
                        call.insert(0, "__self".to_string());
                    }
                    ArgRefKind::Ref
                    | ArgRefKind::RefMut
                    | ArgRefKind::Ptr
                    | ArgRefKind::PtrMut => match release("__self") {
                        Some(r) => {
                            pre.insert(0, fresh("__self", "self", true));
                            call.insert(0, "&__self".to_string());
                            post.push(r);
                        }
                        None => ok = false,
                    },
                }
            }
            let ret = m.ret_ty(f.return_type.as_deref());
            let take = match &ret {
                Ty::Void => Some(None),
                t => match (exact(t), c_type(t), m.take_expr(t, "__r")) {
                    (Some(a), Some(ct), Some(e)) => Some(Some((a, ct, e))),
                    _ => None,
                },
            };
            let member = format!("azul{}", upper_first(&camel(&f.method_name)));
            let (true, Some(take)) = (ok, take) else {
                self.skipped.push(f.c_name.clone());
                continue;
            };
            if !used.insert(member.clone()) {
                self.skipped.push(format!("{} {}", f.c_name, TAKEN));
                continue;
            }
            body.doc(1, &self.rw(&f.doc));
            let throws = if matches!(ret, Ty::Result { .. }) {
                " throws"
            } else {
                ""
            };
            body.l(
                1,
                &format!(
                    "public {}func {}({}){} -> {} {{",
                    if inst { "" } else { "static " },
                    member,
                    params.join(", "),
                    throws,
                    take.as_ref().map_or("Void", |(a, _, _)| a.as_str())
                ),
            );
            for l in &pre {
                body.l(2, l);
            }
            match &take {
                Some((_, ct, e)) => {
                    body.l(
                        2,
                        &format!("let __r: {} = {}({})", ct, f.c_name, call.join(", ")),
                    );
                    for l in &post {
                        body.l(2, l);
                    }
                    body.l(2, &format!("return {}", e));
                }
                None => {
                    body.l(2, &format!("{}({})", f.c_name, call.join(", ")));
                    for l in &post {
                        body.l(2, l);
                    }
                }
            }
            body.l(1, "}");
            body.l(0, "");
        }

        // ---- the derives, on the Rust value ---------------------------------
        let t = |k| m.trait_fn(name, k);
        let free = |b: &mut W, locals: &[&str]| {
            for l in locals {
                if let Some(r) = release(l) {
                    b.l(2, &r);
                }
            }
        };
        if let Some(f) = t(FunctionKind::DebugToString) {
            if used.insert("azulDescription".to_string()) {
                body.l(1, "/// Rust `Debug` for the string libazul holds.");
                body.l(1, &format!("public var azulDescription: {} {{", ex));
                body.l(2, &fresh("__s", "self", true));
                body.l(2, &format!("let __r: {} = {}(&__s)", raw, f));
                free(&mut body, &["__s"]);
                body.l(2, "return _Native.takeString(__r)");
                body.l(1, "}");
                body.l(0, "");
            }
        }
        if let Some(f) = t(FunctionKind::PartialEq) {
            if used.insert("azulEquals".to_string()) {
                body.l(1, "/// Rust `PartialEq` for the string libazul holds.");
                body.l(1, &format!("public func azulEquals(_ other: {}) -> Bool {{", ex));
                body.l(2, &fresh("__l", "self", true));
                body.l(2, &fresh("__r", "other", true));
                body.l(2, &format!("let __eq: Bool = {}(&__l, &__r)", f));
                free(&mut body, &["__l", "__r"]);
                body.l(2, "return __eq");
                body.l(1, "}");
                body.l(0, "");
            }
        }
        // Rust's C ordering: 0 Less, 1 Equal, 2 Greater, 255 incomparable.
        for (kind, member, doc, partial) in [
            (
                FunctionKind::Cmp,
                "azulCompare",
                "/// Rust `Ord` for the string libazul holds: -1, 0 or 1.",
                false,
            ),
            (
                FunctionKind::PartialCmp,
                "azulPartialCompare",
                "/// Rust `PartialOrd`: -1, 0 or 1, nil when they do not compare.",
                true,
            ),
        ] {
            let Some(f) = t(kind) else { continue };
            if !used.insert(member.to_string()) {
                continue;
            }
            body.l(1, doc);
            body.l(
                1,
                &format!(
                    "public func {}(_ other: {}) -> Int{} {{",
                    member,
                    ex,
                    if partial { "?" } else { "" }
                ),
            );
            body.l(2, &fresh("__l", "self", true));
            body.l(2, &fresh("__r", "other", true));
            body.l(2, &format!("let __o: UInt8 = {}(&__l, &__r)", f));
            free(&mut body, &["__l", "__r"]);
            if partial {
                body.l(2, "if __o > 2 {");
                body.l(3, "return nil");
                body.l(2, "}");
            }
            body.l(2, "return Int(__o) - 1");
            body.l(1, "}");
            body.l(0, "");
        }
        if let Some(f) = t(FunctionKind::Hash) {
            if used.insert("azulHashValue".to_string()) {
                body.l(1, "/// Rust `Hash` for the string libazul holds.");
                body.l(1, "public var azulHashValue: UInt64 {");
                body.l(2, &fresh("__s", "self", true));
                body.l(2, &format!("let __h: UInt64 = {}(&__s)", f));
                free(&mut body, &["__s"]);
                body.l(2, "return __h");
                body.l(1, "}");
                body.l(0, "");
            }
        }
        if let Some(f) = t(FunctionKind::DeepCopy) {
            if used.insert("azulCopy".to_string()) {
                body.l(1, "/// Rust `Clone` for the string libazul holds: a round trip");
                body.l(1, "/// through its allocator, which a Swift copy does not make.");
                body.l(1, &format!("public func azulCopy() -> {} {{", ex));
                body.l(2, &fresh("__s", "self", true));
                body.l(2, &format!("let __c: {} = {}(&__s)", raw, f));
                free(&mut body, &["__s"]);
                body.l(2, "return _Native.takeString(__c)");
                body.l(1, "}");
                body.l(0, "");
            }
        }
        if let Some(f) = t(FunctionKind::Default) {
            if used.insert("azulDefault".to_string()) {
                body.l(1, "/// Rust `Default` for the string libazul holds.");
                body.l(1, &format!("public static func azulDefault() -> {} {{", ex));
                body.l(2, &format!("return _Native.takeString({}())", f));
                body.l(1, "}");
                body.l(0, "");
            }
        }

        if body.out.trim().is_empty() {
            return;
        }
        w.l(0, &format!("extension Swift.{} {{", ex));
        w.out.push_str(&body.out);
        w.l(0, "}");
        w.l(0, "");
    }

    // ------------------------------------------------------------------------
    // Constants
    // ------------------------------------------------------------------------

    /// api.json's `constants` (the OpenGL enum values), as `static let`
    /// members of the class that declares them: `GlContextPtr.<NAME>`. A
    /// class with a Swift declaration gets an extension; one that is only a
    /// namespace here (nothing declares it) gets a caseless enum.
    fn emit_constants(&mut self, modules: &mut BTreeMap<String, W>) {
        let m = self.m;
        let mut by_class: BTreeMap<&str, Vec<&ConstantDef>> = BTreeMap::new();
        for k in &m.ir.constants {
            let Some((class, _)) = k.name.split_once('_') else {
                continue;
            };
            if !m.config.should_include_type(class) {
                continue;
            }
            by_class.entry(class).or_default().push(k);
        }
        for (class, consts) in by_class {
            let module = m
                .classes
                .get(class)
                .map(|c| c.module.clone())
                .or_else(|| m.enums.get(class).map(|e| e.module.clone()))
                .unwrap_or_else(|| consts[0].module.clone());
            let w = modules.entry(module_key(&module)).or_default();
            let sname = swift_type_name(class);
            // An extension when something declares the type under that name
            // (a class, an enum, a Vec's container class, the string's
            // typealias); a caseless enum when nothing does, so the
            // constants still have a home.
            let declared = m.enums.contains_key(class)
                || m.classes.contains_key(class)
                    && !matches!(m.owned(class), Ty::Option { .. } | Ty::Result { .. });
            w.l(0, "/// The constants api.json declares on this type.");
            if declared {
                w.l(0, &format!("extension {} {{", sname));
            } else {
                w.l(0, &format!("public enum {} {{", sname));
            }
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for k in consts {
                let bare = k.member_name();
                if !seen.insert(bare.clone()) {
                    continue;
                }
                for d in &k.doc {
                    w.l(1, &format!("/// {}", d.trim_end()));
                }
                // The declared type when it resolves to a Swift scalar; a
                // plain `let` otherwise, so an unmapped type still compiles.
                match Prim::from_rust(m.unalias(&k.type_name)) {
                    Some(p) => w.l(
                        1,
                        &format!(
                            "public static let {}: {} = {}",
                            escape(&bare),
                            p.swift(),
                            k.value.trim()
                        ),
                    ),
                    None => w.l(
                        1,
                        &format!("public static let {} = {}", escape(&bare), k.value.trim()),
                    ),
                }
            }
            w.l(0, "}");
            w.l(0, "");
        }
    }

    fn emit_result_conv(&mut self, w: &mut W, c: &ClassInfo, t: &Ty) {
        let m = self.m;
        let (Ty::Result { name, ok, err }, Shape::Union { tag_u8, variants }) = (t, &c.shape)
        else {
            return;
        };
        let okv = variants.iter().find(|v| v.name == "Ok").unwrap();
        let errv = variants.iter().find(|v| v.name == "Err").unwrap();
        let raw = format!("Az{}", name);
        let (Some(okx), Some(errx), Some(take_ok), Some(take_err)) = (
            exact(ok),
            exact(err),
            m.take_expr(ok, &format!("v.Ok.{}", escape(&okv.fields[0].c_name))),
            m.take_expr(err, &format!("v.Err.{}", escape(&errv.fields[0].c_name))),
        ) else {
            return;
        };
        w.l(0, "extension _Conv {");
        w.l(
            1,
            &format!(
                "static func take_{}(_ v: {}) throws -> {} {{",
                name, raw, okx
            ),
        );
        w.l(
            2,
            &format!("if {} == {} {{", tag_read(okv, "v", *tag_u8), okv.index),
        );
        w.l(3, &format!("return {}", take_ok));
        w.l(2, "}");
        w.l(2, &format!("throw AzulError<{}>({})", errx, take_err));
        w.l(1, "}");
        w.l(0, "}");
        w.l(0, "");
    }
}

/// Suffix of a skipped function whose Swift name and signature another member
/// (usually the enum case it constructs) already has.
pub const TAKEN: &str = "(same name and signature as another member)";

#[derive(Clone)]
struct CallbackArg<'a> {
    td: &'a CallbackTypedefDef,
    wrapper: Option<(String, String, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MethodKind {
    Init,
    Static,
    Instance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Role {
    Func,
    Getter(String),
    /// Property name and the value's Swift type.
    Setter(String, String),
}

struct Plan {
    c_name: String,
    doc: Vec<String>,
    kind: MethodKind,
    name: String,
    raw_name: String,
    params: Vec<Param>,
    ret_type: String,
    throws: bool,
    generic: bool,
    mutating: bool,
    discardable: bool,
    body: Vec<String>,
    role: Role,
    trampolines: Vec<String>,
    convenience: bool,
}

impl Plan {
    fn selector(&self) -> Selector {
        match &self.role {
            Role::Getter(n) | Role::Setter(n, _) => Selector {
                is_static: false,
                base: n.clone(),
                labels: vec![],
            },
            Role::Func => Selector {
                is_static: self.kind != MethodKind::Instance,
                base: self.name.clone(),
                labels: self.params.iter().map(|p| p.label.clone()).collect(),
            },
        }
    }

    fn sig(&self) -> Option<String> {
        match self.role {
            Role::Func => Some(
                self.params
                    .iter()
                    .map(|p| p.ty.clone())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
            _ => None,
        }
    }

    /// Falls back to the api.json name (`createFoo`, `getX`) as a method.
    fn fallback_name(&mut self) {
        self.role = Role::Func;
        if self.kind == MethodKind::Init {
            self.kind = MethodKind::Static;
            // `init` bodies end in `self.init(...)`: rewrite into a factory.
            let mut body = Vec::new();
            for line in &self.body {
                if let Some(rest) = line.strip_prefix("self.init(") {
                    body.push(format!("let __o = Self({}", rest));
                } else {
                    body.push(line.replace("_ptr.pointer(", "__o._ptr.pointer("));
                }
            }
            body.push("return __o".to_string());
            self.body = body;
            self.ret_type = "Self".to_string();
        }
        self.name = self.raw_name.clone();
    }

    fn header(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.discardable {
            out.push("@discardableResult".to_string());
        }
        let generic = if self.generic { "<T: AnyObject>" } else { "" };
        let throws = if self.throws { " throws" } else { "" };
        let params = param_list(&self.params);
        match self.kind {
            MethodKind::Init => {
                let conv = if self.convenience { "convenience " } else { "" };
                out.push(format!(
                    "public {}init{}({}){} {{",
                    conv, generic, params, throws
                ));
            }
            MethodKind::Static => {
                let ret = if self.ret_type == "Self" {
                    // A class factory returns the final class itself.
                    " -> Self".to_string()
                } else if self.ret_type == "Void" {
                    String::new()
                } else {
                    format!(" -> {}", self.ret_type)
                };
                out.push(format!(
                    "public static func {}{}({}){}{} {{",
                    escape(&self.name),
                    generic,
                    params,
                    throws,
                    ret
                ));
            }
            MethodKind::Instance => {
                let ret = if self.ret_type == "Void" {
                    String::new()
                } else {
                    format!(" -> {}", self.ret_type)
                };
                out.push(format!(
                    "public {}func {}{}({}){}{} {{",
                    if self.mutating { "mutating " } else { "" },
                    escape(&self.name),
                    generic,
                    params,
                    throws,
                    ret
                ));
            }
        }
        out
    }
}

/// Swift case names for C variant names, unique within the enum.
fn case_names(variants: &[String]) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    variants
        .iter()
        .map(|v| {
            let mut n = camel(v);
            if n.is_empty() || n.starts_with(|c: char| c.is_ascii_digit()) {
                n = format!("_{}", n);
            }
            if reserved_member(&n) && !n.starts_with('_') {
                n.push('_');
            }
            while !seen.insert(n.clone()) {
                n.push('_');
            }
            n
        })
        .collect()
}

/// Swift member name for an api.json field name.
/// `fromUtf16Le` -> `FromUtf16Le`, for a name built by prefixing another.
fn upper_first(s: &str) -> String {
    let mut ch = s.chars();
    match ch.next() {
        Some(f) => f.to_ascii_uppercase().to_string() + ch.as_str(),
        None => String::new(),
    }
}

fn member_name(field: &str) -> String {
    let mut n = camel(field);
    if n.is_empty() || n.starts_with(|c: char| c.is_ascii_digit()) {
        n = format!("field{}", n);
    }
    if reserved_member(&n) {
        n.push('_');
    }
    n
}

fn conformance_clause(list: &[String]) -> String {
    if list.is_empty() {
        String::new()
    } else {
        format!(": {}", list.join(", "))
    }
}

/// Reads a union's tag as an `Int` through variant `v`'s member.
fn tag_read(v: &Variant, value: &str, tag_u8: bool) -> String {
    if tag_u8 {
        format!("Int({}.{}.tag)", value, escape(&v.name))
    } else {
        format!("Int({}.{}.tag.rawValue)", value, escape(&v.name))
    }
}

/// The value to store in variant `v`'s tag.
fn tag_value(c: &ClassInfo, v: &Variant, tag_u8: bool) -> String {
    if tag_u8 {
        format!("{}", v.index)
    } else {
        format!("Az{}_Tag(rawValue: {})", c.name, v.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_names_are_lower_camel_and_unique() {
        let v: Vec<String> = ["DoNothing", "RefreshDom", "URL", "Em", "EM", "Default"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            case_names(&v),
            vec!["doNothing", "refreshDom", "url", "em", "em_", "default_"]
        );
    }

    #[test]
    fn native_types_at_the_boundary() {
        let opt = Ty::Option {
            name: "OptionString".into(),
            payload: Box::new(Ty::Str),
        };
        assert_eq!(exact(&opt).as_deref(), Some("String?"));
        let bytes = Ty::Vec {
            name: "U8Vec".into(),
            elem: Box::new(Ty::Prim(Prim::U8)),
        };
        assert_eq!(exact(&bytes).as_deref(), Some("[UInt8]"));
        let any = Ty::Option {
            name: "OptionRefAny".into(),
            payload: Box::new(Ty::RefAny),
        };
        assert_eq!(restriction(&any).as_deref(), Some("AnyObject?"));
        assert_eq!(exact(&any).as_deref(), Some("RefAny?"));
        let res = Ty::Result {
            name: "ResultXmlXmlError".into(),
            ok: Box::new(Ty::Class("Xml".into())),
            err: Box::new(Ty::Class("XmlError".into())),
        };
        assert_eq!(exact(&res).as_deref(), Some("Xml"));
        assert_eq!(restriction(&res), None);
    }

    #[test]
    fn protocol_requirements_are_never_generated_as_members() {
        for n in ["description", "hash", "copy", "init", "_take", "default"] {
            assert!(reserved_member(n), "{n} must be reserved");
        }
        assert!(!reserved_member("withCss"));
    }
}
