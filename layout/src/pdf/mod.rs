//! Intermediate representation for PDF rendering operations.
//!
//! This module provides a PDF-agnostic way to generate PDF documents from azul layouts.
//! It does not depend on any specific PDF library (like printpdf), making it suitable
//! for use as an intermediate format that can be consumed by external PDF generators.

pub mod display_list_to_pdf;
pub mod pdf_ops;
pub mod resources;

pub use display_list_to_pdf::{display_list_to_pdf_ops, PdfPageRender};
pub use pdf_ops::{FontId, PdfColor, PdfLine, PdfOp, PdfPoint, PdfTextMatrix, TextItem, XObjectId};
pub use resources::PdfRenderResources;
