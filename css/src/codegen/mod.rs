//! Multi-language code generation backends for parsed CSS.
//!
//! The kernel of a future "compile UI to standalone project" feature exposed by
//! the `azul-dll` debug HTTP server: a parsed [`Css`](crate::css::Css) is fed to
//! a [`CodegenBackend`] implementation, which returns a complete source string
//! plus auxiliary files (e.g. `Cargo.toml`) for a target language.
//!
//! Today only [`rust::RustBackend`] is implemented; [`cpp`] and [`python`] hold
//! stub backends so the public API stays stable as new languages come online.

use alloc::{string::String, vec::Vec};

use crate::css::Css;

pub mod cpp;
pub mod format;
pub mod python;
pub mod rust;

/// One emitted source artifact (e.g. `src/main.rs`, `Cargo.toml`).
#[derive(Debug)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

/// Pluggable code-generation strategy. Each backend turns a parsed [`Css`] into
/// a list of files that, taken together, form a buildable standalone project.
pub trait CodegenBackend {
    /// Stable identifier (e.g. `"rust"`) used by the HTTP layer to pick a backend.
    fn lang(&self) -> &'static str;

    /// Render a single CSS expression. Useful for tests / quick previews where
    /// the caller doesn't want a full project layout.
    fn emit_css(&self, css: &Css) -> String;

    /// Emit a complete standalone project. The returned files are root-relative.
    fn emit_project(&self, css: &Css) -> Vec<GeneratedFile>;
}

/// Look up a backend by its [`CodegenBackend::lang`] identifier.
#[must_use] pub fn backend_for(lang: &str) -> Option<Box<dyn CodegenBackend>> {
    match lang {
        "rust" => Some(Box::new(rust::RustBackend)),
        "cpp" | "c++" => Some(Box::new(cpp::CppBackend)),
        "python" | "py" => Some(Box::new(python::PythonBackend)),
        _ => None,
    }
}
