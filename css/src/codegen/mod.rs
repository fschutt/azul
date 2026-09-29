//! CSS -> source code, for every language azul has bindings for.
//!
//! Compiled only with the `codegen` cargo feature. The pipeline is
//!
//! ```text
//! CSS text --parser2--> Css --lower--> ir::Module --lang::<x>--> source / project
//! ```
//!
//! * [`ir`] - ONE language-neutral IR: constructor calls, enum variants,
//!   struct literals, vectors, strings and primitive literals, in api.json
//!   vocabulary.
//! * [`lower`] (+ the generated, exhaustive [`lower_types`]) - the parsed
//!   model -> IR, written once: a `match` over every `CssProperty` variant and
//!   one `impl Lower` per reachable api.json type (conditions `@media` /
//!   `@os` / `@theme` / pseudo-states, `env()` references, gradients,
//!   transforms, shadows, filters, grid, animations ...).
//! * [`lang`] - one small printer per binding language, following that
//!   language's binding naming rules (`doc/src/codegen/v2/lang_*`).
//!
//! Entry points: [`backend_for`] / [`all_backends`] and the
//! [`CodegenBackend`] methods (`emit_css` = named styles, `emit_stylesheet` =
//! the exact `Css` value, `emit_project` = a buildable standalone project).
//!
//! [`format`] is the legacy Rust-only `FormatAsRustCode` formatter (kept for
//! `azul-core`'s `xml.rs`; superseded by this pipeline).

use alloc::{boxed::Box, string::String, vec::Vec};

use crate::css::Css;

pub mod doc;
pub mod format;
pub mod ir;
pub mod lang;
pub mod lower;
pub mod lower_types;

/// The api.json version the generated build files depend on.
pub use crate::AZUL_VERSION;

/// One emitted source artifact (e.g. `src/main.rs`, `Cargo.toml`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedFile {
    /// Project-root-relative path, `/`-separated.
    pub path: String,
    pub contents: String,
}

/// A code-generation target language.
pub trait CodegenBackend {
    /// Stable identifier (`"rust"`, `"c"`, `"cpp"`, `"python"`, ...).
    fn lang(&self) -> &'static str;

    /// Other names [`backend_for`] accepts (`"c++"`, `"py"`, ...).
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    /// Human-readable name (`"C++"`).
    fn display_name(&self) -> &'static str;

    /// File extension of [`CodegenBackend::emit_module`] output (no dot).
    fn extension(&self) -> &'static str;

    /// `true` if this printer prints DOM construction (builder methods,
    /// item parameters, apps - see the DOM section of [`ir`]); otherwise a
    /// DOM item comes out as "not expressible" with the reason.
    fn exports_dom(&self) -> bool {
        false
    }

    /// Print a lowered module (the snippet: imports + one function per item).
    fn emit_module(&self, module: &ir::Module) -> String;

    /// A buildable standalone project around `module` (build file + the
    /// snippet + a `main` that builds every item). Paths are root-relative.
    fn emit_project_files(&self, module: &ir::Module) -> Vec<GeneratedFile>;

    /// The stylesheet as named styles (one `CssPropertyWithConditionsVec`
    /// per base selector, conditions as `apply_if`) - see [`lower::lower_styles`].
    fn emit_css(&self, css: &Css) -> String {
        self.emit_module(&lower::lower_styles(css))
    }

    /// The exact `Css` value - see [`lower::lower_stylesheet`].
    fn emit_stylesheet(&self, css: &Css) -> String {
        self.emit_module(&lower::lower_stylesheet(css))
    }

    /// A standalone project around [`CodegenBackend::emit_css`].
    fn emit_project(&self, css: &Css) -> Vec<GeneratedFile> {
        self.emit_project_files(&lower::lower_styles(css))
    }
}

/// Look up a backend by its [`CodegenBackend::lang`] id or an alias.
#[must_use]
pub fn backend_for(lang: &str) -> Option<Box<dyn CodegenBackend>> {
    lang::by_name(lang)
}

/// Every backend, in documentation order.
#[must_use]
pub fn all_backends() -> Vec<Box<dyn CodegenBackend>> {
    lang::all()
}

/// `"rust, c, cpp, ..."` - for error messages.
#[must_use]
pub fn supported_languages() -> String {
    lang::supported_list()
}
