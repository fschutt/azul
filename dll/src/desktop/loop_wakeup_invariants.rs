//! Source-text invariants for the run loops' APP-LEVEL event sources: the
//! system tray, native notifications and global hotkeys.
//!
//! None of these belongs to a window, and each reports on something the
//! windows' own connections never see: the tray and the notification server
//! talk D-Bus on Linux, X11 hotkeys arrive on a second display connection,
//! the Wayland portal's `Activated` lands on a listener thread. The loops
//! that have to notice them are `#[cfg(target_os = ...)]` code a test binary
//! on one host cannot run, so these tests pin the SHAPES whose absence was
//! the defect:
//!
//! * the Linux loops wait on those sources (their fds, plus one wake fd for
//!   the threads) instead of capping their park at 100 ms while one is live;
//! * every D-Bus user drains the shared connection completely, since
//!   `dbus_connection_read_write_dispatch` handles ONE message per call and a
//!   message parsed but not dispatched is invisible to `poll(2)`;
//! * a plain tray click (no menu callback) reaches the app at all -
//!   `drain_tray_events` used to have no caller;
//! * the three sources pick their window by rule (the tray and notifications
//!   the most recently focused, a hotkey the window that declared it), not
//!   "whatever the registry lists first" (pointer order, a `HashMap`, HWND
//!   order);
//! * the manual macOS loop delivers every source right before it parks, so a
//!   fire handled inside a `sendEvent:` never waits for the next event.

const RUN_RS: &str = include_str!("shell2/run.rs");
const X11_RS: &str = include_str!("shell2/linux/x11/mod.rs");
const WAYLAND_RS: &str = include_str!("shell2/linux/wayland/mod.rs");
const GNOME_MENU_RS: &str = include_str!("shell2/linux/gnome_menu/manager.rs");
const TRAY_RS: &str = include_str!("tray/mod.rs");
const TRAY_LINUX_RS: &str = include_str!("tray/linux.rs");
const TRAY_MACOS_RS: &str = include_str!("tray/macos.rs");
const NOTIFY_LINUX_RS: &str = include_str!("notifications/linux.rs");
const HOTKEY_RS: &str = include_str!("global_hotkey/mod.rs");
const HOTKEY_X11_RS: &str = include_str!("global_hotkey/x11.rs");
const HOTKEY_PORTAL_RS: &str = include_str!("global_hotkey/portal.rs");
const LOOP_WAKER_RS: &str = include_str!("loop_waker.rs");
const APP_EVENTS_RS: &str = include_str!("app_events.rs");

/// The text of the first top-level `fn` whose signature contains `name`, up
/// to its closing brace in column 0.
fn top_level_fn_body<'a>(source: &'a str, name: &str) -> &'a str {
    let start = source
        .find(name)
        .unwrap_or_else(|| panic!("{name} not found - was it renamed?"));
    let rest = &source[start..];
    match rest.find("\n}\n") {
        Some(end) => &rest[..end],
        None => rest,
    }
}

/// How often `needle` occurs in `haystack`.
fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The X11 and Wayland loops park in `poll(2)`. They used to cap that park at
/// 100 ms whenever a tray was installed, a notification was outstanding or a
/// hotkey was registered, because the D-Bus socket, the hotkey grab
/// connection and the portal thread were not in the wait set - ten wake-ups a
/// second for as long as the app had a tray icon, and still up to 100 ms of
/// latency on every click. The sources are now IN the wait set.
#[test]
fn no_linux_loop_caps_its_park_for_the_tray_notifications_or_hotkeys() {
    for (backend, src) in [("X11", X11_RS), ("Wayland", WAYLAND_RS)] {
        assert!(
            !src.contains("has_tray || has_hotkeys"),
            "{backend}: the park is still capped while a tray / hotkey is live"
        );
        assert!(
            !src.contains("notifications::needs_polling()"),
            "{backend}: an outstanding notification still caps the park"
        );
        assert!(
            !src.contains("global_hotkey::needs_loop_polling()"),
            "{backend}: a registered hotkey still caps the park"
        );
        assert!(
            src.contains("loop_waker::wait_fds()"),
            "{backend}: the app-level sources' fds are not in the poll set"
        );
        assert!(
            src.contains("loop_waker::must_not_park()"),
            "{backend}: work already buffered in libdbus / Xlib (which no fd announces) is not \
             checked before parking"
        );
    }
}

/// With two or more windows the Linux run loop waits in
/// `wait_for_linux_window_activity` instead of a window's own
/// `wait_for_events`; the app-level sources must wake that wait too.
#[test]
fn the_multi_window_linux_wait_includes_the_app_sources() {
    let body = top_level_fn_body(RUN_RS, "fn wait_for_linux_window_activity");
    assert!(
        body.contains("loop_waker::wait_fds()"),
        "the multi-window wait does not poll the app-level sources"
    );
    assert!(
        body.contains("loop_waker::must_not_park()"),
        "the multi-window wait parks on top of buffered D-Bus / X work"
    );
}

/// The tray, the notification backend and the GNOME menu exporter share ONE
/// libdbus session connection (`dbus_bus_get` returns the same one to every
/// caller). `dbus_connection_read_write_dispatch` either reads OR dispatches
/// one message per call, so a message it read stayed parsed in libdbus's
/// queue - where no `poll(2)` on the socket can see it - until the next
/// wake-up. Each user drains the connection completely and registers it for
/// the wait set.
#[test]
fn every_d_bus_user_drains_the_shared_connection_completely() {
    for (user, src) in [
        ("tray/linux.rs", TRAY_LINUX_RS),
        ("notifications/linux.rs", NOTIFY_LINUX_RS),
        ("gnome_menu/manager.rs", GNOME_MENU_RS),
    ] {
        assert!(
            !src.contains("dbus_connection_read_write_dispatch)("),
            "{user}: still dispatches one message per call"
        );
        assert!(
            src.contains("drain_connection("),
            "{user}: does not drain the connection through the shared helper"
        );
        assert!(
            src.contains("loop_waker::watch_dbus_connection("),
            "{user}: its connection is not in the loops' wait set"
        );
    }
}

/// X11 hotkeys arrive on a display connection of their own; its fd goes into
/// the wait set. The portal's `Activated` arrives on a listener thread, which
/// hands the press to the App's hotkey sink - and the sink runs the waker the
/// Linux loop attached (`loop_waker::wake`). With the waker attached and the
/// fd watched, the manager never asks the loops to poll.
#[test]
fn hotkey_backends_wake_the_loop_instead_of_being_polled() {
    assert!(
        HOTKEY_X11_RS.contains("XConnectionNumber"),
        "the X11 grab connection's fd is never looked up"
    );
    let attach = RUN_RS
        .find("global_hotkey::attach_loop_waker(")
        .expect("the Linux loop never attaches its waker to the hotkey sink");
    let call = &RUN_RS[attach..(attach + 200).min(RUN_RS.len())];
    assert!(
        call.contains("loop_waker::wake") && call.contains("true"),
        "the Linux loop's hotkey waker is not the shared loop waker, or it does not say it \
         watches the grab connection's fd"
    );
    assert!(
        LOOP_WAKER_RS.contains("global_hotkey::wake_fds()"),
        "the hotkey grab connection's fd is not in the loops' wait set"
    );
    assert!(
        LOOP_WAKER_RS.contains("global_hotkey::has_buffered_input()"),
        "a loop can park on top of presses Xlib already read off the grab connection"
    );
    assert!(
        HOTKEY_PORTAL_RS.contains("sink.push(BackendEvent::Fired"),
        "the portal listener parks its presses somewhere the attached waker never hears of"
    );
}

/// A tray click that carries no menu callback - a plain click on the icon,
/// a middle click, a scroll, a menu item without a callback - is queued into
/// the tray event mailbox. Nothing ever drained that mailbox, so none of
/// those reached the app. They are now delivered to the tray's own callback
/// (`TrayIconData::callback`) with the event readable through
/// `CallbackInfo::get_tray_event`, and every loop takes them through the one
/// app-event collector instead of pumping the tray by hand.
#[test]
fn plain_tray_clicks_reach_the_app() {
    let body = top_level_fn_body(TRAY_RS, "fn take_tray_deliveries");
    assert!(
        body.contains("drain_tray_events()"),
        "take_tray_deliveries does not drain the tray event mailbox"
    );
    assert!(
        !RUN_RS.contains("tray::pump_tray()"),
        "a run loop still pumps the tray by hand, skipping the plain-click mailbox"
    );
    // macOS: without a menu, the status item's button must have an action,
    // or a click on it does nothing at all.
    assert!(
        TRAY_MACOS_RS.contains("setAction(Some(objc2::sel!(menuItemAction:)))"),
        "the macOS status item button has no action: a plain click is lost"
    );
    assert!(
        TRAY_MACOS_RS.contains("TrayEventType::Activate"),
        "the macOS tray never reports Activate"
    );
}

/// "The first window" was whatever each registry listed first: a `BTreeMap`
/// keyed by `NSWindow` pointer (allocation order), a `HashMap` of X11 ids /
/// `wl_surface` pointers (random), a `BTreeMap` of HWNDs. Tray clicks,
/// notification clicks and hotkeys now go through one collector and one
/// window rule per platform.
#[test]
fn app_level_events_do_not_run_against_an_arbitrary_first_window() {
    assert!(
        !RUN_RS.contains("pump_into_first_") && !HOTKEY_RS.contains("fn pump_into_first_"),
        "hotkeys still run against the registry's first window"
    );
    assert!(
        !RUN_RS.contains("fn pump_tray_into_windows")
            && !RUN_RS.contains("fn pump_notifications_into_windows"),
        "the macOS tray / notification pumps still pick the registry's first window"
    );
    assert!(
        count(RUN_RS, "app_events::deliver_to_macos_windows()") >= 3,
        "the macOS loops (RunForever timer, manual loop top, manual loop before parking) do \
         not all deliver through the collector"
    );
    assert!(
        count(RUN_RS, "app_events::deliver_to_win32_windows()") >= 1,
        "the Win32 loop does not deliver through the collector"
    );
    assert!(
        count(RUN_RS, "app_events::deliver_to_linux_windows()") >= 1,
        "the Linux loop does not deliver through the collector"
    );
}

/// The manual macOS loop (the DEFAULT termination behaviour) drains NSEvents,
/// processes its windows and then parks in `runMode:beforeDate:`. A Carbon
/// hotkey, a status-item click or a tray menu pick is handled INSIDE a
/// `sendEvent:` of that drain, so whatever it queued has to be delivered
/// between the drain and the park - the tray used to be pumped only at the
/// top of the loop, i.e. after the NEXT event.
#[test]
fn the_manual_macos_loop_delivers_every_app_source_right_before_it_parks() {
    let park = RUN_RS
        .find("run_loop.runMode_beforeDate(")
        .expect("the manual macOS loop parks in runMode:beforeDate:");
    let before = &RUN_RS[park.saturating_sub(2500)..park];
    assert!(
        before.contains("app_events::deliver_to_macos_windows()"),
        "the manual macOS loop parks without delivering the tray / notifications it just \
         handled"
    );
    assert!(
        before.contains("global_hotkey::pump_macos_windows()"),
        "the manual macOS loop parks without delivering the hotkeys it just handled"
    );
}

/// A click on a freedesktop notification brings an `ActivationToken` (spec
/// 1.2; GNOME and KDE send it just before `ActionInvoked`). On Wayland that
/// token is the ONLY way the app may raise its window for the click - a
/// notification click carries no input serial. The Linux backend keeps the
/// token (`notifications::take_activation_token`) and the Wayland window can
/// spend it (`WaylandWindow::activate_with_token`), but the run-loop block
/// that joined the two was replaced by the app-event collector when the loops
/// moved onto it, and nothing took the token any more: the callback ran and
/// the window stayed behind every other window.
#[test]
fn a_notification_click_raises_the_wayland_window_with_its_activation_token() {
    let body = top_level_fn_body(APP_EVENTS_RS, "fn deliver_to_linux_windows");
    let take = body
        .find("notifications::take_activation_token()")
        .expect("the Linux collector never takes the token a notification click brought");
    assert!(
        body.contains(".activate_with_token("),
        "the Linux collector never spends the activation token on the Wayland window"
    );
    // Taken before the collector's early return: a click on a notification
    // that has no callback still raises the app, and a token left behind
    // would be spent on the NEXT, unrelated delivery.
    let early_return = body
        .find("return;")
        .expect("the Linux collector returns early when nothing was collected");
    assert!(
        take < early_return,
        "the activation token is only taken when a callback was collected"
    );
}
