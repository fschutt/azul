//! One printer per binding language.
//!
//! Every printer implements [`ExprSyntax`] (how to spell the IR node kinds in
//! that language, following the naming rules of its bindings generator in
//! `doc/src/codegen/v2/lang_*`) plus the module / project layout in its
//! [`CodegenBackend`](super::CodegenBackend) impl. [`expr_doc`] walks the IR
//! once for all of them.

use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

pub use super::ir::is_droppable_vec;
use super::{
    doc::Doc,
    ir::{EnumShape, Expr, Module, Prim},
    CodegenBackend,
};

pub mod c;
pub mod rust;

/// How one language spells the IR node kinds.
pub trait ExprSyntax {
    /// An integer literal of type `ty`.
    fn int(&self, value: i128, ty: Prim) -> String;
    /// A float literal; `text` is canonical (`"10.0"`, `"-0.5"`, `"nan"`,
    /// `"inf"`, `"-inf"`).
    fn float(&self, text: &str, ty: Prim) -> String;
    fn boolean(&self, b: bool) -> String;
    /// An api.json `String` built from `s`.
    fn string(&self, s: &str) -> Doc;
    /// `class::method(args)` (api.json snake_case method name).
    fn call(&self, class: &str, method: &str, args: Vec<Doc>, broken: bool) -> Doc;
    /// An enum variant (`args` empty for a unit variant).
    fn variant(&self, ty: &str, shape: EnumShape, variant: &str, args: Vec<Doc>, broken: bool) -> Doc;
    /// A struct literal, fields in declaration order.
    fn strukt(&self, ty: &str, fields: Vec<(String, Doc)>, broken: bool) -> Doc;
    /// An `XxxVec` from its items (already filtered of unsupported ones).
    fn vec(&self, ty: &str, elem: &str, items: Vec<Doc>, broken: bool) -> Doc;
    /// A value the bindings cannot express (only reached outside a Vec).
    fn unsupported(&self, what: &str) -> Doc;

    /// Why THIS language's bindings cannot build the node `e` itself (not its
    /// children), e.g. "the Python bindings cannot build a Vec of 2+ items".
    /// The nearest droppable list (see [`is_droppable_vec`]) then drops the
    /// item containing it. Default: no limitation.
    fn limitation(&self, _e: &Expr) -> Option<String> {
        None
    }
}

/// Why `e` cannot be built in language `s`: the first [`Expr::Unsupported`]
/// or language [`ExprSyntax::limitation`] inside it, not looking into a
/// droppable list (that one drops its own bad items).
#[must_use]
pub fn blocker(s: &dyn ExprSyntax, e: &Expr) -> Option<String> {
    if let Expr::Unsupported { what } = e {
        return Some(what.clone());
    }
    if let Some(r) = s.limitation(e) {
        return Some(r);
    }
    match e {
        Expr::Vec { ty, items, .. } => {
            if is_droppable_vec(ty) {
                None
            } else {
                items.iter().find_map(|i| blocker(s, i))
            }
        }
        Expr::Call { args, .. } | Expr::Variant { args, .. } => args.iter().find_map(|a| blocker(s, a)),
        Expr::Struct { fields, .. } => fields.iter().find_map(|(_, v)| blocker(s, v)),
        Expr::Int { .. } | Expr::Float { .. } | Expr::Bool(_) | Expr::Str(_) => None,
    }
}

/// The items of a `Vec` node a printer emits: a droppable list drops the
/// items [`blocker`] rejects; any other list keeps everything (a blocked
/// value list blocks its parent instead).
#[must_use]
pub fn kept_items<'a>(s: &dyn ExprSyntax, ty: &str, items: &'a [Expr]) -> Vec<&'a Expr> {
    if is_droppable_vec(ty) {
        items.iter().filter(|i| blocker(s, i).is_none()).collect()
    } else {
        items.iter().collect()
    }
}

/// Why each item a LANGUAGE limitation dropped from a droppable list inside
/// `e` was dropped (the language-independent `Unsupported` drops are already
/// in the item's doc, from the lowering). Printers add these as comments.
#[must_use]
pub fn limitation_notes(s: &dyn ExprSyntax, e: &Expr) -> Vec<String> {
    let mut out = Vec::new();
    e.walk(&mut |n| {
        if let Expr::Vec { ty, items, .. } = n {
            if is_droppable_vec(ty) {
                out.extend(
                    items
                        .iter()
                        .filter(|i| !i.contains_unsupported())
                        .filter_map(|i| blocker(s, i))
                        .map(|r| format!("dropped a value these bindings cannot build: {r}")),
                );
            }
        }
    });
    out
}

/// The comment lines of an item: its doc plus [`limitation_notes`].
#[must_use]
pub fn item_comments(s: &dyn ExprSyntax, item: &super::ir::Item) -> Vec<String> {
    let mut out = item.doc.clone();
    out.extend(limitation_notes(s, &item.value));
    out
}

/// `true` if the node lays out one child per line: a `Vec` with two or more
/// kept items, or any node containing a tall child.
#[must_use]
pub fn is_tall(s: &dyn ExprSyntax, e: &Expr) -> bool {
    match e {
        Expr::Vec { ty, items, .. } => {
            let kept = kept_items(s, ty, items);
            kept.len() >= 2 || kept.iter().any(|i| is_tall(s, i))
        }
        Expr::Call { args, .. } | Expr::Variant { args, .. } => args.iter().any(|a| is_tall(s, a)),
        Expr::Struct { fields, .. } => fields.iter().any(|(_, v)| is_tall(s, v)),
        Expr::Int { .. }
        | Expr::Float { .. }
        | Expr::Bool(_)
        | Expr::Str(_)
        | Expr::Unsupported { .. } => false,
    }
}

/// Lay out `e` in language `s`.
#[must_use]
pub fn expr_doc(s: &dyn ExprSyntax, e: &Expr) -> Doc {
    expr_doc_inner(s, e, is_tall(s, e))
}

/// Like [`expr_doc`], but a non-empty top-level `Vec` (a style's property
/// list) is always one item per line.
#[must_use]
pub fn expr_doc_top(s: &dyn ExprSyntax, e: &Expr) -> Doc {
    let force = matches!(e, Expr::Vec { ty, items, .. } if !kept_items(s, ty, items).is_empty());
    expr_doc_inner(s, e, force || is_tall(s, e))
}

fn expr_doc_inner(s: &dyn ExprSyntax, e: &Expr, broken: bool) -> Doc {
    match e {
        Expr::Int { value, ty } => Doc::text(s.int(*value, *ty)),
        Expr::Float { text, ty } => Doc::text(s.float(text, *ty)),
        Expr::Bool(b) => Doc::text(s.boolean(*b)),
        Expr::Str(x) => s.string(x),
        Expr::Call {
            class,
            method,
            args,
        } => s.call(
            class,
            method,
            args.iter().map(|a| expr_doc(s, a)).collect(),
            broken,
        ),
        Expr::Variant {
            ty,
            shape,
            variant,
            args,
        } => s.variant(
            ty,
            *shape,
            variant,
            args.iter().map(|a| expr_doc(s, a)).collect(),
            broken,
        ),
        Expr::Struct { ty, fields } => s.strukt(
            ty,
            fields
                .iter()
                .map(|(k, v)| (k.clone(), expr_doc(s, v)))
                .collect(),
            broken,
        ),
        Expr::Vec { ty, elem, items } => s.vec(
            ty,
            elem,
            kept_items(s, ty, items)
                .into_iter()
                .map(|i| expr_doc(s, i))
                .collect(),
            broken,
        ),
        Expr::Unsupported { what } => s.unsupported(what),
    }
}

// ------------------------------------------------------------------ naming

/// The C-ABI variant-constructor method of `variant`: the variant with its
/// first character lowered (`MinContent` -> `minContent`), plus `Variant`
/// when that collides with a trait function every binding recognizes by name
/// (`Default` -> `defaultVariant`, `Clone` -> `cloneVariant`). Mirrors
/// `variant_constructor_method_name` in `doc/src/codegen/v2/ir_builder.rs`.
#[must_use]
pub fn variant_ctor_method(variant: &str) -> String {
    let m = super::ir::lower_first(variant);
    if RESERVED_VARIANT_CTORS.contains(&m.as_str()) {
        format!("{m}Variant")
    } else {
        m
    }
}

const RESERVED_VARIANT_CTORS: &[&str] = &[
    "default",
    "delete",
    "clone",
    "partialEq",
    "partialCmp",
    "cmp",
    "hash",
    "createDefault",
    "toDbgString",
];

/// Escape `s` as the body of a double-quoted C-family string literal:
/// `\\`, `\"`, `\n`, `\r`, `\t`, other control / non-ASCII bytes as 3-digit
/// octal escapes (never `\x`, which would swallow following hex digits).
#[must_use]
pub fn c_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(char::from(b)),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out
}

/// Make `text` safe inside a `/* .. */` block comment.
#[must_use]
pub fn block_comment_safe(text: &str) -> String {
    text.replace("*/", "* /")
}

/// Every api.json module the module's types live in (sorted), for printers
/// whose bindings are split into modules.
#[must_use]
pub fn used_api_modules(m: &Module) -> Vec<&'static str> {
    let mut mods: Vec<&'static str> = m
        .used_types()
        .iter()
        .filter_map(|t| super::lower_types::api_module(t))
        .collect();
    mods.sort_unstable();
    mods.dedup();
    mods
}

/// `true` if any float literal in the module is NaN or infinite.
#[must_use]
pub fn uses_nonfinite_float(m: &Module) -> bool {
    let mut found = false;
    for item in &m.items {
        item.value.walk(&mut |e| {
            if let Expr::Float { text, .. } = e {
                if text == "nan" || text.ends_with("inf") {
                    found = true;
                }
            }
        });
    }
    found
}

// ---------------------------------------------------------------- registry

/// Every language printer, in the order the docs list them.
#[must_use]
pub fn all() -> Vec<Box<dyn CodegenBackend>> {
    alloc::vec![Box::new(rust::Rust), Box::new(c::C)]
}

/// Look a printer up by its id or one of its aliases (`"c++"` -> `cpp`).
#[must_use]
pub fn by_name(name: &str) -> Option<Box<dyn CodegenBackend>> {
    let name = name.trim().to_ascii_lowercase();
    all()
        .into_iter()
        .find(|b| b.lang() == name || b.aliases().contains(&name.as_str()))
}

/// `[a, b]` -> `"a, b"` (for the error message of an unknown language).
#[must_use]
pub fn supported_list() -> String {
    all()
        .iter()
        .map(|b| b.lang().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
