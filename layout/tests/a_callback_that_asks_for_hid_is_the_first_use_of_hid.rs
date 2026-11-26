//! The HID devices are enumerated from their first use, and a callback that asks for them is one.
//!
//! `get_hid_devices()` and `get_hid_reports()` are poll-style accessors: an app can call them from any callback without
//! ever listening for `HidReport`. The pump no longer opens every HID device of the machine at the first window (146 ms of
//! the window's thread on a Mac, for an app that uses none), so the ask itself is what makes it enumerate, on its next
//! pass; the first answer is what there is (nothing before the first enumeration) and the list is there from the next
//! event on. Without the ask the accessors would answer an empty list for ever in an app that listens for nothing.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use azul_core::{
    dom::{DomId, DomNodeId},
    geom::OptionLogicalPosition,
    gl::OptionGlContextPtr,
    hid::HidDevice,
    hit_test::ScrollPosition,
    refany::OptionRefAny,
    resources::RendererResources,
    styled_dom::NodeHierarchyItemId,
    window::{MonitorVec, RawWindowHandle},
};
use azul_css::system::SystemStyle;
use azul_layout::{
    callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

fn fresh_window() -> LayoutWindow {
    LayoutWindow::new(FcFontCache::default()).expect("a layout window")
}

/// Run `f` against a `CallbackInfo` over `lw`, the way a user callback sees it.
fn with_info<R>(lw: &LayoutWindow, f: impl FnOnce(&mut CallbackInfo) -> R) -> R {
    let renderer_resources = RendererResources::default();
    let previous_window_state: Option<FullWindowState> = None;
    let current_window_state = lw.current_window_state.clone();
    let gl_context = OptionGlContextPtr::None;
    let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> = BTreeMap::new();
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
    let mut info = CallbackInfo::new(&ref_data, &changes, DomNodeId::ROOT, OptionLogicalPosition::None, OptionLogicalPosition::None);
    f(&mut info)
}

/// A window nobody asked anything of has no use of the HID devices (the pump does not enumerate), and a callback that
/// asks for the devices, or for the reports, is the first use: the pump may enumerate on its next pass, once.
///
/// Mutation: `get_hid_devices` that does not ask; `get_hid_reports` that does not ask.
#[test]
fn a_callback_that_asks_for_the_devices_or_the_reports_is_the_first_use_of_them() {
    type Ask = fn(&mut CallbackInfo);
    let asks: [(&str, Ask); 2] = [
        ("get_hid_devices", |info| {
            let _ = info.get_hid_devices();
        }),
        ("get_hid_reports", |info| {
            let _ = info.get_hid_reports();
        }),
    ];
    for (name, ask) in asks {
        let lw = fresh_window();
        assert!(!lw.app_services.hid.wanted(), "{name}: a new window has asked for nothing");
        assert!(!lw.app_services.hid.claim_enumeration_if_used(false), "{name}: nothing uses the devices yet: the pump does not enumerate");
        with_info(&lw, ask);
        assert!(lw.app_services.hid.wanted(), "{name}: the callback asked");
        assert!(lw.app_services.hid.claim_enumeration_if_used(false), "{name}: the ask is the first use: the pump enumerates on its next pass");
        assert!(!lw.app_services.hid.claim_enumeration_if_used(false), "{name}: once");
    }
}

/// The first answer is what there is (nothing before the first enumeration, and no panic), and the list is there from
/// the next event on, once the pump has enumerated and handed the devices to the window's manager.
///
/// Mutation: an answer that is always empty; a first ask that clears what the manager holds.
#[test]
fn the_first_answer_is_what_there_is_and_the_list_is_there_once_it_was_enumerated() {
    let mut lw = fresh_window();
    let (devices, reports) = with_info(&lw, |info| (info.get_hid_devices().as_slice().len(), info.get_hid_reports().as_slice().len()));
    assert_eq!((devices, reports), (0, 0), "nothing was enumerated at the first ask");
    lw.hid_manager.set_devices(vec![HidDevice {
        vendor_id: 1,
        product_id: 2,
        usage_page: 1,
        usage: 4,
        name: "pad".into(),
        serial: "".into(),
        instance: 7,
    }]);
    let names = with_info(&lw, |info| info.get_hid_devices().as_slice().iter().map(|d| d.name.as_str().to_string()).collect::<Vec<_>>());
    assert_eq!(names, vec!["pad".to_string()], "the next ask has the list the pump handed over");
}
