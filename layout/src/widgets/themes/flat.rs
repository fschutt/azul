use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{CoreCallbackData, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{
        Dom, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec, NodeType,
        TabIndex,
    },
    geom::{LogicalPosition, LogicalRect},
    refany::RefAny,
};
use azul_css::{css::BoxOrStatic, AzString};
#[allow(clippy::wildcard_imports)]
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::widgets::button::{Button, ButtonOnClick};

// ---------------------------------------------------------------------------
// The flat palette.
//
// THE TOKEN NAMES ARE THE SAME ONES `flora` USES, deliberately. A widget that
// reaches for `SUR` or `INK` instead of an inline `ColorU { r: 178, .. }` can
// then be written once and read correctly under either theme, which is the
// whole point of having themes rather than two hand-maintained copies of every
// widget. The VALUES are what makes this theme flat: plain surfaces, one thin
// border weight, and "gradient" stops that are simply equal.
//
// Names follow flora.css so the two stay comparable:
//   PG/SUR/DESK/STRIP/TRACK  surfaces, back to front
//   BD..BD5, SEP, SEP2       borders and separators
//   INK/INK2/INTRO/SOFT1..3  text, most to least prominent
//   RT/RB, HT/HB, PT/PB      raised / hover / pressed control faces
//   FLD/FLD2                 input fields
//   DISBG/DISTX              disabled
//   QT/QT2                   quiet (toolbar) fills
//   ACC/DEEP/SOFT/GLOW/ON_ACC accent ramp and what sits on it
// ---------------------------------------------------------------------------

// Light mode colors
/// Page: the window canvas behind everything.
pub const LIGHT_PG: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Surface: panels and cards sitting on the page.
pub const LIGHT_SUR: ColorU = ColorU {
    r: 248,
    g: 249,
    b: 250,
    a: 255,
};
/// Desk: the recessed area a document sits on.
pub const LIGHT_DESK: ColorU = ColorU {
    r: 241,
    g: 243,
    b: 245,
    a: 255,
};
/// Strip: toolbars and header bands.
pub const LIGHT_STRIP: ColorU = ColorU {
    r: 233,
    g: 236,
    b: 239,
    a: 255,
};
/// Track: scrollbar and slider grooves.
pub const LIGHT_TRACK: ColorU = ColorU {
    r: 222,
    g: 226,
    b: 230,
    a: 255,
};
/// Default border.
pub const LIGHT_BD: ColorU = ColorU {
    r: 206,
    g: 212,
    b: 218,
    a: 255,
};
/// Lighter border, for internal divisions.
pub const LIGHT_BD2: ColorU = ColorU {
    r: 222,
    g: 226,
    b: 230,
    a: 255,
};
/// Stronger border, for emphasis or focus.
pub const LIGHT_BD3: ColorU = ColorU {
    r: 173,
    g: 181,
    b: 189,
    a: 255,
};
/// Faintest border.
pub const LIGHT_BD4: ColorU = ColorU {
    r: 233,
    g: 236,
    b: 239,
    a: 255,
};
/// Border that reads as a highlight.
pub const LIGHT_BD5: ColorU = ColorU {
    r: 248,
    g: 249,
    b: 250,
    a: 255,
};
/// Separator line.
pub const LIGHT_SEP: ColorU = ColorU {
    r: 222,
    g: 226,
    b: 230,
    a: 255,
};
/// Fainter separator.
pub const LIGHT_SEP2: ColorU = ColorU {
    r: 233,
    g: 236,
    b: 239,
    a: 255,
};
/// Body text.
pub const LIGHT_INK: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
};
/// Secondary text.
pub const LIGHT_INK2: ColorU = ColorU {
    r: 73,
    g: 80,
    b: 87,
    a: 255,
};
/// Introductory / lead text.
pub const LIGHT_INTRO: ColorU = ColorU {
    r: 108,
    g: 117,
    b: 125,
    a: 255,
};
/// Muted text, still readable.
pub const LIGHT_SOFT1: ColorU = ColorU {
    r: 134,
    g: 142,
    b: 150,
    a: 255,
};
/// Muted text, hint level.
pub const LIGHT_SOFT2: ColorU = ColorU {
    r: 173,
    g: 181,
    b: 189,
    a: 255,
};
/// Barely-there text.
pub const LIGHT_SOFT3: ColorU = ColorU {
    r: 206,
    g: 212,
    b: 218,
    a: 255,
};
/// Icon glyphs.
pub const LIGHT_ICON: ColorU = ColorU {
    r: 73,
    g: 80,
    b: 87,
    a: 255,
};
/// Raised control, top of the face (flat: equal to RB).
pub const LIGHT_RT: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Raised control, bottom of the face.
pub const LIGHT_RB: ColorU = ColorU {
    r: 248,
    g: 249,
    b: 250,
    a: 255,
};
/// Hovered control, top of the face.
pub const LIGHT_HT: ColorU = ColorU {
    r: 241,
    g: 243,
    b: 245,
    a: 255,
};
/// Hovered control, bottom of the face.
pub const LIGHT_HB: ColorU = ColorU {
    r: 233,
    g: 236,
    b: 239,
    a: 255,
};
/// Pressed control, top of the face.
pub const LIGHT_PT: ColorU = ColorU {
    r: 222,
    g: 226,
    b: 230,
    a: 255,
};
/// Pressed control, bottom of the face.
pub const LIGHT_PB: ColorU = ColorU {
    r: 206,
    g: 212,
    b: 218,
    a: 255,
};
/// Input field fill.
pub const LIGHT_FLD: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Secondary field fill (readonly, inset).
pub const LIGHT_FLD2: ColorU = ColorU {
    r: 248,
    g: 249,
    b: 250,
    a: 255,
};
/// Disabled control fill.
pub const LIGHT_DISBG: ColorU = ColorU {
    r: 233,
    g: 236,
    b: 239,
    a: 255,
};
/// Disabled control text.
pub const LIGHT_DISTX: ColorU = ColorU {
    r: 173,
    g: 181,
    b: 189,
    a: 255,
};
/// Quiet fill, e.g. a toolbar button at rest.
pub const LIGHT_QT: ColorU = ColorU {
    r: 248,
    g: 249,
    b: 250,
    a: 255,
};
/// Quiet fill, one step stronger.
pub const LIGHT_QT2: ColorU = ColorU {
    r: 241,
    g: 243,
    b: 245,
    a: 255,
};
/// Accent.
pub const LIGHT_ACC: ColorU = ColorU {
    r: 13,
    g: 110,
    b: 253,
    a: 255,
};
/// Accent, pressed.
pub const LIGHT_DEEP: ColorU = ColorU {
    r: 10,
    g: 88,
    b: 202,
    a: 255,
};
/// Accent, muted.
pub const LIGHT_SOFT: ColorU = ColorU {
    r: 110,
    g: 168,
    b: 254,
    a: 255,
};
/// Accent as a focus glow (translucent).
pub const LIGHT_GLOW: ColorU = ColorU {
    r: 13,
    g: 110,
    b: 253,
    a: 64,
};
/// Text and icons ON an accent fill.
pub const LIGHT_ON_ACC: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

// Dark mode colors
/// Page: the window canvas behind everything.
pub const DARK_PG: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
};
/// Surface: panels and cards sitting on the page.
pub const DARK_SUR: ColorU = ColorU {
    r: 52,
    g: 58,
    b: 64,
    a: 255,
};
/// Desk: the recessed area a document sits on.
pub const DARK_DESK: ColorU = ColorU {
    r: 26,
    g: 29,
    b: 33,
    a: 255,
};
/// Strip: toolbars and header bands.
pub const DARK_STRIP: ColorU = ColorU {
    r: 43,
    g: 48,
    b: 53,
    a: 255,
};
/// Track: scrollbar and slider grooves.
pub const DARK_TRACK: ColorU = ColorU {
    r: 73,
    g: 80,
    b: 87,
    a: 255,
};
/// Default border.
pub const DARK_BD: ColorU = ColorU {
    r: 73,
    g: 80,
    b: 87,
    a: 255,
};
/// Lighter border, for internal divisions.
pub const DARK_BD2: ColorU = ColorU {
    r: 52,
    g: 58,
    b: 64,
    a: 255,
};
/// Stronger border, for emphasis or focus.
pub const DARK_BD3: ColorU = ColorU {
    r: 108,
    g: 117,
    b: 125,
    a: 255,
};
/// Faintest border.
pub const DARK_BD4: ColorU = ColorU {
    r: 43,
    g: 48,
    b: 53,
    a: 255,
};
/// Border that reads as a highlight.
pub const DARK_BD5: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
};
/// Separator line.
pub const DARK_SEP: ColorU = ColorU {
    r: 73,
    g: 80,
    b: 87,
    a: 255,
};
/// Fainter separator.
pub const DARK_SEP2: ColorU = ColorU {
    r: 52,
    g: 58,
    b: 64,
    a: 255,
};
/// Body text.
pub const DARK_INK: ColorU = ColorU {
    r: 248,
    g: 249,
    b: 250,
    a: 255,
};
/// Secondary text.
pub const DARK_INK2: ColorU = ColorU {
    r: 222,
    g: 226,
    b: 230,
    a: 255,
};
/// Introductory / lead text.
pub const DARK_INTRO: ColorU = ColorU {
    r: 173,
    g: 181,
    b: 189,
    a: 255,
};
/// Muted text, still readable.
pub const DARK_SOFT1: ColorU = ColorU {
    r: 134,
    g: 142,
    b: 150,
    a: 255,
};
/// Muted text, hint level.
pub const DARK_SOFT2: ColorU = ColorU {
    r: 108,
    g: 117,
    b: 125,
    a: 255,
};
/// Barely-there text.
pub const DARK_SOFT3: ColorU = ColorU {
    r: 73,
    g: 80,
    b: 87,
    a: 255,
};
/// Icon glyphs.
pub const DARK_ICON: ColorU = ColorU {
    r: 222,
    g: 226,
    b: 230,
    a: 255,
};
/// Raised control, top of the face (flat: equal to RB).
pub const DARK_RT: ColorU = ColorU {
    r: 60,
    g: 66,
    b: 73,
    a: 255,
};
/// Raised control, bottom of the face.
pub const DARK_RB: ColorU = ColorU {
    r: 52,
    g: 58,
    b: 64,
    a: 255,
};
/// Hovered control, top of the face.
pub const DARK_HT: ColorU = ColorU {
    r: 73,
    g: 80,
    b: 87,
    a: 255,
};
/// Hovered control, bottom of the face.
pub const DARK_HB: ColorU = ColorU {
    r: 60,
    g: 66,
    b: 73,
    a: 255,
};
/// Pressed control, top of the face.
pub const DARK_PT: ColorU = ColorU {
    r: 43,
    g: 48,
    b: 53,
    a: 255,
};
/// Pressed control, bottom of the face.
pub const DARK_PB: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
};
/// Input field fill.
pub const DARK_FLD: ColorU = ColorU {
    r: 43,
    g: 48,
    b: 53,
    a: 255,
};
/// Secondary field fill (readonly, inset).
pub const DARK_FLD2: ColorU = ColorU {
    r: 52,
    g: 58,
    b: 64,
    a: 255,
};
/// Disabled control fill.
pub const DARK_DISBG: ColorU = ColorU {
    r: 52,
    g: 58,
    b: 64,
    a: 255,
};
/// Disabled control text.
pub const DARK_DISTX: ColorU = ColorU {
    r: 108,
    g: 117,
    b: 125,
    a: 255,
};
/// Quiet fill, e.g. a toolbar button at rest.
pub const DARK_QT: ColorU = ColorU {
    r: 43,
    g: 48,
    b: 53,
    a: 255,
};
/// Quiet fill, one step stronger.
pub const DARK_QT2: ColorU = ColorU {
    r: 52,
    g: 58,
    b: 64,
    a: 255,
};
/// Accent.
pub const DARK_ACC: ColorU = ColorU {
    r: 59,
    g: 130,
    b: 246,
    a: 255,
};
/// Accent, pressed.
pub const DARK_DEEP: ColorU = ColorU {
    r: 37,
    g: 99,
    b: 235,
    a: 255,
};
/// Accent, muted.
pub const DARK_SOFT: ColorU = ColorU {
    r: 96,
    g: 165,
    b: 250,
    a: 255,
};
/// Accent as a focus glow (translucent).
pub const DARK_GLOW: ColorU = ColorU {
    r: 59,
    g: 130,
    b: 246,
    a: 80,
};
/// Text and icons ON an accent fill.
pub const DARK_ON_ACC: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

// The names this theme used before it had a full palette. Kept so existing
// callers keep compiling and keep meaning the same thing.
/// Deprecated alias for [`LIGHT_SUR`].
pub const LIGHT_BG: ColorU = LIGHT_SUR;
/// Deprecated alias for [`LIGHT_INK`].
pub const LIGHT_FG: ColorU = LIGHT_INK;
/// Deprecated alias for [`DARK_SUR`].
pub const DARK_BG: ColorU = DARK_SUR;
/// Deprecated alias for [`DARK_INK`].
pub const DARK_FG: ColorU = DARK_INK;

#[must_use]
pub fn button(btn: Button) -> Dom {
    let callbacks = match btn.on_click.into_option() {
        Some(ButtonOnClick {
            refany: data,
            callback,
        }) => vec![CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::Click),
            callback: azul_core::callbacks::CoreCallback {
                cb: callback.cb as *const () as usize,
                ctx: callback.ctx,
            },
            refany: data,
        }],
        None => Vec::new(),
    };

    let btn_type = btn.button_type;
    let type_class = btn.button_type.class_name();
    let classes: Vec<IdOrClass> = vec![
        Class(AzString::from("__azul-native-button")),
        Class(AzString::from(type_class)),
        Class(AzString::from("__azul-theme-flat")),
    ];

    let mut button = Dom::create_node(NodeType::Button);

    let has_icon = !btn.icon.as_str().is_empty() || btn.icon_dom.is_some();
    let has_image = btn.image.is_some();
    let has_trailing_icon = !btn.trailing_icon.as_str().is_empty();

    // Resolved before `btn`'s fields are moved into the tree below.
    let btn_container_style = btn.resolved_container_style();
    let btn_label_style = btn.resolved_label_style();
    let btn_image_style = btn.resolved_image_style();
    let btn_icon_style = btn.resolved_icon_style();
    let btn_trailing_icon_style = btn.resolved_trailing_icon_style();

    let a11y_name_src: String = if btn.label.as_str().is_empty() {
        if has_icon {
            btn.icon.as_str().to_string()
        } else {
            String::new()
        }
    } else {
        btn.label.as_str().to_string()
    };

    if has_icon {
        button = button.with_child(match btn.icon_dom.into_option() {
            Some(dom) => dom,
            None => Dom::create_icon(btn.icon).with_css_props(btn_icon_style),
        });
    }

    if let Some(image) = btn.image.into_option() {
        button = button.with_child(Dom::create_image(image).with_css_props(btn_image_style));
    }

    let skip_label = btn.label.as_str().is_empty() && (has_icon || has_image || has_trailing_icon);
    if !skip_label {
        button = button.with_child(
            crate::widgets::widget_p()
                .with_css_props(btn_label_style)
                .with_children(azul_core::dom::DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(btn.label),
                ])),
        );
    }

    if has_trailing_icon {
        button = button.with_child(
            Dom::create_icon(btn.trailing_icon).with_css_props(btn_trailing_icon_style),
        );
    }

    let a11y_name = a11y_name_src;
    let mut a11y = AccessibilityInfo {
        role: AccessibilityRole::PushButton,
        ..AccessibilityInfo::default()
    };
    if !a11y_name.is_empty() {
        a11y.accessibility_name = Some(AzString::from(a11y_name)).into();
    }

    // Add dark mode colors to container style
    let mut container_style: Vec<CssPropertyWithConditions> =
        btn_container_style.as_slice().to_vec();

    // In a flat theme we just override the background and text color for dark mode
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_BG)])
                .into(),
        ),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_FG }.into()),
    ));

    // The interactive states go LAST. Inline declarations resolve last-match
    // wins and a `dark_theme(..)` rule matches in every pseudo-state, so any
    // dark resting colour pushed after a `dark_on_hover` / `dark_on_focus` twin
    // would shadow it — no ring, no hover face, in dark mode.
    container_style.extend(button_states(btn_type));

    button
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_callbacks(callbacks.into())
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(a11y)
}

use crate::widgets::check_box::CheckBox;

#[must_use]
pub fn check_box(cb: CheckBox) -> Dom {
    let cb_name = cb.accessibility_name.clone();
    crate::widgets::warn_widget_needs_a_name("check_box", cb_name.is_some());

    let checked_now = cb.check_box_state.inner.checked;

    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, HoverEventFilter},
    };

    let mut container_style: Vec<CssPropertyWithConditions> =
        cb.resolved_container_style().as_slice().to_vec();
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_BG)])
                .into(),
        ),
    ));
    // Flat checkmark background in dark mode
    let is_checked = cb.check_box_state.inner.checked;
    let mut content_style: Vec<CssPropertyWithConditions> =
        cb.resolved_content_style().as_slice().to_vec();
    if checked_now {
        content_style.push(CssPropertyWithConditions::dark_theme(
            CssProperty::BackgroundContent(
                StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_FG)])
                    .into(),
            ),
        ));
    }

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from(
            crate::widgets::check_box::CHECKBOX_CONTAINER_CLASS,
        ))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback {
                    cb: crate::widgets::check_box::input::default_on_checkbox_clicked as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(cb.check_box_state),
            }]
            .into(),
        )
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::CheckButton,
            accessibility_name: cb_name,
            states: azul_core::a11y::AccessibilityStateVec::from_const_slice(if checked_now {
                &[azul_core::a11y::AccessibilityState::CheckedTrue]
            } else {
                &[azul_core::a11y::AccessibilityState::CheckedFalse]
            }),
            ..Default::default()
        })
        .with_children(
            vec![Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from(
                    crate::widgets::check_box::CHECKBOX_CONTENT_CLASS,
                ))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(content_style))]
            .into(),
        )
}

use crate::widgets::text_input::{
    default_on_focus_lost, default_on_focus_received, default_on_mouse_hover,
    default_on_text_input, default_on_virtual_key_down, TextInput, TEXT_INPUT_CONTAINER_CLASS,
    TEXT_INPUT_LABEL_CLASS,
};

#[must_use]
pub fn text_input(mut ti: TextInput) -> Dom {
    let a11y_name: Option<AzString> = ti.text_input_state.inner.placeholder.as_ref().cloned();
    let a11y_value: String = ti
        .text_input_state
        .inner
        .text
        .as_ref()
        .iter()
        .filter_map(|c| core::char::from_u32(*c))
        .collect();

    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{
            AttributeType, DomVec, EventFilter, FocusEventFilter, HoverEventFilter,
            IdOrClass::Class, TabIndex,
        },
    };

    ti.text_input_state.inner.cursor_pos = ti.text_input_state.inner.text.len();

    let label_text: String = ti
        .text_input_state
        .inner
        .text
        .iter()
        .filter_map(|s| core::char::from_u32(*s))
        .collect();

    let placeholder = ti
        .text_input_state
        .inner
        .placeholder
        .as_ref()
        .map(|s| s.as_str().to_string())
        .unwrap_or_default();

    // Resolved before `ti.text_input_state` is moved out below, and through the
    // widget's own resolver rather than a second copy of its default — the point
    // of the resolver is that flat and flora cannot drift on this answer.
    let resolved_container_style = ti.resolved_container_style();
    let resolved_label_style = ti.resolved_label_style();

    let state_ref = RefAny::new(ti.text_input_state);

    let mut container_style: Vec<CssPropertyWithConditions> =
        resolved_container_style.as_slice().to_vec();
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_BG)])
                .into(),
        ),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_FG }.into()),
    ));

    // The interactive states the widget no longer declares. Appended LAST —
    // after the base style and after the theme's own dark resting colours —
    // because the last matching inline declaration wins: a `dark_theme` border
    // pushed after these would beat the dark hover/focus ring. One array so
    // half of them cannot ship.
    container_style.extend_from_slice(&FIELD_BORDER_STATES);

    let mut label_style: Vec<CssPropertyWithConditions> = resolved_label_style.as_slice().to_vec();
    label_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_FG }.into()),
    ));

    Dom::create_div()
        .with_ids_and_classes(vec![Class(TEXT_INPUT_CONTAINER_CLASS.into())].into())
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Text,
            accessibility_name: a11y_name.into(),
            accessibility_value: Some(AzString::from(a11y_value)).into(),
            ..Default::default()
        })
        .with_contenteditable(true)
        .with_dataset(Some(state_ref.clone()).into())
        .with_callbacks(
            vec![
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                    refany: state_ref.clone(),
                    callback: CoreCallback {
                        cb: default_on_focus_received as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::FocusLost),
                    refany: state_ref.clone(),
                    callback: CoreCallback {
                        cb: default_on_focus_lost as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::TextInput),
                    refany: state_ref.clone(),
                    callback: CoreCallback {
                        cb: default_on_text_input as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    refany: state_ref.clone(),
                    callback: CoreCallback {
                        cb: default_on_virtual_key_down as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseOver),
                    refany: state_ref,
                    callback: CoreCallback {
                        cb: default_on_mouse_hover as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
            ]
            .into(),
        )
        .with_children(
            vec![crate::widgets::widget_p()
                .with_ids_and_classes(vec![Class(TEXT_INPUT_LABEL_CLASS.into())].into())
                .with_css_props(CssPropertyWithConditionsVec::from_vec(label_style))
                .with_attribute(AttributeType::Placeholder(placeholder.into()))
                .with_children(DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(label_text),
                ]))]
            .into(),
        )
}

#[must_use]
pub fn label(l: crate::widgets::label::Label) -> Dom {
    use azul_core::dom::{IdOrClass::Class, IdOrClassVec};
    use AzString;

    static LABEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-label"))];

    // Resolved before `l.string` is moved out below.
    let label_style = l.resolved_label_style();

    crate::widgets::widget_p_with_text(l.string)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
        .with_css_props(label_style)
}

#[must_use]
pub fn switch(s: crate::widgets::switch::Switch) -> Dom {
    let is_checked = s.switch_state.inner.checked;
    // Resolved up front: the knob's Dom is built after `s.switch_state` has
    // been moved into the callback's RefAny, and the resolver needs the whole
    // widget.
    let resolved_track_style = s.resolved_track_style();
    let resolved_knob_style = s.resolved_knob_style();
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{Dom, EventFilter, HoverEventFilter, IdOrClassVec, TabIndex},
    };

    let sw_name = s.accessibility_name.clone();
    crate::widgets::warn_widget_needs_a_name("switch", sw_name.is_some());

    let switch_checked = s.switch_state.inner.checked;

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from(
            crate::widgets::switch::SWITCH_TRACK_CLASS,
        ))
        .with_css_props(resolved_track_style.as_slice().to_vec().into())
        .with_callbacks(
            alloc::vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback {
                    cb: crate::widgets::switch::input::default_on_switch_clicked as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(s.switch_state),
            }]
            .into(),
        )
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::CheckButton,
            accessibility_name: sw_name,
            states: azul_core::a11y::AccessibilityStateVec::from_vec(alloc::vec![
                if switch_checked {
                    azul_core::a11y::AccessibilityState::CheckedTrue
                } else {
                    azul_core::a11y::AccessibilityState::CheckedFalse
                },
            ]),
            ..Default::default()
        })
        .with_children(
            alloc::vec![Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from(
                    crate::widgets::switch::SWITCH_KNOB_CLASS
                ))
                .with_css_props(resolved_knob_style.as_slice().to_vec().into())]
            .into(),
        )
}

// -----------------------------------------------------------------------------
// PROGRESSBAR
// -----------------------------------------------------------------------------

#[must_use]
pub fn progressbar(bar: crate::widgets::progressbar::ProgressBar) -> Dom {
    let height = bar.height;
    let dataset = RefAny::new(crate::widgets::progressbar::ProgressBarLocalDataset { bar });
    Dom::create_virtual_view(
        dataset.clone(),
        azul_core::callbacks::VirtualViewCallback::create(progressbar_render_virtual_view),
    )
    .with_dataset(Some(dataset).into())
    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
        CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
            LayoutHeight::Px(height),
        ))),
        CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
            LayoutWidth::Px(PixelValue::percent(100.0)),
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowX(LayoutOverflowValue::Exact(
            LayoutOverflow::Hidden,
        ))),
        CssPropertyWithConditions::simple(CssProperty::OverflowY(LayoutOverflowValue::Exact(
            LayoutOverflow::Hidden,
        ))),
    ]))
}

/// The render core behind [`ProgressBar::render_bar`] (percentage widths,
/// `bounds_px: None`) and the `VirtualView` callback (absolute pixel sizes
/// computed from the node's known bounds, `Some((width, height))`).
///
/// The split exists because the two contexts size differently. Percentages
/// inside a `VirtualView` DO resolve correctly against the view's bounds
/// (the child DOM lays out against its own viewport - it briefly resolved
/// against the WINDOW, fixed 2026-08-29, pinned by
/// `a_virtual_view_child_lays_out_against_the_view_bounds_not_the_window`),
/// but the bounds mode stays PIXEL-based for what percentages cannot
/// express: the container is sized to `bounds - 2px borders` so its 1px
/// border ring lands INSIDE the box - with the normal-flow sizing (content
/// height + borders) the ring overflowed the VV node and was clipped away
/// at the right and bottom ("oddly cut off", user report 2026-08-29) - and
/// the fill is an exact device-pixel split of the known content width.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn progressbar_render_bar_impl(
    bar: crate::widgets::progressbar::ProgressBar,
    bounds_px: Option<(f32, f32)>,
) -> Dom {
    {
        use azul_core::dom::DomVec;

        let this = bar;
        let percent_done = this.progressbar_state.percent_done.clamp(0.0, 100.0);
        // Sizes resolved per context (see fn docs). The bounds branch
        // subtracts the container's 1px border ring so children + borders
        // exactly fill the VV box.
        let (bar_width, remaining_width) = match bounds_px {
            Some((w, _)) => {
                let inner = (w - 2.0).max(0.0);
                let filled = inner * percent_done / 100.0;
                (PixelValue::px(filled), PixelValue::px(inner - filled))
            }
            None => (
                PixelValue::percent(percent_done),
                PixelValue::percent(100.0 - percent_done),
            ),
        };
        let container_height = match bounds_px {
            Some((_, h)) => PixelValue::px((h - 2.0).max(0.0)),
            None => this.height,
        };

        let mut container_props = vec![
            // .__azul-native-progress-bar-container
            CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
                LayoutHeight::Px(container_height),
            ))),
            // `display: flex` is LOAD-BEARING: azul's default display is
            // BLOCK, so `flex-direction: row` alone stacks the two
            // children as full-width, zero-height block boxes - the fill
            // never painted anywhere the widget was used (found 2026-08-29
            // via the azpaint pressure meter; also the real culprit behind
            // the "inline-width meter never repaints" ledger entry).
            CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
                LayoutDisplay::Flex,
            ))),
            CssPropertyWithConditions::simple(CssProperty::FlexDirection(
                LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomRightRadius(
                StyleBorderBottomRightRadiusValue::Exact(StyleBorderBottomRightRadius {
                    inner: PixelValue::const_px(3),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomLeftRadius(
                StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
                    inner: PixelValue::const_px(3),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopRightRadius(
                StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
                    inner: PixelValue::const_px(3),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopLeftRadius(
                StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
                    inner: PixelValue::const_px(3),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
                LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
                    inner: PixelValue::const_px(1),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
                LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
                    inner: PixelValue::const_px(1),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
                LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
                    inner: PixelValue::const_px(1),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
                LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
                    inner: PixelValue::const_px(1),
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
                StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
                    inner: BorderStyle::Solid,
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
                StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
                    inner: BorderStyle::Solid,
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
                StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
                    inner: BorderStyle::Solid,
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
                StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
                    inner: BorderStyle::Solid,
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
                StyleBorderBottomColorValue::Exact(StyleBorderBottomColor { inner: LIGHT_BD3 }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
                StyleBorderLeftColorValue::Exact(StyleBorderLeftColor { inner: LIGHT_BD3 }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
                StyleBorderRightColorValue::Exact(StyleBorderRightColor { inner: LIGHT_BD3 }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
                StyleBorderTopColorValue::Exact(StyleBorderTopColor { inner: LIGHT_BD3 }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                StyleBackgroundContentVecValue::Exact(this.container_background.clone()),
            )),
        ];
        if let Some((w, _)) = bounds_px {
            container_props.push(CssPropertyWithConditions::simple(CssProperty::Width(
                LayoutWidthValue::Exact(LayoutWidth::Px(PixelValue::px((w - 2.0).max(0.0)))),
            )));
        }

        Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(container_props))
            .with_ids_and_classes({
                const IDS_AND_CLASSES_10874511710181900075: &[IdOrClass] = &[Class(
                    AzString::from_const_str("__azul-native-progress-bar-container"),
                )];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_10874511710181900075)
            })
            // For a progress bar the VALUE is the content: two coloured divs
            // say nothing to a screen reader, "75%" says everything. Published
            // on every build so it tracks the bar; a callback that moves the
            // bar live without a rebuild keeps it current with
            // `CallbackInfo::set_accessibility_value` on this node.
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::ProgressBar,
                accessibility_value: Some(AzString::from(alloc::format!(
                    "{:.0}%",
                    // NaN clamps to NaN and would read "NaN%"; an unknown
                    // value announces as empty, like the bar it draws.
                    if percent_done.is_finite() { percent_done } else { 0.0 }
                )))
                .into(),
                ..Default::default()
            })
            .with_children(DomVec::from_vec(vec![
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        // .__azul-native-progress-bar-bar
                        // Use percentage width instead of flex-grow hack
                        CssPropertyWithConditions::simple(CssProperty::Width(
                            LayoutWidthValue::Exact(LayoutWidth::Px(bar_width)),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderBottomRightRadius(
                            StyleBorderBottomRightRadiusValue::Exact(
                                StyleBorderBottomRightRadius {
                                    inner: PixelValue::const_px(1),
                                },
                            ),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderBottomLeftRadius(
                            StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopRightRadius(
                            StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopLeftRadius(
                            StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                            StyleBackgroundContentVecValue::Exact(this.bar_background),
                        )),
                    ]))
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_16512648314570682783: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-progress-bar-bar"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16512648314570682783)
                    }),
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        // .__azul-native-progress-bar-remaining
                        // Use percentage width for the remaining space
                        CssPropertyWithConditions::simple(CssProperty::Width(
                            LayoutWidthValue::Exact(LayoutWidth::Px(remaining_width)),
                        )),
                    ]))
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2492405364126620395: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-progress-bar-remaining"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2492405364126620395)
                    }),
            ]))
    }
}

#[must_use]
/// The widget's `VirtualView` callback: render the CURRENT state of the bar
/// into the node's bounds. Invoked on mount and again every time
/// [`ProgressBar::update_progress`] queues a re-render.
///
/// The bar is not scrollable content, so all three rects collapse to one:
/// `materialized` == `virtual_rect` == the container's box at origin zero.
pub extern "C" fn progressbar_render_virtual_view(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let Some(state) = data.downcast_ref::<crate::widgets::progressbar::ProgressBarLocalDataset>()
    else {
        // Foreign payload: render nothing rather than lying about bounds.
        return VirtualViewReturn::default();
    };
    let size = info.bounds.get_logical_size();
    let rect = LogicalRect::new(LogicalPosition::zero(), size);
    // Clone-per-render is two enum copies + an `AzString`-less state copy; the
    // backgrounds are either `&'static` (shared, no alloc) or a caller-owned
    // heap vec that must be preserved for the NEXT render anyway. Pixel
    // widths, not percentages: the callback knows its bounds (see
    // `render_bar_impl`).
    VirtualViewReturn::with_dom(
        progressbar_render_bar_impl(state.bar.clone(), Some((size.width, size.height))),
        rect,
        rect,
    )
}

pub fn slider(slider: crate::widgets::slider::Slider) -> Dom {
    let value_now = slider.slider_state.inner.value;
    let a11y_name = slider.accessibility_name.clone();
    crate::widgets::warn_widget_needs_a_name("Slider", a11y_name.is_some());

    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, HoverEventFilter, TabIndex},
        refany::{OptionRefAny, RefAny},
    };

    // Resolved before `slider.slider_state` is moved out below; the resolvers
    // borrow `&slider`, and the thumb's margin is derived from the state.
    let resolved_track_style = slider.resolved_track_style();
    let resolved_thumb_style = slider.resolved_thumb_style();

    let state = RefAny::new(slider.slider_state);
    let mk = |event: EventFilter, cb: usize| CoreCallbackData {
        event,
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany: state.clone(),
    };
    let callbacks = vec![
        mk(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            crate::widgets::slider::on_slider_pointer_down as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::MouseMove),
            crate::widgets::slider::on_slider_pointer_move as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            crate::widgets::slider::on_slider_pointer_up as usize,
        ),
        mk(
            EventFilter::Focus(azul_core::events::FocusEventFilter::VirtualKeyDown),
            crate::widgets::slider::on_slider_key as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::MouseLeave),
            crate::widgets::slider::on_slider_pointer_leave as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::TouchStart),
            crate::widgets::slider::on_slider_pointer_down as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::TouchMove),
            crate::widgets::slider::on_slider_pointer_move as usize,
        ),
        mk(
            EventFilter::Hover(HoverEventFilter::TouchEnd),
            crate::widgets::slider::on_slider_pointer_up as usize,
        ),
    ];

    let mut track_style = resolved_track_style.as_slice().to_vec();
    let mut thumb_style = resolved_thumb_style.as_slice().to_vec();

    // Flat specific:
    track_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_BG)])
                .into(),
        ),
    ));
    thumb_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_FG)])
                .into(),
        ),
    ));

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(
            AzString::from_const_str("__azul-native-slider"),
        )]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(track_style))
        .with_callbacks(callbacks.into())
        .with_dataset(OptionRefAny::Some(state))
        .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(
            crate::widgets::slider::merge_slider_state,
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Slider,
            accessibility_name: a11y_name,
            accessibility_value: Some(AzString::from(alloc::format!("{value_now}"))).into(),
            ..Default::default()
        })
        .with_children(
            vec![Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(
                    AzString::from_const_str("__azul-native-slider-thumb"),
                )]))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(thumb_style))]
            .into(),
        )
}

#[must_use]
pub fn text_area(mut ta: crate::widgets::text_area::TextArea) -> Dom {
    let ta_name: Option<AzString> = ta.text_area_state.inner.placeholder.as_ref().cloned();

    use azul_core::dom::{
        AttributeType, DomVec, EventFilter, FocusEventFilter, IdOrClass::Class, TabIndex,
    };

    ta.text_area_state.inner.cursor_pos = ta.text_area_state.inner.text.len();

    // Resolved before `ta.text_area_state` is moved out below, and through the
    // widget's resolver rather than a second copy of its default.
    let resolved_container_style = ta.resolved_container_style();

    let label_text: String = ta
        .text_area_state
        .inner
        .text
        .iter()
        .filter_map(|s| core::char::from_u32(*s))
        .collect();

    let placeholder = ta
        .text_area_state
        .inner
        .placeholder
        .as_ref()
        .map(|s| s.as_str().to_string())
        .unwrap_or_default();

    let state_ref = RefAny::new(ta.text_area_state);

    let mut container_style: Vec<CssPropertyWithConditions> =
        resolved_container_style.as_slice().to_vec();
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_BG)])
                .into(),
        ),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_FG }.into()),
    ));

    let mut label_style: Vec<CssPropertyWithConditions> = match &ta.label_style {
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(s) => {
            s.as_slice().to_vec()
        }
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
            crate::widgets::text_area::TEXT_AREA_LABEL_PROPS.to_vec()
        }
    };
    label_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_FG }.into()),
    ));

    // The interactive states go LAST. Inline declarations resolve last-match
    // wins and a `dark_theme(..)` rule matches in every pseudo-state, so any
    // dark resting colour pushed after a `dark_on_hover` / `dark_on_focus` twin
    // would shadow it — no ring, no hover face, in dark mode.
    container_style.extend_from_slice(&FIELD_BORDER_STATES);

    Dom::create_div()
        .with_ids_and_classes(vec![Class("__azul-native-text-area-container".into())].into())
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Text,
            accessibility_name: ta_name.into(),
            ..Default::default()
        })
        .with_contenteditable(true)
        .with_dataset(Some(state_ref.clone()).into())
        .with_callbacks(
            vec![
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                    refany: state_ref.clone(),
                    callback: azul_core::callbacks::CoreCallback {
                        cb: crate::widgets::text_area::default_on_focus_received as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::FocusLost),
                    refany: state_ref.clone(),
                    callback: azul_core::callbacks::CoreCallback {
                        cb: crate::widgets::text_area::default_on_focus_lost as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::TextInput),
                    refany: state_ref.clone(),
                    callback: azul_core::callbacks::CoreCallback {
                        cb: crate::widgets::text_area::default_on_text_input as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
                CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                    refany: state_ref,
                    callback: azul_core::callbacks::CoreCallback {
                        cb: crate::widgets::text_area::default_on_virtual_key_down as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                },
            ]
            .into(),
        )
        .with_children(
            vec![crate::widgets::widget_p()
                .with_ids_and_classes(vec![Class("__azul-native-text-area-label".into())].into())
                .with_css_props(CssPropertyWithConditionsVec::from_vec(label_style))
                .with_attribute(AttributeType::Placeholder(placeholder.into()))
                .with_children(DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(label_text),
                ]))]
            .into(),
        )
}

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

/// The dropdown's border, as a palette token rather than a private literal.
///
/// It had no dark counterpart, so a dropdown kept a light-grey outline on a
/// dark surface; the rules that use it now pair it with `DARK_BD`.
const FLAT_BORDER_NORMAL: ColorU = LIGHT_BD;

const FLAT_DROPDOWN_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineFlex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        2,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(2),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(LIGHT_BG)]),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_FG,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: FLAT_BORDER_NORMAL,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: FLAT_BORDER_NORMAL,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: FLAT_BORDER_NORMAL,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: FLAT_BORDER_NORMAL,
        },
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(DARK_BG)]),
    )),
    // The four border colours above are light-mode values; without these the
    // dropdown kept a light-grey outline on a dark surface.
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_BD },
    )),
    CssPropertyWithConditions::dark_theme(CssProperty::const_text_color(StyleTextColor {
        inner: DARK_FG,
    })),
];

const FLAT_DROPDOWN_LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_FG,
    })),
    CssPropertyWithConditions::dark_theme(CssProperty::const_text_color(StyleTextColor {
        inner: DARK_FG,
    })),
];

const FLAT_DROPDOWN_ARROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_FG,
    })),
    CssPropertyWithConditions::dark_theme(CssProperty::const_text_color(StyleTextColor {
        inner: DARK_FG,
    })),
];

#[must_use]
pub fn drop_down(dd: crate::widgets::drop_down::DropDown) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{
            Dom, DomVec, EventFilter, FocusEventFilter, IdOrClass::Class, IdOrClassVec, TabIndex,
        },
        refany::RefAny,
    };
    use azul_css::AzString;

    let selected_label: Option<AzString> = dd
        .choices
        .as_ref()
        .get(dd.selected)
        .map(|o| AzString::from(o.as_str().to_string()));

    const DROPDOWN_CLASS: &[IdOrClass] =
        &[Class(AzString::from_const_str("__azul-native-dropdown"))];

    let selected_text = dd
        .choices
        .as_slice()
        .get(dd.selected)
        .cloned()
        .unwrap_or_else(|| AzString::from_const_str(""));

    let refany = RefAny::new(dd);

    Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
            FLAT_DROPDOWN_WRAPPER_STYLE,
        ))
        .with_ids_and_classes(IdOrClassVec::from_const_slice(DROPDOWN_CLASS))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::ComboBox,
            accessibility_value: selected_label.into(),
            ..Default::default()
        })
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                refany,
                callback: CoreCallback {
                    cb: crate::widgets::drop_down::on_dropdown_click as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
            }]
            .into(),
        )
        .with_children(DomVec::from_vec(vec![
            crate::widgets::widget_p()
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    FLAT_DROPDOWN_LABEL_STYLE,
                ))
                .with_children(DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(selected_text),
                ])),
            Dom::create_icon(AzString::from_const_str("arrow_drop_down")).with_css_props(
                CssPropertyWithConditionsVec::from_const_slice(FLAT_DROPDOWN_ARROW_STYLE),
            ),
        ]))
}

#[must_use]
pub fn avatar(a: crate::widgets::avatar::Avatar) -> Dom {
    use azul_core::dom::{Dom, IdOrClassVec};
    let size = a.size;
    let child = match a.image.into_option() {
        Some(image) => Dom::create_image(image)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(
                crate::widgets::avatar::AVATAR_IMAGE_CLASS,
            ))
            .with_css_props(crate::widgets::avatar::build_image_style(size)),
        None => crate::widgets::widget_p_with_text(a.initials).with_ids_and_classes(
            IdOrClassVec::from_const_slice(crate::widgets::avatar::AVATAR_INITIALS_CLASS),
        ),
    };

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(
            crate::widgets::avatar::AVATAR_CLASS,
        ))
        .with_css_props(
            match a.avatar_style {
                azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(style) => {
                    style.as_slice().to_vec()
                }
                azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
                    crate::widgets::avatar::build_avatar_style(size)
                        .as_slice()
                        .to_vec()
                }
            }
            .into(),
        )
        .with_children(alloc::vec![child].into())
}

// ---------------------------------------------------------------------------
// INTERACTIVE STATES
// ---------------------------------------------------------------------------
//
// Every hover / pressed / focus rule the flat theme paints lives here, in
// LIGHT+DARK pairs, and the widget files reference these consts instead of
// declaring their own.
//
// The reason is the dark half. A widget file cannot see `DARK_HT` — the palette
// is in this module — so a rule written there could only ever name a light
// colour, and that is exactly how every interactive state in this toolkit came
// to paint its light-mode fill onto a dark surface. Declaring the pair together,
// where both halves are in scope, makes the light-only version unwriteable.
//
// These are `const` rather than builder functions because the widgets that use
// them hold their styles in `static [CssPropertyWithConditions]` slices, and a
// const slice cannot splice a function's return value.

/// Row hover, light mode: the Explorer selection tint (#E5F3FF).
///
/// Deliberately NOT [`LIGHT_HT`]. A list or tree ROW hovers to the selection
/// blue; a control FACE hovers to the neutral grey. They are different surfaces
/// and the two tokens are not interchangeable.
pub const LIGHT_ROW_HOVER: ColorU = ColorU {
    r: 229,
    g: 243,
    b: 255,
    a: 255,
};

/// Row hover, dark mode: the twin of [`LIGHT_ROW_HOVER`].
pub const DARK_ROW_HOVER: ColorU = ColorU {
    r: 42,
    g: 45,
    b: 46,
    a: 255,
};

/// The hover fill for one row of a list, tree or menu — light mode.
///
/// Always use it with [`ROW_HOVER_DARK`]; either alone is the bug this section
/// exists to prevent.
pub const ROW_HOVER: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
        StyleBackgroundContent::Color(LIGHT_ROW_HOVER),
    ])),
);

/// The dark twin of [`ROW_HOVER`].
pub const ROW_HOVER_DARK: CssPropertyWithConditions = CssPropertyWithConditions::dark_on_hover(
    CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
        StyleBackgroundContent::Color(DARK_ROW_HOVER),
    ])),
);

/// The focus ring: the focused control's border takes the accent colour.
///
/// One const per edge, because a border colour is four properties and a focus
/// ring that sets only some of them leaves the rest at their resting colour.
/// Each has a dark twin using [`DARK_ACC`] — the accent is the one state colour
/// that genuinely has a per-mode value in both palettes, which is why the plan
/// names it.
pub const FOCUS_BORDER_TOP: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: LIGHT_ACC,
    }));

/// See [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_BOTTOM: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: LIGHT_ACC }),
);

/// See [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_LEFT: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_border_left_color(StyleBorderLeftColor { inner: LIGHT_ACC }),
);

/// See [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_RIGHT: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_border_right_color(StyleBorderRightColor { inner: LIGHT_ACC }),
);

/// The dark twin of [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_TOP_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_ACC },
    ));

/// The dark twin of [`FOCUS_BORDER_BOTTOM`].
pub const FOCUS_BORDER_BOTTOM_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_ACC },
    ));

/// The dark twin of [`FOCUS_BORDER_LEFT`].
pub const FOCUS_BORDER_LEFT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_ACC },
    ));

/// The dark twin of [`FOCUS_BORDER_RIGHT`].
pub const FOCUS_BORDER_RIGHT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_ACC },
    ));

/// Option-row hover for a drop-down list, light mode (#EAF4FC).
///
/// A shade lighter than [`LIGHT_ROW_HOVER`]: a menu that is already floating
/// over the page needs less contrast than a row inside a field.
pub const LIGHT_OPTION_HOVER: ColorU = ColorU {
    r: 234,
    g: 244,
    b: 252,
    a: 255,
};

/// The hover fill for one row of a drop-down list — light mode. Pair with
/// [`OPTION_HOVER_DARK`].
pub const OPTION_HOVER: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
        StyleBackgroundContent::Color(LIGHT_OPTION_HOVER),
    ])),
);

/// The dark twin of [`OPTION_HOVER`], reusing the row-hover dark fill: a menu
/// and a list row want the same treatment once the surface is dark.
pub const OPTION_HOVER_DARK: CssPropertyWithConditions = CssPropertyWithConditions::dark_on_hover(
    CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
        StyleBackgroundContent::Color(DARK_ROW_HOVER),
    ])),
);

/// A hover fill at a caller-supplied colour, paired with this theme's own dark
/// value.
///
/// For chrome whose LIGHT hover colour arrives from outside this palette — a
/// window-decoration colour the compositor reported, say. It still gets a twin:
/// azul's dark mode is its own CSS condition, not a reflection of the OS theme
/// (an app can force dark while the desktop is light), so a system colour
/// chosen for a light desktop is not automatically right here. The caller's
/// colour is used in light mode and [`DARK_HT`] in dark mode.
///
/// Returns both halves so a caller cannot take one without the other.
#[must_use]
pub fn hover_bg_pair(light: ColorU) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::on_hover(CssProperty::const_background_content(
            StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(light)]),
        )),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_background_content(
            StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(
                DARK_HT
            )]),
        )),
    ]
}

// ---------------------------------------------------------------------------
// INTERACTIVE STATES — text fields
// ---------------------------------------------------------------------------

/// Border colour on hover, one const per edge, light mode.
///
/// A border colour is four properties, so a state that sets only some edges
/// leaves the rest at their resting colour — which is why these travel as a set
/// rather than individually.
pub const HOVER_BORDER_TOP: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: LIGHT_ACC,
    }));

/// See [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_BOTTOM: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: LIGHT_ACC }),
);

/// See [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_LEFT: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_border_left_color(StyleBorderLeftColor { inner: LIGHT_ACC }),
);

/// See [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_RIGHT: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_border_right_color(StyleBorderRightColor { inner: LIGHT_ACC }),
);

/// The dark twin of [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_TOP_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_ACC },
    ));

/// The dark twin of [`HOVER_BORDER_BOTTOM`].
pub const HOVER_BORDER_BOTTOM_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_ACC },
    ));

/// The dark twin of [`HOVER_BORDER_LEFT`].
pub const HOVER_BORDER_LEFT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_ACC },
    ));

/// The dark twin of [`HOVER_BORDER_RIGHT`].
pub const HOVER_BORDER_RIGHT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_ACC },
    ));

/// Every border state a text field takes: the accent on hover and on focus, each
/// edge, each with its dark twin.
///
/// One array so a theme function appends the whole set in a line and cannot ship
/// half of it. This is what a widget file used to declare itself, in light mode
/// only — the reason a hovered field kept its light-blue ring on a dark surface.
pub const FIELD_BORDER_STATES: [CssPropertyWithConditions; 16] = [
    HOVER_BORDER_TOP,
    HOVER_BORDER_BOTTOM,
    HOVER_BORDER_LEFT,
    HOVER_BORDER_RIGHT,
    HOVER_BORDER_TOP_DARK,
    HOVER_BORDER_BOTTOM_DARK,
    HOVER_BORDER_LEFT_DARK,
    HOVER_BORDER_RIGHT_DARK,
    FOCUS_BORDER_TOP,
    FOCUS_BORDER_BOTTOM,
    FOCUS_BORDER_LEFT,
    FOCUS_BORDER_RIGHT,
    FOCUS_BORDER_TOP_DARK,
    FOCUS_BORDER_BOTTOM_DARK,
    FOCUS_BORDER_LEFT_DARK,
    FOCUS_BORDER_RIGHT_DARK,
];

/// Every state a button of one semantic type takes: hover fill, pressed fill and
/// focus ring, each with its dark twin.
///
/// The light values come from [`crate::widgets::button::get_button_colors`], so
/// there is still one source of truth for them; the DARK halves are chosen here,
/// because this is the only place the palette is in scope.
///
/// The dark rule differs by type on purpose:
///
/// * `Default` is the neutral grey button, so its surface belongs to the PAGE and its dark states
///   come from the theme ([`DARK_HT`] / [`DARK_PT`]).
/// * Every other type carries its own semantic colour — a Primary button is blue whichever mode the
///   app is in — so the same hover and pressed colours apply in dark mode. A neutral grey hover on
///   a blue button would be wrong, and inventing a second blue would be a design decision this
///   refactor has no business making.
/// * `Link` has no surface at all: it underlines instead, in both modes.
#[must_use]
pub fn button_states(
    button_type: crate::widgets::button::ButtonType,
) -> Vec<CssPropertyWithConditions> {
    use crate::widgets::button::ButtonType;

    let bg = |c: ColorU| {
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(c)])
                .into(),
        )
    };

    if button_type == ButtonType::Link {
        return alloc::vec![
            CssPropertyWithConditions::on_hover(CssProperty::TextDecoration(
                StyleTextDecoration::Underline.into(),
            )),
            CssPropertyWithConditions::dark_on_hover(CssProperty::TextDecoration(
                StyleTextDecoration::Underline.into(),
            )),
        ];
    }

    let (_, bg_hover, bg_active) = crate::widgets::button::get_button_colors(button_type);
    let neutral = button_type == ButtonType::Default;
    let (dark_hover, dark_active) = if neutral {
        (DARK_HT, DARK_PT)
    } else {
        (bg_hover, bg_active)
    };

    let mut out = alloc::vec![
        CssPropertyWithConditions::on_hover(bg(bg_hover)),
        CssPropertyWithConditions::dark_on_hover(bg(dark_hover)),
        CssPropertyWithConditions::on_active(bg(bg_active)),
        CssPropertyWithConditions::dark_on_active(bg(dark_active)),
    ];

    // The neutral button is the only one with a visible resting border, so it is
    // the only one whose border reacts to hover.
    if neutral {
        let light = ColorU::rgb(173, 181, 189);
        for (l, d) in [
            (
                CssProperty::const_border_top_color(StyleBorderTopColor { inner: light }),
                CssProperty::const_border_top_color(StyleBorderTopColor { inner: DARK_BD }),
            ),
            (
                CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: light }),
                CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: DARK_BD }),
            ),
            (
                CssProperty::const_border_left_color(StyleBorderLeftColor { inner: light }),
                CssProperty::const_border_left_color(StyleBorderLeftColor { inner: DARK_BD }),
            ),
            (
                CssProperty::const_border_right_color(StyleBorderRightColor { inner: light }),
                CssProperty::const_border_right_color(StyleBorderRightColor { inner: DARK_BD }),
            ),
        ] {
            out.push(CssPropertyWithConditions::on_hover(l));
            out.push(CssPropertyWithConditions::dark_on_hover(d));
        }
    }

    // The focus ring is the accent in both modes, and the consts already pair it.
    out.push(FOCUS_BORDER_TOP);
    out.push(FOCUS_BORDER_BOTTOM);
    out.push(FOCUS_BORDER_LEFT);
    out.push(FOCUS_BORDER_RIGHT);
    out.push(FOCUS_BORDER_TOP_DARK);
    out.push(FOCUS_BORDER_BOTTOM_DARK);
    out.push(FOCUS_BORDER_LEFT_DARK);
    out.push(FOCUS_BORDER_RIGHT_DARK);
    out
}

/// A hover fill with BOTH halves chosen by the caller.
///
/// For widgets that carry their own palette (`RibbonTheme`, `StatusBarTheme`,
/// ...). The rule for picking `dark`: a surface that is its own colour — a blue
/// nav, a coloured button — keeps its light hover colour, because the surface
/// does not change in dark mode either; a page-neutral surface takes this
/// theme's [`DARK_HT`]. See `button_states` for the same rule applied.
#[must_use]
pub fn hover_bg_both(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    let bg = |c: ColorU| {
        CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(alloc::vec![
            StyleBackgroundContent::Color(c),
        ]))
    };
    [
        CssPropertyWithConditions::on_hover(bg(light)),
        CssPropertyWithConditions::dark_on_hover(bg(dark)),
    ]
}

/// A pressed fill with BOTH halves chosen by the caller — see [`hover_bg_both`].
#[must_use]
pub fn active_bg_both(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    let bg = |c: ColorU| {
        CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(alloc::vec![
            StyleBackgroundContent::Color(c),
        ]))
    };
    [
        CssPropertyWithConditions::on_active(bg(light)),
        CssPropertyWithConditions::dark_on_active(bg(dark)),
    ]
}

// ===========================================================================
// PHASE 2 — per-widget state sections. Each widget's interactive-state consts
// live between its own pair of markers and nowhere else, so that the
// migrations can proceed in parallel without touching one another's lines.
// ===========================================================================

// == STATES: list_view ==

// ---------------------------------------------------------------------------
// INTERACTIVE STATES — list view
// ---------------------------------------------------------------------------
//
// The sixty hover / pressed / focus rules `list_view.rs` used to declare, as
// forty-five LIGHT+DARK pairs (the row's thirteen hover rules were spelled out
// twice over there). A column header hovers to a bluish-white face with a blue
// underline and presses to a bordered, inset-shadowed face; a row hovers to a
// blue-ringed tint and focuses to a stronger one. The light values are the
// widget's own, unchanged — this is a move, not a restyle. The dark halves are
// chosen here, because this is the only place the palette is in scope.
//
// Every surface the list styles is page-neutral — a white field, a
// white-to-grey header band — so each dark twin comes from the theme rather
// than from a colour invented for the occasion:
//
//   * fills: `DARK_HT` for the hovered header face and `DARK_PT` for the pressed one (a header is a
//     control face); `DARK_ROW_HOVER` for a hovered row, the fill a tree row already takes; and
//     `DARK_GLOW`, the translucent accent, for the focused row, so it stays distinguishable from a
//     merely hovered one the way #B8E0F3 is from #E5F3FB in light mode;
//   * borders: the light blues map onto the accent ramp — `DARK_SOFT`, the muted step, for the
//     hover underline and the hover ring, `DARK_ACC` for the focus ring — and the pressed header's
//     neutral grey-blue border takes `DARK_BD`;
//   * the pressed header's inset shadow shades its face toward the bottom stop, which is what
//     `DARK_PB` is.
//
// Border widths (1px) and styles (solid) do not change with the theme. Their
// dark twins restate the same value: the pairing is the invariant this
// section exists for, and keeping it uniform beats remembering which rules are
// exempt. They are shared between the header and the row, since a 1px solid
// edge is the same rule wherever it lands.

/// Column-header hover underline, light mode (#9ADFFE).
pub const LIGHT_LIST_HEADER_HOVER_LINE: ColorU = ColorU {
    r: 154,
    g: 223,
    b: 254,
    a: 255,
};

/// Column-header hover face, top stop, light mode (#F7FCFE).
///
/// The face is a vertical gradient with a hard step at the midpoint: this
/// colour held to 50%, then [`LIGHT_LIST_HEADER_HOVER_MID`] fading into
/// [`LIGHT_LIST_HEADER_HOVER_BOTTOM`].
pub const LIGHT_LIST_HEADER_HOVER_TOP: ColorU = ColorU {
    r: 247,
    g: 252,
    b: 254,
    a: 255,
};

/// Column-header hover face, just below the midpoint step (#E8F6FE).
pub const LIGHT_LIST_HEADER_HOVER_MID: ColorU = ColorU {
    r: 232,
    g: 246,
    b: 254,
    a: 255,
};

/// Column-header hover face, bottom stop (#CEE7F4).
pub const LIGHT_LIST_HEADER_HOVER_BOTTOM: ColorU = ColorU {
    r: 206,
    g: 231,
    b: 244,
    a: 255,
};

/// Column-header pressed face, light mode (#F9FAFB).
pub const LIGHT_LIST_HEADER_PRESSED: ColorU = ColorU {
    r: 249,
    g: 250,
    b: 251,
    a: 255,
};

/// Column-header pressed border, light mode (#C2CDDB): a neutral grey-blue,
/// not an accent.
pub const LIGHT_LIST_HEADER_PRESSED_BORDER: ColorU = ColorU {
    r: 194,
    g: 205,
    b: 219,
    a: 255,
};

/// Column-header pressed inset shadow, light mode (#CEE7F4) — the hover face's
/// bottom stop, so a pressed header reads as sunk into the hovered one.
pub const LIGHT_LIST_HEADER_PRESSED_SHADOW: ColorU = ColorU {
    r: 206,
    g: 231,
    b: 244,
    a: 255,
};

/// Row hover fill, light mode (#E5F3FB).
///
/// Not [`LIGHT_ROW_HOVER`] (#E5F3FF): the list's tint is four units short in
/// the blue channel. Kept as the widget had it — this is a move, not a restyle
/// — though nothing but the value separates the two, and a later pass may well
/// decide a list row and a tree row should hover alike.
pub const LIGHT_LIST_ROW_HOVER: ColorU = ColorU {
    r: 229,
    g: 243,
    b: 251,
    a: 255,
};

/// Row hover ring, light mode (#65B5DC).
pub const LIGHT_LIST_ROW_HOVER_BORDER: ColorU = ColorU {
    r: 101,
    g: 181,
    b: 220,
    a: 255,
};

/// Focused-row fill, light mode (#B8E0F3): a stronger tint than
/// [`LIGHT_LIST_ROW_HOVER`], so the keyboard cursor row stays distinct from a
/// row the pointer merely passes over.
pub const LIGHT_LIST_ROW_FOCUS: ColorU = ColorU {
    r: 184,
    g: 224,
    b: 243,
    a: 255,
};

/// Focused-row ring, light mode (#26A0DA).
pub const LIGHT_LIST_ROW_FOCUS_BORDER: ColorU = ColorU {
    r: 38,
    g: 160,
    b: 218,
    a: 255,
};

// The edge VALUES every list state shares — a 1px solid border, one const per
// edge and per property — so that each state rule below is a single line.
const LIST_EDGE_TOP_1PX: CssProperty = CssProperty::const_border_top_width(LayoutBorderTopWidth {
    inner: PixelValue::const_px(1),
});
const LIST_EDGE_BOTTOM_1PX: CssProperty =
    CssProperty::const_border_bottom_width(LayoutBorderBottomWidth {
        inner: PixelValue::const_px(1),
    });
const LIST_EDGE_LEFT_1PX: CssProperty =
    CssProperty::const_border_left_width(LayoutBorderLeftWidth {
        inner: PixelValue::const_px(1),
    });
const LIST_EDGE_RIGHT_1PX: CssProperty =
    CssProperty::const_border_right_width(LayoutBorderRightWidth {
        inner: PixelValue::const_px(1),
    });
const LIST_EDGE_TOP_SOLID: CssProperty = CssProperty::const_border_top_style(StyleBorderTopStyle {
    inner: BorderStyle::Solid,
});
const LIST_EDGE_BOTTOM_SOLID: CssProperty =
    CssProperty::const_border_bottom_style(StyleBorderBottomStyle {
        inner: BorderStyle::Solid,
    });
const LIST_EDGE_LEFT_SOLID: CssProperty =
    CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    });
const LIST_EDGE_RIGHT_SOLID: CssProperty =
    CssProperty::const_border_right_style(StyleBorderRightStyle {
        inner: BorderStyle::Solid,
    });

// -- hover: the 1px solid edge ---------------------------------------------

/// A 1px edge on hover, one const per edge and per property: the ring a
/// hovered row draws, and the underline a hovered column header draws (bottom
/// edge only). Each pairs with its `_DARK` twin.
pub const LIST_HOVER_BORDER_TOP_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(LIST_EDGE_TOP_1PX);
/// See [`LIST_HOVER_BORDER_TOP_WIDTH`].
pub const LIST_HOVER_BORDER_BOTTOM_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(LIST_EDGE_BOTTOM_1PX);
/// See [`LIST_HOVER_BORDER_TOP_WIDTH`].
pub const LIST_HOVER_BORDER_LEFT_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(LIST_EDGE_LEFT_1PX);
/// See [`LIST_HOVER_BORDER_TOP_WIDTH`].
pub const LIST_HOVER_BORDER_RIGHT_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(LIST_EDGE_RIGHT_1PX);
/// See [`LIST_HOVER_BORDER_TOP_WIDTH`].
pub const LIST_HOVER_BORDER_TOP_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(LIST_EDGE_TOP_SOLID);
/// See [`LIST_HOVER_BORDER_TOP_WIDTH`].
pub const LIST_HOVER_BORDER_BOTTOM_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(LIST_EDGE_BOTTOM_SOLID);
/// See [`LIST_HOVER_BORDER_TOP_WIDTH`].
pub const LIST_HOVER_BORDER_LEFT_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(LIST_EDGE_LEFT_SOLID);
/// See [`LIST_HOVER_BORDER_TOP_WIDTH`].
pub const LIST_HOVER_BORDER_RIGHT_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(LIST_EDGE_RIGHT_SOLID);

/// The dark twin of [`LIST_HOVER_BORDER_TOP_WIDTH`] — the same 1px, since a
/// border's weight does not change with the theme.
pub const LIST_HOVER_BORDER_TOP_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(LIST_EDGE_TOP_1PX);
/// The dark twin of [`LIST_HOVER_BORDER_BOTTOM_WIDTH`].
pub const LIST_HOVER_BORDER_BOTTOM_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(LIST_EDGE_BOTTOM_1PX);
/// The dark twin of [`LIST_HOVER_BORDER_LEFT_WIDTH`].
pub const LIST_HOVER_BORDER_LEFT_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(LIST_EDGE_LEFT_1PX);
/// The dark twin of [`LIST_HOVER_BORDER_RIGHT_WIDTH`].
pub const LIST_HOVER_BORDER_RIGHT_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(LIST_EDGE_RIGHT_1PX);
/// The dark twin of [`LIST_HOVER_BORDER_TOP_STYLE`].
pub const LIST_HOVER_BORDER_TOP_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(LIST_EDGE_TOP_SOLID);
/// The dark twin of [`LIST_HOVER_BORDER_BOTTOM_STYLE`].
pub const LIST_HOVER_BORDER_BOTTOM_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(LIST_EDGE_BOTTOM_SOLID);
/// The dark twin of [`LIST_HOVER_BORDER_LEFT_STYLE`].
pub const LIST_HOVER_BORDER_LEFT_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(LIST_EDGE_LEFT_SOLID);
/// The dark twin of [`LIST_HOVER_BORDER_RIGHT_STYLE`].
pub const LIST_HOVER_BORDER_RIGHT_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(LIST_EDGE_RIGHT_SOLID);

// -- pressed: the 1px solid edge -------------------------------------------

/// A 1px edge while pressed, one const per edge and per property: the border a
/// pressed column header draws all the way round. Each pairs with its `_DARK`
/// twin.
pub const LIST_ACTIVE_BORDER_TOP_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(LIST_EDGE_TOP_1PX);
/// See [`LIST_ACTIVE_BORDER_TOP_WIDTH`].
pub const LIST_ACTIVE_BORDER_BOTTOM_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(LIST_EDGE_BOTTOM_1PX);
/// See [`LIST_ACTIVE_BORDER_TOP_WIDTH`].
pub const LIST_ACTIVE_BORDER_LEFT_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(LIST_EDGE_LEFT_1PX);
/// See [`LIST_ACTIVE_BORDER_TOP_WIDTH`].
pub const LIST_ACTIVE_BORDER_RIGHT_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(LIST_EDGE_RIGHT_1PX);
/// See [`LIST_ACTIVE_BORDER_TOP_WIDTH`].
pub const LIST_ACTIVE_BORDER_TOP_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(LIST_EDGE_TOP_SOLID);
/// See [`LIST_ACTIVE_BORDER_TOP_WIDTH`].
pub const LIST_ACTIVE_BORDER_BOTTOM_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(LIST_EDGE_BOTTOM_SOLID);
/// See [`LIST_ACTIVE_BORDER_TOP_WIDTH`].
pub const LIST_ACTIVE_BORDER_LEFT_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(LIST_EDGE_LEFT_SOLID);
/// See [`LIST_ACTIVE_BORDER_TOP_WIDTH`].
pub const LIST_ACTIVE_BORDER_RIGHT_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(LIST_EDGE_RIGHT_SOLID);

/// The dark twin of [`LIST_ACTIVE_BORDER_TOP_WIDTH`] — the same 1px, since a
/// border's weight does not change with the theme.
pub const LIST_ACTIVE_BORDER_TOP_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(LIST_EDGE_TOP_1PX);
/// The dark twin of [`LIST_ACTIVE_BORDER_BOTTOM_WIDTH`].
pub const LIST_ACTIVE_BORDER_BOTTOM_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(LIST_EDGE_BOTTOM_1PX);
/// The dark twin of [`LIST_ACTIVE_BORDER_LEFT_WIDTH`].
pub const LIST_ACTIVE_BORDER_LEFT_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(LIST_EDGE_LEFT_1PX);
/// The dark twin of [`LIST_ACTIVE_BORDER_RIGHT_WIDTH`].
pub const LIST_ACTIVE_BORDER_RIGHT_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(LIST_EDGE_RIGHT_1PX);
/// The dark twin of [`LIST_ACTIVE_BORDER_TOP_STYLE`].
pub const LIST_ACTIVE_BORDER_TOP_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(LIST_EDGE_TOP_SOLID);
/// The dark twin of [`LIST_ACTIVE_BORDER_BOTTOM_STYLE`].
pub const LIST_ACTIVE_BORDER_BOTTOM_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(LIST_EDGE_BOTTOM_SOLID);
/// The dark twin of [`LIST_ACTIVE_BORDER_LEFT_STYLE`].
pub const LIST_ACTIVE_BORDER_LEFT_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(LIST_EDGE_LEFT_SOLID);
/// The dark twin of [`LIST_ACTIVE_BORDER_RIGHT_STYLE`].
pub const LIST_ACTIVE_BORDER_RIGHT_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(LIST_EDGE_RIGHT_SOLID);

// -- focus: the 1px solid edge ---------------------------------------------

/// A 1px edge on focus, one const per edge and per property: the ring the
/// focused row draws. Each pairs with its `_DARK` twin.
pub const LIST_FOCUS_BORDER_TOP_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(LIST_EDGE_TOP_1PX);
/// See [`LIST_FOCUS_BORDER_TOP_WIDTH`].
pub const LIST_FOCUS_BORDER_BOTTOM_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(LIST_EDGE_BOTTOM_1PX);
/// See [`LIST_FOCUS_BORDER_TOP_WIDTH`].
pub const LIST_FOCUS_BORDER_LEFT_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(LIST_EDGE_LEFT_1PX);
/// See [`LIST_FOCUS_BORDER_TOP_WIDTH`].
pub const LIST_FOCUS_BORDER_RIGHT_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(LIST_EDGE_RIGHT_1PX);
/// See [`LIST_FOCUS_BORDER_TOP_WIDTH`].
pub const LIST_FOCUS_BORDER_TOP_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(LIST_EDGE_TOP_SOLID);
/// See [`LIST_FOCUS_BORDER_TOP_WIDTH`].
pub const LIST_FOCUS_BORDER_BOTTOM_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(LIST_EDGE_BOTTOM_SOLID);
/// See [`LIST_FOCUS_BORDER_TOP_WIDTH`].
pub const LIST_FOCUS_BORDER_LEFT_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(LIST_EDGE_LEFT_SOLID);
/// See [`LIST_FOCUS_BORDER_TOP_WIDTH`].
pub const LIST_FOCUS_BORDER_RIGHT_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(LIST_EDGE_RIGHT_SOLID);

/// The dark twin of [`LIST_FOCUS_BORDER_TOP_WIDTH`] — the same 1px, since a
/// border's weight does not change with the theme.
pub const LIST_FOCUS_BORDER_TOP_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(LIST_EDGE_TOP_1PX);
/// The dark twin of [`LIST_FOCUS_BORDER_BOTTOM_WIDTH`].
pub const LIST_FOCUS_BORDER_BOTTOM_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(LIST_EDGE_BOTTOM_1PX);
/// The dark twin of [`LIST_FOCUS_BORDER_LEFT_WIDTH`].
pub const LIST_FOCUS_BORDER_LEFT_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(LIST_EDGE_LEFT_1PX);
/// The dark twin of [`LIST_FOCUS_BORDER_RIGHT_WIDTH`].
pub const LIST_FOCUS_BORDER_RIGHT_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(LIST_EDGE_RIGHT_1PX);
/// The dark twin of [`LIST_FOCUS_BORDER_TOP_STYLE`].
pub const LIST_FOCUS_BORDER_TOP_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(LIST_EDGE_TOP_SOLID);
/// The dark twin of [`LIST_FOCUS_BORDER_BOTTOM_STYLE`].
pub const LIST_FOCUS_BORDER_BOTTOM_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(LIST_EDGE_BOTTOM_SOLID);
/// The dark twin of [`LIST_FOCUS_BORDER_LEFT_STYLE`].
pub const LIST_FOCUS_BORDER_LEFT_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(LIST_EDGE_LEFT_SOLID);
/// The dark twin of [`LIST_FOCUS_BORDER_RIGHT_STYLE`].
pub const LIST_FOCUS_BORDER_RIGHT_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(LIST_EDGE_RIGHT_SOLID);

// -- the column header ------------------------------------------------------

/// Column-header hover underline — light: [`LIGHT_LIST_HEADER_HOVER_LINE`] on
/// the bottom edge. Pair with [`LIST_HEADER_HOVER_LINE_COLOR_DARK`].
pub const LIST_HEADER_HOVER_LINE_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: LIGHT_LIST_HEADER_HOVER_LINE,
        },
    ));

/// The dark twin of [`LIST_HEADER_HOVER_LINE_COLOR`]: [`DARK_SOFT`], the muted
/// accent. The light value is a pale blue, and the ramp's muted step is the
/// nearest thing the palette has to it.
pub const LIST_HEADER_HOVER_LINE_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_SOFT },
    ));

// The stops of the hovered column header's face: `LIGHT_LIST_HEADER_HOVER_TOP`
// held to the midpoint, then a hard step to `LIGHT_LIST_HEADER_HOVER_MID`
// fading into `LIGHT_LIST_HEADER_HOVER_BOTTOM`.
const LIST_HEADER_HOVER_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(LIGHT_LIST_HEADER_HOVER_TOP),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(50),
        color: ColorOrSystem::color(LIGHT_LIST_HEADER_HOVER_TOP),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(51),
        color: ColorOrSystem::color(LIGHT_LIST_HEADER_HOVER_MID),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(LIGHT_LIST_HEADER_HOVER_BOTTOM),
    },
];

// The hovered column header's face: a top-to-bottom gradient over
// `LIST_HEADER_HOVER_STOPS`.
const LIST_HEADER_HOVER_FACE: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(LIST_HEADER_HOVER_STOPS),
    })];

/// Column-header hover face — light. Pair with [`LIST_HEADER_HOVER_BG_DARK`].
///
/// The white-to-blue-grey gradient built from [`LIGHT_LIST_HEADER_HOVER_TOP`],
/// [`LIGHT_LIST_HEADER_HOVER_MID`] and [`LIGHT_LIST_HEADER_HOVER_BOTTOM`].
pub const LIST_HEADER_HOVER_BG: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(LIST_HEADER_HOVER_FACE),
    ));

/// The dark twin of [`LIST_HEADER_HOVER_BG`]: [`DARK_HT`], the hovered control
/// face. A flat fill rather than a gradient, because in this theme the two
/// stops of a face are the same colour anyway.
pub const LIST_HEADER_HOVER_BG_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(DARK_HT)]),
    ));

// The inset glow a pressed column header draws inside each edge, light mode: a
// 5px blur of `LIGHT_LIST_HEADER_PRESSED_SHADOW`, no offset, no spread.
const LIST_HEADER_PRESSED_INSET: StyleBoxShadow = StyleBoxShadow {
    offset_x: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    offset_y: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    color: LIGHT_LIST_HEADER_PRESSED_SHADOW,
    blur_radius: PixelValueNoPercent {
        inner: PixelValue::const_px(5),
    },
    spread_radius: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    clip_mode: BoxShadowClipMode::Inset,
};

// `LIST_HEADER_PRESSED_INSET` in dark mode: the same glow in `DARK_PB`, the
// pressed face's bottom stop, which is what an inset shadow shades a face
// toward.
const LIST_HEADER_PRESSED_INSET_DARK: StyleBoxShadow = StyleBoxShadow {
    offset_x: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    offset_y: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    color: DARK_PB,
    blur_radius: PixelValueNoPercent {
        inner: PixelValue::const_px(5),
    },
    spread_radius: PixelValueNoPercent {
        inner: PixelValue::const_px(0),
    },
    clip_mode: BoxShadowClipMode::Inset,
};

/// Column-header pressed inset shadow, bottom edge — light: a 5px inset blur
/// of [`LIGHT_LIST_HEADER_PRESSED_SHADOW`].
///
/// Four edges, one const each, so the glow runs all the way round; pair each
/// with its `_DARK` twin.
pub const LIST_HEADER_ACTIVE_SHADOW_BOTTOM: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(CssProperty::BoxShadowBottom(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&LIST_HEADER_PRESSED_INSET),
    )));
/// See [`LIST_HEADER_ACTIVE_SHADOW_BOTTOM`].
pub const LIST_HEADER_ACTIVE_SHADOW_TOP: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(CssProperty::BoxShadowTop(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&LIST_HEADER_PRESSED_INSET),
    )));
/// See [`LIST_HEADER_ACTIVE_SHADOW_BOTTOM`].
pub const LIST_HEADER_ACTIVE_SHADOW_RIGHT: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(CssProperty::BoxShadowRight(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&LIST_HEADER_PRESSED_INSET),
    )));
/// See [`LIST_HEADER_ACTIVE_SHADOW_BOTTOM`].
pub const LIST_HEADER_ACTIVE_SHADOW_LEFT: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(CssProperty::BoxShadowLeft(StyleBoxShadowValue::Exact(
        BoxOrStatic::Static(&LIST_HEADER_PRESSED_INSET),
    )));

/// The dark twin of [`LIST_HEADER_ACTIVE_SHADOW_BOTTOM`]: the same glow in
/// [`DARK_PB`], the pressed face's bottom stop — what an inset shadow shades a
/// face toward.
pub const LIST_HEADER_ACTIVE_SHADOW_BOTTOM_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(CssProperty::BoxShadowBottom(
        StyleBoxShadowValue::Exact(BoxOrStatic::Static(&LIST_HEADER_PRESSED_INSET_DARK)),
    ));
/// The dark twin of [`LIST_HEADER_ACTIVE_SHADOW_TOP`].
pub const LIST_HEADER_ACTIVE_SHADOW_TOP_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(CssProperty::BoxShadowTop(
        StyleBoxShadowValue::Exact(BoxOrStatic::Static(&LIST_HEADER_PRESSED_INSET_DARK)),
    ));
/// The dark twin of [`LIST_HEADER_ACTIVE_SHADOW_RIGHT`].
pub const LIST_HEADER_ACTIVE_SHADOW_RIGHT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(CssProperty::BoxShadowRight(
        StyleBoxShadowValue::Exact(BoxOrStatic::Static(&LIST_HEADER_PRESSED_INSET_DARK)),
    ));
/// The dark twin of [`LIST_HEADER_ACTIVE_SHADOW_LEFT`].
pub const LIST_HEADER_ACTIVE_SHADOW_LEFT_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(CssProperty::BoxShadowLeft(
        StyleBoxShadowValue::Exact(BoxOrStatic::Static(&LIST_HEADER_PRESSED_INSET_DARK)),
    ));

/// Column-header pressed border, bottom edge — light:
/// [`LIGHT_LIST_HEADER_PRESSED_BORDER`]. Four edges; pair each with its `_DARK`
/// twin.
pub const LIST_HEADER_ACTIVE_BORDER_BOTTOM_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: LIGHT_LIST_HEADER_PRESSED_BORDER,
        },
    ));
/// See [`LIST_HEADER_ACTIVE_BORDER_BOTTOM_COLOR`].
pub const LIST_HEADER_ACTIVE_BORDER_LEFT_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: LIGHT_LIST_HEADER_PRESSED_BORDER,
        },
    ));
/// See [`LIST_HEADER_ACTIVE_BORDER_BOTTOM_COLOR`].
pub const LIST_HEADER_ACTIVE_BORDER_RIGHT_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: LIGHT_LIST_HEADER_PRESSED_BORDER,
        },
    ));
/// See [`LIST_HEADER_ACTIVE_BORDER_BOTTOM_COLOR`].
pub const LIST_HEADER_ACTIVE_BORDER_TOP_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_active(CssProperty::const_border_top_color(
        StyleBorderTopColor {
            inner: LIGHT_LIST_HEADER_PRESSED_BORDER,
        },
    ));

/// The dark twin of [`LIST_HEADER_ACTIVE_BORDER_BOTTOM_COLOR`]: [`DARK_BD`],
/// the default border — the light value is a neutral grey-blue, not an accent.
pub const LIST_HEADER_ACTIVE_BORDER_BOTTOM_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_BD },
    ));
/// The dark twin of [`LIST_HEADER_ACTIVE_BORDER_LEFT_COLOR`].
pub const LIST_HEADER_ACTIVE_BORDER_LEFT_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_BD },
    ));
/// The dark twin of [`LIST_HEADER_ACTIVE_BORDER_RIGHT_COLOR`].
pub const LIST_HEADER_ACTIVE_BORDER_RIGHT_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_BD },
    ));
/// The dark twin of [`LIST_HEADER_ACTIVE_BORDER_TOP_COLOR`].
pub const LIST_HEADER_ACTIVE_BORDER_TOP_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_BD },
    ));

/// Column-header pressed face — light: [`LIGHT_LIST_HEADER_PRESSED`]. Pair
/// with [`LIST_HEADER_ACTIVE_BG_DARK`].
pub const LIST_HEADER_ACTIVE_BG: CssPropertyWithConditions = CssPropertyWithConditions::on_active(
    CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
        StyleBackgroundContent::Color(LIGHT_LIST_HEADER_PRESSED),
    ])),
);

/// The dark twin of [`LIST_HEADER_ACTIVE_BG`]: [`DARK_PT`], the pressed
/// control face.
pub const LIST_HEADER_ACTIVE_BG_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_active(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(DARK_PT)]),
    ));

// -- the row ----------------------------------------------------------------

/// Row hover ring, bottom edge — light: [`LIGHT_LIST_ROW_HOVER_BORDER`]. Four
/// edges; pair each with its `_DARK` twin.
pub const LIST_ROW_HOVER_BORDER_BOTTOM_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: LIGHT_LIST_ROW_HOVER_BORDER,
        },
    ));
/// See [`LIST_ROW_HOVER_BORDER_BOTTOM_COLOR`].
pub const LIST_ROW_HOVER_BORDER_LEFT_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: LIGHT_LIST_ROW_HOVER_BORDER,
        },
    ));
/// See [`LIST_ROW_HOVER_BORDER_BOTTOM_COLOR`].
pub const LIST_ROW_HOVER_BORDER_RIGHT_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: LIGHT_LIST_ROW_HOVER_BORDER,
        },
    ));
/// See [`LIST_ROW_HOVER_BORDER_BOTTOM_COLOR`].
pub const LIST_ROW_HOVER_BORDER_TOP_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: LIGHT_LIST_ROW_HOVER_BORDER,
    }));

/// The dark twin of [`LIST_ROW_HOVER_BORDER_BOTTOM_COLOR`]: [`DARK_SOFT`], the
/// muted accent — one step below the focus ring's [`DARK_ACC`], as #65B5DC is
/// one step below #26A0DA in light mode.
pub const LIST_ROW_HOVER_BORDER_BOTTOM_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_SOFT },
    ));
/// The dark twin of [`LIST_ROW_HOVER_BORDER_LEFT_COLOR`].
pub const LIST_ROW_HOVER_BORDER_LEFT_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_SOFT },
    ));
/// The dark twin of [`LIST_ROW_HOVER_BORDER_RIGHT_COLOR`].
pub const LIST_ROW_HOVER_BORDER_RIGHT_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_SOFT },
    ));
/// The dark twin of [`LIST_ROW_HOVER_BORDER_TOP_COLOR`].
pub const LIST_ROW_HOVER_BORDER_TOP_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_SOFT },
    ));

/// Row hover fill — light: [`LIGHT_LIST_ROW_HOVER`]. Pair with
/// [`LIST_ROW_HOVER_BG_DARK`].
pub const LIST_ROW_HOVER_BG: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
        StyleBackgroundContent::Color(LIGHT_LIST_ROW_HOVER),
    ])),
);

/// The dark twin of [`LIST_ROW_HOVER_BG`]: [`DARK_ROW_HOVER`], the fill a
/// hovered tree row already takes.
pub const LIST_ROW_HOVER_BG_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(
            DARK_ROW_HOVER,
        )]),
    ));

/// Focused-row ring, bottom edge — light: [`LIGHT_LIST_ROW_FOCUS_BORDER`].
/// Four edges; pair each with its `_DARK` twin.
pub const LIST_ROW_FOCUS_BORDER_BOTTOM_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: LIGHT_LIST_ROW_FOCUS_BORDER,
        },
    ));
/// See [`LIST_ROW_FOCUS_BORDER_BOTTOM_COLOR`].
pub const LIST_ROW_FOCUS_BORDER_LEFT_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: LIGHT_LIST_ROW_FOCUS_BORDER,
        },
    ));
/// See [`LIST_ROW_FOCUS_BORDER_BOTTOM_COLOR`].
pub const LIST_ROW_FOCUS_BORDER_RIGHT_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: LIGHT_LIST_ROW_FOCUS_BORDER,
        },
    ));
/// See [`LIST_ROW_FOCUS_BORDER_BOTTOM_COLOR`].
pub const LIST_ROW_FOCUS_BORDER_TOP_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: LIGHT_LIST_ROW_FOCUS_BORDER,
    }));

/// The dark twin of [`LIST_ROW_FOCUS_BORDER_BOTTOM_COLOR`]: [`DARK_ACC`] — a
/// focus ring is the accent, as it is for every other focused control in this
/// theme.
pub const LIST_ROW_FOCUS_BORDER_BOTTOM_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor { inner: DARK_ACC },
    ));
/// The dark twin of [`LIST_ROW_FOCUS_BORDER_LEFT_COLOR`].
pub const LIST_ROW_FOCUS_BORDER_LEFT_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor { inner: DARK_ACC },
    ));
/// The dark twin of [`LIST_ROW_FOCUS_BORDER_RIGHT_COLOR`].
pub const LIST_ROW_FOCUS_BORDER_RIGHT_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor { inner: DARK_ACC },
    ));
/// The dark twin of [`LIST_ROW_FOCUS_BORDER_TOP_COLOR`].
pub const LIST_ROW_FOCUS_BORDER_TOP_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_top_color(
        StyleBorderTopColor { inner: DARK_ACC },
    ));

/// Focused-row fill — light: [`LIGHT_LIST_ROW_FOCUS`]. Pair with
/// [`LIST_ROW_FOCUS_BG_DARK`].
pub const LIST_ROW_FOCUS_BG: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
        StyleBackgroundContent::Color(LIGHT_LIST_ROW_FOCUS),
    ])),
);

/// The dark twin of [`LIST_ROW_FOCUS_BG`]: [`DARK_GLOW`], the translucent
/// accent.
///
/// Over the dark field it tints the row toward the accent the way #B8E0F3 does
/// over white, and stays distinguishable from a hovered row's
/// [`DARK_ROW_HOVER`].
pub const LIST_ROW_FOCUS_BG_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_focus(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(DARK_GLOW)]),
    ));

// == /STATES: list_view ==

//
//
//

// == STATES: tabs ==

// ---------------------------------------------------------------------------
// INTERACTIVE STATES — tabs
// ---------------------------------------------------------------------------
//
// The hover of an inactive tab, in the Windows-classic idiom `tabs.rs` draws:
//
//     .__azul-native-tabs-tab-not-active:hover {
//         border: 1px solid #7EB4EA;
//         background: linear-gradient(#ECF4FC, #DDEDFC);
//     }
//
// Thirteen declarations once the `border` shorthand is expanded — four widths,
// four styles, four colours — plus the fill, and they are one set. The widths
// and styles are not decoration: a `-noleftborder` / `-norightborder` tab has
// nulled the edge it shares with the active tab, and these are what draw it
// back while the pointer is over the tab, so the hover ring is whole. Take the
// colours without the widths and the two tabs beside the active one hover with
// a side missing.
//
// A width or a style has no per-mode value, so its dark twin repeats it. That
// is deliberate: every rule in this section is a pair, with no exceptions a
// reader would have to know about, and "dark >= light" is the invariant the
// check script holds the themes to.

/// Hover border of an inactive tab, light mode (#7EB4EA).
///
/// The Windows "hot" tab edge: a SOFT accent blue, neither the accent itself
/// nor the neutral #ACACAC the tab rests at. On this palette's accent ramp it
/// is the `SOFT` rung — [`LIGHT_SOFT`] is #6EA8FE — and that is what picks its
/// dark twin.
pub const LIGHT_TAB_HOVER_BORDER: ColorU = ColorU {
    r: 126,
    g: 180,
    b: 234,
    a: 255,
};

/// Hover border of an inactive tab, dark mode: [`DARK_SOFT`], the same rung of
/// the ramp in the other mode.
///
/// Not [`DARK_ACC`], which is the full accent and would ring a hovered tab
/// harder in dark mode than in light; not [`DARK_BD`], which is the neutral
/// border and would lose the tint altogether.
pub const DARK_TAB_HOVER_BORDER: ColorU = DARK_SOFT;

/// Top stop of an inactive tab's hover fill, light mode (#ECF4FC).
pub const LIGHT_TAB_HOVER_TOP: ColorU = ColorU {
    r: 236,
    g: 244,
    b: 252,
    a: 255,
};

/// Bottom stop of an inactive tab's hover fill, light mode (#DDEDFC).
pub const LIGHT_TAB_HOVER_BOTTOM: ColorU = ColorU {
    r: 221,
    g: 237,
    b: 252,
    a: 255,
};

// The light fill is a top-to-bottom gradient, so the dark one is too, and its
// stops are this theme's own hover pair: a tab strip is a page-neutral surface,
// and `HT` -> `HB` is what the plan names for a hovered control face.
const TAB_HOVER_STOPS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(LIGHT_TAB_HOVER_TOP),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(LIGHT_TAB_HOVER_BOTTOM),
    },
];

const TAB_HOVER_STOPS_DARK: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(DARK_HT),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(DARK_HB),
    },
];

const TAB_HOVER_FILL: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(TAB_HOVER_STOPS),
    })];

const TAB_HOVER_FILL_DARK: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(TAB_HOVER_STOPS_DARK),
    })];

/// A hovered inactive tab's border: 1px on every edge — light mode.
///
/// The first of the thirteen rules in [`TAB_HOVER_STATES`]; use the set, not
/// the rule. A border width has no per-mode value, so the dark twin repeats it.
pub const TAB_HOVER_BORDER_BOTTOM_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_LEFT_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_RIGHT_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_width(
        LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_TOP_WIDTH: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_width(
        LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_BOTTOM_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_LEFT_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_style(
        StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_RIGHT_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_TOP_STYLE: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Solid,
    }));

/// See [`TAB_HOVER_STATES`]. The colour is [`LIGHT_TAB_HOVER_BORDER`].
pub const TAB_HOVER_BORDER_BOTTOM_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: LIGHT_TAB_HOVER_BORDER,
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_LEFT_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: LIGHT_TAB_HOVER_BORDER,
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_RIGHT_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: LIGHT_TAB_HOVER_BORDER,
        },
    ));

/// See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BORDER_TOP_COLOR: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: LIGHT_TAB_HOVER_BORDER,
    }));

/// A hovered inactive tab's fill — light mode: the [`LIGHT_TAB_HOVER_TOP`] to
/// [`LIGHT_TAB_HOVER_BOTTOM`] gradient. See [`TAB_HOVER_STATES`].
pub const TAB_HOVER_BG: CssPropertyWithConditions =
    CssPropertyWithConditions::on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(TAB_HOVER_FILL),
    ));

/// The dark twin of [`TAB_HOVER_BORDER_BOTTOM_WIDTH`] — the same width; see
/// the section comment for why a twin exists at all.
pub const TAB_HOVER_BORDER_BOTTOM_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_LEFT_WIDTH`].
pub const TAB_HOVER_BORDER_LEFT_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_RIGHT_WIDTH`].
pub const TAB_HOVER_BORDER_RIGHT_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_right_width(
        LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_TOP_WIDTH`].
pub const TAB_HOVER_BORDER_TOP_WIDTH_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_top_width(
        LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_BOTTOM_STYLE`].
pub const TAB_HOVER_BORDER_BOTTOM_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_LEFT_STYLE`].
pub const TAB_HOVER_BORDER_LEFT_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_left_style(
        StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_RIGHT_STYLE`].
pub const TAB_HOVER_BORDER_RIGHT_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_TOP_STYLE`].
pub const TAB_HOVER_BORDER_TOP_STYLE_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_top_style(
        StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_BOTTOM_COLOR`]: [`DARK_TAB_HOVER_BORDER`].
pub const TAB_HOVER_BORDER_BOTTOM_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: DARK_TAB_HOVER_BORDER,
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_LEFT_COLOR`].
pub const TAB_HOVER_BORDER_LEFT_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: DARK_TAB_HOVER_BORDER,
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_RIGHT_COLOR`].
pub const TAB_HOVER_BORDER_RIGHT_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: DARK_TAB_HOVER_BORDER,
        },
    ));

/// The dark twin of [`TAB_HOVER_BORDER_TOP_COLOR`].
pub const TAB_HOVER_BORDER_TOP_COLOR_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_top_color(
        StyleBorderTopColor {
            inner: DARK_TAB_HOVER_BORDER,
        },
    ));

/// The dark twin of [`TAB_HOVER_BG`]: the [`DARK_HT`] to [`DARK_HB`] gradient.
pub const TAB_HOVER_BG_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::dark_on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(TAB_HOVER_FILL_DARK),
    ));

/// Every state an inactive tab takes: the hover ring (four widths, four
/// styles, four colours) and the hover fill, each with its dark twin.
///
/// One array so a theme function can append the whole set in a line and cannot
/// ship half of it. `tabs.rs` holds its styles in const slices, which cannot
/// splice an array, so it names these twenty-six one by one — in this order.
pub const TAB_HOVER_STATES: [CssPropertyWithConditions; 26] = [
    TAB_HOVER_BORDER_BOTTOM_WIDTH,
    TAB_HOVER_BORDER_LEFT_WIDTH,
    TAB_HOVER_BORDER_RIGHT_WIDTH,
    TAB_HOVER_BORDER_TOP_WIDTH,
    TAB_HOVER_BORDER_BOTTOM_STYLE,
    TAB_HOVER_BORDER_LEFT_STYLE,
    TAB_HOVER_BORDER_RIGHT_STYLE,
    TAB_HOVER_BORDER_TOP_STYLE,
    TAB_HOVER_BORDER_BOTTOM_COLOR,
    TAB_HOVER_BORDER_LEFT_COLOR,
    TAB_HOVER_BORDER_RIGHT_COLOR,
    TAB_HOVER_BORDER_TOP_COLOR,
    TAB_HOVER_BG,
    TAB_HOVER_BORDER_BOTTOM_WIDTH_DARK,
    TAB_HOVER_BORDER_LEFT_WIDTH_DARK,
    TAB_HOVER_BORDER_RIGHT_WIDTH_DARK,
    TAB_HOVER_BORDER_TOP_WIDTH_DARK,
    TAB_HOVER_BORDER_BOTTOM_STYLE_DARK,
    TAB_HOVER_BORDER_LEFT_STYLE_DARK,
    TAB_HOVER_BORDER_RIGHT_STYLE_DARK,
    TAB_HOVER_BORDER_TOP_STYLE_DARK,
    TAB_HOVER_BORDER_BOTTOM_COLOR_DARK,
    TAB_HOVER_BORDER_LEFT_COLOR_DARK,
    TAB_HOVER_BORDER_RIGHT_COLOR_DARK,
    TAB_HOVER_BORDER_TOP_COLOR_DARK,
    TAB_HOVER_BG_DARK,
];

// == /STATES: tabs ==

//
//
//

// == STATES: text_input ==
//
// text_input declares no states of its own: it takes `FIELD_BORDER_STATES`
// (the text-fields section above), because its eight rules were the same
// accent ring on hover and on focus that text_area had, and `text_input()`
// appends that array rather than a second copy of it. The one difference the
// widget file used to make — a grey hover ring on Windows only — was a
// platform distinction neither theme draws, so it went with the move.
//
// == /STATES: text_input ==

//
//
//

// == STATES: chrome ==

// ---------------------------------------------------------------------------
// INTERACTIVE STATES — chrome (ribbon, statusbar, quick_access, backstage)
// ---------------------------------------------------------------------------
//
// The four chrome widgets carry their own palettes (`RibbonTheme`,
// `StatusBarTheme`, `QuickAccessTheme`, `BackstageTheme`) whose colours are
// RUNTIME values — an Office preset, or whatever `from_system` read off the
// desktop — so their states cannot be consts. They are builders instead, like
// [`hover_bg_both`] and [`active_bg_both`] above, and every builder returns
// BOTH halves of a rule in one array, so a caller cannot take the light half
// without the dark one.
//
// The dark half is the caller's to choose, and the rule is the one
// `button_states` applies: a surface that is its own colour in both modes (the
// backstage's blue nav column, the ribbon's accent-filled application button,
// the status bar's accent strip) keeps its light state colour, because the
// surface does not change in dark mode either; a page-neutral surface (the
// ribbon chrome, the quick-access band) takes this theme's tokens —
// [`DARK_HT`] / [`DARK_PT`] for the fills, [`DARK_BD`] for a hover border,
// [`DARK_ACC`] for hovered accent text and focus rings.

/// Border colour on hover, all four edges, with BOTH halves chosen by the
/// caller.
///
/// Eight rules, because a border colour is four properties and a hover that
/// sets only some edges leaves the rest at their resting colour. The four dark
/// rules follow the four light ones so that they win in dark mode (inline
/// declarations resolve last-match-wins).
#[must_use]
pub fn hover_border_both(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 8] {
    [
        CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(
            StyleBorderTopColor { inner: light },
        )),
        CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(
            StyleBorderLeftColor { inner: light },
        )),
        CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(
            StyleBorderRightColor { inner: light },
        )),
        CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor { inner: light },
        )),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_top_color(
            StyleBorderTopColor { inner: dark },
        )),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_left_color(
            StyleBorderLeftColor { inner: dark },
        )),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_right_color(
            StyleBorderRightColor { inner: dark },
        )),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor { inner: dark },
        )),
    ]
}

/// Border colour on focus, all four edges, with BOTH halves chosen by the
/// caller: the focus ring of a field whose accent is a runtime colour.
///
/// The const [`FOCUS_BORDER_TOP`] family is the same ring at this theme's own
/// accent; this is for a palette that brings its own. Same shape as
/// [`hover_border_both`], for the same reason.
#[must_use]
pub fn focus_border_both(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 8] {
    [
        CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(
            StyleBorderTopColor { inner: light },
        )),
        CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(
            StyleBorderLeftColor { inner: light },
        )),
        CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(
            StyleBorderRightColor { inner: light },
        )),
        CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor { inner: light },
        )),
        CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_top_color(
            StyleBorderTopColor { inner: dark },
        )),
        CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_left_color(
            StyleBorderLeftColor { inner: dark },
        )),
        CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_right_color(
            StyleBorderRightColor { inner: dark },
        )),
        CssPropertyWithConditions::dark_on_focus(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor { inner: dark },
        )),
    ]
}

/// Text colour on hover, with BOTH halves chosen by the caller — a tab header
/// that takes the accent when hovered, say. Pass [`DARK_ACC`] as `dark` when
/// the light colour is the palette's accent.
#[must_use]
pub const fn hover_text_color_both(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 2] {
    [
        CssPropertyWithConditions::on_hover(CssProperty::const_text_color(StyleTextColor {
            inner: light,
        })),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_text_color(StyleTextColor {
            inner: dark,
        })),
    ]
}

/// Corner radius on hover, all four corners, paired with an identical dark
/// twin.
///
/// A radius has no colour, so the twin repeats the value. It is emitted anyway
/// so that the invariant this section exists for — every state rule has a dark
/// twin — holds without an exception to remember, the same way `button_states`
/// pairs the link button's underline.
#[must_use]
pub const fn hover_radius_pair(radius: PixelValue) -> [CssPropertyWithConditions; 8] {
    [
        CssPropertyWithConditions::on_hover(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius { inner: radius },
        )),
        CssPropertyWithConditions::on_hover(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius { inner: radius },
        )),
        CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius { inner: radius },
        )),
        CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius { inner: radius },
        )),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius { inner: radius },
        )),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius { inner: radius },
        )),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius { inner: radius },
        )),
        CssPropertyWithConditions::dark_on_hover(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius { inner: radius },
        )),
    ]
}

// == /STATES: chrome ==

//
//
//
