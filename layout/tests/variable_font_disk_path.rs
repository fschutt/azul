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
    // IN-REPO FALLBACK (2026-08-20). The three paths above are the variable
    // Ubuntu family, which only Ubuntu 23.10+ ships — on the ubuntu-22.04
    // runners `fonts-ubuntu` installs the STATIC instances (Ubuntu-R.ttf, …)
    // and nothing here matched, so this test printed `SKIP` and returned Ok in
    // every CI run it was ever part of. `doc/fonts/RedHatMono-VariableFont_wght`
    // is a real wght-variable TrueType face checked into this repository, so
    // the assertion below now has something to assert against on every host,
    // with no package to install and no runner-image drift to track.
    let vendored = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../doc/fonts/RedHatMono-VariableFont_wght.ttf");
    if vendored.exists() {
        return Some(vendored);
    }
    None
}

/// A test that prints `SKIP:` and returns `Ok` is indistinguishable from a test
/// that passed — the runner shows the same green `ok` line either way.
/// `AZ_REQUIRE_TEST_FONTS=1` (set by CI's `test_lib` job) says "this machine is
/// supposed to HAVE the fonts", and turns the skip into a hard failure.
///
/// Deliberately NOT `-> !` / `process::exit`: this file is a `mod` of
/// `tests/all.rs`, so exiting the process here would take ~940 unrelated tests
/// with it. The caller `return`s.
fn missing_font(what: &str) {
    assert!(
        std::env::var_os("AZ_REQUIRE_TEST_FONTS").is_none(),
        "AZ_REQUIRE_TEST_FONTS=1 but {what}. This job is supposed to have the \
         fonts installed (apt: fonts-ubuntu); either install them or unset \
         AZ_REQUIRE_TEST_FONTS. Silently skipping is NOT a pass."
    );
    eprintln!("SKIP: {what} (set AZ_REQUIRE_TEST_FONTS=1 to make this a failure)");
}

#[test]
fn variable_font_from_disk_yields_usable_glyphs() {
    let Some(path) = variable_font_path() else {
        missing_font("no variable Ubuntu font installed");
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
