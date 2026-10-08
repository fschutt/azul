//! An HVIF icon registered by name (`register_hvif_icon`, the way an SVG icon
//! is) resolves to the picture drawn at the size its `<icon>` is shown at -
//! the icon's font size, as a glyph icon's - and renders like any icon. A
//! registered name wins over nothing; bytes that are no HVIF register nothing.

#![cfg(all(feature = "cpurender", feature = "text_layout", feature = "font_loading"))]

use azul_core::{
    dom::Dom,
    icon::{resolve_icons_in_dom, IconMeta, SharedIconProvider},
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::{font::StyleFontSize, PixelValue},
        layout::LayoutDisplay,
        property::CssProperty,
    },
    system::SystemStyle,
};
use azul_layout::{
    cpurender::{render_dom_to_image, AzulPixmap},
    icon::{create_default_icon_provider, register_hvif_icon},
};

fn haiku(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/../examples/azul-icons-haiku/icons/{name}.hvif",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

#[test]
fn bytes_that_are_no_hvif_register_nothing() {
    let mut provider = create_default_icon_provider();
    assert!(!register_hvif_icon(&mut provider, "app", "x", b"not an icon", IconMeta::for_image()));
    assert!(register_hvif_icon(&mut provider, "app", "x", &haiku("Mail_Reply"), IconMeta::for_image()));
}

#[test]
fn an_hvif_icon_draws_its_colours_at_its_font_size() {
    let mut provider = create_default_icon_provider();
    // "Reply": Haiku's green arrow.
    assert!(register_hvif_icon(&mut provider, "app", "reply-arrow", &haiku("Mail_Reply"), IconMeta::for_image()));
    let shared = SharedIconProvider::from_handle(provider);
    let icon = Dom::create_icon("reply-arrow").with_css_props(
        vec![
            CssPropertyWithConditions::simple(CssProperty::display(LayoutDisplay::Block)),
            CssPropertyWithConditions::simple(CssProperty::font_size(StyleFontSize {
                inner: PixelValue::px(32.0),
            })),
        ]
        .into(),
    );
    let mut dom = Dom::create_div().with_child(icon);
    resolve_icons_in_dom(&mut dom, &shared, &SystemStyle::default());
    let green = green_pixels(dom);
    assert!(green.len() > 100, "the green arrow is drawn: {} pixels", green.len());
    // Drawn 32 px wide, not at its 64-unit grid or the default size.
    let max_x = green.iter().map(|p| p.0).max().unwrap_or(0);
    assert!(max_x < 33 && max_x > 20, "within its 32 px box: x up to {max_x}");
}

/// The pixels of Haiku's green arrow in `dom` drawn 48 x 48.
fn green_pixels(dom: Dom) -> Vec<(u32, u32)> {
    let png = render_dom_to_image(dom, azul_css::css::Css::empty(), 48.0, 48.0, 1.0)
        .expect("render the icon");
    let pm = AzulPixmap::decode_png(&png).expect("decode the render");
    let data = pm.data();
    (0..pm.height)
        .flat_map(|y| (0..pm.width).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let i = ((y * pm.width + x) * 4) as usize;
            data[i + 1] > 120 && data[i] < 120 && data[i + 2] < 120
        })
        .collect()
}

/// `Dom::with_css` is a scoped stylesheet (`* { font-size: 16px }`), not the
/// node's inline style, and the resolver runs before the cascade: it cannot
/// read the size. The icon still has to be as big as the text it stands in -
/// 16 px here, as the Material glyph it replaces under flat would be - not a
/// fixed size of its own (it was 24 px: every app icon sized this way, the
/// settings page's category list among them, grew by half under flora).
#[test]
fn an_hvif_icon_sized_by_a_stylesheet_takes_the_size_of_its_text() {
    let mut provider = create_default_icon_provider();
    assert!(register_hvif_icon(&mut provider, "app", "reply-arrow", &haiku("Mail_Reply"), IconMeta::for_image()));
    let shared = SharedIconProvider::from_handle(provider);
    let icon = Dom::create_icon("reply-arrow").with_css("display: block; font-size: 16px;");
    let mut dom = Dom::create_div().with_child(icon);
    resolve_icons_in_dom(&mut dom, &shared, &SystemStyle::default());
    let green = green_pixels(dom);
    assert!(green.len() > 20, "the green arrow is drawn: {} pixels", green.len());
    let max_x = green.iter().map(|p| p.0).max().unwrap_or(0);
    let max_y = green.iter().map(|p| p.1).max().unwrap_or(0);
    assert!(max_x <= 16 && max_x >= 10, "within its 16 px box: x up to {max_x}");
    assert!(max_y <= 16, "within its 16 px box: y up to {max_y}");
}
