//! The rules of the frames themselves: how each travels, who may send it, the quick buttons' caps, which paths
//! the Mac may look up, and what two protocol versions make of each other.

use super::*;
use crate::access::ScreenId;
use crate::limits::*;

fn any_frames() -> Vec<Frame> {
    let gesture = |phase| Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 1, op: ViewportOp::Gesture { id: 1, phase, pan_x: 0.0, pan_y: 0.0, zoom: 1.0, focus: Focus::Pen } });
    vec![
        Frame::Hello(Hello { role: Role::Phone, protocol: 1, min_protocol: 1, app_version: String::new(), name: String::new(), ticket: String::new() }),
        Frame::Ping { nonce: 1 },
        Frame::Pong { nonce: 1 },
        Frame::Bye,
        Frame::PairRequest { proof: [0; 32], phone_name: String::new() },
        gesture(Phase::Begin),
        gesture(Phase::Update),
        gesture(Phase::End),
        Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 1, op: ViewportOp::Fit }),
        Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 1, op: ViewportOp::PageStep(1) }),
        Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 1, op: ViewportOp::Reset }),
        Frame::ListDir { request_id: 1, path: String::new(), offset: 0 },
        Frame::OpenFile { screen: ScreenId::MAIN, request_id: 1, path: String::new(), line: None },
        Frame::Button { screen: ScreenId::MAIN, request_id: 1, index: 0 },
        Frame::SetButtons { request_id: 1, buttons: ButtonSet::defaults() },
        Frame::NeedThumbnail { id: 1 },
        Frame::PairAccepted { code: 1 },
        Frame::PairRejected(PairReject::Busy),
        Frame::Refused(Refusal::NotPaired),
        Frame::PenState(PenState::empty()),
        Frame::Thumbnail(Thumbnail { id: 1, width: 1, height: 1, format: ThumbFormat::Rgba8, bytes: vec![0; 4] }),
        Frame::Rows(Rows { request_id: 1, dir: String::new(), offset: 0, entries: Vec::new(), more: false }),
        Frame::Buttons(ButtonSet::defaults()),
        Frame::Reply(Reply { request_id: 1, status: ReplyStatus::Done }),
        Frame::Notice(Notice { level: NoticeLevel::Info, text: String::new() }),
        Frame::ContinueReviewing { screen: ScreenId::MAIN },
        Frame::Settings(HostSettings::default()),
        Frame::AgentFinished(AgentFinished { conversation_id: 1, file: String::new(), line: 0, kind: FinishKind::Done }),
    ]
}

// ---- how a frame travels

#[test]
fn the_pen_state_and_the_gesture_updates_are_newest_wins_each_on_its_own_track() {
    assert_eq!(Frame::PenState(PenState::empty()).delivery(), Delivery::Latest(TRACK_PEN));
    let update = Frame::Viewport(ViewportCmd {
        screen: ScreenId::MAIN,
        seq: 1,
        op: ViewportOp::Gesture { id: 1, phase: Phase::Update, pan_x: 0.1, pan_y: 0.0, zoom: 1.0, focus: Focus::Pen },
    });
    assert_eq!(update.delivery(), Delivery::Latest(TRACK_VIEWPORT));
    assert_ne!(TRACK_PEN, TRACK_VIEWPORT);
}

#[test]
fn everything_else_is_reliable_so_a_begin_an_end_a_fit_a_button_are_never_dropped() {
    for frame in any_frames() {
        let latest = matches!(frame, Frame::PenState(_))
            || matches!(frame, Frame::Viewport(ViewportCmd { op: ViewportOp::Gesture { phase: Phase::Update, .. }, .. }));
        if !latest {
            assert_eq!(frame.delivery(), Delivery::Reliable, "{}", frame.name());
        }
    }
}

#[test]
fn each_frame_belongs_to_one_direction_or_to_both() {
    let host_to_phone = ["pair accepted", "pair rejected", "refused", "pen state", "thumbnail", "rows", "buttons", "reply", "notice", "settings", "agent finished"];
    let phone_to_host = ["pair request", "viewport", "list dir", "open file", "button", "set buttons", "need thumbnail", "continue reviewing"];
    for frame in any_frames() {
        let name = frame.name();
        assert_eq!(frame.is_host_to_phone(), host_to_phone.contains(&name), "{name}");
        assert_eq!(frame.is_phone_to_host(), phone_to_host.contains(&name), "{name}");
        assert!(!(frame.is_host_to_phone() && frame.is_phone_to_host()), "{name}");
    }
}

// ---- the quick buttons

#[test]
fn the_default_buttons_are_the_four_placeholders_and_fit_their_caps() {
    let set = ButtonSet::defaults();
    let labels: Vec<&str> = set.buttons.iter().map(|b| b.label.as_str()).collect();
    assert_eq!(labels, ["Tone", "Explain", "Shorter", "Doc"]);
    assert_eq!(set.buttons[0].text, "Slop wording: please rewrite this in a normal, plain tone.");
    for b in &set.buttons {
        assert!(b.label.chars().count() <= MAX_LABEL_CHARS && b.text.chars().count() <= MAX_BUTTON_TEXT_CHARS);
    }
    let (clean, capped) = set.sanitized();
    assert_eq!(clean, set);
    assert!(capped.is_empty());
}

#[test]
fn a_button_set_is_cleaned_and_cut_and_says_what_it_cut() {
    let mut set = ButtonSet::defaults();
    set.buttons[0].label = "  A label that is far too long  ".into();
    set.buttons[1].text = format!("tab\u{9}\u{0}and\nnewline {}", "z".repeat(400));
    set.buttons[2].label = "Ok\u{7}".into();
    let (clean, capped) = set.sanitized();
    assert_eq!(clean.buttons[0].label, "A label that");
    assert!(clean.buttons[0].label.chars().count() <= MAX_LABEL_CHARS);
    assert_eq!(clean.buttons[1].text.chars().count(), MAX_BUTTON_TEXT_CHARS);
    assert!(clean.buttons[1].text.starts_with("taband\nnewline"), "{}", clean.buttons[1].text);
    assert!(!clean.buttons[1].text.contains('\u{0}'));
    assert!(clean.buttons[1].text.contains("and\nnewline"), "a newline stays in an instruction");
    assert_eq!(clean.buttons[2].label, "Ok");
    let what: Vec<&str> = capped.iter().map(|c| c.what).collect();
    assert_eq!(what, ["button label", "button text"], "{capped:?}");
    assert_eq!(capped[0].limit, MAX_LABEL_CHARS);
    assert_eq!(capped[1].limit, MAX_BUTTON_TEXT_CHARS);
}

// ---- paths

#[test]
fn a_path_inside_the_workspace_is_safe() {
    for path in ["", "src", "src/lib.rs", "a/b/c.txt", "ünï/cöde.rs", "dir with spaces/x y", ".hidden/x", "a..b/c", "file:name/x", "-rf/x"] {
        assert!(is_safe_relative_path(path), "{path:?}");
    }
}

#[test]
fn a_path_that_leaves_the_workspace_or_is_not_a_path_is_not() {
    let long = "p".repeat(MAX_PATH_BYTES + 1);
    for path in [
        "/etc/passwd",
        "../x",
        "a/../b",
        "a/..",
        "..",
        ".",
        "./a",
        "a/./b",
        "a//b",
        "a/",
        "a\\b",
        "..\\x",
        "C:/Windows",
        "c:x",
        "a\0b",
        "a\nb",
        "a\u{1b}b",
        long.as_str(),
    ] {
        assert!(!is_safe_relative_path(path), "{path:?}");
    }
    assert!(is_safe_relative_path(&"p".repeat(MAX_PATH_BYTES)));
}

// ---- versions

fn hello(role: Role, protocol: u16, min_protocol: u16) -> Hello {
    Hello { role, protocol, min_protocol, app_version: String::new(), name: String::new(), ticket: String::new() }
}

#[test]
fn two_builds_of_the_same_protocol_speak_it() {
    assert_eq!(negotiate(Role::Host, (1, 1), &hello(Role::Phone, 1, 1)), Ok(1));
    assert_eq!(negotiate(Role::Phone, (1, 1), &hello(Role::Host, 1, 1)), Ok(1));
}

#[test]
fn a_newer_peer_that_still_understands_ours_speaks_ours() {
    assert_eq!(negotiate(Role::Host, (3, 1), &hello(Role::Phone, 2, 1)), Ok(2));
    assert_eq!(negotiate(Role::Phone, (2, 1), &hello(Role::Host, 3, 1)), Ok(2));
    assert_eq!(negotiate(Role::Host, (1, 1), &hello(Role::Phone, 4, 1)), Ok(1));
}

#[test]
fn a_phone_too_old_for_the_mac_is_told_to_update_the_companion() {
    assert_eq!(negotiate(Role::Host, (3, 2), &hello(Role::Phone, 1, 1)), Err(Refusal::UpdateCompanion));
    assert_eq!(negotiate(Role::Phone, (1, 1), &hello(Role::Host, 3, 2)), Err(Refusal::UpdateCompanion));
}

#[test]
fn a_mac_too_old_for_the_phone_is_told_to_update_azreview() {
    assert_eq!(negotiate(Role::Host, (1, 1), &hello(Role::Phone, 3, 2)), Err(Refusal::UpdateAzReview));
    assert_eq!(negotiate(Role::Phone, (3, 2), &hello(Role::Host, 1, 1)), Err(Refusal::UpdateAzReview));
}

#[test]
fn a_protocol_of_zero_is_older_than_any_build_understands() {
    assert_eq!(negotiate(Role::Host, (1, 1), &hello(Role::Phone, 0, 0)), Err(Refusal::UpdateCompanion));
    assert_eq!(negotiate(Role::Phone, (1, 1), &hello(Role::Host, 0, 0)), Err(Refusal::UpdateAzReview));
}

#[test]
fn this_build_agrees_with_itself() {
    let me = hello(Role::Phone, PROTOCOL_VERSION, MIN_PROTOCOL_VERSION);
    assert_eq!(negotiate(Role::Host, (PROTOCOL_VERSION, MIN_PROTOCOL_VERSION), &me), Ok(PROTOCOL_VERSION));
}

// ---- what the settings make of a finished agent

#[test]
fn a_result_buzzes_as_a_success_a_suggestion_as_a_warning_a_failure_as_an_error_and_nothing_to_do_never_buzzes() {
    let on = HostSettings::default();
    assert_eq!(on.haptic_for(FinishKind::Done), Some(Haptic::Success));
    assert_eq!(on.haptic_for(FinishKind::Suggestion), Some(Haptic::Warning));
    assert_eq!(on.haptic_for(FinishKind::Failed), Some(Haptic::Error));
    assert_eq!(on.haptic_for(FinishKind::NoAction), None);
}

#[test]
fn each_kind_of_buzz_is_switched_off_on_its_own_and_both_off_is_silence() {
    let only_done = HostSettings { haptic_suggestion: false, ..HostSettings::default() };
    assert_eq!((only_done.haptic_for(FinishKind::Done), only_done.haptic_for(FinishKind::Suggestion)), (Some(Haptic::Success), None));
    let only_suggestion = HostSettings { haptic_done: false, ..HostSettings::default() };
    assert_eq!((only_suggestion.haptic_for(FinishKind::Done), only_suggestion.haptic_for(FinishKind::Suggestion)), (None, Some(Haptic::Warning)));
    let only_failed = HostSettings { haptic_done: false, haptic_suggestion: false, ..HostSettings::default() };
    assert_eq!(
        [FinishKind::Done, FinishKind::Suggestion, FinishKind::Failed, FinishKind::NoAction].map(|k| only_failed.haptic_for(k)),
        [None, None, Some(Haptic::Error), None]
    );
    let off = HostSettings { haptic_done: false, haptic_suggestion: false, haptic_failed: false, ..HostSettings::default() };
    for kind in [FinishKind::Done, FinishKind::Suggestion, FinishKind::NoAction, FinishKind::Failed] {
        assert_eq!(off.haptic_for(kind), None, "{kind:?}");
    }
}

#[test]
fn the_defaults_of_the_settings_are_direct_manipulation_no_snap_and_every_buzz_on() {
    let d = HostSettings::default();
    assert_eq!((d.reverse_pan, d.snap_to_page_top, d.haptic_done, d.haptic_suggestion, d.haptic_failed), (false, false, true, true, true));
}

#[test]
fn a_pen_state_over_pages_has_a_file_and_one_over_the_board_has_none() {
    let mut state = PenState::empty();
    state.surface = Surface::Page(PageSurface { file: "a/b.rs".into(), ..PageSurface::empty() });
    assert_eq!((state.file(), state.page().is_some(), state.board().is_some()), ("a/b.rs", true, false));
    state.surface = Surface::Whiteboard(WhiteboardSurface { board_id: 1, name: "Layout".into(), aspect: 1.5, item_count: 3 });
    assert_eq!((state.file(), state.page().is_some(), state.board().is_some()), ("", false, true));
}
