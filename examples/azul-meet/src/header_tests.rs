//! The line under the title row says what happened in a few words and never names the meetings
//! service's address: connection details belong in Settings > Meetings, not in the meeting's
//! own screens.

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
