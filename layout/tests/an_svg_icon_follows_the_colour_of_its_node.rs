//! An SVG icon painted in `currentColor` takes the `<icon>` node's own
//! cascaded `color`, like a font glyph; a palette remap swaps the paints it
//! lists (design 8.1, "SVG (user packs, the ricing case)").
//!
//! `register_svg_icon` is the no-Rust path a user icon pack takes: an SVG
//! file and its metadata, no custom resolver. The colour here comes from the
//! CONTAINER (inherited), which the resolver cannot see - it runs before the
//! cascade - so only the display list can put it on the ink.

#![cfg(all(feature = "cpurender", feature = "text_layout", feature = "font_loading"))]

use azul_core::{
    dom::Dom,
    icon::{
        resolve_icons_in_dom, IconColorMapping, IconColorMappingVec, IconMeta, IconRecolor,
        SharedIconProvider,
    },
};
use azul_css::{
    css::CssPropertyValue,
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::color::{ColorU, SystemColorRef},
        layout::LayoutDisplay,
        property::CssProperty,
        style::text::StyleTextColor,
    },
    system::{SystemStyle, Theme},
};
use azul_layout::{
    cpurender::{render_dom_to_image, AzulPixmap},
    icon::{create_default_icon_provider, default_svg_icon_meta, register_svg_icon},
};

const DOT: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect x="4" y="4" width="8" height="8" fill="currentColor"/></svg>"#;
const TWO_TONE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="8" height="16" fill="#000000"/><rect x="8" width="8" height="16" fill="#00ff00"/></svg>"##;

const BLUE: ColorU = ColorU {
    r: 20,
    g: 40,
    b: 230,
    a: 255,
};
const RED: ColorU = ColorU {
    r: 230,
    g: 30,
    b: 20,
    a: 255,
};
const GREEN: ColorU = ColorU {
    r: 0,
    g: 255,
    b: 0,
    a: 255,
};
const WHITE: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

/// `svg` as the only icon of a provider, resolved under `style` inside a
/// container whose `color` is `color` (inherited by the icon, never set on
/// it), rendered on the CPU over a white page.
fn render(svg: &[u8], meta: IconMeta, color: ColorU, style: &SystemStyle) -> AzulPixmap {
    let mut provider = create_default_icon_provider();
    assert!(register_svg_icon(&mut provider, "app", "icon", svg, meta));
    let shared = SharedIconProvider::from_handle(provider);

    let icon = Dom::create_icon("icon").with_css_props(
        vec![CssPropertyWithConditions::simple(CssProperty::display(
            LayoutDisplay::Block,
        ))]
        .into(),
    );
    let mut dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::TextColor(
                CssPropertyValue::Exact(StyleTextColor { inner: color }),
            ))]
            .into(),
        )
        .with_child(icon);
    resolve_icons_in_dom(&mut dom, &shared, style);

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
    let near = |a: u8, b: u8| (i32::from(a) - i32::from(b)).abs() <= 12;
    near(p[0], c.r) && near(p[1], c.g) && near(p[2], c.b)
}

#[test]
fn an_svg_icon_in_current_color_follows_the_colour_of_its_node() {
    let meta = default_svg_icon_meta(DOT);
    assert_eq!(meta.recolor, IconRecolor::CurrentColor);
    for color in [BLUE, RED] {
        let pm = render(DOT, meta.clone(), color, &SystemStyle::default());
        let ink = px(&pm, 8, 8);
        assert!(
            close_to(ink, color),
            "the ink must take the inherited `color` {color:?}, got {ink:?}"
        );
        let margin = px(&pm, 1, 1);
        assert!(
            close_to(margin, WHITE),
            "outside the ink the icon stays transparent, got {margin:?}"
        );
    }
}

#[test]
fn a_palette_remap_swaps_the_listed_paint_when_the_svg_is_drawn() {
    let meta = IconMeta::for_image().with_recolor(IconRecolor::Palette(
        IconColorMappingVec::from_vec(vec![IconColorMapping {
            from: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            to: BLUE,
        }]),
    ));
    let pm = render(TWO_TONE, meta, RED, &SystemStyle::default());
    let left = px(&pm, 4, 8);
    assert!(close_to(left, BLUE), "the listed black is drawn blue, got {left:?}");
    let right = px(&pm, 12, 8);
    assert!(close_to(right, GREEN), "the unlisted green is kept, got {right:?}");
}

#[test]
fn a_palette_remap_to_a_system_colour_follows_the_mode() {
    let meta = IconMeta::for_image().with_recolor(IconRecolor::Palette(
        IconColorMappingVec::from_vec(vec![IconColorMapping {
            from: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            to: SystemColorRef::Text.to_color_token(),
        }]),
    ));
    let mut dark = SystemStyle::default();
    dark.theme = Theme::Dark;
    let expected = SystemColorRef::Text.resolve_for_theme(&dark.colors, true);
    let pm = render(TWO_TONE, meta, RED, &dark);
    let left = px(&pm, 4, 8);
    assert!(
        close_to(left, expected),
        "`system:text` in dark mode is {expected:?}, got {left:?}"
    );
    assert!(
        !close_to(left, ColorU { r: 0, g: 0, b: 0, a: 255 }),
        "the token must be resolved, not drawn as the black it replaced"
    );
}
