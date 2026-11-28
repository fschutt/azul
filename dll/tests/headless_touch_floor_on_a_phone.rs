//! The touch floor of a button through the shell's own flow: a window made with the style `AppConfig`
//! detected (Android's, iOS's), the app's layout callback, `regenerate_layout` - not the layout funnel
//! alone, as `layout/tests/a_button_is_a_touch_target_on_a_phone.rs` has it.
//!
//! The layout tests say a default button is 48 dp (Android) and 44 pt (iOS) tall and wide; the QA's audit
//! of an Android build measured AzPaint's toolbar keys at 22 x 34 dp, so the question this file asks is
//! whether the window of a phone gets the floor on the way the shells build it: the first pass, the passes
//! that keep the retained layout, and the pass that follows a change of the system style (the phone's
//! language).

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::{
    common::event::{PlatformWindow, SharedUndoManager},
    headless::HeadlessWindow,
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::{Dom, DomId, NodeId},
    geom::LogicalSize,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::{OptionRefAny, RefAny},
    resources::AppConfig,
};
use azul_css::{
    system::{defaults, SystemStyle},
    AzString,
};
use azul_layout::{widgets::button::Button, window_state::WindowCreateOptions};
use rust_fontconfig::FcFontCache;

/// The way AzPaint's toolbar is made: a row of icon-only buttons, and a button with words.
extern "C" fn toolbar(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
        .with_css(
            "margin: 0px; display: flex; flex-direction: row; justify-content: space-around; \
             align-items: center; padding: 6px 8px;",
        )
        .with_child(Button::create(AzString::from("")).with_icon(AzString::from("undo")).dom())
        .with_child(Button::create(AzString::from("")).with_icon(AzString::from("redo")).dom())
        .with_child(Button::create(AzString::from("OK")).dom())
}

fn window(style: SystemStyle) -> HeadlessWindow {
    let mut config = AppConfig::default();
    config.system_style = style;

    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize { width: 411.0, height: 914.0 };
    options.window_state.layout_callback = LayoutCallback { cb: toolbar, ctx: OptionRefAny::None };

    HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(()))),
        SharedUndoManager::new(),
        config,
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

/// The sizes `(width, height)` of the buttons of the laid-out window, in document order.
fn buttons(window: &HeadlessWindow) -> Vec<(f32, f32)> {
    let lw = window.common.layout_window.as_ref().expect("a layout window");
    let lr = lw.layout_results.get(&DomId::ROOT_ID).expect("the root was laid out");
    let mut out = Vec::new();
    for i in 0..lr.styled_dom.node_data.as_ref().len() {
        let node = NodeId::new(i);
        let is_button = lr.styled_dom.node_data.as_container()[node]
            .get_ids_and_classes()
            .iter()
            .any(|c| matches!(c.as_class(), Some(s) if s == "__azul-native-button"));
        if !is_button {
            continue;
        }
        let idx = *lr
            .layout_tree
            .dom_to_layout
            .get(&node)
            .and_then(|v| v.first())
            .expect("the button has a layout node");
        let size = lr.layout_tree.get(idx).expect("layout node").used_size.expect("the button has a size");
        out.push((size.width, size.height));
    }
    out
}

fn every_button_is_at_least(window: &HeadlessWindow, floor: f32, when: &str) {
    let sizes = buttons(window);
    assert_eq!(sizes.len(), 3, "{when}: the three buttons are laid out: {sizes:?}");
    for (index, (w, h)) in sizes.iter().enumerate() {
        assert!(
            *w >= floor - 0.1 && *h >= floor - 0.1,
            "{when}: button {index} is {w} x {h}, the floor is {floor}: {sizes:?}"
        );
    }
}

#[test]
fn a_button_of_an_android_window_is_48_dp_on_the_first_pass_and_on_the_next_ones() {
    let mut window = window(defaults::android_material_light());
    window.regenerate_layout().expect("the first pass");
    every_button_is_at_least(&window, 48.0, "first pass");
    for pass in 2..=4 {
        window.regenerate_layout().expect("a later pass");
        every_button_is_at_least(&window, 48.0, &format!("pass {pass}"));
    }
}

#[test]
fn a_button_of_an_ios_window_is_44_pt_on_the_first_pass_and_on_the_next_ones() {
    let mut window = window(defaults::ios_light());
    window.regenerate_layout().expect("the first pass");
    every_button_is_at_least(&window, 44.0, "first pass");
    for pass in 2..=4 {
        window.regenerate_layout().expect("a later pass");
        every_button_is_at_least(&window, 44.0, &format!("pass {pass}"));
    }
}

#[test]
fn the_floor_stays_when_the_system_style_is_replaced_by_one_of_another_language() {
    // The Android shell's ConfigChanged: the same style with the phone's new language.
    let mut window = window(defaults::android_material_light());
    window.regenerate_layout().expect("the first pass");
    let mut style = defaults::android_material_light();
    style.language = azul_css::system::SystemLanguage::new("de-DE", false);
    window.adopt_system_style(Arc::new(style));
    window.regenerate_layout().expect("the pass after the change");
    every_button_is_at_least(&window, 48.0, "after the language moved");
}

#[test]
fn a_desktop_window_keeps_the_size_of_its_buttons() {
    for style in [defaults::macos_modern_light(), defaults::windows_11_light()] {
        let mut window = window(style);
        window.regenerate_layout().expect("the first pass");
        for (w, h) in buttons(&window) {
            assert!(h < 40.0, "a desktop button does not grow to a finger: {w} x {h}");
        }
    }
}
