//! Switch (toggle) widget — a boolean on/off control rendered as a rounded,
//! pill-shaped "track" with a sliding circular "knob". A near-clone of
//! [`crate::widgets::check_box::CheckBox`] (boolean state + an `on_toggle`
//! callback) restyled as a switch: toggling flips the knob's horizontal
//! position (via `margin-left`) and the track's background colour.
//!
//! Key types: [`Switch`], [`SwitchState`], [`SwitchOnToggle`].

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, *},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutWidth, LayoutHeight, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPaddingBottom, LayoutMarginLeft},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleCursor},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static SWITCH_TRACK_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-switch"))];
static SWITCH_KNOB_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-switch-knob"))];

/// Callback function type invoked when the switch is toggled.
pub type SwitchOnToggleCallbackType = extern "C" fn(RefAny, CallbackInfo, SwitchState) -> Update;
impl_widget_callback!(
    SwitchOnToggle,
    OptionSwitchOnToggle,
    SwitchOnToggleCallback,
    SwitchOnToggleCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        SwitchOnToggleCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SWITCH_ON_TOGGLE_INVOKER,
    invoker_ty:     AzSwitchOnToggleCallbackInvoker,
    thunk_fn:       az_switch_on_toggle_callback_thunk,
    setter_fn:      AzApp_setSwitchOnToggleCallbackInvoker,
    from_handle_fn: AzSwitchOnToggleCallback_createFromHostHandle,
    extra_args:     [ state: SwitchState ],
}

/// A toggleable on/off switch widget with a sliding knob and toggle callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Switch {
    pub switch_state: SwitchStateWrapper,
    /// Style for the switch track (the pill-shaped container)
    pub track_style: CssPropertyWithConditionsVec,
    /// Style for the sliding knob
    pub knob_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct SwitchStateWrapper {
    /// On/off state of this Switch
    pub inner: SwitchState,
    /// Optional: function to call when the Switch is toggled
    pub on_toggle: OptionSwitchOnToggle,
}

/// The on/off state of a [`Switch`].
#[derive(Copy, Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct SwitchState {
    /// `true` = on (knob slid right), `false` = off (knob at left)
    pub checked: bool,
}

// ---- dimensions ----
const TRACK_WIDTH: isize = 36;
const TRACK_HEIGHT: isize = 20;
const TRACK_PADDING: isize = 2;
const TRACK_RADIUS: isize = 10;
const KNOB_SIZE: isize = 16;
const KNOB_RADIUS: isize = 8;
/// Horizontal travel of the knob = `track_width` − 2·padding − `knob_size`.
const KNOB_TRAVEL: isize = TRACK_WIDTH - (2 * TRACK_PADDING) - KNOB_SIZE;

// ---- colours ----
const TRACK_OFF_COLOR: ColorU = ColorU {
    r: 204,
    g: 204,
    b: 204,
    a: 255,
}; // #cccccc
const TRACK_ON_COLOR: ColorU = ColorU {
    r: 76,
    g: 217,
    b: 100,
    a: 255,
}; // #4cd964
const KNOB_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
}; // white

const TRACK_OFF_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(TRACK_OFF_COLOR)];
const TRACK_OFF_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(TRACK_OFF_BG_ITEMS);
const TRACK_ON_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(TRACK_ON_COLOR)];
const TRACK_ON_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(TRACK_ON_BG_ITEMS);
const KNOB_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(KNOB_COLOR)];
const KNOB_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(KNOB_BG_ITEMS);

/// Build the track (pill container) style. Background colour is the only
/// state-dependent property, so the style is built at runtime per the recipe's
/// "runtime vec if param-dependent" path.
fn build_track_style(checked: bool) -> CssPropertyWithConditionsVec {
    let bg = if checked { TRACK_ON_BG } else { TRACK_OFF_BG };
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(
            TRACK_WIDTH,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
            TRACK_HEIGHT,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(TRACK_PADDING),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(TRACK_PADDING),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(TRACK_PADDING),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(TRACK_PADDING),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(TRACK_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(TRACK_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(TRACK_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(TRACK_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg)),
    ])
}

/// Build the knob style. The knob's `margin-left` is the state-dependent
/// property that slides it between the off (left) and on (right) positions.
fn build_knob_style(checked: bool) -> CssPropertyWithConditionsVec {
    let margin = if checked { KNOB_TRAVEL } else { 0 };
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(
            KNOB_SIZE,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(
            KNOB_SIZE,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(KNOB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(KNOB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(KNOB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(KNOB_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(KNOB_BG)),
        CssPropertyWithConditions::simple(CssProperty::const_margin_left(
            LayoutMarginLeft::const_px(margin),
        )),
    ])
}

impl Switch {
    /// Creates a new switch in the given on/off state with default styling.
    #[must_use] pub fn create(checked: bool) -> Self {
        Self {
            switch_state: SwitchStateWrapper {
                inner: SwitchState { checked },
                ..Default::default()
            },
            track_style: build_track_style(checked),
            knob_style: build_knob_style(checked),
        }
    }

    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(false);
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_toggle<C: Into<SwitchOnToggleCallback>>(&mut self, data: RefAny, on_toggle: C) {
        self.switch_state.on_toggle = Some(SwitchOnToggle {
            callback: on_toggle.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use] pub fn with_on_toggle<C: Into<SwitchOnToggleCallback>>(
        mut self,
        data: RefAny,
        on_toggle: C,
    ) -> Self {
        self.set_on_toggle(data, on_toggle);
        self
    }

    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{Dom, EventFilter, HoverEventFilter},
        };

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from(SWITCH_TRACK_CLASS))
            .with_css_props(self.track_style)
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb: input::default_on_switch_clicked as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(self.switch_state),
                }]
                .into(),
            )
            .with_tab_index(TabIndex::Auto)
            .with_children(
                vec![Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from(SWITCH_KNOB_CLASS))
                    .with_css_props(self.knob_style)]
                .into(),
            )
    }
}

impl Default for Switch {
    fn default() -> Self {
        Self::create(false)
    }
}

// handle input events for the switch
mod input {

    use azul_core::{callbacks::Update, refany::RefAny};
    use azul_css::props::{layout::LayoutMarginLeft, property::CssProperty};

    use super::{SwitchOnToggle, SwitchStateWrapper, KNOB_TRAVEL, TRACK_OFF_BG, TRACK_ON_BG};
    use crate::callbacks::CallbackInfo;

    pub(super) extern "C" fn default_on_switch_clicked(
        mut switch: RefAny,
        mut info: CallbackInfo,
    ) -> Update {
        let Some(mut switch) = switch.downcast_mut::<SwitchStateWrapper>() else {
            return Update::DoNothing;
        };

        let track_id = info.get_hit_node();
        let Some(knob_id) = info.get_first_child(track_id) else {
            return Update::DoNothing;
        };

        switch.inner.checked = !switch.inner.checked;

        let result = {
            // rustc doesn't understand the borrowing lifetime here
            let switch = &mut *switch;
            let on_toggle = &mut switch.on_toggle;
            let inner = switch.inner;

            match on_toggle.as_mut() {
                Some(SwitchOnToggle {
                    callback,
                    refany: data,
                }) => (callback.cb)(data.clone(), info, inner),
                None => Update::DoNothing,
            }
        };

        // CallbackInfo is Copy, so `info` is still usable after the call above.
        if switch.inner.checked {
            info.set_css_property(track_id, CssProperty::const_background_content(TRACK_ON_BG));
            info.set_css_property(
                knob_id,
                CssProperty::const_margin_left(LayoutMarginLeft::const_px(KNOB_TRAVEL)),
            );
        } else {
            info.set_css_property(track_id, CssProperty::const_background_content(TRACK_OFF_BG));
            info.set_css_property(
                knob_id,
                CssProperty::const_margin_left(LayoutMarginLeft::const_px(0)),
            );
        }

        result
    }
}

impl From<Switch> for Dom {
    fn from(s: Switch) -> Self {
        s.dom()
    }
}
