//! miniword ENGINE-ISSUE 1/2 decisive experiment: a VARIABLE font loaded
//! through the normal DISK path (`ParsedFont::from_bytes_shared`, the same
//! call `load_font_shared`/`load_missing_for_chains` make) must yield
//! usable glyphs — the report hypothesized that disk-scanned variable
//! faces "skip baking" and render as .notdef boxes.

use std::sync::Arc;

fn variable_font_path() -> Option<std::path::PathBuf> {
    // Any of these exercise the same wght/wdth-variable TrueType shape.
    for p in [
        "/usr/share/fonts/truetype/ubuntu/Ubuntu[wdth,wght].ttf",
        "/usr/share/fonts/truetype/ubuntu/UbuntuSans[wdth,wght].ttf",
        "/usr/share/fonts/truetype/ubuntu/UbuntuMono[wght].ttf",
    ] {
        let pb = std::path::PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    None
}

#[test]
fn variable_font_from_disk_yields_usable_glyphs() {
    let Some(path) = variable_font_path() else {
        eprintln!("SKIP: no variable Ubuntu font installed");
        return;
    };
    let bytes = std::fs::read(&path).expect("read font file");
    let font_bytes = Arc::new(rust_fontconfig::FontBytes::Owned(bytes.into()));

    let mut warnings = Vec::new();
    let parsed = azul_layout::font::parsed::ParsedFont::from_bytes_shared(font_bytes, 0, &mut warnings)
        .expect("variable font must parse through the disk path");

    // cmap must map basic Latin — .notdef would be glyph 0.
    for ch in ['A', 'a', 'H', '0'] {
        let gid = parsed.lookup_glyph_index(ch as u32);
        assert!(
            gid.is_some_and(|g| g != 0),
            "codepoint {ch:?} must map to a real glyph in {path:?}, got {gid:?}"
        );
    }

    // The mapped glyph must have real outline data (this is where an
    // unbaked/misparsed variable face would produce empty boxes).
    let gid = parsed.lookup_glyph_index('A' as u32).unwrap();
    let advance = parsed.get_horizontal_advance(gid);
    assert!(
        advance > 0,
        "glyph {gid} advance must be positive, got {advance}"
    );
}
