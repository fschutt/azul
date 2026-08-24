//! Rendering an icon-registry entry to RGBA pixels, at any size.
//!
//! This exists for the system tray, which is the one consumer that needs an
//! icon as *pixels* rather than as part of a DOM: Windows hands RGBA to
//! `CreateIconIndirect`, macOS to `NSBitmapImageRep`, and Linux to SNI's
//! `IconPixmap` (`a(iiay)`).
//!
//! # Why it goes through the DOM
//!
//! The obvious implementation — look up the icon, notice it is a font glyph,
//! rasterize the glyph — would work for Material Icons and nothing else. The
//! registry holds `ImageIconData` (image packs loaded from a ZIP) and
//! `FontIconData` (Material Icons) today, and whatever a custom resolver
//! returns tomorrow.
//!
//! So instead this does what an `<icon>` node does: build a one-node icon DOM,
//! run the SAME `resolve_icons_in_styled_dom` pass, and render the resulting
//! `StyledDom` with the CPU renderer. Every icon kind is handled because none
//! of them is special-cased — and anything a future resolver can express as a
//! DOM (an SVG, an emoji, a styled `<div>`) becomes a usable tray icon for
//! free, with no change here.
//!
//! It also means a tray icon and an in-DOM `<icon>` of the same spec can never
//! disagree: they are literally the same code path up to rasterization.

use alloc::{string::String, vec::Vec};

use azul_core::{
    dom::Dom,
    icon::{resolve_icons_in_styled_dom, SharedIconProvider},
    styled_dom::StyledDom,
};
use azul_css::{
    css::{Css, CssPropertyValue},
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{ColorU, StyleFontSize},
        layout::{LayoutHeight, LayoutWidth},
        property::CssProperty,
        style::text::StyleTextColor,
    },
    system::SystemStyle,
};

/// A rendered icon: device-pixel dimensions plus straight RGBA8, top row first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedIcon {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes, non-premultiplied RGBA8.
    pub rgba: Vec<u8>,
}

/// Render an icon spec to an RGBA bitmap of `size_px` square.
///
/// `spec` is exactly what an `<icon>` node takes: a bare name (`"settings"`),
/// a pack-qualified name (`"mypack:logo"`), or a comma-separated fallback list.
/// Parsing it is the registry's job — this passes it through untouched so that
/// a tray icon and an `<icon>` can never resolve differently.
///
/// The background is fully transparent, which is what all three tray backends
/// want. Note the consequence for **Material Icons specifically**: they resolve
/// to a text node whose colour comes from the cascade, and the default is
/// opaque black. That is correct for a light menu bar / taskbar and invisible
/// on a dark one. Callers that know the tray's background — macOS, where a
/// template image is tinted by AppKit anyway — should pass `tint`.
///
/// Returns `None` if the spec resolves to nothing (an unregistered icon), or if
/// the renderer fails.
#[must_use]
pub fn render_icon_to_rgba(
    spec: &str,
    size_px: u32,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
    tint: Option<ColorU>,
) -> Option<RenderedIcon> {
    use crate::{
        cpurender::{render_component_preview, ComponentPreviewOptions},
        font::loading::build_font_cache,
        font_traits::FontManager,
    };

    if size_px == 0 || spec.is_empty() {
        return None;
    }

    // Cheap reject before doing any layout work: an unregistered spec would
    // otherwise resolve to an empty div and render a fully transparent square,
    // which the caller cannot distinguish from a legitimately blank icon.
    if !spec_resolves(spec, provider) {
        return None;
    }

    #[allow(clippy::cast_precision_loss)] // icon sizes are small
    let size = size_px as f32;

    let mut props = alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::Width(CssPropertyValue::Exact(
            LayoutWidth::px(size)
        ))),
        CssPropertyWithConditions::simple(CssProperty::Height(CssPropertyValue::Exact(
            LayoutHeight::px(size)
        ))),
        // A font icon resolves to TEXT, and a glyph is sized by font-size, not
        // by its box — without this the box would be size_px and the glyph
        // whatever the cascade's default font-size happens to be.
        CssPropertyWithConditions::simple(CssProperty::FontSize(CssPropertyValue::Exact(
            StyleFontSize::px(size)
        ))),
    ];
    if let Some(c) = tint {
        props.push(CssPropertyWithConditions::simple(CssProperty::TextColor(
            CssPropertyValue::Exact(StyleTextColor { inner: c }),
        )));
    }

    let mut dom = Dom::create_icon(String::from(spec))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(props));

    let mut styled = StyledDom::create(&mut dom, Css::empty());
    resolve_icons_in_styled_dom(&mut styled, provider, system_style);

    let fc_cache = build_font_cache();
    let font_manager = FontManager::new(fc_cache).ok()?;

    let result = render_component_preview(
        &styled,
        &font_manager,
        ComponentPreviewOptions {
            width: Some(size),
            height: Some(size),
            dpi_factor: 1.0,
            // Transparent, NOT the white the preview renderer defaults to — a
            // tray icon composited over a white square is a white square.
            background_color: ColorU { r: 0, g: 0, b: 0, a: 0 },
        },
        None,
    )
    .ok()?;

    if result.rgba.is_empty() || result.pixel_width == 0 || result.pixel_height == 0 {
        return None;
    }

    Some(RenderedIcon {
        width: result.pixel_width,
        height: result.pixel_height,
        rgba: result.rgba,
    })
}

/// Does any pack hold this spec? Mirrors the registry's own lookup, including
/// the comma-separated fallback list, so a spec that `<icon>` would resolve is
/// never rejected here.
fn spec_resolves(spec: &str, provider: &SharedIconProvider) -> bool {
    spec.split(',').any(|alt| {
        let alt = alt.trim();
        !alt.is_empty() && provider.lookup(alt).is_some()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul_core::icon::{IconProviderHandle, SharedIconProvider};

    fn empty_provider() -> SharedIconProvider {
        SharedIconProvider::from_handle(IconProviderHandle::new())
    }

    #[test]
    fn degenerate_inputs_are_rejected_before_any_layout_work() {
        let p = empty_provider();
        let s = SystemStyle::default();
        assert!(render_icon_to_rgba("settings", 0, &p, &s, None).is_none());
        assert!(render_icon_to_rgba("", 16, &p, &s, None).is_none());
    }

    #[test]
    fn an_unregistered_spec_is_none_not_a_blank_square() {
        // The distinction matters: resolution turns an unknown icon into an
        // empty div, which renders as a fully transparent square. Returning
        // that as Some() would put an invisible icon in the tray and look
        // exactly like a working one.
        let p = empty_provider();
        assert!(render_icon_to_rgba("definitely_not_registered", 16, &p, &SystemStyle::default(), None).is_none());
    }

    #[test]
    fn spec_resolution_honours_the_fallback_list() {
        use azul_core::refany::RefAny;
        let mut h = IconProviderHandle::new();
        h.register_icon("testpack", "logo", RefAny::new(1u8));
        let p = SharedIconProvider::from_handle(h);

        assert!(spec_resolves("logo", &p));
        assert!(spec_resolves("testpack:logo", &p));
        // First alternative missing, second present — must still resolve.
        assert!(spec_resolves("missing:x, logo", &p));
        assert!(!spec_resolves("missing:x", &p));
        assert!(!spec_resolves("", &p));
        assert!(!spec_resolves(" , ", &p));
    }
}
