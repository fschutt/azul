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

// Flat theme colors
pub const LIGHT_BG: ColorU = ColorU {
    r: 248,
    g: 249,
    b: 250,
    a: 255,
};
pub const LIGHT_FG: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
};
pub const DARK_BG: ColorU = ColorU {
    r: 52,
    g: 58,
    b: 64,
    a: 255,
};
pub const DARK_FG: ColorU = ColorU {
    r: 248,
    g: 249,
    b: 250,
    a: 255,
};

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

    let type_class = btn.button_type.class_name();
    let classes: Vec<IdOrClass> = vec![
        IdOrClass::Class(AzString::from("__azul-native-button")),
        IdOrClass::Class(AzString::from(type_class)),
        IdOrClass::Class(AzString::from("__azul-theme-flat")),
    ];

    let mut button = Dom::create_node(NodeType::Button);

    let has_icon = !btn.icon.as_str().is_empty() || btn.icon_dom.is_some();
    let has_image = btn.image.is_some();
    let has_trailing_icon = !btn.trailing_icon.as_str().is_empty();

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
            None => Dom::create_icon(btn.icon).with_css_props(btn.icon_style),
        });
    }

    if let Some(image) = btn.image.into_option() {
        button = button.with_child(Dom::create_image(image).with_css_props(btn.image_style));
    }

    let skip_label = btn.label.as_str().is_empty() && (has_icon || has_image || has_trailing_icon);
    if !skip_label {
        button = button.with_child(
            crate::widgets::widget_p()
                .with_css_props(btn.label_style)
                .with_children(azul_core::dom::DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(btn.label),
                ])),
        );
    }

    if has_trailing_icon {
        button = button.with_child(
            Dom::create_icon(btn.trailing_icon).with_css_props(btn.trailing_icon_style),
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
        btn.container_style.as_slice().to_vec();

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

    button
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_ids_and_classes(IdOrClassVec::from_vec(classes))
        .with_callbacks(callbacks.into())
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(a11y)
}

use crate::widgets::check_box::CheckBox;

pub fn check_box(cb: CheckBox) -> Dom {
    let cb_name = cb.accessibility_name.clone();
    crate::widgets::warn_widget_needs_a_name("check_box", cb_name.is_some());

    let checked_now = cb.check_box_state.inner.checked;

    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, HoverEventFilter},
    };

    let mut container_style: Vec<CssPropertyWithConditions> = match cb.container_style {
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(style) => {
            style.as_slice().to_vec()
        }
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
            crate::widgets::check_box::DEFAULT_CHECKBOX_CONTAINER_STYLE.to_vec()
        }
    };
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_BG)])
                .into(),
        ),
    ));
    // Flat checkmark background in dark mode
    let is_checked = cb.check_box_state.inner.checked;
    let mut content_style: Vec<CssPropertyWithConditions> = match cb.content_style {
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(style) => {
            style.as_slice().to_vec()
        }
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
            if is_checked {
                crate::widgets::check_box::DEFAULT_CHECKBOX_CONTENT_STYLE_CHECKED.to_vec()
            } else {
                crate::widgets::check_box::DEFAULT_CHECKBOX_CONTENT_STYLE_UNCHECKED.to_vec()
            }
        }
    };
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
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::CheckButton,
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

    let state_ref = RefAny::new(ti.text_input_state);

    let mut container_style: Vec<CssPropertyWithConditions> =
        ti.container_style.as_slice().to_vec();
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::BackgroundContent(
            StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(DARK_BG)])
                .into(),
        ),
    ));
    container_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_FG }.into()),
    ));

    let mut label_style: Vec<CssPropertyWithConditions> = ti.label_style.as_slice().to_vec();
    label_style.push(CssPropertyWithConditions::dark_theme(
        CssProperty::TextColor(StyleTextColor { inner: DARK_FG }.into()),
    ));

    Dom::create_div()
        .with_ids_and_classes(vec![Class(TEXT_INPUT_CONTAINER_CLASS.into())].into())
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Text,
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

pub fn label(l: crate::widgets::label::Label) -> Dom {
    use azul_core::dom::{IdOrClass::Class, IdOrClassVec};
    use AzString;

    static LABEL_CLASS: &[azul_core::dom::IdOrClass] =
        &[Class(AzString::from_const_str("__azul-native-label"))];

    crate::widgets::widget_p_with_text(l.string)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
        .with_css_props(l.label_style)
}

pub fn switch(s: crate::widgets::switch::Switch) -> Dom {
    let is_checked = s.switch_state.inner.checked;
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
        .with_css_props(
            match s.track_style {
                azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(style) => {
                    style.as_slice().to_vec()
                }
                azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
                    crate::widgets::switch::build_track_style(is_checked)
                        .as_slice()
                        .to_vec()
                }
            }
            .into(),
        )
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
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::CheckButton,
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
                .with_css_props(
                    match s.knob_style {
                        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(
                            style,
                        ) => style.as_slice().to_vec(),
                        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None =>
                            crate::widgets::switch::build_knob_style(is_checked)
                                .as_slice()
                                .to_vec(),
                    }
                    .into()
                )]
            .into(),
        )
}

// -----------------------------------------------------------------------------
// PROGRESSBAR
// -----------------------------------------------------------------------------

#[must_use]
pub fn progressbar(bar: crate::widgets::progressbar::ProgressBar) -> Dom {
    let height = bar.height;
    let dataset =
        azul_core::refany::RefAny::new(crate::widgets::progressbar::ProgressBarLocalDataset {
            bar,
        });
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
                StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
                    inner: ColorU {
                        r: 178,
                        g: 178,
                        b: 178,
                        a: 255,
                    },
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
                StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                    inner: ColorU {
                        r: 178,
                        g: 178,
                        b: 178,
                        a: 255,
                    },
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
                StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                    inner: ColorU {
                        r: 178,
                        g: 178,
                        b: 178,
                        a: 255,
                    },
                }),
            )),
            CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
                StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                    inner: ColorU {
                        r: 178,
                        g: 178,
                        b: 178,
                        a: 255,
                    },
                }),
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
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::ProgressBar,
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

    let mut track_style = slider.track_style.as_slice().to_vec();
    let mut thumb_style = slider.thumb_style.as_slice().to_vec();

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
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
            AzString::from_const_str("__azul-native-slider"),
        )]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(track_style))
        .with_callbacks(callbacks.into())
        .with_dataset(OptionRefAny::Some(state))
        .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(
            crate::widgets::slider::merge_slider_state,
        ))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Slider,
            accessibility_name: a11y_name,
            accessibility_value: Some(AzString::from(alloc::format!("{value_now}"))).into(),
            ..Default::default()
        })
        .with_children(
            vec![Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_vec(vec![IdOrClass::Class(
                    AzString::from_const_str("__azul-native-slider-thumb"),
                )]))
                .with_css_props(CssPropertyWithConditionsVec::from_vec(thumb_style))]
            .into(),
        )
}

pub fn text_area(mut ta: crate::widgets::text_area::TextArea) -> Dom {
    let ta_name: Option<AzString> = ta.text_area_state.inner.placeholder.as_ref().cloned();

    use azul_core::dom::{
        AttributeType, DomVec, EventFilter, FocusEventFilter, IdOrClass::Class, TabIndex,
    };

    ta.text_area_state.inner.cursor_pos = ta.text_area_state.inner.text.len();

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

    let mut container_style: Vec<CssPropertyWithConditions> = match &ta.container_style {
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(s) => {
            s.as_slice().to_vec()
        }
        azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::None => {
            crate::widgets::text_area::TEXT_AREA_CONTAINER_PROPS.to_vec()
        }
    };
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

    Dom::create_div()
        .with_ids_and_classes(vec![Class("__azul-native-text-area-container".into())].into())
        .with_css_props(CssPropertyWithConditionsVec::from_vec(container_style))
        .with_tab_index(TabIndex::Auto)
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Text,
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

const FLAT_BORDER_NORMAL: ColorU = ColorU {
    r: 172,
    g: 172,
    b: 172,
    a: 255,
};

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

    const DROPDOWN_CLASS: &[azul_core::dom::IdOrClass] =
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
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::ComboBox,
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

pub fn avatar(a: crate::widgets::avatar::Avatar) -> Dom {
    use azul_core::dom::{Dom, IdOrClassVec};
    let size = a.size;
    let child = match a.image.into_option() {
        Some(image) => Dom::create_image(image)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(
                crate::widgets::avatar::AVATAR_IMAGE_CLASS,
            ))
            .with_css_props(crate::widgets::avatar::build_image_style(size).into()),
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
