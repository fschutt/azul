use azul_impl::{
    dom::{
        TabIndex, Dom, IdOrClass, IdOrClass::Class,
        NodeDataInlineCssProperty,
        NodeDataInlineCssProperty::{Normal, Hover, Active, Focus},
    },
    image::ImageRef,
    css::*,
    str::String as AzString,
    callbacks::{CallbackType, Update, CallbackInfo, RefAny},
    vec::{IdOrClassVec, StyleBackgroundContentVec, NodeDataInlineCssPropertyVec},
};

static CHECKBOX_CONTAINER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-checkbox-container"))];
static CHECKBOX_CONTENT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-checkbox-content"))];

pub type CheckBoxOnToggleCallback = extern "C" fn(&mut RefAny, &CheckBoxState, &mut CallbackInfo) -> Update;

#[repr(C)]
pub struct CheckBoxOnToggleCallback {
    pub cb: CheckboxCallback,
}

impl_callback!(CheckBoxOnToggleCallback);

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct CheckBox {
    pub state: CheckBoxStateWrapper,
    /// Style for the checkbox container
    pub container_style: NodeDataInlineCssPropertyVec,
    /// Style for the checkbox content
    pub content_style: NodeDataInlineCssPropertyVec,
}

#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct CheckBoxStateWrapper {
    /// Content (image or text) of this CheckBox, centered by default
    pub inner: CheckBoxState,
    /// Optional: Function to call when the CheckBox is toggled
    pub on_toggle: OptionCheckBoxOnToggle,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct CheckBoxOnToggle {
    pub data: RefAny,
    pub callback: CheckBoxOnToggleCallback,
}

impl_option!(CheckBoxOnToggle, OptionCheckBoxOnToggle, [Debug, Clone, PartialEq, PartialOrd]);

#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct CheckBoxState {
    pub checked: bool,
}

const BACKGROUND_COLOR: ColorU = ColorU { r: 255,  g: 255,  b: 255,  a: 255 }; // white
const BACKGROUND_THEME_LIGHT: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(BACKGROUND_COLOR)];
const BACKGROUND_COLOR_LIGHT: StyleBackgroundContentVec = StyleBackgroundContentVec::from_const_slice(BACKGROUND_THEME_LIGHT);
const COLOR_9B9B9B: ColorU = ColorU { r: 155, g: 155, b: 155, a: 255 }; // #9b9b9b

const FILL_COLOR: ColorU = ColorU { r: 155, g: 155, b: 155, a: 255 }; // #9b9b9b
const FILL_THEME: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(FILL_COLOR)];
const FILL_COLOR_BACKGROUND: StyleBackgroundContentVec = StyleBackgroundContentVec::from_const_slice(FILL_THEME);

static DEFAULT_CHECKBOX_CONTAINER_STYLE: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_background_content(BACKGROUND_COLOR_LIGHT)),

    // padding: 2px

    Normal(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(2))),
    Normal(CssProperty::const_padding_right(LayoutPaddingRight::const_px(2))),
    Normal(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
    Normal(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(2))),

    // border: 1px solid #484c52;

    Normal(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
    Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Normal(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
    Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),

    Normal(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Inset })),

    Normal(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_9B9B9B })),

    Normal(CssProperty::const_cursor(StyleCursor::Pointer)),
];

static DEFAULT_CHECKBOX_CONTENT_STYLE_CHECKED: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_width(LayoutWidth::const_px(12))),
    Normal(CssProperty::const_height(LayoutHeight::const_px(12))),
    Normal(CssProperty::const_background_content(FILL_COLOR_BACKGROUND)),
    Normal(CssProperty::const_opacity(StyleOpacity::const_new(100))),

    // padding: 2px
];

static DEFAULT_CHECKBOX_CONTENT_STYLE_UNCHECKED: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_width(LayoutWidth::const_px(12))),
    Normal(CssProperty::const_height(LayoutHeight::const_px(12))),
    Normal(CssProperty::const_background_content(FILL_COLOR_BACKGROUND)),
    Normal(CssProperty::const_opacity(StyleOpacity::const_new(0))),

    // padding: 2px
];

impl CheckBox {

    pub fn new(checked: bool) -> Self {
        Self {
            state: CheckBoxStateWrapper {
                inner: CheckBoxState { checked },
                .. Default::default()
            },
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(DEFAULT_CHECKBOX_CONTAINER_STYLE),
            content_style: if checked {
                NodeDataInlineCssPropertyVec::from_const_slice(DEFAULT_CHECKBOX_CONTENT_STYLE_CHECKED)
            } else {
                NodeDataInlineCssPropertyVec::from_const_slice(DEFAULT_CHECKBOX_CONTENT_STYLE_UNCHECKED)
            },
        }
    }

    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(false);
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_toggle(&mut self, on_toggle: CheckboxCallback, data: RefAny) {
        self.state.on_toggle = Some(CheckBoxOnToggleFn {
            callback: CheckBoxCallbackFn { cb: on_toggle },
            data,
        }).into();
    }

    #[inline]
    pub fn dom(self) -> Dom {

        use azul::vec::DomVec;
        use azul::dom::{
            Dom, EventFilter, HoverEventFilter,
            CallbackData,
        };
        use azul::callbacks::Callback;

        Dom::div()
        .with_ids_and_classes(IdOrClassVec::from(CHECKBOX_CONTAINER_CLASS))
        .with_inline_css_props(self.container_style)
        .with_callbacks(vec![
            CallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: Callback { cb: self::input::default_on_checkbox_clicked },
                data: RefAny::new(self.state),
            }
        ].into())
        .with_tab_index(TabIndex::Auto)
        .with_children(vec![
            Dom::div()
            .with_ids_and_classes(IdOrClassVec::from(CHECKBOX_CONTENT_CLASS))
            .with_inline_css_props(self.content_style)

        ].into())
    }
}

// handle input events for the checkbox
mod input {

    use azul::callbacks::{RefAny, CallbackInfo, Update};
    use super::CheckBoxStateWrapper;
    use azul::css::{CssProperty, StyleOpacity};

    pub(in super) extern "C" fn default_on_checkbox_clicked(check_box: &mut RefAny, mut info: CallbackInfo) -> Update {

        let mut check_box = match check_box.downcast_mut::<CheckBoxStateWrapper>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };

        let checkbox_content_id = match info.get_first_child(info.get_hit_node()).into_option() {
            Some(s) => s,
            None => return Update::DoNothing,
        };

        check_box.inner.checked = !check_box.inner.checked;

        let result = {
            // rustc doesn't understand the borrowing lifetime here
            let check_box = &mut *check_box;
            let ontoggle = &mut check_box.on_toggle;
            let inner = &check_box.inner;

            match ontoggle.as_mut() {
                Some(CheckBoxOnToggleFn { callback, data }) => (callback)(data, &inner, &mut info),
                None => Update::DoNothing,
            }
        };

        if check_box.inner.checked {
            info.set_css_property(checkbox_content_id, CssProperty::const_opacity(StyleOpacity::const_new(100)));
        } else {
            info.set_css_property(checkbox_content_id, CssProperty::const_opacity(StyleOpacity::const_new(0)));
        }

        result
    }
}

impl From<CheckBox> for Dom {
    fn from(b: CheckBox) -> Dom {
        b.dom()
    }
}
