//! A file drop is one event, however many passes the window runs for it.
//!
//! The OS hands the window one drop (macOS `performDragOperation:`, Windows
//! `IDropTarget::Drop`, XDND / Wayland `drop`) with every dropped file in it.
//! An app whose drop handler rebuilds the page makes the event pass run again
//! for the rebuilt DOM, and that pass must not deliver the same drop a second
//! time: AzDrive uploaded the dropped file once per pass and raised a notice
//! for each upload. A drag moving over the window is one `HoveredFile` per
//! move, and the drop ends the hover.

use std::sync::Mutex;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    events::{EventFilter, WindowEventFilter},
    refany::OptionRefAny,
};

use super::*;

/// What the window's drop and hover callbacks heard.
#[derive(Debug, Default)]
struct Heard {
    /// The files of each `DroppedFile` call, in order.
    drops: Vec<Vec<String>>,
    /// How many `HoveredFile` calls ran.
    hovers: usize,
}

/// The app state: the log the callbacks write. Every callback rebuilds the
/// page, as an app does that shows the dropped files.
#[derive(Debug, Clone, Default)]
struct DropPage {
    heard: Arc<Mutex<Heard>>,
}

extern "C" fn on_drop(mut data: RefAny, info: azul_layout::callbacks::CallbackInfo) -> Update {
    if let Some(page) = data.downcast_ref::<DropPage>() {
        let files = info
            .get_dropped_files()
            .as_ref()
            .iter()
            .map(|f| f.as_str().to_string())
            .collect();
        page.heard.lock().unwrap().drops.push(files);
    }
    Update::RefreshDom
}

extern "C" fn on_hover(mut data: RefAny, _info: azul_layout::callbacks::CallbackInfo) -> Update {
    if let Some(page) = data.downcast_ref::<DropPage>() {
        page.heard.lock().unwrap().hovers += 1;
    }
    Update::RefreshDom
}

/// `body` with a window-wide drop and hover callback, and a line that says
/// how many drops arrived (so every rebuild changes the page).
extern "C" fn drop_page_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let page = data
        .downcast_ref::<DropPage>()
        .map(|p| (*p).clone())
        .unwrap_or_default();
    let drops = page.heard.lock().unwrap().drops.len();
    let callback = |event, cb: usize| CoreCallbackData {
        event: EventFilter::Window(event),
        callback: CoreCallback {
            cb,
            ctx: OptionRefAny::None,
        },
        refany: RefAny::new(page.clone()),
    };
    Dom::create_body()
        .with_callbacks(
            vec![
                callback(WindowEventFilter::DroppedFile, on_drop as usize),
                callback(WindowEventFilter::HoveredFile, on_hover as usize),
            ]
            .into(),
        )
        .with_child(Dom::create_p_with_text(
            format!("{drops} drops").as_str(),
        ))
}

fn drop_window() -> (HeadlessWindow, Arc<Mutex<Heard>>) {
    let page = DropPage::default();
    let heard = page.heard.clone();
    let state = Arc::new(RefCell::new(RefAny::new(page)));
    let mut window = make_window_with(&state, drop_page_layout);
    window.regenerate_layout().expect("initial layout");
    let _ = window.common.take_regeneration();
    (window, heard)
}

fn paths(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| (*n).to_string()).collect()
}

#[test]
fn one_dropped_file_reaches_the_drop_callback_once_when_it_rebuilds_the_page() {
    let (mut window, heard) = drop_window();
    window.inject_event(HeadlessEvent::FileDrop {
        x: 50.0,
        y: 50.0,
        paths: paths(&["/home/a/report.pdf"]),
    });
    window.pump_once(true);
    assert_eq!(
        heard.lock().unwrap().drops,
        [paths(&["/home/a/report.pdf"])],
        "one drop, one DroppedFile call, though the callback rebuilt the page"
    );
}

#[test]
fn a_drop_of_three_files_is_one_event_with_all_three() {
    let (mut window, heard) = drop_window();
    let three = paths(&["/home/a/1.txt", "/home/a/2.txt", "/home/b/3.txt"]);
    window.inject_event(HeadlessEvent::FileDrop {
        x: 50.0,
        y: 50.0,
        paths: three.clone(),
    });
    window.pump_once(true);
    assert_eq!(heard.lock().unwrap().drops, [three]);
}

#[test]
fn a_drag_over_the_window_is_one_hover_per_move_and_the_drop_ends_it() {
    let (mut window, heard) = drop_window();
    let file = paths(&["/home/a/report.pdf"]);
    window.inject_event(HeadlessEvent::FileHover {
        x: 40.0,
        y: 40.0,
        paths: file.clone(),
    });
    window.pump_once(true);
    assert_eq!(
        heard.lock().unwrap().hovers,
        1,
        "one move of the drag, one HoveredFile call, though the callback rebuilt the page"
    );
    window.inject_event(HeadlessEvent::FileHover {
        x: 50.0,
        y: 50.0,
        paths: file.clone(),
    });
    window.inject_event(HeadlessEvent::FileDrop {
        x: 50.0,
        y: 50.0,
        paths: file.clone(),
    });
    window.pump_once(true);
    assert_eq!(heard.lock().unwrap().hovers, 2, "the second move");
    assert_eq!(heard.lock().unwrap().drops, [file]);
    assert!(
        window
            .common
            .layout_window
            .as_ref()
            .expect("laid out")
            .file_drop_manager
            .get_hovered_files()
            .is_empty(),
        "the drop ended the drag: nothing hovers the window any more"
    );

    // A later pass that has nothing to do with the drag hears neither again.
    window.inject_event(HeadlessEvent::MouseMove { x: 60.0, y: 60.0 });
    window.pump_once(true);
    let heard = heard.lock().unwrap();
    assert_eq!(heard.hovers, 2, "no hover after the drop");
    assert_eq!(heard.drops.len(), 1, "no second drop");
}
