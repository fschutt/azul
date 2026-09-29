//! `<input type=..>`, `<select>` and `<textarea>` become azul widgets.
//!
//! An app that writes `Dom::create_input_no_a11y("range", ..)` - or the same
//! control in XML - gets a `Slider`, the way an `<icon>` gets its icon: the
//! raw form node is REPLACED, before the cascade, by the widget its `type`
//! names, with the node's attributes mapped onto the widget and its ids,
//! classes, inline style, callbacks, tab index and key carried to the
//! widget's root. Every path from a user `Dom` to a `StyledDom` goes through
//! `LayoutWindow::style_user_dom*`, so that is where these tests look - no
//! test here touches the replacement pass directly, which is also what keeps
//! them compiling against the pre-replacement engine (where they fail, since
//! the raw nodes survive).
//!
//! The widget's STATE is the other half: a checkbox the user checked, a
//! select option the user picked, text the user typed - all survive the
//! app's next rebuild, because nothing in a raw `<input>` app stores them.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use azul_core::{
    a11y::{AccessibilityRole, AccessibilityState},
    callbacks::{Update, VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{
        AttributeNameValue, AttributeType, Dom, DomId, DomNodeId, EventFilter, HoverEventFilter,
        NodeData, NodeId, NodeType, TabIndex,
    },
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    hit_test::ScrollPosition,
    refany::{OptionRefAny, RefAny},
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{MonitorVec, RawWindowHandle},
};
use azul_css::{
    css::CssDeclaration,
    dynamic_selector::CssPropertyWithConditions,
    props::{basic::color::ColorU, layout::LayoutWidth, property::CssProperty},
    system::SystemStyle,
};
use azul_layout::{
    callbacks::{Callback, CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
    widgets::{
        check_box::CHECKBOX_CONTAINER_CLASS,
        color_input::{color_to_hex, COLOR_INPUT_CLASS},
        drop_down::DropDown,
        radio_group::{RadioGroupState, RadioGroupStateWrapper},
        text_input::{TextInputState, TextInputStateWrapper, TEXT_INPUT_CONTAINER_CLASS},
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const SLIDER_CLASS: &str = "__azul-native-slider";
const RADIO_GROUP_CLASS: &str = "__azul-native-radio-group";
const BUTTON_CLASS: &str = "__azul-native-button";
const DATE_PICKER_CLASS: &str = "__azul-native-date-picker";
const TIME_PICKER_CLASS: &str = "__azul-native-time-picker";
const DROP_DOWN_CLASS: &str = "__azul-native-dropdown";
const TEXT_AREA_CLASS: &str = "__azul-native-text-area-container";
const COMBOBOX_CLASS: &str = "__azul-native-combobox";

// ── Fixtures ────────────────────────────────────────────────────────────────

/// A window that can STYLE a DOM. No font scan: styling needs no fonts.
fn styling_window() -> LayoutWindow {
    window_with(FcFontCache::default())
}

/// A window that can also lay out and shape text.
fn text_window() -> LayoutWindow {
    window_with(FcFontCache::build())
}

fn window_with(fonts: FcFontCache) -> LayoutWindow {
    let mut lw = LayoutWindow::new(fonts).expect("LayoutWindow::new");
    lw.system_animations_override = Some(azul_core::resources::SystemAnimations::disabled());
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state;
    lw
}

/// `<input type=ty>` the way an app writes it.
fn input(ty: &str) -> Dom {
    Dom::create_input_no_a11y(ty.into(), "field".into(), "Field".into())
}

fn page(child: Dom) -> Dom {
    Dom::create_body().with_child(child)
}

fn attr(name: &str, value: &str) -> AttributeType {
    AttributeType::Custom(AttributeNameValue {
        attr_name: name.into(),
        value: value.into(),
    })
}

fn node(styled: &StyledDom, id: NodeId) -> &NodeData {
    &styled.node_data.as_ref()[id.index()]
}

/// The class name the check box's container wears (the widget exports it as
/// an `IdOrClass` slice).
fn checkbox_container() -> &'static str {
    match &CHECKBOX_CONTAINER_CLASS[0] {
        azul_core::dom::IdOrClass::Class(c) => c.as_str(),
        azul_core::dom::IdOrClass::Id(i) => i.as_str(),
    }
}

fn all_nodes(styled: &StyledDom) -> Vec<NodeId> {
    (0..styled.node_data.as_ref().len()).map(NodeId::new).collect()
}

fn with_class(styled: &StyledDom, class: &str) -> Vec<NodeId> {
    all_nodes(styled)
        .into_iter()
        .filter(|id| node(styled, *id).has_class(class))
        .collect()
}

fn one_with_class(styled: &StyledDom, class: &str) -> NodeId {
    let found = with_class(styled, class);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one node with class {class}, found {found:?} in {:?}",
        node_types(styled)
    );
    found[0]
}

fn node_types(styled: &StyledDom) -> Vec<String> {
    all_nodes(styled)
        .into_iter()
        .map(|id| format!("{:?}", node(styled, id).get_node_type()))
        .collect()
}

/// Raw form nodes the replacement should have removed.
fn raw_form_nodes(styled: &StyledDom) -> Vec<NodeId> {
    all_nodes(styled)
        .into_iter()
        .filter(|id| {
            matches!(
                node(styled, *id).get_node_type(),
                NodeType::Input | NodeType::Select | NodeType::TextArea
            )
        })
        .collect()
}

fn is_under(styled: &StyledDom, mut id: NodeId, ancestor: NodeId) -> bool {
    let hierarchy = styled.node_hierarchy.as_container();
    loop {
        if id == ancestor {
            return true;
        }
        match hierarchy[id].parent_id() {
            Some(parent) => id = parent,
            None => return false,
        }
    }
}

/// `root` and every node under it.
fn subtree(styled: &StyledDom, root: NodeId) -> Vec<NodeId> {
    all_nodes(styled)
        .into_iter()
        .filter(|id| is_under(styled, *id, root))
        .collect()
}

/// The text of every text node under `root`, in document order.
fn text_under(styled: &StyledDom, root: NodeId) -> String {
    subtree(styled, root)
        .into_iter()
        .filter_map(|id| match node(styled, id).get_node_type() {
            NodeType::Text(t) => Some(t.as_str().to_string()),
            _ => None,
        })
        .collect()
}

fn a11y_states(styled: &StyledDom, id: NodeId) -> Vec<AccessibilityState> {
    node(styled, id)
        .get_accessibility_info()
        .map(|a| a.states.as_slice().to_vec())
        .unwrap_or_default()
}

fn a11y_value(styled: &StyledDom, id: NodeId) -> Option<String> {
    node(styled, id)
        .get_accessibility_info()
        .and_then(|a| a.accessibility_value.as_ref().map(|v| v.as_str().to_string()))
}

fn dom_node(id: NodeId) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(id)),
    }
}

fn lay_out(lw: &mut LayoutWindow, styled: StyledDom) {
    let window_state = lw.current_window_state.clone();
    lw.layout_new_generation(
        styled,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .expect("layout");
}

/// Run `f` against a `CallbackInfo` over `lw`, hit on `hit`, the way a user
/// callback sees it.
fn with_info<R>(
    lw: &LayoutWindow,
    hit: DomNodeId,
    f: impl FnOnce(CallbackInfo) -> R,
) -> (R, Vec<CallbackChange>) {
    let renderer_resources = RendererResources::default();
    let previous_window_state: Option<FullWindowState> = None;
    let current_window_state = lw.current_window_state.clone();
    let gl_context = OptionGlContextPtr::None;
    let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
        BTreeMap::new();
    let window_handle = RawWindowHandle::Unsupported;
    let system_callbacks = ExternalSystemCallbacks::rust_internal();

    let ref_data = CallbackInfoRefData {
        layout_window: lw,
        renderer_resources: &renderer_resources,
        previous_window_state: &previous_window_state,
        current_window_state: &current_window_state,
        gl_context: &gl_context,
        current_scroll_manager: &scroll_states,
        current_window_handle: &window_handle,
        system_callbacks: &system_callbacks,
        system_style: Arc::new(SystemStyle::default()),
        monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
        #[cfg(feature = "icu")]
        icu_localizer: azul_layout::icu::IcuLocalizerHandle::default(),
        ctx: core::cell::RefCell::new(OptionRefAny::None),
    };
    let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
    let info = CallbackInfo::new(
        &ref_data,
        &changes,
        hit,
        OptionLogicalPosition::None,
        OptionLogicalPosition::None,
    );
    let out = f(info);
    let queued = info.take_changes();
    (out, queued)
}

// ── One test per mapping family ─────────────────────────────────────────────

#[test]
fn an_input_of_type_text_becomes_a_text_input_holding_its_value() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("text")
            .with_attribute(AttributeType::Value("ada".into()))
            .with_attribute(AttributeType::Placeholder("Your name".into())),
    ));
    assert_eq!(raw_form_nodes(&styled), vec![], "{:?}", node_types(&styled));
    let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
    assert!(
        node(&styled, field).is_contenteditable(),
        "the text input's container is the editing host"
    );
    assert_eq!(text_under(&styled, field), "ada", "the value attribute is the text");
    let prompt = subtree(&styled, field)
        .into_iter()
        .find_map(|id| node(&styled, id).get_placeholder().map(str::to_string));
    assert_eq!(prompt.as_deref(), Some("Your name"));
}

#[test]
fn an_input_without_a_type_or_with_an_unknown_one_is_a_text_input() {
    // HTML: a missing or unrecognised `type` is the text state.
    let lw = styling_window();
    let untyped = Dom::create_from_data(NodeData::create_node(NodeType::Input));
    let styled = lw.style_user_dom(
        Dom::create_body()
            .with_child(untyped)
            .with_child(input("banana")),
    );
    assert_eq!(raw_form_nodes(&styled), vec![]);
    assert_eq!(with_class(&styled, TEXT_INPUT_CONTAINER_CLASS).len(), 2);
}

#[test]
fn an_input_of_type_checkbox_becomes_a_check_box_that_is_checked_when_the_input_is() {
    let lw = styling_window();
    let checked = lw.style_user_dom(page(input("checkbox").with_attribute(AttributeType::CheckedTrue)));
    let unchecked = lw.style_user_dom(page(input("checkbox")));
    assert_eq!(raw_form_nodes(&checked), vec![]);
    let on = one_with_class(&checked, checkbox_container());
    let off = one_with_class(&unchecked, checkbox_container());
    assert_eq!(a11y_states(&checked, on), vec![AccessibilityState::CheckedTrue]);
    assert_eq!(a11y_states(&unchecked, off), vec![AccessibilityState::CheckedFalse]);
}

#[test]
fn an_input_of_type_radio_becomes_a_radio_checked_when_the_input_is() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("radio")
            .with_attribute(AttributeType::Value("red".into()))
            .with_attribute(AttributeType::CheckedTrue),
    ));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let group = one_with_class(&styled, RADIO_GROUP_CLASS);
    let radios: Vec<NodeId> = subtree(&styled, group)
        .into_iter()
        .filter(|id| {
            node(&styled, *id)
                .get_accessibility_info()
                .is_some_and(|a| a.role == AccessibilityRole::RadioButton)
        })
        .collect();
    assert_eq!(radios.len(), 1, "one input is one radio");
    assert_eq!(a11y_states(&styled, radios[0]), vec![AccessibilityState::CheckedTrue]);
}

#[test]
fn an_input_of_type_color_becomes_a_color_swatch_of_its_value() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("color").with_attribute(AttributeType::Value("#ff0000".into())),
    ));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let swatch = one_with_class(&styled, COLOR_INPUT_CLASS);
    let red = ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };
    assert_eq!(a11y_value(&styled, swatch), Some(color_to_hex(red)));
}

#[test]
fn an_input_of_type_file_becomes_a_file_button() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(input("file")));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let button = one_with_class(&styled, BUTTON_CLASS);
    assert!(matches!(node(&styled, button).get_node_type(), NodeType::Button));
}

#[test]
fn an_input_of_type_number_becomes_a_number_input_showing_its_value() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("number").with_attribute(AttributeType::Value("42".into())),
    ));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
    assert_eq!(text_under(&styled, field), "42");
}

#[test]
fn an_input_of_type_range_becomes_a_slider_within_min_and_max() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("range")
            .with_attribute(AttributeType::Min("0".into()))
            .with_attribute(AttributeType::Max("10".into()))
            .with_attribute(AttributeType::Value("5".into())),
    ));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let slider = one_with_class(&styled, SLIDER_CLASS);
    assert_eq!(a11y_value(&styled, slider).as_deref(), Some("5"));

    // Out of range: clamped, as HTML clamps a range's value.
    let clamped = lw.style_user_dom(page(
        input("range")
            .with_attribute(AttributeType::Max("10".into()))
            .with_attribute(AttributeType::Value("99".into())),
    ));
    let slider = one_with_class(&clamped, SLIDER_CLASS);
    assert_eq!(a11y_value(&clamped, slider).as_deref(), Some("10"));
}

#[test]
fn an_input_of_type_date_becomes_a_date_picker_on_its_value() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("date").with_attribute(AttributeType::Value("2024-03-15".into())),
    ));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let picker = one_with_class(&styled, DATE_PICKER_CLASS);
    assert_eq!(a11y_value(&styled, picker).as_deref(), Some("2024-03-15"));
}

#[test]
fn an_input_of_type_time_becomes_a_time_picker_on_its_value() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("time").with_attribute(AttributeType::Value("09:05".into())),
    ));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let picker = one_with_class(&styled, TIME_PICKER_CLASS);
    let text = text_under(&styled, picker);
    assert!(text.contains('9') && text.contains("05"), "hour and minute shown: {text:?}");
}

#[test]
fn an_input_of_type_button_becomes_a_button_labelled_by_its_value() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("button").with_attribute(AttributeType::Value("Go".into())),
    ));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let button = one_with_class(&styled, BUTTON_CLASS);
    assert!(matches!(node(&styled, button).get_node_type(), NodeType::Button));
    assert_eq!(text_under(&styled, button), "Go");
}

fn fruit_select(selected: usize) -> Dom {
    let mut select = Dom::create_select_no_a11y("fruit".into(), "Fruit".into());
    for (i, (value, label)) in [("a", "Apple"), ("b", "Banana"), ("c", "Cherry")]
        .into_iter()
        .enumerate()
    {
        let mut option = Dom::create_option_no_a11y(value.into(), label.into());
        if i == selected {
            option = option.with_attribute(AttributeType::Selected);
        }
        select = select.with_child(option);
    }
    select
}

#[test]
fn a_select_becomes_a_drop_down_showing_its_selected_option() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(fruit_select(1)));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let drop_down = one_with_class(&styled, DROP_DOWN_CLASS);
    assert_eq!(text_under(&styled, drop_down), "Banana");
    assert!(
        !all_nodes(&styled)
            .iter()
            .any(|id| matches!(node(&styled, *id).get_node_type(), NodeType::SelectOption)),
        "the options became the drop-down's choices, not nodes"
    );
}

#[test]
fn a_textarea_becomes_a_text_area_holding_its_text() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        Dom::create_textarea_no_a11y("notes".into(), "Notes".into()).with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("hello"),
        ),
    ));
    assert_eq!(raw_form_nodes(&styled), vec![]);
    let area = one_with_class(&styled, TEXT_AREA_CLASS);
    assert_eq!(text_under(&styled, area), "hello");
}

#[test]
fn a_text_input_backed_by_a_datalist_becomes_a_combobox() {
    let lw = styling_window();
    let datalist = Dom::create_datalist_no_a11y()
        .with_id("fruits".into())
        .with_child(Dom::create_option_no_a11y("Apple".into(), "Apple".into()))
        .with_child(Dom::create_option_no_a11y("Pear".into(), "Pear".into()));
    let styled = lw.style_user_dom(
        Dom::create_body()
            .with_child(input("text").with_attribute(attr("list", "fruits")))
            .with_child(datalist),
    );
    assert_eq!(raw_form_nodes(&styled), vec![]);
    one_with_class(&styled, COMBOBOX_CLASS);
    assert!(with_class(&styled, TEXT_INPUT_CONTAINER_CLASS).is_empty());
}

#[test]
fn every_visible_html_input_type_becomes_a_widget() {
    // Never a raw node that draws nothing; which widget each type becomes is
    // pinned per type in `dedicated_widgets` below.
    let lw = styling_window();
    for ty in [
        "text", "checkbox", "radio", "color", "file", "number", "range", "date", "time", "button",
        "password", "search", "email", "tel", "url", "month", "week", "datetime-local", "reset",
        "submit", "image",
    ] {
        let styled = lw.style_user_dom(page(input(ty)));
        assert_eq!(
            raw_form_nodes(&styled),
            vec![],
            "type={ty} left a raw node: {:?}",
            node_types(&styled)
        );
        assert!(
            styled.node_data.as_ref().len() > 2,
            "type={ty} became a widget, not an empty box"
        );
    }
}

// ── Attribute mapping ───────────────────────────────────────────────────────

#[test]
fn the_inputs_id_classes_and_inline_style_move_to_the_widget_root() {
    let lw = styling_window();
    let width = CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(200.0)));
    let styled = lw.style_user_dom(page(
        input("text")
            .with_id("email".into())
            .with_class("wide".into())
            .with_css_property(width.clone()),
    ));
    let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
    let root = node(&styled, field);
    assert!(root.has_id("email"), "the id names the widget now");
    assert!(root.has_class("wide"));
    let last_width = root
        .get_style()
        .rules
        .as_slice()
        .iter()
        .flat_map(|rule| rule.declarations.as_slice().iter())
        .filter_map(|decl| match decl {
            CssDeclaration::Static(p @ CssProperty::Width(_)) => Some(p.clone()),
            _ => None,
        })
        .last();
    assert_eq!(
        last_width,
        Some(width.property),
        "the app's inline width is the LAST width on the root, so it wins over the widget's"
    );
}

#[test]
fn constraint_and_form_attributes_stay_on_the_widget_root() {
    // Form validation, the soft keyboard's purpose and reset detection read
    // these off the node the user interacts with.
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("email")
            .with_attribute(AttributeType::Required)
            .with_attribute(AttributeType::Pattern(".+@.+".into()))
            .with_attribute(AttributeType::MinLength(3)),
    ));
    let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
    let attrs = node(&styled, field).attributes().as_slice().to_vec();
    for expected in [
        AttributeType::Required,
        AttributeType::Pattern(".+@.+".into()),
        AttributeType::MinLength(3),
        AttributeType::Name("field".into()),
        AttributeType::InputType("email".into()),
    ] {
        assert!(attrs.contains(&expected), "{expected:?} missing from {attrs:?}");
    }
}

#[test]
fn a_disabled_input_becomes_a_widget_nothing_can_activate_or_focus() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("checkbox").with_attribute(AttributeType::Disabled),
    ));
    let root = one_with_class(&styled, checkbox_container());
    assert!(node(&styled, root).attributes().as_slice().contains(&AttributeType::Disabled));
    for id in subtree(&styled, root) {
        let n = node(&styled, id);
        assert!(n.get_callbacks().as_slice().is_empty(), "node {id:?} still reacts");
        assert_eq!(n.get_tab_index(), None, "node {id:?} is still focusable");
    }
}

#[test]
fn a_readonly_text_input_is_not_editable() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("text")
            .with_attribute(AttributeType::Readonly)
            .with_attribute(AttributeType::Value("fixed".into())),
    ));
    let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
    assert!(!node(&styled, field).is_contenteditable());
    assert_eq!(text_under(&styled, field), "fixed");
    assert!(
        node(&styled, field).get_tab_index().is_some(),
        "readonly is still focusable (and selectable), unlike disabled"
    );
}

#[test]
fn tabindex_and_autofocus_reach_the_widget_root() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(
        input("range")
            .with_tab_index(TabIndex::OverrideInParent(3))
            .with_attribute(AttributeType::Autofocus),
    ));
    let slider = one_with_class(&styled, SLIDER_CLASS);
    assert_eq!(
        node(&styled, slider).get_tab_index(),
        Some(TabIndex::OverrideInParent(3))
    );
    assert!(node(&styled, slider).has_autofocus());
}

extern "C" fn app_click(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::RefreshDom
}

#[test]
fn the_apps_callbacks_on_the_input_fire_on_the_widget_root() {
    // Carried, not mapped: the root is the node the pointer and the focus
    // land on, so the app's handler fires for the same interactions it
    // would have on the raw input - AFTER the widget's own, which has by
    // then updated the widget.
    let lw = styling_window();
    let styled = lw.style_user_dom(page(input("checkbox").with_callback(
        EventFilter::Hover(HoverEventFilter::Click),
        RefAny::new(0u32),
        Callback::from_ptr(app_click).to_core(),
    )));
    let root = one_with_class(&styled, checkbox_container());
    let callbacks = node(&styled, root).get_callbacks().as_slice().to_vec();
    let app_cb = Callback::from_ptr(app_click).to_core().cb;
    let app_at = callbacks
        .iter()
        .position(|c| c.callback.cb == app_cb)
        .expect("the app's click handler is on the widget root");
    assert_eq!(
        callbacks[app_at].event,
        EventFilter::Hover(HoverEventFilter::Click)
    );
    assert!(app_at > 0, "the widget's own click handler runs first");
}

#[test]
fn an_input_marked_data_azul_widget_none_stays_a_raw_input() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(input("range").with_attribute(AttributeType::Data(
        AttributeNameValue {
            attr_name: "data-azul-widget".into(),
            value: "none".into(),
        },
    ))));
    assert_eq!(raw_form_nodes(&styled).len(), 1, "opted out: the raw node stays");
    assert!(with_class(&styled, SLIDER_CLASS).is_empty());
}

// ── XML ─────────────────────────────────────────────────────────────────────

#[test]
fn xml_form_controls_become_the_same_widgets() {
    let xml = r#"<html><body>
        <input type="range" min="0" max="10" value="5" />
        <input type="checkbox" checked="checked" />
        <select name="fruit">
            <option value="a">Apple</option>
            <optgroup label="Later">
                <option value="b" selected="selected">Banana</option>
            </optgroup>
        </select>
        <textarea rows="4">hi</textarea>
    </body></html>"#;
    let parsed = azul_layout::xml::parse_xml(xml).expect("parses");
    let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
    let lw = styling_window();
    let styled = lw.style_user_dom(dom);

    assert_eq!(raw_form_nodes(&styled), vec![], "{:?}", node_types(&styled));
    let slider = one_with_class(&styled, SLIDER_CLASS);
    assert_eq!(a11y_value(&styled, slider).as_deref(), Some("5"));
    let check = one_with_class(&styled, checkbox_container());
    assert_eq!(a11y_states(&styled, check), vec![AccessibilityState::CheckedTrue]);
    let drop_down = one_with_class(&styled, DROP_DOWN_CLASS);
    assert_eq!(
        text_under(&styled, drop_down),
        "Banana",
        "an option inside an optgroup is still a choice"
    );
    let area = one_with_class(&styled, TEXT_AREA_CLASS);
    assert_eq!(text_under(&styled, area), "hi");
}

// ── VirtualViews ────────────────────────────────────────────────────────────

extern "C" fn view_with_a_range(_data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let rect = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(200.0, 40.0));
    VirtualViewReturn::with_dom(
        Dom::create_div()
            .with_css("display: block;")
            .with_child(input("range")),
        rect,
        rect,
    )
}

#[test]
fn an_input_inside_a_virtual_view_becomes_a_widget() {
    // The other producer of an app DOM: the replacement is on the funnel,
    // not on the layout callback's path only (the icon lesson).
    let mut lw = text_window();
    let dom = Dom::create_body().with_child(
        Dom::create_virtual_view(RefAny::new(0u32), VirtualViewCallback::create(view_with_a_range))
            .with_css("width: 200px; height: 40px; overflow: hidden;"),
    );
    let styled = lw.style_user_dom(dom);
    lay_out(&mut lw, styled);
    let nested = lw
        .virtual_view_manager
        .get_nested_dom_id(DomId::ROOT_ID, NodeId::new(1))
        .expect("the view mounted a nested dom");
    let nested_styled = &lw
        .get_layout_result(&nested)
        .expect("nested dom laid out")
        .styled_dom;
    assert_eq!(raw_form_nodes(nested_styled), vec![]);
    one_with_class(nested_styled, SLIDER_CLASS);
}

// ── State across rebuilds ───────────────────────────────────────────────────

fn text_page() -> Dom {
    page(input("text").with_attribute(AttributeType::Value("hello".into())))
}

#[test]
fn typing_into_a_replaced_text_input_keeps_its_text_across_a_rebuild() {
    let mut lw = text_window();
    let styled = lw.style_user_dom(text_page());
    let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
    lay_out(&mut lw, styled);

    let (value_line, text_leaf) = {
        let lr = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
        let hierarchy = lr.styled_dom.node_hierarchy.as_container();
        let line = hierarchy[field].first_child_id(field).expect("the value line");
        let leaf = hierarchy[line].first_child_id(line).expect("its text");
        (line, leaf)
    };

    // A keystroke, the way the shell commits one.
    lw.focus_manager.set_focused_node(Some(dom_node(field)));
    lw.start_editing_at(
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        },
        DomId::ROOT_ID,
        text_leaf,
        0,
    );
    let _ = lw.record_text_input("X");
    let _ = lw.apply_text_changeset();

    // The app rebuilds from ITS model - which never heard of the keystroke.
    let rebuilt = lw.style_user_dom(text_page());
    lay_out(&mut lw, rebuilt);

    let content = lw.get_text_before_textinput(DomId::ROOT_ID, value_line);
    assert_eq!(
        lw.extract_text_from_inline_content(&content),
        "Xhello",
        "the typed text survives the app's rebuild"
    );
}

#[test]
fn a_clicked_checkbox_stays_checked_across_a_rebuild() {
    let mut lw = text_window();
    let styled = lw.style_user_dom(page(input("checkbox")));
    let root = one_with_class(&styled, checkbox_container());
    let state = node(&styled, root).get_callbacks().as_slice()[0].refany.clone();
    lay_out(&mut lw, styled);

    // The widget's OWN click handler, as the event loop would run it.
    let (update, _) = with_info(&lw, dom_node(root), |info| {
        azul_layout::widgets::check_box::input::default_on_checkbox_clicked(state, info)
    });
    assert_ne!(update, Update::RefreshDom, "a check needs no rebuild to show");

    // Any later rebuild (another control's RefreshDom) starts from the raw
    // input again - which still says "unchecked".
    let rebuilt = lw.style_user_dom(page(input("checkbox")));
    let root = one_with_class(&rebuilt, checkbox_container());
    assert_eq!(
        a11y_states(&rebuilt, root),
        vec![AccessibilityState::CheckedTrue],
        "the user's check survives the rebuild"
    );
}

/// The drop-down's own change hook, fired with `choice`.
fn pick(lw: &LayoutWindow, styled: &StyledDom, choice: usize) -> Update {
    let root = one_with_class(styled, DROP_DOWN_CLASS);
    let mut data = node(styled, root).get_callbacks().as_slice()[0].refany.clone();
    let hook = {
        let dd = data.downcast_ref::<DropDown>().expect("the drop-down's state");
        dd.on_choice_change
            .as_ref()
            .cloned()
            .expect("the replacement listens for the pick")
    };
    with_info(lw, dom_node(root), |info| {
        hook.callback.invoke(hook.refany.clone(), info, choice)
    })
    .0
}

#[test]
fn picking_a_select_option_asks_for_the_rebuild_that_shows_it() {
    let lw = styling_window();
    let styled = lw.style_user_dom(page(fruit_select(0)));
    let update = pick(&lw, &styled, 2);
    assert_eq!(
        update,
        Update::RefreshDom,
        "a drop-down only shows a choice by being rebuilt"
    );
    let rebuilt = lw.style_user_dom(page(fruit_select(0)));
    let drop_down = one_with_class(&rebuilt, DROP_DOWN_CLASS);
    assert_eq!(text_under(&rebuilt, drop_down), "Cherry");
}

#[test]
fn an_app_that_changes_the_default_overrides_the_remembered_choice() {
    // The HTML "dirty value" rule: the user's value stands until the APP
    // changes the default, then the app is the truth again.
    let lw = styling_window();
    let styled = lw.style_user_dom(page(fruit_select(0)));
    let _ = pick(&lw, &styled, 2);
    let rebuilt = lw.style_user_dom(page(fruit_select(1)));
    let drop_down = one_with_class(&rebuilt, DROP_DOWN_CLASS);
    assert_eq!(text_under(&rebuilt, drop_down), "Banana");
}

fn colour_radios(checked: &str) -> Dom {
    let radio = |value: &str| {
        let mut r = input("radio")
            .with_attribute(AttributeType::Name("colour".into()))
            .with_attribute(AttributeType::Value(value.into()));
        if value == checked {
            r = r.with_attribute(AttributeType::CheckedTrue);
        }
        r
    };
    Dom::create_body()
        .with_child(radio("red"))
        .with_child(radio("green"))
}

/// Per radio group root, whether its one radio is checked.
fn radio_checks(styled: &StyledDom) -> Vec<bool> {
    with_class(styled, RADIO_GROUP_CLASS)
        .into_iter()
        .map(|group| {
            subtree(styled, group).into_iter().any(|id| {
                node(styled, id)
                    .get_accessibility_info()
                    .is_some_and(|a| {
                        a.role == AccessibilityRole::RadioButton
                            && a.states.as_slice().contains(&AccessibilityState::CheckedTrue)
                    })
            })
        })
        .collect()
}

#[test]
fn checking_one_radio_unchecks_the_others_of_its_name_on_the_rebuild_it_asks_for() {
    let lw = styling_window();
    let styled = lw.style_user_dom(colour_radios("red"));
    assert_eq!(radio_checks(&styled), vec![true, false]);

    // The GREEN radio's change hook, as its row's click handler fires it.
    let green_group = with_class(&styled, RADIO_GROUP_CLASS)[1];
    let row = subtree(&styled, green_group)
        .into_iter()
        .find(|id| !node(&styled, *id).get_callbacks().as_slice().is_empty())
        .expect("the radio row listens");
    let mut data = node(&styled, row).get_callbacks().as_slice()[0].refany.clone();
    let hook = {
        let rg = data
            .downcast_ref::<RadioGroupStateWrapper>()
            .expect("the radio's state");
        rg.on_change.as_ref().cloned().expect("the replacement listens")
    };
    let (update, _) = with_info(&lw, dom_node(row), |info| {
        hook.callback
            .invoke(hook.refany.clone(), info, RadioGroupState { selected_index: 0 })
    });
    assert_eq!(update, Update::RefreshDom, "the other radios change on the rebuild");

    let rebuilt = lw.style_user_dom(colour_radios("red"));
    assert_eq!(radio_checks(&rebuilt), vec![false, true]);
}

#[test]
fn text_the_widget_reported_survives_a_rebuild_even_after_the_overlay_is_acked() {
    // An app that folds typing into its own model acks EVERY text revision
    // (`mark_text_revision_synced` is window-wide), which retires the
    // overlay entry of a raw input it knows nothing about. The replaced
    // widget reports its text through its own change hook, so the rebuild
    // still shows it.
    let lw = styling_window();
    let styled = lw.style_user_dom(text_page());
    let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
    let mut data = node(&styled, field)
        .get_dataset()
        .cloned()
        .expect("the text input's state");
    let hook = {
        let ti = data
            .downcast_ref::<TextInputStateWrapper>()
            .expect("the text input's state");
        ti.on_text_input.as_ref().cloned().expect("the replacement listens")
    };
    let mut typed = TextInputState::default();
    typed.text = "hello world".chars().map(|c| c as u32).collect::<Vec<_>>().into();
    let _ = with_info(&lw, dom_node(field), |info| {
        hook.callback.invoke(hook.refany.clone(), info, typed)
    });

    let rebuilt = lw.style_user_dom(text_page());
    let field = one_with_class(&rebuilt, TEXT_INPUT_CONTAINER_CLASS);
    assert_eq!(text_under(&rebuilt, field), "hello world");
}

// ── The dedicated widgets each type maps to ─────────────────────────────────

/// Every `<input type>` that has a widget of its own becomes THAT widget, not
/// the nearest general one: a password is masked, a week is an ISO week, a
/// submit button submits.
mod dedicated_widgets {
    use azul_layout::widgets::{
        date_picker::DatePickerState,
        datetime_local::{
            DateTimeLocalPickerState, DateTimeLocalPickerStateWrapper, DATETIME_LOCAL_CLASS,
        },
        drop_down::DropDownOptGroup,
        form::default_on_form_button_click,
        text_input::{TextInputKind, PASSWORD_MASK_CHAR, SEARCH_CLEAR_CLASS, SEARCH_FIELD_CLASS},
        time_picker::TimePickerState,
    };

    use super::*;

    /// The state the text input editing host `field` carries.
    fn text_state(styled: &StyledDom, field: NodeId) -> TextInputState {
        let mut ds = node(styled, field)
            .get_dataset()
            .cloned()
            .expect("a text input carries its state");
        let state = ds
            .downcast_ref::<TextInputStateWrapper>()
            .expect("the text input's state")
            .inner
            .clone();
        state
    }

    fn has_callback(styled: &StyledDom, id: NodeId, cb: usize) -> bool {
        node(styled, id)
            .get_callbacks()
            .as_slice()
            .iter()
            .any(|c| c.callback.cb == cb)
    }

    #[test]
    fn a_password_input_becomes_a_text_input_that_shows_one_bullet_per_character() {
        let lw = styling_window();
        let styled = lw.style_user_dom(page(
            input("password").with_attribute(AttributeType::Value("hunter2".into())),
        ));
        assert_eq!(raw_form_nodes(&styled), vec![], "{:?}", node_types(&styled));
        let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
        let bullets: String = core::iter::repeat(PASSWORD_MASK_CHAR).take(7).collect();
        assert_eq!(
            text_under(&styled, field),
            bullets,
            "the line shows the mask, never the value"
        );
        let state = text_state(&styled, field);
        assert_eq!(state.kind, TextInputKind::Password);
        assert_eq!(state.get_text(), "hunter2", "the state keeps the real value");
        assert!(a11y_states(&styled, field).contains(&AccessibilityState::Protected));
    }

    #[test]
    fn a_search_input_becomes_a_search_field_with_a_clear_button() {
        let lw = styling_window();
        let styled = lw.style_user_dom(page(
            input("search").with_attribute(AttributeType::Value("rust".into())),
        ));
        assert_eq!(raw_form_nodes(&styled), vec![]);
        let row = one_with_class(&styled, SEARCH_FIELD_CLASS);
        let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
        let clear = one_with_class(&styled, SEARCH_CLEAR_CLASS);
        assert!(is_under(&styled, field, row) && is_under(&styled, clear, row));
        assert_eq!(text_state(&styled, field).kind, TextInputKind::Search);
        assert_eq!(text_state(&styled, field).get_text(), "rust");
        // The row is the widget's ROOT: the input's identity lands there.
        let attrs = node(&styled, row).attributes().as_slice().to_vec();
        assert!(attrs.contains(&AttributeType::Name("field".into())), "{attrs:?}");
    }

    #[test]
    fn email_tel_and_url_inputs_become_text_inputs_of_their_kind() {
        let lw = styling_window();
        for (ty, kind) in [
            ("email", TextInputKind::Email),
            ("tel", TextInputKind::Tel),
            ("url", TextInputKind::Url),
            ("text", TextInputKind::Text),
            ("banana", TextInputKind::Text),
        ] {
            let styled = lw.style_user_dom(page(input(ty)));
            let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
            assert_eq!(text_state(&styled, field).kind, kind, "type={ty}");
        }
    }

    #[test]
    fn an_email_input_holding_a_malformed_address_is_invalid() {
        let lw = styling_window();
        let styled = lw.style_user_dom(page(
            input("email").with_attribute(AttributeType::Value("not-an-address".into())),
        ));
        let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
        assert!(!text_state(&styled, field).validity.is_valid());
    }

    #[test]
    fn a_pattern_attribute_constrains_the_replaced_text_input() {
        let lw = styling_window();
        let styled = lw.style_user_dom(page(
            input("text")
                .with_attribute(AttributeType::Pattern("[0-9]{3}".into()))
                .with_attribute(AttributeType::Value("12a".into())),
        ));
        let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
        let state = text_state(&styled, field);
        assert_eq!(
            state.pattern.as_ref().map(|p| p.as_str().to_string()).as_deref(),
            Some("[0-9]{3}")
        );
        assert!(!state.validity.is_valid(), "the app's value fails the pattern");
    }

    #[test]
    fn a_month_input_becomes_a_month_picker_on_its_value() {
        let lw = styling_window();
        let styled = lw.style_user_dom(page(
            input("month").with_attribute(AttributeType::Value("2024-03".into())),
        ));
        assert_eq!(raw_form_nodes(&styled), vec![]);
        let picker = one_with_class(&styled, DATE_PICKER_CLASS);
        assert_eq!(a11y_value(&styled, picker).as_deref(), Some("2024-03"));
    }

    #[test]
    fn a_week_input_becomes_an_iso_week_picker_on_its_value() {
        // 2021-01-01 is a Friday: ISO week 1 of 2021 starts on Monday the
        // 4th, and 2026-W01 starts on Monday 2025-12-29. A picker counting
        // weeks from January 1st lands in the wrong week for both.
        let lw = styling_window();
        for value in ["2024-W11", "2021-W01", "2020-W53", "2026-W01"] {
            let styled = lw.style_user_dom(page(
                input("week").with_attribute(AttributeType::Value(value.into())),
            ));
            assert_eq!(raw_form_nodes(&styled), vec![]);
            let picker = one_with_class(&styled, DATE_PICKER_CLASS);
            assert_eq!(a11y_value(&styled, picker).as_deref(), Some(value));
        }
    }

    fn datetime_page() -> Dom {
        page(input("datetime-local").with_attribute(AttributeType::Value("2024-03-15T10:30".into())))
    }

    #[test]
    fn a_datetime_local_input_becomes_a_date_and_time_picker_on_its_value() {
        let lw = styling_window();
        let styled = lw.style_user_dom(datetime_page());
        assert_eq!(raw_form_nodes(&styled), vec![]);
        let row = one_with_class(&styled, DATETIME_LOCAL_CLASS);
        assert_eq!(a11y_value(&styled, row).as_deref(), Some("2024-03-15T10:30"));
        assert_eq!(
            with_class(&styled, TIME_PICKER_CLASS).len(),
            1,
            "the time half is a time picker"
        );
    }

    #[test]
    fn a_datetime_local_pick_survives_the_apps_rebuild() {
        let lw = styling_window();
        let styled = lw.style_user_dom(datetime_page());
        let row = one_with_class(&styled, DATETIME_LOCAL_CLASS);
        let mut data = node(&styled, row)
            .get_dataset()
            .cloned()
            .expect("the row carries the combined state");
        let hook = {
            let w = data
                .downcast_ref::<DateTimeLocalPickerStateWrapper>()
                .expect("the combined state");
            w.on_change.as_ref().cloned().expect("the replacement listens")
        };
        let picked = DateTimeLocalPickerState {
            date: DatePickerState {
                year: 2025,
                month: 12,
                day: 24,
            },
            time: TimePickerState {
                hour: 18,
                minute: 45,
                is_pm: false,
                is_24h: true,
            },
        };
        let _ = with_info(&lw, dom_node(row), |info| {
            hook.callback.invoke(hook.refany.clone(), info, picked)
        });

        let rebuilt = lw.style_user_dom(datetime_page());
        let row = one_with_class(&rebuilt, DATETIME_LOCAL_CLASS);
        assert_eq!(a11y_value(&rebuilt, row).as_deref(), Some("2025-12-24T18:45"));
    }

    #[test]
    fn submit_and_reset_inputs_become_buttons_that_act_on_their_form() {
        let lw = styling_window();
        for (ty, label) in [("submit", "Submit"), ("reset", "Reset")] {
            let styled = lw.style_user_dom(page(input(ty)));
            assert_eq!(raw_form_nodes(&styled), vec![]);
            let button = one_with_class(&styled, BUTTON_CLASS);
            assert_eq!(text_under(&styled, button), label, "HTML's default label for type={ty}");
            assert!(
                has_callback(&styled, button, default_on_form_button_click as usize),
                "type={ty} must act on its form"
            );
            assert!(node(&styled, button)
                .attributes()
                .as_slice()
                .contains(&AttributeType::InputType(ty.into())));
        }
        let styled = lw.style_user_dom(page(
            input("submit").with_attribute(AttributeType::Value("Send".into())),
        ));
        let button = one_with_class(&styled, BUTTON_CLASS);
        assert_eq!(text_under(&styled, button), "Send", "the value is the label");
    }

    #[test]
    fn an_image_input_becomes_a_submit_button_named_by_its_alt_text() {
        let lw = styling_window();
        let styled = lw.style_user_dom(page(
            input("image")
                .with_attribute(AttributeType::Alt("Go".into()))
                .with_attribute(AttributeType::Src("go.png".into())),
        ));
        assert_eq!(raw_form_nodes(&styled), vec![]);
        let button = one_with_class(&styled, BUTTON_CLASS);
        assert!(
            has_callback(&styled, button, default_on_form_button_click as usize),
            "an image button submits its form"
        );
        let name = node(&styled, button)
            .get_accessibility_info()
            .and_then(|a| a.accessibility_name.as_ref().map(|n| n.as_str().to_string()));
        assert_eq!(name.as_deref(), Some("Go"), "named by its alt text");
        let attrs = node(&styled, button).attributes().as_slice().to_vec();
        assert!(attrs.contains(&AttributeType::InputType("image".into())), "{attrs:?}");
        assert!(
            !attrs.contains(&AttributeType::InputType("submit".into())),
            "one type, the app's: {attrs:?}"
        );
    }

    #[test]
    fn a_hidden_input_is_out_of_the_accessibility_tree_and_keeps_its_name_and_value() {
        let lw = styling_window();
        let styled = lw.style_user_dom(page(
            input("hidden").with_attribute(AttributeType::Value("t0k3n".into())),
        ));
        assert_eq!(raw_form_nodes(&styled), vec![]);
        // body > the hidden input's node
        let attrs = node(&styled, NodeId::new(1)).attributes().as_slice().to_vec();
        assert!(attrs.contains(&AttributeType::Hidden), "{attrs:?}");
        assert!(attrs.contains(&AttributeType::Name("field".into())));
        assert!(attrs.contains(&AttributeType::Value("t0k3n".into())));
    }

    #[test]
    fn an_optgroup_becomes_a_heading_of_the_drop_down_instead_of_being_flattened() {
        let lw = styling_window();
        let select = Dom::create_select_no_a11y("fruit".into(), "Fruit".into())
            .with_child(Dom::create_option_no_a11y("a".into(), "Apple".into()))
            .with_child(
                Dom::create_optgroup_no_a11y("Later".into())
                    .with_child(Dom::create_option_no_a11y("b".into(), "Banana".into()))
                    .with_child(Dom::create_option_no_a11y("c".into(), "Cherry".into())),
            )
            .with_child(Dom::create_option_no_a11y("d".into(), "Date".into()));
        let styled = lw.style_user_dom(page(select));
        assert_eq!(raw_form_nodes(&styled), vec![]);
        let root = one_with_class(&styled, DROP_DOWN_CLASS);
        let mut data = node(&styled, root).get_callbacks().as_slice()[0].refany.clone();
        let (choices, groups) = {
            let dd = data.downcast_ref::<DropDown>().expect("the drop-down's state");
            let choices: Vec<String> = dd
                .choices
                .as_ref()
                .iter()
                .map(|c| c.as_str().to_string())
                .collect();
            let groups: Vec<DropDownOptGroup> = dd.groups.as_ref().to_vec();
            (choices, groups)
        };
        assert_eq!(choices, ["Apple", "Banana", "Cherry", "Date"]);
        assert_eq!(
            groups,
            vec![DropDownOptGroup {
                label: "Later".into(),
                first_choice: 1,
                len: 2,
            }],
            "the group is a heading over its options, not a choice"
        );
    }
}

// ── <form> ──────────────────────────────────────────────────────────────────

/// A raw `<form>` - `Dom::create_form*` or XML - becomes a `Form`: its
/// controls are collected into `FormData`, Enter in a field and a submit
/// button submit it, a reset button resets it, and the app's own `Submit` /
/// `Reset` handlers on the raw form run exactly when a `Form`'s `on_submit` /
/// `on_reset` would - once each.
mod forms {
    use std::collections::HashMap;

    use azul_core::window::VirtualKeyCode;
    use azul_layout::{
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        widgets::form::{
            collect_form_data, default_on_form_button_click, default_on_form_submit_event,
            reset_form, submit_form, Form, FormData, FormOnSubmitCallbackType, FormStateWrapper,
        },
        window::DomLayoutResult,
    };

    use super::*;

    /// Make `styled` the window's root DOM the way a layout pass leaves it,
    /// without laying it out: the form handlers only walk the node tree and
    /// read datasets and attributes.
    pub(super) fn mount(lw: &mut LayoutWindow, styled: StyledDom) {
        lw.layout_results.insert(
            DomId::ROOT_ID,
            DomLayoutResult {
                styled_dom: styled,
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
                display_list: Arc::new(DisplayList::default()),
                scroll_ids: HashMap::new(),
                scroll_id_to_node_id: HashMap::new(),
            },
        );
    }

    /// `<input type=ty name=name>`.
    pub(super) fn named(ty: &str, name: &str) -> Dom {
        Dom::create_input_no_a11y(ty.into(), name.into(), name.into())
    }

    pub(super) fn pairs(data: &FormData) -> Vec<(String, String)> {
        data.entries
            .as_ref()
            .iter()
            .map(|e| (e.name.as_str().to_string(), e.value.as_str().to_string()))
            .collect()
    }

    pub(super) fn owned(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        expected
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect()
    }

    /// The one form node of `styled`.
    pub(super) fn the_form(styled: &StyledDom) -> NodeId {
        let forms: Vec<NodeId> = all_nodes(styled)
            .into_iter()
            .filter(|id| matches!(node(styled, *id).get_node_type(), NodeType::Form))
            .collect();
        assert_eq!(forms.len(), 1, "one form: {:?}", node_types(styled));
        forms[0]
    }

    /// The initial values the form node recorded at build, if it is a `Form`.
    pub(super) fn initial_values(styled: &StyledDom, form: NodeId) -> Option<Vec<(String, String)>> {
        let mut ds = node(styled, form).get_dataset().cloned()?;
        let initial = ds
            .downcast_ref::<FormStateWrapper>()
            .map(|state| pairs(&state.initial));
        initial
    }

    /// What the app's raw-form handlers saw.
    #[derive(Default)]
    pub(super) struct Calls {
        pub(super) submits: Vec<Vec<(String, String)>>,
        pub(super) resets: usize,
    }

    /// The app's `Submit` handler on a RAW form: a plain callback. It reads
    /// the values from whichever node the event hit (the form, a button, a
    /// field) - `collect_form_data` finds the enclosing form.
    pub(super) extern "C" fn app_on_submit(mut data: RefAny, mut info: CallbackInfo) -> Update {
        let hit = info.get_hit_node();
        let values = collect_form_data(&mut info, hit)
            .map(|d| pairs(&d))
            .unwrap_or_default();
        if let Some(mut calls) = data.downcast_mut::<Calls>() {
            calls.submits.push(values);
        }
        Update::RefreshDom
    }

    pub(super) extern "C" fn app_on_reset(mut data: RefAny, _info: CallbackInfo) -> Update {
        if let Some(mut calls) = data.downcast_mut::<Calls>() {
            calls.resets += 1;
        }
        Update::DoNothing
    }

    pub(super) fn submits(calls: &RefAny) -> Vec<Vec<(String, String)>> {
        let mut calls = calls.clone();
        let submits = calls.downcast_ref::<Calls>().expect("calls").submits.clone();
        submits
    }

    pub(super) fn resets(calls: &RefAny) -> usize {
        let mut calls = calls.clone();
        let resets = calls.downcast_ref::<Calls>().expect("calls").resets;
        resets
    }

    /// `<form id=signup>` around `children`, with the app's own Submit and
    /// Reset handlers on it.
    pub(super) fn raw_form(calls: &RefAny, children: Vec<Dom>) -> Dom {
        let mut form = Dom::create_form_no_a11y()
            .with_id("signup".into())
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Submit),
                calls.clone(),
                Callback::from_ptr(app_on_submit).to_core(),
            )
            .with_callback(
                EventFilter::Hover(HoverEventFilter::Reset),
                calls.clone(),
                Callback::from_ptr(app_on_reset).to_core(),
            );
        for child in children {
            form = form.with_child(child);
        }
        form
    }

    fn signup(calls: &RefAny) -> Dom {
        page(raw_form(
            calls,
            vec![
                named("text", "user").with_attribute(AttributeType::Value("ann".into())),
                named("submit", "go"),
                named("reset", "clear"),
            ],
        ))
    }

    /// The node carrying a callback to `cb`, and that callback's payload.
    fn with_callback_to(styled: &StyledDom, cb: usize) -> (NodeId, RefAny) {
        all_nodes(styled)
            .into_iter()
            .find_map(|id| {
                node(styled, id)
                    .get_callbacks()
                    .as_slice()
                    .iter()
                    .find(|c| c.callback.cb == cb)
                    .map(|c| (id, c.refany.clone()))
            })
            .expect("no node carries that callback")
    }

    #[test]
    fn a_raw_form_becomes_a_form_that_records_its_initial_values() {
        let calls = RefAny::new(Calls::default());
        let lw = styling_window();
        let styled = lw.style_user_dom(signup(&calls));
        let form = the_form(&styled);
        assert_eq!(
            initial_values(&styled, form),
            Some(owned(&[("user", "ann")])),
            "the form node carries the form state (buttons submit no value)"
        );
        assert!(node(&styled, form).has_id("signup"), "the form keeps its identity");
    }

    #[test]
    fn submitting_a_raw_form_runs_the_apps_submit_handler_once_with_the_values() {
        let calls = RefAny::new(Calls::default());
        let mut lw = styling_window();
        let styled = lw.style_user_dom(signup(&calls));
        let form = the_form(&styled);
        mount(&mut lw, styled);
        let (update, _) = with_info(&lw, dom_node(form), |mut info| {
            submit_form(&mut info, dom_node(form))
        });
        assert_eq!(update, Update::RefreshDom, "the app's verdict");
        assert_eq!(submits(&calls), vec![owned(&[("user", "ann")])]);
    }

    #[test]
    fn the_engines_submit_event_reaches_the_apps_handler_exactly_once() {
        // The engine dispatches `Submit` to EVERY Submit handler on the form
        // node. The app's handler runs as the form's `on_submit`, so it must
        // not also stay on the node, or it would run twice.
        let calls = RefAny::new(Calls::default());
        let mut lw = styling_window();
        let styled = lw.style_user_dom(signup(&calls));
        let form = the_form(&styled);
        let submit_handlers: Vec<_> = node(&styled, form)
            .get_callbacks()
            .as_slice()
            .iter()
            .filter(|c| c.event == EventFilter::Hover(HoverEventFilter::Submit))
            .cloned()
            .collect();
        assert_eq!(submit_handlers.len(), 1, "only the form's own Submit handler");
        assert_eq!(
            submit_handlers[0].callback.cb,
            default_on_form_submit_event as usize
        );
        mount(&mut lw, styled);
        let payload = submit_handlers[0].refany.clone();
        let _ = with_info(&lw, dom_node(form), |info| {
            default_on_form_submit_event(payload, info)
        });
        assert_eq!(submits(&calls).len(), 1);
    }

    #[test]
    fn a_submit_button_in_a_raw_form_submits_it() {
        let calls = RefAny::new(Calls::default());
        let mut lw = styling_window();
        let styled = lw.style_user_dom(signup(&calls));
        let (button, payload) = with_callback_to(&styled, default_on_form_button_click as usize);
        mount(&mut lw, styled);
        let _ = with_info(&lw, dom_node(button), |info| {
            default_on_form_button_click(payload, info)
        });
        assert_eq!(
            submits(&calls),
            vec![owned(&[("user", "ann")])],
            "values read from the BUTTON's event"
        );
    }

    #[test]
    fn enter_in_a_text_field_of_a_raw_form_submits_it() {
        let calls = RefAny::new(Calls::default());
        let mut lw = styling_window();
        let styled = lw.style_user_dom(signup(&calls));
        let field = one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
        let state = node(&styled, field)
            .get_dataset()
            .cloned()
            .expect("the field's state");
        mount(&mut lw, styled);
        lw.current_window_state.keyboard_state.current_virtual_keycode =
            Some(VirtualKeyCode::Return).into();
        let _ = with_info(&lw, dom_node(field), |info| {
            azul_layout::widgets::text_input::default_on_virtual_key_down(state, info)
        });
        assert_eq!(submits(&calls).len(), 1, "HTML's implicit submission");
    }

    #[test]
    fn resetting_a_raw_form_runs_the_apps_reset_handler_once() {
        let calls = RefAny::new(Calls::default());
        let mut lw = styling_window();
        let styled = lw.style_user_dom(signup(&calls));
        let form = the_form(&styled);
        mount(&mut lw, styled);
        let _ = with_info(&lw, dom_node(form), |mut info| {
            reset_form(&mut info, dom_node(form))
        });
        assert_eq!(resets(&calls), 1);
        assert!(submits(&calls).is_empty());
    }

    #[test]
    fn an_xml_form_becomes_a_form() {
        let xml = r#"<html><body>
            <form id="search">
                <input name="q" value="rust" />
                <input type="submit" value="Go" />
            </form>
        </body></html>"#;
        let parsed = azul_layout::xml::parse_xml(xml).expect("parses");
        let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
        let lw = styling_window();
        let styled = lw.style_user_dom(dom);
        let form = the_form(&styled);
        assert_eq!(initial_values(&styled, form), Some(owned(&[("q", "rust")])));
    }

    extern "C" fn app_form_data(mut data: RefAny, _info: CallbackInfo, form_data: FormData) -> Update {
        if let Some(mut calls) = data.downcast_mut::<Calls>() {
            calls.submits.push(pairs(&form_data));
        }
        Update::DoNothing
    }

    #[test]
    fn a_form_the_app_built_is_left_as_it_is() {
        // A `Form` widget renders a form node too; it is not a raw form, and
        // replacing it would drop the app's `on_submit`.
        let calls = RefAny::new(Calls::default());
        let built = Form::create(vec![named("text", "user")
            .with_attribute(AttributeType::Value("ann".into()))]
        .into())
        .with_on_submit(calls.clone(), app_form_data as FormOnSubmitCallbackType)
        .dom();
        let mut lw = styling_window();
        let styled = lw.style_user_dom(page(built));
        let form = the_form(&styled);
        mount(&mut lw, styled);
        let _ = with_info(&lw, dom_node(form), |mut info| {
            submit_form(&mut info, dom_node(form))
        });
        assert_eq!(submits(&calls), vec![owned(&[("user", "ann")])]);
    }
}

// ── Form reset vs the memory of replaced controls ───────────────────────────

/// The memory hands a replaced control the user's value on every rebuild -
/// which must not UNDO a form reset: the reset puts every control of its
/// form back to its default, for good.
mod form_reset {
    use azul_layout::widgets::{
        check_box::{CheckBoxState, CheckBoxStateWrapper},
        form::reset_form,
    };

    use super::{
        forms::{mount, named, the_form},
        *,
    };

    fn profile() -> Dom {
        page(
            Dom::create_form_no_a11y()
                .with_child(named("text", "user").with_attribute(AttributeType::Value("ann".into())))
                .with_child(named("checkbox", "news")),
        )
    }

    /// The user types `text` into the replaced text input `field`: the
    /// widget reports it through its change hook, as its edit handler does.
    fn type_into(lw: &LayoutWindow, styled: &StyledDom, field: NodeId, text: &str) {
        let mut data = node(styled, field)
            .get_dataset()
            .cloned()
            .expect("the text input's state");
        let hook = {
            let ti = data
                .downcast_ref::<TextInputStateWrapper>()
                .expect("the text input's state");
            ti.on_text_input.as_ref().cloned().expect("the replacement listens")
        };
        let mut typed = TextInputState::default();
        typed.text = text.chars().map(|c| c as u32).collect::<Vec<_>>().into();
        let _ = with_info(lw, dom_node(field), |info| {
            hook.callback.invoke(hook.refany.clone(), info, typed)
        });
    }

    /// The user checks the replaced checkbox: its toggle hook fires, as its
    /// click handler does.
    fn check(lw: &LayoutWindow, styled: &StyledDom) {
        let root = one_with_class(styled, checkbox_container());
        let mut data = node(styled, root).get_callbacks().as_slice()[0].refany.clone();
        let hook = {
            let cb = data
                .downcast_ref::<CheckBoxStateWrapper>()
                .expect("the check box's state");
            cb.on_toggle.as_ref().cloned().expect("the replacement listens")
        };
        let _ = with_info(lw, dom_node(root), |info| {
            hook.callback
                .invoke(hook.refany.clone(), info, CheckBoxState { checked: true })
        });
    }

    /// Reset the form node `form` of `styled`, as a reset button would.
    fn reset(lw: &mut LayoutWindow, styled: StyledDom, form: NodeId) -> Update {
        mount(lw, styled);
        with_info(lw, dom_node(form), |mut info| {
            reset_form(&mut info, dom_node(form))
        })
        .0
    }

    fn field_text(styled: &StyledDom, nth: usize) -> String {
        text_under(styled, with_class(styled, TEXT_INPUT_CONTAINER_CLASS)[nth])
    }

    #[test]
    fn a_form_reset_puts_a_replaced_text_input_back_to_its_default_for_good() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(profile());
        type_into(&lw, &styled, one_with_class(&styled, TEXT_INPUT_CONTAINER_CLASS), "bob");
        // Without a reset, the typed text survives the app's rebuild.
        let rebuilt = lw.style_user_dom(profile());
        assert_eq!(field_text(&rebuilt, 0), "bob");

        let form = the_form(&rebuilt);
        let update = reset(&mut lw, rebuilt, form);
        assert_eq!(update, Update::RefreshDom, "the rebuild that shows the defaults");
        let after = lw.style_user_dom(profile());
        assert_eq!(
            field_text(&after, 0),
            "ann",
            "the memory must not undo the reset on the next rebuild"
        );
    }

    #[test]
    fn a_form_reset_unchecks_a_checkbox_the_user_checked() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(profile());
        check(&lw, &styled);
        let rebuilt = lw.style_user_dom(profile());
        let root = one_with_class(&rebuilt, checkbox_container());
        assert_eq!(a11y_states(&rebuilt, root), vec![AccessibilityState::CheckedTrue]);

        let form = the_form(&rebuilt);
        let update = reset(&mut lw, rebuilt, form);
        assert_eq!(
            update,
            Update::RefreshDom,
            "a replaced checkbox shows its default only by being rebuilt"
        );
        let after = lw.style_user_dom(profile());
        let root = one_with_class(&after, checkbox_container());
        assert_eq!(a11y_states(&after, root), vec![AccessibilityState::CheckedFalse]);
    }

    #[test]
    fn a_reset_forgets_only_the_controls_of_its_own_form() {
        let two_forms = || {
            Dom::create_body()
                .with_child(Dom::create_form_no_a11y().with_child(
                    named("text", "a").with_attribute(AttributeType::Value("first".into())),
                ))
                .with_child(Dom::create_form_no_a11y().with_child(
                    named("text", "b").with_attribute(AttributeType::Value("second".into())),
                ))
        };
        let mut lw = styling_window();
        let styled = lw.style_user_dom(two_forms());
        let fields = with_class(&styled, TEXT_INPUT_CONTAINER_CLASS);
        type_into(&lw, &styled, fields[0], "typed a");
        type_into(&lw, &styled, fields[1], "typed b");
        let rebuilt = lw.style_user_dom(two_forms());
        let first_form = all_nodes(&rebuilt)
            .into_iter()
            .find(|id| matches!(node(&rebuilt, *id).get_node_type(), NodeType::Form))
            .expect("a form");

        let _ = reset(&mut lw, rebuilt, first_form);
        let after = lw.style_user_dom(two_forms());
        assert_eq!(field_text(&after, 0), "first", "the reset form is back at its default");
        assert_eq!(field_text(&after, 1), "typed b", "the other form keeps the user's text");
    }

    #[test]
    fn resetting_an_untouched_form_asks_for_no_rebuild() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(profile());
        let form = the_form(&styled);
        assert_eq!(reset(&mut lw, styled, form), Update::DoNothing);
    }
}

// ── FormData of replaced controls ───────────────────────────────────────────

/// A replaced checkbox, radio, slider, colour, file, time or drop-down keeps
/// no state a form can read on its root - but the replacement knows each
/// one's name and value, so a form's `FormData` holds them the way HTML's
/// does: a checkbox's `value` ("on" by default) only while checked, a radio
/// group's checked value once, a select's chosen OPTION VALUE, the others'
/// value strings. The form's initial values follow the same rules.
mod form_data {
    use azul_layout::widgets::{
        check_box::{CheckBoxState, CheckBoxStateWrapper},
        form::{collect_form_data, reset_form},
        slider::{SliderState, SliderStateWrapper},
    };

    use super::{
        forms::{initial_values, mount, named, owned, pairs, the_form},
        *,
    };

    fn form_page(children: Vec<Dom>) -> Dom {
        let mut form = Dom::create_form_no_a11y();
        for child in children {
            form = form.with_child(child);
        }
        page(form)
    }

    /// What the form of `styled` would submit now.
    fn collect(lw: &mut LayoutWindow, styled: &StyledDom) -> Vec<(String, String)> {
        let form = the_form(styled);
        mount(lw, styled.clone());
        let data = with_info(lw, dom_node(form), |mut info| {
            collect_form_data(&mut info, dom_node(form))
        })
        .0
        .expect("the form node is in a form");
        pairs(&data)
    }

    /// The user checks the `nth` replaced checkbox (its toggle hook fires,
    /// as its click handler does).
    fn check_nth(lw: &LayoutWindow, styled: &StyledDom, nth: usize) {
        let root = with_class(styled, checkbox_container())[nth];
        let mut data = node(styled, root).get_callbacks().as_slice()[0].refany.clone();
        let hook = {
            let cb = data
                .downcast_ref::<CheckBoxStateWrapper>()
                .expect("the check box's state");
            cb.on_toggle.as_ref().cloned().expect("the replacement listens")
        };
        let _ = with_info(lw, dom_node(root), |info| {
            hook.callback
                .invoke(hook.refany.clone(), info, CheckBoxState { checked: true })
        });
    }

    fn checkboxes() -> Dom {
        form_page(vec![
            named("checkbox", "terms").with_attribute(AttributeType::Value("yes".into())),
            named("checkbox", "news").with_attribute(AttributeType::CheckedTrue),
            named("checkbox", "spam"),
        ])
    }

    #[test]
    fn a_checkbox_submits_its_value_only_while_checked() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(checkboxes());
        let expected = owned(&[("news", "on")]);
        assert_eq!(
            initial_values(&styled, the_form(&styled)),
            Some(expected.clone()),
            "unchecked boxes are no part of the form; a checked one without a value says \"on\""
        );
        assert_eq!(collect(&mut lw, &styled), expected);

        check_nth(&lw, &styled, 0);
        assert_eq!(
            collect(&mut lw, &styled),
            owned(&[("terms", "yes"), ("news", "on")]),
            "the user's check counts at once, before any rebuild"
        );
    }

    #[test]
    fn right_after_a_reset_the_form_reads_the_defaults() {
        // A carried reset handler reads the form before the rebuild the
        // reset asks for.
        let mut lw = styling_window();
        let styled = lw.style_user_dom(checkboxes());
        check_nth(&lw, &styled, 0);
        let form = the_form(&styled);
        mount(&mut lw, styled.clone());
        let _ = with_info(&lw, dom_node(form), |mut info| {
            reset_form(&mut info, dom_node(form))
        });
        assert_eq!(collect(&mut lw, &styled), owned(&[("news", "on")]));
    }

    fn colours() -> Dom {
        let radio = |value: &str| {
            named("radio", "colour").with_attribute(AttributeType::Value(value.into()))
        };
        form_page(vec![
            radio("red").with_attribute(AttributeType::CheckedTrue),
            radio("green"),
        ])
    }

    #[test]
    fn a_radio_group_submits_its_checked_radios_value_once() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(colours());
        assert_eq!(
            initial_values(&styled, the_form(&styled)),
            Some(owned(&[("colour", "red")]))
        );
        assert_eq!(collect(&mut lw, &styled), owned(&[("colour", "red")]));

        // The GREEN radio's change hook, as its row's click handler fires it.
        let green = with_class(&styled, RADIO_GROUP_CLASS)[1];
        let row = subtree(&styled, green)
            .into_iter()
            .find(|id| !node(&styled, *id).get_callbacks().as_slice().is_empty())
            .expect("the radio row listens");
        let mut data = node(&styled, row).get_callbacks().as_slice()[0].refany.clone();
        let hook = {
            let rg = data
                .downcast_ref::<RadioGroupStateWrapper>()
                .expect("the radio's state");
            rg.on_change.as_ref().cloned().expect("the replacement listens")
        };
        let _ = with_info(&lw, dom_node(row), |info| {
            hook.callback
                .invoke(hook.refany.clone(), info, RadioGroupState { selected_index: 0 })
        });
        assert_eq!(collect(&mut lw, &styled), owned(&[("colour", "green")]));
    }

    fn valued(ty: &str, name: &str, value: &str) -> Dom {
        named(ty, name).with_attribute(AttributeType::Value(value.into()))
    }

    fn value_strings() -> Dom {
        form_page(vec![
            valued("range", "vol", "5")
                .with_attribute(AttributeType::Min("0".into()))
                .with_attribute(AttributeType::Max("10".into())),
            valued("color", "tint", "#ff0000"),
            valued("number", "qty", "42"),
            valued("date", "day", "2024-03-15"),
            valued("time", "at", "09:05"),
            valued("month", "mo", "2024-03"),
            valued("week", "wk", "2021-W01"),
            valued("datetime-local", "when", "2024-03-15T10:30"),
            named("file", "doc"),
        ])
    }

    #[test]
    fn range_colour_number_date_time_and_file_inputs_submit_their_value_strings() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(value_strings());
        let expected = owned(&[
            ("vol", "5"),
            ("tint", "#ff0000"),
            ("qty", "42"),
            ("day", "2024-03-15"),
            ("at", "09:05"),
            ("mo", "2024-03"),
            ("wk", "2021-W01"),
            ("when", "2024-03-15T10:30"),
            // HTML: a file input with nothing picked is an empty file.
            ("doc", ""),
        ]);
        assert_eq!(initial_values(&styled, the_form(&styled)), Some(expected.clone()));
        assert_eq!(collect(&mut lw, &styled), expected);
    }

    #[test]
    fn a_moved_slider_submits_its_new_value() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(value_strings());
        let slider = one_with_class(&styled, SLIDER_CLASS);
        let mut data = node(&styled, slider)
            .get_dataset()
            .cloned()
            .expect("the slider's state");
        let hook = {
            let s = data
                .downcast_ref::<SliderStateWrapper>()
                .expect("the slider's state");
            s.on_value_change.as_ref().cloned().expect("the replacement listens")
        };
        let moved = SliderState {
            value: 7.0,
            min: 0.0,
            max: 10.0,
        };
        let _ = with_info(&lw, dom_node(slider), |info| {
            hook.callback.invoke(hook.refany.clone(), info, moved)
        });
        let got = collect(&mut lw, &styled);
        assert_eq!(got[0], ("vol".to_string(), "7".to_string()), "{got:?}");
    }

    #[test]
    fn a_select_submits_the_chosen_options_value_not_its_label() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(form_page(vec![fruit_select(1)]));
        assert_eq!(
            initial_values(&styled, the_form(&styled)),
            Some(owned(&[("fruit", "b")]))
        );
        assert_eq!(collect(&mut lw, &styled), owned(&[("fruit", "b")]));
        let _ = pick(&lw, &styled, 2);
        assert_eq!(collect(&mut lw, &styled), owned(&[("fruit", "c")]));
    }

    #[test]
    fn a_textarea_submits_its_text() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(form_page(vec![Dom::create_textarea_no_a11y(
            "notes".into(),
            "Notes".into(),
        )
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("hello"))]));
        assert_eq!(
            initial_values(&styled, the_form(&styled)),
            Some(owned(&[("notes", "hello")]))
        );
        assert_eq!(collect(&mut lw, &styled), owned(&[("notes", "hello")]));
    }

    #[test]
    fn disabled_and_unnamed_controls_are_not_submitted() {
        let mut lw = styling_window();
        let styled = lw.style_user_dom(form_page(vec![
            valued("text", "off", "x").with_attribute(AttributeType::Disabled),
            Dom::create_from_data(NodeData::create_node(NodeType::Input))
                .with_attribute(AttributeType::Value("anonymous".into())),
            valued("text", "on", "y"),
        ]));
        assert_eq!(
            initial_values(&styled, the_form(&styled)),
            Some(owned(&[("on", "y")]))
        );
        assert_eq!(collect(&mut lw, &styled), owned(&[("on", "y")]));
    }
}

// ── <datalist> ──────────────────────────────────────────────────────────────

/// A `<datalist>` is suggestions for an input, never content: HTML's
/// user-agent sheet says `datalist { display: none }`. It stays in the tree
/// (the combobox reads it; an app may too) but takes no space - unless the
/// app's own style shows it.
mod datalist {
    use azul_css::props::layout::LayoutDisplay;

    use super::*;

    fn fruits() -> Dom {
        Dom::create_datalist_no_a11y()
            .with_id("fruits".into())
            .with_child(Dom::create_option_no_a11y("Apple".into(), "Apple".into()))
            .with_child(Dom::create_option_no_a11y("Pear".into(), "Pear".into()))
    }

    fn the_datalist(styled: &StyledDom) -> NodeId {
        all_nodes(styled)
            .into_iter()
            .find(|id| matches!(node(styled, *id).get_node_type(), NodeType::DataList))
            .expect("the datalist stays in the tree")
    }

    /// The display the node's inline style ends on (last match wins).
    fn inline_display(styled: &StyledDom, id: NodeId) -> Option<LayoutDisplay> {
        node(styled, id)
            .get_style()
            .rules
            .as_slice()
            .iter()
            .flat_map(|rule| rule.declarations.as_slice().iter())
            .filter_map(|decl| match decl {
                CssDeclaration::Static(CssProperty::Display(value)) => {
                    value.get_property().cloned()
                }
                _ => None,
            })
            .last()
    }

    #[test]
    fn a_datalist_next_to_its_input_is_not_displayed() {
        let lw = styling_window();
        let styled = lw.style_user_dom(
            Dom::create_body()
                .with_child(input("text").with_attribute(attr("list", "fruits")))
                .with_child(fruits()),
        );
        let list = the_datalist(&styled);
        assert_eq!(inline_display(&styled, list), Some(LayoutDisplay::None));
    }

    #[test]
    fn a_datalist_in_a_page_without_controls_is_not_displayed_either() {
        let lw = styling_window();
        let styled = lw.style_user_dom(Dom::create_body().with_child(fruits()));
        let list = the_datalist(&styled);
        assert_eq!(inline_display(&styled, list), Some(LayoutDisplay::None));
    }

    #[test]
    fn an_app_that_shows_its_datalist_still_can() {
        let lw = styling_window();
        let shown = fruits().with_css_property(CssPropertyWithConditions::simple(
            CssProperty::const_display(LayoutDisplay::Block),
        ));
        let styled = lw.style_user_dom(Dom::create_body().with_child(shown));
        let list = the_datalist(&styled);
        assert_eq!(
            inline_display(&styled, list),
            Some(LayoutDisplay::Block),
            "the user-agent default comes BEFORE the app's own style"
        );
    }
}

// ── An XML document mounted in a window ─────────────────────────────────────

/// The E2E `mount` op installs an XML document instead of the app's DOM
/// (`LayoutWindow::style_xml_document`). Its raw controls are replaced like
/// every app DOM's - with the WINDOW's memory: that is where a form reads a
/// replaced checkbox's or slider's value, where a reset forgets the user's,
/// and what a re-mount (a theme switch re-mounts the document) restores the
/// user's values from. A throw-away memory lost all three.
mod xml_mount {
    use azul_core::icon::{IconProviderHandle, SharedIconProvider};
    use azul_layout::widgets::{
        check_box::{CheckBoxState, CheckBoxStateWrapper},
        form::collect_form_data,
    };

    use super::{
        forms::{mount, owned, pairs, the_form},
        *,
    };

    const DOCUMENT: &str = r#"<html><body><form>
        <input type="checkbox" name="news" />
        <input type="range" name="vol" min="0" max="10" value="4" />
    </form></body></html>"#;

    fn mounted(lw: &LayoutWindow) -> StyledDom {
        let provider = SharedIconProvider::from_handle(IconProviderHandle::new());
        lw.style_xml_document(DOCUMENT, &provider, &SystemStyle::default())
            .expect("the document parses")
    }

    /// The user checks the replaced checkbox: its toggle hook fires, as its
    /// click handler does.
    fn check(lw: &LayoutWindow, styled: &StyledDom) {
        let root = one_with_class(styled, checkbox_container());
        let mut data = node(styled, root).get_callbacks().as_slice()[0].refany.clone();
        let hook = {
            let cb = data
                .downcast_ref::<CheckBoxStateWrapper>()
                .expect("the check box's state");
            cb.on_toggle.as_ref().cloned().expect("the replacement listens")
        };
        let _ = with_info(lw, dom_node(root), |info| {
            hook.callback
                .invoke(hook.refany.clone(), info, CheckBoxState { checked: true })
        });
    }

    #[test]
    fn a_mounted_documents_controls_keep_their_values_in_the_windows_memory() {
        let mut lw = styling_window();
        let styled = mounted(&lw);
        assert_eq!(raw_form_nodes(&styled), vec![], "{:?}", node_types(&styled));
        check(&lw, &styled);

        let form = the_form(&styled);
        mount(&mut lw, styled);
        let data = with_info(&lw, dom_node(form), |mut info| {
            collect_form_data(&mut info, dom_node(form))
        })
        .0
        .expect("the form node is in a form");
        assert_eq!(
            pairs(&data),
            owned(&[("news", "on"), ("vol", "4")]),
            "the form reads the replaced controls from the window's memory"
        );

        // The document is mounted again: the user's check survives it.
        let again = mounted(&lw);
        let root = one_with_class(&again, checkbox_container());
        assert_eq!(a11y_states(&again, root), vec![AccessibilityState::CheckedTrue]);
    }
}

// ── <button> ────────────────────────────────────────────────────────────────

/// A raw `<button>` (`Dom::create_button*`, XML) becomes the Button widget,
/// as `<input type=submit|reset|button>` does: its `type` decides what it
/// does to its form - SUBMIT when it has none (HTML's default for a
/// `<button>`) - and its content is its label. A Button widget's own root is
/// a `<button>` node too, and is never replaced.
mod raw_buttons {
    use azul_layout::widgets::{button::ButtonFormAction, form::default_on_form_button_click};

    use super::{
        forms::{mount, named, owned, raw_form, submits, Calls},
        *,
    };

    /// The nodes the replacement turned into a Button widget.
    fn replaced_buttons(styled: &StyledDom) -> Vec<NodeId> {
        all_nodes(styled)
            .into_iter()
            .filter(|id| {
                node(styled, *id).attributes().as_slice().iter().any(|a| {
                    a.name() == "data-azul-form-control" && a.value().as_str() == "button"
                })
            })
            .collect()
    }

    /// Every node carrying the form-button click handler, with its action
    /// and the handler's payload.
    fn form_buttons(styled: &StyledDom) -> Vec<(NodeId, ButtonFormAction, RefAny)> {
        all_nodes(styled)
            .into_iter()
            .filter_map(|id| {
                let handler = node(styled, id)
                    .get_callbacks()
                    .as_slice()
                    .iter()
                    .find(|c| c.callback.cb == default_on_form_button_click as usize)?
                    .refany
                    .clone();
                let mut payload = handler.clone();
                let action = payload.downcast_ref::<ButtonFormAction>().map(|a| *a)?;
                Some((id, action, handler))
            })
            .collect()
    }

    #[test]
    fn a_raw_button_becomes_a_button_widget_that_submits_its_form() {
        let calls = RefAny::new(Calls::default());
        let mut lw = styling_window();
        let styled = lw.style_user_dom(page(raw_form(
            &calls,
            vec![
                named("text", "user").with_attribute(AttributeType::Value("ann".into())),
                Dom::create_button_no_a11y("Send".into()),
            ],
        )));
        let buttons = replaced_buttons(&styled);
        assert_eq!(buttons.len(), 1, "{:?}", node_types(&styled));
        assert!(node(&styled, buttons[0]).has_class(BUTTON_CLASS), "the Button widget's root");
        assert_eq!(text_under(&styled, buttons[0]), "Send", "its content is its label");

        let actions = form_buttons(&styled);
        assert_eq!(actions.len(), 1);
        assert_eq!(
            (actions[0].0, actions[0].1),
            (buttons[0], ButtonFormAction::Submit),
            "a <button> without a type submits its form (HTML's default)"
        );

        let payload = actions[0].2.clone();
        mount(&mut lw, styled);
        let _ = with_info(&lw, dom_node(buttons[0]), |info| {
            default_on_form_button_click(payload, info)
        });
        assert_eq!(submits(&calls), vec![owned(&[("user", "ann")])]);
    }

    #[test]
    fn a_buttons_type_decides_what_it_does_to_its_form() {
        let calls = RefAny::new(Calls::default());
        let lw = styling_window();
        let typed = |ty: &str, label: &str| {
            Dom::create_button_no_a11y(label.into())
                .with_attribute(AttributeType::InputType(ty.into()))
        };
        let styled = lw.style_user_dom(page(raw_form(
            &calls,
            vec![
                typed("reset", "Clear"),
                typed("button", "Help"),
                typed("submit", "Go"),
            ],
        )));
        let buttons = replaced_buttons(&styled);
        assert_eq!(buttons.len(), 3, "{:?}", node_types(&styled));
        let actions: Vec<(NodeId, ButtonFormAction)> = form_buttons(&styled)
            .into_iter()
            .map(|(id, action, _)| (id, action))
            .collect();
        assert_eq!(
            actions,
            vec![
                (buttons[0], ButtonFormAction::Reset),
                (buttons[2], ButtonFormAction::Submit),
            ],
            "type=button acts on no form"
        );
    }

    #[test]
    fn a_buttons_rich_content_stays_its_content() {
        let lw = styling_window();
        let rich = Dom::create_from_data(NodeData::create_node(NodeType::Button))
            .with_child(Dom::create_icon("send"))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Send"));
        let styled = lw.style_user_dom(page(rich));
        let buttons = replaced_buttons(&styled);
        assert_eq!(buttons.len(), 1, "{:?}", node_types(&styled));
        assert!(
            subtree(&styled, buttons[0])
                .into_iter()
                .any(|id| matches!(node(&styled, id).get_node_type(), NodeType::Icon(_))),
            "the icon the app put in its button is still in it: {:?}",
            node_types(&styled)
        );
        assert!(text_under(&styled, buttons[0]).contains("Send"));
    }

    #[test]
    fn a_button_widget_is_not_replaced() {
        let lw = styling_window();
        let styled = lw.style_user_dom(page(
            azul_layout::widgets::button::Button::create("Built".into()).dom(),
        ));
        assert!(replaced_buttons(&styled).is_empty(), "{:?}", node_types(&styled));
    }

    #[test]
    fn an_xml_button_in_an_xml_form_submits_it() {
        let xml = r#"<html><body>
            <form id="search">
                <input name="q" value="rust" />
                <button>Go</button>
            </form>
        </body></html>"#;
        let parsed = azul_layout::xml::parse_xml(xml).expect("parses");
        let dom = azul_layout::xml::dom_from_parsed_xml(parsed);
        let lw = styling_window();
        let styled = lw.style_user_dom(dom);
        let buttons = replaced_buttons(&styled);
        assert_eq!(buttons.len(), 1, "{:?}", node_types(&styled));
        assert_eq!(text_under(&styled, buttons[0]), "Go");
        let actions = form_buttons(&styled);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].1, ButtonFormAction::Submit);
    }
}

// ── A raw <form>'s own dataset ──────────────────────────────────────────────

/// A raw `<form>`'s dataset is the APP's - what its handlers find with
/// `info.get_dataset(form)`. The Form it becomes keeps it on the form node,
/// and finds its own state elsewhere.
mod raw_form_dataset {
    use azul_layout::widgets::form::submit_form;

    use super::{
        forms::{mount, named, owned, raw_form, submits, the_form, Calls},
        *,
    };

    #[derive(Debug)]
    struct AppData(u32);

    #[test]
    fn a_raw_forms_own_dataset_survives_its_replacement() {
        let calls = RefAny::new(Calls::default());
        let raw = raw_form(
            &calls,
            vec![named("text", "user").with_attribute(AttributeType::Value("ann".into()))],
        )
        .with_dataset(Some(RefAny::new(AppData(7))).into());
        let mut lw = styling_window();
        let styled = lw.style_user_dom(page(raw));
        let form = the_form(&styled);
        let mut dataset = node(&styled, form)
            .get_dataset()
            .cloned()
            .expect("the form node keeps a dataset");
        let seven = dataset.downcast_ref::<AppData>().map(|d| d.0);
        assert_eq!(seven, Some(7), "the app's dataset, not the form's state");

        // ... and the Form still works: its values, its app handler.
        mount(&mut lw, styled);
        let _ = with_info(&lw, dom_node(form), |mut info| {
            submit_form(&mut info, dom_node(form))
        });
        assert_eq!(submits(&calls), vec![owned(&[("user", "ann")])]);
    }
}
