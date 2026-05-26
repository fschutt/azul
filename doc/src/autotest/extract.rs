//! Function extraction for the `autotest` harness.
//!
//! This module parses a Rust source file with `syn` and extracts every testable
//! function — both free `Item::Fn` and `Item::Impl` -> `ImplItem::Fn`. It records
//! the name, full signature string, argument names + types, return type, generics,
//! visibility, the impl `Self`-type for methods, the doc comment, and any `#[cfg(...)]`
//! gate.
//!
//! `#[test]` / `#[cfg(test)]` items and trivial trait-derive style methods are skipped.
//!
//! NOTE: the syn-parsing helpers here are deliberately self-contained copies of the
//! patterns in `autofix::type_index` (which is read-only for this command). They are
//! intentionally simpler — autotest only needs printable signatures + arg/return
//! type strings, not the full FFI normalization that autofix performs.

use std::{fs, path::Path};

use quote::ToTokens;
use syn::{File, ImplItem, Item};

/// How `self` is passed to a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfKind {
    /// `self` — takes ownership
    Value,
    /// `&self` — immutable borrow
    Ref,
    /// `&mut self` — mutable borrow
    RefMut,
}

impl SelfKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SelfKind::Value => "self",
            SelfKind::Ref => "&self",
            SelfKind::RefMut => "&mut self",
        }
    }
}

/// A single (non-self) function argument.
#[derive(Debug, Clone)]
pub struct FnArg {
    /// Argument name (e.g. `input`).
    pub name: String,
    /// Type as a printable string (e.g. `&str`, `&[u8]`, `f32`).
    pub ty: String,
}

/// A function or method extracted from a source file.
#[derive(Debug, Clone)]
pub struct ExtractedFn {
    /// Function name (e.g. `parse`, `from_str`).
    pub name: String,
    /// Full signature string, e.g. `pub fn parse(s: &str) -> Result<Self, JsonParseError>`.
    pub signature: String,
    /// How `self` is passed (None for free functions / static methods).
    pub self_kind: Option<SelfKind>,
    /// Non-self arguments.
    pub args: Vec<FnArg>,
    /// Return type as a printable string (None for `()` / no return).
    pub return_type: Option<String>,
    /// Generic parameters declared on the function (e.g. `T`, `'a`).
    pub generics: Vec<String>,
    /// Whether the function is `pub`.
    pub is_pub: bool,
    /// For methods: the impl `Self`-type (e.g. `Json`). None for free functions.
    pub self_type: Option<String>,
    /// Doc comment lines (rustdoc convention: one leading space trimmed).
    pub doc: Vec<String>,
    /// The `#[cfg(...)]` gate string if present (e.g. `target_arch = "wasm32"`).
    pub cfg: Option<String>,
}

impl ExtractedFn {
    /// A printable "qualified" name: `Json::parse` for methods, `parse_float_value`
    /// for free functions.
    pub fn qualified_name(&self) -> String {
        match &self.self_type {
            Some(ty) => format!("{}::{}", ty, self.name),
            None => self.name.clone(),
        }
    }
}

/// Parse a single source file and extract every testable function.
///
/// Returns `Err` only on read / parse failure; an empty `Vec` means the file had no
/// testable functions.
pub fn extract_functions_from_file(file_path: &Path) -> Result<Vec<ExtractedFn>, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;

    let syntax_tree: File = syn::parse_file(&content)
        .map_err(|e| format!("Failed to parse {}: {}", file_path.display(), e))?;

    let mut out = Vec::new();
    extract_from_items(&syntax_tree.items, &mut out);
    Ok(out)
}

/// Recursively walk a list of items, collecting functions from free `fn`s, inherent
/// impls, and inline modules.
fn extract_from_items(items: &[Item], out: &mut Vec<ExtractedFn>) {
    for item in items {
        match item {
            // Free function: `fn foo(...) { ... }`
            Item::Fn(f) => {
                if is_cfg_test(&f.attrs) || has_test_attr(&f.attrs) {
                    continue;
                }
                if let Some(extracted) = extract_free_fn(f) {
                    out.push(extracted);
                }
            }

            // Inherent impl block: `impl Type { ... }`
            Item::Impl(impl_item) => {
                // Skip trait impls — those are mostly derive-equivalents (Display,
                // Default, From, etc.). We DO still want their fns categorized as
                // serializers/constructors, but only for a curated set of std traits;
                // pure marker / auto-derive trait impls add noise. To keep coverage
                // useful we keep inherent impls (trait_ == None) AND a small set of
                // hand-written std trait impls (Display / FromStr) that are genuinely
                // worth testing.
                let trait_name = impl_item
                    .trait_
                    .as_ref()
                    .and_then(|(_, path, _)| path.segments.last())
                    .map(|seg| seg.ident.to_string());

                let is_inherent = trait_name.is_none();
                let is_testworthy_trait = matches!(
                    trait_name.as_deref(),
                    Some("Display") | Some("FromStr")
                );

                if !is_inherent && !is_testworthy_trait {
                    continue;
                }

                // Skip the whole impl if it is cfg(test)-gated.
                if is_cfg_test(&impl_item.attrs) {
                    continue;
                }

                let self_type = type_path_name(impl_item.self_ty.as_ref());
                let impl_cfg = cfg_string(&impl_item.attrs);

                for impl_item_inner in &impl_item.items {
                    if let ImplItem::Fn(method) = impl_item_inner {
                        if is_cfg_test(&method.attrs) || has_test_attr(&method.attrs) {
                            continue;
                        }
                        if let Some(mut extracted) =
                            extract_method(method, self_type.as_deref())
                        {
                            // Inherit the impl-level cfg gate if the method has none.
                            if extracted.cfg.is_none() {
                                extracted.cfg = impl_cfg.clone();
                            }
                            // Trait-impl methods (Display::fmt / FromStr::from_str)
                            // are public via the trait even without a `pub` keyword.
                            if !is_inherent {
                                extracted.is_pub = true;
                            }
                            out.push(extracted);
                        }
                    }
                }
            }

            // Inline module: `mod foo { ... }`
            Item::Mod(m) => {
                if is_cfg_test(&m.attrs) {
                    continue;
                }
                if let Some((_, nested_items)) = &m.content {
                    extract_from_items(nested_items, out);
                }
            }

            _ => {}
        }
    }
}

/// Extract a free function (`Item::Fn`).
fn extract_free_fn(f: &syn::ItemFn) -> Option<ExtractedFn> {
    let name = f.sig.ident.to_string();
    let is_pub = matches!(&f.vis, syn::Visibility::Public(_));

    let (args, _self_kind) = extract_args(&f.sig);
    let return_type = extract_return_type(&f.sig);
    let generics = extract_generics(&f.sig);
    let doc = extract_doc_comments(&f.attrs);
    let cfg = cfg_string(&f.attrs);
    let signature = signature_string(&f.vis, &f.sig);

    Some(ExtractedFn {
        name,
        signature,
        self_kind: None,
        args,
        return_type,
        generics,
        is_pub,
        self_type: None,
        doc,
        cfg,
    })
}

/// Extract a method from an impl block (`ImplItem::Fn`).
fn extract_method(method: &syn::ImplItemFn, self_type: Option<&str>) -> Option<ExtractedFn> {
    let name = method.sig.ident.to_string();
    let is_pub = matches!(&method.vis, syn::Visibility::Public(_));

    let (args, self_kind) = extract_args(&method.sig);
    let return_type = extract_return_type(&method.sig);
    let generics = extract_generics(&method.sig);
    let doc = extract_doc_comments(&method.attrs);
    let cfg = cfg_string(&method.attrs);
    let signature = signature_string(&method.vis, &method.sig);

    Some(ExtractedFn {
        name,
        signature,
        self_kind,
        args,
        return_type,
        generics,
        is_pub,
        self_type: self_type.map(|s| s.to_string()),
        doc,
        cfg,
    })
}

/// Extract non-self arguments and detect the `self` receiver kind.
fn extract_args(sig: &syn::Signature) -> (Vec<FnArg>, Option<SelfKind>) {
    let mut args = Vec::new();
    let mut self_kind = None;

    for input in &sig.inputs {
        match input {
            syn::FnArg::Receiver(receiver) => {
                self_kind = Some(if receiver.reference.is_some() {
                    if receiver.mutability.is_some() {
                        SelfKind::RefMut
                    } else {
                        SelfKind::Ref
                    }
                } else {
                    SelfKind::Value
                });
            }
            syn::FnArg::Typed(pat_type) => {
                let name = match pat_type.pat.as_ref() {
                    syn::Pat::Ident(pat_ident) => pat_ident.ident.to_string(),
                    _ => "_".to_string(),
                };
                let ty = type_string(&pat_type.ty);
                args.push(FnArg { name, ty });
            }
        }
    }

    (args, self_kind)
}

/// Extract the return type as a printable string. `None` for `()` / no return.
fn extract_return_type(sig: &syn::Signature) -> Option<String> {
    match &sig.output {
        syn::ReturnType::Default => None,
        syn::ReturnType::Type(_, ty) => {
            let s = type_string(ty);
            if s.is_empty() || s == "()" {
                None
            } else {
                Some(s)
            }
        }
    }
}

/// Extract generic parameter names (type params + lifetimes + const params).
fn extract_generics(sig: &syn::Signature) -> Vec<String> {
    sig.generics
        .params
        .iter()
        .map(|p| match p {
            syn::GenericParam::Type(tp) => tp.ident.to_string(),
            syn::GenericParam::Lifetime(lt) => format!("'{}", lt.lifetime.ident),
            syn::GenericParam::Const(c) => format!("const {}", c.ident),
        })
        .collect()
}

/// Build a clean, single-line signature string from visibility + signature.
fn signature_string(vis: &syn::Visibility, sig: &syn::Signature) -> String {
    let vis_str = match vis {
        syn::Visibility::Public(_) => "pub ",
        _ => "",
    };
    let sig_str = clean_type_string(&sig.to_token_stream().to_string());
    format!("{}{}", vis_str, sig_str)
}

/// Render a `syn::Type` to a clean printable string.
fn type_string(ty: &syn::Type) -> String {
    clean_type_string(&ty.to_token_stream().to_string())
}

/// Get the last path segment ident of an impl `self_ty` (e.g. `Json` from
/// `crate::json::Json`). Returns None for non-path types.
fn type_path_name(ty: &syn::Type) -> Option<String> {
    if let syn::Type::Path(type_path) = ty {
        type_path
            .path
            .segments
            .last()
            .map(|seg| seg.ident.to_string())
    } else {
        None
    }
}

/// Clean up a type / signature string: collapse whitespace and fix syn's pointer /
/// reference spacing quirks (`* const T` -> `*const T`, `& 'a T` -> `&'a T`).
fn clean_type_string(s: &str) -> String {
    let result = s.split_whitespace().collect::<Vec<_>>().join(" ");
    result
        .replace("* const", "*const")
        .replace("* mut", "*mut")
        .replace("& mut", "&mut")
        .replace("& '", "&'")
        // Collapse "& T" (reference to a type/`self`) -> "&T". Must run after the
        // "& mut"/"& '" fixes above so we don't mangle those.
        .replace("& ", "&")
        .replace(" < ", "<")
        .replace(" > ", ">")
        .replace("< ", "<")
        .replace(" >", ">")
        .replace(" ,", ",")
        .replace(" :: ", "::")
        .replace(":: ", "::")
        .replace(" ::", "::")
        // Tidy "name : Type" -> "name: Type" in printed signatures.
        .replace(" : ", ": ")
}

/// Extract doc comments from attributes as a multi-line vector (one leading space
/// trimmed, per rustdoc convention).
fn extract_doc_comments(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut docs = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    let line = s.value();
                    let trimmed = if line.starts_with(' ') {
                        &line[1..]
                    } else {
                        &line[..]
                    };
                    docs.push(trimmed.to_string());
                }
            }
        }
    }
    docs
}

/// True if any attribute is `#[test]`.
fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("test"))
}

/// True if any attribute is `#[cfg(test)]`.
fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        if !a.path().is_ident("cfg") {
            return false;
        }
        if let syn::Meta::List(list) = &a.meta {
            // Match `test`, `all(test, ...)`, `feature = "..."` is fine to keep.
            let s = list.tokens.to_string();
            // crude but effective: a bare `test` token gated module/fn
            s.split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|tok| tok == "test")
        } else {
            false
        }
    })
}

/// Return the `#[cfg(...)]` gate string (inner tokens) if present, else None.
/// Skips `cfg(test)` gates (those items are filtered out entirely).
fn cfg_string(attrs: &[syn::Attribute]) -> Option<String> {
    for a in attrs {
        if !a.path().is_ident("cfg") {
            continue;
        }
        if let syn::Meta::List(list) = &a.meta {
            let s = clean_type_string(&list.tokens.to_string());
            return Some(s);
        }
    }
    None
}
