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

use super::system_palette;
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
    // A caller who injected a container style (`Some`) chose every property in
    // it — resting face, dark colours and states included. The chrome widgets
    // hand in part styles complete with their own hover/pressed pairs, and
    // anything this theme appended after them would win the cascade (inline
    // resolution is last-match) and paint the theme's greys over the ribbon's
    // blue. So the theme adds to its OWN default only.
    let btn_owns_style = btn.container_style.as_ref().is_none();
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
            crate::widgets::widget_p_chrome()
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

    if btn_owns_style {
        // The dark face and ink — for the NEUTRAL surface only. A coloured
        // command (primary, danger, ...) is its own colour in both modes and
        // a link has no face; painting a dark face over every type, as this
        // once did, turned a blue primary button into an invisible dark box
        // on a dark window. (`ButtonType::surface` is the one place that rule
        // lives; `button_states` reads it too.)
        //
        // The neutral face is the DESKTOP's button: `system:button-face` and
        // `system:button-text`, with the outline in `system:separator` - the
        // light grey outline of the light face would otherwise ring a dark
        // button in light grey.
        match btn_type.surface() {
            crate::widgets::button::ButtonSurface::Neutral => {
                container_style.push(system_palette::DARK_BUTTON_FACE);
                container_style.push(system_palette::DARK_BUTTON_TEXT);
                container_style.extend(system_palette::dark_border(system_palette::SEPARATOR));
            }
            // A link is text on whatever surface it sits on: in the dark
            // theme it takes the desktop's link colour, which is chosen to
            // read on a dark surface - the light theme's blue is not.
            crate::widgets::button::ButtonSurface::NoSurface => {
                container_style.push(system_palette::DARK_LINK);
            }
            crate::widgets::button::ButtonSurface::OwnColour => {}
        }

        // The interactive states go LAST. Inline declarations resolve last-match
        // wins and a `dark_theme(..)` rule matches in every pseudo-state, so any
        // dark resting colour pushed after a `dark_on_hover` / `dark_on_focus` twin
        // would shadow it — no ring, no hover face, in dark mode.
        container_style.extend(button_states(btn_type));
    }

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

    // The box is a field: in the dark theme it sits on the desktop's
    // `system:control-background`, like the text fields next to it.
    let mut container_style: Vec<CssPropertyWithConditions> =
        cb.resolved_container_style().as_slice().to_vec();
    container_style.push(system_palette::DARK_CONTROL_BACKGROUND);
    // The checked mark in the dark theme: the desktop's label colour, which
    // reads on the dark field the way the light theme's grey mark reads on
    // white.
    let is_checked = cb.check_box_state.inner.checked;
    let mut content_style: Vec<CssPropertyWithConditions> =
        cb.resolved_content_style().as_slice().to_vec();
    if checked_now {
        content_style.push(system_palette::dark_background(SystemColorRef::Text));
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

    // What the line SHOWS - the value, or a password's mask. The engine's
    // buffer is seeded from it, so this is also what every caret offset the
    // engine reports indexes into.
    let label_text: String = crate::widgets::text_input::display_text(&ti.text_input_state.inner);

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

    // The dark field is the DESKTOP's field: `system:control-background`
    // under `system:text`, the colours the native text fields around it use.
    let mut container_style: Vec<CssPropertyWithConditions> =
        resolved_container_style.as_slice().to_vec();
    container_style.push(system_palette::DARK_CONTROL_BACKGROUND);
    container_style.push(system_palette::DARK_TEXT);

    // The interactive states the widget no longer declares. Appended LAST —
    // after the base style and after the theme's own dark resting colours —
    // because the last matching inline declaration wins: a `dark_theme` border
    // pushed after these would beat the dark hover/focus ring. One array so
    // half of them cannot ship.
    container_style.extend_from_slice(&FIELD_BORDER_STATES);

    let mut label_style: Vec<CssPropertyWithConditions> = resolved_label_style.as_slice().to_vec();
    label_style.push(system_palette::DARK_TEXT);
    // After the resting ink, which matches in the `::placeholder` state too.
    label_style.push(FIELD_PLACEHOLDER_DARK);

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

/// The flat bar, mounted in the widget's `VirtualView` wrapper (the same box
/// in every theme: `progressbar::mount`).
#[must_use]
pub fn progressbar(bar: crate::widgets::progressbar::ProgressBar) -> Dom {
    crate::widgets::progressbar::mount(bar, progressbar_render_virtual_view)
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

        // .__azul-native-progress-bar-container: the widget's base (its
        // structure, the same in every theme), then flat's skin.
        let mut container_props = crate::widgets::progressbar::BAR_CONTAINER_BASE.to_vec();
        container_props.extend(vec![
            CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
                LayoutHeight::Px(container_height),
            ))),
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
        ]);
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
                // What the bar measures - only the caller knows; see
                // `ProgressBar::with_accessibility_name`.
                accessibility_name: this.accessibility_name.clone(),
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

    // Flat specific, dark theme: the rail is a groove in the desktop's field
    // colour and the thumb keeps the role it has in the light theme - the
    // accent - in the desktop's own accent.
    track_style.push(system_palette::DARK_CONTROL_BACKGROUND);
    thumb_style.push(system_palette::DARK_ACCENT_BACKGROUND);

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

    // Same field as the text input: `system:control-background` under
    // `system:text` in the dark theme.
    let mut container_style: Vec<CssPropertyWithConditions> =
        resolved_container_style.as_slice().to_vec();
    container_style.push(system_palette::DARK_CONTROL_BACKGROUND);
    container_style.push(system_palette::DARK_TEXT);

    let mut label_style: Vec<CssPropertyWithConditions> = match &ta.label_style {
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(s) => {
            s.as_slice().to_vec()
        }
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
            crate::widgets::text_area::TEXT_AREA_LABEL_PROPS.to_vec()
        }
    };
    label_style.push(system_palette::DARK_TEXT);
    // After the resting ink, which matches in the `::placeholder` state too.
    label_style.push(FIELD_PLACEHOLDER_DARK);

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
/// dark surface; the rules that use it now pair it with `system:separator`.
const FLAT_BORDER_NORMAL: ColorU = LIGHT_BD;

/// Flat's trigger skin, after `drop_down::DROPDOWN_WRAPPER_BASE` (R5).
const FLAT_DROPDOWN_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
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
    // The dark trigger is a field like the text input's: the desktop's
    // `system:control-background` under `system:text`.
    system_palette::DARK_CONTROL_BACKGROUND,
    // The four border colours above are light-mode values; without these the
    // dropdown kept a light-grey outline on a dark surface.
    system_palette::DARK_SEPARATOR_BORDER_TOP,
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
    system_palette::DARK_SEPARATOR_BORDER_LEFT,
    system_palette::DARK_SEPARATOR_BORDER_RIGHT,
    system_palette::DARK_TEXT,
];

/// Flat's label skin, after `drop_down::DROPDOWN_LABEL_BASE` (R5).
const FLAT_DROPDOWN_LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_FG,
    })),
    system_palette::DARK_TEXT,
];

/// Flat's arrow skin, after `drop_down::DROPDOWN_ARROW_BASE` (R5).
const FLAT_DROPDOWN_ARROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LIGHT_FG,
    })),
    system_palette::DARK_TEXT,
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

    // Every part: the widget's structure (R5), then flat's skin.
    use crate::widgets::drop_down::{
        DROPDOWN_ARROW_BASE, DROPDOWN_LABEL_BASE, DROPDOWN_WRAPPER_BASE,
    };

    Dom::create_div()
        .with_css_props(CssPropertyWithConditionsVec::from_vec(
            [DROPDOWN_WRAPPER_BASE, FLAT_DROPDOWN_WRAPPER_STYLE].concat(),
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
            crate::widgets::widget_p_chrome()
                .with_css_props(CssPropertyWithConditionsVec::from_vec(
                    [DROPDOWN_LABEL_BASE, FLAT_DROPDOWN_LABEL_STYLE].concat(),
                ))
                .with_children(DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(selected_text),
                ])),
            Dom::create_icon(AzString::from_const_str("arrow_drop_down")).with_css_props(
                CssPropertyWithConditionsVec::from_vec(
                    [DROPDOWN_ARROW_BASE, FLAT_DROPDOWN_ARROW_STYLE].concat(),
                ),
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

/// The text fields' ring — the border every text field takes on hover and on
/// focus — is `#4286f4`, the value `TextInput`, `TextArea` and `NumberInput`
/// carried before the theme owned their states (the widgets' own
/// `COLOR_4286F4`). It is deliberately NOT [`LIGHT_ACC`]: the accent is the
/// theme's newer blue, and a migration that swaps one light value for another
/// changes what every existing app looks like. The dark twin is [`DARK_ACC`],
/// where the fields had no dark ring at all before.
pub const FIELD_RING: ColorU = ColorU {
    r: 66,
    g: 134,
    b: 244,
    a: 255,
};

/// The focus ring: the focused control's border takes the accent colour.
///
/// One const per edge, because a border colour is four properties and a focus
/// ring that sets only some of them leaves the rest at their resting colour.
/// Each has a dark twin using [`DARK_ACC`] — the accent is the one state colour
/// that genuinely has a per-mode value in both palettes, which is why the plan
/// names it.
pub const FOCUS_BORDER_TOP: CssPropertyWithConditions =
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: FIELD_RING,
    }));

/// See [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_BOTTOM: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: FIELD_RING }),
);

/// See [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_LEFT: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_border_left_color(StyleBorderLeftColor { inner: FIELD_RING }),
);

/// See [`FOCUS_BORDER_TOP`].
pub const FOCUS_BORDER_RIGHT: CssPropertyWithConditions = CssPropertyWithConditions::on_focus(
    CssProperty::const_border_right_color(StyleBorderRightColor { inner: FIELD_RING }),
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
        inner: FIELD_RING,
    }));

/// See [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_BOTTOM: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: FIELD_RING }),
);

/// See [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_LEFT: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_border_left_color(StyleBorderLeftColor { inner: FIELD_RING }),
);

/// See [`HOVER_BORDER_TOP`].
pub const HOVER_BORDER_RIGHT: CssPropertyWithConditions = CssPropertyWithConditions::on_hover(
    CssProperty::const_border_right_color(StyleBorderRightColor { inner: FIELD_RING }),
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

/// The prompt of an empty field in the dark theme: `system:placeholder-text`.
///
/// A field's resting dark ink is a `dark_theme` declaration, and one of those
/// matches in EVERY pseudo-state - `::placeholder` included - so without this
/// the prompt painted exactly as bright as the value in the dark theme. Push it
/// after the resting ink (last match wins); its light half is the field's own
/// `on_placeholder` colour.
pub const FIELD_PLACEHOLDER_DARK: CssPropertyWithConditions =
    CssPropertyWithConditions::with_single_condition(
        CssProperty::const_text_color(StyleTextColor {
            inner: system_palette::PLACEHOLDER_TEXT,
        }),
        &[
            azul_css::dynamic_selector::DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark),
            azul_css::dynamic_selector::DynamicSelector::PseudoState(
                azul_css::dynamic_selector::PseudoStateType::Placeholder,
            ),
        ],
    );

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
    let neutral = button_type.surface() == crate::widgets::button::ButtonSurface::Neutral;
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
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(LIGHT_LIST_HEADER_HOVER_TOP),
    },
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(50),
        color: ColorOrSystem::color(LIGHT_LIST_HEADER_HOVER_TOP),
    },
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(51),
        color: ColorOrSystem::color(LIGHT_LIST_HEADER_HOVER_MID),
    },
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
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
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(LIGHT_TAB_HOVER_TOP),
    },
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(LIGHT_TAB_HOVER_BOTTOM),
    },
];

const TAB_HOVER_STOPS_DARK: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(DARK_HT),
    },
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
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
pub const fn hover_border_both(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 8] {
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
pub const fn focus_border_both(light: ColorU, dark: ColorU) -> [CssPropertyWithConditions; 8] {
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

// ==== dialog ====
//
// Dialog, Modal and Popover share one builder (`widgets::dialog::build_dialog`);
// what a theme hands it is a skin, the style of every part. Flat is the
// established look, unchanged: white paper with a #ccc hairline and an 8px
// radius, the desktop's window surface and separator in the dark, a 50% black
// `::backdrop`. What it adds is the focus ring the close glyph never had: a
// transparent 1px ring slot at rest, `FIELD_RING` / `DARK_ACC` on focus, and
// the ink hover every flat quiet control takes.

/// Flat's dialog skin (also the modal's; the popover swaps in its panel).
#[must_use]
pub(crate) fn dialog_skin() -> crate::widgets::dialog::DialogSkin {
    use super::style_kit as kit;
    use crate::widgets::dialog as d;

    // Every part: the dialog's structure (R5), then flat's skin.
    let mut close = d::DIALOG_CLOSE_BASE.to_vec();
    close.extend_from_slice(d::DIALOG_CLOSE_STYLE);
    close.extend(kit::radius(3));
    close.extend(kit::ring_slot());
    // States last: a resting dark twin matches in every state.
    close.extend(kit::hover_ink(LIGHT_INK, system_palette::TEXT));
    close.extend(kit::focus_ring(FIELD_RING, DARK_ACC));

    d::DialogSkin {
        theme: super::UiTheme::Flat,
        panel: CssPropertyWithConditionsVec::from_vec(
            [d::DIALOG_PANEL_BASE, d::DIALOG_PANEL_STYLE].concat(),
        ),
        title: CssPropertyWithConditionsVec::from_vec(
            [d::DIALOG_TITLE_BASE, d::DIALOG_TITLE_STYLE].concat(),
        ),
        close_row: CssPropertyWithConditionsVec::from_const_slice(d::DIALOG_CLOSE_ROW_STYLE),
        close: CssPropertyWithConditionsVec::from_vec(close),
        content: CssPropertyWithConditionsVec::from_const_slice(d::DIALOG_CONTENT_STYLE),
        backdrop: d::default_backdrop_style(),
    }
}

/// Flat's popover panel: the established small bordered surface.
#[must_use]
pub fn popover_panel_style() -> CssPropertyWithConditionsVec {
    crate::widgets::popover::build_panel_style()
}

/// Renders a [`crate::widgets::dialog::Dialog`] in the flat theme.
#[must_use]
pub fn dialog(d: crate::widgets::dialog::Dialog) -> Dom {
    d.build(dialog_skin())
}

/// Renders a [`crate::widgets::modal::Modal`] in the flat theme.
#[must_use]
pub fn modal(m: crate::widgets::modal::Modal) -> Dom {
    m.build(dialog_skin())
}

/// Renders a [`crate::widgets::popover::Popover`] in the flat theme.
#[must_use]
pub fn popover(p: crate::widgets::popover::Popover) -> Dom {
    let mut skin = dialog_skin();
    skin.panel = popover_panel_style();
    p.build(skin)
}

// ==== number_input ====
//
// A NumberInput draws nothing of its own: the TextInput it wraps is the field.
// Flat hands it the flat theme - the established white field, the desktop's
// field in the dark, the `FIELD_RING` / `DARK_ACC` ring - and marks the root.

/// Renders a [`crate::widgets::number_input::NumberInput`] in the flat theme.
#[must_use]
pub fn number_input(mut n: crate::widgets::number_input::NumberInput) -> Dom {
    n.text_input.set_theme(super::UiTheme::Flat);
    let mut dom = n.build();
    dom.add_class(AzString::from_const_str(super::style_kit::FLAT_CLASS));
    dom
}

// ==== pagination ====
//
// Flat is the established bar: white paper pages under a #ced4da hairline, the
// accent page in the fixed accent blue, the desktop's button face and separator
// in the dark. What it adds are the states the bar never had: a neutral page
// hovers to `LIGHT_HT` / `DARK_HT` and presses to `LIGHT_PT` / `DARK_PT`, and
// every button is ringed on focus - an inset 2px ring, because the inner
// buttons share their side borders and a border ring would miss an edge
// (`FIELD_RING` / `DARK_ACC`; white on the accent page, where blue would vanish).

/// Flat's pagination skin.
#[must_use]
pub(crate) fn pagination_skin() -> crate::widgets::pagination::PaginationSkin {
    crate::widgets::pagination::PaginationSkin {
        theme: super::UiTheme::Flat,
        button: pagination_button,
    }
}

/// One flat pagination button: the established face and dark twins, then the
/// states.
fn pagination_button(
    face: crate::widgets::pagination::PageFace,
    is_first: bool,
    is_last: bool,
) -> CssPropertyWithConditionsVec {
    use super::style_kit as kit;
    use crate::widgets::pagination::{button_style, PageFace};

    let mut v = button_style(
        face == PageFace::Current,
        face == PageFace::Disabled,
        is_first,
        is_last,
    )
    .into_library_owned_vec();
    if face == PageFace::Neutral {
        v.extend(kit::hover_bg(LIGHT_HT, DARK_HT));
        v.extend(kit::active_bg(LIGHT_PT, DARK_PT));
    }
    let (ring, ring_dark) = if face == PageFace::Current {
        (LIGHT_ON_ACC, DARK_ON_ACC)
    } else {
        (FIELD_RING, DARK_ACC)
    };
    v.extend(kit::focus_shadow_ring(ring, ring_dark));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Renders a [`crate::widgets::pagination::Pagination`] in the flat theme.
#[must_use]
pub fn pagination(p: crate::widgets::pagination::Pagination) -> Dom {
    p.build(pagination_skin())
}

// ==== radio_group ====
//
// Flat is the established group: the #9b9b9b ring holding the accent dot (the
// desktop's accent in the dark), labels in the page's ink. What it adds is the
// focus ring the rows never had - the row is the focusable radio (the group's
// Tab stop, and the arrow keys' targets), so the row gets a transparent 1px ring
// slot and a little inset, and takes `FIELD_RING` / `DARK_ACC` on focus. The
// indicator's fixed geometry is the widget's own and stays.

/// Flat's radio-group skin for a group laid out `horizontal`ly or not.
#[must_use]
pub(crate) fn radio_group_skin(horizontal: bool) -> crate::widgets::radio_group::RadioGroupSkin {
    use super::style_kit as kit;
    use crate::widgets::radio_group as r;

    let mut row = r::build_row_style(horizontal).into_library_owned_vec();
    row.extend(kit::padding(1, 4, 1, 2));
    row.extend(kit::radius(3));
    row.extend(kit::ring_slot());
    row.extend(kit::focus_ring(FIELD_RING, DARK_ACC));

    // The indicator: the widget's base (its structure, the same in every
    // theme), then flat's established skin.
    let on_base = |base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]| {
        CssPropertyWithConditionsVec::from_vec([base, skin].concat())
    };

    r::RadioGroupSkin {
        theme: super::UiTheme::Flat,
        row: CssPropertyWithConditionsVec::from_vec(row),
        circle: on_base(r::RADIO_GROUP_CIRCLE_BASE, r::RADIO_GROUP_CIRCLE_STYLE),
        dot_selected: on_base(r::RADIO_GROUP_DOT_BASE, r::RADIO_GROUP_DOT_STYLE_SELECTED),
        dot_unselected: on_base(r::RADIO_GROUP_DOT_BASE, r::RADIO_GROUP_DOT_STYLE_UNSELECTED),
        label: CssPropertyWithConditionsVec::from_const_slice(r::RADIO_GROUP_LABEL_STYLE),
    }
}

/// Renders a [`crate::widgets::radio_group::RadioGroup`] in the flat theme.
#[must_use]
pub fn radio_group(rg: crate::widgets::radio_group::RadioGroup) -> Dom {
    let skin = radio_group_skin(rg.radio_group_state.horizontal);
    rg.build(skin)
}

// ==== segmented ====
//
// Flat is the established control: white segments under a #ced4da hairline,
// the choice in the fixed accent, the desktop's button face and accent in the
// dark, and the same colours from the selection restyle. What it adds are the
// states it never had: an unselected segment hovers to `LIGHT_HT` / `DARK_HT`
// and presses to `LIGHT_PT` / `DARK_PT`, and every segment is ringed on focus
// with an inset 2px ring (inner segments share their side borders) -
// `FIELD_RING` / `DARK_ACC`, white on the accent choice.

/// Flat's segmented skin.
#[must_use]
pub(crate) fn segmented_skin() -> crate::widgets::segmented::SegmentedSkin {
    crate::widgets::segmented::SegmentedSkin {
        theme: super::UiTheme::Flat,
        segment: segmented_segment,
    }
}

/// One flat segment: the established face and dark twins, then the states.
fn segmented_segment(selected: bool, is_first: bool, is_last: bool) -> CssPropertyWithConditionsVec {
    use super::style_kit as kit;

    let mut v = crate::widgets::segmented::segment_style(selected, is_first, is_last)
        .into_library_owned_vec();
    if !selected {
        v.extend(kit::hover_bg(LIGHT_HT, DARK_HT));
        v.extend(kit::active_bg(LIGHT_PT, DARK_PT));
    }
    let (ring, ring_dark) = if selected {
        (LIGHT_ON_ACC, DARK_ON_ACC)
    } else {
        (FIELD_RING, DARK_ACC)
    };
    v.extend(kit::focus_shadow_ring(ring, ring_dark));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Renders a [`crate::widgets::segmented::Segmented`] in the flat theme.
#[must_use]
pub fn segmented(s: crate::widgets::segmented::Segmented) -> Dom {
    s.build(segmented_skin())
}

// ==== split_pane ====
//
// Flat is the established divider: a 6px #adb5bd bar, the desktop's separator
// in the dark. What it adds is the focus it never showed - the divider is the
// splitter's keyboard handle (Tab, then the arrows) - so on focus the whole bar
// lights in `FIELD_RING` / `DARK_ACC` and carries an inset ring of the same
// colour: on a 6px bar a ring alone would be two hairlines.

/// Flat's split-pane skin for a pane split in `direction`.
#[must_use]
pub(crate) fn split_pane_skin(
    direction: crate::widgets::split_pane::SplitDirection,
) -> crate::widgets::split_pane::SplitPaneSkin {
    use super::style_kit as kit;
    use crate::widgets::split_pane as s;

    let mut divider = s::divider_style(direction).into_library_owned_vec();
    // States last: the resting dark twin matches in every state.
    divider.extend(CssPropertyWithConditions::themed_on_focus(
        kit::bg(FIELD_RING),
        kit::bg(DARK_ACC),
    ));
    divider.extend(kit::focus_shadow_ring(FIELD_RING, DARK_ACC));

    s::SplitPaneSkin {
        theme: super::UiTheme::Flat,
        divider: CssPropertyWithConditionsVec::from_vec(divider),
    }
}

/// Renders a [`crate::widgets::split_pane::SplitPane`] in the flat theme.
#[must_use]
pub fn split_pane(sp: crate::widgets::split_pane::SplitPane) -> Dom {
    let skin = split_pane_skin(sp.split_pane_state.inner.direction);
    sp.build(skin)
}

// ==== stepper ====
//
// Flat is the established stepper: accent circles and line for the way walked,
// #e9ecef circles and a #ced4da line ahead, dark / muted labels, the desktop's
// quiet highlight and label colours in the dark - and the same colours from the
// click restyle. What it adds is the focus ring the step cells never had: each
// cell is a tab stop, so it takes an inset 2px ring (`FIELD_RING` / `DARK_ACC`)
// on focus - declared for `:focus` only, so the resting cell is unchanged.

/// Flat's stepper skin.
#[must_use]
pub(crate) fn stepper_skin() -> crate::widgets::stepper::StepperSkin {
    use crate::widgets::stepper as s;
    s::StepperSkin {
        theme: super::UiTheme::Flat,
        cell: stepper_cell,
        circle: stepper_circle,
        connector: stepper_connector,
        label: stepper_label,
    }
}

fn stepper_cell() -> CssPropertyWithConditionsVec {
    let mut v = crate::widgets::stepper::STEPPER_STEP_STYLE.to_vec();
    v.extend(super::style_kit::focus_shadow_ring(FIELD_RING, DARK_ACC));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn stepper_circle(reached: bool) -> CssPropertyWithConditionsVec {
    use crate::widgets::stepper as s;
    s::with_dark_twins(s::circle_style(reached), &s::circle_dark_twins(reached))
}

fn stepper_connector(fill: crate::widgets::stepper::ConnFill) -> CssPropertyWithConditionsVec {
    use crate::widgets::stepper as s;
    s::with_dark_twins(s::connector_style(fill), &s::connector_dark_twins(fill))
}

fn stepper_label(reached: bool) -> CssPropertyWithConditionsVec {
    use crate::widgets::stepper as s;
    s::with_dark_twins(s::label_style(reached), &s::label_dark_twins(reached))
}

/// Renders a [`crate::widgets::stepper::Stepper`] in the flat theme.
#[must_use]
pub fn stepper(s: crate::widgets::stepper::Stepper) -> Dom {
    s.build(stepper_skin())
}

// ==== time_picker ====
//
// Flat is the established picker: a #ced4da frame (the desktop's separator in
// the dark), grey arrows, dark readouts, the accent AM/PM pill. What it adds are
// the states its buttons never had: an arrow hovers to `LIGHT_HT` / `DARK_HT` and
// presses to `LIGHT_PT` / `DARK_PT`, and every column (the spin button, the Tab
// stop - the arrows are click targets only) and the toggle are ringed on focus
// with an inset 2px ring (`FIELD_RING` / `DARK_ACC`; white on the accent pill) -
// no border, so the arrows keep their 40x16 hit box.

/// Flat's time picker skin.
#[must_use]
pub(crate) fn time_picker_skin() -> crate::widgets::time_picker::TimePickerSkin {
    use super::style_kit as kit;
    use crate::widgets::time_picker as t;

    // The column is the spin button: its base, then its focus ring.
    let mut spinner = t::SPINNER_STYLE.to_vec();
    spinner.extend(kit::radius(3));
    spinner.extend(kit::focus_shadow_ring(FIELD_RING, DARK_ACC));

    // Every part is the widget's base, then flat's established const skin.
    let mut arrow = on_base(t::CLICKABLE_BASE, t::ARROW_STYLE).into_library_owned_vec();
    arrow.extend(kit::radius(3));
    // States last: the resting dark twin matches in every state.
    arrow.extend(kit::hover_bg(LIGHT_HT, DARK_HT));
    arrow.extend(kit::active_bg(LIGHT_PT, DARK_PT));

    let mut ampm = on_base(t::CLICKABLE_BASE, t::AMPM_STYLE).into_library_owned_vec();
    ampm.extend(kit::focus_shadow_ring(LIGHT_ON_ACC, DARK_ON_ACC));

    t::TimePickerSkin {
        theme: super::UiTheme::Flat,
        container: on_base(t::CONTAINER_BASE, t::CONTAINER_STYLE),
        spinner: CssPropertyWithConditionsVec::from_vec(spinner),
        arrow: CssPropertyWithConditionsVec::from_vec(arrow),
        display: on_base(t::READOUT_BASE, t::DISPLAY_STYLE),
        separator: on_base(t::READOUT_BASE, t::SEPARATOR_STYLE),
        ampm: CssPropertyWithConditionsVec::from_vec(ampm),
    }
}

/// Renders a [`crate::widgets::time_picker::TimePicker`] in the flat theme.
#[must_use]
pub fn time_picker(p: crate::widgets::time_picker::TimePicker) -> Dom {
    p.build(time_picker_skin())
}

// ==== toast ====
//
// Flat is the established toast: the kind's alert palette on a 6px card by day,
// its deep tint under light ink by night. What it adds is the focus ring the
// "x" never had - a 2px halo just outside the glyph, `FIELD_RING` / `DARK_ACC`,
// declared for `:focus` only so the resting button is unchanged.

/// Flat's toast skin.
#[must_use]
pub(crate) fn toast_skin() -> crate::widgets::toast::ToastSkin {
    use crate::widgets::toast as t;

    // The widget's close base, then flat's static; the card's base is laid by
    // `build_toast_style` itself.
    let mut close = on_base(t::TOAST_CLOSE_BASE, t::TOAST_CLOSE_STYLE).into_library_owned_vec();
    close.extend(super::style_kit::focus_halo(FIELD_RING, DARK_ACC));

    t::ToastSkin {
        theme: super::UiTheme::Flat,
        container: toast_container,
        message: CssPropertyWithConditionsVec::from_const_slice(t::TOAST_MESSAGE_STYLE),
        close: CssPropertyWithConditionsVec::from_vec(close),
    }
}

/// The kind's card: its light face, then its dark twins.
fn toast_container(kind: crate::widgets::toast::ToastKind) -> CssPropertyWithConditionsVec {
    use crate::widgets::toast as t;
    let mut v = t::build_toast_style(kind).into_library_owned_vec();
    v.extend(t::build_toast_dark_twins(kind));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Renders a [`crate::widgets::toast::Toast`] in the flat theme.
#[must_use]
pub fn toast(t: crate::widgets::toast::Toast) -> Dom {
    t.build(toast_skin())
}

// ==== tooltip ====
//
// Flat is the established tip: a translucent #333 chip under white text - its
// own colour, so the same chip by day and by night (a dark tip reads over
// either). Nothing in a tooltip takes focus, so it owes no ring.

/// Flat's tooltip skin.
#[must_use]
pub(crate) fn tooltip_skin() -> crate::widgets::tooltip::TooltipSkin {
    use crate::widgets::tooltip as t;
    t::TooltipSkin {
        theme: super::UiTheme::Flat,
        wrapper: CssPropertyWithConditionsVec::from_const_slice(t::TOOLTIP_WRAPPER_STYLE),
        // The widget's tip base (placement, one line, hidden), then flat's chip.
        tip: on_base(t::TIP_BASE, t::TOOLTIP_TIP_STYLE),
    }
}

/// Renders a [`crate::widgets::tooltip::Tooltip`] in the flat theme.
#[must_use]
pub fn tooltip(t: crate::widgets::tooltip::Tooltip) -> Dom {
    t.build(tooltip_skin())
}

// ==== video ====
//
// The picture is the source's own; the video widget's only chrome is the
// "no signal" poster it shows until the first frame arrives. Flat's is the
// established dark screen - #2a2a30 under a #44444c hairline - the same by day
// and by night: a screen is dark whatever the window around it is. Nothing in
// the widget takes focus (the app builds and names the controls).

/// Flat's "no signal" poster.
#[must_use]
pub(crate) fn video_poster_style() -> CssPropertyWithConditionsVec {
    use super::style_kit as kit;
    const SCREEN: ColorU = ColorU::new(42, 42, 48, 255);
    const SCREEN_EDGE: ColorU = ColorU::new(68, 68, 76, 255);

    let mut v = kit::fill().to_vec();
    v.push(CssPropertyWithConditions::simple(kit::bg(SCREEN)));
    v.extend(kit::border(kit::Edges::ALL, 1, SCREEN_EDGE, SCREEN_EDGE));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Renders a [`crate::widgets::video::VideoWidget`] in the flat theme.
#[must_use]
pub fn video(w: crate::widgets::video::VideoWidget) -> Dom {
    w.build(super::UiTheme::Flat)
}

// ==== text input kinds (type=search) ====
//
// The search field's row and its clear button. `text_input.rs` builds the
// field itself (the same `text_input()` above) and wires the button's click;
// the look is the theme's.

/// The clear button (a cross) of a `type=search` field, shown only while the field
/// holds text (`visible`). The widget flips `display` live on the
/// empty/non-empty transition; this is the state it is BUILT in. Its
/// structure is the widget's (`text_input::search_clear_base`); flat's skin
/// is a bare glyph with a little air either side.
#[must_use]
pub fn search_clear_button(visible: bool) -> Dom {
    let mut style: Vec<CssPropertyWithConditions> =
        crate::widgets::text_input::search_clear_base(visible).to_vec();
    style.extend([
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            14,
        ))),
    ]);
    // A quiet glyph that darkens under the pointer, in both modes.
    style.extend(CssPropertyWithConditions::themed(
        CssProperty::const_text_color(StyleTextColor { inner: LIGHT_ICON }),
        CssProperty::const_text_color(StyleTextColor {
            inner: system_palette::SECONDARY_TEXT,
        }),
    ));
    style.extend(CssPropertyWithConditions::themed_on_hover(
        CssProperty::const_text_color(StyleTextColor { inner: LIGHT_INK }),
        CssProperty::const_text_color(StyleTextColor {
            inner: system_palette::TEXT,
        }),
    ));

    crate::widgets::widget_p_with_text(AzString::from_const_str("\u{00D7}"))
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from_const_str(
            crate::widgets::text_input::SEARCH_CLEAR_CLASS,
        ))]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
}

/// The row of a `type=search` field: the field (which grows) and its clear
/// button after it - the widget's row (`text_input::SEARCH_FIELD_BASE`);
/// flat paints nothing on it.
#[must_use]
pub fn search_field(field: Dom, clear: Dom) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from_const_str(
            crate::widgets::text_input::SEARCH_FIELD_CLASS,
        ))]))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
            crate::widgets::text_input::SEARCH_FIELD_BASE,
        ))
        .with_children(vec![field, clear].into())
}

// ==== text input kinds (invalid look) ====

/// The border of a text field whose value the user edited into an INVALID
/// state (`type=email` / `type=url` syntax, `pattern`): the danger red the
/// flat Danger button carries. Light mode.
pub const INVALID_RING: ColorU = ColorU {
    r: 220,
    g: 53,
    b: 69,
    a: 255,
};

/// [`INVALID_RING`] in the dark theme: lifted so it still reads as red on a
/// dark field instead of sinking into it.
pub const DARK_INVALID_RING: ColorU = ColorU {
    r: 241,
    g: 112,
    b: 123,
    a: 255,
};

/// The four border colours of the invalid look, for the light (`dark ==
/// false`) or the dark theme. `text_input.rs` writes them as an OVERRIDE on
/// the field host while the value is invalid (see its `paint_invalid_ring`),
/// which is why they are plain properties and not a light/dark pair: an
/// override carries no theme condition, so the mode is chosen when it is
/// written.
#[must_use]
pub fn text_input_invalid_ring(dark: bool) -> Vec<CssProperty> {
    let inner = if dark { DARK_INVALID_RING } else { INVALID_RING };
    vec![
        CssProperty::const_border_top_color(StyleBorderTopColor { inner }),
        CssProperty::const_border_right_color(StyleBorderRightColor { inner }),
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner }),
        CssProperty::const_border_left_color(StyleBorderLeftColor { inner }),
    ]
}

// ==== datetime-local ====

/// The four border edges of a 1 px solid outline in `light`, each with its
/// dark twin in `dark` - one call so no edge ships without its twin.
fn outline_pairs(light: ColorU, dark: ColorU) -> Vec<CssPropertyWithConditions> {
    let mut v = vec![
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
            LayoutBorderRightWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
            LayoutBorderBottomWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
            LayoutBorderLeftWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
            StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_style(
            StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ];
    v.extend(CssPropertyWithConditions::themed(
        CssProperty::const_border_top_color(StyleBorderTopColor { inner: light }),
        CssProperty::const_border_top_color(StyleBorderTopColor { inner: dark }),
    ));
    v.extend(CssPropertyWithConditions::themed(
        CssProperty::const_border_right_color(StyleBorderRightColor { inner: light }),
        CssProperty::const_border_right_color(StyleBorderRightColor { inner: dark }),
    ));
    v.extend(CssPropertyWithConditions::themed(
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: light }),
        CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: dark }),
    ));
    v.extend(CssPropertyWithConditions::themed(
        CssProperty::const_border_left_color(StyleBorderLeftColor { inner: light }),
        CssProperty::const_border_left_color(StyleBorderLeftColor { inner: dark }),
    ));
    v
}

/// `<input type=datetime-local>`: the date part and the time part in one row,
/// held together by a hairline outline so the pair reads as ONE control.
#[must_use]
pub fn datetime_local(date: Dom, time: Dom) -> Dom {
    // The widget's structure (R5), then flat's skin.
    let mut style: Vec<CssPropertyWithConditions> = crate::widgets::datetime_local::base_row();
    style.extend([
        CssPropertyWithConditions::simple(CssProperty::ColumnGap(LayoutColumnGapValue::Exact(
            LayoutColumnGap {
                inner: PixelValue::const_px(8),
            },
        ))),
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
    ]);
    style.extend(outline_pairs(LIGHT_BD, system_palette::SEPARATOR));

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from_const_str(
            crate::widgets::datetime_local::DATETIME_LOCAL_CLASS,
        ))]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
        .with_children(vec![date, time].into())
}

// ==== form ====

/// `<form>`: a `NodeType::Form` node stacking its content in a column. The
/// form paints nothing of its own in either mode - it is structure, and its
/// controls carry their own light and dark faces.
#[must_use]
pub fn form(children: azul_core::dom::DomVec) -> Dom {
    Dom::create_node(NodeType::Form)
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from_const_str(
            crate::widgets::form::FORM_CLASS,
        ))]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec({
            // The widget's structure (R5), then flat's gap.
            let mut style = crate::widgets::form::base_form();
            style.push(CssPropertyWithConditions::simple(CssProperty::RowGap(
                LayoutRowGapValue::Exact(LayoutRowGap {
                    inner: PixelValue::const_px(8),
                }),
            )));
            style
        }))
        .with_children(children)
}

// ==== badge ====
//
// The flat badge is the widget's established pill: a kind-coloured fill with
// white (or, on the light Warning / Info fills, near-black) text, 2px 8px
// padding, a 10px radius. Its colour IS its meaning, so it is the same pill in
// the dark theme - the rule `button_states` applies to a coloured command - and
// every kind reads at better than 2:1 on either window
// (`widgets::theme_contrast`). A badge is not focusable, so it has no ring.

/// The flat badge: [`crate::widgets::badge::Badge::resolved_badge_style`] on a
/// `<p>` pill.
#[must_use]
pub fn badge(b: crate::widgets::badge::Badge) -> Dom {
    // Resolved before `b.string` is moved out below.
    let style = b.resolved_badge_style();
    crate::widgets::widget_p_with_text(b.string)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(
            crate::widgets::badge::BADGE_CLASS,
        ))
        .with_css_props(style)
}

// ==== divider ====
//
// The flat divider is the widget's established rule: 1px of #DDDDDD with 4px
// of breathing room, the desktop's `system:separator` as its dark twin - the
// slot every other widget draws its rules with. Not focusable: no ring.

/// The flat divider:
/// [`crate::widgets::divider::Divider::resolved_divider_style`] on a `div`.
#[must_use]
pub fn divider(d: crate::widgets::divider::Divider) -> Dom {
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(
            crate::widgets::divider::DIVIDER_CLASS,
        ))
        .with_css_props(d.resolved_divider_style())
}

// ==== spinner ====
//
// Flat's own indicator is the Windows 11 ProgressRing: a round-capped arc in
// the desktop accent. `system:accent` resolves in whichever theme paints it,
// so the arc needs no dark twin - it is the user's accent in both. Asked for
// the spokes, flat draws macOS's exactly: pure black by day, pure white at
// night; the sprite is ink and only its alpha varies. The show / hide fade is
// quick, 150 ms (KDE fades its busy indicator over 100). A spinner takes no
// focus, so there is no ring to draw.

/// The flat spinner: the Windows ring, or the macOS spokes when asked.
#[must_use]
pub fn spinner(s: crate::widgets::spinner::Spinner) -> Dom {
    use crate::widgets::spinner::{SpinnerLook, SpinnerStyle};
    crate::widgets::spinner::build(
        s,
        &SpinnerLook {
            auto: SpinnerStyle::Ring,
            spoke_ink: (
                StyleBackgroundContent::Color(ColorU::BLACK),
                Some(StyleBackgroundContent::Color(ColorU::WHITE)),
            ),
            arc_ink: (
                StyleBackgroundContent::SystemColor(SystemColorRef::Accent),
                None,
            ),
            fade_ms: 150,
            marker: None,
        },
    )
}

// ==== chip ====
//
// The flat chip is the widget's established tag: a 12px-radius pill, the
// neutral kind light grey (the desktop's quiet neutral highlight and label
// ink at night), a coloured kind its own colour in both modes. What it
// lacked was a visible focus: the remove button and a clickable label take
// the keyboard but had no border to colour, so a focused "x" looked exactly
// like an unfocused one. They get a halo in flat's focus colour (the fields'
// ring, #4286F4; flat's night accent in the dark theme) - a spread shadow,
// so the pill's geometry does not move.

/// The flat pill for a chip kind: the widget's own style, plus the neutral
/// tag's dark twins (a coloured chip is its own colour in both modes).
fn flat_chip_container(
    kind: crate::widgets::chip::ChipKind,
) -> Vec<CssPropertyWithConditions> {
    let mut style = crate::widgets::chip::build_chip_style(kind).into_library_owned_vec();
    if kind == crate::widgets::chip::ChipKind::Default {
        style.extend_from_slice(crate::widgets::chip::CHIP_DEFAULT_DARK_TWINS);
    }
    style
}

/// The flat chip: the established tag, with a focus halo on everything that
/// takes the keyboard.
#[must_use]
pub fn chip(c: crate::widgets::chip::Chip) -> Dom {
    use super::decl;
    use crate::widgets::chip::{ChipLook, CHIP_REMOVE_STYLE};

    // The skins: `chip::build` lays the label's and the "x"'s over their
    // bases; the pill starts with its own (`build_chip_style`).
    let mut label_focus = decl::radius(3).to_vec();
    label_focus.extend(decl::focus_halo(FIELD_RING, DARK_ACC));

    let mut remove = CHIP_REMOVE_STYLE.to_vec();
    remove.extend(decl::radius(4));
    remove.extend(decl::focus_halo(FIELD_RING, DARK_ACC));

    crate::widgets::chip::build(
        c,
        &ChipLook {
            container: flat_chip_container,
            label: Vec::new(),
            label_focus,
            remove,
            marker: None,
        },
    )
}

// ==== alert ====
//
// The flat alert is the widget's established Bootstrap banner - a pastel face
// in a 1px rule, a 6px radius - with Bootstrap's dark alert palette as its
// night twins. Its close button takes the keyboard but had nothing to show
// focus with; it gets flat's focus halo (#4286F4, the night accent at night).

/// The flat banner for an alert kind: the widget's own pastel face, then its
/// dark twins (Bootstrap's dark alert palette).
fn flat_alert_container(
    kind: crate::widgets::alert::AlertKind,
) -> Vec<CssPropertyWithConditions> {
    let mut style = crate::widgets::alert::build_alert_style(kind).into_library_owned_vec();
    style.extend(crate::widgets::alert::build_alert_dark_twins(kind));
    style
}

/// The flat alert: the established banner, with a focus halo on its close
/// button.
#[must_use]
pub fn alert(a: crate::widgets::alert::Alert) -> Dom {
    use super::decl;
    use crate::widgets::alert::{AlertLook, ALERT_CLOSE_STYLE};

    // The skins: `alert::build` lays the message's and the close button's
    // over their bases; the banner starts with its own
    // (`build_alert_style`).
    let mut close = ALERT_CLOSE_STYLE.to_vec();
    close.extend(decl::radius(4));
    close.extend(decl::focus_halo(FIELD_RING, DARK_ACC));

    crate::widgets::alert::build(
        a,
        &AlertLook {
            container: flat_alert_container,
            message: Vec::new(),
            close,
            marker: None,
        },
    )
}

// ==== card ====
//
// The flat card is the widget's established panel: white, a #DEE2E6 hairline,
// an 8px radius and a soft drop shadow, with the desktop's window surface and
// separator as its night twins - the application's text inside it inherits
// the themed ink. A card takes no focus, so it has no ring.

/// The flat card: the widget's own panel style.
#[must_use]
pub fn card(c: crate::widgets::card::Card) -> Dom {
    crate::widgets::card::build(
        c,
        crate::widgets::card::CARD_STYLE,
        IdOrClassVec::from_const_slice(crate::widgets::card::CARD_CLASS),
    )
}

// ==== frame ====
//
// The flat frame is the widget's established group box: #DDDDDD rules split
// around an 11px system-UI title, the desktop's separator for every rule at
// night. A frame takes no focus.

/// The flat frame's look: the widget's own part styles.
#[must_use]
pub(crate) fn frame_look() -> crate::widgets::frame::FrameLook {
    use crate::widgets::frame::{
        FrameLook, FRAME_AFTER_STYLE, FRAME_BEFORE_STYLE, FRAME_CONTENT_STYLE,
        FRAME_HEADER_STYLE, FRAME_ROOT_STYLE, FRAME_TITLE_STYLE,
    };
    FrameLook {
        root: FRAME_ROOT_STYLE.to_vec(),
        header: FRAME_HEADER_STYLE.to_vec(),
        before: FRAME_BEFORE_STYLE.to_vec(),
        title: FRAME_TITLE_STYLE.to_vec(),
        after: FRAME_AFTER_STYLE.to_vec(),
        content: FRAME_CONTENT_STYLE.to_vec(),
        marker: None,
    }
}

/// The flat frame: the widget's own part styles.
#[must_use]
pub fn frame(f: crate::widgets::frame::Frame) -> Dom {
    crate::widgets::frame::build(f, &frame_look())
}

// ==== breadcrumb ====
//
// The flat breadcrumb is the widget's established trail: Bootstrap-blue
// links (the desktop's link colour at night), a grey "/" between them, the
// current page bold. Each crumb is a keyboard stop with nothing to show
// focus, and a link that never underlined because the widget's style had to
// stay a const slice; the theme can say both, so a crumb underlines under the
// pointer and shows flat's focus halo.

/// The flat breadcrumb: the established trail, with a hover underline and a
/// focus halo on every crumb.
#[must_use]
pub fn breadcrumb(b: crate::widgets::breadcrumb::Breadcrumb) -> Dom {
    use super::decl;
    use crate::widgets::breadcrumb::{
        BreadcrumbLook, BREADCRUMB_CURRENT_STYLE, BREADCRUMB_ITEM_STYLE,
        BREADCRUMB_SEPARATOR_STYLE, SEPARATOR_GLYPH,
    };

    // The skins: `breadcrumb::build` lays each over the crumb's base.
    let mut item = BREADCRUMB_ITEM_STYLE.to_vec();
    item.extend(decl::radius(3));
    item.extend(decl::hover_underline());
    item.extend(decl::focus_halo(FIELD_RING, DARK_ACC));

    crate::widgets::breadcrumb::build(
        b,
        &BreadcrumbLook {
            item,
            current: BREADCRUMB_CURRENT_STYLE.to_vec(),
            separator: BREADCRUMB_SEPARATOR_STYLE.to_vec(),
            separator_glyph: SEPARATOR_GLYPH,
            marker: None,
        },
    )
}

// ==== accordion ====
//
// The flat accordion is the widget's established panel: a #DEE2E6 hairline,
// a 6px radius, #F8F9FA header bars; the window surface, separator and label
// ink at night. Each header is a keyboard stop and a click target that never
// showed either: it now lights up under the pointer (the neutral hover grey
// flat's buttons use, flat's night hover face at night) and rings on focus -
// drawn inside the header, because the rounded panel clips its edges.

/// The flat accordion: the established panel, with a hover face and an inset
/// focus ring on every header.
#[must_use]
pub fn accordion(a: crate::widgets::accordion::Accordion) -> Dom {
    use super::decl;
    use crate::widgets::accordion::{
        AccordionLook, ACCORDION_CONTAINER_STYLE, ACCORDION_HEADER_STYLE, ACCORDION_SECTION_STYLE,
    };

    // The skins: `accordion::build` lays each over the part's base.
    let mut header = ACCORDION_HEADER_STYLE.to_vec();
    header.extend(decl::hover_fill(ColorU::rgb(233, 236, 239), DARK_HT));
    header.extend(decl::focus_halo_inset(FIELD_RING, DARK_ACC));

    crate::widgets::accordion::build(
        a,
        &AccordionLook {
            container: ACCORDION_CONTAINER_STYLE.to_vec(),
            section: ACCORDION_SECTION_STYLE.to_vec(),
            header,
            title: Vec::new(),
            // The Windows 11 expander's chevron: down, up when open.
            chevron: crate::widgets::accordion::chevron_box(16),
            chevron_icon: "expand_more",
            chevron_turn_deg: 180,
            marker: None,
        },
    )
}

// ==== menubar ====
//
// The flat menu bar is the widget's established bar, styled with the
// desktop's own `system:` colours (window background, text, and the selection
// colours under the pointer) - which resolve in whichever theme paints them,
// so the bar needs no dark twins. Its items are pointer targets, not keyboard
// stops (the keyboard reaches a menu through the platform), so there is no
// focus ring to draw.

/// The flat menu bar.
#[must_use]
pub fn menubar(m: crate::widgets::menubar::Menubar) -> Dom {
    crate::widgets::menubar::build_flat(&m.menu)
}

// ==== color_input ====
//
// The flat colour input is the widget's established swatch and Chrome-style
// picker: a white panel in a #c8c8c8 rule, the desktop's window surface and
// separator at night. Two things were missing. The swatch and the picker's
// plane, hue and alpha bars are keyboard stops (arrows drive the bars) that
// showed no focus; each now gets flat's focus halo, which moves nothing. And
// the preview's frame and the grip's handle kept their light grey at night;
// they take the separator, as the panel's own border does (`PREVIEW_CSS`,
// `GRIP_HANDLE_CSS`).

/// The flat colour input: the established swatch and picker, with focus
/// rings on every keyboard stop.
#[must_use]
pub fn color_input(c: crate::widgets::color_input::ColorInput) -> Dom {
    use super::decl;
    use crate::widgets::color_input::{
        ColorInputLook, EYEDROPPER_CSS, GRIP_HANDLE_CSS, PANEL_CSS, PREVIEW_CSS,
    };
    let ring = decl::focus_halo(FIELD_RING, DARK_ACC).to_vec();
    crate::widgets::color_input::build(
        c,
        &ColorInputLook {
            swatch: ring.clone(),
            panel_css: PANEL_CSS,
            preview_css: PREVIEW_CSS,
            eyedropper_css: EYEDROPPER_CSS,
            grip_handle_css: GRIP_HANDLE_CSS,
            slider_focus: ring,
            marker: None,
        },
    )
}

// ==== date_picker ====
//
// The flat date picker is the widget's established field and calendar: a
// white field in a #CED4DA rule, a white calendar, the Bootstrap-blue
// picked cell; the desktop's field, separator, accent and label colours at
// night. Its keyboard stops - the field, the two header buttons (month or
// year) and the grid (one roving stop over the days, the whole-week days or
// the twelve months) - showed no focus. The field (it has a border) takes
// the fields' ring on it; the buttons and the cells, which have none, take
// flat's focus halo. The month grid's cells are the day faces, so they ring
// too.

/// The flat date picker: the established field and calendar, with a focus
/// ring on every keyboard stop in every mode.
#[must_use]
pub fn date_picker(d: crate::widgets::date_picker::DatePicker) -> Dom {
    use super::decl;
    use crate::widgets::date_picker::DatePickerLook;

    let mut look = DatePickerLook::established();
    look.field.extend(decl::focus_ring(FIELD_RING, DARK_ACC));
    look.nav.extend(decl::radius(3));
    look.nav.extend(decl::focus_halo(FIELD_RING, DARK_ACC));
    look.day_selected
        .extend(decl::focus_halo(FIELD_RING, DARK_ACC));
    look.day_other.extend(decl::focus_halo(FIELD_RING, DARK_ACC));
    crate::widgets::date_picker::build(d, &look)
}

// ==== combobox ====
//
// Flat is the established combobox, unchanged: a white field in a #acacac
// hairline with a 4px radius, the desktop's field and separator at night, the
// `FIELD_RING` / `DARK_ACC` ring on focus; a white list in the same hairline;
// option rows in the page's ink that wash on hover. What it adds is the ring
// the option rows never had - each row is a Tab stop - drawn as flat's inset
// focus ring, so the row's box does not grow.

/// Flat's combobox skin.
#[must_use]
pub(crate) fn combobox_skin() -> crate::widgets::combobox::ComboBoxSkin {
    use super::style_kit as kit;
    use crate::widgets::combobox as c;

    // The widget's structure first (R5), then flat's skin.
    let mut option = c::COMBOBOX_OPTION_BASE.to_vec();
    option.extend_from_slice(c::COMBOBOX_OPTION_STYLE);
    // States last: the hover wash (and its dark twin) is already declared.
    option.extend(kit::focus_shadow_ring(FIELD_RING, DARK_ACC));

    c::ComboBoxSkin {
        theme: super::UiTheme::Flat,
        wrapper: CssPropertyWithConditionsVec::from_const_slice(c::COMBOBOX_WRAPPER_STYLE),
        field: CssPropertyWithConditionsVec::from_vec(
            [c::COMBOBOX_FIELD_BASE, c::COMBOBOX_INPUT_STYLE].concat(),
        ),
        text: CssPropertyWithConditionsVec::from_const_slice(c::COMBOBOX_TEXT_STYLE),
        arrow: CssPropertyWithConditionsVec::from_const_slice(c::COMBOBOX_ARROW_STYLE),
        option: CssPropertyWithConditionsVec::from_vec(option),
        list: c::build_list_style(false),
    }
}

/// Renders a [`crate::widgets::combobox::ComboBox`] in the flat theme.
#[must_use]
pub fn combobox(c: crate::widgets::combobox::ComboBox) -> Dom {
    c.build(combobox_skin())
}

// ==== tree_view ====
//
// The flat tree is the widget's established look, unchanged: a near-white
// field (the list view's surface) in the system font, rows that wash on hover
// (`ROW_HOVER`, with its dark twin), the selected row in the Windows accent
// with white ink, a 16px indent per level. Its selected row's icon and label
// keep the resting styles, as they always had.

/// Flat's tree-view look: the widget's base under each part ([`on_base`]),
/// then the tree's established const styles.
#[must_use]
pub(crate) fn tree_view_look() -> crate::widgets::tree_view::TreeViewLook {
    use crate::widgets::tree_view as t;
    t::TreeViewLook {
        container: on_base(t::TREE_CONTAINER_BASE, t::TREE_CONTAINER_STYLE),
        row: on_base(t::ROW_BASE, t::ROW_STYLE),
        row_selected: on_base(t::ROW_BASE, t::ROW_SELECTED_STYLE),
        children: on_base(t::CHILDREN_BASE, t::CHILDREN_STYLE),
        icon: on_base(t::ICON_BASE, t::ICON_STYLE),
        icon_selected: on_base(t::ICON_BASE, t::ICON_STYLE),
        // No skin: the spacer's style is the same in every theme.
        leaf_spacer: CssPropertyWithConditionsVec::from_const_slice(t::LEAF_SPACER_STYLE),
        label: on_base(t::LABEL_BASE, t::LABEL_STYLE),
        label_selected: on_base(t::LABEL_BASE, t::LABEL_STYLE),
        marker: None,
    }
}

// ==== tabs ====
//
// The flat tab bar is the widget's established Windows-native look,
// unchanged: grey gradient tabs in a #acacac rule, the active tab white and
// two pixels taller, its neighbours sharing one seam with it; the hover ring
// and fill (`TAB_HOVER_*`, with their dark twins) and the desktop's surfaces
// at night. The panel is white in the same rule, open at the top, the
// desktop's window surface at night.

/// Flat's tab-bar look: the widget's base under each part ([`on_base`]),
/// then the tab bar's established const styles.
#[must_use]
pub(crate) fn tab_header_look() -> crate::widgets::tabs::TabHeaderLook {
    use crate::widgets::tabs as t;
    t::TabHeaderLook {
        header: on_base(t::HEADER_BASE, t::CSS_MATCH_9988039989460234263.as_slice()),
        // No base: flat's spacer grows (`flex-grow: 1`), flora's does not.
        before: t::CSS_MATCH_17290739305197504468,
        after: on_base(t::AFTER_BASE, t::CSS_MATCH_3088386549906605418.as_slice()),
        active: on_base(t::TAB_BASE, t::CSS_MATCH_14575853790110873394.as_slice()),
        before_active: on_base(t::TAB_BASE, t::CSS_MATCH_4415083954137121609.as_slice()),
        after_active: on_base(t::TAB_BASE, t::CSS_MATCH_13824480602841492081.as_slice()),
        inactive: on_base(t::TAB_BASE, t::CSS_MATCH_11510695043643111367.as_slice()),
        marker: None,
    }
}

/// Flat's tab-panel look: the widget's base, then the panel's established
/// const styles.
#[must_use]
pub(crate) fn tab_content_look() -> crate::widgets::tabs::TabContentLook {
    use crate::widgets::tabs as t;
    t::TabContentLook {
        padded: on_base(t::PANEL_BASE, t::CSS_MATCH_18014909903571752977.as_slice()),
        unpadded: on_base(
            t::PANEL_BASE,
            t::CSS_MATCH_18014909903571752977_NO_PADDING.as_slice(),
        ),
        marker: None,
    }
}

// ==== titlebar ====
//
// The flat titlebar is the NATIVE one, unchanged: no fill of its own (the
// window shows through, as behind a transparent native bar) unless the
// desktop stated a titlebar colour, the platform's title colour (with a dark
// twin for the light default), the platform's line (macOS: one device pixel
// of #D0D0D0, #000000 at night), `:backdrop` dimming where the desktop gives
// it, and the desktop's hover colours on the window controls.

/// Flat's titlebar look: the bar's native paint, from its colour fields.
#[must_use]
pub(crate) fn titlebar_look(
    bar: &crate::widgets::titlebar::Titlebar,
    show_buttons: bool,
) -> crate::widgets::titlebar::TitlebarLook {
    use crate::widgets::titlebar::{flat_control_hover, TitlebarLook};
    TitlebarLook {
        container: bar.build_container_style(show_buttons),
        title: bar.build_title_style(show_buttons),
        button: flat_control_hover(bar.button_hover_color),
        close: flat_control_hover(bar.close_hover_color),
        marker: None,
    }
}

// ==== combobox (active option) ====
//
// The option the arrow keys made ACTIVE (the field keeps focus; WAI-ARIA
// combobox) wears the row-hover wash its options take under the pointer, so
// the keyboard's "you are here" reads like the mouse's. Light, then dark.

/// The fill of a flat combobox's active option, `[light, dark]`.
pub(crate) const COMBOBOX_ACTIVE_OPTION: [ColorU; 2] = [LIGHT_OPTION_HOVER, DARK_ROW_HOVER];

// ==== R5-D: a flat part is the widget's base, then flat's skin ====

/// A part as flat builds it from a widget's const styles: the widget's
/// `base` (its structure, the same in every theme - R5), then flat's `skin`
/// (paint and metrics). Both are plain declarations: the `@theme` blocks of
/// an unpinned widget are made from the whole part afterwards
/// (`theme_blocks::follow_props`), so the part is simply the two lists one
/// after the other - no `@theme` rank to keep (`theme_blocks::stack_parts`
/// is for stacking parts that already carry theme blocks).
/// (`decl::on_base` as the vector type flat's builders answer.)
fn on_base(
    base: &[CssPropertyWithConditions],
    skin: &[CssPropertyWithConditions],
) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(super::decl::on_base(base, skin))
}

// ==== accordion (groups) ====
//
// The flat GROUPS accordion is Explorer's group header (Windows 7's
// "Hard Disk Drives (2)"): the title and its count in the group-header blue,
// a #E2E2E2 hairline to the end of the row, and the indicator - a chevron
// that points right while the group is closed and turns down when it opens.
// No panel: the groups sit on the page. A header washes to the row-hover
// blue under the pointer and rings inside on focus. At night the title takes
// the desktop's link ink, the rule the separator, the wash flat's dark row
// hover.

/// The group-header blue of Explorer's groups (#1E3287).
const GROUP_HEADER_INK: ColorU = ColorU {
    r: 30,
    g: 50,
    b: 135,
    a: 255,
};

/// The hairline after a group's title (#E2E2E2).
const GROUP_RULE: ColorU = ColorU {
    r: 226,
    g: 226,
    b: 226,
    a: 255,
};

/// The flat groups accordion: Explorer's group headers.
#[must_use]
pub fn accordion_groups(a: crate::widgets::accordion::Accordion) -> Dom {
    use super::decl;
    use crate::widgets::accordion::AccordionLook;

    let container = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
        CssPropertyWithConditions::simple(decl::ink(LIGHT_INK)),
        system_palette::DARK_TEXT,
    ];

    let section = decl::margin(0, 0, 6, 0).to_vec();

    let mut header = decl::padding(3, 4, 3, 4).to_vec();
    header.extend(decl::radius(2));
    header.extend(decl::hover_fill(LIGHT_ROW_HOVER, DARK_ROW_HOVER));
    header.extend(decl::focus_halo_inset(FIELD_RING, DARK_ACC));

    // The title hugs its text; the rule takes the rest of the row.
    let title = vec![
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(decl::ink(GROUP_HEADER_INK)),
        system_palette::DARK_LINK,
    ];

    let mut rule = decl::margin(0, 0, 0, 8).to_vec();
    rule.extend(decl::border_bottom(1));
    rule.extend(decl::themed_border_bottom_color(GROUP_RULE, DARK_SEP));

    crate::widgets::accordion::build_groups(
        a,
        &AccordionLook {
            container,
            section,
            header,
            title,
            chevron: crate::widgets::accordion::chevron_box(16),
            chevron_icon: "chevron_right",
            chevron_turn_deg: 90,
            marker: None,
        },
        &rule,
    )
}

// ==== tile ====
//
// The flat tile is Windows 7 Explorer's drive tile: the icon in a steel
// blue beside the title, the capacity bar in Explorer's #26A0DA (the alarm
// red #DA2626 past a tenth free) on a #E6E6E6 track, the "x free of y" line
// in the secondary ink. A tile washes to the row-hover blue under the
// pointer, the selected one takes the selection blue #CCE8FF; focus is an
// inset ring. At night: the desktop's secondary ink, the tree's dark
// selection, flat's dark row hover.

/// Explorer's capacity blue.
const TILE_BAR_FILL: ColorU = ColorU {
    r: 38,
    g: 160,
    b: 218,
    a: 255,
};
/// Explorer's capacity red: the volume is nearly full.
const TILE_BAR_ALARM: ColorU = ColorU {
    r: 218,
    g: 38,
    b: 38,
    a: 255,
};
/// The bar's track.
const TILE_BAR_TRACK: ColorU = ColorU {
    r: 230,
    g: 230,
    b: 230,
    a: 255,
};
/// The tile icon's steel blue, by day and by night.
const TILE_ICON_LIGHT: ColorU = ColorU {
    r: 74,
    g: 122,
    b: 181,
    a: 255,
};
const TILE_ICON_DARK: ColorU = ColorU {
    r: 122,
    g: 167,
    b: 224,
    a: 255,
};
/// Explorer's selection blue (#CCE8FF); the tree's dark selection at night.
const TILE_SELECTED_LIGHT: ColorU = ColorU {
    r: 204,
    g: 232,
    b: 255,
    a: 255,
};
const TILE_SELECTED_DARK: ColorU = ColorU {
    r: 9,
    g: 71,
    b: 113,
    a: 255,
};

/// One solid layer, as a bar's fill or track.
fn solid(color: ColorU) -> StyleBackgroundContentVec {
    StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(color)])
}

/// Flat's tile look.
#[must_use]
pub(crate) fn tile_look() -> crate::widgets::tile::TileLook {
    use super::decl;
    let mut tile = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    tile.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    tile.extend(decl::padding(6, 6, 6, 6));
    tile.extend(decl::radius(3));
    tile.extend(decl::hover_fill(LIGHT_ROW_HOVER, DARK_ROW_HOVER));
    tile.extend(decl::focus_halo_inset(FIELD_RING, DARK_ACC));

    let tile_selected = decl::themed_fill(TILE_SELECTED_LIGHT, TILE_SELECTED_DARK).to_vec();

    let mut icon = vec![CssPropertyWithConditions::simple(CssProperty::const_font_size(
        StyleFontSize::const_px(44),
    ))];
    icon.extend(decl::margin(0, 8, 0, 0));
    icon.extend(decl::themed_ink(TILE_ICON_LIGHT, TILE_ICON_DARK));

    let mut detail = vec![CssPropertyWithConditions::simple(CssProperty::const_font_size(
        StyleFontSize::const_px(12),
    ))];
    detail.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    crate::widgets::tile::TileLook {
        tile,
        tile_selected,
        icon,
        title: Vec::new(),
        detail,
        bar: decl::margin(3, 0, 3, 0).to_vec(),
        bar_height: 12,
        bar_track: solid(TILE_BAR_TRACK),
        bar_fill: solid(TILE_BAR_FILL),
        bar_fill_alarm: solid(TILE_BAR_ALARM),
        marker: None,
    }
}

/// The flat tile: Explorer's drive tile.
#[must_use]
pub fn tile(t: crate::widgets::tile::Tile) -> Dom {
    crate::widgets::tile::build(t, &tile_look())
}

// ==== details_pane ====
//
// The flat details pane is Explorer's: a strip on the window surface under
// a hairline, the big icon in the tile's steel blue, the item's name a shade
// heavier than its kind, the keys in the secondary ink set right so the
// values line up. At night the desktop's surfaces and inks.

/// Flat's details-pane look.
#[must_use]
pub(crate) fn details_pane_look() -> crate::widgets::details_pane::DetailsPaneLook {
    use super::decl;
    let mut pane = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    pane.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    pane.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    pane.extend(decl::padding(8, 16, 8, 16));
    pane.extend([
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(
            StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            },
        )),
    ]);
    pane.extend(decl::themed_border_top_color(LIGHT_BD, DARK_BD));

    let mut icon = vec![CssPropertyWithConditions::simple(CssProperty::const_font_size(
        StyleFontSize::const_px(56),
    ))];
    icon.extend(decl::margin(0, 12, 0, 0));
    icon.extend(decl::themed_ink(TILE_ICON_LIGHT, TILE_ICON_DARK));

    let heading = decl::margin(0, 24, 0, 0).to_vec();
    let title = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            14,
        ))),
        decl::semibold(),
    ];
    let mut subtitle = vec![CssPropertyWithConditions::simple(CssProperty::const_font_size(
        StyleFontSize::const_px(12),
    ))];
    subtitle.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    let mut key = vec![CssPropertyWithConditions::simple(CssProperty::const_width(
        LayoutWidth::const_px(110),
    ))];
    key.extend(decl::margin(0, 6, 0, 0));
    key.extend(decl::themed_ink(LIGHT_SOFT1, DARK_SOFT1));

    crate::widgets::details_pane::DetailsPaneLook {
        pane,
        icon,
        heading,
        title,
        subtitle,
        properties: Vec::new(),
        row: decl::margin(1, 0, 1, 0).to_vec(),
        key,
        value: Vec::new(),
        marker: None,
    }
}

/// The flat details pane.
#[must_use]
pub fn details_pane(p: crate::widgets::details_pane::DetailsPane) -> Dom {
    crate::widgets::details_pane::build(p, &details_pane_look())
}

// ==== address_bar ====
//
// The flat address bar is Explorer's: a strip on the window surface over a
// hairline, the arrows and Refresh as flat icon buttons, the path in a
// field - a white box in the field rule, ringed by the field ring under the
// pointer - and the search box at a fixed width. At night the desktop's
// surfaces, the dark field and the accent ring.

/// Flat's address-bar look.
#[must_use]
pub(crate) fn address_bar_look() -> crate::widgets::address_bar::AddressBarLook {
    use super::decl;
    let mut bar = vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    ];
    bar.extend(decl::themed_ink(LIGHT_INK, DARK_INK));
    bar.extend(decl::themed_fill(LIGHT_SUR, DARK_SUR));
    bar.extend(decl::padding(4, 8, 4, 8));
    bar.extend(decl::border_bottom(1));
    bar.extend(decl::themed_border_bottom_color(LIGHT_BD, DARK_BD));

    let mut field = vec![CssPropertyWithConditions::simple(CssProperty::const_height(
        LayoutHeight::const_px(26),
    ))];
    field.extend(decl::padding(0, 6, 0, 6));
    field.extend(decl::margin(0, 8, 0, 4));
    field.extend(decl::border(1));
    field.extend(decl::themed_border_color(LIGHT_BD3, DARK_BD3));
    field.extend(decl::radius(2));
    field.extend(decl::themed_fill(LIGHT_FLD, DARK_FLD));
    field.extend(decl::hover_border_color(FIELD_RING, DARK_ACC));

    crate::widgets::address_bar::AddressBarLook {
        bar,
        nav: decl::margin(0, 2, 0, 0).to_vec(),
        field,
        field_editing: decl::margin(0, 8, 0, 4).to_vec(),
        search: vec![CssPropertyWithConditions::simple(CssProperty::const_width(
            LayoutWidth::const_px(220),
        ))],
        marker: None,
    }
}

/// The flat address bar.
#[must_use]
pub fn address_bar(b: crate::widgets::address_bar::AddressBar) -> Dom {
    crate::widgets::address_bar::build(b, &address_bar_look())
}
