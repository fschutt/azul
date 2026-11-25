//! `font-optical-sizing: auto` (the initial value) draws a face with an `opsz` axis at the used
//! font size in CSS px, clamped to the axis; `none` draws the axis default; an explicit
//! `"opsz"` value of `font-variation-settings` beats `auto`.
//!
//! The fixture `azul-mock-variable.ttf` has `opsz` 8 to 144, default 14, and the glyph `O`
//! whose advance is 500 + 2 * (opsz - 14). macOS draws its whole UI from one face with an `opsz`
//! axis (`SFNS.ttf`, 17 to 96, default 28): that case runs where the file exists.

use crate::variable_fonts::{advance_of, cache_with_file, drawn_bytes, mock_variable_path};

fn opsz_advance(cache: &rust_fontconfig::FcFontCache, family: &str, css: &str) -> u16 {
    advance_of(&drawn_bytes(cache, family, css), 'O')
}

#[test]
fn auto_draws_the_face_at_the_used_font_size() {
    let (cache, family, _source, _file) = cache_with_file(&mock_variable_path());
    assert_eq!(opsz_advance(&cache, &family, "font-size: 20px"), 512, "20px: opsz 20");
    assert_eq!(opsz_advance(&cache, &family, "font-size: 14px"), 500, "14px: the default");
    assert_eq!(opsz_advance(&cache, &family, "font-size: 8px"), 488, "8px: opsz 8");
    assert_eq!(opsz_advance(&cache, &family, "font-size: 4px"), 488, "4px clamps to the 8 of the axis");
    assert_eq!(opsz_advance(&cache, &family, "font-size: 144px"), 760, "144px: opsz 144");
    assert_eq!(opsz_advance(&cache, &family, "font-size: 200px"), 760, "200px clamps to the 144 of the axis");
    assert_eq!(
        opsz_advance(&cache, &family, "font-size: 12.4px"),
        496,
        "the size is rounded to whole px first: opsz 12"
    );
    assert_eq!(
        opsz_advance(&cache, &family, "font-optical-sizing: auto; font-size: 20px"),
        512,
        "`auto` spelled out is the initial value"
    );
}

#[test]
fn none_draws_the_default_instance() {
    let (cache, family, _source, _file) = cache_with_file(&mock_variable_path());
    for size in ["8px", "20px", "144px"] {
        assert_eq!(
            opsz_advance(
                &cache,
                &family,
                &format!("font-optical-sizing: none; font-size: {size}")
            ),
            500,
            "`none` at {size}: the default optical size"
        );
    }
}

#[test]
fn an_explicit_opsz_beats_auto() {
    let (cache, family, _source, _file) = cache_with_file(&mock_variable_path());
    assert_eq!(
        opsz_advance(
            &cache,
            &family,
            "font-size: 8px; font-variation-settings: \"opsz\" 20"
        ),
        512,
        "\"opsz\" 20 at 8px"
    );
    assert_eq!(
        opsz_advance(
            &cache,
            &family,
            "font-optical-sizing: none; font-variation-settings: \"opsz\" 144"
        ),
        760,
        "\"opsz\" 144 with `none`"
    );
}

/// The face macOS draws its UI in: 13px text is "SF Pro Text" (opsz 17, the bottom of the axis),
/// 28px is the default instance's design, and `none` is the default at any size. Skipped where
/// the file does not exist.
#[test]
fn the_system_font_of_macos_follows_the_size_unless_it_is_none() {
    let path = std::path::Path::new("/System/Library/Fonts/SFNS.ttf");
    if !path.exists() {
        eprintln!("SKIP: no /System/Library/Fonts/SFNS.ttf on this host (macOS only)");
        return;
    }
    let (cache, family, _source, _file) = cache_with_file(path);
    let at = |css: &str| drawn_bytes(&cache, &family, css);
    let small = at("font-size: 13px");
    let large = at("font-size: 28px");
    let none = at("font-optical-sizing: none; font-size: 13px");
    assert!(small != large, "13px and 28px are two optical sizes of the system font");
    assert!(none == large, "`none` draws the default optical size (28)");
    assert!(
        !crate::variable_fonts::is_variable(&small),
        "the face drawn is a static instance"
    );
}
