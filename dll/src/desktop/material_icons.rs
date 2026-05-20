//! Embedded Material Icons font — owned by the dll, downstream of codegen.
//!
//! `azul-doc codegen all` generates `target/codegen/material_icons.ttf.br`,
//! and `azul-doc` builds (depends on) `azul-layout` — so the `include!` of
//! that generated artifact must NOT live in `azul-layout` (it created a
//! build cycle that broke on `cargo clean`). It lives here: the dll is the
//! consumer of the codegen output and is not built by `azul-doc`. The
//! decompressed TTF is handed to `azul_layout::icon::register_embedded_material_icons`.

/// Brotli-compressed Material Icons font (≈348 KB raw → ≈130 KB).
/// Decompressed lazily on first use.
#[cfg(feature = "icons")]
static MATERIAL_ICONS_BR: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/codegen/material_icons.ttf.br"
));

#[cfg(feature = "icons")]
static MATERIAL_ICONS_DECOMPRESSED: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();

/// Raw TTF bytes of the embedded Material Icons font (brotli-decompressed
/// on first access), or `None` when the `icons` feature is off.
#[cfg(feature = "icons")]
pub fn get_material_icons_font_bytes() -> Option<&'static [u8]> {
    Some(MATERIAL_ICONS_DECOMPRESSED.get_or_init(|| {
        let mut output = Vec::new();
        brotli_decompressor::BrotliDecompress(&mut &MATERIAL_ICONS_BR[..], &mut output)
            .expect("Failed to decompress Material Icons font");
        output
    }))
}

#[cfg(not(feature = "icons"))]
pub fn get_material_icons_font_bytes() -> Option<&'static [u8]> {
    None
}
