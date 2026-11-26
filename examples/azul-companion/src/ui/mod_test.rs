//! What a tap does, against a connection that writes down what it is asked: navigation between the screens, the
//! files, the four buttons and their editor, forgetting the Mac, the pad's gestures and the rectangle that follows the
//! finger, and what the clock changes.

use std::sync::{Arc, Mutex};

use azul_review_link::frame::{ButtonSet, EntryKind, FileState, Entry, Focus, PenMark, PenState, Phase, Rect, Reply, ReplyStatus};
use azul_review_link::limits::Capped;
use azul_review_link::pairing::OfferError;

use super::*;
use crate::env::DEVICES;

#[derive(Debug, Clone, PartialEq)]
enum Call {
    Gesture(u32, Phase, (f32, f32), f32, Focus),
    Fit,
    Reset,
    PageStep(i8),
    ListDir(String, u32),
    OpenFile(String, Option<u32>),
    Press(usize),
    SetButtons(ButtonSet),
    BeginPairing(String),
    Retry,
    Forget,
}

/// A connection that remembers what it was asked.
struct TestApi {
    view: PhoneView,
    calls: Arc<Mutex<Vec<Call>>>,
}

impl PhoneApi for TestApi {
    fn view(&self) -> &PhoneView {
        &self.view
    }

    fn poll(&mut self, _now: Millis) -> bool {
        false
    }

    fn gesture(&mut self, _now: Millis, id: u32, phase: Phase, pan: (f32, f32), zoom: f32, focus: Focus) {
        self.calls.lock().unwrap().push(Call::Gesture(id, phase, pan, zoom, focus));
    }

    fn fit(&mut self, _now: Millis) {
        self.calls.lock().unwrap().push(Call::Fit);
    }

    fn page_step(&mut self, _now: Millis, step: i8) {
        self.calls.lock().unwrap().push(Call::PageStep(step));
    }

    fn reset_view(&mut self, _now: Millis) {
        self.calls.lock().unwrap().push(Call::Reset);
    }

    fn list_dir(&mut self, _now: Millis, path: &str, offset: u32) {
        self.view.begin_listing(path);
        self.calls.lock().unwrap().push(Call::ListDir(path.to_string(), offset));
    }

    fn open_file(&mut self, _now: Millis, path: &str, line: Option<u32>) {
        self.calls.lock().unwrap().push(Call::OpenFile(path.to_string(), line));
    }

    fn press_button(&mut self, _now: Millis, index: usize) {
        self.view.button_pressed(index, 100 + index as u32);
        self.calls.lock().unwrap().push(Call::Press(index));
    }

    fn set_buttons(&mut self, _now: Millis, buttons: &ButtonSet) -> Vec<Capped> {
        self.calls.lock().unwrap().push(Call::SetButtons(buttons.clone()));
        buttons.sanitized().1
    }

    fn begin_pairing(&mut self, _now: Millis, offer: &str) -> Result<(), OfferError> {
        self.calls.lock().unwrap().push(Call::BeginPairing(offer.to_string()));
        Ok(())
    }

    fn retry(&mut self, _now: Millis) {
        self.calls.lock().unwrap().push(Call::Retry);
    }

    fn forget_mac(&mut self, _now: Millis) {
        self.calls.lock().unwrap().push(Call::Forget);
        self.view.session = Session::Unpaired;
    }
}

fn pen_state() -> PenState {
    PenState {
        seq: 1,
        folder: "azul".into(),
        file: "src/lib.rs".into(),
        page_index: 1,
        page_count: 5,
        line: 42,
        line_count: 900,
        page_w: 794.0,
        page_h: 1123.0,
        view: Rect { x: 0.25, y: 0.30, w: 0.5, h: 0.4 },
        zoom: 2.0,
        pen: Some(PenMark { x: 0.5, y: 0.5, down: false }),
        thumb_id: 1,
        context: azul_review_link::frame::Context::Line,
    }
}

fn root_with(view: PhoneView) -> (Root, Arc<Mutex<Vec<Call>>>) {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let api = TestApi { view, calls: calls.clone() };
    (Root::new(Box::new(api), EnvSource::Fixed(DEVICES[0].env(false, Lang::En, true))), calls)
}

fn connected() -> PhoneView {
    let mut view = PhoneView::new();
    view.session = Session::Connected { mac_name: "Studio Mac".into() };
    view.mac_name = "Studio Mac".into();
    view.pen = Some(pen_state());
    view
}

fn taken(calls: &Arc<Mutex<Vec<Call>>>) -> Vec<Call> {
    std::mem::take(&mut *calls.lock().unwrap())
}

// ---- the screens

#[test]
fn the_tabs_switch_between_the_remote_and_the_files_and_the_files_ask_for_the_workspace_once() {
    let (mut root, calls) = root_with(connected());
    assert_eq!(root.ui.screen, Screen::Remote);
    assert!(root.perform(10, Action::Tab(Screen::Files)));
    assert_eq!(root.ui.screen, Screen::Files);
    assert_eq!(taken(&calls), [Call::ListDir(String::new(), 0)]);
    root.perform(20, Action::Tab(Screen::Remote));
    root.perform(30, Action::Tab(Screen::Files));
    assert!(taken(&calls).is_empty(), "the answer is awaited, so it is not asked again");
}

#[test]
fn the_gear_opens_the_settings_and_back_returns_to_where_it_came_from() {
    let (mut root, _) = root_with(connected());
    root.perform(0, Action::Tab(Screen::Files));
    root.perform(1, Action::OpenSettings);
    assert_eq!(root.ui.screen, Screen::Settings);
    assert!(root.perform(2, Action::Back));
    assert_eq!(root.ui.screen, Screen::Files);
    root.perform(3, Action::Back);
    assert_eq!(root.ui.screen, Screen::Files, "back from a main screen stays");
}

#[test]
fn the_remote_controls_go_to_the_mac() {
    let (mut root, calls) = root_with(connected());
    root.perform(1, Action::Fit);
    root.perform(2, Action::ResetView);
    root.perform(3, Action::PageStep(1));
    root.perform(4, Action::PageStep(-1));
    root.perform(5, Action::Strip(1));
    assert_eq!(taken(&calls), [Call::Fit, Call::Reset, Call::PageStep(1), Call::PageStep(-1), Call::PageStep(1)]);
}

// ---- the quick buttons

#[test]
fn a_tap_on_a_quick_button_sends_it_and_a_button_that_does_not_exist_sends_nothing() {
    let (mut root, calls) = root_with(connected());
    assert!(root.perform(100, Action::Button(2)));
    assert_eq!(taken(&calls), [Call::Press(2)]);
    assert!(!root.perform(101, Action::Button(4)));
    assert!(!root.perform(102, Action::Button(usize::MAX)));
    assert!(taken(&calls).is_empty());
}

#[test]
fn a_long_press_opens_the_editor_on_that_button_and_the_click_after_it_is_not_a_press() {
    let (mut root, calls) = root_with(connected());
    assert!(root.perform(1000, Action::EditButtons(Some(1))));
    assert_eq!(root.ui.screen, Screen::Editor);
    assert_eq!(root.ui.editor_focus, Some(1));
    assert_eq!(root.ui.drafts[1].0, "Explain");
    root.perform(1100, Action::Back);
    assert_eq!(root.ui.screen, Screen::Remote);
    root.perform(1200, Action::Button(1));
    assert!(taken(&calls).is_empty(), "the release of the long press, soon after");
    root.perform(1000 + 700, Action::Button(1));
    assert_eq!(taken(&calls), [Call::Press(1)]);
}

#[test]
fn the_editor_saves_its_drafts_through_the_connection_and_says_so() {
    let (mut root, calls) = root_with(connected());
    root.perform(0, Action::EditButtons(None));
    root.ui.drafts[0] = ("Nicer".into(), "Say this in a friendlier tone.".into());
    root.ui.editor_dirty = true;
    assert!(root.perform(10, Action::EditorSave));
    let sent = taken(&calls);
    assert_eq!(sent.len(), 1);
    match &sent[0] {
        Call::SetButtons(set) => {
            assert_eq!(set.buttons[0].label, "Nicer");
            assert_eq!(set.buttons[1].label, "Explain", "the others as they were");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(root.ui.screen, Screen::Remote);
    assert!(!root.ui.editor_dirty);
    assert_eq!(root.ui.toast.as_ref().map(|t| t.0.as_str()), Some("editor-saved"));
}

#[test]
fn saving_a_draft_over_its_caps_is_cut_and_the_toast_says_it_was() {
    let (mut root, _) = root_with(connected());
    root.perform(0, Action::EditButtons(Some(0)));
    root.ui.drafts[0].0 = "A label far too long for the key".into();
    root.perform(10, Action::EditorSave);
    assert_eq!(root.ui.toast.as_ref().map(|t| t.0.as_str()), Some("editor-capped"));
}

#[test]
fn reset_puts_the_defaults_back_in_the_drafts_and_the_editor_remembers_it_has_changes() {
    let (mut root, _) = root_with(connected());
    let mut custom = ButtonSet::defaults();
    custom.buttons[3].label = "Mine".into();
    root.api = Box::new(TestApi { view: PhoneView { buttons: custom, ..connected() }, calls: Arc::new(Mutex::new(Vec::new())) });
    root.perform(0, Action::EditButtons(Some(3)));
    assert_eq!(root.ui.drafts[3].0, "Mine");
    assert!(root.perform(1, Action::EditorReset));
    assert_eq!(root.ui.drafts[3].0, "Doc");
    assert!(root.ui.editor_dirty);
}

// ---- the files

#[test]
fn a_folder_a_crumb_and_more_rows_ask_the_mac_and_a_file_opens_in_azreview() {
    let mut view = connected();
    view.files.dir = "layout/src".into();
    view.files.entries = vec![Entry { name: "window.rs".into(), kind: EntryKind::File, state: FileState::None }; 3];
    view.files.more = true;
    let (mut root, calls) = root_with(view);
    root.perform(0, Action::OpenDir("layout/src/solver3".into()));
    assert_eq!(taken(&calls), [Call::ListDir("layout/src/solver3".into(), 0)]);
    root.perform(1, Action::Crumb(1));
    assert_eq!(taken(&calls), [Call::ListDir("layout".into(), 0)]);
    root.perform(2, Action::Crumb(0));
    assert_eq!(taken(&calls), [Call::ListDir(String::new(), 0)]);
    root.perform(3, Action::LoadMore);
    assert_eq!(taken(&calls), [Call::ListDir("layout/src".into(), 3)]);
    root.perform(4, Action::OpenFile("layout/src/window.rs".into()));
    assert_eq!(taken(&calls), [Call::OpenFile("layout/src/window.rs".into(), None)]);
    assert_eq!(root.ui.opened.as_ref().map(|o| o.0.as_str()), Some("layout/src/window.rs"));
    root.perform(5, Action::RetryFiles);
    assert_eq!(taken(&calls), [Call::ListDir("layout/src".into(), 0)]);
}

#[test]
fn a_path_that_is_not_safe_is_not_asked_for() {
    let (mut root, calls) = root_with(connected());
    for path in ["../etc", "/etc", "a/../b"] {
        root.perform(0, Action::OpenDir(path.into()));
        root.perform(0, Action::OpenFile(path.into()));
    }
    assert!(taken(&calls).is_empty());
}

// ---- forgetting and pairing

#[test]
fn forgetting_the_mac_asks_first_and_then_unpairs() {
    let (mut root, calls) = root_with(connected());
    root.perform(0, Action::OpenSettings);
    root.perform(1, Action::ForgetAsk);
    assert!(root.ui.forget_ask);
    root.perform(2, Action::ForgetNo);
    assert!(!root.ui.forget_ask && taken(&calls).is_empty());
    root.perform(3, Action::ForgetAsk);
    root.perform(4, Action::ForgetYes);
    assert_eq!(taken(&calls), [Call::Forget]);
    assert!(!root.ui.forget_ask);
    assert_eq!(root.ui.screen, Screen::Remote);
}

#[test]
fn the_pairing_buttons_go_to_the_connection() {
    let (mut root, calls) = root_with(PhoneView::new());
    root.perform(0, Action::PairFind);
    root.perform(1, Action::PairRetry);
    root.perform(2, Action::PairCancel);
    assert_eq!(taken(&calls), [Call::Retry, Call::Retry, Call::Forget]);
}

#[test]
fn a_pairing_that_goes_through_shows_the_card_until_start_and_a_lost_session_keeps_the_screens() {
    let mut view = PhoneView::new();
    view.session = Session::Pairing { mac_name: "Studio Mac".into(), code: 482_913 };
    let (mut root, _) = root_with(view);
    root.tick(0);
    assert_eq!(root.ui.last_session, SessionKind::Pairing);
    let mut api = TestApi { view: connected(), calls: Arc::new(Mutex::new(Vec::new())) };
    api.view.session = Session::Connected { mac_name: "Studio Mac".into() };
    root.api = Box::new(api);
    assert!(root.tick(10));
    assert!(root.ui.pair_card, "just paired");
    root.perform(20, Action::PairStart);
    assert!(!root.ui.pair_card);
    // a connection that comes back after a restart (not a pairing) shows no card
    let (mut again, _) = root_with(connected());
    again.tick(0);
    assert!(!again.ui.pair_card);
}

#[test]
fn a_session_that_is_not_connected_takes_the_phone_back_to_the_first_screen() {
    let (mut root, _) = root_with(connected());
    root.tick(0);
    root.perform(1, Action::Tab(Screen::Files));
    root.perform(2, Action::OpenSettings);
    let mut view = connected();
    view.session = Session::Lost { mac_name: "Studio Mac".into(), attempt: 1 };
    root.api = Box::new(TestApi { view, calls: Arc::new(Mutex::new(Vec::new())) });
    root.tick(10);
    assert_eq!(root.ui.screen, Screen::Settings, "a lost link keeps what the user was doing");
    let mut view = connected();
    view.session = Session::Unpaired;
    root.api = Box::new(TestApi { view, calls: Arc::new(Mutex::new(Vec::new())) });
    root.tick(20);
    assert_eq!(root.ui.screen, Screen::Remote);
}

#[test]
fn a_mac_that_cannot_be_reached_offers_to_pair_again_and_a_connection_ends_that() {
    let mut view = connected();
    view.session = Session::Failed { kind: azul_review_link::view::FailureKind::AddressChanged };
    let (mut root, calls) = root_with(view);
    root.tick(0);
    assert!(!root.ui.pair_again);
    assert!(root.perform(1, Action::PairAgain));
    assert!(root.ui.pair_again);
    assert!(taken(&calls).is_empty(), "nothing is forgotten by asking to pair again");
    assert!(root.perform(2, Action::PairBack));
    assert!(!root.ui.pair_again);
    root.perform(3, Action::PairAgain);
    root.api = Box::new(TestApi { view: connected(), calls: Arc::new(Mutex::new(Vec::new())) });
    root.tick(10);
    assert!(!root.ui.pair_again, "once connected the pairing screen is over");
}

// ---- the clock

#[test]
fn what_became_of_a_press_is_noticed_and_a_line_shown_for_a_moment_goes_away() {
    let (mut root, _) = root_with(connected());
    root.tick(0);
    root.perform(100, Action::Button(0));
    let mut view = connected();
    view.button_pressed(0, 100);
    view.apply_frame(&azul_review_link::frame::Frame::Reply(Reply { request_id: 100, status: ReplyStatus::Working }));
    root.api = Box::new(TestApi { view, calls: Arc::new(Mutex::new(Vec::new())) });
    assert!(root.tick(500), "the status changed");
    assert!(!root.tick(520), "and nothing since");
    root.ui.toast = Some(("editor-saved".into(), 1000));
    assert!(root.tick(1000 + 2600));
    assert!(root.ui.toast.is_none());
}

// ---- the pad

#[test]
fn a_drag_on_the_pad_goes_to_the_mac_as_a_whole_gesture_around_the_pen_and_the_rectangle_follows_the_finger() {
    let (mut root, calls) = root_with(connected());
    assert!(root.ui.hint);
    root.pad_event(0, PadEvent::Down { x: 100.0, y: 100.0 }, 300.0);
    root.pad_event(5, PadEvent::Move { x: 120.0, y: 100.0 }, 300.0);
    let shown_at_start = root.shown_view().unwrap();
    root.pad_event(10, PadEvent::Move { x: 220.0, y: 100.0 }, 300.0);
    let shown = root.shown_view().unwrap();
    assert!(shown.x < shown_at_start.x, "the finger went right, the rectangle goes left: {shown:?} {shown_at_start:?}");
    root.pad_event(15, PadEvent::Up { x: 220.0, y: 100.0 }, 300.0);
    let sent = taken(&calls);
    let phases: Vec<Phase> = sent
        .iter()
        .filter_map(|c| if let Call::Gesture(_, phase, ..) = c { Some(*phase) } else { None })
        .collect();
    assert_eq!(phases.first(), Some(&Phase::Begin));
    assert_eq!(phases.last(), Some(&Phase::End));
    assert!(sent.iter().all(|c| !matches!(c, Call::Gesture(_, _, _, _, f) if *f != Focus::Pen)), "zoom and pan are about the pen: {sent:?}");
    assert!(!root.ui.hint, "the hint is for the first use");
    assert_eq!(root.shown_view(), Some(pen_state().view), "after the gesture the rectangle is what the Mac says again");
}

#[test]
fn two_quick_taps_fit_the_width_and_a_right_click_resets_the_view() {
    let (mut root, calls) = root_with(connected());
    for t in [0, 120] {
        root.pad_event(t, PadEvent::Down { x: 50.0, y: 50.0 }, 300.0);
        root.pad_event(t + 20, PadEvent::Up { x: 50.0, y: 50.0 }, 300.0);
    }
    assert_eq!(taken(&calls), [Call::Fit]);
    root.pad_event(500, PadEvent::Secondary, 300.0);
    assert_eq!(taken(&calls), [Call::Reset]);
}

#[test]
fn a_pinch_zooms_about_the_pen_and_the_rectangle_shrinks() {
    let (mut root, calls) = root_with(connected());
    root.pad_event(0, PadEvent::Pinch { scale: 1.0, began: true, ended: false }, 300.0);
    root.pad_event(10, PadEvent::Pinch { scale: 2.0, began: false, ended: false }, 300.0);
    let shown = root.shown_view().unwrap();
    assert!((shown.w - 0.25).abs() < 1e-3, "half as wide at twice the zoom: {shown:?}");
    root.pad_event(20, PadEvent::Pinch { scale: 2.0, began: false, ended: true }, 300.0);
    let sent = taken(&calls);
    assert!(sent.iter().any(|c| matches!(c, Call::Gesture(_, Phase::End, _, z, Focus::Pen) if (*z - 2.0).abs() < 1e-3)), "{sent:?}");
}

#[test]
fn a_pad_with_no_pen_state_to_go_by_ignores_the_fingers_and_never_panics() {
    let (mut root, calls) = root_with(PhoneView::new());
    for event in [
        PadEvent::Down { x: 1.0, y: 1.0 },
        PadEvent::Move { x: 50.0, y: 50.0 },
        PadEvent::Up { x: 50.0, y: 50.0 },
        PadEvent::Pinch { scale: 1.5, began: true, ended: true },
        PadEvent::Cancel,
        PadEvent::Secondary,
    ] {
        root.pad_event(0, event, 300.0);
    }
    root.pad_event(1, PadEvent::Down { x: f32::NAN, y: 1.0 }, f32::NAN);
    assert!(taken(&calls).is_empty());
    assert_eq!(root.shown_view(), None);
}

#[test]
fn a_cancelled_gesture_ends_at_the_mac_and_the_hint_can_be_dismissed() {
    let (mut root, calls) = root_with(connected());
    root.pad_event(0, PadEvent::Down { x: 0.0, y: 0.0 }, 300.0);
    root.pad_event(1, PadEvent::Move { x: 30.0, y: 0.0 }, 300.0);
    root.pad_event(2, PadEvent::Cancel, 300.0);
    let sent = taken(&calls);
    assert!(matches!(sent.last(), Some(Call::Gesture(_, Phase::End, ..))), "{sent:?}");
    assert_eq!(root.ui.live_base, None);
    root.ui.hint = true;
    root.perform(3, Action::DismissHint);
    assert!(!root.ui.hint);
}
