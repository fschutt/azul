//! Regression: text in an UNRESOLVED `font-family` must still paint, in a fallback.
//!
//! An unresolved family is normal, not exceptional — the shipped widgets demo asks
//! for `Cantarell` (GNOME's UI font, absent on KDE and most non-GNOME installs),
//! `Sans` and `system:ui`, and gets
//! `[azul][font] UNRESOLVED font-family "Cantarell"` for all three. That warning is
//! correct behaviour. What must NEVER follow is a hash that layout shaped with and
//! the renderer then cannot resolve: layout picking a fallback face and the
//! rasterizer dropping the run is the failure mode this guards.
//!
//! Runs against the REAL system font cache (`render_dom_to_image` →
//! `build_font_cache`), so it exercises the same resolver the demo does.

#![cfg(all(
    feature = "cpurender",
    feature = "text_layout",
    feature = "font_loading"
))]

use azul_core::dom::{Dom, IdOrClass};
use azul_layout::cpurender::{render_dom_to_image, AzulPixmap};

fn css(decls: &str) -> azul_css::css::Css {
    let (c, _) = azul_css::parser2::new_from_str(&format!(".t {{ {decls} }}"));
    c
}

fn dark_pixels(pm: &AzulPixmap) -> usize {
    pm.data()
        .chunks_exact(4)
        .filter(|p| p[0] < 128 && p[1] < 128 && p[2] < 128)
        .count()
}

fn render_text_in(family: &str) -> usize {
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("t".to_string().into())].into())
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "HELLO WORLD",
        ));
    let style = css(&format!(
        "font-family: {family}; font-size: 40px; color: #000000;"
    ));
    let png = render_dom_to_image(dom, style, 400.0, 100.0, 1.0).expect("render_dom_to_image");
    let pm = AzulPixmap::decode_png(&png).expect("decode png");
    dark_pixels(&pm)
}

#[test]
fn text_in_a_family_that_exists_nowhere_renders_in_a_fallback() {
    let dark = render_text_in("\"Definitely Not Installed Sans\"");
    assert!(
        dark > 50,
        "text in an unresolved family painted {dark} dark pixels — layout resolved a \
         fallback face but the renderer could not draw it"
    );
}

/// The demo's exact stack. On a machine WITH Cantarell this simply resolves; on one
/// without (KDE, most non-GNOME Linux) it takes the fallback path. Either way the
/// text must reach the framebuffer.
#[test]
fn the_shipped_demos_cantarell_stack_renders_either_way() {
    let dark = render_text_in("Cantarell, Sans, sans-serif");
    assert!(
        dark > 50,
        "Cantarell/Sans/sans-serif painted {dark} dark pixels — this is the shipped \
         widgets-demo font stack"
    );
}
