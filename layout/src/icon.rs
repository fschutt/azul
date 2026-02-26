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
    AzString, OptionString, 
    system::SystemStyle,
    props::basic::{FontRef, StyleFontFamily, StyleFontFamilyVec},
    props::basic::font::StyleFontSize,
    props::basic::color::ColorU,
    props::basic::length::FloatValue,
    props::layout::{LayoutWidth, LayoutHeight},
    props::property::CssProperty,
    props::style::filter::{StyleFilter, StyleFilterVec, StyleColorMatrix},
    props::style::text::StyleTextColor,
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    css::{Css, CssPropertyValue},
};

use azul_core::{
    dom::{Dom, NodeData, NodeType, AccessibilityInfo, AccessibilityRole, OptionDomNodeId, AccessibilityStateVec},
    icon::{IconProviderHandle, IconResolverCallbackType},
    refany::{OptionRefAny, RefAny},
    resources::ImageRef,
    styled_dom::StyledDom,
    window::OptionVirtualKeyCodeCombo,
};

// ============================================================================
// Icon Data Marker Structs (for RefAny::downcast)
// ============================================================================

/// Marker for image-based icon data stored in RefAny
pub struct ImageIconData {
    pub image: ImageRef,
    /// Width duplicated from ImageRef at registration time
    pub width: f32,
    /// Height duplicated from ImageRef at registration time
    pub height: f32,
}

/// Marker for font-based icon data stored in RefAny
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
/// 1. If icon_data is None -> return empty div (icon not found)
/// 2. If icon_data contains ImageIconData -> render as image
/// 3. If icon_data contains FontIconData -> render as text with font
/// 4. Unknown data type -> return empty div
///
/// Styles from the original icon DOM are copied to the result,
/// filtered based on SystemStyle preferences.
pub extern "C" fn default_icon_resolver(
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
        return create_image_icon_from_original(&*img, original_icon_dom, system_style);
    }
    
    // Try FontIconData
    if let Some(font_icon) = data.downcast_ref::<FontIconData>() {
        return create_font_icon_from_original(&*font_icon, original_icon_dom, system_style);
    }
    
    // Unknown data type → empty div
    let mut dom = Dom::create_div();
    StyledDom::create(&mut dom, Css::empty())
}

// Icon DOM Creation (from original)

/// Create a StyledDom for an image-based icon, copying styles from original.
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
            dom = dom.with_accessibility_info(*a11y.clone());
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

/// Create a StyledDom for a font-based icon, copying styles from original.
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
            dom = dom.with_accessibility_info(*a11y.clone());
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
    let original_props = original_node.get_css_props();
    original_props.as_ref().iter().cloned().collect()
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
#[cfg(feature = "zip_support")]
pub fn register_icons_from_zip(provider: &mut IconProviderHandle, pack_name: &str, zip_bytes: &[u8]) {
    for (icon_name, image, width, height) in load_images_from_zip(zip_bytes) {
        let data = ImageIconData { image, width, height };
        provider.register_icon(pack_name, &icon_name, RefAny::new(data));
    }
}

#[cfg(not(feature = "zip_support"))]
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

/// Load all images from a ZIP file, returning (icon_name, ImageRef, width, height)
#[cfg(all(feature = "zip_support", feature = "image_decoding"))]
fn load_images_from_zip(zip_bytes: &[u8]) -> Vec<(String, ImageRef, f32, f32)> {
    use crate::zip::{ZipFile, ZipReadConfig};
    use crate::image::decode::{decode_raw_image_from_any_bytes, ResultRawImageDecodeImageError};
    use std::path::Path;
    
    let mut result = Vec::new();
    let config = ZipReadConfig::default();
    let entries = match ZipFile::list(zip_bytes, &config) {
        Ok(e) => e,
        Err(_) => return result,
    };
    
    for entry in entries.iter() {
        if entry.path.ends_with('/') { continue; } // Skip directories
        
        let file_bytes = match ZipFile::get_single_file(zip_bytes, entry, &config) {
            Ok(Some(b)) => b,
            _ => continue,
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

#[cfg(not(all(feature = "zip_support", feature = "image_decoding")))]
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
/// (e.g., "home", "settings", "arrow_back", etc.).
/// 
/// Requires the "icons" feature with material-icons crate.
#[cfg(feature = "icons")]
pub fn register_material_icons(provider: &mut IconProviderHandle, font: FontRef) {
    use material_icons::{ALL_ICONS, icon_to_char, icon_to_html_name};
    
    // Register all Material Icons with their Unicode codepoints
    for icon in ALL_ICONS.iter() {
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
#[cfg(all(feature = "icons", feature = "text_layout"))]
pub fn register_embedded_material_icons(provider: &mut IconProviderHandle) -> bool {
    use crate::font::parsed::ParsedFont;
    use crate::parsed_font_to_font_ref;
    
    // Get the embedded Material Icons font bytes from the material-icons crate
    let font_bytes: &'static [u8] = material_icons::FONT;
    
    // Parse the font
    let mut warnings = Vec::new();
    let parsed_font = match ParsedFont::from_bytes(font_bytes, 0, &mut warnings) {
        Some(f) => f,
        None => {
            return false;
        }
    };
    
    // Convert to FontRef
    let font_ref = parsed_font_to_font_ref(parsed_font);
    
    // Register all material icons
    register_material_icons(provider, font_ref);
    
    true
}

#[cfg(not(all(feature = "icons", feature = "text_layout")))]
pub fn register_embedded_material_icons(_provider: &mut IconProviderHandle) -> bool {
    // Icons or text_layout feature not enabled
    false
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Create an IconProviderHandle with the default resolver.
pub fn create_default_icon_provider() -> IconProviderHandle {
    IconProviderHandle::with_resolver(default_icon_resolver)
}

// ============================================================================
// Embedded Font Bytes Access
// ============================================================================

/// Returns the raw TTF bytes of the embedded Material Icons font,
/// or `None` if the `icons` feature is not enabled.
///
/// This is useful for serving the font over HTTP (e.g., in the debug server)
/// so that the debugger UI does not need an internet connection to Google Fonts.
#[cfg(feature = "icons")]
pub fn get_material_icons_font_bytes() -> Option<&'static [u8]> {
    Some(material_icons::FONT)
}

#[cfg(not(feature = "icons"))]
pub fn get_material_icons_font_bytes() -> Option<&'static [u8]> {
    None
}

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
