//! A variable face a CSS family resolves to is never drawn as the variable file: shaping, glyph
//! decode and the PDF writer see an ordinary STATIC face (no `fvar`, no `gvar`) at the exact
//! coordinates, the default instance included.
//!
//! Before, a regular request drew a system FILE as it was (the raw variable font, at its default
//! instance, in the shaper and in the PDF) and a face registered in memory was cut into nine
//! static faces by weight bucket at registration. A static face is untouched.
//!
//! The three ways a face enters a chain: A, registered from memory by family name
//! (`FontManager::register_named_font`; printpdf's `register_input_fonts`), B, a font FILE found
//! by the system scan, and C, registered at runtime (`RegisteredFonts`, unit-tested in
//! `text3/fontreg_test.rs`, where its `ensure_in` is visible).

use rust_fontconfig::FcFontCache;

use crate::variable_fonts::{
    bytes_of, cache_with_file, chain_through, coverage_of, drawn_bytes, instance_bytes,
    is_variable, red_hat_mono_path, static_face_path, Manager,
};

/// B: a variable FILE, as the system scan registers `SFNS.ttf`.
#[test]
fn a_variable_file_is_drawn_as_a_static_instance_at_every_weight() {
    let (cache, family, _source, file) = cache_with_file(&red_hat_mono_path());
    assert!(is_variable(&file), "premise: the file is a variable font");
    for (css, wght) in [
        ("font-weight: 400", None),
        ("font-weight: bold", Some(700.0)),
        ("", None),
    ] {
        let drawn = drawn_bytes(&cache, &family, css);
        assert!(!is_variable(&drawn), "`{css}`: the face drawn is still a variable font");
        let expected = instance_bytes(&file, &wght.map_or(Vec::new(), |w| vec![(*b"wght", w)]));
        assert!(
            drawn == expected,
            "`{css}`: the face drawn is not the instance of the file at {wght:?}"
        );
    }
}

/// A: registered from memory, once, as the raw variable font.
#[test]
fn a_variable_font_registered_in_memory_is_drawn_as_a_static_instance() {
    let file = std::fs::read(red_hat_mono_path()).expect("the face is checked in");
    let coverage = coverage_of(&file);
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    let _id = manager.register_named_font("Azul Memory Variable", &file, coverage);
    for (css, wght) in [("font-weight: 400", None), ("font-weight: 700", Some(700.0))] {
        let chain = chain_through(&manager, "Azul Memory Variable", css);
        let drawn = bytes_of(&manager.fc_cache, &chain);
        assert!(!is_variable(&drawn), "`{css}`: the face drawn is still a variable font");
        let expected = instance_bytes(&file, &wght.map_or(Vec::new(), |w| vec![(*b"wght", w)]));
        assert!(drawn == expected, "`{css}`: not the instance of the file at {wght:?}");
    }
}

/// A static face is drawn as the file, untouched, whatever is asked of it.
#[test]
fn a_static_face_is_drawn_as_the_file() {
    let (cache, family, _source, file) = cache_with_file(&static_face_path());
    for css in ["", "font-weight: 700", "font-weight: 350", "font-stretch: 80%"] {
        assert!(
            drawn_bytes(&cache, &family, css) == file,
            "`{css}`: a static face must be drawn as the file"
        );
    }
}
