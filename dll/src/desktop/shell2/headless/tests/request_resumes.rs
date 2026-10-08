//! A request's resume rebuilds the windows that show its answer
//! (HEADLESSRESUME15, 2026-10-08).
//!
//! The request / resume queue (`azul_layout::request`) is PROCESS-wide: a
//! completion is delivered by whichever window's pass drains it first after
//! the answer arrived - not necessarily the window that shows what the resume
//! changes, and sometimes inside a pass whose result its caller drops.
//!
//! The live case, AzCalendar's `mint-and-join.mjs` against the meeting dev
//! server: a screen reader's Save (the debug server's `accessibility_action`)
//! stores the draft with a pending AzMeet link and registers it with an HTTP
//! POST, which a worker answers a few milliseconds later; the save closes the
//! draft's popover. The root's frame drops the popover's node, the popover's
//! next turn runs its close protocol - and that pass delivered the answer. The
//! resume marked the link registered and asked for `RefreshDom`, which went
//! down with the closing popover: the week kept showing "AzMeet link waits for
//! the server" until an unrelated click rebuilt it.
//!
//! The presses here are a screen reader's, the ingress AzCalendar's script
//! used: the module needs the `a11y` feature (see its `mod` line).

use super::*;

/// AzCalendar's week with a draft: the event's meeting line and, while the
/// draft is open, its popover holding Save.
struct Draft {
    open: bool,
    registered: bool,
    /// The window whose pass delivered the registration's answer.
    answered_in: String,
}

/// Save: the draft closes (and with it its popover), its meeting link is
/// registered with the server - a request a worker answers later
/// (`HttpRequestConfig::http_request` defers exactly like this).
extern "C" fn save_the_draft(
    mut data: RefAny,
    _info: azul_layout::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    if let Some(mut draft) = data.downcast_mut::<Draft>() {
        draft.open = false;
    }
    // The answer arrives after the save's own pass asked once (the root's
    // Phase 2), and before the next pass asks: the closing popover's. A
    // test running beside this one drains the same process-wide queue, so
    // only this test's thread is answered.
    let owner = std::thread::current().id();
    let mut asked = 0usize;
    let _registration = azul_layout::request::defer(
        data.clone(),
        azul_layout::callbacks::ResumeCallback::create(registered),
        Box::new(move || {
            if std::thread::current().id() != owner {
                return None;
            }
            asked += 1;
            (asked >= 2).then(|| RefAny::new(()))
        }),
    );
    azul_core::callbacks::Update::RefreshDom
}

/// The server's answer: the link is registered, the week must say so.
extern "C" fn registered(
    mut data: RefAny,
    info: azul_layout::callbacks::CallbackInfo,
    _answer: RefAny,
) -> azul_core::callbacks::Update {
    let window = info.get_current_window_state().window_id.as_str().to_string();
    if let Some(mut draft) = data.downcast_mut::<Draft>() {
        draft.registered = true;
        draft.answered_in = window;
    }
    azul_core::callbacks::Update::RefreshDom
}

/// `body > p` with the event's meeting line and, while the draft is open, its
/// popover: a `<transient-window open>` (a window of its own) whose content is
/// the 120x40 Save button `.draft-save`.
extern "C" fn draft_week_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{NodeData, NodeType},
        events::{EventFilter, HoverEventFilter},
        transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
    };
    let (open, registered) = data
        .downcast_ref::<Draft>()
        .map(|d| (d.open, d.registered))
        .unwrap_or((false, false));
    let week = Dom::create_body().with_child(Dom::create_p_with_text(if registered {
        "Join meeting"
    } else {
        "AzMeet link waits for the server"
    }));
    if !open {
        return week;
    }
    let save = Dom::create_div()
        .with_class("draft-save".into())
        .with_css("width: 120px; height: 40px;")
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback {
                    cb: save_the_draft as usize,
                    ctx: OptionRefAny::None,
                },
                refany: data.clone(),
            }]
            .into(),
        );
    let popover = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(
        TransientWindowConfig::opened()
            .with_anchor(TransientAnchor::Viewport)
            .with_dismiss(TransientDismiss::None),
    )))
    .with_child(save);
    week.with_child(Dom::create_div().with_child(popover))
}

/// The node a screen reader presses.
struct PressTarget {
    node: azul_core::dom::NodeId,
}

/// The debug server's `accessibility_action` op, as its timer runs it in the
/// window a script talks to: a screen reader's press on the target.
extern "C" fn press_like_a_screen_reader(
    mut data: RefAny,
    mut info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    if let Some(node) = data.downcast_ref::<PressTarget>().map(|t| t.node) {
        info.callback_info.perform_accessibility_action(
            azul_core::dom::DomId::ROOT_ID,
            node,
            azul_core::dom::AccessibilityAction::Default,
        );
    }
    azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
}

/// Every text the window's root DOM holds: what `get_node_hierarchy` answers.
fn shown_texts(window: &HeadlessWindow) -> Vec<String> {
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

/// AzCalendar's Save, as `mint-and-join.mjs` presses it: the registration's
/// answer reaches the loop while the draft's popover closes, and the popover's
/// close pass delivers it. Its `RefreshDom` must rebuild the week - the window
/// that shows the meeting line - in the turns that follow, with no further
/// event.
#[test]
fn a_request_answered_while_its_popover_closes_rebuilds_the_window_that_shows_the_answer() {
    let state = Arc::new(RefCell::new(RefAny::new(Draft {
        open: true,
        registered: false,
        answered_in: String::new(),
    })));
    let mut root = make_window_with(&state, draft_week_layout);
    root.regenerate_layout().expect("the week's first layout");
    let _ = root.common.take_regeneration();
    root.pump_children();
    let popovers: Vec<String> = root
        .children
        .iter()
        .map(|c| c.common.current_window_state().window_id.as_str().to_string())
        .collect();
    assert_eq!(
        popovers,
        vec!["azul-transient".to_string()],
        "harness: the draft's popover is a window of its own"
    );
    let save = node_with_class(&root, "draft-save")
        .expect("harness: Save is in the week's DOM, which the popover mirrors");

    let get_time = azul_core::task::GetSystemTimeCallback {
        cb: azul_core::task::get_system_time_libstd,
    };
    root.start_timer(
        azul_core::task::TimerId::unique().id,
        azul_layout::timer::Timer::create(
            RefAny::new(PressTarget { node: save }),
            press_like_a_screen_reader as azul_layout::timer::TimerCallbackType,
            get_time,
        ),
    );
    // The run loop's turns (`HeadlessWindow::run`), without the process-wide
    // notification and hotkey mailboxes a test beside this one may fill.
    for _ in 0..6 {
        root.pump_once(false);
        root.pump_children();
    }

    let (registered, answered_in) = state
        .borrow_mut()
        .downcast_ref::<Draft>()
        .map(|d| (d.registered, d.answered_in.clone()))
        .expect("the draft");
    assert!(registered, "harness: the registration was answered");
    assert_eq!(
        answered_in, "azul-transient",
        "harness: the closing popover's pass delivered the answer, as in AzCalendar's run"
    );
    assert!(
        root.children.is_empty(),
        "harness: the popover closed with the save"
    );
    let texts = shown_texts(&root);
    assert!(
        texts.iter().any(|t| t == "Join meeting")
            && !texts.iter().any(|t| t.contains("waits for the server")),
        "the resume's RefreshDom must rebuild the week that shows the meeting line, with no \
         further event; it went down with the closing popover: {texts:?}"
    );
}
