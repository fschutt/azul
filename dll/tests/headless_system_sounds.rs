//! System sounds through the shell's own drain, end to end on a headless window.
//!
//! `CallbackInfo::play_system_sound` queues a `CallbackChange::PlaySound`; the shell applies it into
//! the window's `SoundManager` and, at the end of the outermost pass (`process_window_events` at
//! depth 0), decides each request, records it and hands what plays to the platform. A headless
//! window plays nothing: it records every request and marks what would have played
//! `skipped:headless`, which is what `assert_sound` reads.

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::{common::event::PlatformWindow, headless::HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::Dom,
    geom::LogicalSize,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::AppConfig,
    sound::{SoundOutcome, SoundRecord, SystemSound},
};
use azul_layout::{callbacks::CallbackChange, window_state::WindowCreateOptions};
use rust_fontconfig::FcFontCache;

struct AppState;

extern "C" fn a_page(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_css("margin: 0px;")
}

fn window() -> HeadlessWindow {
    let fc_cache = Arc::new(FcFontCache::default());
    let app_data = Arc::new(RefCell::new(RefAny::new(AppState)));
    let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());

    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions = LogicalSize {
        width: 360.0,
        height: 640.0,
    };
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = a_page;
    options.window_state.layout_callback = LayoutCallback::create(cb);

    HeadlessWindow::new(
        options,
        app_data,
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        icon_provider,
        fc_cache,
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

/// What the window recorded, oldest first.
fn recorded(window: &HeadlessWindow) -> Vec<SoundRecord> {
    window
        .common
        .layout_window
        .as_ref()
        .expect("the window has a LayoutWindow after regenerate_layout")
        .sound_manager
        .records()
        .cloned()
        .collect()
}

/// What the shell does at the end of an event pass: the baseline of the window state (the contract
/// shape: snapshot, mutate, pass), then the pass, whose last step drains the sounds.
fn end_the_pass(window: &mut HeadlessWindow) {
    window.snapshot_window_state_baseline("test.system_sounds");
    let _ = window.process_window_events(0);
}

fn ask(window: &mut HeadlessWindow, sound: SystemSound) {
    // what a callback's `play_system_sound` leaves for the shell to apply
    let _ = window.apply_user_change(&CallbackChange::PlaySound { sound });
}

#[test]
fn the_requests_of_a_pass_are_decided_at_its_end_and_a_headless_window_plays_nothing() {
    let mut window = window();
    window.regenerate_layout().expect("the first layout");
    ask(&mut window, SystemSound::TabChange);
    ask(&mut window, SystemSound::TabChange);
    ask(&mut window, SystemSound::Error);
    assert!(
        recorded(&window).is_empty(),
        "a request waits in the queue until the pass ends"
    );

    end_the_pass(&mut window);

    let records = recorded(&window);
    assert_eq!(records.len(), 3, "every request is recorded: {records:#?}");
    assert_eq!(records[0].sound, SystemSound::TabChange);
    assert_eq!(records[1].sound, SystemSound::TabChange);
    assert_eq!(
        records[1].suppressed_by,
        Some("coalesced"),
        "the same sound twice in one pass is one sound"
    );
    assert_eq!(records[2].sound, SystemSound::Error);
    for record in &records {
        assert!(
            matches!(record.outcome, SoundOutcome::Skipped(_)),
            "a headless window plays nothing, whatever the route: {record:#?}"
        );
    }
    // a request that would have played is marked as headless, not as dropped by the manager
    if records[0].suppressed_by.is_none() {
        assert_eq!(
            records[0].outcome,
            SoundOutcome::Skipped(String::from("headless"))
        );
    }
}

#[test]
fn a_second_pass_decides_only_the_requests_made_after_the_first() {
    let mut window = window();
    window.regenerate_layout().expect("the first layout");
    ask(&mut window, SystemSound::Click);
    end_the_pass(&mut window);
    assert_eq!(recorded(&window).len(), 1);

    end_the_pass(&mut window);
    assert_eq!(
        recorded(&window).len(),
        1,
        "a pass with nothing queued records nothing"
    );

    ask(&mut window, SystemSound::Back);
    end_the_pass(&mut window);
    let records = recorded(&window);
    assert_eq!(records.len(), 2);
    assert_eq!(records[1].sound, SystemSound::Back);
    assert!(records[1].sequence > records[0].sequence);
}

#[test]
fn the_apps_mute_reaches_the_window_and_is_the_reason_of_the_request() {
    let mut window = window();
    window.regenerate_layout().expect("the first layout");
    let _ = window.apply_user_change(&CallbackChange::SetSoundsMuted { muted: true });
    ask(&mut window, SystemSound::Notification);
    end_the_pass(&mut window);

    let records = recorded(&window);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].suppressed_by, Some("app-muted"));
}
