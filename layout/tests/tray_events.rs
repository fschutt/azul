//! System-tray events that no menu item's own callback handles: a click on
//! the icon, a middle click, a scroll, a context-menu request, a menu item
//! without a callback.
//!
//! The platform callbacks (an AppKit action, an SNI `Activate` D-Bus call)
//! queue them into `azul_layout::managers::tray_event`'s mailbox. Nothing
//! drained that mailbox, so none of them ever reached the app. These tests
//! pin the half every host can run:
//!
//! * the mailbox - arrival order, bounded;
//! * the routing to the tray's own callback (`TrayIconData::with_callback`), and that a tray
//!   without one drains and drops;
//! * the delivery: the callback runs through `LayoutWindow::invoke_single_callback` (what the
//!   dll's `invoke_menu_callback` calls) and reads WHICH event it runs for from
//!   `CallbackInfo::get_tray_event`, which is `None` everywhere else.

use std::sync::{Arc, Mutex, MutexGuard};

use azul_core::{
    callbacks::Update,
    dom::Dom,
    geom::LogicalSize,
    gl::OptionGlContextPtr,
    menu::OptionCoreMenuCallback,
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
    tray::{TrayEvent, TrayEventType, TrayIconData},
    window::RawWindowHandle,
};
use azul_css::{system::SystemStyle, AzString};
use azul_layout::{
    callbacks::{Callback, CallbackInfo, ExternalSystemCallbacks},
    managers::tray_event::{
        current_tray_event, drain_tray_events, has_queued_tray_events, queue_tray_event,
        route_tray_events, with_current_tray_event, TrayDelivery, MAX_QUEUED_TRAY_EVENTS,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The mailbox is a process global; the tests that touch it take turns.
fn serial() -> MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What the tray's callback saw.
#[derive(Debug, Default)]
struct Seen {
    events: Vec<TrayEvent>,
    calls: usize,
}

extern "C" fn remember_tray_event(mut data: RefAny, info: CallbackInfo) -> Update {
    let event = info.get_tray_event();
    if let Some(mut seen) = data.downcast_mut::<Seen>() {
        seen.calls += 1;
        if let Some(event) = event {
            seen.events.push(event);
        }
    }
    Update::DoNothing
}

fn seen_by(data: &RefAny) -> (Vec<TrayEvent>, usize) {
    let mut probe = data.clone();
    let seen = probe.downcast_ref::<Seen>().expect("the RefAny holds a Seen");
    (seen.events.clone(), seen.calls)
}

fn tray_with_callback(data: &RefAny) -> TrayIconData {
    TrayIconData::new(AzString::from("rs.azul.test-tray"), AzString::from("Test"))
        .with_callback(data.clone(), Callback::from_ptr(remember_tray_event))
}

/// An empty page, laid out once: what `invoke_single_callback` runs against.
fn laid_out_window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(200.0, 100.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(Dom::create_body()),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
    lw
}

/// Deliver the way the dll's collector does: the tray's callback, through
/// the single-callback path a tray menu item uses, with the event installed.
fn deliver(lw: &mut LayoutWindow, delivery: TrayDelivery) -> Update {
    let TrayDelivery { callback, event } = delivery;
    let mut cb = Callback::from_core(callback.callback);
    let mut data = callback.refany;
    let state = lw.current_window_state.clone();
    with_current_tray_event(&event, || {
        lw.invoke_single_callback(
            &mut cb,
            &mut data,
            &RawWindowHandle::Unsupported,
            &OptionGlContextPtr::None,
            Arc::new(SystemStyle::default()),
            &ExternalSystemCallbacks::rust_internal(),
            &None,
            &state,
            &RendererResources::default(),
        )
        .1
    })
}

#[test]
fn a_tray_has_no_callback_until_one_is_set() {
    assert!(TrayIconData::default().callback.as_ref().is_none());
    let data = RefAny::new(Seen::default());
    let tray = tray_with_callback(&data);
    assert!(tray.callback.as_ref().is_some(), "with_callback stores it");
}

#[test]
fn a_plain_click_on_the_icon_is_routed_to_the_trays_callback() {
    let _serial = serial();
    drop(drain_tray_events());
    let data = RefAny::new(Seen::default());
    let tray = tray_with_callback(&data);

    // What the platforms queue: macOS's button action and SNI's Activate,
    // a middle click, a scroll, a menu item without a callback of its own.
    assert!(queue_tray_event(TrayEvent::simple(TrayEventType::Activate)));
    assert!(queue_tray_event(TrayEvent::simple(TrayEventType::SecondaryActivate)));
    let mut scroll = TrayEvent::simple(TrayEventType::Scroll);
    scroll.scroll_delta = -3;
    assert!(queue_tray_event(scroll));
    assert!(queue_tray_event(TrayEvent::menu_item(7)));
    assert!(has_queued_tray_events());

    let deliveries = route_tray_events(drain_tray_events(), &tray.callback);
    assert!(!has_queued_tray_events(), "routing drained the mailbox");
    let kinds: Vec<TrayEventType> = deliveries.iter().map(|d| d.event.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TrayEventType::Activate,
            TrayEventType::SecondaryActivate,
            TrayEventType::Scroll,
            TrayEventType::MenuItem,
        ],
        "one delivery per event, in arrival order"
    );
    assert_eq!(deliveries[2].event.scroll_delta, -3);
    assert_eq!(deliveries[3].event.menu_command, 7);
    let expected = tray.callback.as_ref().expect("the tray has a callback");
    assert!(
        deliveries.iter().all(|d| d.callback == *expected),
        "every event goes to the tray's own callback"
    );
}

#[test]
fn a_tray_without_a_callback_drains_and_delivers_nothing() {
    let _serial = serial();
    drop(drain_tray_events());
    assert!(queue_tray_event(TrayEvent::simple(TrayEventType::Activate)));
    let deliveries = route_tray_events(drain_tray_events(), &OptionCoreMenuCallback::None);
    assert!(deliveries.is_empty());
    assert!(
        !has_queued_tray_events(),
        "the events are dropped, not left to pile up"
    );
}

#[test]
fn the_mailbox_is_bounded_and_keeps_arrival_order() {
    let _serial = serial();
    drop(drain_tray_events());
    for i in 0..MAX_QUEUED_TRAY_EVENTS {
        #[allow(clippy::cast_possible_truncation)]
        let command = i as u32;
        assert!(queue_tray_event(TrayEvent::menu_item(command)));
    }
    assert!(
        !queue_tray_event(TrayEvent::simple(TrayEventType::Activate)),
        "a full mailbox refuses"
    );
    let drained = drain_tray_events();
    assert_eq!(drained.len(), MAX_QUEUED_TRAY_EVENTS);
    assert_eq!(drained[0].menu_command, 0);
    assert_eq!(
        drained[MAX_QUEUED_TRAY_EVENTS - 1].menu_command as usize,
        MAX_QUEUED_TRAY_EVENTS - 1
    );
    assert!(drain_tray_events().is_empty());
}

#[test]
fn the_trays_callback_reads_which_event_it_runs_for() {
    let _serial = serial();
    drop(drain_tray_events());
    let data = RefAny::new(Seen::default());
    let tray = tray_with_callback(&data);
    let mut lw = laid_out_window();

    assert!(queue_tray_event(TrayEvent::simple(TrayEventType::Activate)));
    let mut deliveries = route_tray_events(drain_tray_events(), &tray.callback);
    assert_eq!(deliveries.len(), 1);
    let update = deliver(&mut lw, deliveries.remove(0));
    assert_eq!(update, Update::DoNothing, "the callback's answer comes back");

    let (events, calls) = seen_by(&data);
    assert_eq!(calls, 1, "the tray's callback ran exactly once");
    assert_eq!(
        events,
        vec![TrayEvent::simple(TrayEventType::Activate)],
        "it saw the click it was delivered for"
    );
    assert_eq!(
        current_tray_event(),
        None,
        "outside a delivery there is no current tray event"
    );
}
