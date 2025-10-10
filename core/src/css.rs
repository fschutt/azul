//! Module for printing the CSS to Rust code
//!
//! Re-exports functionality from azul_css::format_rust_code

// Re-export the formatting traits and types from azul_css
pub use azul_css::format_rust_code::{
    css_to_rust_code, format_static_css_prop, FormatAsRustCode, GetHash, VecContents,
};
