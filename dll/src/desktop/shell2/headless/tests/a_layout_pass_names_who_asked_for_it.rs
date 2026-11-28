//! A layout pass names who asked for it (E76, 2026-11-28).
//!
//! Every cold start of AzWidgets ran a second full layout a few milliseconds after the first picture, tagged
//! `reason=RefreshDom`, and nobody could say who asked: `RefreshDom` is the tag of most of the sixty-odd places that
//! raise the window's request, so the startup timeline's line told the pass and not its producer. The window now
//! remembers where the FIRST request since the last pass was made (`CommonWindowState::request_regeneration` is
//! `#[track_caller]`) and, when a callback's answer asked for it, whose (the event and the node of an event callback, a
//! timer, a thread's write-back); a pass consumes both, and the timeline prints them:
//! `layout_begin #2 reason=RefreshDom by=<file>:<line> via=MouseUp on node 3 (a Button #go)`.

use super::*;
use crate::desktop::{
    app_settings::AppSettings, shell2::common::app_shared::AppShared,
    startup_timeline::StartupTimeline,
};

/// `<body><div>{label}</div></body>`: the smallest page with something to lay out.
extern "C" fn page_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    Dom::create_body().with_child(Dom::create_div().with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper("a page"),
    ))
}

/// A window of an app whose start is timed (`AZ_STARTUP=1`), and that timeline.
fn timed_window() -> (HeadlessWindow, Arc<StartupTimeline>) {
    timed_window_of(page_layout)
}

/// [`timed_window`] for the page `layout` builds.
fn timed_window_of(
    layout: azul_core::callbacks::LayoutCallbackType,
) -> (HeadlessWindow, Arc<StartupTimeline>) {
    use azul_core::icon::{IconProviderHandle, SharedIconProvider};
    let timeline = Arc::new(StartupTimeline::new(true, std::time::Instant::now(), None));
    let config = AppConfig::default();
    let app_shared = Arc::new(AppShared::with_startup(
        &config,
        AppSettings::default(),
        Arc::clone(&timeline),
    ));
    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: layout,
        ctx: OptionRefAny::None,
    };
    options.window_state.size.dimensions = LogicalSize::new(400.0, 300.0);
    let window = HeadlessWindow::with_app_shared(
        options,
        Arc::new(RefCell::new(RefAny::new(()))),
        event::SharedUndoManager::new(),
        app_shared,
        config,
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("a window");
    inject_harness_font(&window);
    (window, timeline)
}

/// The detail of every `layout_begin` mark of the timeline, in order (`#2 reason=RefreshDom by=<file>:<line>`).
fn layout_begins(timeline: &StartupTimeline) -> Vec<String> {
    timeline
        .marks()
        .into_iter()
        .filter(|mark| mark.name == "layout_begin")
        .map(|mark| mark.detail)
        .collect()
}

/// The window's first pass is the one it was born owing: nobody asked for it, so the timeline names no producer. A
/// request made afterwards names the line that made it, whatever tag it carries.
///
/// Mutation: the timeline line without the producer (what the shells printed until now): the second begin is
/// `#2 reason=RefreshDom`, with nothing to say who asked.
#[test]
fn the_second_pass_names_the_line_that_asked_for_it() {
    let (mut window, timeline) = timed_window();
    window.regenerate_layout().expect("the first layout");

    let asked_on = line!() + 1;
    window.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    window.regenerate_layout().expect("the second layout");

    let begins = layout_begins(&timeline);
    assert_eq!(begins.len(), 2, "two passes were timed: {begins:?}");
    assert_eq!(
        begins[0], "#1 reason=Initial",
        "the pass a window is born owing has no asker: nobody asked for it, the window was born owing it"
    );
    let here = file!();
    assert_eq!(
        begins[1],
        format!("#2 reason=RefreshDom by={here}:{asked_on}"),
        "the second pass names the call that asked for it"
    );
}

/// Several asks for one pass are one pass: the first of them is its producer, a later ask is merged into it and does not
/// move the blame; the pass takes what was asked, so the next ask names itself.
#[test]
fn the_first_ask_since_a_pass_is_its_producer_and_the_pass_consumes_it() {
    let (mut window, _timeline) = timed_window();
    window.regenerate_layout().expect("the first layout");
    assert!(
        window.common.regeneration_requested_at().is_none(),
        "nothing was asked since the pass that served the window's own request"
    );

    let first = line!() + 1;
    window.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    window.request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
    let at = window
        .common
        .regeneration_requested_at()
        .expect("a request was made");
    assert_eq!(
        (at.file(), at.line()),
        (file!(), first),
        "the first ask names itself, the second one does not take its place"
    );
    assert_eq!(
        window.common.regeneration_reason(),
        azul_core::callbacks::RelayoutReason::Resize,
        "harness: the more specific reason of the two is the one the pass carries"
    );

    window.regenerate_layout().expect("the pass");
    assert!(
        window.common.regeneration_requested_at().is_none(),
        "the pass consumed the ask"
    );

    let next = line!() + 1;
    window.request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    let at = window
        .common
        .regeneration_requested_at()
        .expect("a new request was made");
    assert_eq!(
        (at.file(), at.line()),
        (file!(), next),
        "the next ask after a pass names itself"
    );
}

/// The wording of the line: the tag alone for a pass nobody asked for, the tag and the place for one somebody did.
#[test]
fn the_timeline_prints_the_tag_and_the_place() {
    use crate::desktop::shell2::common::event::layout_pass_reason;
    use azul_core::callbacks::RelayoutReason;

    assert_eq!(layout_pass_reason(RelayoutReason::Initial, None, None), "Initial");
    let at = core::panic::Location::caller();
    assert_eq!(
        layout_pass_reason(RelayoutReason::Resize, Some(at), None),
        format!("Resize by={}:{}", at.file(), at.line())
    );
    assert_eq!(
        layout_pass_reason(RelayoutReason::RefreshDom, Some(at), Some("timer 7")),
        format!("RefreshDom by={}:{} via=timer 7", at.file(), at.line())
    );
}

/// A button whose release answers `RefreshDom`: the page's one callback.
extern "C" fn answer_refresh(
    _data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    azul_core::callbacks::Update::RefreshDom
}

/// `body > div#go [MouseUp: answer_refresh]`, 100 x 40 at the top left.
extern "C" fn button_page(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        events::{EventFilter, HoverEventFilter},
    };

    Dom::create_body().with_css("margin: 0;").with_child(
        Dom::create_div()
            .with_ids_and_classes(azul_core::dom::IdOrClassVec::from_vec(vec![
                azul_core::dom::IdOrClass::Id("go".into()),
            ]))
            .with_css("width: 100px; height: 40px;")
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb: answer_refresh as usize,
                        ctx: OptionRefAny::None,
                    },
                    refany: RefAny::new(()),
                }]
                .into(),
            ),
    )
}

/// A pass that a callback's answer asked for names the callback: the event it ran for and the node, as the DOM lint names
/// a node, not only the line of the shell that carried the answer to the window's request.
///
/// Mutation: the timeline line without the callback (the shell's line alone, the same for every callback of every widget):
/// the second begin has `by=` and no `via=`.
#[test]
fn a_pass_a_callback_asked_for_names_the_callback() {
    let (mut window, timeline) = timed_window_of(button_page);
    window.regenerate_layout().expect("the first layout");

    let _ = click_at(&mut window, 50.0, 20.0);

    let begins = layout_begins(&timeline);
    assert_eq!(begins.len(), 2, "the click laid the page out once more: {begins:?}");
    assert!(
        begins[1].starts_with("#2 reason=RefreshDom by=") && begins[1].contains(" via=MouseUp on node "),
        "the pass names the shell's line and the callback that asked: {:?}",
        begins[1]
    );
    assert!(
        begins[1].contains("#go"),
        "the node is named by its id, as the DOM lint names it: {:?}",
        begins[1]
    );
}
