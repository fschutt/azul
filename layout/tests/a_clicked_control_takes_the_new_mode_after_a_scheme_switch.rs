//! A control restyled by a click shows the NEW mode's colours after a
//! restyle-only light / dark switch - exactly the faces a control BUILT in
//! the clicked state shows.
//!
//! segmented / stepper / pagination / date_picker repaint themselves on a
//! click (no DOM rebuild). They used `set_css_property` with colours baked
//! for the mode of the moment, each deciding the mode itself
//! (`window_is_dark` / `renders_dark`): a user override outranks every
//! declaration, so after `set_mode` switched the retained DOM to
//! dark, a clicked control kept its light colours until the next rebuild
//! (W4 section 6.2).
//!
//! The check, per widget and theme: build the control, click it the way the
//! shell does (the callback's writes land through the content chokepoint),
//! and build a twin already in the clicked state. By day and after a switch
//! to dark (the restyle path: the retained DOM re-styled, no new DOM), every
//! part must resolve to the twin's background and ink.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId},
    geom::LogicalSize,
    gl::OptionGlContextPtr,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{OptionDarkLightMode, RawWindowHandle, DarkLightMode},
};
use azul_css::{
    props::property::{CssProperty, CssPropertyType},
    system::{defaults, SystemStyle},
    AzString, StringVec,
};
use azul_layout::{
    callbacks::{Callback, CallbackChange, ExternalSystemCallbacks},
    overlay::ContentChange,
    widgets::{
        date_picker::DatePicker, pagination::Pagination, segmented::Segmented, stepper::Stepper,
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn env_pinned() -> bool {
    azul_css::dynamic_selector::mode_pinned_by_env().is_some()
}

fn window_state(theme: DarkLightMode) -> FullWindowState {
    let mut ws = FullWindowState {
        mode: theme,
        ..Default::default()
    };
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    ws
}

fn lay_out(lw: &mut LayoutWindow, styled: StyledDom, ws: &FullWindowState) {
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled,
        ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
}

/// `widget` in a window on a light desktop, the app following it.
fn window(widget: Dom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(defaults::macos_modern_light()));
    lw.mode = OptionDarkLightMode::None;
    let dom = Dom::create_body().with_css("margin: 0;").with_child(widget);
    lay_out(
        &mut lw,
        StyledDom::create_from_dom(dom),
        &window_state(DarkLightMode::Light),
    );
    lw
}

/// `CallbackInfo::set_mode`'s restyle path: the RETAINED DOM
/// re-styled under the new scheme, no new DOM.
fn switch_scheme(lw: &mut LayoutWindow, scheme: OptionDarkLightMode) {
    lw.mode = scheme;
    let ws = window_state(lw.window_mode_for(DarkLightMode::Light));
    let retained = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    lay_out(lw, retained, &ws);
}

fn styled(lw: &LayoutWindow) -> &StyledDom {
    &lw.layout_results[&DomId::ROOT_ID].styled_dom
}

fn has_class(lw: &LayoutWindow, node: NodeId, class: &str) -> bool {
    styled(lw).node_data.as_container()[node]
        .get_ids_and_classes()
        .iter()
        .any(|c| matches!(c.as_class(), Some(s) if s == class))
}

fn click_callback(lw: &LayoutWindow, node: NodeId) -> Option<(Callback, azul_core::refany::RefAny)> {
    styled(lw).node_data.as_container()[node]
        .get_callbacks()
        .as_ref()
        .iter()
        .find(|cb| cb.event == EventFilter::Hover(HoverEventFilter::Click))
        .map(|cb| (Callback::from_core(cb.callback.clone()), cb.refany.clone()))
}

/// Every node carrying `class`, in document order.
fn with_class(lw: &LayoutWindow, class: &str) -> Vec<NodeId> {
    (0..styled(lw).node_data.as_ref().len())
        .map(NodeId::new)
        .filter(|n| has_class(lw, *n, class))
        .collect()
}

/// A click on `node`, run the way the shell runs it; every style write it
/// makes lands through the content chokepoint the shell applies it through.
fn click(lw: &mut LayoutWindow, node: NodeId) {
    let (mut callback, mut data) = click_callback(lw, node).expect("harness: the node takes clicks");
    let state = lw.current_window_state.clone();
    let hit = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    };
    let (changes, _update) = lw.invoke_single_callback_at(
        hit,
        &mut callback,
        &mut data,
        &RawWindowHandle::Unsupported,
        &OptionGlContextPtr::None,
        Arc::new(SystemStyle::default()),
        &ExternalSystemCallbacks::rust_internal(),
        &None,
        &state,
        &RendererResources::default(),
    );
    for change in changes {
        let content = match change {
            CallbackChange::ChangeNodeCssProperties {
                dom_id,
                node_id,
                properties,
            } => ContentChange::NodeCss {
                dom_id,
                node_id,
                props: properties.as_ref().to_vec(),
                override_only: false,
            },
            CallbackChange::OverrideNodeCssProperties {
                dom_id,
                node_id,
                properties,
            } => ContentChange::NodeCss {
                dom_id,
                node_id,
                props: properties.as_ref().to_vec(),
                override_only: true,
            },
            CallbackChange::SetNodeStyle {
                dom_id,
                node_id,
                style,
            } => ContentChange::NodeStyle {
                dom_id,
                node_id,
                style,
            },
            // Focus, Tab stops, a11y states, text, popups: not colours.
            _ => continue,
        };
        let _ = lw.apply_content_change(content);
    }
}

/// A node's look: its background and its ink.
type Look = (Option<CssProperty>, Option<CssProperty>);

/// What `node` paints with: its background and its ink, as the cascade
/// resolves them (user overrides first - what the display list reads).
fn look(lw: &LayoutWindow, node: NodeId) -> Look {
    let sd = styled(lw);
    let node_data = sd.node_data.as_container();
    let state = sd.get_styled_node_state(&node);
    let get = |ty: CssPropertyType| {
        sd.get_css_property_cache()
            .get_property(&node_data[node], &node, &state, &ty)
            .cloned()
    };
    (
        get(CssPropertyType::BackgroundContent),
        get(CssPropertyType::TextColor),
    )
}

/// The looks of every node carrying one of `parts`, in document order.
fn looks(lw: &LayoutWindow, parts: &[&str]) -> Vec<(NodeId, Look)> {
    let mut nodes: Vec<NodeId> = parts.iter().flat_map(|p| with_class(lw, p)).collect();
    nodes.sort();
    nodes.into_iter().map(|n| (n, look(lw, n))).collect()
}

/// The check. `before` is clicked on the `nth` clickable node carrying
/// `target`; `after` is the same control BUILT in the state the click leads
/// to. Every node carrying one of `parts` must look the same in both, by
/// day and after a switch to dark.
fn assert_click_follows_the_mode(
    what: &str,
    before: Dom,
    after: Dom,
    target: &str,
    nth: usize,
    parts: &[&str],
) {
    let mut clicked = window(before);
    let targets: Vec<NodeId> = with_class(&clicked, target)
        .into_iter()
        .filter(|n| click_callback(&clicked, *n).is_some())
        .collect();
    click(&mut clicked, targets[nth]);
    let mut built = window(after);

    assert!(!looks(&built, parts).is_empty(), "{what}: harness - no parts found");
    assert_eq!(
        looks(&clicked, parts),
        looks(&built, parts),
        "{what}: by day, the clicked control looks like one built in its new state"
    );

    switch_scheme(&mut clicked, OptionDarkLightMode::Some(DarkLightMode::Dark));
    switch_scheme(&mut built, OptionDarkLightMode::Some(DarkLightMode::Dark));
    assert_eq!(
        looks(&clicked, parts),
        looks(&built, parts),
        "{what}: after a switch to dark, the clicked control shows the DARK faces - a colour \
         baked for light mode must not outlive the switch"
    );

    switch_scheme(&mut clicked, OptionDarkLightMode::None);
    switch_scheme(&mut built, OptionDarkLightMode::None);
    assert_eq!(
        looks(&clicked, parts),
        looks(&built, parts),
        "{what}: and back to light"
    );
}

/// Pinned flat, pinned flora, and following the app theme.
fn themed<W>(w: impl Fn() -> W, with_theme: fn(W, UiTheme) -> W) -> Vec<(String, W)> {
    vec![
        ("flat".to_string(), with_theme(w(), UiTheme::Flat)),
        ("flora".to_string(), with_theme(w(), UiTheme::Flora)),
        ("unpinned".to_string(), w()),
    ]
}

fn labels() -> StringVec {
    StringVec::from_vec(vec![
        AzString::from("Day"),
        AzString::from("Week"),
        AzString::from("Month"),
    ])
}

#[test]
fn a_clicked_segment_takes_the_new_mode_after_a_scheme_switch() {
    if env_pinned() {
        return;
    }
    let before = themed(|| Segmented::create(labels()), Segmented::with_theme);
    let after = themed(
        || Segmented::create(labels()).with_selected_index(1),
        Segmented::with_theme,
    );
    for ((name, b), (_, a)) in before.into_iter().zip(after) {
        assert_click_follows_the_mode(
            &format!("{name} segmented"),
            b.dom(),
            a.dom(),
            "__azul-native-segmented-item",
            1,
            &["__azul-native-segmented-item"],
        );
    }
}

#[test]
fn a_clicked_page_takes_the_new_mode_after_a_scheme_switch() {
    if env_pinned() {
        return;
    }
    let before = themed(|| Pagination::create(1, 3), Pagination::with_theme);
    let after = themed(|| Pagination::create(2, 3), Pagination::with_theme);
    for ((name, b), (_, a)) in before.into_iter().zip(after) {
        assert_click_follows_the_mode(
            &format!("{name} pagination"),
            b.dom(),
            a.dom(),
            "__azul-native-pagination-page",
            1,
            &[
                "__azul-native-pagination-nav",
                "__azul-native-pagination-page",
            ],
        );
    }
}

#[test]
fn a_clicked_step_takes_the_new_mode_after_a_scheme_switch() {
    if env_pinned() {
        return;
    }
    let before = themed(|| Stepper::create(labels()), Stepper::with_theme);
    let after = themed(
        || Stepper::create(labels()).with_current_step(2),
        Stepper::with_theme,
    );
    for ((name, b), (_, a)) in before.into_iter().zip(after) {
        assert_click_follows_the_mode(
            &format!("{name} stepper"),
            b.dom(),
            a.dom(),
            "__azul-native-stepper-step",
            2,
            &[
                "__azul-native-stepper-circle",
                "__azul-native-stepper-connector",
                "__azul-native-stepper-label",
            ],
        );
    }
}

#[test]
fn a_clicked_day_takes_the_new_mode_after_a_scheme_switch() {
    if env_pinned() {
        return;
    }
    let before = themed(|| DatePicker::create(2026, 9, 10), DatePicker::with_theme);
    let after = themed(|| DatePicker::create(2026, 9, 20), DatePicker::with_theme);
    for ((name, b), (_, a)) in before.into_iter().zip(after) {
        // The 20th: the 20th clickable day cell (blank cells take no clicks).
        assert_click_follows_the_mode(
            &format!("{name} date picker"),
            b.dom(),
            a.dom(),
            "__azul-native-date-picker-day",
            19,
            &["__azul-native-date-picker-day"],
        );
    }
}
