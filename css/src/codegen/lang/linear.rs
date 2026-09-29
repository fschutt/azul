//! Statement-oriented printers: languages whose bindings cannot nest the
//! construction of a struct, union or Vec inside an expression (Pascal
//! records are only literal in typed constants, COBOL cannot nest calls at
//! all, Fortran / Ada / BASIC need a declared array for a Vec, ...).
//!
//! [`lower_to_statements`] flattens an IR value into typed temporaries in
//! evaluation order (children first): every constructor call, variant,
//! struct, string and Vec gets one; C-like enum constants and numbers stay
//! inline. A [`LinearSyntax`] spells declarations and statements.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use super::blocker_with;
use crate::codegen::{
    ir::{is_droppable_vec, EnumShape, Expr, Item, Prim},
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
    /// Statements that make `target` the Vec copied from array `arr`.
    fn vec_from_array(&self, target: &str, ty: &str, elem: &str, arr: &str, n: usize) -> Vec<String>;
    /// Statements that make `target` an empty Vec.
    fn vec_empty(&self, target: &str, ty: &str) -> Vec<String>;
    /// Statements that make `target` an `AzString` holding `s`.
    fn string(&self, target: &str, s: &str) -> Vec<String>;
    /// Why the bindings cannot build the node `e` (see
    /// [`ExprSyntax::limitation`](super::ExprSyntax::limitation)).
    fn limitation(&self, _e: &Expr) -> Option<String> {
        None
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

    fn emit(&mut self, e: &Expr) -> String {
        let s = self.s;
        match e {
            Expr::Int { value, ty } => s.int(*value, *ty),
            Expr::Float { text, ty } => s.float(text, *ty),
            Expr::Bool(b) => s.boolean(*b),
            Expr::Str(text) => {
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
                let args: Vec<String> = args.iter().map(|a| self.emit(a)).collect();
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
                    let args: Vec<String> = args.iter().map(|a| self.emit(a)).collect();
                    let t = self.temp(s.type_name(ty));
                    let st = s.variant_call(&t, ty, variant, &args);
                    self.out.body.extend(st);
                    t
                }
                EnumShape::TaggedShadowed | EnumShape::Generic { .. } => {
                    let payload = args.first().map(|a| self.emit(a));
                    let t = self.temp(s.type_name(ty));
                    let tag = union_tag(ty, variant).unwrap_or(0);
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
                let t = self.temp(s.type_name(ty));
                let st = s.vec_from_array(&t, ty, elem, &arr, values.len());
                self.out.body.extend(st);
                t
            }
            Expr::Unsupported { what } => {
                self.out.dropped.push(what.clone());
                String::new()
            }
        }
    }
}

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
    blocker_with(&|n| s.limitation(n), &item.value)
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
