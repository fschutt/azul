use azul_core::{
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec},
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{
            color::{ColorU, ColorOrSystem, SystemColorRef},
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    system::SystemFontType,
    *,
};

/// A native-styled titlebar widget for custom window chrome.
/// 
/// This widget renders a centered title text using the system title font.
/// It is designed to be used with `WindowDecorations::NoTitle` where the
/// system only draws the window control buttons (close/minimize/maximize)
/// and the application draws its own title.
/// 
/// # Example
/// 
/// ```rust,no_run
/// use azul_layout::widgets::Titlebar;
/// 
/// let titlebar = Titlebar::create("My Application".into());
/// let dom = titlebar.dom();
/// ```
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Titlebar {
    /// The title text to display
    pub title: AzString,
    /// Height of the titlebar in pixels (default: 32.0)
    pub height: f32,
    /// Style for the titlebar container
    pub container_style: CssPropertyWithConditionsVec,
    /// Style for the title text
    pub title_style: CssPropertyWithConditionsVec,
}

// Default titlebar height on different platforms
#[cfg(target_os = "macos")]
const DEFAULT_TITLEBAR_HEIGHT: f32 = 28.0;

#[cfg(target_os = "windows")]
const DEFAULT_TITLEBAR_HEIGHT: f32 = 32.0;

#[cfg(target_os = "linux")]
const DEFAULT_TITLEBAR_HEIGHT: f32 = 30.0;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const DEFAULT_TITLEBAR_HEIGHT: f32 = 32.0;

// Default font size for title text
#[cfg(target_os = "macos")]
const DEFAULT_TITLE_FONT_SIZE: f32 = 13.0;

#[cfg(target_os = "windows")]
const DEFAULT_TITLE_FONT_SIZE: f32 = 12.0;

#[cfg(target_os = "linux")]
const DEFAULT_TITLE_FONT_SIZE: f32 = 13.0;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const DEFAULT_TITLE_FONT_SIZE: f32 = 13.0;

/// Build the default container style for the titlebar
fn build_container_style(height: f32) -> CssPropertyWithConditionsVec {
    let mut props = Vec::new();
    
    // Flex container, row direction
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_display(LayoutDisplay::Flex),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_flex_direction(LayoutFlexDirection::Row),
    ));
    
    // Center content horizontally and vertically
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_justify_content(LayoutJustifyContent::Center),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_align_items(LayoutAlignItems::Center),
    ));
    
    // Fixed height
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_height(LayoutHeight::const_px(height as isize)),
    ));
    
    // Transparent background (relies on window material)
    
    CssPropertyWithConditionsVec::from_vec(props)
}

/// Build the default style for the title text
fn build_title_style() -> CssPropertyWithConditionsVec {
    // Use system title font
    let font_family = StyleFontFamilyVec::from_vec(vec![
        StyleFontFamily::SystemType(SystemFontType::Title),
    ]);
    
    let mut props = Vec::new();
    
    // Font settings
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_font_size(StyleFontSize::const_px(DEFAULT_TITLE_FONT_SIZE as isize)),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_font_family(font_family),
    ));
    
    // Center text
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_text_align(StyleTextAlign::Center),
    ));
    
    // Use system text color (adapts to light/dark mode)
    // Note: For true dynamic theming, use ColorOrSystem with system colors in backgrounds,
    // but for text we use a standard dark gray that works well with most window materials
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_text_color(StyleTextColor {
            inner: ColorU { r: 76, g: 76, b: 76, a: 255 }, // #4C4C4C - works for both light/dark
        }),
    ));
    
    CssPropertyWithConditionsVec::from_vec(props)
}

impl Titlebar {
    /// Create a new titlebar with the given title text.
    #[inline]
    pub fn create(title: AzString) -> Self {
        let height = DEFAULT_TITLEBAR_HEIGHT;
        Self {
            title,
            height,
            container_style: build_container_style(height),
            title_style: build_title_style(),
        }
    }
    
    /// Create a titlebar with a custom height.
    #[inline]
    pub fn with_height(title: AzString, height: f32) -> Self {
        Self {
            title,
            height,
            container_style: build_container_style(height),
            title_style: build_title_style(),
        }
    }
    
    /// Set the titlebar height.
    #[inline]
    pub fn set_height(&mut self, height: f32) {
        self.height = height;
        self.container_style = build_container_style(height);
    }
    
    /// Set the title text.
    #[inline]
    pub fn set_title(&mut self, title: AzString) {
        self.title = title;
    }
    
    /// Swap the titlebar with a default instance, returning the old value.
    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Titlebar::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }
    
    /// Convert this titlebar into a DOM tree.
    #[inline]
    pub fn dom(self) -> Dom {
        static TITLEBAR_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-titlebar"))];
        static TITLE_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-titlebar-title"))];
        
        // Create the title text node
        let title_text = Dom::create_text(self.title)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TITLE_CLASS))
            .with_css_props(self.title_style);
        
        // Create the titlebar container
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TITLEBAR_CLASS))
            .with_css_props(self.container_style)
            .with_child(title_text)
    }
}

impl From<Titlebar> for Dom {
    fn from(t: Titlebar) -> Dom {
        t.dom()
    }
}

impl Default for Titlebar {
    fn default() -> Self {
        Titlebar::create(AzString::from_const_str(""))
    }
}
