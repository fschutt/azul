//! The line under the title row says what happened in a few words and never names the meetings
//! service's address: connection details belong in Settings > Meetings, not in the meeting's
//! own screens. A refused camera or microphone puts "Open settings" in the same bar, and only while
//! the screen shows the device it is about.

use super::*;

const WORKER: &str = "http://127.0.0.1:8790";

fn room_with_a_silent_service() -> RoomSession {
    crate::l10n::in_english();
    RoomSession::new(WORKER.to_string(), rooms::Relay::Default)
}

#[test]
fn a_meetings_service_that_does_not_answer_is_said_without_its_address() {
    let _room = room_with_a_silent_service();
    let notice = service_notice(&Err(format!("{WORKER}/rooms: Connection refused (os error 61)")));
    assert!(!notice.is_empty());
    assert!(!notice.contains("127.0.0.1"), "{notice}");
}

#[test]
fn a_meetings_service_that_answers_with_an_error_is_said_without_its_address() {
    let _room = room_with_a_silent_service();
    let notice = service_notice(&Ok((503, None)));
    assert!(notice.contains("503"), "{notice}");
    assert!(!notice.contains("127.0.0.1"), "{notice}");
}

#[test]
fn the_waiting_room_of_a_meeting_made_here_does_not_name_the_service() {
    let room = room_with_a_silent_service();
    let notice = preview_notice(&room);
    assert!(!notice.is_empty());
    assert!(!notice.contains("127.0.0.1"), "{notice}");
    let notice = cannot_join_notice(&room);
    assert!(!notice.contains("127.0.0.1"), "{notice}");
}

/// The window's view with the devices on or off and refused or not (no window, no meeting).
fn devices(cam_on: bool, camera_denied: bool, mic_on: bool, mic_denied: bool) -> ui::CallView {
    crate::l10n::in_english();
    let mut s = MeetState::new("Ada", make_kit());
    s.room = Some(RoomSession::new(String::new(), rooms::Relay::Default));
    s.cam_on = cam_on;
    s.camera_denied = camera_denied;
    s.mic_on = mic_on;
    s.mic_denied = mic_denied;
    snapshot(&s)
}

#[test]
fn a_camera_that_is_on_and_refused_is_the_refusal_the_bar_offers_settings_for() {
    let view = devices(true, true, false, false);
    assert!(ui::camera_refused(&view));
    assert!(!ui::mic_refused(&view), "the microphone is neither on nor refused");
}

#[test]
fn a_microphone_that_is_on_and_refused_is_the_refusal_the_bar_offers_settings_for() {
    let view = devices(false, false, true, true);
    assert!(ui::mic_refused(&view));
    assert!(!ui::camera_refused(&view));
}

#[test]
fn a_refusal_of_a_device_that_is_off_shows_nothing_and_offers_nothing() {
    // refused earlier, turned off now: the tile and the meter show nothing of it, so the bar has no button
    let view = devices(false, true, false, true);
    assert!(!ui::camera_refused(&view));
    assert!(!ui::mic_refused(&view));
}

#[test]
fn a_device_that_is_allowed_offers_nothing() {
    let view = devices(true, false, true, false);
    assert!(!ui::camera_refused(&view));
    assert!(!ui::mic_refused(&view));
}

#[test]
fn a_test_pattern_and_a_test_tone_are_no_device_the_system_could_refuse() {
    crate::l10n::in_english();
    let mut s = MeetState::new("Ada", make_kit());
    s.room = Some(RoomSession::new(String::new(), rooms::Relay::Default));
    s.cam_on = true;
    s.camera_denied = true;
    s.pattern_video = true;
    s.mic_on = true;
    s.mic_denied = true;
    s.tone_mic = true;
    let view = snapshot(&s);
    assert!(!ui::camera_refused(&view), "a pattern needs no camera");
    assert!(!ui::mic_refused(&view), "a tone needs no microphone");
}
