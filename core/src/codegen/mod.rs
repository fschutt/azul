//! Markup, a DOM and component libraries → source code in every language azul
//! has bindings for: the DOM half of azul's code generator.
//!
//! Compiled only with the `codegen` cargo feature, the twin of azul-css's
//! `codegen` feature (the CSS half). There is ONE code generator: this crate
//! LOWERS markup to azul-css's language-neutral IR
//! ([`azul_css::codegen::ir`]), and azul-css's per-language printers
//! ([`azul_css::codegen::lang`], one per binding language) PRINT it, the same
//! printers that print stylesheets.
//!
//! ```text
//! markup (XmlNodeChild) --dom::lower_*--> ir::Module --backend(lang)--> source / project
//! ```
//!
//! * [`dom`] - markup → IR: elements, their CSS and attributes, template
//!   placeholders, component instances as calls of their functions.
//! * [`project`] - the two generators together (DOM + CSS) on top of the
//!   component API: a project for a language (the "zip"), markup as a
//!   function or an app, a component library, pasted HTML.
//!
//! Entry point for printing: [`backend`] (one "unknown language" answer for
//! every caller).

use alloc::{boxed::Box, string::String};

use azul_css::codegen::{backend_for, ir::Ident, supported_languages, CodegenBackend};

pub mod dom;
pub mod project;

/// The code generator for `language` (an id or an alias of
/// [`azul_css::codegen::supported_languages`]).
///
/// # Errors
/// An unknown language; the message lists the available ones.
pub fn backend(language: &str) -> Result<Box<dyn CodegenBackend>, String> {
    backend_for(language).ok_or_else(|| {
        format!(
            "no code generator for {language:?}; available: {}",
            supported_languages()
        )
    })
}

/// `render_<base>` in snake case: the name of the render function the code
/// export gives a subtree, a page or a component (`card` → `render_card`,
/// `my-card` → `render_my_card`).
#[must_use]
pub fn render_fn_name(base: &str) -> String {
    format!("render_{}", Ident::from_text(base).snake())
}

/// The warning a DOM export carries when `b`'s printer does not print DOM
/// construction (it prints the reason, item by item, instead of a UI).
#[must_use]
pub fn dom_warning(b: &dyn CodegenBackend) -> Option<String> {
    (!b.exports_dom()).then(|| {
        format!(
            "the {} printer does not print DOM construction yet: the code says why instead of \
             building the UI",
            b.display_name()
        )
    })
}
