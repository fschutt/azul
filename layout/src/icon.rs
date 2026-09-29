//! Default icon resolver implementations for Azul
//!
//! This module provides the standard callback implementations for icon resolution.
//! The core types and resolution infrastructure are in `azul_core::icon`.
//!
//! # Usage
//!
//! ```rust,ignore
//! use azul_core::icon::IconProviderHandle;
//! use azul_layout::icon::{default_icon_resolver, ImageIconData, FontIconData};
//!
//! // Create provider with the default resolver
//! let provider = IconProviderHandle::with_resolver(default_icon_resolver);
//!
//! // Register an image icon (full-colour artwork: never recoloured)
//! provider.register_icon("app-images", "logo", RefAny::new(ImageIconData::with_meta(
//!     image_ref, 32.0, 32.0, IconMeta::for_image(),
//! )));
//!
//! // Register a font icon (follows the text colour)
//! provider.register_icon("material-icons", "home", RefAny::new(FontIconData::new(
//!     font_ref, "\u{e88a}",
//! )));
//! ```
//!
//! # Metadata: request x capability
//!
//! Every registered icon carries an [`IconMeta`]: the mode its artwork was
//! drawn for, its variants for the light / dark / high-contrast modes, and
//! HOW it may be recoloured ([`IconRecolor`]). The default resolver combines
//! that capability with the system's request (`IconStyleOptions`: tint,
//! grayscale, inherit the text colour) and never guesses from the kind:
//!
//! | recolor        | font glyph            | raster / SVG                              |
//! |----------------|-----------------------|-------------------------------------------|
//! | `CurrentColor` | `color` (tint = color)| monochrome: `flood(currentColor) composite(in)` |
//! | `Mask`         | `color`               | tint: `flood(tint) composite(in)`         |
//! | `Palette`      | as drawn              | listed paints swapped (SVG)               |
//! | `Fixed`        | `color` = the colour  | monochrome: `flood(colour) composite(in)` |
//! | `None`         | as drawn              | variants only                             |

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use azul_core::{
    dom::{Dom, NodeData, NodeType},
    icon::{IconMeta, IconProviderHandle, IconRecolor},
    refany::{OptionRefAny, RefAny},
    resources::ImageRef,
    styled_dom::StyledDom,
};
use azul_css::{
    css::{Css, CssPropertyValue},
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{
            color::{ColorU, OptionColorU, CURRENT_COLOR_TOKEN},
            length::FloatValue,
            FontRef, StyleFontFamily, StyleFontFamilyVec,
        },
        layout::{LayoutHeight, LayoutWidth},
        property::CssProperty,
        style::{
            filter::{StyleColorMatrix, StyleCompositeFilter, StyleFilter, StyleFilterVec},
            text::StyleTextColor,
        },
    },
    system::{SystemStyle, DarkLightMode},
};

// ============================================================================
// Icon Data Marker Structs (for RefAny::downcast)
// ============================================================================

/// Image-based icon data stored in `RefAny` for the icon resolver.
///
/// Pass to `register_image_icon` or wrap in `RefAny::new(...)` and register
/// directly via `IconProviderHandle::register_icon`.
#[derive(Debug)]
pub struct ImageIconData {
    pub image: ImageRef,
    /// The icon's NATURAL WIDTH in logical px - the size it is drawn at, not
    /// the width of the bitmap behind it.
    ///
    /// The two differ whenever the bitmap is oversampled: a 16px indicator
    /// rasterised from an SVG at 32x32 so it stays crisp on a 2x display is
    /// still a 16px indicator, and laying it out at 32 makes it twice the
    /// size the desktop draws it (this is exactly how a window-close button
    /// once came out as a giant X). [`register_image_icon`] takes the bitmap's
    /// own size, which is right for a 1:1 bitmap; an oversampled one must say
    /// its natural size with [`register_image_icon_sized`].
    pub width: f32,
    /// The icon's natural HEIGHT in logical px - see [`Self::width`].
    pub height: f32,
    /// How the artwork may be recoloured and which mode it was drawn for.
    /// [`IconMeta::for_image`] (never recoloured) unless the registration
    /// says otherwise: a full-colour bitmap gets `variants`, never a tint.
    pub meta: IconMeta,
}

impl ImageIconData {
    /// An image icon with explicit metadata; `width` / `height` are its
    /// natural size in logical px (see [`Self::width`]).
    #[must_use]
    pub const fn with_meta(image: ImageRef, width: f32, height: f32, meta: IconMeta) -> Self {
        Self {
            image,
            width,
            height,
            meta,
        }
    }
}

/// Font-based icon data stored in `RefAny` for the icon resolver.
///
/// Pass to `register_font_icon` or wrap in `RefAny::new(...)` and register
/// directly via `IconProviderHandle::register_icon`.
#[derive(Debug)]
pub struct FontIconData {
    pub font: FontRef,
    /// The character/codepoint for this specific icon (e.g., "\u{e88a}" for home)
    pub icon_char: String,
    /// [`IconMeta::for_font`] by default: the glyph IS the text colour.
    pub meta: IconMeta,
}

impl FontIconData {
    /// A glyph icon with the font default metadata ([`IconMeta::for_font`]).
    #[must_use]
    pub fn new(font: FontRef, icon_char: impl Into<String>) -> Self {
        Self {
            font,
            icon_char: icon_char.into(),
            meta: IconMeta::for_font(),
        }
    }

    /// This icon with other metadata (a dark variant, a fixed colour).
    #[must_use]
    pub fn with_meta(mut self, meta: IconMeta) -> Self {
        self.meta = meta;
        self
    }
}

/// An SVG icon: the document itself, drawn when the icon is resolved.
///
/// Register with [`register_svg_icon`]; the default resolver needs nothing
/// else - no custom resolver, no Rust - which is what makes an SVG file a
/// usable user icon. `currentColor` in the document follows the `<icon>`
/// node's cascaded `color` (a monochrome document exactly, through its alpha;
/// see [`IconMeta::is_mask_artwork`]), and an [`IconRecolor::Palette`] swaps
/// the paints it lists as the document is drawn.
#[derive(Debug, Clone)]
pub struct SvgIconData {
    /// The document, UTF-8 XML.
    pub svg: Vec<u8>,
    /// The icon's natural size in logical px (the document's `width` /
    /// `height`, else its viewBox).
    pub width: f32,
    pub height: f32,
    pub meta: IconMeta,
}

/// The largest SVG document [`register_svg_icon`] accepts. Icons are small;
/// a user theme is untrusted input (design 9.1 pitfall 8), so an oversized
/// file is refused rather than parsed.
pub const MAX_SVG_ICON_BYTES: usize = 1 << 20;

/// The natural size an SVG that states none is drawn at.
const DEFAULT_SVG_ICON_SIZE: f32 = 24.0;

/// How many device pixels per logical pixel an SVG icon is rasterised at,
/// so it stays crisp on a 2x display.
#[cfg_attr(not(feature = "cpurender"), allow(dead_code))] // only the rasteriser reads it
const SVG_ICON_OVERSAMPLE: f32 = 2.0;

/// The metadata an SVG document implies when its registration states none:
/// a document that paints in `currentColor` follows the text colour
/// ([`IconRecolor::CurrentColor`]), and is `monochrome` when EVERY paint is
/// `currentColor` or `none`; anything else is full-colour artwork, never
/// recoloured ([`IconMeta::for_image`]).
///
/// This reads the author's own statement (`currentColor`) - it does not
/// guess from the kind of icon.
#[must_use]
pub fn default_svg_icon_meta(svg: &[u8]) -> IconMeta {
    let mentions_current_color = svg
        .windows(b"currentcolor".len())
        .any(|w| w.eq_ignore_ascii_case(b"currentcolor"));
    if !mentions_current_color {
        return IconMeta::for_image();
    }
    #[cfg(feature = "cpurender")]
    let monochrome = crate::cpurender::svg_uses_only_current_color(svg);
    #[cfg(not(feature = "cpurender"))]
    let monochrome = false;
    IconMeta::for_image()
        .with_recolor(IconRecolor::CurrentColor)
        .with_monochrome(monochrome)
}

/// Register an SVG document as an icon, with its metadata (see
/// [`default_svg_icon_meta`] for what a plain `currentColor` icon wants).
///
/// Returns `false`, registering nothing, when `svg` is larger than
/// [`MAX_SVG_ICON_BYTES`] or is not an SVG document.
pub fn register_svg_icon(
    provider: &mut IconProviderHandle,
    pack_name: &str,
    icon_name: &str,
    svg: &[u8],
    meta: IconMeta,
) -> bool {
    let Some(data) = svg_icon_data(svg, meta) else {
        return false;
    };
    provider.register_icon(pack_name, icon_name, RefAny::new(data));
    true
}

/// The registered value for an SVG document, or `None` for input that is
/// too large or not an SVG.
fn svg_icon_data(svg: &[u8], meta: IconMeta) -> Option<SvgIconData> {
    if svg.len() > MAX_SVG_ICON_BYTES || !is_svg_document(svg) {
        return None;
    }
    #[cfg(feature = "cpurender")]
    let size = crate::cpurender::svg_natural_size(svg);
    #[cfg(not(feature = "cpurender"))]
    let size: Option<(f32, f32)> = None;
    let (width, height) = size.unwrap_or((DEFAULT_SVG_ICON_SIZE, DEFAULT_SVG_ICON_SIZE));
    Some(SvgIconData {
        svg: svg.to_vec(),
        width,
        height,
        meta,
    })
}

/// Does `svg` parse as XML with an `<svg>` root?
fn is_svg_document(svg: &[u8]) -> bool {
    #[cfg(feature = "xml")]
    {
        let Ok(text) = core::str::from_utf8(svg) else {
            return false;
        };
        let Ok(nodes) = crate::xml::parse_xml_string(text) else {
            return false;
        };
        nodes.iter().any(|n| {
            matches!(n, azul_core::xml::XmlNodeChild::Element(e)
                if e.node_type.as_str().eq_ignore_ascii_case("svg"))
        })
    }
    #[cfg(not(feature = "xml"))]
    {
        let _ = svg;
        false
    }
}

/// An icon that IS a `Dom` - the general case, of which image and font icons
/// are special cases.
///
/// Register any DOM as an icon and refer to it by name everywhere an icon spec
/// is taken (`<icon>my-logo</icon>`, a tray icon, an app icon). Because the DOM
/// carries its own inline styles AND its own stylesheets (`Dom::css`), the icon
/// decides what it looks like - colour included - instead of every call site
/// having to thread a tint parameter down to the renderer.
///
/// The whole subtree is spliced in before the cascade, so a DOM icon can be any
/// number of nodes with any styling, not just a single glyph.
#[derive(Debug, Clone)]
pub struct DomIconData {
    pub dom: Dom,
}

impl DomIconData {
    #[must_use]
    pub const fn new(dom: Dom) -> Self {
        Self { dom }
    }
}

// ============================================================================
// Default Icon Resolver
// ============================================================================

/// Default icon resolver that handles both image and font icons.
///
/// Resolution logic:
/// 1. If `icon_data` is None -> return empty div (icon not found)
/// 2. If `icon_data` contains `ImageIconData` -> render as image
/// 3. If `icon_data` contains `FontIconData` -> render as text with font
/// 4. Unknown data type -> return empty div
///
/// Styles from the original icon DOM are copied to the result,
/// filtered based on `SystemStyle` preferences.
#[must_use]
pub extern "C" fn default_icon_resolver(
    icon_data: OptionRefAny,
    original_icon_node: &NodeData,
    system_style: &SystemStyle,
) -> Dom {
    // No icon found -> empty div
    let Some(mut data) = icon_data.into_option() else {
        return Dom::create_div();
    };

    // A registered Dom: the icon IS a DOM, spliced in whole. This is what lets
    // an icon carry its own styling - colour above all - instead of every call
    // site having to pass a tint down. Checked FIRST because it is the most
    // specific: a caller who registered a whole DOM meant that DOM.
    if let Some(dom_icon) = data.downcast_ref::<DomIconData>() {
        return create_dom_icon_from_original(&dom_icon, original_icon_node);
    }

    // Try ImageIconData
    if let Some(img) = data.downcast_ref::<ImageIconData>() {
        if let Some(variant) = variant_redirect(&img.meta, original_icon_node, system_style) {
            return variant;
        }
        return create_image_icon_from_original(&img, original_icon_node, system_style);
    }

    // Try FontIconData
    if let Some(font_icon) = data.downcast_ref::<FontIconData>() {
        if let Some(variant) = variant_redirect(&font_icon.meta, original_icon_node, system_style)
        {
            return variant;
        }
        return create_font_icon_from_original(&font_icon, original_icon_node, system_style);
    }

    // Try SvgIconData
    if let Some(svg) = data.downcast_ref::<SvgIconData>() {
        if let Some(variant) = variant_redirect(&svg.meta, original_icon_node, system_style) {
            return variant;
        }
        return create_svg_icon_from_original(&svg, original_icon_node, system_style);
    }

    // Unknown data type -> empty div
    Dom::create_div()
}

/// Is the style the resolver was handed in the dark mode? The core
/// resolution entry point hands the resolver the WINDOW's mode here, not the
/// desktop's (`azul_core::icon::resolve_icons_in_dom_with_context`).
///
/// Reads `SystemStyle::theme`, the light / dark slot (renamed to a mode by
/// the naming migration; this is the one place to follow it).
fn is_dark(style: &SystemStyle) -> bool {
    style.theme == DarkLightMode::Dark
}

/// The `<icon>` redirected to the artwork for the current mode, when the
/// metadata names one ([`azul_core::icon::IconVariants::pick`]).
///
/// The redirect is an ICON NODE: the core resolution loop resolves an icon
/// that resolves to another icon again, so the variant can be any registered
/// icon and gets its own metadata applied. The call site's inline styles and
/// accessibility travel with it. A variant naming the icon itself is not
/// followed (the loop would stop there anyway, leaving nothing drawn).
fn variant_redirect(meta: &IconMeta, original: &NodeData, style: &SystemStyle) -> Option<Dom> {
    let spec = meta
        .variants
        .pick(is_dark(style), bool::from(style.prefers_high_contrast))?;
    if let NodeType::Icon(name) = original.get_node_type() {
        if name.as_str().trim().eq_ignore_ascii_case(spec.as_str().trim()) {
            return None;
        }
    }
    let mut dom = Dom::create_icon(spec.clone());
    let props = copy_appropriate_styles_vec(original);
    if !props.is_empty() {
        dom.root
            .set_css_props(CssPropertyWithConditionsVec::from_vec(props));
    }
    if let Some(a11y) = original.get_accessibility_info() {
        dom = dom.with_accessibility_info(a11y.clone());
    }
    Some(dom)
}

/// What the resolver paints an icon's ink with: the request (the system's
/// `IconStyleOptions`) combined with the capability (the icon's
/// [`IconRecolor`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ink {
    /// The artwork's own colours.
    Native,
    /// The `<icon>` node's cascaded `color`.
    CurrentColor,
    /// One colour: concrete, or a `system:` token the cascade resolves.
    Color(ColorU),
}

/// Request x capability (design 8.2): a tint on `CurrentColor` becomes the
/// colour, on `Mask` it is flooded, on `Palette` / `None` it is ignored; an
/// explicit `Fixed` colour beats the tint and the CSS `color` alike (9.1
/// pitfall 10). `Mask` artwork drawn for the other mode, or asked to follow
/// the text colour (`inherit_text_color`), takes the text colour.
fn ink_for(meta: &IconMeta, style: &SystemStyle) -> Ink {
    let dark = is_dark(style);
    let tint = match style.icon_style.tint_color {
        OptionColorU::Some(c) => Some(c),
        OptionColorU::None => None,
    };
    match &meta.recolor {
        IconRecolor::Fixed(colors) => Ink::Color(colors.for_mode(dark)),
        IconRecolor::CurrentColor => tint.map_or(Ink::CurrentColor, Ink::Color),
        IconRecolor::Mask => match tint {
            Some(c) => Ink::Color(c),
            None if style.icon_style.inherit_text_color || !meta.designed_for.suits(dark) => {
                Ink::CurrentColor
            }
            None => Ink::Native,
        },
        IconRecolor::Palette(_) | IconRecolor::None => Ink::Native,
    }
}

/// An icon that IS a `Dom`: whatever the caller registered, spliced in whole.
///
/// The call site's styles are applied FIRST and the registered DOM's own styles
/// second, so the icon wins on anything it specifies (colour, background,
/// borders) while still inheriting what it does not (the size the tray or the
/// surrounding text asked for). That ordering is the point: an icon knows what
/// it should look like, a call site knows how big it should be.
fn create_dom_icon_from_original(icon: &DomIconData, original: &NodeData) -> Dom {
    let mut dom = icon.dom.clone();

    let mut props = copy_appropriate_styles_vec(original);
    if props.is_empty() {
        return dom;
    }
    // The registered DOM's own declarations go last so they override.
    props.extend(copy_appropriate_styles_vec(&dom.root));
    dom.root
        .set_css_props(CssPropertyWithConditionsVec::from_vec(props));
    dom
}

// Icon DOM Creation (from original)

/// Create a `StyledDom` for an image-based icon, copying styles from original.
///
/// Applies SystemStyle-aware modifications:
/// - Grayscale filter if `prefer_grayscale` is true
/// - Tint color overlay if `tint_color` is set
fn create_image_icon_from_original(
    img: &ImageIconData,
    original: &NodeData,
    system_style: &SystemStyle,
) -> Dom {
    let mut dom = Dom::create_image(img.image.clone());

    // Copy appropriate styles from original
    {
        let original_node = original;
        let mut props_vec = copy_appropriate_styles_vec(original_node);

        // Add default dimensions if not specified in original styles
        let has_width = props_vec
            .iter()
            .any(|p| matches!(&p.property, CssProperty::Width(_)));
        let has_height = props_vec
            .iter()
            .any(|p| matches!(&p.property, CssProperty::Height(_)));

        if !has_width {
            props_vec.push(CssPropertyWithConditions::simple(CssProperty::width(
                LayoutWidth::px(img.width),
            )));
        }
        if !has_height {
            props_vec.push(CssPropertyWithConditions::simple(CssProperty::height(
                LayoutHeight::px(img.height),
            )));
        }

        // Apply SystemStyle-aware filters
        apply_icon_style_filters(&mut props_vec, &img.meta, system_style);

        dom.root
            .set_css_props(CssPropertyWithConditionsVec::from_vec(props_vec));

        // Copy accessibility info
        if let Some(a11y) = original_node.get_accessibility_info() {
            dom = dom.with_accessibility_info(a11y.clone());
        }
    }

    dom
}

/// Create a `StyledDom` for a font-based icon, copying styles from original.
///
/// Applies SystemStyle-aware modifications:
/// - Text color override if `inherit_text_color` is true
/// - Tint color if `tint_color` is set
fn create_font_icon_from_original(
    font_icon: &FontIconData,
    original: &NodeData,
    system_style: &SystemStyle,
) -> Dom {
    // A SPAN, not a bare text node and not a <p>.
    //
    // The glyph carries css (font-family, colour, tint) and frequently sits as
    // a direct item of a flex or inline-flex container — the engine warned about
    // exactly that: "text node ... is one of 2 items in a InlineFlex container".
    // A bare text run has no box, so the css is INERT and the glyph competes as
    // a flex item with nothing to size. A <p> would give it a box and get the
    // flow wrong: an icon belongs INLINE inside its label, not as a block
    // paragraph beside it. `span` is the one that is both boxed and inline.
    let mut dom = Dom::create_span_with_text(font_icon.icon_char.clone());

    // Add font family
    let font_prop = CssPropertyWithConditions::simple(CssProperty::font_family(
        StyleFontFamilyVec::from_vec(vec![StyleFontFamily::Ref(font_icon.font.clone())]),
    ));

    {
        let original_node = original;
        let mut props_vec = copy_appropriate_styles_vec(original_node);
        props_vec.push(font_prop);

        // Apply SystemStyle-aware color modifications for font icons
        apply_font_icon_color(&mut props_vec, &font_icon.meta, system_style);

        dom.root
            .set_css_props(CssPropertyWithConditionsVec::from_vec(props_vec));

        // Copy accessibility info
        if let Some(a11y) = original_node.get_accessibility_info() {
            dom = dom.with_accessibility_info(a11y.clone());
        }
    }

    dom
}

/// Draw an SVG icon: the document is rasterised with the paints the metadata
/// asks for, then placed exactly like an image icon (natural size, the call
/// site's styles, the ink as `flood() composite(in)` where it applies).
///
/// `currentColor` in the document:
/// - monochrome artwork with an ink ([`Ink::Color`] / [`Ink::CurrentColor`]) is drawn in opaque
///   black - a pure alpha mask - and the ink is flooded through it by the image filters, so it
///   follows the node's CASCADED `color` exactly (the resolver runs before the cascade, so only
///   the display list knows that colour);
/// - otherwise it is the explicit ink, else the `<icon>`'s own inline `color`, else the mode's
///   `system:text`.
///
/// A [`IconRecolor::Palette`] swaps its paints as the document is drawn,
/// `system:` targets resolved for the mode the icon is drawn in.
#[cfg(feature = "cpurender")]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded icon pixel size
fn create_svg_icon_from_original(
    svg: &SvgIconData,
    original: &NodeData,
    system_style: &SystemStyle,
) -> Dom {
    use azul_css::props::basic::color::SystemColorRef;

    let dark = is_dark(system_style);
    let resolve = |c: ColorU| {
        SystemColorRef::from_color_token(c)
            .map_or(c, |r| r.resolve_for_theme(&system_style.colors, dark))
    };
    let ink = ink_for(&svg.meta, system_style);
    let flooded = svg.meta.is_mask_artwork() && ink != Ink::Native;
    let current_color = if flooded {
        ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    } else {
        match ink {
            Ink::Color(c) => resolve(c),
            Ink::CurrentColor | Ink::Native => resolve(
                inline_text_color(original).unwrap_or_else(|| SystemColorRef::Text.to_color_token()),
            ),
        }
    };
    let palette = match &svg.meta.recolor {
        IconRecolor::Palette(map) => map
            .as_ref()
            .iter()
            .map(|m| (m.from, resolve(m.to)))
            .collect(),
        _ => Vec::new(),
    };
    let paint = crate::cpurender::SvgPaintContext {
        current_color: Some(current_color),
        palette,
    };
    let device = |logical: f32| ((logical * SVG_ICON_OVERSAMPLE).ceil().max(1.0)) as u32;
    let Ok(image) = crate::cpurender::render_svg_to_imageref_painted(
        &svg.svg,
        device(svg.width),
        device(svg.height),
        &paint,
    ) else {
        return Dom::create_div();
    };
    let as_image = ImageIconData::with_meta(image, svg.width, svg.height, svg.meta.clone());
    create_image_icon_from_original(&as_image, original, system_style)
}

/// Without the rasteriser there is nothing to draw an SVG icon with.
#[cfg(not(feature = "cpurender"))]
fn create_svg_icon_from_original(
    _svg: &SvgIconData,
    _original: &NodeData,
    _system_style: &SystemStyle,
) -> Dom {
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][icons] an SVG icon was resolved, but this build has no `cpurender` feature - \
             SVG icons render as nothing"
        );
    });
    Dom::create_div()
}

/// The last `color:` the call site set on the `<icon>` itself, if any.
#[cfg(feature = "cpurender")]
fn inline_text_color(original: &NodeData) -> Option<ColorU> {
    copy_appropriate_styles_vec(original)
        .iter()
        .rev()
        .find_map(|p| match &p.property {
            CssProperty::TextColor(CssPropertyValue::Exact(c)) => Some(c.inner),
            _ => None,
        })
}

/// Copy styles from original node
/// Returns a Vec for easier manipulation
fn copy_appropriate_styles_vec(original_node: &NodeData) -> Vec<CssPropertyWithConditions> {
    // Reconstruct the legacy flat list from the unified Css store.
    original_node
        .get_style()
        .iter_inline_properties()
        .map(|(prop, conds)| CssPropertyWithConditions {
            property: prop.clone(),
            apply_if: conds.clone(),
        })
        .collect()
}

/// Apply SystemStyle-aware filters to image-like icon properties.
///
/// - a grayscale colour matrix if `prefer_grayscale` is asked for;
/// - the ink ([`ink_for`]) as `flood(c) composite(in)` when the artwork is an
///   alpha mask ([`IconMeta::is_mask_artwork`]). The composite is the point:
///   a bare `flood()` REPLACES the element with a solid colour, so the tint
///   used to paint a filled square the size of the icon (ledger E15);
///   `composite(in)` keeps the flood only where the artwork's own alpha is.
///   Full-colour artwork is never flooded - it gets `variants` instead.
fn apply_icon_style_filters(
    props_vec: &mut Vec<CssPropertyWithConditions>,
    meta: &IconMeta,
    system_style: &SystemStyle,
) {
    let filters = icon_filters(meta, system_style);
    if !filters.is_empty() {
        props_vec.push(CssPropertyWithConditions::simple(CssProperty::Filter(
            CssPropertyValue::Exact(StyleFilterVec::from_vec(filters)),
        )));
    }
}

/// The filters [`apply_icon_style_filters`] adds, in order: grayscale first,
/// then the ink - all in ONE `filter:` list (a second declaration would
/// replace the first in the cascade).
fn icon_filters(meta: &IconMeta, system_style: &SystemStyle) -> Vec<StyleFilter> {
    let mut filters = Vec::new();
    if system_style.icon_style.prefer_grayscale {
        filters.push(StyleFilter::ColorMatrix(grayscale_matrix()));
    }
    if meta.is_mask_artwork() {
        let flood = match ink_for(meta, system_style) {
            Ink::Color(c) => Some(c),
            // The display list swaps the token for the node's own `color`.
            Ink::CurrentColor => Some(CURRENT_COLOR_TOKEN),
            Ink::Native => None,
        };
        if let Some(color) = flood {
            filters.push(StyleFilter::Flood(color));
            filters.push(StyleFilter::Composite(StyleCompositeFilter::In));
        }
    }
    filters
}

/// Rec. 709 luminance as a colour matrix, alpha passed through.
///
/// Row-major 4x5 (SVG `feColorMatrix` order), offsets in the fifth column:
/// ```text
/// [0.2126, 0.7152, 0.0722, 0, 0]  <- R output
/// [0.2126, 0.7152, 0.0722, 0, 0]  <- G output
/// [0.2126, 0.7152, 0.0722, 0, 0]  <- B output
/// [0,      0,      0,      1, 0]  <- A output
/// ```
fn grayscale_matrix() -> StyleColorMatrix {
    StyleColorMatrix {
        m0: FloatValue::new(0.2126),
        m1: FloatValue::new(0.7152),
        m2: FloatValue::new(0.0722),
        m3: FloatValue::new(0.0),
        m4: FloatValue::new(0.0),
        m5: FloatValue::new(0.2126),
        m6: FloatValue::new(0.7152),
        m7: FloatValue::new(0.0722),
        m8: FloatValue::new(0.0),
        m9: FloatValue::new(0.0),
        m10: FloatValue::new(0.2126),
        m11: FloatValue::new(0.7152),
        m12: FloatValue::new(0.0722),
        m13: FloatValue::new(0.0),
        m14: FloatValue::new(0.0),
        m15: FloatValue::new(0.0),
        m16: FloatValue::new(0.0),
        m17: FloatValue::new(0.0),
        m18: FloatValue::new(1.0),
        m19: FloatValue::new(0.0),
    }
}

/// Apply SystemStyle-aware color modifications for font icons.
///
/// A glyph is painted in the text colour, so the ink ([`ink_for`]) is simply
/// a `color:` pushed AFTER the call site's own declarations - which is what
/// makes an explicit recolour (and a tint) beat an inline `color`. Following
/// the CSS colour needs nothing: `color` is inherited, and pushing one would
/// break that.
fn apply_font_icon_color(
    props_vec: &mut Vec<CssPropertyWithConditions>,
    meta: &IconMeta,
    system_style: &SystemStyle,
) {
    if let Ink::Color(color) = ink_for(meta, system_style) {
        props_vec.push(CssPropertyWithConditions::simple(CssProperty::TextColor(
            CssPropertyValue::Exact(StyleTextColor { inner: color }),
        )));
    }
}

// IconProviderHandle Helper Functions

/// Register an image icon in a pack
pub fn register_image_icon(
    provider: &mut IconProviderHandle,
    pack_name: &str,
    icon_name: &str,
    image: ImageRef,
) {
    // The bitmap's own size IS the natural size for a 1:1 bitmap. An
    // OVERSAMPLED one (an SVG rasterised at 2x for a HiDPI display) must use
    // `register_image_icon_sized` instead, or it lays out at twice the size
    // it is drawn at.
    let size = image.get_size();
    register_image_icon_sized(
        provider,
        pack_name,
        icon_name,
        image,
        size.width,
        size.height,
    );
}

/// Register an image icon whose NATURAL SIZE differs from its bitmap's.
///
/// `width`/`height` are the logical px the icon occupies in layout; the
/// bitmap may be larger (an SVG rasterised at 2x or 3x so it stays crisp when
/// the display or the zoom scales it up). This is the difference between
/// "how big is this picture" and "how big is this icon", and only the caller
/// that rasterised it knows the second: a freedesktop theme states it in the
/// directory the file came from (`actions/16`, `actions/22`, ...), and a
/// `scalable` icon has whatever size its consumer intends.
///
/// A non-finite or non-positive size is IGNORED in favour of the bitmap's own
/// - a zero-sized icon is invisible, which is a worse failure than a
/// wrong-sized one, and NaN would poison layout.
pub fn register_image_icon_sized(
    provider: &mut IconProviderHandle,
    pack_name: &str,
    icon_name: &str,
    image: ImageRef,
    width: f32,
    height: f32,
) {
    let bitmap = image.get_size();
    let sane = |asked: f32, fallback: f32| {
        if asked.is_finite() && asked > 0.0 {
            asked
        } else {
            fallback
        }
    };
    let data = ImageIconData {
        width: sane(width, bitmap.width),
        height: sane(height, bitmap.height),
        image,
        meta: IconMeta::for_image(),
    };
    provider.register_icon(pack_name, icon_name, RefAny::new(data));
}

/// Register an image icon with metadata: how it may be recoloured
/// ([`IconMeta::for_mask`] for monochrome ink on alpha, which a tint floods
/// through its own alpha), the mode it was drawn for and its variants for
/// other modes. The bitmap's own size is the natural size, as in
/// [`register_image_icon`].
pub fn register_image_icon_with_meta(
    provider: &mut IconProviderHandle,
    pack_name: &str,
    icon_name: &str,
    image: ImageRef,
    meta: IconMeta,
) {
    let size = image.get_size();
    let data = ImageIconData::with_meta(image, size.width, size.height, meta);
    provider.register_icon(pack_name, icon_name, RefAny::new(data));
}

/// Register icons from a ZIP file (file names become icon names)
#[cfg(feature = "zip")]
pub fn register_icons_from_zip(
    provider: &mut IconProviderHandle,
    pack_name: &str,
    zip_bytes: &[u8],
) {
    for (icon_name, image, width, height) in load_images_from_zip(zip_bytes) {
        let data = ImageIconData::with_meta(image, width, height, IconMeta::for_image());
        provider.register_icon(pack_name, &icon_name, RefAny::new(data));
    }
}

#[cfg(not(feature = "zip"))]
pub fn register_icons_from_zip(
    _provider: &mut IconProviderHandle,
    _pack_name: &str,
    _zip_bytes: &[u8],
) {
    // ZIP support not enabled — the caller explicitly handed us an icon pack
    // and NOTHING got registered; every later lookup will just miss. Say so
    // once (this gate is hit by DEFAULT builds: `zip` is not a default
    // feature of azul-layout).
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][icons] register_icons_from_zip called, but this build has no `zip` feature — \
             NO icons were registered from the pack. Rebuild azul-layout with the `zip` (+ \
             `image_decoding`) features"
        );
    });
}

/// Register a font icon in a pack
/// Register an arbitrary `Dom` as an icon.
///
/// The general case: an icon is just a DOM, and image / font icons are special
/// cases of it. Whatever you register is spliced in wherever that icon name is
/// used - an `<icon>` node, a tray icon, an app icon - and it keeps its own
/// styling, because the DOM carries both inline properties and its own
/// stylesheets (`Dom::css`).
///
/// That is what makes colour work without a tint parameter: an icon that should
/// be red says so itself, once, instead of every call site having to thread a
/// colour down to the renderer. Sizing still comes from the call site, since the
/// caller is the one who knows how big the icon has to be.
pub fn register_dom_icon(
    provider: &mut IconProviderHandle,
    pack_name: &str,
    icon_name: &str,
    dom: Dom,
) {
    provider.register_icon(pack_name, icon_name, RefAny::new(DomIconData::new(dom)));
}

pub fn register_font_icon(
    provider: &mut IconProviderHandle,
    pack_name: &str,
    icon_name: &str,
    font: FontRef,
    icon_char: &str,
) {
    let data = FontIconData::new(font, icon_char);
    provider.register_icon(pack_name, icon_name, RefAny::new(data));
}

// ============================================================================
// ZIP Support
// ============================================================================

/// Load all images from a ZIP file, returning (`icon_name`, `ImageRef`, width, height)
#[cfg(all(feature = "zip", feature = "image_decoding"))]
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/counter/fixed-point cast
fn load_images_from_zip(zip_bytes: &[u8]) -> Vec<(String, ImageRef, f32, f32)> {
    use std::path::Path;

    use crate::{
        image::decode::{decode_raw_image_from_any_bytes, ResultRawImageDecodeImageError},
        zip::{ZipFile, ZipReadConfig},
    };

    let mut result = Vec::new();
    let config = ZipReadConfig::default();
    let Ok(entries) = ZipFile::list(zip_bytes, &config) else {
        return result;
    };

    for entry in &entries {
        if entry.path.ends_with('/') {
            continue;
        } // Skip directories

        let Ok(Some(file_bytes)) = ZipFile::get_single_file(zip_bytes, entry, &config) else {
            continue;
        };

        // Decode as image
        if let ResultRawImageDecodeImageError::Ok(raw_image) =
            decode_raw_image_from_any_bytes(&file_bytes)
        {
            // Icon name = filename without extension
            let path = Path::new(&entry.path);
            let icon_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();

            let width = raw_image.width as f32;
            let height = raw_image.height as f32;

            if let Some(image) = ImageRef::new_rawimage(raw_image) {
                result.push((icon_name, image, width, height));
            }
        }
    }

    result
}

#[cfg(not(all(feature = "zip", feature = "image_decoding")))]
fn load_images_from_zip(_zip_bytes: &[u8]) -> Vec<(String, ImageRef, f32, f32)> {
    // Only reachable when `zip` is on but `image_decoding` is off (the
    // zip-off case never calls this) — same silent-empty trap, so say so.
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][icons] icon ZIP was readable but this build has no `image_decoding` feature — \
             0 images decoded, NO icons registered"
        );
    });
    Vec::new()
}

// ============================================================================
// Material Icons Registration
// ============================================================================

/// Register all Material Icons in the provider.
///
/// This registers all 2234 Material Icons from the `material-icons` crate.
/// Each icon is registered under the "material-icons" pack with its HTML name
/// (e.g., "home", "settings", "`arrow_back`", etc.).
///
/// Requires the "icons" feature with material-icons crate.
#[cfg(feature = "icons")]
pub fn register_material_icons(provider: &mut IconProviderHandle, font: &FontRef) {
    use material_icons::{icon_to_char, icon_to_html_name, ALL_ICONS};

    // Register all Material Icons with their Unicode codepoints
    for icon in &ALL_ICONS {
        let icon_char = icon_to_char(*icon);
        let name = icon_to_html_name(icon);

        let data = FontIconData::new(font.clone(), icon_char.to_string());
        provider.register_icon("material-icons", name, RefAny::new(data));
    }
}

#[cfg(not(feature = "icons"))]
pub fn register_material_icons(_provider: &mut IconProviderHandle, _font: FontRef) {
    // Icons feature not enabled — the caller asked for 2234 Material Icons
    // and got zero, silently. Say so once.
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][icons] register_material_icons called, but this build has no `icons` feature \
             — NO Material Icons were registered"
        );
    });
}

/// Load the embedded Material Icons font and register all standard icons.
///
/// This uses the `material-icons` crate which embeds the Material Icons TTF font.
/// The font is Apache 2.0 licensed by Google.
///
/// Returns true if registration was successful.
/// Register all Material Icons from caller-supplied TTF bytes.
///
/// The font bytes are NOT embedded here. `azul-doc codegen all` generates
/// `target/codegen/material_icons.ttf.br`, and `azul-doc` builds (depends
/// on) `azul-layout` — so `include!`ing that generated artifact in this
/// crate is a build cycle (it bit us on `cargo clean`). The `include!` +
/// brotli-decompression live in `azul-dll` (downstream of codegen), which
/// passes the decompressed TTF in here.
#[cfg(all(feature = "icons", feature = "text_layout"))]
pub fn register_embedded_material_icons(
    provider: &mut IconProviderHandle,
    font_bytes: &[u8],
) -> bool {
    use crate::{font::parsed::ParsedFont, parsed_font_to_font_ref};

    let mut warnings = Vec::new();
    let Some(parsed_font) = ParsedFont::from_bytes(font_bytes, 0, &mut warnings) else {
        return false;
    };

    let font_ref = parsed_font_to_font_ref(parsed_font);
    register_material_icons(provider, &font_ref);

    true
}

#[cfg(not(all(feature = "icons", feature = "text_layout")))]
pub fn register_embedded_material_icons(
    _provider: &mut IconProviderHandle,
    _font_bytes: &[u8],
) -> bool {
    // Icons or text_layout feature not enabled. Returning false is a weak
    // signal callers routinely ignore — name the gate once.
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][icons] register_embedded_material_icons called, but this build lacks the \
             `icons` and/or `text_layout` feature — NO icons registered (returning false)"
        );
    });
    false
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Create an `IconProviderHandle` with the default resolver.
pub fn create_default_icon_provider() -> IconProviderHandle {
    IconProviderHandle::with_resolver(default_icon_resolver)
}

// The embedded Material Icons font bytes (the `include!` of the
// codegen-generated `target/codegen/material_icons.ttf.br` + brotli
// decompression) deliberately live in `azul-dll`, not here — see
// `register_embedded_material_icons` above for why (build-cycle: azul-doc
// builds azul-layout to generate that artifact).

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_resolver_no_data() {
        let style = SystemStyle::default();
        let original = Dom::create_div().root;

        let result = default_icon_resolver(OptionRefAny::None, &original, &style);

        // Without data, should return empty div StyledDom
        assert_eq!(result.children.as_ref().len(), 0);
    }

    #[test]
    fn test_create_default_provider() {
        let provider = create_default_icon_provider();
        assert!(provider.list_packs().is_empty());
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::items_after_statements,
    clippy::redundant_clone,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names,
    unused_qualifications,
    unreachable_pub,
    private_interfaces
)] // pedantic lints are noise in adversarial test code
mod autotest_generated {
    use azul_core::{a11y::SmallAriaInfo, dom::NodeType, resources::RawImageFormat};
    use azul_css::props::basic::color::{ColorU, OptionColorU};

    use super::*;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// A `FontRef` whose `parsed` pointer addresses a `'static` byte and whose
    /// destructor is a no-op, so nothing is freed on drop. Sound here because
    /// nothing on the icon-resolution path ever dereferences `parsed` (only
    /// `cpurender::raster` does, and that is not reached from `StyledDom::create`).
    fn dummy_font_ref() -> FontRef {
        static DUMMY_FONT_DATA: u8 = 0;
        extern "C" fn dummy_destructor(_: *mut core::ffi::c_void) {}
        FontRef::new(
            core::ptr::addr_of!(DUMMY_FONT_DATA).cast::<core::ffi::c_void>(),
            dummy_destructor,
        )
    }

    /// A null (non-decoded) `ImageRef` of the given pixel size — `get_size()`
    /// reports exactly `width` / `height`, with no allocation.
    fn null_img(width: usize, height: usize) -> ImageRef {
        ImageRef::null_image(width, height, RawImageFormat::RGBA8, Vec::new())
    }

    /// `ImageIconData` with explicitly-chosen (possibly hostile) f32 dimensions.
    fn image_icon(width: f32, height: f32) -> ImageIconData {
        ImageIconData::with_meta(null_img(1, 1), width, height, IconMeta::for_image())
    }

    fn font_icon(icon_char: &str) -> FontIconData {
        FontIconData::new(dummy_font_ref(), icon_char)
    }

    fn grayscale_style() -> SystemStyle {
        let mut s = SystemStyle::default();
        s.icon_style.prefer_grayscale = true;
        s
    }

    fn tint_style(color: ColorU) -> SystemStyle {
        let mut s = SystemStyle::default();
        s.icon_style.tint_color = OptionColorU::Some(color);
        s
    }

    /// A "normal" original icon node: a single div carrying `props` as inline style.
    fn original_with(props: Vec<CssPropertyWithConditions>) -> NodeData {
        let mut dom = Dom::create_div();
        dom.root
            .set_css_props(CssPropertyWithConditionsVec::from_vec(props));
        dom.root
    }

    /// An original icon node with NO inline styles. The resolver takes a
    /// `NodeData` now, so "no node at all" is no longer representable - and no
    /// longer needs to be: the old `else` branch it drove existed only because
    /// a `StyledDom` could be empty.
    fn original_without_nodes() -> NodeData {
        Dom::create_div().root
    }

    /// Every inline property on every node of the result, in document order.
    /// (Collected across all nodes rather than `node_data[0]` so the assertions
    /// survive any future anonymous-node insertion in `StyledDom::create`.)
    fn all_props(dom: &Dom) -> Vec<CssPropertyWithConditions> {
        all_nodes(dom)
            .iter()
            .flat_map(|nd| {
                nd.get_style()
                    .iter_inline_properties()
                    .map(|(property, apply_if)| CssPropertyWithConditions {
                        property: property.clone(),
                        apply_if: apply_if.clone(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn width_px(dom: &Dom) -> Option<f32> {
        all_props(dom).into_iter().find_map(|p| match p.property {
            CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::Px(px))) => {
                Some(px.number.get())
            }
            _ => None,
        })
    }

    fn height_px(dom: &Dom) -> Option<f32> {
        all_props(dom).into_iter().find_map(|p| match p.property {
            CssProperty::Height(CssPropertyValue::Exact(LayoutHeight::Px(px))) => {
                Some(px.number.get())
            }
            _ => None,
        })
    }

    fn count_widths(dom: &Dom) -> usize {
        all_props(dom)
            .iter()
            .filter(|p| matches!(p.property, CssProperty::Width(_)))
            .count()
    }

    /// Every node of a `Dom` tree, in document order.
    fn all_nodes(dom: &Dom) -> Vec<NodeData> {
        let mut out = Vec::new();
        fn walk(d: &Dom, out: &mut Vec<NodeData>) {
            out.push(d.root.clone());
            for c in d.children.as_ref() {
                walk(c, out);
            }
        }
        walk(dom, &mut out);
        out
    }

    fn text_of(dom: &Dom) -> Option<String> {
        all_nodes(dom)
            .iter()
            .find_map(|nd| match nd.get_node_type() {
                NodeType::Text(t) => Some(t.as_str().to_string()),
                _ => None,
            })
    }

    fn has_image_node(dom: &Dom) -> bool {
        all_nodes(dom)
            .iter()
            .any(|nd| matches!(nd.get_node_type(), NodeType::Image(_)))
    }

    /// All `StyleFilter`s across every `filter:` property in the list.
    fn filters_of(props: &[CssPropertyWithConditions]) -> Vec<StyleFilter> {
        props
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::Filter(CssPropertyValue::Exact(v)) => Some(v.as_ref().to_vec()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn text_color_of(props: &[CssPropertyWithConditions]) -> Option<ColorU> {
        props.iter().find_map(|p| match &p.property {
            CssProperty::TextColor(CssPropertyValue::Exact(c)) => Some(c.inner),
            _ => None,
        })
    }

    fn resolve(data: RefAny, original: &NodeData, style: &SystemStyle) -> Dom {
        default_icon_resolver(OptionRefAny::Some(data), original, style)
    }

    // ---------------------------------------------------------------------
    // default_icon_resolver — dispatch
    // ---------------------------------------------------------------------

    #[test]
    fn resolver_none_yields_single_unstyled_div() {
        let out = default_icon_resolver(
            OptionRefAny::None,
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        assert_eq!(all_nodes(&out).len(), 1);
        assert!(matches!(out.root.get_node_type(), NodeType::Div));
        // The "not found" placeholder must carry no styling at all — in particular
        // it must not inherit the original's width/height.
        assert!(all_props(&out).is_empty());
    }

    #[test]
    fn resolver_unknown_refany_type_yields_empty_div() {
        // A RefAny holding neither ImageIconData nor FontIconData must fall through
        // to the placeholder rather than panicking on a bad downcast.
        struct NotAnIconAtAll {
            _payload: [u64; 4],
        }
        let data = RefAny::new(NotAnIconAtAll { _payload: [7; 4] });
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());

        assert_eq!(all_nodes(&out).len(), 1);
        assert!(matches!(out.root.get_node_type(), NodeType::Div));
        assert!(!has_image_node(&out));
        assert!(text_of(&out).is_none());
    }

    #[test]
    fn resolver_image_icon_yields_image_node_with_default_dimensions() {
        let out = resolve(
            RefAny::new(image_icon(32.0, 24.0)),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        assert!(has_image_node(&out));
        assert_eq!(width_px(&out), Some(32.0));
        assert_eq!(height_px(&out), Some(24.0));
    }

    #[test]
    fn resolver_font_icon_yields_text_node_with_font_family() {
        let font = dummy_font_ref();
        let data = RefAny::new(FontIconData::new(font.clone(), "\u{e88a}"));
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());

        // TWO nodes: the <span> and its text leaf. It was one when the glyph
        // was a BARE text node — which is what made its css inert and left it
        // competing as a flex item with no box ("text node ... is one of 2
        // items in a InlineFlex container"). The span is the box; the glyph
        // still reads through it.
        assert_eq!(all_nodes(&out).len(), 2);
        assert_eq!(text_of(&out).as_deref(), Some("\u{e88a}"));
        assert!(
            matches!(out.root.get_node_type(), azul_core::dom::NodeType::Span),
            "the icon must be wrapped in a span — inline, and boxed"
        );

        // The registered font must be the one that ends up in `font-family`.
        let has_font = all_props(&out).iter().any(|p| match &p.property {
            CssProperty::FontFamily(CssPropertyValue::Exact(families)) => families
                .as_ref()
                .iter()
                .any(|f| matches!(f, StyleFontFamily::Ref(fr) if *fr == font)),
            _ => false,
        });
        assert!(has_font, "font-family with the icon's FontRef must be set");
    }

    // ---------------------------------------------------------------------
    // default_icon_resolver — degenerate originals
    // ---------------------------------------------------------------------

    #[test]
    fn image_icon_with_node_less_original_still_gets_dimensions() {
        // `original.node_data.first()` is None -> the fallback branch must still
        // produce a fully-sized image instead of panicking / emitting no style.
        let original = original_without_nodes();
        let out = resolve(
            RefAny::new(image_icon(16.0, 16.0)),
            &original,
            &SystemStyle::default(),
        );
        assert!(has_image_node(&out));
        assert_eq!(width_px(&out), Some(16.0));
        assert_eq!(height_px(&out), Some(16.0));
    }

    #[test]
    fn font_icon_with_node_less_original_still_gets_font() {
        let original = original_without_nodes();
        let out = resolve(
            RefAny::new(font_icon("A")),
            &original,
            &SystemStyle::default(),
        );
        assert_eq!(text_of(&out).as_deref(), Some("A"));
        assert!(all_props(&out)
            .iter()
            .any(|p| matches!(p.property, CssProperty::FontFamily(_))));
    }

    // ---------------------------------------------------------------------
    // numeric limits: the icon dimensions are attacker-controlled f32s
    // ---------------------------------------------------------------------

    #[test]
    fn image_icon_nan_dimensions_saturate_to_zero_without_panicking() {
        // FloatValue stores `(v * 1000.0) as isize`; `NaN as isize` saturates to 0,
        // so a NaN-sized icon degrades to a 0x0 box rather than poisoning layout.
        let out = resolve(
            RefAny::new(image_icon(f32::NAN, f32::NAN)),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        let (w, h) = (
            width_px(&out).expect("width emitted"),
            height_px(&out).expect("height emitted"),
        );
        assert!(
            w.is_finite() && h.is_finite(),
            "NaN must not survive into CSS"
        );
        assert_eq!(w, 0.0);
        assert_eq!(h, 0.0);
    }

    #[test]
    fn image_icon_infinite_dimensions_saturate_to_finite_values() {
        let out = resolve(
            RefAny::new(image_icon(f32::INFINITY, f32::NEG_INFINITY)),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        let w = width_px(&out).expect("width emitted");
        let h = height_px(&out).expect("height emitted");
        assert!(w.is_finite(), "+inf must saturate, got {w}");
        assert!(h.is_finite(), "-inf must saturate, got {h}");
        assert!(w > 0.0 && h < 0.0, "saturation must keep the sign");
    }

    #[test]
    fn image_icon_negative_dimensions_are_passed_through_unclamped() {
        // Documents current behaviour: the resolver does NOT reject negative sizes,
        // it forwards them verbatim into `width` / `height`.
        let out = resolve(
            RefAny::new(image_icon(-32.0, -1.5)),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        assert_eq!(width_px(&out), Some(-32.0));
        assert_eq!(height_px(&out), Some(-1.5));
    }

    /// THE bug this exists for: an oversampled bitmap is not a bigger icon.
    /// A 16px window control rasterised at 32x32 for a 2x display laid out at
    /// 32px and came out as a giant X next to the title.
    #[test]
    fn an_oversampled_icon_lays_out_at_its_natural_size_not_its_bitmap_size() {
        let mut provider = create_default_icon_provider();
        register_image_icon_sized(
            &mut provider,
            "system",
            "close",
            null_img(32, 32),
            16.0,
            16.0,
        );
        let data = provider.lookup("close").expect("icon registered");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
        assert_eq!(width_px(&out), Some(16.0));
        assert_eq!(height_px(&out), Some(16.0));

        // The negative control: the plain constructor still takes the bitmap's
        // size, which is correct for a 1:1 bitmap.
        let mut provider = create_default_icon_provider();
        register_image_icon(&mut provider, "system", "close", null_img(32, 32));
        let data = provider.lookup("close").expect("icon registered");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
        assert_eq!(width_px(&out), Some(32.0));
    }

    /// A nonsense natural size must not make the icon invisible: an unusable
    /// answer falls back to the bitmap, which is at least drawable.
    #[test]
    fn a_nonsense_natural_size_falls_back_to_the_bitmap() {
        for bad in [0.0, -8.0, f32::NAN, f32::INFINITY] {
            let mut provider = create_default_icon_provider();
            register_image_icon_sized(&mut provider, "system", "x", null_img(24, 24), bad, bad);
            let data = provider.lookup("x").expect("icon registered");
            let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
            assert_eq!(
                width_px(&out),
                Some(24.0),
                "natural size {bad} must be ignored"
            );
            assert_eq!(height_px(&out), Some(24.0));
        }
    }

    #[test]
    fn register_image_icon_with_usize_max_size_saturates() {
        // `ImageRef::get_size()` casts usize -> f32 (1.8e19); FloatValue then scales
        // by 1000 and casts to isize, which must saturate rather than wrap/panic.
        let mut provider = create_default_icon_provider();
        register_image_icon(
            &mut provider,
            "huge",
            "big",
            null_img(usize::MAX, usize::MAX),
        );
        let data = provider.lookup("big").expect("icon registered");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());

        let w = width_px(&out).expect("width emitted");
        assert!(
            w.is_finite() && w > 0.0,
            "usize::MAX size must saturate finitely, got {w}"
        );
    }

    #[test]
    fn image_icon_zero_size_is_preserved() {
        let out = resolve(
            RefAny::new(image_icon(0.0, 0.0)),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        assert_eq!(width_px(&out), Some(0.0));
        assert_eq!(height_px(&out), Some(0.0));
    }

    // ---------------------------------------------------------------------
    // style copying / precedence
    // ---------------------------------------------------------------------

    #[test]
    fn original_dimensions_win_over_image_defaults() {
        let original = original_with(vec![
            CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(999.0))),
            CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(888.0))),
        ]);
        let out = resolve(
            RefAny::new(image_icon(32.0, 32.0)),
            &original,
            &SystemStyle::default(),
        );

        assert_eq!(width_px(&out), Some(999.0));
        assert_eq!(height_px(&out), Some(888.0));
        // ...and the 32px default must not be appended as a *second* width.
        assert_eq!(count_widths(&out), 1);
    }

    #[test]
    fn copy_appropriate_styles_vec_round_trips_exactly() {
        // encode (set_css_props -> Css) == decode (copy_appropriate_styles_vec)
        let props = vec![
            CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(12.5))),
            CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(7.0))),
        ];
        let mut nd = NodeData::create_div();
        nd.set_css_props(CssPropertyWithConditionsVec::from_vec(props.clone()));

        assert_eq!(copy_appropriate_styles_vec(&nd), props);
    }

    #[test]
    fn copy_appropriate_styles_vec_of_unstyled_node_is_empty() {
        let nd = NodeData::create_div();
        assert!(copy_appropriate_styles_vec(&nd).is_empty());
    }

    #[test]
    fn copy_appropriate_styles_vec_preserves_order_of_many_props() {
        // 512 same-typed declarations: nothing may be deduplicated or reordered,
        // otherwise the last-wins cascade of the copied icon style would flip.
        let props: Vec<CssPropertyWithConditions> = (0..512u32)
            .map(|i| {
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(i as f32)))
            })
            .collect();
        let mut nd = NodeData::create_div();
        nd.set_css_props(CssPropertyWithConditionsVec::from_vec(props.clone()));

        let copied = copy_appropriate_styles_vec(&nd);
        assert_eq!(copied.len(), 512);
        assert_eq!(copied, props);
    }

    #[test]
    fn accessibility_info_is_copied_onto_the_resolved_icon() {
        let original = Dom::create_div()
            .with_accessibility_info(SmallAriaInfo::label("Save").to_full_info())
            .root;

        let out = resolve(
            RefAny::new(image_icon(8.0, 8.0)),
            &original,
            &SystemStyle::default(),
        );

        let nodes = all_nodes(&out);
        let a11y = nodes
            .iter()
            .find_map(NodeData::get_accessibility_info)
            .expect("a11y info must survive icon resolution");
        assert_eq!(
            a11y.accessibility_name.as_ref().map(|s| s.as_str()),
            Some("Save")
        );
    }

    // ---------------------------------------------------------------------
    // apply_icon_style_filters
    // ---------------------------------------------------------------------

    #[test]
    fn icon_filters_default_style_adds_nothing() {
        let mut props = Vec::new();
        apply_icon_style_filters(&mut props, &IconMeta::for_image(), &SystemStyle::default());
        assert!(
            props.is_empty(),
            "default SystemStyle must not synthesise a filter"
        );
    }

    #[test]
    fn icon_filters_grayscale_uses_quantised_luminance_matrix() {
        let mut props = Vec::new();
        apply_icon_style_filters(&mut props, &IconMeta::for_image(), &grayscale_style());

        let filters = filters_of(&props);
        assert_eq!(filters.len(), 1);
        let StyleFilter::ColorMatrix(m) = &filters[0] else {
            panic!(
                "prefer_grayscale must emit a ColorMatrix filter, got {:?}",
                filters[0]
            );
        };

        // Rec.709 luminance weights, rounded through FloatValue's 1/1000 fixed point.
        for r in [m.m0, m.m5, m.m10] {
            assert!((r.get() - 0.2126).abs() < 0.001, "R weight {}", r.get());
        }
        for g in [m.m1, m.m6, m.m11] {
            assert!((g.get() - 0.7152).abs() < 0.001, "G weight {}", g.get());
        }
        for b in [m.m2, m.m7, m.m12] {
            assert!((b.get() - 0.0722).abs() < 0.001, "B weight {}", b.get());
        }
        // Alpha row must be pass-through, or grayscale icons would turn opaque/invisible.
        assert_eq!(m.m18.get(), 1.0);
        assert_eq!(m.m15.get(), 0.0);
        assert_eq!(m.m19.get(), 0.0);

        // FloatValue truncates at 3 decimals: the 4th digit of 0.2126 is lost.
        assert_ne!(m.m0.get(), 0.2126);
    }

    #[test]
    fn icon_filters_tint_on_a_mask_floods_even_when_fully_transparent() {
        // a == 0 is still forwarded — the resolver does not treat it as "no tint".
        let transparent = ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 0,
        };
        let mut props = Vec::new();
        apply_icon_style_filters(&mut props, &IconMeta::for_mask(), &tint_style(transparent));

        let filters = filters_of(&props);
        assert_eq!(
            filters,
            vec![
                StyleFilter::Flood(transparent),
                StyleFilter::Composite(StyleCompositeFilter::In),
            ]
        );
    }

    #[test]
    fn icon_filters_tint_on_full_colour_artwork_is_ignored() {
        // An image registered without metadata is full-colour artwork: a tint
        // cannot recolour it (it gets `variants`, never a flood), so the
        // request is dropped rather than painting a coloured box.
        let mut props = Vec::new();
        apply_icon_style_filters(
            &mut props,
            &IconMeta::for_image(),
            &tint_style(ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }),
        );
        assert!(filters_of(&props).is_empty());
    }

    #[test]
    fn icon_filters_grayscale_and_tint_are_ordered_matrix_then_flood_in() {
        let tint = ColorU {
            r: 255,
            g: 0,
            b: 128,
            a: 255,
        };
        let mut style = grayscale_style();
        style.icon_style.tint_color = OptionColorU::Some(tint);

        let mut props = Vec::new();
        apply_icon_style_filters(&mut props, &IconMeta::for_mask(), &style);

        // All filters must live in ONE `filter:` declaration (a second declaration
        // would overwrite the first in the cascade, silently dropping the grayscale).
        let filter_decls = props
            .iter()
            .filter(|p| matches!(p.property, CssProperty::Filter(_)))
            .count();
        assert_eq!(filter_decls, 1);

        let filters = filters_of(&props);
        assert_eq!(filters.len(), 3);
        assert!(matches!(filters[0], StyleFilter::ColorMatrix(_)));
        assert!(matches!(filters[1], StyleFilter::Flood(c) if c == tint));
        assert!(matches!(
            filters[2],
            StyleFilter::Composite(StyleCompositeFilter::In)
        ));
    }

    #[test]
    fn icon_filters_preserve_pre_existing_properties() {
        let mut props = vec![CssPropertyWithConditions::simple(CssProperty::width(
            LayoutWidth::px(4.0),
        ))];
        apply_icon_style_filters(&mut props, &IconMeta::for_image(), &grayscale_style());

        assert_eq!(props.len(), 2);
        assert!(
            matches!(props[0].property, CssProperty::Width(_)),
            "existing props must not be clobbered"
        );
        assert!(matches!(props[1].property, CssProperty::Filter(_)));
    }

    #[test]
    fn image_icon_grayscale_reaches_the_resolved_dom() {
        let out = resolve(
            RefAny::new(image_icon(10.0, 10.0)),
            &Dom::create_div().root,
            &grayscale_style(),
        );
        let filters = filters_of(&all_props(&out));
        assert_eq!(filters.len(), 1);
        assert!(matches!(filters[0], StyleFilter::ColorMatrix(_)));
    }

    // ---------------------------------------------------------------------
    // apply_font_icon_color
    // ---------------------------------------------------------------------

    #[test]
    fn font_icon_color_default_style_adds_nothing() {
        let mut props = Vec::new();
        apply_font_icon_color(&mut props, &IconMeta::for_font(), &SystemStyle::default());
        assert!(props.is_empty());
    }

    #[test]
    fn font_icon_color_inherit_text_color_alone_is_a_noop() {
        // Documented: inheritance is CSS's default, so `inherit_text_color` must
        // *not* synthesise a `color:` declaration (that would break inheritance).
        let mut style = SystemStyle::default();
        style.icon_style.inherit_text_color = true;
        let mut props = Vec::new();
        apply_font_icon_color(&mut props, &IconMeta::for_font(), &style);
        assert!(props.is_empty());
    }

    #[test]
    fn font_icon_color_tint_becomes_text_color() {
        let tint = ColorU {
            r: 9,
            g: 8,
            b: 7,
            a: 6,
        };
        let mut props = Vec::new();
        apply_font_icon_color(&mut props, &IconMeta::for_font(), &tint_style(tint));

        assert_eq!(props.len(), 1);
        assert_eq!(text_color_of(&props), Some(tint));
    }

    #[test]
    fn font_icon_color_tint_wins_over_inherit_text_color() {
        let tint = ColorU {
            r: 1,
            g: 1,
            b: 1,
            a: 255,
        };
        let mut style = tint_style(tint);
        style.icon_style.inherit_text_color = true;

        let mut props = Vec::new();
        apply_font_icon_color(&mut props, &IconMeta::for_font(), &style);
        assert_eq!(text_color_of(&props), Some(tint));
    }

    #[test]
    fn font_icons_never_get_a_grayscale_filter() {
        // Font icons take the color path, not the filter path — a ColorMatrix here
        // would double-apply on top of the (inherited) text color.
        let out = resolve(
            RefAny::new(font_icon("\u{e88a}")),
            &Dom::create_div().root,
            &grayscale_style(),
        );
        assert!(filters_of(&all_props(&out)).is_empty());
    }

    // ---------------------------------------------------------------------
    // unicode / huge strings in the icon char
    // ---------------------------------------------------------------------

    #[test]
    fn font_icon_empty_char_yields_empty_text_node() {
        let out = resolve(
            RefAny::new(font_icon("")),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        assert_eq!(text_of(&out).as_deref(), Some(""));
    }

    #[test]
    fn font_icon_hostile_unicode_round_trips_verbatim() {
        // ZWJ emoji sequence, RTL override, combining marks, an embedded NUL and a
        // lone PUA codepoint: none may be normalised, truncated or panicked on.
        for s in [
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}",
            "\u{202E}gnippilf\u{202C}",
            "e\u{0301}\u{0327}\u{0328}",
            "a\0b",
            "\u{F8FF}",
            "\u{FFFD}",
        ] {
            let out = resolve(
                RefAny::new(font_icon(s)),
                &Dom::create_div().root,
                &SystemStyle::default(),
            );
            assert_eq!(
                text_of(&out).as_deref(),
                Some(s),
                "icon_char {s:?} was altered"
            );
        }
    }

    #[test]
    fn font_icon_huge_char_string_does_not_panic() {
        let huge = "\u{e88a}".repeat(65_536);
        let out = resolve(
            RefAny::new(font_icon(&huge)),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        assert_eq!(text_of(&out).map(|s| s.chars().count()), Some(65_536));
    }

    // ---------------------------------------------------------------------
    // registration helpers
    // ---------------------------------------------------------------------

    #[test]
    fn register_image_icon_lowercases_the_name_and_reads_size_from_the_imageref() {
        let mut provider = create_default_icon_provider();
        register_image_icon(&mut provider, "App-Images", "HOME", null_img(64, 32));

        // pack names are case-sensitive, icon names are normalised to lowercase
        assert_eq!(provider.list_packs(), vec![String::from("App-Images")]);
        assert_eq!(
            provider.list_icons_in_pack("App-Images"),
            vec![String::from("home")]
        );
        assert!(provider.list_icons_in_pack("app-images").is_empty());
        assert!(provider.has_icon("hOmE"));

        let data = provider.lookup("HOME").expect("case-insensitive lookup");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
        assert_eq!(width_px(&out), Some(64.0));
        assert_eq!(height_px(&out), Some(32.0));
    }

    #[test]
    fn register_font_icon_accepts_empty_pack_and_icon_names() {
        let mut provider = create_default_icon_provider();
        register_font_icon(&mut provider, "", "", dummy_font_ref(), "");

        assert_eq!(provider.list_packs(), vec![String::new()]);
        assert!(provider.has_icon(""));
        let data = provider
            .lookup("")
            .expect("empty-named icon is still addressable");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
        assert_eq!(text_of(&out).as_deref(), Some(""));
    }

    #[test]
    fn register_icon_handles_oversized_and_unicode_names() {
        let mut provider = create_default_icon_provider();
        let long_name = "n".repeat(10_000);
        register_font_icon(&mut provider, "p", &long_name, dummy_font_ref(), "x");
        assert!(provider.has_icon(&long_name));

        // "İ" (U+0130) lowercases to TWO chars (i + U+0307); the key is the
        // lowercased form, so the dotless "i" must NOT match.
        register_font_icon(&mut provider, "p", "\u{130}", dummy_font_ref(), "y");
        let folded = "\u{130}".to_lowercase();
        assert!(provider.has_icon("\u{130}"));
        assert!(provider.has_icon(&folded));
        assert!(!provider.has_icon("i"));
    }

    #[test]
    fn duplicate_icon_across_packs_resolves_to_the_first_registered_pack() {
        // "First match wins" in pack RANK order, then REGISTRATION order - not
        // by pack name: "zzz", registered first, shadows "aaa".
        let mut provider = create_default_icon_provider();
        register_image_icon(&mut provider, "zzz", "dup", null_img(1, 1));
        register_image_icon(&mut provider, "aaa", "dup", null_img(2, 2));

        let data = provider.lookup("dup").expect("icon registered");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
        assert_eq!(
            width_px(&out),
            Some(1.0),
            "lookup must return the first registered pack's icon"
        );

        // ...unless the later pack is ranked.
        provider.set_pack_rank("aaa", 0);
        let data = provider.lookup("dup").expect("icon registered");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
        assert_eq!(width_px(&out), Some(2.0));
    }

    #[test]
    fn re_registering_an_icon_replaces_it_and_unregistering_drops_the_empty_pack() {
        let mut provider = create_default_icon_provider();
        register_image_icon(&mut provider, "p", "icon", null_img(1, 1));
        register_image_icon(&mut provider, "p", "ICON", null_img(5, 5));

        assert_eq!(provider.list_icons_in_pack("p").len(), 1);
        let data = provider.lookup("icon").expect("icon registered");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
        assert_eq!(width_px(&out), Some(5.0));

        provider.unregister_icon("p", "IcOn");
        assert!(!provider.has_icon("icon"));
        assert!(
            provider.list_packs().is_empty(),
            "empty pack must be removed"
        );
    }

    #[test]
    fn create_default_icon_provider_starts_empty_and_misses_resolve_to_a_placeholder() {
        let provider = create_default_icon_provider();
        assert!(provider.list_packs().is_empty());
        assert!(provider.lookup("nope").is_none());
        assert!(!provider.has_icon("nope"));

        let out = default_icon_resolver(
            OptionRefAny::from(provider.lookup("nope")),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        assert_eq!(all_nodes(&out).len(), 1);
        assert!(all_props(&out).is_empty());
    }

    // ---------------------------------------------------------------------
    // ZIP / font-bytes entry points (both cfg variants share these signatures)
    // ---------------------------------------------------------------------

    #[test]
    fn load_images_from_zip_rejects_malformed_archives() {
        assert!(load_images_from_zip(&[]).is_empty());
        assert!(load_images_from_zip(b"definitely not a zip file").is_empty());
        // valid local-file-header magic, truncated body
        assert!(load_images_from_zip(b"PK\x03\x04\x00\x00\x00\x00").is_empty());
        // End-of-central-directory magic claiming 0xFFFF entries that don't exist
        assert!(load_images_from_zip(b"PK\x05\x06\xFF\xFF\xFF\xFF\xFF\xFF\xFF\xFF").is_empty());
        assert!(load_images_from_zip(&[0xFFu8; 4096]).is_empty());
    }

    #[test]
    fn register_icons_from_zip_registers_nothing_for_garbage_bytes() {
        for bytes in [
            &b""[..],
            &b"not a zip"[..],
            &b"PK\x03\x04\x00\x00\x00\x00"[..],
            &[0x00u8; 512][..],
        ] {
            let mut provider = create_default_icon_provider();
            register_icons_from_zip(&mut provider, "pack", bytes);
            assert!(
                provider.list_packs().is_empty(),
                "a malformed ZIP must not create a pack"
            );
        }
    }

    #[test]
    fn register_embedded_material_icons_rejects_non_font_bytes() {
        for bytes in [
            &b""[..],
            &b"this is not a TTF"[..],
            // sfnt version tag + nothing else
            &b"\x00\x01\x00\x00"[..],
            &[0xFFu8; 256][..],
        ] {
            let mut provider = create_default_icon_provider();
            let ok = register_embedded_material_icons(&mut provider, bytes);
            assert!(!ok, "corrupt font bytes must not report success");
            assert!(provider.list_packs().is_empty());
        }
    }

    #[cfg(feature = "icons")]
    #[test]
    fn register_material_icons_fills_a_single_lowercase_pack() {
        let mut provider = create_default_icon_provider();
        let font = dummy_font_ref();
        register_material_icons(&mut provider, &font);

        assert_eq!(provider.list_packs(), vec![String::from("material-icons")]);
        let names = provider.list_icons_in_pack("material-icons");
        assert!(
            names.len() > 1000,
            "expected the full icon set, got {}",
            names.len()
        );
        assert!(
            names.iter().all(|n| *n == n.to_lowercase()),
            "every registered icon name must be normalised to lowercase"
        );
        assert!(provider.has_icon("home"));
        assert!(provider.has_icon("HOME"));

        let data = provider.lookup("home").expect("material 'home' icon");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
        assert!(
            text_of(&out).is_some(),
            "a material icon must resolve to a text node"
        );
    }

    // ---------------------------------------------------------------------
    // icon metadata: defaults, the variant per mode, request x capability
    // (scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md 8.1/8.2)
    // ---------------------------------------------------------------------

    use azul_core::icon::{IconDesignedFor, IconModeColors};

    fn dark_style() -> SystemStyle {
        let mut s = SystemStyle::default();
        s.theme = DarkLightMode::Dark;
        s
    }

    fn masked_image(meta: IconMeta) -> RefAny {
        RefAny::new(ImageIconData::with_meta(null_img(4, 4), 4.0, 4.0, meta))
    }

    /// The icon spec a resolution redirected to, if it is an icon node.
    fn icon_name_of(dom: &Dom) -> Option<String> {
        match dom.root.get_node_type() {
            NodeType::Icon(n) => Some(n.as_str().to_string()),
            _ => None,
        }
    }

    /// The LAST `color:` of the resolved icon (later declarations win).
    fn last_text_color(dom: &Dom) -> Option<ColorU> {
        all_props(dom).iter().rev().find_map(|p| match &p.property {
            CssProperty::TextColor(CssPropertyValue::Exact(c)) => Some(c.inner),
            _ => None,
        })
    }

    fn with_css_color(color: ColorU) -> NodeData {
        original_with(vec![CssPropertyWithConditions::simple(
            CssProperty::TextColor(CssPropertyValue::Exact(StyleTextColor { inner: color })),
        )])
    }

    const CSS_RED: ColorU = ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    const JSON_BLUE: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    };

    #[test]
    fn a_font_icon_follows_the_text_colour_by_default() {
        let icon = FontIconData::new(dummy_font_ref(), "\u{e88a}");
        assert_eq!(icon.meta.recolor, IconRecolor::CurrentColor);
        assert!(icon.meta.monochrome, "a glyph is one colour by construction");
        assert_eq!(icon.meta.designed_for, IconDesignedFor::Any);
        assert_eq!(icon.meta, IconMeta::for_font());
    }

    #[test]
    fn an_image_icon_is_never_recoloured_by_default() {
        let mut provider = create_default_icon_provider();
        register_image_icon(&mut provider, "app", "logo", null_img(8, 8));
        let mut data = provider.lookup("logo").expect("registered");
        {
            let img = data
                .downcast_ref::<ImageIconData>()
                .expect("an image icon");
            assert_eq!(img.meta.recolor, IconRecolor::None);
            assert!(!img.meta.monochrome);
            assert_eq!(img.meta, IconMeta::for_image());
        }
        // A tint request cannot touch full-colour artwork: no flood, no filter.
        let out = resolve(data, &Dom::create_div().root, &tint_style(CSS_RED));
        assert!(filters_of(&all_props(&out)).is_empty());
    }

    #[test]
    fn registering_an_image_with_metadata_keeps_the_metadata() {
        let mut provider = create_default_icon_provider();
        register_image_icon_with_meta(
            &mut provider,
            "app",
            "glyph",
            null_img(16, 16),
            IconMeta::for_mask(),
        );
        let mut data = provider.lookup("glyph").expect("registered");
        let img = data.downcast_ref::<ImageIconData>().expect("an image icon");
        assert_eq!(img.meta, IconMeta::for_mask());
        assert_eq!((img.width, img.height), (16.0, 16.0));
    }

    #[test]
    fn the_dark_mode_picks_the_dark_variant_and_the_light_mode_keeps_the_icon() {
        let meta = IconMeta::for_image().with_dark_variant("home-dark");
        let original = Dom::create_icon("home").root;

        let dark = resolve(masked_image(meta.clone()), &original, &dark_style());
        assert_eq!(
            icon_name_of(&dark).as_deref(),
            Some("home-dark"),
            "dark mode must redirect to the dark artwork"
        );

        let light = resolve(masked_image(meta), &original, &SystemStyle::default());
        assert!(has_image_node(&light), "light mode draws the icon itself");
    }

    #[test]
    fn a_light_to_dark_switch_swaps_the_artwork_through_the_provider() {
        let mut provider = create_default_icon_provider();
        register_image_icon_with_meta(
            &mut provider,
            "app",
            "home",
            null_img(4, 4),
            IconMeta::for_image().with_dark_variant("home-dark"),
        );
        register_image_icon(&mut provider, "app", "home-dark", null_img(9, 9));
        let shared = azul_core::icon::SharedIconProvider::from_handle(provider);

        let mut light = Dom::create_icon("home");
        azul_core::icon::resolve_icons_in_dom(&mut light, &shared, &SystemStyle::default());
        assert_eq!(width_px(&light), Some(4.0));

        // Same provider, same cache: the mode flip alone must swap the artwork.
        let mut dark = Dom::create_icon("home");
        azul_core::icon::resolve_icons_in_dom(&mut dark, &shared, &dark_style());
        assert_eq!(width_px(&dark), Some(9.0), "the dark variant's own artwork");
    }

    #[test]
    fn the_high_contrast_variant_wins_when_high_contrast_is_asked_for() {
        let meta = IconMeta::for_image()
            .with_dark_variant("home-dark")
            .with_high_contrast_variant("home-hc");
        let mut style = dark_style();
        style.prefers_high_contrast = azul_css::dynamic_selector::BoolCondition::True;
        let out = resolve(masked_image(meta), &Dom::create_icon("home").root, &style);
        assert_eq!(icon_name_of(&out).as_deref(), Some("home-hc"));
    }

    #[test]
    fn a_variant_that_names_the_icon_itself_is_not_followed() {
        let meta = IconMeta::for_image().with_dark_variant("HOME");
        let out = resolve(masked_image(meta), &Dom::create_icon("home").root, &dark_style());
        assert!(has_image_node(&out), "a self-reference draws the icon itself");
    }

    #[test]
    fn a_tint_on_a_mask_icon_floods_it_through_its_own_alpha() {
        let tint = ColorU {
            r: 200,
            g: 10,
            b: 10,
            a: 255,
        };
        let out = resolve(
            masked_image(IconMeta::for_mask()),
            &Dom::create_div().root,
            &tint_style(tint),
        );
        assert_eq!(
            filters_of(&all_props(&out)),
            vec![
                StyleFilter::Flood(tint),
                StyleFilter::Composite(StyleCompositeFilter::In),
            ],
            "a bare flood paints the whole box (E15); composite(in) keeps it inside the \
             artwork's alpha"
        );
    }

    #[test]
    fn a_mask_icon_without_a_request_is_drawn_as_it_is() {
        let out = resolve(
            masked_image(IconMeta::for_mask()),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        assert!(filters_of(&all_props(&out)).is_empty());
    }

    #[test]
    fn current_colour_on_a_monochrome_raster_floods_with_the_css_colour() {
        let meta = IconMeta::for_image()
            .with_recolor(IconRecolor::CurrentColor)
            .with_monochrome(true);
        let out = resolve(masked_image(meta), &Dom::create_div().root, &SystemStyle::default());
        assert_eq!(
            filters_of(&all_props(&out)),
            vec![
                StyleFilter::Flood(CURRENT_COLOR_TOKEN),
                StyleFilter::Composite(StyleCompositeFilter::In),
            ],
            "the flood carries the currentColor token; the display list swaps in the node's \
             cascaded `color`"
        );
    }

    #[test]
    fn current_colour_on_full_colour_raster_artwork_is_not_honoured() {
        // `monochrome: false` says the artwork is not an alpha mask: flooding
        // it would erase its own colours, so only variants apply.
        let meta = IconMeta::for_image().with_recolor(IconRecolor::CurrentColor);
        let out = resolve(masked_image(meta), &Dom::create_div().root, &tint_style(CSS_RED));
        assert!(filters_of(&all_props(&out)).is_empty());
    }

    #[test]
    fn a_mask_icon_drawn_for_light_follows_the_text_colour_in_dark_mode() {
        let meta = IconMeta::for_mask().with_designed_for(IconDesignedFor::Light);

        let light = resolve(
            masked_image(meta.clone()),
            &Dom::create_div().root,
            &SystemStyle::default(),
        );
        assert!(
            filters_of(&all_props(&light)).is_empty(),
            "in the mode it was drawn for, the artwork is drawn as it is"
        );

        let dark = resolve(masked_image(meta), &Dom::create_div().root, &dark_style());
        assert_eq!(
            filters_of(&all_props(&dark)),
            vec![
                StyleFilter::Flood(CURRENT_COLOR_TOKEN),
                StyleFilter::Composite(StyleCompositeFilter::In),
            ],
            "dark ink on a dark background: follow the text colour instead"
        );
    }

    #[test]
    fn a_fixed_recolour_picks_the_colour_of_the_mode() {
        let colors = IconModeColors {
            light: ColorU {
                r: 1,
                g: 1,
                b: 1,
                a: 255,
            },
            dark: ColorU {
                r: 250,
                g: 250,
                b: 250,
                a: 255,
            },
        };
        let meta = IconMeta::for_mask().with_recolor(IconRecolor::Fixed(colors));
        let dark = resolve(masked_image(meta.clone()), &Dom::create_div().root, &dark_style());
        assert_eq!(filters_of(&all_props(&dark))[0], StyleFilter::Flood(colors.dark));
        let light = resolve(masked_image(meta), &Dom::create_div().root, &SystemStyle::default());
        assert_eq!(filters_of(&all_props(&light))[0], StyleFilter::Flood(colors.light));
    }

    /// Pitfall 10: a rice can set `color` on the `<icon>` (CSS) and `recolor`
    /// in `remap.json`. An explicit recolour colour beats the CSS `color`.
    #[test]
    fn an_explicit_recolour_colour_beats_the_css_colour_on_a_font_icon() {
        let icon = FontIconData::new(dummy_font_ref(), "x").with_meta(
            IconMeta::for_font().with_recolor(IconRecolor::Fixed(IconModeColors::same(JSON_BLUE))),
        );
        let out = resolve(RefAny::new(icon), &with_css_color(CSS_RED), &SystemStyle::default());
        assert_eq!(last_text_color(&out), Some(JSON_BLUE));
    }

    /// ...and `recolor: currentColor` means "take the CSS colour".
    #[test]
    fn current_color_recolour_takes_the_css_colour_on_a_font_icon() {
        let icon = FontIconData::new(dummy_font_ref(), "x");
        let out = resolve(RefAny::new(icon), &with_css_color(CSS_RED), &SystemStyle::default());
        assert_eq!(last_text_color(&out), Some(CSS_RED));
    }

    // ---------------------------------------------------------------------
    // SVG icons: registration without Rust-side rasterising
    // ---------------------------------------------------------------------

    const CURRENT_COLOR_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16"><rect x="4" y="4" width="8" height="8" fill="currentColor"/></svg>"#;
    const FULL_COLOUR_SVG: &[u8] = br##"<svg viewBox="0 0 24 12"><rect width="12" height="12" fill="#000000"/><rect x="12" width="12" height="12" fill="#00ff00"/></svg>"##;

    #[cfg(feature = "cpurender")]
    #[test]
    fn a_current_color_svg_defaults_to_following_the_text_colour() {
        let meta = default_svg_icon_meta(CURRENT_COLOR_SVG);
        assert_eq!(meta.recolor, IconRecolor::CurrentColor);
        assert!(meta.monochrome, "every paint is currentColor: an alpha mask");
    }

    #[test]
    fn a_full_colour_svg_defaults_to_never_recoloured() {
        assert_eq!(default_svg_icon_meta(FULL_COLOUR_SVG), IconMeta::for_image());
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn register_svg_icon_keeps_the_document_its_natural_size_and_metadata() {
        let mut provider = create_default_icon_provider();
        let meta = IconMeta::for_image().with_dark_variant("logo-dark");
        assert!(register_svg_icon(
            &mut provider,
            "app",
            "Logo",
            FULL_COLOUR_SVG,
            meta.clone()
        ));
        let mut data = provider.lookup("logo").expect("registered, name folded");
        let svg = data.downcast_ref::<SvgIconData>().expect("an SVG icon");
        assert_eq!((svg.width, svg.height), (24.0, 12.0), "the viewBox size");
        assert_eq!(svg.meta, meta);
        assert_eq!(svg.svg.as_slice(), FULL_COLOUR_SVG);
    }

    #[test]
    fn register_svg_icon_refuses_what_is_not_an_svg() {
        let mut provider = create_default_icon_provider();
        assert!(!register_svg_icon(
            &mut provider,
            "app",
            "x",
            b"not an svg",
            IconMeta::for_image()
        ));
        let huge = vec![b' '; MAX_SVG_ICON_BYTES + 1];
        assert!(!register_svg_icon(
            &mut provider,
            "app",
            "y",
            &huge,
            IconMeta::for_image()
        ));
        assert!(provider.list_packs().is_empty(), "nothing registered");
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn a_resolved_svg_icon_is_an_image_at_its_natural_size() {
        let mut provider = create_default_icon_provider();
        register_svg_icon(
            &mut provider,
            "app",
            "dot",
            CURRENT_COLOR_SVG,
            default_svg_icon_meta(CURRENT_COLOR_SVG),
        );
        let data = provider.lookup("dot").expect("registered");
        let out = resolve(data, &Dom::create_div().root, &SystemStyle::default());
        assert!(has_image_node(&out));
        assert_eq!(width_px(&out), Some(16.0));
        assert_eq!(height_px(&out), Some(16.0));
        assert_eq!(
            filters_of(&all_props(&out)),
            vec![
                StyleFilter::Flood(CURRENT_COLOR_TOKEN),
                StyleFilter::Composite(StyleCompositeFilter::In),
            ],
            "a monochrome currentColor document follows the node's `color` like a glyph"
        );
    }

    #[test]
    fn a_font_icon_that_refuses_recolouring_ignores_the_tint() {
        let icon = FontIconData::new(dummy_font_ref(), "x")
            .with_meta(IconMeta::for_font().with_recolor(IconRecolor::None));
        let out = resolve(RefAny::new(icon), &Dom::create_div().root, &tint_style(JSON_BLUE));
        assert_eq!(last_text_color(&out), None);
    }
}
