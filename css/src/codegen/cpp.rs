//! C++ backend stub.
//!
//! Returns a single placeholder source file noting that the
//! emitter is not implemented; the dispatch path in [`super::backend_for`] is
//! still wired so `?lang=cpp` produces a structured response instead of a 404.

use alloc::{string::String, string::ToString, vec, vec::Vec};

use super::{CodegenBackend, GeneratedFile};
use crate::css::Css;

#[derive(Copy, Clone, Debug)]
pub struct CppBackend;

impl CodegenBackend for CppBackend {
    fn lang(&self) -> &'static str {
        "cpp"
    }

    fn emit_css(&self, _css: &Css) -> String {
        "// TODO: C++ codegen backend not yet implemented.\n".to_string()
    }

    fn emit_project(&self, css: &Css) -> Vec<GeneratedFile> {
        vec![GeneratedFile {
            path: "main.cpp".to_string(),
            contents: self.emit_css(css),
        }]
    }
}
