//! Prints every top-level item of a Rust source file, one tab-separated line
//! each, so `scripts/refactor/split.py` can find items by NAME and move their
//! text verbatim.
//!
//! ```text
//! item_index <file.rs> [--members]
//!
//! ITEM    <idx> <kind> <key> <first line> <last line> <vis> <vis insert line:col> <extra>
//! MEMBER  <item idx> <kind> <name> <line> <vis> <vis insert line:col>
//! ```
//!
//! * `first line` includes the item's doc comments and outer attributes; both
//!   lines are 1-based and inclusive. Plain `//` comments are not tokens, so
//!   they are never part of an item - the split script gives each item the gap
//!   above it.
//! * `vis insert line:col` (0-based char column) is where a `pub(super) ` token
//!   goes when the item or member has no visibility of its own, `-` otherwise.
//! * `extra`: `body=<open>:<close>` for an inline `mod x { .. }` (the lines of
//!   its braces), `self=<Type> trait=<Trait>` for an `impl`, `-` otherwise.
//! * `--members` adds the fields of structs and the fns / consts of impls.

use std::{env, fs, process::ExitCode};

use proc_macro2::{LineColumn, Span, TokenStream, TokenTree};
use quote::ToTokens;
use syn::{spanned::Spanned, Fields, ImplItem, Item, Visibility};

/// The first and last line any token of `ts` covers (line 0 = no location).
fn token_lines(ts: TokenStream, lo: &mut usize, hi: &mut usize) {
    for tt in ts {
        let (start, end) = match &tt {
            TokenTree::Group(g) => {
                token_lines(g.stream(), lo, hi);
                (g.span_open().start(), g.span_close().end())
            }
            other => (other.span().start(), other.span().end()),
        };
        if start.line != 0 {
            *lo = (*lo).min(start.line);
        }
        if end.line != 0 {
            *hi = (*hi).max(end.line);
        }
    }
}

fn item_lines(item: &Item) -> (usize, usize) {
    let (mut lo, mut hi) = (usize::MAX, 0);
    token_lines(item.to_token_stream(), &mut lo, &mut hi);
    (lo, hi)
}

fn compact(t: &impl ToTokens) -> String {
    t.to_token_stream().to_string().chars().filter(|c| !c.is_whitespace()).collect()
}

fn vis_str(v: &Visibility) -> String {
    match v {
        Visibility::Inherited => "inherited".into(),
        Visibility::Public(_) => "pub".into(),
        Visibility::Restricted(r) => format!("pub({}{})", if r.in_token.is_some() { "in " } else { "" }, compact(&r.path)),
    }
}

fn pos(lc: LineColumn) -> String {
    format!("{}:{}", lc.line, lc.column)
}

/// Where a visibility token would go: right before `anchor`, if `v` is empty.
fn vis_insert(v: &Visibility, anchor: Span) -> String {
    match v {
        Visibility::Inherited => pos(anchor.start()),
        _ => "-".into(),
    }
}

/// The first token of a fn signature (`const`, `async`, `unsafe`, `extern` or `fn`).
fn sig_anchor(sig: &syn::Signature) -> Span {
    if let Some(t) = &sig.constness {
        t.span
    } else if let Some(t) = &sig.asyncness {
        t.span
    } else if let Some(t) = &sig.unsafety {
        t.span
    } else if let Some(a) = &sig.abi {
        a.extern_token.span
    } else {
        sig.fn_token.span
    }
}

/// The last identifier of a type's path (`Foo` for `Foo<T>` / `crate::x::Foo`).
fn type_base(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default(),
        syn::Type::Reference(r) => type_base(&r.elem),
        syn::Type::Paren(p) => type_base(&p.elem),
        syn::Type::Group(g) => type_base(&g.elem),
        other => compact(other),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: item_index <file.rs> [--members]");
        return ExitCode::from(2);
    };
    let members = args.iter().any(|a| a == "--members");
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{path}: {e}");
            return ExitCode::from(2);
        }
    };
    let file = match syn::parse_file(&src) {
        Ok(f) => f,
        Err(e) => {
            let lc = e.span().start();
            eprintln!("{path}:{}:{}: {e}", lc.line, lc.column);
            return ExitCode::from(1);
        }
    };
    let lines: Vec<&str> = src.lines().collect();
    for (idx, item) in file.items.iter().enumerate() {
        let (start, end) = item_lines(item);
        let (kind, key, vis, ins, extra) = match item {
            Item::Fn(f) => ("fn", f.sig.ident.to_string(), vis_str(&f.vis), vis_insert(&f.vis, sig_anchor(&f.sig)), "-".to_string()),
            Item::Struct(s) => ("struct", s.ident.to_string(), vis_str(&s.vis), vis_insert(&s.vis, s.struct_token.span), "-".into()),
            Item::Enum(e) => ("enum", e.ident.to_string(), vis_str(&e.vis), vis_insert(&e.vis, e.enum_token.span), "-".into()),
            Item::Union(u) => ("union", u.ident.to_string(), vis_str(&u.vis), vis_insert(&u.vis, u.union_token.span), "-".into()),
            Item::Const(c) => ("const", c.ident.to_string(), vis_str(&c.vis), vis_insert(&c.vis, c.const_token.span), "-".into()),
            Item::Static(s) => ("static", s.ident.to_string(), vis_str(&s.vis), vis_insert(&s.vis, s.static_token.span), "-".into()),
            Item::Type(t) => ("type", t.ident.to_string(), vis_str(&t.vis), vis_insert(&t.vis, t.type_token.span), "-".into()),
            Item::Trait(t) => {
                let anchor = t.unsafety.map(|u| u.span).or(t.auto_token.map(|a| a.span)).unwrap_or(t.trait_token.span);
                ("trait", t.ident.to_string(), vis_str(&t.vis), vis_insert(&t.vis, anchor), "-".into())
            }
            Item::TraitAlias(t) => ("traitalias", t.ident.to_string(), vis_str(&t.vis), vis_insert(&t.vis, t.trait_token.span), "-".into()),
            Item::Mod(m) => {
                let extra = match &m.content {
                    Some((brace, _)) => {
                        format!("body={}:{}", brace.span.open().start().line, brace.span.close().start().line)
                    }
                    None => "external".into(),
                };
                ("mod", m.ident.to_string(), vis_str(&m.vis), vis_insert(&m.vis, m.mod_token.span), extra)
            }
            Item::Impl(i) => {
                let self_ty = compact(&*i.self_ty);
                let key = match &i.trait_ {
                    Some((bang, path, _)) => format!("impl {}{} for {}", if bang.is_some() { "!" } else { "" }, compact(path), self_ty),
                    None => format!("impl {self_ty}"),
                };
                let trait_key = i.trait_.as_ref().map(|(_, p, _)| compact(p)).unwrap_or_else(|| "-".into());
                ("impl", key, "-".into(), "-".into(), format!("self={} trait={}", type_base(&i.self_ty), trait_key))
            }
            Item::Use(u) => ("use", compact(&u.tree), vis_str(&u.vis), "-".into(), "-".into()),
            Item::Macro(m) => match &m.ident {
                Some(id) => ("macro_rules", id.to_string(), "-".into(), "-".into(), "-".into()),
                None => ("macro", format!("{}!", compact(&m.mac.path)), "-".into(), "-".into(), "-".into()),
            },
            Item::ExternCrate(e) => ("externcrate", e.ident.to_string(), vis_str(&e.vis), "-".into(), "-".into()),
            Item::ForeignMod(_) => ("foreignmod", "extern".into(), "-".into(), "-".into(), "-".into()),
            other => ("verbatim", compact(other), "-".into(), "-".into(), "-".into()),
        };
        // An outer attribute or doc comment on the line right above an item is
        // always one of its tokens; a sanity check that the range is whole.
        if start == usize::MAX || end < start || end > lines.len() {
            eprintln!("{path}: item {idx} ({kind} {key}) has no usable span");
            return ExitCode::from(1);
        }
        println!("ITEM\t{idx}\t{kind}\t{key}\t{start}\t{end}\t{vis}\t{ins}\t{extra}");
        if !members {
            continue;
        }
        match item {
            Item::Struct(s) => {
                let fields: Vec<&syn::Field> = match &s.fields {
                    Fields::Named(n) => n.named.iter().collect(),
                    Fields::Unnamed(u) => u.unnamed.iter().collect(),
                    Fields::Unit => Vec::new(),
                };
                for (n, f) in fields.into_iter().enumerate() {
                    let name = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_else(|| n.to_string());
                    let anchor = f.ident.as_ref().map(|i| i.span()).unwrap_or_else(|| f.ty.span());
                    println!(
                        "MEMBER\t{idx}\tfield\t{name}\t{}\t{}\t{}",
                        anchor.start().line,
                        vis_str(&f.vis),
                        vis_insert(&f.vis, anchor)
                    );
                }
            }
            Item::Impl(i) if i.trait_.is_none() => {
                for it in &i.items {
                    let (kind, name, vis, anchor) = match it {
                        ImplItem::Fn(f) => {
                            let anchor = f.defaultness.map(|d| d.span).unwrap_or_else(|| sig_anchor(&f.sig));
                            ("fn", f.sig.ident.to_string(), &f.vis, anchor)
                        }
                        ImplItem::Const(c) => {
                            let anchor = c.defaultness.map(|d| d.span).unwrap_or(c.const_token.span);
                            ("const", c.ident.to_string(), &c.vis, anchor)
                        }
                        ImplItem::Type(t) => {
                            let anchor = t.defaultness.map(|d| d.span).unwrap_or(t.type_token.span);
                            ("type", t.ident.to_string(), &t.vis, anchor)
                        }
                        _ => continue,
                    };
                    println!(
                        "MEMBER\t{idx}\t{kind}\t{name}\t{}\t{}\t{}",
                        anchor.start().line,
                        vis_str(vis),
                        vis_insert(vis, anchor)
                    );
                }
            }
            _ => {}
        }
    }
    ExitCode::SUCCESS
}
