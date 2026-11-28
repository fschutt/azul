//! A start lays the page out ONCE, and an event that no callback answered and that changed nothing the page
//! depends on lays nothing out again (E76, 2026-11-28).
//!
//! The QA's cold-start lab of AzWidgets (Android, 10 starts of 10; iOS Simulator too) printed a second full layout a
//! few milliseconds after the first picture, `layout_begin #2 reason=RefreshDom`, with the same picture and 0.3 to
//! 0.9 s of the engine's time: `RefreshDom` re-runs the APP's `layout()` and the whole solve. The rule these tests pin
//! is the one every shell has to keep: a window asks for a new DOM only when a callback returned
//! `Update::RefreshDom` or when state the DOM was built from changed (the language, the mode, a size that crosses a
//! breakpoint). A window gaining the focus, a pointer moving over the page, a press that nothing handles, a wheel
//! notch, a key nobody listens to, a resize to the size the window has, a timer turn and the same system style again
//! are none of those.
//!
//! The page counts the calls of its `layout()` callback; the window is the headless shell, which runs the shared
//! event pass and the shared frame decision (`PlatformWindow::process_window_events`, `service_frame`) every shell does.

use core::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// The app state: how often the page was asked for.
#[derive(Debug, Clone, Default)]
struct Page {
    layouts: Arc<AtomicUsize>,
}

/// `body > [text, button, text input, check box, a scroll box with a tall filler]`: the controls a gallery has, none
/// with a callback of the app's, so nothing the events below do is the app's to ask for.
extern "C" fn page_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_layout::widgets::{button::Button, check_box::CheckBox, text_input::TextInput};

    if let Some(page) = data.downcast_ref::<Page>() {
        page.layouts.fetch_add(1, Ordering::SeqCst);
    }
    Dom::create_body()
        .with_css("margin: 0;")
        .with_child(Dom::create_p_with_text("A page with controls"))
        .with_child(Button::create("Ok".into()).dom())
        .with_child(TextInput::create().dom())
        .with_child(CheckBox::create(false).dom())
        .with_child(
            Dom::create_div()
                .with_css("width: 300px; height: 120px; overflow-y: scroll;")
                .with_child(Dom::create_div().with_css("width: 280px; height: 1000px;")),
        )
}

/// A started window, on a frozen clock, and how often its page was asked for.
fn started() -> (HeadlessWindow, Arc<AtomicUsize>) {
    azul_core::task::reset_test_clock();
    azul_core::task::freeze_test_clock();
    let page = Page::default();
    let layouts = Arc::clone(&page.layouts);
    let state = Arc::new(RefCell::new(RefAny::new(page)));
    let window = make_window_sized(&state, page_layout, 400.0, 600.0);
    (window, layouts)
}

/// Turns of the window's own loop, a driver period apart (the headless shell's `pump_once`: the timers, the threads
/// and the frame a raised request still owes).
fn pump(window: &mut HeadlessWindow, turns: usize) {
    for _ in 0..turns {
        let _ = azul_core::task::advance_test_clock_ms(16);
        window.pump_once(true);
    }
}

/// A window that has had its first turns: laid out once, nothing owed.
fn settled() -> (HeadlessWindow, Arc<AtomicUsize>) {
    let (mut window, layouts) = started();
    pump(&mut window, 5);
    (window, layouts)
}

fn layouts_of(layouts: &AtomicUsize) -> usize {
    layouts.load(Ordering::SeqCst)
}

/// What the window still owes after an event: a DOM rebuild is the thing none of these events may leave.
fn rebuild_owed(window: &HeadlessWindow) -> bool {
    window.common.regeneration_pending() && !window.common.relayout_only_pending()
}

/// The first turns of a window ask the page for its DOM once.
///
/// Mutation: any request raised and not retired by the first layout (a callback of the first pass that returned
/// `RefreshDom`, a shell that asks again after its first frame): the second call shows here.
#[test]
fn a_start_asks_the_page_for_its_dom_once() {
    let (mut window, layouts) = started();
    pump(&mut window, 20);
    assert_eq!(
        layouts_of(&layouts),
        1,
        "twenty turns of a window that nobody touched asked the page {} times",
        layouts_of(&layouts)
    );
    assert!(!rebuild_owed(&window), "nothing is owed after the start");
}

/// The window loses the keyboard focus and gains it (X11's `FocusOut` and `FocusIn`, Win32's `WM_KILLFOCUS` and
/// `WM_SETFOCUS`, AppKit's `resignKeyWindow` and `becomeKeyWindow`): the engine styles the window (in)active and fires
/// `WindowFocusLost` and `WindowFocusReceived`; no page is asked for.
#[test]
fn the_window_losing_and_gaining_the_focus_asks_the_page_for_nothing() {
    let (mut window, layouts) = settled();
    for focused in [false, true] {
        window.snapshot_window_state_baseline("test.window_focus");
        window
            .common
            .update_unsynced_state(|ws| ws.window_focused = focused);
        if focused {
            window.common.note_focus_gained();
        }
        let result = window.process_window_events(0);
        window.service_frame(result);
        pump(&mut window, 3);
        assert_eq!(
            layouts_of(&layouts),
            1,
            "the window {} the focus and asked for a new DOM (result {result:?})",
            if focused { "gaining" } else { "losing" }
        );
        assert!(!rebuild_owed(&window));
    }
}

/// The pointer moves over the page, across a button, a text field and a check box, and leaves it again.
#[test]
fn a_pointer_moving_over_the_controls_asks_the_page_for_nothing() {
    let (mut window, layouts) = settled();
    for (x, y) in [(10.0, 10.0), (20.0, 40.0), (20.0, 70.0), (20.0, 100.0), (390.0, 590.0), (200.0, 300.0)] {
        step(&mut window, HeadlessEvent::MouseMove { x, y });
        pump(&mut window, 1);
        assert_eq!(
            layouts_of(&layouts),
            1,
            "the pointer at ({x}, {y}) asked for a new DOM"
        );
    }
    assert!(!rebuild_owed(&window));
}

/// A press and a release on blank page that nothing handles (no callback of the app's, no control).
#[test]
fn a_press_that_nothing_handles_asks_the_page_for_nothing() {
    let (mut window, layouts) = settled();
    let _ = click_at(&mut window, 380.0, 500.0);
    pump(&mut window, 3);
    assert_eq!(layouts_of(&layouts), 1, "a click on blank page asked for a new DOM");
    assert!(!rebuild_owed(&window));
}

/// A wheel notch over the scroll box and a key nobody listens to.
#[test]
fn a_wheel_notch_and_a_key_nobody_listens_to_ask_the_page_for_nothing() {
    let (mut window, layouts) = settled();
    step(&mut window, HeadlessEvent::MouseMove { x: 100.0, y: 200.0 });
    step(&mut window, HeadlessEvent::Scroll { delta_x: 0.0, delta_y: -40.0 });
    pump(&mut window, 20);
    assert_eq!(layouts_of(&layouts), 1, "a wheel notch asked for a new DOM");
    step(&mut window, HeadlessEvent::KeyDown { virtual_keycode: VirtualKeyCode::F9 });
    step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: VirtualKeyCode::F9 });
    pump(&mut window, 3);
    assert_eq!(layouts_of(&layouts), 1, "a key nobody listens to asked for a new DOM");
    assert!(!rebuild_owed(&window));
}

/// A resize to the size the window already has (the system says "resized" at every relayout of its surface; Android does it
/// at the first draw), taken the way every shell takes a size change: the shared policy
/// (`CommonWindowState::request_regeneration_for_resize`) rebuilds the DOM only when a breakpoint is crossed or a recorded
/// size query flips, and the same size is neither.
#[test]
fn a_resize_to_the_size_the_window_has_asks_the_page_for_nothing() {
    let (mut window, layouts) = settled();
    let size = window.common.current_window_state().size.dimensions;
    window.snapshot_window_state_baseline("test.same_size");
    window
        .common
        .update_window_state(event::WindowStateSource::Os, |ws| ws.size.dimensions = size);
    let full = window.common.request_regeneration_for_resize(size, size);
    let result = window.process_window_events(0);
    window.service_frame(result);
    pump(&mut window, 3);
    assert!(!full, "the policy asked for a full rebuild at the size the window has");
    assert_eq!(layouts_of(&layouts), 1, "the same size again asked for a new DOM");
    assert!(!rebuild_owed(&window));
}

/// The system style the window already shows, handed over again (the OS reports its appearance at every wake).
#[test]
fn the_same_system_style_again_asks_the_page_for_nothing() {
    let (mut window, layouts) = settled();
    let shown = window.common.current_window_state().mode;
    let _ = window.set_system_theme(shown);
    pump(&mut window, 3);
    assert_eq!(layouts_of(&layouts), 1, "the mode it already shows asked for a new DOM");
    assert!(!rebuild_owed(&window));
}
