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
//! // Register an image icon
//! provider.register_icon("app-images", "logo", RefAny::new(ImageIconData { 
//!     image: image_ref, width: 32.0, height: 32.0 
//! }));
//!
//! // Register a font icon
//! provider.register_icon("material-icons", "home", RefAny::new(FontIconData {
//!     font: font_ref, icon_char: "\u{e88a}".to_string()
//! }));
//! ```

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use azul_css::{
    system::SystemStyle,
    props::basic::{FontRef, StyleFontFamily, StyleFontFamilyVec},
    props::basic::length::FloatValue,
    props::layout::{LayoutWidth, LayoutHeight},
    props::property::CssProperty,
    props::style::filter::{StyleFilter, StyleFilterVec, StyleColorMatrix},
    props::style::text::StyleTextColor,
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    css::{Css, CssPropertyValue},
};

use azul_core::{
    dom::{Dom, NodeData},
    icon::IconProviderHandle,
    refany::{OptionRefAny, RefAny},
    resources::ImageRef,
    styled_dom::StyledDom,
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
    /// Width duplicated from `ImageRef` at registration time
    pub width: f32,
    /// Height duplicated from `ImageRef` at registration time
    pub height: f32,
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
#[must_use] pub extern "C" fn default_icon_resolver(
    icon_data: OptionRefAny,
    original_icon_dom: &StyledDom,
    system_style: &SystemStyle,
) -> StyledDom {
    // No icon found → empty div
    let Some(mut data) = icon_data.into_option() else {
        let mut dom = Dom::create_div();
        return StyledDom::create(&mut dom, Css::empty());
    };
    
    // Try ImageIconData
    if let Some(img) = data.downcast_ref::<ImageIconData>() {
        return create_image_icon_from_original(&img, original_icon_dom, system_style);
    }
    
    // Try FontIconData
    if let Some(font_icon) = data.downcast_ref::<FontIconData>() {
        return create_font_icon_from_original(&font_icon, original_icon_dom, system_style);
    }
    
    // Unknown data type → empty div
    let mut dom = Dom::create_div();
    StyledDom::create(&mut dom, Css::empty())
}

// Icon DOM Creation (from original)

/// Create a `StyledDom` for an image-based icon, copying styles from original.
///
/// Applies SystemStyle-aware modifications:
/// - Grayscale filter if `prefer_grayscale` is true
/// - Tint color overlay if `tint_color` is set
fn create_image_icon_from_original(
    img: &ImageIconData,
    original: &StyledDom,
    system_style: &SystemStyle,
) -> StyledDom {
    let mut dom = Dom::create_image(img.image.clone());
    
    // Copy appropriate styles from original
    if let Some(original_node) = original.node_data.as_ref().first() {
        let mut props_vec = copy_appropriate_styles_vec(original_node);
        
        // Add default dimensions if not specified in original styles
        let has_width = props_vec.iter().any(|p| matches!(&p.property, CssProperty::Width(_)));
        let has_height = props_vec.iter().any(|p| matches!(&p.property, CssProperty::Height(_)));
        
        if !has_width {
            props_vec.push(CssPropertyWithConditions::simple(
                CssProperty::width(LayoutWidth::px(img.width))
            ));
        }
        if !has_height {
            props_vec.push(CssPropertyWithConditions::simple(
                CssProperty::height(LayoutHeight::px(img.height))
            ));
        }
        
        // Apply SystemStyle-aware filters
        apply_icon_style_filters(&mut props_vec, system_style);
        
        dom.root.set_css_props(CssPropertyWithConditionsVec::from_vec(props_vec));
        
        // Copy accessibility info
        if let Some(a11y) = original_node.get_accessibility_info() {
            dom = dom.with_accessibility_info(a11y.clone());
        }
    } else {
        // No original node, use default dimensions
        let mut props_vec = vec![
            CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(img.width))),
            CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(img.height))),
        ];
        
        // Apply SystemStyle-aware filters even without original node
        apply_icon_style_filters(&mut props_vec, system_style);
        
        dom.root.set_css_props(CssPropertyWithConditionsVec::from_vec(props_vec));
    }
    
    StyledDom::create(&mut dom, Css::empty())
}

/// Create a `StyledDom` for a font-based icon, copying styles from original.
///
/// Applies SystemStyle-aware modifications:
/// - Text color override if `inherit_text_color` is true
/// - Tint color if `tint_color` is set
fn create_font_icon_from_original(
    font_icon: &FontIconData,
    original: &StyledDom,
    system_style: &SystemStyle,
) -> StyledDom {
    let mut dom = Dom::create_text(font_icon.icon_char.clone());
    
    // Add font family
    let font_prop = CssPropertyWithConditions::simple(
        CssProperty::font_family(StyleFontFamilyVec::from_vec(vec![
            StyleFontFamily::Ref(font_icon.font.clone())
        ]))
    );
    
    if let Some(original_node) = original.node_data.as_ref().first() {
        let mut props_vec = copy_appropriate_styles_vec(original_node);
        props_vec.push(font_prop);
        
        // Apply SystemStyle-aware color modifications for font icons
        apply_font_icon_color(&mut props_vec, system_style);
        
        dom.root.set_css_props(CssPropertyWithConditionsVec::from_vec(props_vec));
        
        // Copy accessibility info
        if let Some(a11y) = original_node.get_accessibility_info() {
            dom = dom.with_accessibility_info(a11y.clone());
        }
    } else {
        // No original node, just set the font
        let mut props_vec = vec![font_prop];
        
        // Apply SystemStyle-aware color modifications
        apply_font_icon_color(&mut props_vec, system_style);
        
        dom.root.set_css_props(CssPropertyWithConditionsVec::from_vec(props_vec));
    }
    
    StyledDom::create(&mut dom, Css::empty())
}

/// Copy styles from original node
/// Returns a Vec for easier manipulation
fn copy_appropriate_styles_vec(
    original_node: &NodeData,
) -> Vec<CssPropertyWithConditions> {
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

/// Apply SystemStyle-aware filters to icon properties.
///
/// This adds CSS filters based on accessibility and theming settings:
/// - Grayscale filter if `prefer_grayscale` is true
fn apply_icon_style_filters(
    props_vec: &mut Vec<CssPropertyWithConditions>,
    system_style: &SystemStyle,
) {
    let icon_style = &system_style.icon_style;
    
    // Collect filters to apply
    let mut filters = Vec::new();
    
    // Grayscale filter: Uses a color matrix that converts to grayscale
    // Standard luminance weights: R*0.2126 + G*0.7152 + B*0.0722
    if icon_style.prefer_grayscale {
        // Grayscale color matrix (4x5):
        // [0.2126, 0.7152, 0.0722, 0, 0]  <- R output
        // [0.2126, 0.7152, 0.0722, 0, 0]  <- G output
        // [0.2126, 0.7152, 0.0722, 0, 0]  <- B output
        // [0,      0,      0,      1, 0]  <- A output
        let grayscale_matrix = StyleColorMatrix {
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
        };
        filters.push(StyleFilter::ColorMatrix(grayscale_matrix));
    }
    
    // Apply tint color as a flood filter if specified
    if let azul_css::props::basic::color::OptionColorU::Some(tint) = &icon_style.tint_color {
        filters.push(StyleFilter::Flood(*tint));
    }
    
    // Add filters if any were collected
    if !filters.is_empty() {
        props_vec.push(CssPropertyWithConditions::simple(
            CssProperty::Filter(CssPropertyValue::Exact(StyleFilterVec::from_vec(filters)))
        ));
    }
}

/// Apply SystemStyle-aware color modifications for font icons.
///
/// Font icons can use text color directly, so we can:
/// - Apply tint color as text color
/// - Inherit text color from parent
fn apply_font_icon_color(
    props_vec: &mut Vec<CssPropertyWithConditions>,
    system_style: &SystemStyle,
) {
    let icon_style = &system_style.icon_style;
    
    // If tint color is specified, use it as the text color
    if let azul_css::props::basic::color::OptionColorU::Some(tint) = &icon_style.tint_color {
        props_vec.push(CssPropertyWithConditions::simple(
            CssProperty::TextColor(CssPropertyValue::Exact(StyleTextColor { inner: *tint }))
        ));
    }
    // Note: inherit_text_color doesn't need explicit handling - text color
    // is inherited by default in CSS. We only need to NOT override it.
}

// IconProviderHandle Helper Functions

/// Register an image icon in a pack
pub fn register_image_icon(provider: &mut IconProviderHandle, pack_name: &str, icon_name: &str, image: ImageRef) {
    // Get dimensions from ImageRef
    let size = image.get_size();
    let data = ImageIconData { 
        image, 
        width: size.width, 
        height: size.height,
    };
    provider.register_icon(pack_name, icon_name, RefAny::new(data));
}

/// Register icons from a ZIP file (file names become icon names)
#[cfg(feature = "zip")]
pub fn register_icons_from_zip(provider: &mut IconProviderHandle, pack_name: &str, zip_bytes: &[u8]) {
    for (icon_name, image, width, height) in load_images_from_zip(zip_bytes) {
        let data = ImageIconData { image, width, height };
        provider.register_icon(pack_name, &icon_name, RefAny::new(data));
    }
}

#[cfg(not(feature = "zip"))]
pub fn register_icons_from_zip(_provider: &mut IconProviderHandle, _pack_name: &str, _zip_bytes: &[u8]) {
    // ZIP support not enabled
}

/// Register a font icon in a pack
pub fn register_font_icon(provider: &mut IconProviderHandle, pack_name: &str, icon_name: &str, font: FontRef, icon_char: &str) {
    let data = FontIconData { 
        font, 
        icon_char: icon_char.to_string() 
    };
    provider.register_icon(pack_name, icon_name, RefAny::new(data));
}

// ============================================================================
// ZIP Support
// ============================================================================

/// Load all images from a ZIP file, returning (`icon_name`, `ImageRef`, width, height)
#[cfg(all(feature = "zip", feature = "image_decoding"))]
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/counter/fixed-point cast
fn load_images_from_zip(zip_bytes: &[u8]) -> Vec<(String, ImageRef, f32, f32)> {
    use crate::zip::{ZipFile, ZipReadConfig};
    use crate::image::decode::{decode_raw_image_from_any_bytes, ResultRawImageDecodeImageError};
    use std::path::Path;
    
    let mut result = Vec::new();
    let config = ZipReadConfig::default();
    let Ok(entries) = ZipFile::list(zip_bytes, &config) else {
        return result;
    };
    
    for entry in &entries {
        if entry.path.ends_with('/') { continue; } // Skip directories
        
        let Ok(Some(file_bytes)) = ZipFile::get_single_file(zip_bytes, entry, &config) else {
            continue;
        };
        
        // Decode as image
        if let ResultRawImageDecodeImageError::Ok(raw_image) = decode_raw_image_from_any_bytes(&file_bytes) {
            // Icon name = filename without extension
            let path = Path::new(&entry.path);
            let icon_name = path.file_stem()
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
    use material_icons::{ALL_ICONS, icon_to_char, icon_to_html_name};
    
    // Register all Material Icons with their Unicode codepoints
    for icon in &ALL_ICONS {
        let icon_char = icon_to_char(*icon);
        let name = icon_to_html_name(icon);
        
        let data = FontIconData {
            font: font.clone(),
            icon_char: icon_char.to_string(),
        };
        provider.register_icon("material-icons", name, RefAny::new(data));
    }
}

#[cfg(not(feature = "icons"))]
pub fn register_material_icons(_provider: &mut IconProviderHandle, _font: FontRef) {
    // Icons feature not enabled
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
    use crate::font::parsed::ParsedFont;
    use crate::parsed_font_to_font_ref;

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
    // Icons or text_layout feature not enabled
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
        let original = StyledDom::default();
        
        let result = default_icon_resolver(OptionRefAny::None, &original, &style);
        
        // Without data, should return empty div StyledDom
        assert_eq!(result.node_data.as_ref().len(), 1);
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
    use azul_core::{
        a11y::SmallAriaInfo,
        dom::NodeType,
        resources::RawImageFormat,
    };
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
        ImageIconData {
            image: null_img(1, 1),
            width,
            height,
        }
    }

    fn font_icon(icon_char: &str) -> FontIconData {
        FontIconData {
            font: dummy_font_ref(),
            icon_char: icon_char.to_string(),
        }
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

    /// A "normal" original icon DOM: a single div carrying `props` as inline style.
    fn original_with(props: Vec<CssPropertyWithConditions>) -> StyledDom {
        let mut dom = Dom::create_div();
        dom.root
            .set_css_props(CssPropertyWithConditionsVec::from_vec(props));
        StyledDom::create(&mut dom, Css::empty())
    }

    /// A degenerate `StyledDom` with **zero** nodes — drives the `else` branch of
    /// `create_{image,font}_icon_from_original`, which `StyledDom::default()` never
    /// reaches (it always has a body node).
    fn original_without_nodes() -> StyledDom {
        StyledDom {
            node_data: Vec::new().into(),
            ..StyledDom::default()
        }
    }

    /// Every inline property on every node of the result, in document order.
    /// (Collected across all nodes rather than `node_data[0]` so the assertions
    /// survive any future anonymous-node insertion in `StyledDom::create`.)
    fn all_props(dom: &StyledDom) -> Vec<CssPropertyWithConditions> {
        dom.node_data
            .as_ref()
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

    fn width_px(dom: &StyledDom) -> Option<f32> {
        all_props(dom).into_iter().find_map(|p| match p.property {
            CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::Px(px))) => Some(px.number.get()),
            _ => None,
        })
    }

    fn height_px(dom: &StyledDom) -> Option<f32> {
        all_props(dom).into_iter().find_map(|p| match p.property {
            CssProperty::Height(CssPropertyValue::Exact(LayoutHeight::Px(px))) => {
                Some(px.number.get())
            }
            _ => None,
        })
    }

    fn count_widths(dom: &StyledDom) -> usize {
        all_props(dom)
            .iter()
            .filter(|p| matches!(p.property, CssProperty::Width(_)))
            .count()
    }

    fn text_of(dom: &StyledDom) -> Option<String> {
        dom.node_data
            .as_ref()
            .iter()
            .find_map(|nd| match nd.get_node_type() {
                NodeType::Text(t) => Some(t.as_str().to_string()),
                _ => None,
            })
    }

    fn has_image_node(dom: &StyledDom) -> bool {
        dom.node_data
            .as_ref()
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

    fn resolve(data: RefAny, original: &StyledDom, style: &SystemStyle) -> StyledDom {
        default_icon_resolver(OptionRefAny::Some(data), original, style)
    }

    // ---------------------------------------------------------------------
    // default_icon_resolver — dispatch
    // ---------------------------------------------------------------------

    #[test]
    fn resolver_none_yields_single_unstyled_div() {
        let out = default_icon_resolver(
            OptionRefAny::None,
            &StyledDom::default(),
            &SystemStyle::default(),
        );
        assert_eq!(out.node_data.as_ref().len(), 1);
        assert!(matches!(
            out.node_data.as_ref()[0].get_node_type(),
            NodeType::Div
        ));
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
        let out = resolve(data, &StyledDom::default(), &SystemStyle::default());

        assert_eq!(out.node_data.as_ref().len(), 1);
        assert!(matches!(
            out.node_data.as_ref()[0].get_node_type(),
            NodeType::Div
        ));
        assert!(!has_image_node(&out));
        assert!(text_of(&out).is_none());
    }

    #[test]
    fn resolver_image_icon_yields_image_node_with_default_dimensions() {
        let out = resolve(
            RefAny::new(image_icon(32.0, 24.0)),
            &StyledDom::default(),
            &SystemStyle::default(),
        );
        assert!(has_image_node(&out));
        assert_eq!(width_px(&out), Some(32.0));
        assert_eq!(height_px(&out), Some(24.0));
    }

    #[test]
    fn resolver_font_icon_yields_text_node_with_font_family() {
        let font = dummy_font_ref();
        let data = RefAny::new(FontIconData {
            font: font.clone(),
            icon_char: "\u{e88a}".to_string(),
        });
        let out = resolve(data, &StyledDom::default(), &SystemStyle::default());

        assert_eq!(out.node_data.as_ref().len(), 1);
        assert_eq!(text_of(&out).as_deref(), Some("\u{e88a}"));

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
        let out = resolve(RefAny::new(font_icon("A")), &original, &SystemStyle::default());
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
            &StyledDom::default(),
            &SystemStyle::default(),
        );
        let (w, h) = (
            width_px(&out).expect("width emitted"),
            height_px(&out).expect("height emitted"),
        );
        assert!(w.is_finite() && h.is_finite(), "NaN must not survive into CSS");
        assert_eq!(w, 0.0);
        assert_eq!(h, 0.0);
    }

    #[test]
    fn image_icon_infinite_dimensions_saturate_to_finite_values() {
        let out = resolve(
            RefAny::new(image_icon(f32::INFINITY, f32::NEG_INFINITY)),
            &StyledDom::default(),
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
            &StyledDom::default(),
            &SystemStyle::default(),
        );
        assert_eq!(width_px(&out), Some(-32.0));
        assert_eq!(height_px(&out), Some(-1.5));
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
        let out = resolve(data, &StyledDom::default(), &SystemStyle::default());

        let w = width_px(&out).expect("width emitted");
        assert!(w.is_finite() && w > 0.0, "usize::MAX size must saturate finitely, got {w}");
    }

    #[test]
    fn image_icon_zero_size_is_preserved() {
        let out = resolve(
            RefAny::new(image_icon(0.0, 0.0)),
            &StyledDom::default(),
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
        let mut dom = Dom::create_div().with_accessibility_info(SmallAriaInfo::label("Save").to_full_info());
        let original = StyledDom::create(&mut dom, Css::empty());

        let out = resolve(
            RefAny::new(image_icon(8.0, 8.0)),
            &original,
            &SystemStyle::default(),
        );

        let a11y = out
            .node_data
            .as_ref()
            .iter()
            .find_map(azul_core::dom::NodeData::get_accessibility_info)
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
        apply_icon_style_filters(&mut props, &SystemStyle::default());
        assert!(props.is_empty(), "default SystemStyle must not synthesise a filter");
    }

    #[test]
    fn icon_filters_grayscale_uses_quantised_luminance_matrix() {
        let mut props = Vec::new();
        apply_icon_style_filters(&mut props, &grayscale_style());

        let filters = filters_of(&props);
        assert_eq!(filters.len(), 1);
        let StyleFilter::ColorMatrix(m) = &filters[0] else {
            panic!("prefer_grayscale must emit a ColorMatrix filter, got {:?}", filters[0]);
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
    fn icon_filters_tint_emits_flood_even_when_fully_transparent() {
        // a == 0 is still forwarded — the resolver does not treat it as "no tint".
        let transparent = ColorU { r: 1, g: 2, b: 3, a: 0 };
        let mut props = Vec::new();
        apply_icon_style_filters(&mut props, &tint_style(transparent));

        let filters = filters_of(&props);
        assert_eq!(filters.len(), 1);
        assert!(matches!(filters[0], StyleFilter::Flood(c) if c == transparent));
    }

    #[test]
    fn icon_filters_grayscale_and_tint_are_ordered_matrix_then_flood() {
        let tint = ColorU { r: 255, g: 0, b: 128, a: 255 };
        let mut style = grayscale_style();
        style.icon_style.tint_color = OptionColorU::Some(tint);

        let mut props = Vec::new();
        apply_icon_style_filters(&mut props, &style);

        // Both filters must live in ONE `filter:` declaration (a second declaration
        // would overwrite the first in the cascade, silently dropping the grayscale).
        let filter_decls = props
            .iter()
            .filter(|p| matches!(p.property, CssProperty::Filter(_)))
            .count();
        assert_eq!(filter_decls, 1);

        let filters = filters_of(&props);
        assert_eq!(filters.len(), 2);
        assert!(matches!(filters[0], StyleFilter::ColorMatrix(_)));
        assert!(matches!(filters[1], StyleFilter::Flood(c) if c == tint));
    }

    #[test]
    fn icon_filters_preserve_pre_existing_properties() {
        let mut props = vec![CssPropertyWithConditions::simple(CssProperty::width(
            LayoutWidth::px(4.0),
        ))];
        apply_icon_style_filters(&mut props, &grayscale_style());

        assert_eq!(props.len(), 2);
        assert!(matches!(props[0].property, CssProperty::Width(_)), "existing props must not be clobbered");
        assert!(matches!(props[1].property, CssProperty::Filter(_)));
    }

    #[test]
    fn image_icon_grayscale_reaches_the_resolved_dom() {
        let out = resolve(
            RefAny::new(image_icon(10.0, 10.0)),
            &StyledDom::default(),
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
        apply_font_icon_color(&mut props, &SystemStyle::default());
        assert!(props.is_empty());
    }

    #[test]
    fn font_icon_color_inherit_text_color_alone_is_a_noop() {
        // Documented: inheritance is CSS's default, so `inherit_text_color` must
        // *not* synthesise a `color:` declaration (that would break inheritance).
        let mut style = SystemStyle::default();
        style.icon_style.inherit_text_color = true;
        let mut props = Vec::new();
        apply_font_icon_color(&mut props, &style);
        assert!(props.is_empty());
    }

    #[test]
    fn font_icon_color_tint_becomes_text_color() {
        let tint = ColorU { r: 9, g: 8, b: 7, a: 6 };
        let mut props = Vec::new();
        apply_font_icon_color(&mut props, &tint_style(tint));

        assert_eq!(props.len(), 1);
        assert_eq!(text_color_of(&props), Some(tint));
    }

    #[test]
    fn font_icon_color_tint_wins_over_inherit_text_color() {
        let tint = ColorU { r: 1, g: 1, b: 1, a: 255 };
        let mut style = tint_style(tint);
        style.icon_style.inherit_text_color = true;

        let mut props = Vec::new();
        apply_font_icon_color(&mut props, &style);
        assert_eq!(text_color_of(&props), Some(tint));
    }

    #[test]
    fn font_icons_never_get_a_grayscale_filter() {
        // Font icons take the color path, not the filter path — a ColorMatrix here
        // would double-apply on top of the (inherited) text color.
        let out = resolve(
            RefAny::new(font_icon("\u{e88a}")),
            &StyledDom::default(),
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
            &StyledDom::default(),
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
                &StyledDom::default(),
                &SystemStyle::default(),
            );
            assert_eq!(text_of(&out).as_deref(), Some(s), "icon_char {s:?} was altered");
        }
    }

    #[test]
    fn font_icon_huge_char_string_does_not_panic() {
        let huge = "\u{e88a}".repeat(65_536);
        let out = resolve(
            RefAny::new(font_icon(&huge)),
            &StyledDom::default(),
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
        let out = resolve(data, &StyledDom::default(), &SystemStyle::default());
        assert_eq!(width_px(&out), Some(64.0));
        assert_eq!(height_px(&out), Some(32.0));
    }

    #[test]
    fn register_font_icon_accepts_empty_pack_and_icon_names() {
        let mut provider = create_default_icon_provider();
        register_font_icon(&mut provider, "", "", dummy_font_ref(), "");

        assert_eq!(provider.list_packs(), vec![String::new()]);
        assert!(provider.has_icon(""));
        let data = provider.lookup("").expect("empty-named icon is still addressable");
        let out = resolve(data, &StyledDom::default(), &SystemStyle::default());
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
    fn duplicate_icon_across_packs_resolves_to_the_alphabetically_first_pack() {
        // "First match wins" iterates a BTreeMap => pack *name* order, NOT the
        // registration order. Registering into "zzz" first must not shadow "aaa".
        let mut provider = create_default_icon_provider();
        register_image_icon(&mut provider, "zzz", "dup", null_img(1, 1));
        register_image_icon(&mut provider, "aaa", "dup", null_img(2, 2));

        let data = provider.lookup("dup").expect("icon registered");
        let out = resolve(data, &StyledDom::default(), &SystemStyle::default());
        assert_eq!(
            width_px(&out),
            Some(2.0),
            "lookup must return the alphabetically-first pack's icon"
        );
    }

    #[test]
    fn re_registering_an_icon_replaces_it_and_unregistering_drops_the_empty_pack() {
        let mut provider = create_default_icon_provider();
        register_image_icon(&mut provider, "p", "icon", null_img(1, 1));
        register_image_icon(&mut provider, "p", "ICON", null_img(5, 5));

        assert_eq!(provider.list_icons_in_pack("p").len(), 1);
        let data = provider.lookup("icon").expect("icon registered");
        let out = resolve(data, &StyledDom::default(), &SystemStyle::default());
        assert_eq!(width_px(&out), Some(5.0));

        provider.unregister_icon("p", "IcOn");
        assert!(!provider.has_icon("icon"));
        assert!(provider.list_packs().is_empty(), "empty pack must be removed");
    }

    #[test]
    fn create_default_icon_provider_starts_empty_and_misses_resolve_to_a_placeholder() {
        let provider = create_default_icon_provider();
        assert!(provider.list_packs().is_empty());
        assert!(provider.lookup("nope").is_none());
        assert!(!provider.has_icon("nope"));

        let out = default_icon_resolver(
            OptionRefAny::from(provider.lookup("nope")),
            &StyledDom::default(),
            &SystemStyle::default(),
        );
        assert_eq!(out.node_data.as_ref().len(), 1);
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
        assert!(names.len() > 1000, "expected the full icon set, got {}", names.len());
        assert!(
            names.iter().all(|n| *n == n.to_lowercase()),
            "every registered icon name must be normalised to lowercase"
        );
        assert!(provider.has_icon("home"));
        assert!(provider.has_icon("HOME"));

        let data = provider.lookup("home").expect("material 'home' icon");
        let out = resolve(data, &StyledDom::default(), &SystemStyle::default());
        assert!(text_of(&out).is_some(), "a material icon must resolve to a text node");
    }
}
