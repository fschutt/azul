//! PDF export for AzulDoc (SUPER_PLAN_2 §4 P5.1), via `printpdf`.
//!
//! Like `Db` (P4.3), the export API is **always present** and the engine
//! sits behind the `pdf` feature — so it flows through normal api.json
//! codegen with no feature-gating. Without `pdf`, `export_to_pdf` returns
//! `false` (printpdf isn't compiled).
//!
//! `printpdf` is pulled with `default-features = false`: its default `html`
//! feature would drag in `azul-layout` (printpdf's own layout integration)
//! and cycle with our local crate. We only use printpdf's core
//! `PdfDocument`/`Op` API and walk **Azul's** display list ourselves.
//!
//! v1 (this tick) writes a single blank A4 page — proves the engine works
//! end-to-end + establishes the always-present API. The real export (walk
//! the display list → printpdf `Op`s, research/06 §2.3.2's dispatch table;
//! `DisplayListItem::TextLayout` is already half-wired) lands next.

/// Export a PDF to `path`. Returns `true` on success; `false` without the
/// `pdf` feature or on a write error.
pub fn export_to_pdf(path: &str) -> bool {
    #[cfg(feature = "pdf")]
    {
        engine::export(path)
    }
    #[cfg(not(feature = "pdf"))]
    {
        let _ = path;
        false
    }
}

#[cfg(feature = "pdf")]
mod engine {
    use printpdf::{Mm, PdfDocument, PdfPage, PdfSaveOptions, PdfWarnMsg};

    pub fn export(path: &str) -> bool {
        let mut doc = PdfDocument::new("AzulDoc export");
        // A4 portrait, blank for now — the DisplayListItem → Op dispatch is
        // the follow-up. `Vec::new()` infers `Vec<Op>` from `PdfPage::new`.
        let page = PdfPage::new(Mm(210.0), Mm(297.0), Vec::new());
        doc.with_pages(vec![page]);
        let mut warnings: Vec<PdfWarnMsg> = Vec::new();
        let bytes = doc.save(&PdfSaveOptions::default(), &mut warnings);
        std::fs::write(path, bytes).is_ok()
    }
}
