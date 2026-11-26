//! What a tap does, against a connection that writes down what it is asked: navigation between the screens, the
//! files, the four buttons and their editor, forgetting the Mac, the pad's gestures and the rectangle that follows the
//! finger, and what the clock changes.

use std::sync::{Arc, Mutex};

use azul_review_link::access::ScreenId;
use azul_review_link::frame::{
    AgentFinished, ButtonSet, Entry, EntryKind, FileState, FinishKind, Focus, HostSettings, PageSurface, PenMark, PenState, Phase, Rect, Reply,
    ReplyStatus, Surface,
};
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
    Cancel,
    Forget,
    Continue,
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

    fn continue_reviewing(&mut self, _now: Millis) {
        self.calls.lock().unwrap().push(Call::Continue);
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
        // the real answer: the text is an offer or it says why not
        azul_review_link::pairing::PairingOffer::parse(offer).map(|_| ())
    }

    fn retry(&mut self, _now: Millis) {
        self.calls.lock().unwrap().push(Call::Retry);
    }

    fn cancel(&mut self, _now: Millis) {
        self.calls.lock().unwrap().push(Call::Cancel);
        self.view.session = Session::Unpaired;
    }

    fn forget_mac(&mut self, _now: Millis) {
        self.calls.lock().unwrap().push(Call::Forget);
        self.view.session = Session::Unpaired;
    }
}

fn pen_state() -> PenState {
    PenState {
        seq: 1,
        screen: ScreenId::MAIN,
        folder: "azul".into(),
        surface: Surface::Page(PageSurface {
            file: "src/lib.rs".into(),
            page_index: 1,
            page_count: 5,
            line: 42,
            line_count: 900,
            aspect: 794.0 / 1123.0,
            gap: 0.04,
        }),
        view: Rect { x: 0.25, y: 1.6, w: 0.5, h: 0.375 },
        zoom: 2.0,
        pen: Some(PenMark { x: 0.5, y: 1.8, down: false }),
        thumb_id: 1,
        context: azul_review_link::frame::Context::Line,
        return_point: None,
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
    assert_eq!(taken(&calls), [Call::Retry, Call::Retry, Call::Cancel], "cancelling a search does not forget the Mac");
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
fn a_drag_on_the_pad_goes_to_the_mac_as_a_whole_gesture_around_the_pen_and_the_viewport_goes_the_other_way_from_the_finger() {
    let (mut root, calls) = root_with(connected());
    assert!(root.ui.hint);
    root.pad_event(0, PadEvent::Down { x: 100.0, y: 100.0 }, (300.0, 390.0));
    root.pad_event(5, PadEvent::Move { x: 120.0, y: 100.0 }, (300.0, 390.0));
    let shown_at_start = root.shown_view().unwrap();
    root.pad_event(10, PadEvent::Move { x: 220.0, y: 100.0 }, (300.0, 390.0));
    let shown = root.shown_view().unwrap();
    assert!(shown.x < shown_at_start.x, "the finger went right, the content goes right, the viewport goes left: {shown:?} {shown_at_start:?}");
    root.pad_event(15, PadEvent::Up { x: 220.0, y: 100.0 }, (300.0, 390.0));
    let sent = taken(&calls);
    let phases: Vec<Phase> = sent
        .iter()
        .filter_map(|c| if let Call::Gesture(_, phase, ..) = c { Some(*phase) } else { None })
        .collect();
    assert_eq!(phases.first(), Some(&Phase::Begin));
    assert_eq!(phases.last(), Some(&Phase::End));
    assert!(sent.iter().all(|c| !matches!(c, Call::Gesture(_, _, _, _, f) if *f != Focus::Pen)), "zoom and pan are about the pen: {sent:?}");
    assert!(!root.ui.hint, "the hint is for the first use");
    assert_eq!(root.shown_view(), Some(pen_state().view), "after the gesture the view is what the Mac says again");
}

#[test]
fn the_phone_obeys_the_macs_wish_to_reverse_the_pan() {
    let drag_pan = |reverse: bool| {
        let mut view = connected();
        view.settings = HostSettings { reverse_pan: reverse, ..HostSettings::default() };
        let (mut root, calls) = root_with(view);
        root.pad_event(0, PadEvent::Down { x: 100.0, y: 100.0 }, (300.0, 390.0));
        root.pad_event(5, PadEvent::Move { x: 112.0, y: 100.0 }, (300.0, 390.0));
        root.pad_event(10, PadEvent::Move { x: 172.0, y: 100.0 }, (300.0, 390.0));
        let sent = taken(&calls);
        let last = sent.iter().rev().find_map(|c| if let Call::Gesture(_, Phase::Update, pan, ..) = c { Some(*pan) } else { None });
        last.expect("a gesture update")
    };
    let (normal, _) = drag_pan(false);
    let (reversed, _) = drag_pan(true);
    assert!(normal < 0.0, "by default the content follows the finger: a drag to the right sends the viewport to the left: {normal}");
    assert!(reversed > 0.0, "reversed, the viewport follows the finger: {reversed}");
    assert!((normal + reversed).abs() < 1e-5, "the same distance the other way: {normal} {reversed}");
}

#[test]
fn a_drag_is_the_same_number_of_pixels_of_content_whatever_the_zoom() {
    // the pad draws the sheets at a scale that follows the zoom; a drag of 60 px moves the viewport by 60 / scale units, so
    // the sheets move under the finger by 60 px at any zoom
    let pan_for = |view_w: f32| {
        let mut view = connected();
        let mut pen = pen_state();
        pen.view = Rect { x: 0.1, y: 1.6, w: view_w, h: view_w * 0.75 };
        view.pen = Some(pen);
        let (mut root, calls) = root_with(view);
        root.pad_event(0, PadEvent::Down { x: 100.0, y: 100.0 }, (300.0, 390.0));
        root.pad_event(5, PadEvent::Move { x: 112.0, y: 100.0 }, (300.0, 390.0));
        root.pad_event(10, PadEvent::Move { x: 172.0, y: 100.0 }, (300.0, 390.0));
        let sent = taken(&calls);
        let (pan, _) = sent.iter().rev().find_map(|c| if let Call::Gesture(_, Phase::Update, pan, ..) = c { Some(*pan) } else { None }).unwrap();
        (pan, view_w)
    };
    let (wide, wide_w) = pan_for(1.0);
    let (narrow, narrow_w) = pan_for(0.25);
    // 60 px of finger: the viewport moves by 60 / px_per_unit and px_per_unit follows 1 / view width
    assert!((wide / narrow - wide_w / narrow_w).abs() < 1e-3, "{wide} {narrow}");
}

#[test]
fn continue_reviewing_goes_to_the_mac() {
    let (mut root, calls) = root_with(connected());
    assert!(root.perform(10, Action::ContinueReviewing));
    assert_eq!(taken(&calls), [Call::Continue]);
}

fn with_finished(kind: FinishKind, settings: HostSettings) -> PhoneView {
    let mut view = connected();
    view.settings = settings;
    view.finished_count = 1;
    view.finished = Some(AgentFinished { conversation_id: 3, file: "layout/src/fc.rs".into(), line: 1337, kind });
    view
}

#[test]
fn an_agent_that_finishes_owes_a_buzz_by_the_settings_and_is_named_for_a_few_seconds() {
    for (kind, settings, wanted) in [
        (FinishKind::Done, HostSettings::default(), Some(azul_review_link::frame::Haptic::Success)),
        (FinishKind::Suggestion, HostSettings::default(), Some(azul_review_link::frame::Haptic::Warning)),
        (FinishKind::Failed, HostSettings::default(), Some(azul_review_link::frame::Haptic::Error)),
        (FinishKind::Failed, HostSettings { haptic_failed: false, ..HostSettings::default() }, None),
        (FinishKind::NoAction, HostSettings::default(), None),
        (FinishKind::Done, HostSettings { haptic_done: false, ..HostSettings::default() }, None),
        (FinishKind::Suggestion, HostSettings { haptic_suggestion: false, ..HostSettings::default() }, None),
    ] {
        let (mut root, _) = root_with(with_finished(kind, settings));
        assert!(root.tick(1_000), "news is a change");
        assert_eq!(root.take_haptic(), wanted, "{kind:?} {settings:?}");
        assert_eq!(root.take_haptic(), None, "a buzz is played once");
        assert_eq!(root.ui.finished_note.as_ref().map(|(done, _)| done.kind), Some(kind), "named even when it does not buzz");
        assert!(!root.tick(1_100), "nothing new");
        assert_eq!(root.take_haptic(), None);
        assert!(root.tick(1_000 + FINISHED_SHOWN_MS + 10), "the line goes away");
        assert!(root.ui.finished_note.is_none());
    }
}

#[test]
fn a_count_that_went_down_is_a_new_connection_and_buzzes_nothing() {
    let (mut root, _) = root_with(connected());
    root.ui.finished_seen = 5;
    root.tick(10);
    assert_eq!((root.ui.finished_seen, root.take_haptic(), root.ui.finished_note.is_none()), (0, None, true));
}

#[test]
fn two_quick_taps_fit_the_width_and_a_right_click_resets_the_view() {
    let (mut root, calls) = root_with(connected());
    for t in [0, 120] {
        root.pad_event(t, PadEvent::Down { x: 50.0, y: 50.0 }, (300.0, 390.0));
        root.pad_event(t + 20, PadEvent::Up { x: 50.0, y: 50.0 }, (300.0, 390.0));
    }
    assert_eq!(taken(&calls), [Call::Fit]);
    root.pad_event(500, PadEvent::Secondary, (300.0, 390.0));
    assert_eq!(taken(&calls), [Call::Reset]);
}

#[test]
fn a_pinch_zooms_about_the_pen_and_the_rectangle_shrinks() {
    let (mut root, calls) = root_with(connected());
    root.pad_event(0, PadEvent::Pinch { scale: 1.0, began: true, ended: false }, (300.0, 390.0));
    root.pad_event(10, PadEvent::Pinch { scale: 2.0, began: false, ended: false }, (300.0, 390.0));
    let shown = root.shown_view().unwrap();
    assert!((shown.w - 0.25).abs() < 1e-3, "half as wide at twice the zoom: {shown:?}");
    root.pad_event(20, PadEvent::Pinch { scale: 2.0, began: false, ended: true }, (300.0, 390.0));
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
        root.pad_event(0, event, (300.0, 390.0));
    }
    root.pad_event(1, PadEvent::Down { x: f32::NAN, y: 1.0 }, (f32::NAN, f32::NAN));
    assert!(taken(&calls).is_empty());
    assert_eq!(root.shown_view(), None);
}

#[test]
fn a_cancelled_gesture_ends_at_the_mac_and_the_hint_can_be_dismissed() {
    let (mut root, calls) = root_with(connected());
    root.pad_event(0, PadEvent::Down { x: 0.0, y: 0.0 }, (300.0, 390.0));
    root.pad_event(1, PadEvent::Move { x: 30.0, y: 0.0 }, (300.0, 390.0));
    root.pad_event(2, PadEvent::Cancel, (300.0, 390.0));
    let sent = taken(&calls);
    assert!(matches!(sent.last(), Some(Call::Gesture(_, Phase::End, ..))), "{sent:?}");
    assert_eq!(root.ui.live_base, None);
    root.ui.hint = true;
    root.perform(3, Action::DismissHint);
    assert!(!root.ui.hint);
}

// ---- pasting the pairing code

fn offer_text() -> String {
    azul_review_link::pairing::PairingOffer { ticket: "t-1".into(), secret: [3; 16] }.to_text()
}

fn toast_key(root: &Root) -> Option<&str> {
    root.ui.toast.as_ref().map(|t| t.0.as_str())
}

#[test]
fn the_text_of_an_offer_from_the_clipboard_starts_the_pairing() {
    let (mut root, calls) = root_with(PhoneView::new());
    root.ui.pair_again = true;
    root.perform(0, Action::PairPaste(format!("  {}\n", offer_text())));
    assert_eq!(taken(&calls), [Call::BeginPairing(format!("  {}\n", offer_text()))], "the text goes as it is: the connection trims it");
    assert_eq!(toast_key(&root), None);
    assert!(!root.ui.pair_again, "the pairing screen of a Mac that could not be reached is over");
}

#[test]
fn nothing_on_the_clipboard_says_so_and_asks_nothing_of_the_connection() {
    let (mut root, calls) = root_with(PhoneView::new());
    for text in ["", "   \n\t "] {
        root.perform(0, Action::PairPaste(text.into()));
        assert_eq!(toast_key(&root), Some("pair-paste-empty"), "{text:?}");
    }
    assert!(taken(&calls).is_empty());
}

#[test]
fn a_text_that_is_no_offer_says_why_in_words() {
    let good = offer_text();
    let mut damaged = good.clone();
    // the check of the offer is the third part: a changed digit is a damaged offer
    let at = good.splitn(3, ':').take(2).map(|p| p.len() + 1).sum::<usize>();
    let flipped = if damaged.as_bytes()[at] == b'0' { "1" } else { "0" };
    damaged.replace_range(at..=at, flipped);
    for (text, key) in [
        ("hello there".to_string(), "pair-paste-not-offer"),
        ("azrv9:00:00:x".to_string(), "pair-paste-version"),
        (damaged, "pair-paste-damaged"),
        ("x".repeat(5_000), "pair-paste-toolong"),
    ] {
        let (mut root, calls) = root_with(PhoneView::new());
        root.perform(0, Action::PairPaste(text.clone()));
        assert_eq!(toast_key(&root), Some(key), "{}", &text[..text.len().min(20)]);
        assert_eq!(taken(&calls).len(), 1, "the connection was asked and said no");
    }
}

// ---- trying it without a Mac

#[test]
fn trying_it_without_a_mac_puts_a_pretend_mac_in_place_of_the_connection_and_the_remote_comes_up() {
    let (mut root, calls) = root_with(PhoneView::new());
    assert!(!root.demo);
    assert!(root.perform(0, Action::Demo), "the screen changes");
    assert!(root.demo);
    assert_eq!(root.view().session, Session::Connected { mac_name: "Demo Mac".into() });
    assert!(!root.ui.pair_card && root.ui.screen == Screen::Remote, "the remote is what shows, not the pairing card");
    assert!(taken(&calls).is_empty(), "nothing is asked of the real connection");
    // the pretend Mac's pen walks on its own: after a moment the pad has something to show
    root.tick(0);
    root.tick(200);
    assert!(root.view().pen.is_some());
}

#[test]
fn a_second_tap_on_the_demo_key_changes_nothing_and_leaving_brings_the_real_connection_back() {
    let (mut root, _calls) = root_with(PhoneView::new());
    root.perform(0, Action::Demo);
    root.tick(0);
    root.tick(200);
    let pen = root.view().pen.clone();
    root.perform(300, Action::Demo);
    assert_eq!(root.view().pen, pen, "the same pretend Mac, not a new one");
    root.perform(400, Action::LeaveDemo);
    assert!(!root.demo);
    assert_eq!(root.view().session, Session::Unpaired, "the real connection, as it was");
}

// ---- the window's setup is done once

#[test]
fn the_windows_setup_is_asked_for_by_two_callbacks_and_done_once_in_either_order() {
    let (mut root, _calls) = root_with(PhoneView::new());
    assert!(!root.window_ready);
    assert!(root.first_window_ready(), "the first to say the window is ready does the setup");
    assert!(root.window_ready);
    assert!(!root.first_window_ready(), "the second finds it done");
    assert!(!root.first_window_ready(), "and so does every later mount of the screens");
}

// ---- the real connection comes when the window exists

#[test]
fn the_real_connection_replaces_the_stand_in_when_it_comes_and_the_screens_follow_it() {
    let (mut root, _calls) = root_with(PhoneView::new());
    root.connect_later = true;
    let mut searching = PhoneView::new();
    searching.session = Session::Searching { attempt: 1 };
    searching.mac_name = "Studio Mac".into();
    root.connect_now(Box::new(TestApi { view: searching, calls: Arc::new(Mutex::new(Vec::new())) }));
    assert!(!root.connect_later, "it is made once");
    assert_eq!(root.view().session, Session::Searching { attempt: 1 });
    assert_eq!(root.view().mac_name, "Studio Mac");
}

#[test]
fn a_real_connection_that_comes_while_the_demo_is_on_waits_behind_the_pretend_mac() {
    let (mut root, _calls) = root_with(PhoneView::new());
    root.perform(0, Action::Demo);
    let mut real = PhoneView::new();
    real.session = Session::Searching { attempt: 1 };
    root.connect_now(Box::new(TestApi { view: real, calls: Arc::new(Mutex::new(Vec::new())) }));
    assert_eq!(root.view().session, Session::Connected { mac_name: "Demo Mac".into() }, "the pretend Mac is still what the screens talk to");
    root.perform(10, Action::LeaveDemo);
    assert_eq!(root.view().session, Session::Searching { attempt: 1 }, "and leaving the demo brings the real connection that came meanwhile");
}
