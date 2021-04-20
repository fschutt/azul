use azul::{
    dom::{
        Dom, NodeDataInlineCssProperty,
        NodeDataInlineCssProperty::{Normal, Focus}
    },
    css::*,
    vec::{StyleFontFamilyVec, NodeDataInlineCssPropertyVec},
    str::String as AzString,
};

#[derive(Debug, Clone)]
pub struct Label {
    pub string: AzString,
    pub label_style: NodeDataInlineCssPropertyVec,
}

const SANS_SERIF_STR: &str = "sans-serif";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec = StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

const COLOR_4C4C4C: ColorU = ColorU { r: 76, g: 76, b: 76, a: 255 }; // #4C4C4C

static LABEL_STYLE_WINDOWS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::display(LayoutDisplay::Flex)),
    Normal(CssProperty::flex_direction(LayoutFlexDirection::Column)),
    Normal(CssProperty::justify_content(LayoutJustifyContent::Center)),
    Normal(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),

    Normal(CssProperty::text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::text_align(StyleTextAlign::Center)),
    Normal(CssProperty::font_family(SANS_SERIF_FAMILY)),
];

static LABEL_STYLE_LINUX: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::display(LayoutDisplay::Flex)),
    Normal(CssProperty::flex_direction(LayoutFlexDirection::Column)),
    Normal(CssProperty::justify_content(LayoutJustifyContent::Center)),
    Normal(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),

    Normal(CssProperty::text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::text_align(StyleTextAlign::Center)),
    Normal(CssProperty::font_family(SANS_SERIF_FAMILY)),
];

static LABEL_STYLE_MAC: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::display(LayoutDisplay::Flex)),
    Normal(CssProperty::flex_direction(LayoutFlexDirection::Column)),
    Normal(CssProperty::justify_content(LayoutJustifyContent::Center)),
    Normal(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),

    Normal(CssProperty::text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::font_size(StyleFontSize::const_px(12))),
    Normal(CssProperty::text_align(StyleTextAlign::Center)),
    Normal(CssProperty::font_family(SANS_SERIF_FAMILY)),
];

static LABEL_STYLE_OTHER: &[NodeDataInlineCssProperty] = &[
];

impl Label {

    #[inline]
    pub fn new<S: Into<AzString>>(string: S) -> Self {
        Self {
            string: string.into(),
            #[cfg(target_os = "windows")]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(LABEL_STYLE_WINDOWS),
            #[cfg(target_os = "linux")]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(LABEL_STYLE_LINUX),
            #[cfg(target_os = "macos")]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(LABEL_STYLE_MAC),
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "mac")))]
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(LABEL_STYLE_OTHER),
        }
    }

    #[inline]
    pub fn dom(self) -> Dom {

        use azul::vec::{IdOrClassVec};
        use azul::dom::{Dom, IdOrClass, IdOrClass::Class};

        static LABEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-label"))];

        Dom::text(self.string)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
        .with_inline_css_props(self.label_style)
    }
}

impl From<Label> for Dom {
    fn from(l: Label) -> Dom {
        l.dom()
    }
}