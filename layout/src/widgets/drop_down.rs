//! Native drop-down / select widget.
//!
//! Renders a clickable trigger (label + arrow icon) that opens a native
//! menu popup for item selection.  Depends on [`azul_core::menu`] for
//! popup rendering.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, FocusEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        TabIndex,
    },
    menu::{Menu, MenuItem, MenuPopupPosition, StringMenuItem},
    refany::RefAny,
    window::ContextMenuMouseButton,
};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{
            color::{ColorOrSystem, ColorU},
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::CssProperty,
        style::*,
    },
    OptionString, *,
};

use crate::callbacks::{Callback, CallbackInfo};

// -- Callback type via macro --

/// Callback signature invoked when the user selects a new choice.
///
/// The `usize` argument is the zero-based index of the chosen item.
pub type DropDownOnChoiceChangeCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    DropDownOnChoiceChange,
    OptionDropDownOnChoiceChange,
    DropDownOnChoiceChangeCallback,
    DropDownOnChoiceChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DropDownOnChoiceChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: DROP_DOWN_ON_CHOICE_CHANGE_INVOKER,
    invoker_ty:     AzDropDownOnChoiceChangeCallbackInvoker,
    thunk_fn:       az_drop_down_on_choice_change_callback_thunk,
    setter_fn:      AzApp_setDropDownOnChoiceChangeCallbackInvoker,
    from_handle_fn: AzDropDownOnChoiceChangeCallback_createFromHostHandle,
    extra_args:     [ choice_index: usize ],
}

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- Layout constants --

const FONT_SIZE_PX: isize = 13;
const ARROW_FONT_SIZE_PX: isize = 18;
const PADDING_HORIZONTAL_PX: isize = 4;
const PADDING_VERTICAL_PX: isize = 2;
const LABEL_PADDING_RIGHT_PX: isize = 8;
const BORDER_WIDTH_PX: isize = 1;

// -- Colors --

const BORDER_NORMAL: ColorU = ColorU {
    r: 172,
    g: 172,
    b: 172,
    a: 255,
};
const BORDER_HOVER: ColorU = ColorU {
    r: 126,
    g: 180,
    b: 234,
    a: 255,
};
const BORDER_FOCUS: ColorU = ColorU {
    r: 86,
    g: 157,
    b: 229,
    a: 255,
};

const BG_GRADIENT_TOP: ColorU = ColorU {
    r: 245,
    g: 245,
    b: 245,
    a: 255,
};
const BG_GRADIENT_BOTTOM: ColorU = ColorU {
    r: 235,
    g: 235,
    b: 235,
    a: 255,
};
const BG_HOVER_TOP: ColorU = ColorU {
    r: 234,
    g: 244,
    b: 252,
    a: 255,
};
const BG_HOVER_BOTTOM: ColorU = ColorU {
    r: 218,
    g: 236,
    b: 252,
    a: 255,
};
const BG_ACTIVE_TOP: ColorU = ColorU {
    r: 218,
    g: 236,
    b: 252,
    a: 255,
};
const BG_ACTIVE_BOTTOM: ColorU = ColorU {
    r: 202,
    g: 226,
    b: 248,
    a: 255,
};

const NORMAL_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(&[
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(0),
                color: ColorOrSystem::color(BG_GRADIENT_TOP),
            },
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(100),
                color: ColorOrSystem::color(BG_GRADIENT_BOTTOM),
            },
        ]),
    })];

const HOVER_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(&[
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(0),
                color: ColorOrSystem::color(BG_HOVER_TOP),
            },
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(100),
                color: ColorOrSystem::color(BG_HOVER_BOTTOM),
            },
        ]),
    })];

const ACTIVE_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(&[
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(0),
                color: ColorOrSystem::color(BG_ACTIVE_TOP),
            },
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(100),
                color: ColorOrSystem::color(BG_ACTIVE_BOTTOM),
            },
        ]),
    })];

// -- Dropdown wrapper styles (the clickable trigger) --

static DROPDOWN_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    // Layout
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineFlex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    // Font
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
        FONT_SIZE_PX,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    // Padding
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(PADDING_HORIZONTAL_PX),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(PADDING_HORIZONTAL_PX),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        PADDING_VERTICAL_PX,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(PADDING_VERTICAL_PX),
    )),
    // Border
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(BORDER_WIDTH_PX),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(BORDER_WIDTH_PX),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(BORDER_WIDTH_PX),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(BORDER_WIDTH_PX),
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
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: BORDER_NORMAL,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: BORDER_NORMAL,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: BORDER_NORMAL,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: BORDER_NORMAL,
        },
    )),
    // Background
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(NORMAL_BG_ITEMS),
    )),
    // Hover
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: BORDER_HOVER,
    })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: BORDER_HOVER,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: BORDER_HOVER,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: BORDER_HOVER,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(HOVER_BG_ITEMS),
    )),
    // Active
    CssPropertyWithConditions::on_active(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(ACTIVE_BG_ITEMS),
    )),
    // Focus
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: BORDER_FOCUS,
    })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: BORDER_FOCUS,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: BORDER_FOCUS,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: BORDER_FOCUS,
        },
    )),
];

// -- Label text style --

static DROPDOWN_LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(LABEL_PADDING_RIGHT_PX),
    )),
];

// -- Arrow icon style --

static DROPDOWN_ARROW_ICON_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
        ARROW_FONT_SIZE_PX,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

// ============================================================================
// Widget struct and API
// ============================================================================

/// A drop-down / select widget that displays the currently selected item
/// and opens a native menu popup when focused.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DropDown {
    /// The list of choices presented in the popup menu.
    pub choices: StringVec,
    /// Zero-based index of the currently selected choice.
    pub selected: usize,
    /// Optional callback invoked when the user picks a different choice.
    pub on_choice_change: OptionDropDownOnChoiceChange,
    /// Style of the clickable trigger wrapper.
    pub wrapper_style: CssPropertyWithConditionsVec,
    /// Style of the selected-choice label.
    pub label_style: CssPropertyWithConditionsVec,
    /// Style of the drop-down arrow icon.
    pub arrow_style: CssPropertyWithConditionsVec,
    /// What this control is CALLED, for assistive technology.
    ///
    /// Carried by the WIDGET so it knows at build time whether it was named;
    /// forwarded into the accessibility declaration it already builds.
    pub accessibility_name: OptionString,
}

impl Default for DropDown {
    fn default() -> Self {
        Self {
            choices: StringVec::from_const_slice(&[]),
            selected: 0,
            on_choice_change: None.into(),
            wrapper_style: CssPropertyWithConditionsVec::from_const_slice(DROPDOWN_WRAPPER_STYLE),
            label_style: CssPropertyWithConditionsVec::from_const_slice(DROPDOWN_LABEL_STYLE),
            arrow_style: CssPropertyWithConditionsVec::from_const_slice(DROPDOWN_ARROW_ICON_STYLE),
            accessibility_name: OptionString::None,
        }
    }
}

impl DropDown {
    /// Creates a new `DropDown` with the given choices and no callback.
    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    #[must_use]
    pub fn new(choices: StringVec) -> Self {
        Self {
            choices,
            ..Self::default()
        }
    }

    /// Selects the choice at `index` - what the trigger DISPLAYS.
    ///
    /// A drop-down is rebuilt from the host's state on every layout, so the
    /// host must write the index its `on_choice_change` callback was handed
    /// back here; otherwise the trigger keeps showing choice 0 no matter what
    /// the user picks (there was no setter at all, so this was the only
    /// possible outcome). An out-of-range index displays the empty string
    /// rather than panicking, the same as an empty choice list.
    pub const fn set_selected(&mut self, index: usize) {
        self.selected = index;
    }

    /// Builder variant of [`Self::set_selected`].
    #[must_use]
    pub const fn with_selected(mut self, index: usize) -> Self {
        self.set_selected(index);
        self
    }

    /// Sets the callback invoked when the user selects a different choice.
    pub fn set_on_choice_change<C: Into<DropDownOnChoiceChangeCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.on_choice_change = Some(DropDownOnChoiceChange {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder variant of [`Self::set_on_choice_change`].
    #[must_use]
    pub fn with_on_choice_change<C: Into<DropDownOnChoiceChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_choice_change(data, callback);
        self
    }

    /// Replaces `self` with the default value and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::default();
        core::mem::swap(&mut m, self);
        m
    }

    /// Builds the DOM tree for this drop-down widget.
    #[must_use]
    pub fn dom(self) -> Dom {
        // Read the selected label before the options are moved into the DOM.
        let selected_label: Option<AzString> = self
            .choices
            .as_ref()
            .get(self.selected)
            .map(|o| AzString::from(o.as_str().to_string()));

        const DROPDOWN_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-dropdown"))];

        let selected_text = self
            .choices
            .as_slice()
            .get(self.selected)
            .cloned()
            .unwrap_or_else(|| AzString::from_const_str(""));

        // The full widget state travels into the focus callback; the style
        // vecs are pulled out first so the rendered nodes use them directly.
        let wrapper_style = self.wrapper_style.clone();
        let label_style = self.label_style.clone();
        let arrow_style = self.arrow_style.clone();
        let refany = RefAny::new(self);

        // Wrapper: focusable trigger that opens popup on focus

        Dom::create_div()
            .with_css_props(wrapper_style)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(DROPDOWN_CLASS))
            .with_tab_index(TabIndex::Auto)
            // A drop-down announces which option is current; without a value a
            // reader says "combo box" and never what is selected.
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
                        cb: on_dropdown_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                }]
                .into(),
            )
            .with_children(DomVec::from_vec(vec![
                // Selected text label wrapped in <p> for proper block formatting
                crate::widgets::widget_p()
                    .with_css_props(label_style)
                    .with_children(DomVec::from_vec(vec![
                        Dom::create_text_do_not_use_without_block_level_wrapper(selected_text),
                    ])),
                // Arrow icon (resolved via Material Icons)
                Dom::create_icon(AzString::from_const_str("arrow_drop_down"))
                    .with_css_props(arrow_style),
            ]))
    }
}

// ============================================================================
// Internal callback data types
// ============================================================================

struct ChoiceCallbackData {
    choice_id: usize,
    on_choice_change: OptionDropDownOnChoiceChange,
}

// ============================================================================
// Callbacks
// ============================================================================

extern "C" fn on_dropdown_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let Some(refany) = refany.downcast_ref::<DropDown>() else {
        return Update::DoNothing;
    };

    let menu_items: Vec<MenuItem> = refany
        .choices
        .iter()
        .enumerate()
        .map(|(idx, choice)| {
            MenuItem::String(StringMenuItem::create(choice.clone()).with_callback(
                RefAny::new(ChoiceCallbackData {
                    choice_id: idx,
                    on_choice_change: refany.on_choice_change.clone(),
                }),
                on_choice_selected as usize,
            ))
        })
        .collect();

    let menu = Menu {
        items: menu_items.into(),
        position: MenuPopupPosition::BottomOfHitRect,
        context_mouse_btn: ContextMenuMouseButton::Right,
    };

    info.open_menu_for_hit_node(menu);
    Update::DoNothing
}

extern "C" fn on_choice_selected(mut refany: RefAny, info: CallbackInfo) -> Update {
    let Some(mut refany) = refany.downcast_mut::<ChoiceCallbackData>() else {
        return Update::DoNothing;
    };

    let choice_id = refany.choice_id;

    match refany.on_choice_change.as_mut() {
        Some(DropDownOnChoiceChange { refany, callback }) => {
            (callback.cb)(refany.clone(), info, choice_id)
        }
        None => Update::DoNothing,
    }
}

impl From<DropDown> for Dom {
    fn from(b: DropDown) -> Self {
        b.dom()
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{
            display_list::{DisplayList, DisplayListItem, WindowLogicalRect},
            layout_tree::LayoutTree,
        },
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// Where every user callback records the index it was handed. Passed through
    /// the widget as a `RefAny`, so the assertions exercise the real data plumbing
    /// (`RefAny::new` -> clone -> `downcast_ref`) rather than a side channel.
    type ChoiceLog = Arc<Mutex<Vec<usize>>>;

    /// Offset added by `reject_choice` so the two recorders below stay
    /// distinguishable in the log.
    const SENTINEL: usize = 1_000_000;

    extern "C" fn record_choice(
        mut data: RefAny,
        _info: CallbackInfo,
        choice_index: usize,
    ) -> Update {
        if let Some(log) = data.downcast_ref::<ChoiceLog>() {
            log.lock().expect("choice log poisoned").push(choice_index);
        }
        Update::RefreshDom
    }

    /// A second callback with a *deliberately different body*: two identical
    /// `extern "C"` bodies are legal prey for identical-code folding, which would
    /// merge their addresses and make the "last write wins" assertion vacuous.
    extern "C" fn reject_choice(
        mut data: RefAny,
        _info: CallbackInfo,
        choice_index: usize,
    ) -> Update {
        if let Some(log) = data.downcast_ref::<ChoiceLog>() {
            log.lock()
                .expect("choice log poisoned")
                .push(choice_index.wrapping_add(SENTINEL));
        }
        Update::RefreshDomAllWindows
    }

    fn log() -> ChoiceLog {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn entries(log: &ChoiceLog) -> Vec<usize> {
        log.lock().expect("choice log poisoned").clone()
    }

    fn cb(f: DropDownOnChoiceChangeCallbackType) -> DropDownOnChoiceChangeCallback {
        DropDownOnChoiceChangeCallback::from(f)
    }

    fn choices(items: &[&str]) -> StringVec {
        StringVec::from_vec(
            items
                .iter()
                .map(|s| AzString::from_string((*s).to_string()))
                .collect(),
        )
    }

    /// Adversarial choice labels: empty, whitespace, combining marks, ZWJ emoji,
    /// RTL, embedded NULs (`AzString` is length-based, so a NUL must not
    /// truncate), bidi overrides, control characters, and strings that collide
    /// with the widget's own class / icon names.
    fn adversarial_choices() -> Vec<String> {
        let mut v: Vec<String> = [
            "",
            " ",
            "OK",
            "e\u{0301}",                                   // e + combining acute
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}", // ZWJ family emoji
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD}",                // RTL Hebrew
            "\0",                                          // a lone NUL
            "a\0b",                                        // embedded NUL
            "\u{FFFD}\u{202E}\u{200B}",                    // replacement, RTL override, ZWSP
            "…\t\r\n",                                     // control chars in a label
            "__azul-native-dropdown",                      // looks like the widget's own class
            "arrow_drop_down",                             // looks like the widget's own icon
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        v.push("x".repeat(100_000));
        v
    }

    fn adversarial_dropdown() -> DropDown {
        DropDown::new(StringVec::from_vec(
            adversarial_choices()
                .into_iter()
                .map(AzString::from_string)
                .collect(),
        ))
    }

    // ------------------------------------------------------------------
    // DOM probes
    // ------------------------------------------------------------------

    /// The text the trigger displays: `root > p > text`. Panics loudly (rather
    /// than returning `None`) if the shape ever changes, because every label
    /// assertion below silently depends on that shape.
    fn label_of(dom: &Dom) -> &str {
        let p = dom
            .children
            .as_ref()
            .first()
            .expect("the trigger must have a label child");
        let text = p
            .children
            .as_ref()
            .first()
            .expect("the label must wrap a text node");
        match text.root.get_node_type() {
            NodeType::Text(s) => s.as_ref().as_str(),
            other => panic!("expected a text node, got {other:?}"),
        }
    }

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The recursive descendant count. `Dom::estimated_total_children` is a
    /// *cached* value that, if too small, makes `convert_dom_into_compact_dom`
    /// under-allocate its arenas and panic on out-of-bounds writes.
    fn count_descendants(dom: &Dom) -> usize {
        dom.children
            .as_ref()
            .iter()
            .map(|c| 1 + count_descendants(c))
            .sum()
    }

    /// Renders `dd`, then hands back both the DOM *and* the very `RefAny` the
    /// widget registered on its own focus callback. Driving `on_dropdown_click`
    /// with that `RefAny` is the real wiring - nothing is re-created by hand, so
    /// a mismatch between what `dom()` stores and what the handler expects
    /// cannot hide behind the fixture.
    fn rendered(dd: DropDown) -> (Dom, RefAny) {
        let dom = dd.dom();
        let refany = dom.root.callbacks.as_ref()[0].refany.clone();
        (dom, refany)
    }

    // ------------------------------------------------------------------
    // CallbackInfo harness (mirrors the one in `check_box.rs` / `timer.rs`)
    // ------------------------------------------------------------------

    struct Env<'a> {
        ref_data: &'a CallbackInfoRefData<'a>,
        changes: &'a Arc<Mutex<Vec<CallbackChange>>>,
        hit: DomNodeId,
    }

    impl Env<'_> {
        fn info(&self) -> CallbackInfo {
            CallbackInfo::new(
                self.ref_data,
                self.changes,
                self.hit,
                OptionLogicalPosition::None,
                OptionLogicalPosition::None,
            )
        }

        fn take_changes(&self) -> Vec<CallbackChange> {
            self.changes
                .lock()
                .map(|mut c| core::mem::take(&mut *c))
                .unwrap_or_default()
        }

        fn take_one(&self) -> CallbackChange {
            let mut changes = self.take_changes();
            assert_eq!(changes.len(), 1, "expected exactly one change: {changes:?}");
            changes.remove(0)
        }
    }

    /// The tag the hit-tester would use for `node`. `open_menu_for_node` resolves
    /// the anchor rect through this mapping, so a forged hit-test area must reuse
    /// the id the styling pass actually assigned.
    fn tag_of(styled_dom: &StyledDom, node: NodeId) -> u64 {
        let nid = NodeHierarchyItemId::from_crate_internal(Some(node));
        styled_dom
            .tag_ids_to_node_ids
            .iter()
            .find(|m| m.node_id == nid)
            .expect("the dropdown trigger must be hit-testable")
            .tag_id
            .inner
    }

    /// A `DomLayoutResult` carrying only a `styled_dom` plus (optionally) one
    /// forged hit-test area. The dropdown handler reaches exactly one geometry
    /// query (`get_node_hit_test_bounds`), which reads the display list only -
    /// no real layout (and no font) is needed.
    fn layout_result(
        styled_dom: StyledDom,
        anchor: Option<(NodeId, LogicalRect)>,
    ) -> DomLayoutResult {
        let mut display_list = DisplayList::default();
        if let Some((node, rect)) = anchor {
            let tag = tag_of(&styled_dom, node);
            display_list.items.push(DisplayListItem::HitTestArea {
                bounds: WindowLogicalRect::new(rect.origin, rect.size),
                // The tag TYPE matters: `get_node_hit_test_bounds` looks for a
                // DOM-node area specifically, because `tag.0` is also used by
                // text-run cursor areas with a colliding numbering scheme. A
                // forged area must therefore carry the same type the display
                // list builder stamps.
                tag: (tag, azul_core::hit_test::TAG_TYPE_DOM_NODE),
            });
        }

        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(display_list),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Runs `f` with a callback environment over an empty `LayoutWindow` and no
    /// hit node - the "nothing to anchor to" case.
    fn with_env<R>(f: impl FnOnce(&Env<'_>) -> R) -> R {
        with_env_cfg(None, f)
    }

    /// Runs `f` with a callback environment whose root DOM is `styled_dom`, whose
    /// node `node` has the hit-test rect `rect`, and whose hit node is `node`.
    fn with_anchored_env<R>(
        styled_dom: StyledDom,
        node: NodeId,
        rect: LogicalRect,
        f: impl FnOnce(&Env<'_>) -> R,
    ) -> R {
        with_env_cfg(Some((styled_dom, node, rect)), f)
    }

    fn with_env_cfg<R>(
        anchored: Option<(StyledDom, NodeId, LogicalRect)>,
        f: impl FnOnce(&Env<'_>) -> R,
    ) -> R {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");

        let hit = match anchored {
            Some((styled_dom, node, rect)) => {
                layout_window.layout_results.insert(
                    DomId::ROOT_ID,
                    layout_result(styled_dom, Some((node, rect))),
                );
                DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::from_crate_internal(Some(node)),
                }
            }
            None => DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::NONE,
            },
        };

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let env = Env {
            ref_data: &ref_data,
            changes: &changes,
            hit,
        };
        f(&env)
    }

    /// The labels of a queued `OpenMenu` change's items, in menu order.
    fn menu_labels(menu: &Menu) -> Vec<String> {
        menu.items
            .as_ref()
            .iter()
            .map(|i| match i {
                MenuItem::String(s) => s.label.as_str().to_string(),
                other => panic!("the dropdown must only emit string items, got {other:?}"),
            })
            .collect()
    }

    // ==================================================================
    // DropDown::new / Default  (constructor invariants)
    // ==================================================================

    #[test]
    fn new_keeps_the_choices_and_starts_unselected_without_a_callback() {
        let dd = DropDown::new(choices(&["a", "b", "c"]));

        assert_eq!(dd.choices.len(), 3);
        assert_eq!(
            dd.choices
                .as_slice()
                .iter()
                .map(AzString::as_str)
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"],
            "choices must be stored verbatim, in order",
        );
        assert_eq!(dd.selected, 0, "a fresh dropdown selects the first item");
        assert!(
            dd.on_choice_change.as_ref().is_none(),
            "`new` must not invent a callback",
        );
    }

    #[test]
    fn new_preserves_every_adversarial_choice_byte_for_byte() {
        let originals = adversarial_choices();
        let dd = DropDown::new(StringVec::from_vec(
            originals
                .iter()
                .cloned()
                .map(AzString::from_string)
                .collect(),
        ));

        assert_eq!(
            dd.choices.len(),
            originals.len(),
            "no choice may be dropped"
        );
        for (stored, original) in dd.choices.as_slice().iter().zip(&originals) {
            assert_eq!(
                stored.as_str(),
                original.as_str(),
                "a NUL / bidi / astral label must survive the StringVec round-trip",
            );
            assert_eq!(
                stored.as_str().len(),
                original.len(),
                "byte length must be preserved — an embedded NUL must not truncate",
            );
        }
    }

    #[test]
    fn new_on_an_empty_choice_list_still_reports_index_zero() {
        // `selected == 0` points *past the end* of an empty list. That is the
        // documented starting state, so every consumer (notably `dom()`) has to
        // tolerate an out-of-range selection from the very first frame.
        let dd = DropDown::new(StringVec::from_const_slice(&[]));
        assert!(dd.choices.is_empty());
        assert_eq!(dd.selected, 0);
        assert!(dd.choices.as_slice().get(dd.selected).is_none());
    }

    #[test]
    fn new_with_ten_thousand_choices_keeps_len_and_capacity_consistent() {
        let n = 10_000;
        let dd = DropDown::new(StringVec::from_vec(
            (0..n)
                .map(|i| AzString::from_string(i.to_string()))
                .collect(),
        ));

        assert_eq!(dd.choices.len(), n);
        assert!(
            dd.choices.capacity() >= dd.choices.len(),
            "capacity must never be smaller than len",
        );
        assert_eq!(
            dd.choices.as_slice().len(),
            n,
            "the C slice view must agree with len"
        );
        assert_eq!(dd.choices.as_slice()[n - 1].as_str(), (n - 1).to_string());
    }

    #[test]
    fn default_is_the_empty_unselected_dropdown() {
        let dd = DropDown::default();
        assert!(dd.choices.is_empty());
        assert_eq!(dd.selected, 0);
        assert!(dd.on_choice_change.as_ref().is_none());
        assert_eq!(dd, DropDown::new(StringVec::from_const_slice(&[])));
    }

    // ==================================================================
    // set_on_choice_change / with_on_choice_change
    // ==================================================================

    #[test]
    fn set_on_choice_change_stores_the_exact_refany_and_function_pointer() {
        let l = log();
        let data = RefAny::new(l);
        let mut dd = DropDown::new(choices(&["a"]));
        dd.set_on_choice_change(data.clone(), cb(record_choice));

        let stored = dd
            .on_choice_change
            .as_ref()
            .expect("the callback must be stored");
        assert_eq!(
            stored.refany, data,
            "the widget must hold the caller's allocation, not a copy",
        );
        assert_eq!(
            stored.callback.cb as usize, record_choice as usize,
            "the function pointer must round-trip unchanged",
        );
        assert!(
            stored.callback.ctx.as_ref().is_none(),
            "a native Rust callback has no FFI context",
        );
    }

    #[test]
    fn set_on_choice_change_is_last_write_wins() {
        let mut dd = DropDown::new(choices(&["a"]));
        dd.set_on_choice_change(RefAny::new(log()), cb(record_choice));
        let second = RefAny::new(log());
        dd.set_on_choice_change(second.clone(), cb(reject_choice));

        let stored = dd
            .on_choice_change
            .as_ref()
            .expect("still exactly one callback");
        assert_eq!(stored.callback.cb as usize, reject_choice as usize);
        assert_eq!(
            stored.refany, second,
            "the second registration must replace the first"
        );
        assert_ne!(
            record_choice as usize, reject_choice as usize,
            "the two probes must not have been folded into one symbol",
        );
    }

    /// The trigger must DISPLAY the selected choice, and `set_selected` /
    /// `with_selected` must be the way a host says which one that is.
    ///
    /// `selected` was a public field with no setter, so a host had no supported
    /// way to feed back the index its `on_choice_change` callback was handed —
    /// and since the widget is rebuilt from host state every layout, the trigger
    /// showed choice 0 forever no matter what the user picked.
    #[test]
    fn selecting_a_choice_changes_what_the_trigger_displays() {
        let items = choices(&["Alpha", "Beta", "Gamma"]);

        // Default: the first choice.
        assert_eq!(label_of(&DropDown::new(items.clone()).dom()), "Alpha");

        // Every index round-trips into the rendered label.
        for (i, expected) in ["Alpha", "Beta", "Gamma"].iter().enumerate() {
            let dd = DropDown::new(items.clone()).with_selected(i);
            assert_eq!(dd.selected, i, "with_selected must store the index");
            assert_eq!(
                label_of(&dd.dom()),
                *expected,
                "the trigger must display choice {i}",
            );
        }

        // The setter and the builder agree.
        let mut a = DropDown::new(items.clone());
        a.set_selected(2);
        assert_eq!(a, DropDown::new(items.clone()).with_selected(2));

        // Out of range renders empty rather than panicking (same as no choices).
        assert_eq!(label_of(&DropDown::new(items).with_selected(99).dom()), "");
    }

    #[test]
    fn set_on_choice_change_does_not_disturb_the_choices_or_the_selection() {
        let mut dd = adversarial_dropdown();
        dd.selected = usize::MAX;
        let before = dd.choices.clone();

        dd.set_on_choice_change(RefAny::new(log()), cb(record_choice));

        assert_eq!(
            dd.choices, before,
            "registering a callback must not touch the model"
        );
        assert_eq!(
            dd.selected,
            usize::MAX,
            "…nor the selection, however out of range"
        );
    }

    #[test]
    fn with_on_choice_change_is_the_setter_plus_a_move() {
        let data = RefAny::new(log());
        let built = DropDown::new(choices(&["a", "b"]))
            .with_on_choice_change(data.clone(), cb(record_choice));

        let mut expected = DropDown::new(choices(&["a", "b"]));
        expected.set_on_choice_change(data, cb(record_choice));

        assert_eq!(
            built, expected,
            "the builder must not differ from the setter"
        );
    }

    #[test]
    fn with_on_choice_change_preserves_an_out_of_range_selection() {
        let mut dd = DropDown::new(choices(&["a"]));
        dd.selected = usize::MAX;
        let dd = dd.with_on_choice_change(RefAny::new(log()), cb(record_choice));

        assert_eq!(
            dd.selected,
            usize::MAX,
            "the builder must not silently clamp"
        );
        assert_eq!(dd.choices.len(), 1);
    }

    #[test]
    fn with_on_choice_change_accepts_a_zero_choice_dropdown() {
        let dd = DropDown::default().with_on_choice_change(RefAny::new(log()), cb(record_choice));
        assert!(dd.choices.is_empty());
        assert!(
            dd.on_choice_change.as_ref().is_some(),
            "a callback on an empty dropdown is legal — it just can never fire",
        );
    }

    // ==================================================================
    // swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_moves_the_original_out_and_leaves_a_default() {
        let data = RefAny::new(log());
        let mut dd = DropDown::new(choices(&["a", "b"]))
            .with_on_choice_change(data.clone(), cb(record_choice));
        dd.selected = 1;

        let taken = dd.swap_with_default();

        assert_eq!(taken.choices.len(), 2);
        assert_eq!(taken.selected, 1);
        assert_eq!(
            taken
                .on_choice_change
                .as_ref()
                .expect("callback moved out")
                .refany,
            data,
        );
        assert_eq!(
            dd,
            DropDown::default(),
            "what stays behind must be the default"
        );
    }

    #[test]
    fn swap_with_default_is_idempotent_after_the_first_call() {
        let mut dd = adversarial_dropdown();
        let _first = dd.swap_with_default();
        let second = dd.swap_with_default();

        assert_eq!(
            second,
            DropDown::default(),
            "the second take yields a default"
        );
        assert_eq!(
            dd,
            DropDown::default(),
            "…and leaves another default behind"
        );
    }

    #[test]
    fn swap_with_default_preserves_an_out_of_range_selection_and_huge_labels() {
        let mut dd = adversarial_dropdown();
        dd.selected = usize::MAX;
        let n = dd.choices.len();

        let taken = dd.swap_with_default();

        assert_eq!(
            taken.selected,
            usize::MAX,
            "swap must not normalise anything"
        );
        assert_eq!(taken.choices.len(), n);
        assert_eq!(dd.selected, 0);
        assert!(dd.choices.is_empty());
    }

    // ==================================================================
    // DropDown::dom
    // ==================================================================

    #[test]
    fn dom_labels_the_selected_choice() {
        for (idx, expected) in [(0, "alpha"), (1, "beta"), (2, "gamma")] {
            let mut dd = DropDown::new(choices(&["alpha", "beta", "gamma"]));
            dd.selected = idx;
            let dom = dd.dom();
            assert_eq!(
                label_of(&dom),
                expected,
                "index {idx} must label the trigger"
            );
        }
    }

    #[test]
    fn dom_falls_back_to_an_empty_label_for_an_out_of_range_selection() {
        // len, len+1 and the arithmetic limit: `.get()` returns None for all of
        // them, and the documented fallback is the empty string — never a panic
        // and never a stale neighbour.
        for idx in [3_usize, 4, usize::MAX - 1, usize::MAX] {
            let mut dd = DropDown::new(choices(&["a", "b", "c"]));
            dd.selected = idx;
            let dom = dd.dom();
            assert_eq!(
                label_of(&dom),
                "",
                "selected = {idx} must render an empty label"
            );
        }
    }

    #[test]
    fn dom_on_an_empty_dropdown_renders_an_empty_label() {
        let dom = DropDown::default().dom();
        assert_eq!(label_of(&dom), "");
        assert_eq!(
            dom.children.len(),
            2,
            "label + arrow are rendered regardless"
        );
    }

    #[test]
    fn dom_label_survives_unicode_embedded_nuls_and_huge_strings() {
        let originals = adversarial_choices();
        for (idx, original) in originals.iter().enumerate() {
            let mut dd = DropDown::new(StringVec::from_vec(
                originals
                    .iter()
                    .cloned()
                    .map(AzString::from_string)
                    .collect(),
            ));
            dd.selected = idx;
            let dom = dd.dom();
            assert_eq!(
                label_of(&dom),
                original.as_str(),
                "label {idx} must reach the text node byte-for-byte",
            );
        }
    }

    #[test]
    fn dom_shape_is_a_trigger_with_a_wrapped_label_and_an_arrow_icon() {
        let dom = DropDown::new(choices(&["a"])).dom();

        assert!(
            matches!(dom.root.get_node_type(), NodeType::Div),
            "the trigger is a div"
        );
        assert_eq!(dom.children.len(), 2, "exactly a label and an arrow");

        let kids = dom.children.as_ref();
        assert!(
            matches!(kids[0].root.get_node_type(), NodeType::P),
            "the label is block-formatted"
        );
        assert_eq!(
            kids[0].children.len(),
            1,
            "the <p> wraps exactly one text node"
        );

        match kids[1].root.get_node_type() {
            NodeType::Icon(s) => assert_eq!(s.as_ref().as_str(), "arrow_drop_down"),
            other => panic!("expected the arrow icon, got {other:?}"),
        }
        assert_eq!(
            kids[1].children.len(),
            1,
            "the icon carries its glyph slot (a text leaf)"
        );
    }

    #[test]
    fn dom_child_count_cache_is_honest_for_every_selection() {
        for idx in [0_usize, 1, 2, 99, usize::MAX] {
            let mut dd = DropDown::new(choices(&["a", "b"]));
            dd.selected = idx;
            let dom = dd.dom();
            assert_eq!(
                dom.estimated_total_children,
                count_descendants(&dom),
                "selected = {idx}: a stale cache makes the compact-DOM arena under-allocate",
            );
            assert_eq!(
                dom.estimated_total_children, 4,
                "p + text + icon + its glyph slot"
            );
        }
    }

    #[test]
    fn dom_marks_the_trigger_focusable_and_gives_it_the_widget_class() {
        let dom = DropDown::new(choices(&["a"])).dom();

        assert_eq!(
            dom.root.get_tab_index(),
            Some(TabIndex::Auto),
            "the popup opens on focus, so the trigger must be reachable by keyboard",
        );
        assert_eq!(classes(&dom), vec!["__azul-native-dropdown".to_string()]);
    }

    #[test]
    fn dom_registers_exactly_one_focus_received_callback() {
        let dom = DropDown::new(choices(&["a", "b"])).dom();
        let cbs = dom.root.callbacks.as_ref();

        assert_eq!(
            cbs.len(),
            1,
            "one handler — a duplicate would open two popups"
        );
        assert_eq!(
            cbs[0].event,
            EventFilter::Focus(FocusEventFilter::FocusReceived)
        );
        assert_eq!(cbs[0].callback.cb, on_dropdown_click as usize);
        assert!(cbs[0].callback.ctx.as_ref().is_none());
    }

    #[test]
    fn dom_hands_the_whole_widget_to_the_callback_refany() {
        let mut dd = adversarial_dropdown();
        dd.selected = 4;
        let expected: Vec<String> = dd
            .choices
            .as_slice()
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();

        let (_dom, mut refany) = rendered(dd);
        let stored = refany
            .downcast_ref::<DropDown>()
            .expect("dom() must store the DropDown itself, unwrapped");

        assert_eq!(stored.selected, 4);
        assert_eq!(
            stored
                .choices
                .as_slice()
                .iter()
                .map(|c| c.as_str().to_string())
                .collect::<Vec<_>>(),
            expected,
            "the handler must see the same choices the label was built from",
        );
    }

    #[test]
    fn from_dropdown_for_dom_renders_the_same_trigger_as_dom() {
        let mut dd = DropDown::new(choices(&["a", "b", "c"]));
        dd.selected = 2;

        let direct = dd.clone().dom();
        let converted = Dom::from(dd);

        assert_eq!(label_of(&direct), label_of(&converted));
        assert_eq!(direct.children.len(), converted.children.len());
        assert_eq!(classes(&direct), classes(&converted));
        assert_eq!(
            direct.estimated_total_children,
            converted.estimated_total_children
        );
    }

    #[test]
    fn dom_with_ten_thousand_choices_renders_only_the_selected_one() {
        let n = 10_000;
        let mut dd = DropDown::new(StringVec::from_vec(
            (0..n)
                .map(|i| AzString::from_string(i.to_string()))
                .collect(),
        ));
        dd.selected = n - 1;

        let dom = dd.dom();
        assert_eq!(label_of(&dom), (n - 1).to_string());
        assert_eq!(
            dom.estimated_total_children, 4,
            "the trigger must not materialise one node per choice",
        );
    }

    #[test]
    fn dom_does_not_mutate_the_selection_it_was_given() {
        // The widget is stateless w.r.t. selection: `dom()` reads `selected` and
        // never writes it. Anything that changes the label has to go through the
        // caller's own state, updated from the choice-change callback.
        let mut dd = DropDown::new(choices(&["a", "b"]));
        dd.selected = 1;
        let (_dom, mut refany) = rendered(dd);
        assert_eq!(
            refany
                .downcast_ref::<DropDown>()
                .expect("stored widget")
                .selected,
            1
        );
    }

    // ==================================================================
    // on_dropdown_click
    // ==================================================================

    #[test]
    fn on_dropdown_click_ignores_a_refany_of_the_wrong_type() {
        with_env(|env| {
            let update = on_dropdown_click(RefAny::new(0_usize), env.info());
            assert_eq!(update, Update::DoNothing);
            assert!(
                env.take_changes().is_empty(),
                "a type mismatch must be a silent no-op, not a half-built menu",
            );
        });
    }

    #[test]
    fn on_dropdown_click_without_a_hit_node_opens_nothing() {
        let (_dom, refany) = rendered(DropDown::new(choices(&["a", "b"])));
        with_env(|env| {
            // The hit node is NONE and the window has no layout results, so the
            // popup has nothing to anchor to.
            let update = on_dropdown_click(refany.clone(), env.info());
            assert_eq!(update, Update::DoNothing);
            assert!(
                env.take_changes().is_empty(),
                "a failed anchor must not queue a half-open menu",
            );
        });
    }

    #[test]
    fn on_dropdown_click_opens_one_menu_item_per_choice_in_order() {
        let labels = ["a", "", "\u{5E9}\u{5DC}\u{5D5}\u{5DD}", "a\0b"];
        let (dom, refany) = rendered(DropDown::new(choices(&labels)));
        let styled_dom = StyledDom::create_from_dom(dom);
        let rect = LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(100.0, 30.0),
        );

        with_anchored_env(styled_dom, NodeId::new(0), rect, |env| {
            let update = on_dropdown_click(refany.clone(), env.info());
            assert_eq!(
                update,
                Update::DoNothing,
                "opening the popup is not a re-layout"
            );

            let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                panic!("expected exactly one OpenMenu change");
            };
            assert_eq!(
                menu_labels(&menu),
                labels.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
                "menu order must mirror choice order, NULs and RTL included",
            );
        });
    }

    #[test]
    fn on_dropdown_click_on_an_empty_dropdown_opens_an_empty_menu() {
        let (dom, refany) = rendered(DropDown::default());
        let styled_dom = StyledDom::create_from_dom(dom);
        let rect = LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(1.0, 1.0));

        with_anchored_env(styled_dom, NodeId::new(0), rect, |env| {
            assert_eq!(
                on_dropdown_click(refany.clone(), env.info()),
                Update::DoNothing
            );
            let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                panic!("expected an OpenMenu change");
            };
            assert!(
                menu.items.is_empty(),
                "no choices means no items — not a panic"
            );
        });
    }

    #[test]
    fn on_dropdown_click_anchors_the_menu_below_the_trigger() {
        let (dom, refany) = rendered(DropDown::new(choices(&["a"])));
        let styled_dom = StyledDom::create_from_dom(dom);
        let rect = LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(100.0, 30.0),
        );

        with_anchored_env(styled_dom, NodeId::new(0), rect, |env| {
            on_dropdown_click(refany.clone(), env.info());
            let CallbackChange::OpenMenu {
                menu,
                position,
                anchor,
            } = env.take_one()
            else {
                panic!("expected an OpenMenu change");
            };
            let p = position.expect("the popup must be pinned to the trigger");
            assert_eq!((p.x, p.y), (10.0, 50.0), "bottom-left of the trigger rect");
            // THE WHOLE TRIGGER RECT travels with the request, not just its
            // bottom-left corner: a backend needs the WIDTH to make the menu at
            // least as wide as the control, the way a `<select>` does, and the
            // rect to edge-flip against (2026-09-01 request). Dropping it here
            // is what left every drop-down menu item-width and looking like a
            // stray context menu.
            let a = anchor.expect("a menu opened FOR a node carries that node's rect");
            assert_eq!(
                (a.origin.x, a.origin.y, a.size.width, a.size.height),
                (10.0, 20.0, 100.0, 30.0),
                "the anchor is the trigger rect itself",
            );
            assert!(matches!(menu.position, MenuPopupPosition::BottomOfHitRect));
            assert!(matches!(
                menu.context_mouse_btn,
                ContextMenuMouseButton::Right
            ));
        });
    }

    #[test]
    fn on_dropdown_click_is_repeatable_and_does_not_consume_the_widget() {
        let (dom, refany) = rendered(DropDown::new(choices(&["a", "b"])));
        let styled_dom = StyledDom::create_from_dom(dom);
        let rect = LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(4.0, 8.0));

        with_anchored_env(styled_dom, NodeId::new(0), rect, |env| {
            for round in 0..3 {
                on_dropdown_click(refany.clone(), env.info());
                let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                    panic!("round {round}: expected an OpenMenu change");
                };
                assert_eq!(menu_labels(&menu), vec!["a".to_string(), "b".to_string()]);
            }
        });
    }

    #[test]
    fn on_dropdown_click_tags_every_item_with_its_own_index_and_handler() {
        let l = log();
        let dd = DropDown::new(choices(&["a", "b", "c"]))
            .with_on_choice_change(RefAny::new(l), cb(record_choice));
        let (dom, refany) = rendered(dd);
        let styled_dom = StyledDom::create_from_dom(dom);
        let rect = LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(4.0, 8.0));

        with_anchored_env(styled_dom, NodeId::new(0), rect, |env| {
            on_dropdown_click(refany.clone(), env.info());
            let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                panic!("expected an OpenMenu change");
            };

            for (idx, item) in menu.items.as_ref().iter().enumerate() {
                let MenuItem::String(s) = item else {
                    panic!("item {idx} is not a string item");
                };
                let menu_cb = s.callback.as_ref().expect("every item must be clickable");
                assert_eq!(
                    menu_cb.callback.cb, on_choice_selected as usize,
                    "item {idx} must route through the widget's own handler",
                );
                let mut data = menu_cb.refany.clone();
                let payload = data
                    .downcast_ref::<ChoiceCallbackData>()
                    .expect("the item payload must be a ChoiceCallbackData");
                assert_eq!(payload.choice_id, idx, "item {idx} carries the wrong index");
                assert!(
                    payload.on_choice_change.as_ref().is_some(),
                    "item {idx} lost the user callback on the way into the menu",
                );
            }
        });
    }

    // ==================================================================
    // on_choice_selected
    // ==================================================================

    #[test]
    fn on_choice_selected_ignores_a_refany_of_the_wrong_type() {
        with_env(|env| {
            let update = on_choice_selected(RefAny::new(0_usize), env.info());
            assert_eq!(update, Update::DoNothing);
            assert!(env.take_changes().is_empty());
        });
    }

    #[test]
    fn on_choice_selected_without_a_registered_callback_does_nothing() {
        let data = RefAny::new(ChoiceCallbackData {
            choice_id: 7,
            on_choice_change: None.into(),
        });
        with_env(|env| {
            let update = on_choice_selected(data.clone(), env.info());
            assert_eq!(
                update,
                Update::DoNothing,
                "an unwired dropdown must stay silent"
            );
            assert!(env.take_changes().is_empty());
        });
    }

    #[test]
    fn on_choice_selected_forwards_the_index_and_propagates_the_return_value() {
        let l = log();
        let data = RefAny::new(ChoiceCallbackData {
            choice_id: 2,
            on_choice_change: Some(DropDownOnChoiceChange {
                refany: RefAny::new(l.clone()),
                callback: cb(record_choice),
            })
            .into(),
        });

        with_env(|env| {
            let update = on_choice_selected(data.clone(), env.info());
            assert_eq!(
                update,
                Update::RefreshDom,
                "the user's Update must not be swallowed"
            );
        });
        assert_eq!(entries(&l), vec![2], "the callback must see its own index");
    }

    #[test]
    fn on_choice_selected_forwards_usize_max_unchanged() {
        // `choice_id` is a plain index with no upper bound: the limit value must
        // pass through untouched rather than wrap, saturate or panic.
        let l = log();
        let data = RefAny::new(ChoiceCallbackData {
            choice_id: usize::MAX,
            on_choice_change: Some(DropDownOnChoiceChange {
                refany: RefAny::new(l.clone()),
                callback: cb(record_choice),
            })
            .into(),
        });

        with_env(|env| {
            assert_eq!(
                on_choice_selected(data.clone(), env.info()),
                Update::RefreshDom
            );
        });
        assert_eq!(entries(&l), vec![usize::MAX]);
    }

    #[test]
    fn on_choice_selected_is_repeatable_on_the_same_payload() {
        let l = log();
        let data = RefAny::new(ChoiceCallbackData {
            choice_id: 1,
            on_choice_change: Some(DropDownOnChoiceChange {
                refany: RefAny::new(l.clone()),
                callback: cb(record_choice),
            })
            .into(),
        });

        with_env(|env| {
            // The handler takes an *exclusive* borrow of the payload; if it were
            // ever leaked, the second call would fail to downcast and silently
            // return DoNothing.
            for _ in 0..3 {
                assert_eq!(
                    on_choice_selected(data.clone(), env.info()),
                    Update::RefreshDom
                );
            }
        });
        assert_eq!(
            entries(&l),
            vec![1, 1, 1],
            "the borrow must be released each time"
        );
    }

    #[test]
    fn on_choice_selected_uses_the_callback_that_was_registered_last() {
        let l = log();
        let mut dd = DropDown::new(choices(&["a", "b"]));
        dd.set_on_choice_change(RefAny::new(l.clone()), cb(record_choice));
        dd.set_on_choice_change(RefAny::new(l.clone()), cb(reject_choice));

        let data = RefAny::new(ChoiceCallbackData {
            choice_id: 1,
            on_choice_change: dd.on_choice_change.clone(),
        });

        with_env(|env| {
            assert_eq!(
                on_choice_selected(data.clone(), env.info()),
                Update::RefreshDomAllWindows
            );
        });
        assert_eq!(
            entries(&l),
            vec![1 + SENTINEL],
            "the replaced callback must not fire"
        );
    }

    // ==================================================================
    // End-to-end: focus -> popup -> pick an item
    // ==================================================================

    #[test]
    fn picking_a_menu_item_delivers_exactly_that_index_to_the_user_callback() {
        let l = log();
        let dd = DropDown::new(choices(&["a", "b", "c", "d"]))
            .with_on_choice_change(RefAny::new(l.clone()), cb(record_choice));
        let (dom, refany) = rendered(dd);
        let styled_dom = StyledDom::create_from_dom(dom);
        let rect = LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(4.0, 8.0));

        with_anchored_env(styled_dom, NodeId::new(0), rect, |env| {
            on_dropdown_click(refany.clone(), env.info());
            let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                panic!("expected an OpenMenu change");
            };

            // Deliberately out of order: the index must come from the item, not
            // from the order in which items happen to be clicked.
            for idx in [3_usize, 0, 2, 1] {
                let MenuItem::String(s) = &menu.items.as_ref()[idx] else {
                    panic!("item {idx} is not a string item");
                };
                let payload = s.callback.as_ref().expect("clickable").refany.clone();
                assert_eq!(
                    on_choice_selected(payload, env.info()),
                    Update::RefreshDom,
                    "item {idx} must reach the user callback",
                );
            }
        });

        assert_eq!(entries(&l), vec![3, 0, 2, 1]);
    }

    #[test]
    fn picking_an_item_does_not_move_the_widgets_own_selection() {
        // NOTE (documented behaviour, not an accident): `DropDown` never updates
        // its own `selected` field. Selection state lives with the caller, which
        // is why the trigger label only changes once the caller re-renders. If
        // this ever starts self-updating, this assertion is the tripwire.
        let l = log();
        let dd = DropDown::new(choices(&["a", "b"]))
            .with_on_choice_change(RefAny::new(l.clone()), cb(record_choice));
        let (dom, mut refany) = rendered(dd);
        let styled_dom = StyledDom::create_from_dom(dom);
        let rect = LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(4.0, 8.0));

        with_anchored_env(styled_dom, NodeId::new(0), rect, |env| {
            on_dropdown_click(refany.clone(), env.info());
            let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                panic!("expected an OpenMenu change");
            };
            let MenuItem::String(s) = &menu.items.as_ref()[1] else {
                panic!("item 1 is not a string item");
            };
            let payload = s.callback.as_ref().expect("clickable").refany.clone();
            on_choice_selected(payload, env.info());
        });

        assert_eq!(entries(&l), vec![1], "the pick was delivered");
        assert_eq!(
            refany
                .downcast_ref::<DropDown>()
                .expect("stored widget")
                .selected,
            0,
            "the widget's own `selected` stays where the caller put it",
        );
    }
}
