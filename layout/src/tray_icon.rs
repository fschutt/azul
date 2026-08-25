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
//! run the SAME `resolve_icons_in_dom` pass, and render the resulting
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
    icon::{resolve_icons_in_dom, SharedIconProvider},
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
/// template image is tinted by `AppKit` anyway — should pass `tint`.
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
    font_manager: &crate::font_traits::FontManager<azul_css::props::basic::FontRef>,
) -> Option<RenderedIcon> {
    use crate::cpurender::{render_component_preview, ComponentPreviewOptions};

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

    let dom = Dom::create_icon(String::from(spec))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(props));

    // Icons resolve on the Dom, BEFORE the cascade, so the replacement is
    // spliced in whole and cascaded exactly once with everything else. Doing it
    // the other way round - cascade, then splice into the flat StyledDom arena -
    // is what used to flatten every icon to a single node and leave the property
    // cache describing the pre-resolution node.
    let mut dom = dom;
    resolve_icons_in_dom(&mut dom, provider, system_style);

    // `create_from_dom`, NOT `create(&mut dom, Css::empty())`: the former also
    // runs `fixup_children_estimated` + `scope_inline_css` and collects the
    // tree's inline CSS, which is what the engine's own layout path
    // (`regenerate_layout`) uses. Building the StyledDom the other way produces
    // a subtly different property cache and the icon does not resolve the same.
    let styled = StyledDom::create_from_dom(dom);

    // The caller's FontManager, NOT a fresh one.
    //
    // Building one here (`FontManager::new(build_font_cache())`) creates a
    // second, disconnected font universe: a `FontManager` shapes from TWO pools
    // — `parsed_fonts` (resolved by family NAME out of the system font cache)
    // and `embedded_fonts` (handed over directly as `StyleFontFamily::Ref`) —
    // and Material Icons live in the SECOND, because
    // `create_font_icon_from_original` styles the glyph with
    // `StyleFontFamily::Ref(font)` rather than a family name. A fresh manager
    // has that pool empty, so every icon shaped to `.notdef` and the tray got a
    // tofu box, with every intermediate step reporting success. Same shape as
    // the 0.2.0 CPU-renderer failure documented on
    // `FontManager::resolve_font_by_hash`.
    //
    // Registering the DOM's embedded fonts is still needed for a manager that
    // has not seen this particular icon font yet; it is idempotent.
    crate::solver3::getters::register_embedded_fonts_from_styled_dom(
        &styled,
        font_manager,
        &system_style.platform,
    );

    let result = render_component_preview(
        &styled,
        font_manager,
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

    // A wrong-looking tray icon is otherwise almost impossible to diagnose: it
    // is a 36px image inside the menu bar, and every intermediate step reports
    // success. `AZ_TRAY_ICON_DUMP=<dir>` writes the exact bitmap we hand the OS.
    if let Ok(dir) = std::env::var("AZ_TRAY_ICON_DUMP") {
        let path = format!("{dir}/tray-icon-{size_px}.png");
        // Diagnostic dump only: a write failure here must never affect the
        // icon actually being handed to the OS.
        drop(std::fs::write(&path, &result.png_data));
    }

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

    // `render_icon_to_rgba` itself is not unit-tested: it needs a real
    // `FontManager`, and building one scans the system font directories, which
    // makes the test slow and machine-dependent. Its pure precondition —
    // "does this spec resolve at all" — is what actually decides whether the
    // caller gets an icon or a silently-blank square, so that is what is
    // pinned here. The rendering itself is exercised by the `tray` example
    // plus `AZ_TRAY_ICON_DUMP`.

    #[test]
    fn spec_resolution_honours_the_fallback_list() {
        use azul_core::refany::RefAny;
        let mut h = IconProviderHandle::new();
        h.register_icon("testpack", "logo", RefAny::new(1u8));
        let p = SharedIconProvider::from_handle(h);

        assert!(spec_resolves("logo", &p));
        assert!(spec_resolves("testpack:logo", &p));
        // First alternative missing, second present - must still resolve.
        assert!(spec_resolves("missing:x, logo", &p));
        assert!(!spec_resolves("missing:x", &p));
        assert!(!spec_resolves("", &p));
        assert!(!spec_resolves(" , ", &p));
    }

    /// REGRESSION (tray icons rendered as `.notdef` tofu boxes).
    ///
    /// `resolve_icons_in_dom` rewrites a font icon's inline style to
    /// carry `font-family: StyleFontFamily::Ref(face)` — the only place that
    /// face is ever named. But the compact CSS cache is precomputed at cascade
    /// time, so before the fix it still described the pre-resolution `<icon>`
    /// node. `collect_font_stacks_from_styled_dom` reads that cache, so it
    /// never saw the `Ref`, never registered the face as an embedded font, and
    /// shaping silently fell back to a system face with no glyph at the icon's
    /// private-use codepoint.
    ///
    /// The invariant this pins: **after resolving a font-backed icon, the face
    /// is visible to font-stack collection.** Everything downstream — embedded
    /// registration, shaping, rasterisation — depends on it, and every one of
    /// those steps reported success while producing a tofu box, so this is the
    /// only place the failure is cheap to catch.
    #[test]
    fn resolving_a_font_icon_leaves_its_face_visible_to_font_collection() {
        use azul_core::{
            dom::Dom, icon::IconProviderHandle, refany::RefAny, styled_dom::StyledDom,
        };
        use azul_css::{css::Css, props::basic::FontRef};

        // A FontRef needs no real font data here: collection only reports which
        // faces are referenced, it does not parse them.
        static DUMMY: u8 = 0;
        extern "C" fn noop(_: *mut core::ffi::c_void) {}
        let face = FontRef::new(
            core::ptr::addr_of!(DUMMY).cast::<core::ffi::c_void>(),
            noop,
        );

        let mut handle = IconProviderHandle::with_resolver(crate::icon::default_icon_resolver);
        handle.register_icon(
            "testpack",
            "gear",
            RefAny::new(crate::icon::FontIconData {
                font: face.clone(),
                icon_char: "\u{e8b8}".into(),
            }),
        );
        let provider = SharedIconProvider::from_handle(handle);

        let mut dom = Dom::create_icon(String::from("gear"));
        azul_core::icon::resolve_icons_in_dom(
            &mut dom,
            &provider,
            &SystemStyle::default(),
        );
        let styled = StyledDom::create_from_dom(dom);

        let collected = crate::solver3::getters::collect_font_stacks_from_styled_dom(
            &styled,
            &SystemStyle::default().platform,
        );
        assert!(
            !collected.font_refs.is_empty(),
            "the resolved icon's FontRef must reach font-stack collection; \
             an empty set is the tofu-box bug (stale compact cache)"
        );
    }

    #[test]
    fn an_unregistered_spec_does_not_resolve() {
        // This is what stops a blank bitmap reaching the tray: resolution turns
        // an unknown icon into an empty div, which renders as a fully
        // TRANSPARENT square - indistinguishable from a working icon once it is
        // 18pt in a menu bar.
        let p = SharedIconProvider::from_handle(IconProviderHandle::new());
        assert!(!spec_resolves("definitely_not_registered", &p));
    }
}
