//! A tint on a raster icon paints the GLYPH, not its box (ledger E15).
//!
//! The raster tint path pushed a bare `flood(tint)`. A flood REPLACES the
//! element with one solid colour, so a tinted icon came out as a filled
//! square the size of the icon - where it came out at all: `filter` did not
//! make a stacking context, so an icon that was not one by some other
//! property never had its filter painted and stayed untinted.
//!
//! The icon here is monochrome ink on alpha (`IconMeta::for_mask`), the one
//! kind of raster artwork a tint may recolour. The tint has to go through
//! the artwork's own alpha - `flood(tint) composite(in)` - which colours the
//! ink and leaves the transparent margin transparent.
//!
//! One PNG, two tints, pixels checked: headless, through the same resolver,
//! display list and CPU renderer a window uses.

#![cfg(all(
    feature = "cpurender",
    feature = "image_decoding",
    feature = "text_layout",
    feature = "font_loading"
))]

use azul_core::{
    dom::Dom,
    icon::{resolve_icons_in_dom, IconMeta, SharedIconProvider},
    resources::ImageRef,
};
use azul_css::{
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::color::{ColorU, OptionColorU},
        layout::LayoutDisplay,
        property::CssProperty,
    },
    system::SystemStyle,
};
use azul_layout::{
    cpurender::{render_dom_to_image, AzulPixmap},
    icon::{create_default_icon_provider, register_image_icon_with_meta},
};

/// The PNG's side, which is also the icon's natural size in logical px.
const SIDE: u32 = 16;
/// The ink: an opaque square from `INK_AT` to `INK_AT + INK_SIDE` on both axes.
const INK_AT: i32 = 4;
const INK_SIDE: i32 = 8;

const RED: ColorU = ColorU {
    r: 220,
    g: 20,
    b: 60,
    a: 255,
};
const BLUE: ColorU = ColorU {
    r: 30,
    g: 60,
    b: 220,
    a: 255,
};
const WHITE: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

/// Black ink on a transparent margin: an RGBA raster, what an icon pack's
/// decoded files are. Built from the pixels directly - the test is about the
/// tint, not the PNG codec (the `png` format feature is not a default one).
fn glyph_image() -> ImageRef {
    let mut pm = AzulPixmap::new(SIDE, SIDE).expect("pixmap");
    pm.fill(0, 0, 0, 0);
    pm.fill_rect(INK_AT, INK_AT, INK_SIDE, INK_SIDE, 0, 0, 0, 255);
    let raw = azul_core::resources::RawImage {
        pixels: azul_core::resources::RawImageData::U8(pm.data().to_vec().into()),
        width: SIDE as usize,
        height: SIDE as usize,
        premultiplied_alpha: false,
        data_format: azul_core::resources::RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    ImageRef::new_rawimage(raw).expect("an image from the glyph's pixels")
}

/// The icon, resolved under `tint`, inside an ordinary block container (so
/// nothing but its own `filter` could make it a stacking context), rendered
/// on the CPU over an opaque white page.
fn render_tinted(tint: ColorU) -> AzulPixmap {
    let mut provider = create_default_icon_provider();
    register_image_icon_with_meta(
        &mut provider,
        "app",
        "glyph",
        glyph_image(),
        IconMeta::for_mask(),
    );
    let shared = SharedIconProvider::from_handle(provider);

    let mut style = SystemStyle::default();
    style.icon_style.tint_color = OptionColorU::Some(tint);

    let icon = Dom::create_icon("glyph").with_css_props(
        vec![CssPropertyWithConditions::simple(CssProperty::display(
            LayoutDisplay::Block,
        ))]
        .into(),
    );
    let mut dom = Dom::create_div().with_child(icon);
    resolve_icons_in_dom(&mut dom, &shared, &style);

    let png = render_dom_to_image(dom, azul_css::css::Css::empty(), 32.0, 32.0, 1.0)
        .expect("render the icon");
    AzulPixmap::decode_png(&png).expect("decode the render")
}

fn px(pm: &AzulPixmap, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * pm.width + x) * 4) as usize;
    let d = pm.data();
    [d[i], d[i + 1], d[i + 2], d[i + 3]]
}

fn close_to(p: [u8; 4], c: ColorU) -> bool {
    let near = |a: u8, b: u8| (i32::from(a) - i32::from(b)).abs() <= 8;
    near(p[0], c.r) && near(p[1], c.g) && near(p[2], c.b)
}

#[test]
fn a_tinted_raster_icon_is_tinted_inside_its_own_alpha_and_nowhere_else() {
    for tint in [RED, BLUE] {
        let pm = render_tinted(tint);

        let ink = px(&pm, 8, 8);
        assert!(
            close_to(ink, tint),
            "tint {tint:?}: the ink must take the tint, got {ink:?}"
        );

        let margin = px(&pm, 1, 1);
        assert!(
            close_to(margin, WHITE),
            "tint {tint:?}: the icon's transparent margin must stay transparent, so the white \
             page shows through - got {margin:?}; a bare flood paints the whole box"
        );

        let outside = px(&pm, 24, 24);
        assert!(
            close_to(outside, WHITE),
            "tint {tint:?}: nothing may paint outside the icon, got {outside:?}"
        );
    }
}
