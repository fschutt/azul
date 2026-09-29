//! Statement-oriented printers: languages whose bindings cannot nest the
//! construction of a struct, union or Vec inside an expression (Pascal
//! records are only literal in typed constants, COBOL cannot nest calls at
//! all, Fortran / Ada / BASIC need a declared array for a Vec, ...).
//!
//! [`lower_to_statements`] flattens an IR value into typed temporaries in
//! evaluation order (children first): every constructor call, variant,
//! struct, string and Vec gets one; C-like enum constants and numbers stay
//! inline. A [`LinearSyntax`] spells declarations and statements.
//!
//! A language that CAN nest some constructions (Ada, Fortran and BASIC nest
//! calls and aggregates, only a Vec needs a declared array) returns them
//! from the `*_expr` methods; those nodes then stay inline and only the
//! rest gets a temporary.
//!
//! DOM construction: a builder method (`Dom.with_child`) is always a
//! statement, and a chain updates ONE temporary (`t1 = AzDom_withChild(t1,
//! child)`), so a tree reads top to bottom; a child that is a chain gets a
//! temporary of its own first. A printer opts in by returning `None` from
//! [`LinearSyntax::dom_limitation`] and spelling [`LinearSyntax::param`] and
//! [`LinearSyntax::concat`] (or their `*_expr` forms).

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use super::{blocker_with, concat_parts, dom::is_dom_item, string_parts, ConcatPart};
use crate::codegen::{
    ir::{is_droppable_vec, EnumShape, Expr, Ident, Item, Prim},
    lower_types::union_tag,
};

/// How one statement-oriented language spells the flattened construction.
pub trait LinearSyntax {
    fn int(&self, value: i128, ty: Prim) -> String;
    fn float(&self, text: &str, ty: Prim) -> String;
    fn boolean(&self, b: bool) -> String;
    /// A C-like enum constant (always inline).
    fn clike(&self, ty: &str, variant: &str) -> String;
    /// The declared type of a temporary holding an api.json type.
    fn type_name(&self, ty: &str) -> String;
    /// The declared type of an array temporary of `n` elements.
    fn array_type(&self, elem: &str, n: usize) -> String;
    /// Statements that make temporary `target` the value of
    /// `class::method(args)`.
    fn call(&self, target: &str, class: &str, method: &str, args: &[String]) -> Vec<String>;
    /// Statements that make `target` the tagged-union variant (via its
    /// variant constructor).
    fn variant_call(&self, target: &str, ty: &str, variant: &str, args: &[String]) -> Vec<String>;
    /// Statements that make `target` the union variant built by hand (a
    /// `CssPropertyValue<T>` alias, or a shadowed `CssProperty` variant).
    fn union(
        &self,
        target: &str,
        ty: &str,
        variant: &str,
        tag: usize,
        payload: Option<&str>,
    ) -> Vec<String>;
    /// Statement setting field `field` of struct temporary `target`.
    fn set_field(&self, target: &str, ty: &str, field: &str, value: &str) -> String;
    /// Statement storing `value` into element `index` (0-based) of array `arr`.
    fn set_elem(&self, arr: &str, index: usize, value: &str) -> String;
    /// Statements that make `target` the Vec copied from array `arr`;
    /// `count` is the item count as a `usize` operand.
    fn vec_from_array(&self, target: &str, ty: &str, elem: &str, arr: &str, count: &str) -> Vec<String>;
    /// Statements that make `target` an empty Vec.
    fn vec_empty(&self, target: &str, ty: &str) -> Vec<String>;
    /// Statements that make `target` an `AzString` holding `s`.
    fn string(&self, target: &str, s: &str) -> Vec<String>;
    /// Why the bindings cannot build the node `e` (see
    /// [`ExprSyntax::limitation`](super::ExprSyntax::limitation)).
    fn limitation(&self, _e: &Expr) -> Option<String> {
        None
    }

    // Inline forms. `None` (the default) means "use a temporary".

    /// `class::method(args)` as an expression.
    fn call_expr(&self, _class: &str, _method: &str, _args: &[String]) -> Option<String> {
        None
    }
    /// A tagged-union variant (via its variant constructor) as an expression.
    fn variant_expr(&self, _ty: &str, _variant: &str, _args: &[String]) -> Option<String> {
        None
    }
    /// A hand-built union variant as an expression.
    fn union_expr(
        &self,
        _ty: &str,
        _variant: &str,
        _tag: usize,
        _payload: Option<&str>,
    ) -> Option<String> {
        None
    }
    /// A struct as an expression (fields in declaration order).
    fn struct_expr(&self, _ty: &str, _fields: &[(String, String)]) -> Option<String> {
        None
    }
    /// An `AzString` holding `s` as an expression.
    fn string_expr(&self, _s: &str) -> Option<String> {
        None
    }
    /// The Vec copied from array `arr` (`count` items) as an expression.
    fn vec_expr(&self, _ty: &str, _elem: &str, _arr: &str, _count: &str) -> Option<String> {
        None
    }
    /// An empty Vec as an expression.
    fn vec_empty_expr(&self, _ty: &str) -> Option<String> {
        None
    }

    // Data items (COBOL: a CALL argument is a data item, not an expression).

    /// The declared type, with its initializer, of a buffer temporary that
    /// holds the bytes of `s` before [`Self::string_from_buffer`] builds the
    /// `AzString`. `None`: [`Self::string`] builds it directly.
    fn string_buffer(&self, _s: &str) -> Option<String> {
        None
    }
    /// Statements that make `target` the `AzString` of buffer `buf`.
    fn string_from_buffer(&self, target: &str, _buf: &str, s: &str) -> Vec<String> {
        self.string(target, s)
    }
    /// The declared type, with its initializer, of a temporary holding the
    /// scalar CALL argument `e`. `None`: the literal is passed inline.
    fn scalar_arg(&self, _e: &Expr) -> Option<String> {
        None
    }
    /// The CALL operand for a [`Self::scalar_arg`] temporary.
    fn scalar_operand(&self, temp: &str) -> String {
        temp.to_string()
    }

    // DOM construction (`Expr::Method` / `Param` / `Concat`, item parameters).

    /// Why this language does not print the DOM node kinds and item
    /// parameters, or `None` once its printer spells [`Self::param`] and
    /// [`Self::concat`] (or their `*_expr` forms) and prints `Item::params`.
    fn dom_limitation(&self) -> Option<&'static str> {
        Some(LINEAR_DOM)
    }

    /// Statements that make `target` the value of the by-value `self` method
    /// `class.method` on `recv` (`target` may be `recv` itself: a chain
    /// updates one temporary). Default: the C-ABI call, receiver first
    /// (`AzDom_withChild(recv, child)`), through [`Self::call`].
    fn method(&self, target: &str, class: &str, method: &str, recv: &str, args: &[String]) -> Vec<String> {
        let mut all = Vec::with_capacity(args.len() + 1);
        all.push(recv.to_string());
        all.extend_from_slice(args);
        self.call(target, class, method, &all)
    }

    /// Item parameter `name` (a native string of the language) as the api
    /// `String` a call takes, as an expression. `None`: [`Self::param`].
    fn param_expr(&self, _name: &Ident) -> Option<String> {
        None
    }

    /// Statements that make `target` the api `String` of parameter `name`.
    fn param(&self, _target: &str, _name: &Ident) -> Vec<String> {
        Vec::new()
    }

    /// The api `String` joined from `parts`, as an expression. `None`:
    /// [`Self::concat`].
    fn concat_expr(&self, _parts: &[ConcatPart<'_>]) -> Option<String> {
        None
    }

    /// Statements that make `target` the api `String` joined from `parts`.
    fn concat(&self, _target: &str, _parts: &[ConcatPart<'_>]) -> Vec<String> {
        Vec::new()
    }

    // Component boundaries (`Expr::ItemCall`).

    /// Why this language does not print a call of another item of the
    /// module (a component's render function), or `None` once its printer
    /// spells [`Self::item_call`] (or [`Self::item_call_expr`]) and
    /// [`Self::native_string_arg`]. Default: not implemented.
    fn item_call_limitation(&self) -> Option<&'static str> {
        Some(super::ITEM_CALL_LIMITATION)
    }

    /// A `String` argument of an item call, from `parts`, as the language's
    /// native string (what the callee's parameter takes), as an expression.
    fn native_string_arg(&self, _parts: &[ConcatPart<'_>]) -> String {
        String::new()
    }

    /// The call `item(args)` as an expression. `None`: [`Self::item_call`].
    fn item_call_expr(&self, _item: &Ident, _args: &[String]) -> Option<String> {
        None
    }

    /// Statements that make `target` the `Dom` item `item` returns for
    /// `args`.
    fn item_call(&self, _target: &str, _item: &Ident, _args: &[String]) -> Vec<String> {
        Vec::new()
    }
}

/// A flattened value: temporaries (name, declared type), statements, and
/// the operand holding the result.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Statements {
    pub decls: Vec<(String, String)>,
    pub body: Vec<String>,
    pub result: String,
    /// Items dropped from droppable lists, with the reason.
    pub dropped: Vec<String>,
}

struct Ctx<'a> {
    s: &'a dyn LinearSyntax,
    out: Statements,
    next: usize,
}

impl Ctx<'_> {
    fn temp(&mut self, ty: String) -> String {
        self.next += 1;
        let name = format!("t{}", self.next);
        self.out.decls.push((name.clone(), ty));
        name
    }

    /// `true` if `operand` is one of this value's temporaries, declared
    /// with type `ty`.
    fn is_temp_of(&self, operand: &str, ty: &str) -> bool {
        self.out.decls.iter().any(|(n, t)| n == operand && t == ty)
    }

    /// A CALL argument: a scalar the language cannot pass inline gets a
    /// temporary of its own.
    fn arg(&mut self, e: &Expr) -> String {
        if matches!(e, Expr::Int { .. } | Expr::Float { .. } | Expr::Bool(_)) {
            if let Some(decl) = self.s.scalar_arg(e) {
                let t = self.temp(decl);
                return self.s.scalar_operand(&t);
            }
        }
        self.emit(e)
    }

    fn emit(&mut self, e: &Expr) -> String {
        let s = self.s;
        match e {
            Expr::Int { value, ty } => s.int(*value, *ty),
            Expr::Float { text, ty } => s.float(text, *ty),
            Expr::Bool(b) => s.boolean(*b),
            Expr::Str(text) => {
                if let Some(x) = s.string_expr(text) {
                    return x;
                }
                if let Some(decl) = s.string_buffer(text) {
                    let buf = self.temp(decl);
                    let t = self.temp(s.type_name("String"));
                    let st = s.string_from_buffer(&t, &buf, text);
                    self.out.body.extend(st);
                    return t;
                }
                let t = self.temp(s.type_name("String"));
                let st = s.string(&t, text);
                self.out.body.extend(st);
                t
            }
            Expr::Call {
                class,
                method,
                args,
            } => {
                let args: Vec<String> = args.iter().map(|a| self.arg(a)).collect();
                if let Some(x) = s.call_expr(class, method, &args) {
                    return x;
                }
                let t = self.temp(s.type_name(class));
                let st = s.call(&t, class, method, &args);
                self.out.body.extend(st);
                t
            }
            Expr::Variant {
                ty,
                shape,
                variant,
                args,
            } => match shape {
                EnumShape::CLike => s.clike(ty, variant),
                EnumShape::Tagged => {
                    let args: Vec<String> = args.iter().map(|a| self.arg(a)).collect();
                    if let Some(x) = s.variant_expr(ty, variant, &args) {
                        return x;
                    }
                    let t = self.temp(s.type_name(ty));
                    let st = s.variant_call(&t, ty, variant, &args);
                    self.out.body.extend(st);
                    t
                }
                EnumShape::TaggedShadowed | EnumShape::Generic { .. } => {
                    let payload = args.first().map(|a| self.emit(a));
                    let tag = union_tag(ty, variant).unwrap_or(0);
                    if let Some(x) = s.union_expr(ty, variant, tag, payload.as_deref()) {
                        return x;
                    }
                    let t = self.temp(s.type_name(ty));
                    let st = s.union(&t, ty, variant, tag, payload.as_deref());
                    self.out.body.extend(st);
                    t
                }
            },
            Expr::Struct { ty, fields } => {
                let values: Vec<(String, String)> = fields
                    .iter()
                    .map(|(k, v)| (k.clone(), self.emit(v)))
                    .collect();
                if let Some(x) = s.struct_expr(ty, &values) {
                    return x;
                }
                let t = self.temp(s.type_name(ty));
                for (k, v) in values {
                    let st = s.set_field(&t, ty, &k, &v);
                    self.out.body.push(st);
                }
                t
            }
            Expr::Vec { ty, elem, items } => {
                let lim = |n: &Expr| s.limitation(n);
                let mut kept: Vec<&Expr> = Vec::new();
                for i in items {
                    match (is_droppable_vec(ty), blocker_with(&lim, i)) {
                        (true, Some(reason)) => self.out.dropped.push(reason),
                        _ => kept.push(i),
                    }
                }
                if kept.is_empty() {
                    if let Some(x) = s.vec_empty_expr(ty) {
                        return x;
                    }
                    let t = self.temp(s.type_name(ty));
                    let st = s.vec_empty(&t, ty);
                    self.out.body.extend(st);
                    return t;
                }
                let values: Vec<String> = kept.iter().map(|i| self.emit(i)).collect();
                let arr = self.temp(s.array_type(elem, values.len()));
                for (i, v) in values.iter().enumerate() {
                    let st = s.set_elem(&arr, i, v);
                    self.out.body.push(st);
                }
                let count = self.arg(&Expr::int(values.len() as i128, Prim::Usize));
                if let Some(x) = s.vec_expr(ty, elem, &arr, &count) {
                    return x;
                }
                let t = self.temp(s.type_name(ty));
                let st = s.vec_from_array(&t, ty, elem, &arr, &count);
                self.out.body.extend(st);
                t
            }
            Expr::Unsupported { what } => {
                self.out.dropped.push(what.clone());
                String::new()
            }
            Expr::Method {
                recv,
                class,
                method,
                args,
            } => {
                let r = self.emit(recv);
                let args: Vec<String> = args.iter().map(|a| self.arg(a)).collect();
                // A chain updates the receiver's temporary in place.
                let ty = s.type_name(class);
                let target = if self.is_temp_of(&r, &ty) {
                    r.clone()
                } else {
                    self.temp(ty)
                };
                let st = s.method(&target, class, method, &r, &args);
                self.out.body.extend(st);
                target
            }
            Expr::Param(name) => {
                if let Some(x) = s.param_expr(name) {
                    return x;
                }
                let t = self.temp(s.type_name("String"));
                let st = s.param(&t, name);
                self.out.body.extend(st);
                t
            }
            Expr::Concat(parts) => {
                let parts = concat_parts(parts);
                if let Some(x) = s.concat_expr(&parts) {
                    return x;
                }
                let t = self.temp(s.type_name("String"));
                let st = s.concat(&t, &parts);
                self.out.body.extend(st);
                t
            }
            Expr::ItemCall { item, args } => {
                let args: Vec<String> = args
                    .iter()
                    .map(|a| s.native_string_arg(&string_parts(a)))
                    .collect();
                if let Some(x) = s.item_call_expr(item, &args) {
                    return x;
                }
                let t = self.temp(s.type_name("Dom"));
                let st = s.item_call(&t, item, &args);
                self.out.body.extend(st);
                t
            }
        }
    }
}

/// Why the statement-oriented printers do not print DOM construction.
pub const LINEAR_DOM: &str = "DOM export (builder methods and parameters) is not implemented for \
                              the statement-oriented printers yet";

/// Flatten `e` into temporaries `t1`, `t2`, ... (children first).
#[must_use]
pub fn lower_to_statements(s: &dyn LinearSyntax, e: &Expr) -> Statements {
    let mut cx = Ctx {
        s,
        out: Statements::default(),
        next: 0,
    };
    let result = cx.emit(e);
    cx.out.result = result;
    cx.out
}

/// Why this language cannot build the item at all (`None` if it can).
#[must_use]
pub fn item_blocker(s: &dyn LinearSyntax, item: &Item) -> Option<String> {
    if is_dom_item(item) {
        if let Some(reason) = s.dom_limitation() {
            return Some(reason.to_string());
        }
    }
    blocker_with(
        &|n| {
            s.limitation(n).or_else(|| {
                if !n.is_dom_node() {
                    return None;
                }
                s.dom_limitation()
                    .or_else(|| {
                        if matches!(n, Expr::ItemCall { .. }) {
                            s.item_call_limitation()
                        } else {
                            None
                        }
                    })
                    .map(ToString::to_string)
            })
        },
        &item.value,
    )
}

/// The item's comment lines: its doc plus the language's dropped items.
#[must_use]
pub fn linear_comments(item: &Item, st: &Statements) -> Vec<String> {
    let mut out = item.doc.clone();
    for r in &st.dropped {
        let line = format!("dropped a value these bindings cannot build: {r}");
        if !out.iter().any(|d| d.contains(r.as_str())) {
            out.push(line);
        }
    }
    out
}
