use azul_desktop::{
    dom::{
        Dom, NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec,
        NodeDataInlineCssProperty::{Normal, Focus}, IdOrClassVec,
        IdOrClass::Class, IdOrClass
    },
    css::*,
    css::AzString,
};

const SANS_SERIF_STR: &str = "sans-serif";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec = StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

const COLOR_06B025: ColorU = ColorU { r: 6, g: 176, b: 37, a: 255 }; // #06b025 green

static PROGRESS_BAR_STYLE_WINDOWS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::Flex)),
    Normal(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),
    Normal(CssProperty::const_align_items(LayoutAlignItems::Center)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),

    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::const_text_align(StyleTextAlign::Center)),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ProgressBar {
    pub state: ProgressBarState,
    pub container_style: NodeDataInlineCssPropertyVec,
    pub bar_style: NodeDataInlineCssPropertyVec,
    pub label_style: NodeDataInlineCssPropertyVec,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct ProgressBarState {
    pub percent_done: f32,
    pub display_percentage: bool,
}

impl ProgressBar {

    #[inline]
    pub fn new(percent_done: f32) -> Self {
        Self {
            state: ProgressBarState { percent_done, display_percentage: false },
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(PROGRESS_BAR_STYLE_WINDOWS),
            bar_style: NodeDataInlineCssPropertyVec::from_const_slice(PROGRESS_BAR_STYLE_WINDOWS),
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(PROGRESS_BAR_STYLE_WINDOWS),
        }
    }

    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(0.0);
        core::mem::swap(&mut s, self);
        s
    }

    pub fn dom(mut self) -> Dom {

    }
}