//! Rectangular input that, when clicked, spawns a color dialog

use azul::css::*;
use azul::dom::{Dom, NodeDataInlineCssProperty, NodeDataInlineCssProperty::Normal};
use azul::callbacks::{UpdateScreen, CallbackInfo, RefAny};
use azul::str::String as AzString;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct ColorInput {
    pub state: ColorInputStateWrapper,
    pub style: Vec<NodeDataInlineCssProperty>,
}

pub type OnColorChangeCallback = extern "C" fn(&mut RefAny, &ColorInputState, &mut CallbackInfo) -> UpdateScreen;

pub struct OnColorChangeFn {
    pub cb: OnColorChangeCallback,
}

impl_callback!(OnColorChangeFn);

#[derive(Debug, Clone, PartialEq)]
pub struct ColorInputStateWrapper {
    pub inner: ColorInputState,
    pub title: AzString,
    pub on_value_change: Option<(OnColorChangeFn, RefAny)>
}

impl Default for ColorInputStateWrapper {
    fn default() -> Self {
        Self {
            inner: ColorInputState::default(),
            title: "Pick color".into(),
            on_value_change: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ColorInputState {
    pub color: ColorU,
}

impl Default for ColorInputState {
    fn default() -> Self {
        Self {
            color: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            }
        }
    }
}

impl ColorInput {

    pub fn new(color: ColorU) -> Self {
        Self {
            state: ColorInputStateWrapper {
                inner: ColorInputState {
                    color,
                    .. Default::default()
                },
                .. Default::default()
            },
            style: vec![
                Normal(CssProperty::flex_grow(LayoutFlexGrow::const_new(1))),
                Normal(CssProperty::min_width(LayoutMinWidth::const_px(15))),
                Normal(CssProperty::min_height(LayoutMinHeight::const_px(15))),
                Normal(CssProperty::cursor(StyleCursor::Pointer)),
            ],
        }
    }

    pub fn on_value_change(mut self, callback: OnColorChangeCallback, data: RefAny) -> Self {
        self.state.on_value_change = Some((OnColorChangeFn { cb: callback }, data));
        self
    }

    pub fn dom(mut self) -> Dom {

        use azul::callbacks::Callback;
        use azul::dom::{
            EventFilter, HoverEventFilter,
            IdOrClass::Class, CallbackData,
        };

        self.style.push(Normal(CssProperty::background_content(vec![
            StyleBackgroundContent::Color(self.state.inner.color)
        ].into())));

        Dom::div()
        .with_ids_and_classes(vec![Class("__azul_native_color_input".into())].into())
        .with_inline_css_props(self.style.into())
        .with_callbacks(vec![
            CallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                data: RefAny::new(self.state),
                callback: Callback { cb: on_color_input_clicked }
            }
        ].into())
    }
}

extern "C" fn on_color_input_clicked(data: &mut RefAny, mut info: CallbackInfo) -> UpdateScreen {

    use azul::dialog::ColorPickerDialog;

    let mut color_input = match data.downcast_mut::<ColorInputStateWrapper>() {
        Some(s) => s,
        None => return UpdateScreen::DoNothing,
    };

    // open the color picker dialog
    let new_color = match ColorPickerDialog::open(
        color_input.title.clone(),
        Some(color_input.inner.color).into()
    ).into_option() {
        Some(s) => s,
        None => return UpdateScreen::DoNothing,
    };

    // Update the color in the data and the screen
    color_input.inner.color = new_color;
    info.set_css_property(info.get_hit_node(), CssProperty::background_content(
        vec![StyleBackgroundContent::Color(new_color)].into(),
    ));

    let result = {
        let color_input = &mut *color_input;
        let onvaluechange = &mut color_input.on_value_change;
        let inner = &color_input.inner;

        match onvaluechange.as_mut() {
            Some((f, d)) => (f.cb)(d, &inner, &mut info),
            None => UpdateScreen::DoNothing,
        }
    };

    result
}