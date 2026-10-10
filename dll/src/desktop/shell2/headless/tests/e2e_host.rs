//! The AZ_E2E / AZ_DEBUG host (HEADLESS6, 2026-10-03): what a script can see
//! and drive through the headless backend.
//!
//! The /e2e corpus runs two ways: in-process (`azul_layout::e2e::runner`, the
//! `layout/tests/e2e_json.rs` gate) and against a real app
//! (`AZ_BACKEND=headless AZ_E2E=<dir> AzPaint`), which goes through THIS
//! backend. Every op the in-process runner answers must be answerable here
//! too, or a scenario that is green in CI is red against an app.

use core::sync::atomic::Ordering;

use super::*;

/// `<body>` with one silver 120x30 box: something to paint.
extern "C" fn one_box_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_css::{
        dynamic_selector::CssPropertyWithConditions,
        props::{
            layout::dimensions::{LayoutHeight, LayoutWidth},
            property::CssProperty,
        },
    };
    Dom::create_body().with_child(
        Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(120.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(30.0))),
            ]
            .into(),
        ),
    )
}

/// `assert_damage_sound { "pixel_identity": true }` compares the frame the
/// damage-driven path PAINTED against an independent full repaint. The
/// in-process runner published that frame; this backend did not, so the four
/// corpus scenarios that ask (bug-slider-thumb-trail, op-resize-grow-
/// exposed-strip, op-resize-grow-reflow, op-resize-shrink-stays-full) failed
/// against every app with "this host does not publish the damage-driven
/// framebuffer".
#[cfg(all(
    feature = "cpurender",
    any(feature = "debug-server", feature = "e2e-scripting")
))]
#[test]
fn a_headless_window_publishes_the_frame_it_painted_for_the_pixel_identity_check() {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, one_box_layout);
    // What `AZ_E2E` / `AZ_DEBUG` switch on when the window opens.
    window.publish_presented_frame = true;
    window.regenerate_layout().expect("a painted frame");
    let _ = window.common.take_regeneration();

    let frame = window
        .cpu_backend
        .last_frame
        .as_ref()
        .expect("the CPU backend painted a frame");
    let lw = window.common.layout_window.as_ref().expect("layout window");
    let (w, h, rgba) = debug_server::e2e_presented_frame(lw)
        .expect("the painted frame is published for the pixel-identity check");
    assert_eq!((w, h), (frame.width(), frame.height()));
    assert!(
        rgba == frame.data(),
        "the published pixels are the painted ones"
    );
}

/// The copy costs a frame's worth of memory traffic per present, so a window
/// no script can ask about does not publish.
#[cfg(all(
    feature = "cpurender",
    any(feature = "debug-server", feature = "e2e-scripting")
))]
#[test]
fn a_headless_window_no_script_drives_publishes_no_frame() {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, one_box_layout);
    window.publish_presented_frame = false;
    window.regenerate_layout().expect("a painted frame");
    let _ = window.common.take_regeneration();
    let lw = window.common.layout_window.as_ref().expect("layout window");
    assert!(debug_server::e2e_presented_frame(lw).is_none());
}

/// What [`record_window_id_layout`] saw.
struct SeenWindowId {
    id: String,
}

/// Records the window id its `layout()` was called for.
extern "C" fn record_window_id_layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    if let Some(mut seen) = data.downcast_mut::<SeenWindowId>() {
        seen.id = info.get_window_id().as_str().to_string();
    }
    Dom::create_body()
}

/// CAL3: AzCalendar could open only ONE event editor, because a layout
/// callback could not tell which window it was building. A window opened
/// with an id (`WindowCreateOptions.window_state.window_id`) hands it to
/// every `layout()` of that window.
#[test]
fn a_layout_callback_is_told_which_window_it_builds() {
    let state = Arc::new(RefCell::new(RefAny::new(SeenWindowId {
        id: String::new(),
    })));
    let mut window = make_window_with(&state, record_window_id_layout);
    window
        .common
        .update_window_state(event::WindowStateSource::App, |ws| {
            ws.window_id = "azcalendar-editor-2".into();
        });
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();

    let seen = state
        .borrow_mut()
        .downcast_ref::<SeenWindowId>()
        .map(|s| s.id.clone())
        .expect("the app state");
    assert_eq!(seen, "azcalendar-editor-2");
}

/// A two-item context menu.
fn copy_paste_menu() -> azul_core::menu::Menu {
    use azul_core::menu::{Menu, MenuItem, StringMenuItem};
    Menu::create(
        vec![
            MenuItem::String(StringMenuItem::create("Copy".into())),
            MenuItem::String(StringMenuItem::create("Paste".into())),
        ]
        .into(),
    )
}

/// Every text a window's root DOM shows.
fn texts_of(window: &HeadlessWindow) -> Vec<String> {
    let lw = window.common.layout_window.as_ref().expect("layout window");
    lw.layout_results
        .get(&azul_core::dom::DomId::ROOT_ID)
        .map(|r| {
            r.styled_dom
                .node_data
                .as_ref()
                .iter()
                .filter_map(|n| n.get_node_type().get_text())
                .map(|t| t.as_str().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// A laid-out window with nothing open.
fn settled_window() -> HeadlessWindow {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, one_box_layout);
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();
    window
}

/// DRIVE2: `show_menu_from_callback` was a no-op in the headless backend,
/// so no E2E script could open a context menu or a ribbon drop-down, let
/// alone click an item. A menu is now what the X11 / Wayland fallback makes
/// it: a window of its own with the menu DOM, laid out and pumped by this
/// window's loop - which the debug server reaches by `window_id`.
#[test]
fn a_menu_opened_in_a_headless_window_is_a_window_a_script_can_reach() {
    let mut window = settled_window();
    window.show_menu_from_callback(&copy_paste_menu(), LogicalPosition::new(20.0, 10.0), None);
    window.pump_children();

    assert_eq!(window.children.len(), 1, "the menu is a window of its own");
    let menu = &window.children[0];
    let ws = menu.common.current_window_state();
    assert_eq!(ws.window_id.as_str(), "azul-menu");
    assert_eq!(ws.flags.window_type, azul_core::window::WindowType::Menu);
    let texts = texts_of(menu);
    assert!(
        texts.iter().any(|t| t == "Copy") && texts.iter().any(|t| t == "Paste"),
        "the menu window shows its items: {texts:?}"
    );
}

/// A menu opened while another is open (a submenu, a second drop-down)
/// gets an id of its own, or a script could only ever reach the first.
#[test]
fn a_second_open_menu_gets_an_id_of_its_own() {
    let mut window = settled_window();
    window.show_menu_from_callback(&copy_paste_menu(), LogicalPosition::new(20.0, 10.0), None);
    window.pump_children();
    window.show_menu_from_callback(&copy_paste_menu(), LogicalPosition::new(60.0, 10.0), None);
    window.pump_children();

    let ids: Vec<String> = window
        .children
        .iter()
        .map(|c| {
            c.common
                .current_window_state()
                .window_id
                .as_str()
                .to_string()
        })
        .collect();
    assert_eq!(
        ids,
        vec!["azul-menu".to_string(), "azul-menu-2".to_string()]
    );
}

/// Layout passes so far (`FrameReport::layout_passes` counts every layout).
fn layout_passes(window: &HeadlessWindow) -> u32 {
    window
        .common
        .layout_window
        .as_ref()
        .expect("layout window")
        .frame_report
        .layout_passes
}

/// e2e/dl-text-patch, red against AzPaint and green in-process: a timer or
/// debug-server edit is laid out by `process_timers_and_threads` BEFORE it
/// raises the relayout-only request - the contract every desktop frame path
/// keeps ("the event arm already re-ran layout: skip it, paint"). The
/// headless frame laid the window out a SECOND time, and the second build -
/// with nothing left to patch - replaced the patched display list
/// (`layout_passes: 2`, `last_dl_build_patched: false`).
#[test]
fn a_relayout_only_frame_paints_the_layout_that_already_ran() {
    let mut window = settled_window();
    let before = layout_passes(&window);
    // What `process_timers_and_threads` does for an in-place edit.
    let mut debug_messages = None;
    window
        .incremental_relayout_dispatching(event::IncrementalRelayout::Restyle, &mut debug_messages)
        .expect("the relayout");
    window.common.request_relayout_only();
    assert_eq!(
        layout_passes(&window),
        before + 1,
        "harness: the edit's relayout ran"
    );

    window.service_frame(azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow);
    assert_eq!(
        layout_passes(&window),
        before + 1,
        "the frame paints the layout that already ran; it does not lay out again"
    );
}

/// A box whose background is the css-id image `e2e-live-img`.
extern "C" fn css_id_image_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_child(
        Dom::create_div()
            .with_css("width: 80px; height: 60px; background-image: url(\"e2e-live-img\");"),
    )
}

/// Does the window's display list paint an image?
fn paints_an_image(window: &HeadlessWindow) -> bool {
    window
        .common
        .layout_window
        .as_ref()
        .and_then(|lw| lw.layout_results.get(&azul_core::dom::DomId::ROOT_ID))
        .is_some_and(|r| {
            r.display_list
                .items
                .iter()
                .any(|item| matches!(item, DisplayListItem::Image { .. }))
        })
}

/// e2e/op-image-cache-id-repaints, red against AzPaint and green in-process:
/// registering an image under a css id a mounted node already references
/// owes a display-list rebuild (`ContentDirtyTier::RebuildDisplayList`; the
/// change marks the list dirty). The headless frame never consumed the flag:
/// it re-laid-out the unchanged tree, kept the cached list, painted nothing.
#[test]
fn registering_a_css_id_image_repaints_the_box_that_uses_it() {
    use azul_core::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};

    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, css_id_image_layout);
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();
    assert!(
        !paints_an_image(&window),
        "harness: nothing is registered under the id yet"
    );

    let image = ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(vec![255u8; 16 * 16 * 4].into()),
        width: 16,
        height: 16,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: b"headless6-solid".to_vec().into(),
    })
    .expect("a solid image");
    let tier = window.apply_user_change(&azul_layout::callbacks::CallbackChange::AddImageToCache {
        id: "e2e-live-img".into(),
        image,
    });
    assert_eq!(
        tier,
        azul_core::events::ProcessEventResult::ShouldUpdateDisplayListCurrentWindow,
        "harness: the registration owes a display-list rebuild"
    );
    // What the loop's Phase 2 does once a timer (the debug server's) applied it.
    window.service_frame(azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow);

    assert!(
        paints_an_image(&window),
        "the box that references the id paints the image the frame after it was registered"
    );
}

/// A spinner: keyframe tracks that run for as long as it is shown.
extern "C" fn spinner_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_child(azul_layout::widgets::spinner::Spinner::create().dom())
}

/// A laid-out spinner window whose animation clock is `scripted` or live,
/// with its animation drivers armed as a pass end arms them.
fn spinner_window(scripted: bool) -> HeadlessWindow {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, spinner_layout);
    window.common.scripted_animation_clock = scripted;
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();
    window.arm_animation_drivers_if_needed();
    window
}

fn css_driver_armed(window: &HeadlessWindow) -> bool {
    window.common.layout_window.as_ref().is_some_and(|lw| {
        lw.timers
            .contains_key(&azul_core::task::CSS_ANIMATION_TIMER_ID)
    })
}

/// e2e/css-animation-multi, red against AzPaint and green in-process: under
/// `AZ_E2E` the wall-clock CSS animation driver stepped the transitions
/// between two ops, one frame per turn of the loop, on top of the
/// scenario's own `tick_animations` (197.333 / 117.336 for 200 / 120). A
/// scripted run owns the animation clock, as the in-process runner's frozen
/// clock does: the live driver is never armed.
#[test]
fn a_scripted_run_owns_the_animation_clock() {
    assert!(
        css_driver_armed(&spinner_window(false)),
        "harness: a live window arms the wall-clock driver for its spinner"
    );
    assert!(
        !css_driver_armed(&spinner_window(true)),
        "a scripted run's animations move only with its tick_animations"
    );
}

// ---- EVENTS7: a headless menu closes the way a desktop menu does -----------
//
// HEADLESS6 left it open: a script could close a headless menu only by
// clicking one of its items (or `close` with its window id). A desktop menu
// also closes on Escape and on a press outside it - X11 through its pointer
// grab, macOS and Win32 natively - and a menu CHAIN (a menu and the submenu
// it opened) goes as one.

/// `window` with two menus open - a menu and a second one, as its submenu
/// would be: the chain.
fn window_with_two_open_menus() -> HeadlessWindow {
    let mut window = settled_window();
    window.show_menu_from_callback(&copy_paste_menu(), LogicalPosition::new(20.0, 10.0), None);
    window.pump_children();
    window.show_menu_from_callback(&copy_paste_menu(), LogicalPosition::new(60.0, 10.0), None);
    window.pump_children();
    assert_eq!(window.children.len(), 2, "harness: two menus are open");
    window
}

/// An Escape press in `window`, the way a backend's key handler runs it.
fn press_escape(window: &mut HeadlessWindow) {
    use azul_core::window::{OptionVirtualKeyCode, VirtualKeyCode};
    window.snapshot_window_state_baseline("test.escape");
    let keyboard = window.common.keyboard_state_mut();
    keyboard
        .pressed_virtual_keycodes
        .insert_hm_item(VirtualKeyCode::Escape);
    keyboard.current_virtual_keycode = OptionVirtualKeyCode::Some(VirtualKeyCode::Escape);
    keyboard.sync_modifiers();
    let _ = window.process_window_events(0);
}

#[test]
fn escape_in_a_headless_menu_closes_the_menu_and_its_chain() {
    let mut window = window_with_two_open_menus();
    press_escape(&mut window.children[1]);
    window.pump_children();
    assert!(
        window.children.is_empty(),
        "Escape leaves the menu, and the chain with it: {} menu(s) still open",
        window.children.len()
    );
}

#[test]
fn escape_in_the_window_that_owns_open_menus_closes_them() {
    let mut window = window_with_two_open_menus();
    press_escape(&mut window);
    window.pump_children();
    assert!(
        window.children.is_empty(),
        "an Escape that reached the owner leaves its menus: {} still open",
        window.children.len()
    );
}

/// `body > div` (300 x 200) whose presses and releases are counted.
extern "C" fn counted_box_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::events::HoverEventFilter;
    let counter = data
        .downcast_ref::<Arc<core::sync::atomic::AtomicUsize>>()
        .map(|c| c.clone())
        .expect("the counter");
    Dom::create_body().with_child(
        Dom::create_div()
            .with_css("width: 300px; height: 200px;")
            .with_callbacks(
                vec![
                    counting_callback(HoverEventFilter::MouseDown, &counter),
                    counting_callback(HoverEventFilter::MouseUp, &counter),
                ]
                .into(),
            ),
    )
}

#[test]
fn a_press_outside_open_menus_closes_them_and_reaches_nothing_under_it() {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use azul_core::events::MouseButton;

    let counted = Arc::new(AtomicUsize::new(0));
    let state = Arc::new(RefCell::new(RefAny::new(counted.clone())));
    let mut window = make_window_with(&state, counted_box_layout);
    window.regenerate_layout().expect("a layout pass");
    let _ = window.common.take_regeneration();
    window.show_menu_from_callback(&copy_paste_menu(), LogicalPosition::new(20.0, 10.0), None);
    window.pump_children();
    assert_eq!(window.children.len(), 1, "harness: a menu is open");

    step(&mut window, HeadlessEvent::MouseMove { x: 150.0, y: 100.0 });
    step(
        &mut window,
        HeadlessEvent::MouseDown {
            button: MouseButton::Left,
        },
    );
    step(
        &mut window,
        HeadlessEvent::MouseUp {
            button: MouseButton::Left,
        },
    );
    window.pump_children();
    assert!(
        window.children.is_empty(),
        "a press outside the menu closes it"
    );
    assert_eq!(
        counted.load(Ordering::SeqCst),
        0,
        "the click that leaves a menu reaches nothing under it - neither its press nor its release"
    );

    // The menu is gone: the next click is the window's again.
    step(
        &mut window,
        HeadlessEvent::MouseDown {
            button: MouseButton::Left,
        },
    );
    step(
        &mut window,
        HeadlessEvent::MouseUp {
            button: MouseButton::Left,
        },
    );
    assert_eq!(
        counted.load(Ordering::SeqCst),
        2,
        "the box hears the next press and its release"
    );
}

/// What [`record_page_layout`] shows: an AzERP record with its delete
/// question open.
struct RecordPage {
    deleted: bool,
}

/// The question's Delete: the record goes, the page must follow.
extern "C" fn answer_delete(
    mut data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(mut page) = data.downcast_mut::<RecordPage>() {
        page.deleted = true;
    }
    azul_core::callbacks::Update::RefreshDom
}

/// `body > p` with the record's name and, while it exists, the delete
/// question: a `<transient-window open>` covering the viewport with no
/// light-dismiss - what a `Modal` is - whose only content is the 120x40
/// Delete button at its top-left.
extern "C" fn record_page_layout(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    record_page(data, answer_delete as usize)
}

/// [`record_page_layout`] with `answer` behind the question's Delete.
fn record_page(mut data: RefAny, answer: usize) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{NodeData, NodeType},
        events::{EventFilter, HoverEventFilter},
        refany::OptionRefAny,
        transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
    };
    let deleted = data.downcast_ref::<RecordPage>().is_some_and(|p| p.deleted);
    let page = Dom::create_body().with_child(Dom::create_p_with_text(if deleted {
        "Deleted"
    } else {
        "Drill press"
    }));
    if deleted {
        return page;
    }
    let delete_button = Dom::create_div()
        .with_css("width: 120px; height: 40px;")
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: answer,
                    ctx: OptionRefAny::None,
                },
                refany: data.clone(),
            }]
            .into(),
        );
    let question = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(
        TransientWindowConfig::opened()
            .with_anchor(TransientAnchor::Viewport)
            .with_dismiss(TransientDismiss::None),
    )))
    .with_child(delete_button);
    page.with_child(Dom::create_div().with_child(question))
}

/// The debug server's `click` op, as its timer runs it in the window a
/// script names: move, press and release at (60, 20) - the middle of the
/// question's Delete button - queued as one state sequence.
extern "C" fn debug_click_timer(
    _data: RefAny,
    mut info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::window::CursorPosition;
    let mut moved = info.callback_info.get_current_window_state().clone();
    moved.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(60.0, 20.0));
    let mut down = moved.clone();
    down.mouse_state.left_down = true;
    let mut up = down.clone();
    up.mouse_state.left_down = false;
    info.callback_info
        .queue_window_state_sequence(vec![moved, down, up].into());
    azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
}

/// R2-APPS, AzERP: a `Modal` is a transient window of its own, and its
/// content is its OWNER's extracted subtree. A script's click on the delete
/// question's Delete ran the callback - the record and its file were gone -
/// but the main window was never rebuilt: the question and the deleted
/// record stayed on screen until a `redraw` op (the check-out form likewise
/// saved, and the page never read "Checked out").
///
/// The popup's pass DOES ask for every window (a refresh inside a popup is a
/// refresh of the owner, `ShouldRegenerateDomAllWindows`), but a click a
/// script sends arrives as the debug server's TIMER change, and the shared
/// `process_timers_and_threads` only fanned out an `Update` a timer
/// returned, never the result of the pass its changes ran.
#[test]
fn a_modal_button_that_changes_app_state_rebuilds_its_parent_window() {
    let (state, root) = click_the_questions_delete(record_page_layout);
    let deleted = state
        .borrow_mut()
        .downcast_ref::<RecordPage>()
        .is_some_and(|p| p.deleted);
    assert!(deleted, "harness: the click ran the question's Delete");
    let texts = texts_of(&root);
    assert!(
        texts.iter().any(|t| t == "Deleted") && !texts.iter().any(|t| t == "Drill press"),
        "the main window was rebuilt for the Delete inside its modal: {texts:?}"
    );
    assert!(
        root.children.is_empty(),
        "the question leaves with the rebuild that dropped it: {} window(s) still open",
        root.children.len()
    );
}

/// The record page under `layout` with its question open in a window of its own, after a
/// script's click on the question's Delete and six turns of the loop.
fn click_the_questions_delete(
    layout: azul_core::callbacks::LayoutCallbackType,
) -> (Arc<RefCell<RefAny>>, HeadlessWindow) {
    let state = Arc::new(RefCell::new(RefAny::new(RecordPage { deleted: false })));
    let mut root = make_window_with(&state, layout);
    root.regenerate_layout().expect("the page's first layout");
    let _ = root.common.take_regeneration();
    root.pump_children();
    assert_eq!(
        root.children.len(),
        1,
        "harness: the question is a window of its own"
    );
    assert_eq!(
        root.children[0]
            .common
            .current_window_state()
            .window_id
            .as_str(),
        "azul-transient",
        "harness: the question's window is the transient window"
    );

    let get_time = azul_core::task::GetSystemTimeCallback {
        cb: azul_core::task::get_system_time_libstd,
    };
    root.children[0].start_timer(
        azul_core::task::TimerId::unique().id,
        azul_layout::timer::Timer::create(
            RefAny::new(()),
            debug_click_timer as azul_layout::timer::TimerCallbackType,
            get_time,
        ),
    );
    for _ in 0..6 {
        root.pump_children();
        root.pump_once(true);
    }
    (state, root)
}

/// A "Don't Save" in a close guard's question: the edits go, and so does the window.
extern "C" fn answer_discard(
    mut data: RefAny,
    mut info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(mut page) = data.downcast_mut::<RecordPage>() {
        page.deleted = true;
    }
    info.close_window();
    azul_core::callbacks::Update::RefreshDom
}

extern "C" fn discard_page_layout(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    record_page(data, answer_discard as usize)
}

/// A callback in a modal that closes "the window" closes the window the modal belongs to.
/// The modal's popup only shows a subtree of its owner's DOM: its callbacks are the owner's,
/// and so is the window they mean - a refresh they ask for already is the owner's. The
/// close closed the popup instead: the `CloseGuard`'s "Don't Save" (`close_window` in its
/// answer) left AzCalendar's editor window open on "This appointment is closed.", and the
/// next editor, under the same window id, was out of a script's reach (E2E-A, 2026-10-06).
#[test]
fn a_close_asked_from_a_modal_closes_the_window_that_owns_it() {
    let (state, root) = click_the_questions_delete(discard_page_layout);
    let discarded = state
        .borrow_mut()
        .downcast_ref::<RecordPage>()
        .is_some_and(|p| p.deleted);
    assert!(discarded, "harness: the click ran the question's answer");
    assert!(
        !root.is_open() || root.common.current_window_state().flags.close_requested,
        "the window that owns the modal closes"
    );
}

/// The thread [`answer_on_a_thread`] started has written back.
static WRITTEN_BACK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn work_on_a_thread(
    _init: RefAny,
    mut sender: azul_layout::thread::ThreadSender,
    _receiver: azul_core::task::ThreadReceiver,
) {
    // Work that outlasts the answer's pass: the modal is gone before it ends.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let _sent = sender.send(azul_layout::thread::ThreadReceiveMsg::WriteBack(
        azul_layout::thread::ThreadWriteBackMsg::new(
            work_written_back as azul_layout::thread::WriteBackCallbackType,
            RefAny::new(()),
        ),
    ));
}

extern "C" fn work_written_back(
    _data: RefAny,
    _back: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    WRITTEN_BACK.store(true, Ordering::SeqCst);
    azul_core::callbacks::Update::RefreshDom
}

/// The question's answer closes the question and leaves its work to a thread: AzPhoto's
/// export sheet (OK), AzWriter's close guard (Save, then close once saved).
extern "C" fn answer_on_a_thread(
    mut data: RefAny,
    mut info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(mut page) = data.downcast_mut::<RecordPage>() {
        page.deleted = true;
    }
    info.add_thread(
        azul_core::task::ThreadId::unique(),
        azul_layout::thread::Thread::create(
            RefAny::new(()),
            data.clone(),
            work_on_a_thread as azul_layout::thread::ThreadCallbackType,
        ),
    );
    azul_core::callbacks::Update::RefreshDom
}

extern "C" fn thread_page_layout(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    record_page(data, answer_on_a_thread as usize)
}

/// A thread a modal's callback starts is its OWNER's: the modal's popup only shows the
/// owner's subtree, its callbacks run on the owner's data, and the popup goes as soon as the
/// answer closes the modal. The thread was the popup's and went with it - AzPhoto's export
/// sheet printed "Exporting Sample.png..." and never finished; AzWriter's close guard saved
/// (Save) but its "close once saved" never came, the window stayed open (E2E sweep,
/// 2026-10-06).
#[test]
fn a_thread_a_modals_answer_starts_writes_back_after_the_modal_closed() {
    WRITTEN_BACK.store(false, Ordering::SeqCst);
    let (state, mut root) = click_the_questions_delete(thread_page_layout);
    let answered = state
        .borrow_mut()
        .downcast_ref::<RecordPage>()
        .is_some_and(|p| p.deleted);
    assert!(answered, "harness: the click ran the question's answer");
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !WRITTEN_BACK.load(Ordering::SeqCst) && std::time::Instant::now() < end {
        root.pump_children();
        root.pump_once(true);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        WRITTEN_BACK.load(Ordering::SeqCst),
        "the thread the modal's answer started wrote back after the modal closed ({} window(s) \
         open besides the owner)",
        root.children.len()
    );
}

/// The menu item [`start_work_from_a_menu_item`] was picked.
static MENU_ITEM_PICKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The thread it started has written back.
static MENU_WORK_WRITTEN_BACK: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

extern "C" fn menu_work_on_a_thread(
    _init: RefAny,
    mut sender: azul_layout::thread::ThreadSender,
    _receiver: azul_core::task::ThreadReceiver,
) {
    // Work that outlasts the pick: the menu is gone before it ends (AzDrive's search waits for
    // the typing to pause first).
    std::thread::sleep(std::time::Duration::from_millis(300));
    let _sent = sender.send(azul_layout::thread::ThreadReceiveMsg::WriteBack(
        azul_layout::thread::ThreadWriteBackMsg::new(
            menu_work_written_back as azul_layout::thread::WriteBackCallbackType,
            RefAny::new(()),
        ),
    ));
}

extern "C" fn menu_work_written_back(
    _data: RefAny,
    _back: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    MENU_WORK_WRITTEN_BACK.store(true, Ordering::SeqCst);
    azul_core::callbacks::Update::RefreshDom
}

/// A menu item's callback that leaves the app's work to a thread: AzDrive's Saved searches
/// (the search it runs again), its Refine menus (the search with the new choice).
extern "C" fn start_work_from_a_menu_item(
    data: RefAny,
    mut info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    MENU_ITEM_PICKED.store(true, Ordering::SeqCst);
    info.add_thread(
        azul_core::task::ThreadId::unique(),
        azul_layout::thread::Thread::create(
            RefAny::new(()),
            data.clone(),
            menu_work_on_a_thread as azul_layout::thread::ThreadCallbackType,
        ),
    );
    azul_core::callbacks::Update::RefreshDom
}

/// A thread a menu item's callback starts is the menu's OWNER's, as a modal's is: the item's
/// callback runs in the menu's own window (headless, and the X11 / Wayland fallback), which
/// closes as the item is picked, and the window that owns the menu dropped the closed menu
/// with its threads - their answers never came. AzDrive's Saved searches ran the saved search
/// from the menu and it never reported (E2E step 23, 2026-10-10): the folder's listing, a
/// thread of a few milliseconds, answered before the menu was dropped; the search, which waits
/// for the typing to pause, did not.
#[test]
fn a_thread_a_menu_items_callback_starts_writes_back_after_the_menu_closed() {
    use azul_core::{
        events::MouseButton,
        menu::{Menu, MenuItem, StringMenuItem},
    };

    MENU_ITEM_PICKED.store(false, Ordering::SeqCst);
    MENU_WORK_WRITTEN_BACK.store(false, Ordering::SeqCst);
    let mut window = settled_window();
    let menu = Menu::create(
        vec![MenuItem::String(
            StringMenuItem::create("Search again".into())
                .with_callback(RefAny::new(()), start_work_from_a_menu_item as usize),
        )]
        .into(),
    );
    window.show_menu_from_callback(&menu, LogicalPosition::new(20.0, 10.0), None);
    window.pump_children();
    assert_eq!(window.children.len(), 1, "harness: the menu is open");

    // The middle of the first item: below the frame's line and padding, half an item down.
    let metrics =
        crate::desktop::menu_renderer::MenuMetrics::from_system_style(&window.common.system_style);
    let x = metrics.border_width + metrics.pad_h + 8.0;
    let y = metrics.border_width + metrics.frame_pad_v + metrics.item_height / 2.0;
    step(&mut window.children[0], HeadlessEvent::MouseMove { x, y });
    step(
        &mut window.children[0],
        HeadlessEvent::MouseDown {
            button: MouseButton::Left,
        },
    );
    step(
        &mut window.children[0],
        HeadlessEvent::MouseUp {
            button: MouseButton::Left,
        },
    );
    assert!(
        MENU_ITEM_PICKED.load(Ordering::SeqCst),
        "harness: the press picked the menu's item"
    );
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !MENU_WORK_WRITTEN_BACK.load(Ordering::SeqCst) && std::time::Instant::now() < end {
        window.pump_children();
        window.pump_once(true);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        window.children.is_empty(),
        "harness: the menu closed when its item was picked"
    );
    assert!(
        MENU_WORK_WRITTEN_BACK.load(Ordering::SeqCst),
        "the thread the menu item's callback started wrote back after the menu closed"
    );
}

/// E2E-C, AzReview: every `<transient-window>` is a `WindowType::Menu` window
/// (`transient::popup_window_state` - borderless, on top, parent-owned), and
/// the headless owner took every Menu-type child for a window-based MENU: an
/// Escape (or a press) that reached the owner closed an open modal as it
/// closes a menu - silently, with no `Dismissed` for the widget, and with the
/// key spent. A window-based menu is a Menu window WITHOUT a mailbox (the rule
/// `process_transient_dismissal` already uses); a modal's Escape is its own.
#[test]
fn an_escape_in_the_owner_of_an_open_modal_does_not_close_it_as_a_menu() {
    let state = Arc::new(RefCell::new(RefAny::new(RecordPage { deleted: false })));
    let mut root = make_window_with(&state, record_page_layout);
    root.regenerate_layout().expect("the page's first layout");
    let _ = root.common.take_regeneration();
    root.pump_children();
    assert_eq!(root.children.len(), 1, "harness: the question is open");

    press_escape(&mut root);
    root.pump_children();
    assert_eq!(
        root.children.len(),
        1,
        "the question has no Escape of its own (dismiss=none), so it stays open: the owner \
         closed it as if it were a menu"
    );
}

/// What [`about_page_layout`] shows: azul-appkit's settings page with its
/// About box, a `Modal` the app keeps open while `open` says so.
struct AboutPage {
    open: bool,
    closes: usize,
}

/// The Modal's `on_close`: the app drops its flag (azul-appkit's
/// `on_about_close` prints `<APP>_ABOUT closed` here).
extern "C" fn about_closed(
    mut data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
    state: azul_layout::widgets::modal::ModalState,
) -> azul_core::callbacks::Update {
    if let Some(mut page) = data.downcast_mut::<AboutPage>() {
        page.open = state.open;
        page.closes += 1;
    }
    azul_core::callbacks::Update::RefreshDom
}

/// `body > p "Settings"` and the About box: a `Modal` holding one line.
extern "C" fn about_page_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_layout::widgets::modal::{Modal, ModalOnCloseCallbackType};
    let open = data.downcast_ref::<AboutPage>().is_some_and(|p| p.open);
    let on_close: ModalOnCloseCallbackType = about_closed;
    Dom::create_body()
        .with_child(Dom::create_p_with_text("Settings"))
        .with_child(
            Modal::create(Dom::create_p_with_text("AzReview 0.1.0"))
                .with_title("About AzReview".into())
                .with_open(open)
                .with_on_close(data.clone(), on_close)
                .dom(),
        )
}

/// E2E-C, AzReview: a script opens the About box and presses Escape. The key
/// reaches the app's first window (the debug server's default), which hands
/// it to the modal - the popup that holds the keyboard - and the modal's own
/// Escape closes it and tells the app through `on_close`. Headless did
/// neither: the owner closed the modal as a menu (see above), and a key
/// forwarded to a popup waited for a pass the headless loop never ran (the
/// desktop backends run it at once, `deliver_forwarded_keys`).
#[test]
fn escape_in_the_owner_of_a_modal_dialog_closes_it_and_tells_the_app() {
    let state = Arc::new(RefCell::new(RefAny::new(AboutPage {
        open: true,
        closes: 0,
    })));
    let mut root = make_window_with(&state, about_page_layout);
    root.regenerate_layout().expect("the page's first layout");
    let _ = root.common.take_regeneration();
    root.pump_children();
    assert_eq!(
        root.children.len(),
        1,
        "harness: the About box is a window of its own"
    );
    root.pump_children();

    press_escape(&mut root);
    for _ in 0..6 {
        root.pump_children();
        root.pump_once(true);
    }

    let (open, closes) = state
        .borrow_mut()
        .downcast_ref::<AboutPage>()
        .map(|p| (p.open, p.closes))
        .expect("the page");
    assert_eq!(
        (open, closes),
        (false, 1),
        "the modal's Escape closed it and its on_close told the app, once"
    );
    assert!(
        root.children.is_empty(),
        "the About box is gone: {} window(s) still open",
        root.children.len()
    );
}

/// What [`drag_and_drop_layout`] counts: the source's DragStarts, the
/// target's Drops.
struct DragAndDrop {
    starts: usize,
    drops: usize,
}

extern "C" fn dnd_drag_start(
    mut data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(mut d) = data.downcast_mut::<DragAndDrop>() {
        d.starts += 1;
    }
    azul_core::callbacks::Update::DoNothing
}

extern "C" fn dnd_drag_over(
    _data: RefAny,
    mut info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    info.accept_drop();
    azul_core::callbacks::Update::DoNothing
}

extern "C" fn dnd_drop(
    mut data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(mut d) = data.downcast_mut::<DragAndDrop>() {
        d.drops += 1;
    }
    azul_core::callbacks::Update::DoNothing
}

/// `body` (8 px UA margin) with a draggable 120x40 source at the top and a
/// 120x40 drop target 60 px under it - AzTasks' planned month in miniature
/// (a task dragged onto a day).
extern "C" fn drag_and_drop_layout(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::AttributeType,
        events::{EventFilter, HoverEventFilter},
        refany::OptionRefAny,
    };
    let on = |event: HoverEventFilter, cb: usize| CoreCallbackData {
        event: EventFilter::Hover(event),
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany: data.clone(),
    };
    Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_css("width: 120px; height: 40px;")
                .with_attribute(AttributeType::Draggable(true))
                .with_callbacks(
                    vec![on(HoverEventFilter::DragStart, dnd_drag_start as usize)].into(),
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css("width: 120px; height: 40px; margin-top: 60px;")
                .with_callbacks(
                    vec![
                        on(HoverEventFilter::DragOver, dnd_drag_over as usize),
                        on(HoverEventFilter::Drop, dnd_drop as usize),
                    ]
                    .into(),
                ),
        )
}

/// The debug server's `mouse_down` / `mouse_move` / `mouse_up` ops, as a
/// script drags: a press on the source at (60, 28), moves down a frame each,
/// the release on the target at (60, 128).
extern "C" fn debug_drag_timer(
    _data: RefAny,
    mut info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::window::CursorPosition;
    let at = |state: &azul_layout::window_state::FullWindowState, y: f32, down: bool| {
        let mut s = state.clone();
        s.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(60.0, y));
        s.mouse_state.left_down = down;
        s
    };
    let base = info.callback_info.get_current_window_state().clone();
    let mut states = vec![at(&base, 28.0, false), at(&base, 28.0, true)];
    for y in [34.0, 50.0, 80.0, 110.0, 128.0] {
        states.push(at(&base, y, true));
    }
    states.push(at(&base, 128.0, false));
    info.callback_info
        .queue_window_state_sequence(states.into());
    azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
}

/// E2E-C, AzTasks / AzShow: a script's drag never became a drag. The debug
/// server's pointer ops arrive as window-state changes (`ModifyWindowState`,
/// `QueueWindowStateSequence`), which ran the event pass but never fed the
/// gesture manager - every backend's mouse handler does
/// (`record_input_sample`) - so no input session existed, `detect_drag`
/// never saw one, no `DragStart` fired and a held-button move was only a
/// text-selection drag: AzTasks' planned month and board, and AzShow's slide
/// sorter, got no drop.
#[test]
fn a_scripted_drag_from_a_draggable_node_drops_on_the_target_under_the_release() {
    let state = Arc::new(RefCell::new(RefAny::new(DragAndDrop {
        starts: 0,
        drops: 0,
    })));
    let mut window = make_window_with(&state, drag_and_drop_layout);
    window.regenerate_layout().expect("the first layout");
    let _ = window.common.take_regeneration();

    let get_time = azul_core::task::GetSystemTimeCallback {
        cb: azul_core::task::get_system_time_libstd,
    };
    window.start_timer(
        azul_core::task::TimerId::unique().id,
        azul_layout::timer::Timer::create(
            RefAny::new(()),
            debug_drag_timer as azul_layout::timer::TimerCallbackType,
            get_time,
        ),
    );
    for _ in 0..4 {
        window.pump_once(true);
    }

    let (starts, drops) = state
        .borrow_mut()
        .downcast_ref::<DragAndDrop>()
        .map(|d| (d.starts, d.drops))
        .expect("the counters");
    assert_eq!(
        starts, 1,
        "the press on the source and the moves past the threshold are a drag"
    );
    assert_eq!(drops, 1, "the release over the target drops there");
}

/// How many DoubleClicks [`double_click_layout`]'s box heard.
struct DoubleClicks {
    count: usize,
}

extern "C" fn count_double_click(
    mut data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(mut d) = data.downcast_mut::<DoubleClicks>() {
        d.count += 1;
    }
    azul_core::callbacks::Update::DoNothing
}

/// `body` with one 120x40 box that counts its DoubleClicks (AzReader's
/// library tile opens its book on one).
extern "C" fn double_click_layout(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        events::{EventFilter, HoverEventFilter},
        refany::OptionRefAny,
    };
    Dom::create_body().with_child(
        Dom::create_div()
            .with_css("width: 120px; height: 40px;")
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DoubleClick),
                    callback: CoreCallback {
                        cb: count_double_click as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: data.clone(),
                }]
                .into(),
            ),
    )
}

/// The debug server's `double_click` op: two press / release cycles at the
/// box's centre, each its own window-state change.
extern "C" fn debug_double_click_timer(
    _data: RefAny,
    mut info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::window::CursorPosition;
    let mut state = info.callback_info.get_current_window_state().clone();
    state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(60.0, 28.0));
    for _ in 0..2 {
        state.mouse_state.left_down = true;
        info.callback_info.modify_window_state(state.clone());
        state.mouse_state.left_down = false;
        info.callback_info.modify_window_state(state.clone());
    }
    azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
}

/// E2E-C: with the scripted pointer feeding the gesture sessions (see the
/// drag above), the `double_click` op's two press / release cycles ARE a
/// double click - exactly one, raised by the release that completes it. The
/// op also injected a native DoubleClick (its stand-in while scripts fed no
/// sessions), which would now be a second one.
#[test]
fn two_scripted_press_release_cycles_on_one_spot_are_one_double_click() {
    let state = Arc::new(RefCell::new(RefAny::new(DoubleClicks { count: 0 })));
    let mut window = make_window_with(&state, double_click_layout);
    window.regenerate_layout().expect("the first layout");
    let _ = window.common.take_regeneration();

    let get_time = azul_core::task::GetSystemTimeCallback {
        cb: azul_core::task::get_system_time_libstd,
    };
    window.start_timer(
        azul_core::task::TimerId::unique().id,
        azul_layout::timer::Timer::create(
            RefAny::new(()),
            debug_double_click_timer as azul_layout::timer::TimerCallbackType,
            get_time,
        ),
    );
    for _ in 0..4 {
        window.pump_once(true);
    }

    let count = state
        .borrow_mut()
        .downcast_ref::<DoubleClicks>()
        .map(|d| d.count)
        .expect("the counter");
    assert_eq!(
        count, 1,
        "two press / release cycles on one spot are one double click"
    );
}

/// [`selecting_tile_layout`]'s state: the tile's selection and its opens.
struct SelectingTile {
    selected: bool,
    opened: usize,
}

extern "C" fn select_the_tile(
    mut data: RefAny,
    mut info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    // AzDrive's `on_place_click`: the click is the tile's alone.
    info.stop_propagation();
    if let Some(mut t) = data.downcast_mut::<SelectingTile>() {
        t.selected = true;
    }
    azul_core::callbacks::Update::RefreshDom
}

extern "C" fn open_the_tile(
    mut data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(mut t) = data.downcast_mut::<SelectingTile>() {
        t.opened += 1;
    }
    azul_core::callbacks::Update::RefreshDom
}

/// `body` with a focusable 120x40 tile at the top-left that a click SELECTS (the page is
/// rebuilt, the click stops there) and a double-click OPENS - AzDrive's drive tile. A selected tile's page holds more nodes BEFORE
/// it (the ribbon's commands for a selection; here two hidden ones), so the tile's node ids
/// move with the rebuild the first click asked for.
extern "C" fn selecting_tile_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        events::{EventFilter, HoverEventFilter},
        refany::OptionRefAny,
    };
    let selected = data.downcast_ref::<SelectingTile>().is_some_and(|t| t.selected);
    let hook = |filter: HoverEventFilter, cb: usize| CoreCallbackData {
        event: EventFilter::Hover(filter),
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany: data.clone(),
    };
    let tile = Dom::create_div()
        .with_css("width: 120px; height: 40px;")
        .with_tab_index(azul_core::dom::TabIndex::Auto)
        .with_callbacks(
            vec![
                hook(HoverEventFilter::Click, select_the_tile as usize),
                hook(HoverEventFilter::DoubleClick, open_the_tile as usize),
            ]
            .into(),
        )
        .with_child(Dom::create_div().with_css("width: 24px; height: 24px;"));
    let mut body = Dom::create_body();
    if selected {
        body.add_child(Dom::create_div().with_css("display: none;"));
        body.add_child(Dom::create_div().with_css("display: none;"));
    }
    // The pane around the tile takes clicks too (a click on its empty space clears the
    // selection): the tile's click stops before it.
    let pane = Dom::create_div()
        .with_callbacks(vec![hook(HoverEventFilter::Click, select_the_tile as usize)].into())
        .with_child(tile);
    body.with_child(pane)
}

/// E2E sweep, 2026-10-06, AzDrive: a double-click on a drive tile never opened the drive. The
/// second release raises a Click and the DoubleClick in one pass; the tile's click handler
/// stops the CLICK's propagation, and the dispatcher, on reaching the pane's click handler,
/// ended the whole pass - the DoubleClick planned after it never ran. A `stopPropagation`
/// ends the propagation of its own event only.
#[test]
fn a_click_that_stops_its_propagation_leaves_the_double_click_of_the_same_release_alone() {
    let state = Arc::new(RefCell::new(RefAny::new(SelectingTile {
        selected: false,
        opened: 0,
    })));
    let mut window = make_window_with(&state, selecting_tile_layout);
    window.regenerate_layout().expect("the first layout");
    let _ = window.common.take_regeneration();

    let get_time = azul_core::task::GetSystemTimeCallback {
        cb: azul_core::task::get_system_time_libstd,
    };
    window.start_timer(
        azul_core::task::TimerId::unique().id,
        azul_layout::timer::Timer::create(
            RefAny::new(()),
            debug_double_click_timer as azul_layout::timer::TimerCallbackType,
            get_time,
        ),
    );
    for _ in 0..4 {
        window.pump_once(true);
    }

    let (selected, opened) = state
        .borrow_mut()
        .downcast_ref::<SelectingTile>()
        .map(|t| (t.selected, t.opened))
        .expect("the tile's state");
    assert!(selected, "harness: the first click selected the tile");
    assert_eq!(opened, 1, "the double-click opened the tile once");
}

/// The question's "Close": what a CloseGuard's Don't Save does, and what
/// AzWriter's save write-back does after a Save the question started.
extern "C" fn close_from_the_question(
    _data: RefAny,
    mut info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    info.close_window();
    azul_core::callbacks::Update::DoNothing
}

/// `body > p` and a `<transient-window open>` covering the viewport with no
/// light-dismiss - a `Modal` - whose only content is a 120x40 button at its
/// top-left that closes the window.
extern "C" fn closing_question_layout(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{NodeData, NodeType},
        events::{EventFilter, HoverEventFilter},
        refany::OptionRefAny,
        transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
    };
    let close_button = Dom::create_div()
        .with_css("width: 120px; height: 40px;")
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: close_from_the_question as usize,
                    ctx: OptionRefAny::None,
                },
                refany: data.clone(),
            }]
            .into(),
        );
    let question = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(
        TransientWindowConfig::opened()
            .with_anchor(TransientAnchor::Viewport)
            .with_dismiss(TransientDismiss::None),
    )))
    .with_child(close_button);
    Dom::create_body()
        .with_child(Dom::create_p_with_text("Report.md"))
        .with_child(Dom::create_div().with_child(question))
}

/// E2E-C, AzWriter: "Save changes?" > Save saved the document, and the
/// write-back's `close_window()` closed - the question's own window. A
/// Modal's content is its OWNER's subtree (`common::transient`): a callback
/// there that closes "the window" means the window the user sees it in, the
/// owner; the popup goes with it. (The CloseGuard's Don't Save calls
/// `close_window()` from inside its question too.)
#[test]
fn close_window_from_inside_a_modal_closes_the_window_that_owns_it() {
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut root = make_window_with(&state, closing_question_layout);
    root.regenerate_layout().expect("the page's first layout");
    let _ = root.common.take_regeneration();
    root.pump_children();
    assert_eq!(
        root.children.len(),
        1,
        "harness: the question is a window of its own"
    );

    let get_time = azul_core::task::GetSystemTimeCallback {
        cb: azul_core::task::get_system_time_libstd,
    };
    root.children[0].start_timer(
        azul_core::task::TimerId::unique().id,
        azul_layout::timer::Timer::create(
            RefAny::new(()),
            debug_click_timer as azul_layout::timer::TimerCallbackType,
            get_time,
        ),
    );
    for _ in 0..8 {
        if !root.is_open() {
            break;
        }
        root.pump_children();
        root.pump_once(true);
    }
    assert!(
        !root.is_open(),
        "close_window() inside the modal closes the window that owns it ({} popup(s) open)",
        root.children.len()
    );
}
