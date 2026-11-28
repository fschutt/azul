//! The endpoint's late bind: nothing is bound when the app starts, a platform that is not ready yet is waited for, and a changed
//! setting binds nothing.

use super::*;
use crate::{make_kit, room_at_start, rooms, MeetState};

#[test]
fn the_start_binds_nothing() {
    // `start_rooms` runs the app, so what is held is its text: the start screen needs no endpoint, and a bind there would put the
    // system's question about the local network (iOS, macOS) in front of the first screen and, on Android, run in the library's
    // constructor before the activity exists (the engine then refuses: "the Android context is not ready")
    let source = include_str!("lib.rs");
    let body = source.split("fn start_rooms(").nth(1).and_then(|rest| rest.split("\nfn run(").next()).expect("the body of start_rooms");
    let code: Vec<&str> = body.lines().filter(|line| !line.trim_start().starts_with("//")).collect();
    assert!(code.len() > 10, "the body was found");
    for word in ["bind_endpoint(", "IrohEndpoint::bind", "ensure_endpoint(", "rebind_endpoint("] {
        assert!(!code.iter().any(|line| line.contains(word)), "start_rooms must not use {word}: the endpoint is bound when the first thing that needs it starts");
    }
}

#[test]
fn a_platform_that_is_not_ready_is_waited_for_a_second_at_a_time_and_twenty_times() {
    assert!(not_ready("the Android context is not ready"));
    assert!(not_ready("The Android context is Not Ready yet"));
    assert!(!not_ready("this build has no iroh engine"));
    let mut late = LateBind::default();
    assert!(!late.due(10_000), "nothing is scheduled");
    assert!(late.failed("the Android context is not ready", 5_000));
    assert!(!late.due(5_999) && late.due(6_000), "a second later");
    for _ in 1..BIND_RETRIES {
        assert!(late.failed("the Android context is not ready", 7_000));
    }
    assert!(!late.failed("the Android context is not ready", 7_000), "twenty tries and no more");
    assert!(!late.due(u64::MAX), "and nothing is scheduled then");
    // a bound endpoint forgets the tries, and says whether it looks on the local network
    late.bound(true);
    assert!(late.lan);
    assert!(late.failed("the Android context is not ready", 9_000), "the count starts again");
}

#[test]
fn the_local_network_is_looked_at_when_a_feature_asks_and_always_with_no_relay() {
    use crate::{lan_for, rooms::Relay};
    assert!(!lan_for(&Relay::Default, false), "a room with a relay needs no announcement on the local network");
    assert!(!lan_for(&Relay::Custom("https://relay.example".to_string()), false));
    assert!(lan_for(&Relay::Default, true), "a direct call by id asks for it");
    assert!(lan_for(&Relay::Off, false), "with no relay at all it is the way peers find each other");
    assert!(lan_for(&Relay::Off, true));
}

#[test]
fn any_other_reason_is_final() {
    let mut late = LateBind::default();
    assert!(!late.failed("this build has no iroh engine", 1_000));
    assert!(!late.due(5_000), "no engine is not waited for");
    assert!(late.failed("the Android context is not ready", 1_000));
    assert!(!late.failed("no socket could be opened", 1_500), "a different reason ends the waiting");
    assert!(!late.due(5_000));
}

#[test]
fn a_changed_relay_setting_binds_nothing_while_no_endpoint_is_bound_and_the_bind_that_comes_uses_it() {
    crate::l10n::in_english();
    let mut s = MeetState::new("Ada", make_kit());
    s.room = Some(room_at_start("http://127.0.0.1:8790".to_string(), rooms::Relay::Default, "--worker"));
    rebind_endpoint(&mut s, rooms::Relay::Off);
    assert!(s.endpoint.is_none(), "nothing was bound");
    assert_eq!(s.room.as_ref().map(|room| room.relay.clone()), Some(rooms::Relay::Off), "the choice is kept for the bind that comes");
}

#[test]
fn with_no_room_session_there_is_nothing_to_bind_for_and_nothing_is_bound() {
    crate::l10n::in_english();
    let mut s = MeetState::new("Ada", make_kit());
    s.room = None;
    assert!(!ensure_endpoint(&mut s, false));
    assert!(!s.late_bind.wanted, "nothing asked for the local network");
    assert!(!ensure_endpoint(&mut s, true));
    assert!(s.late_bind.wanted, "the wish is kept for the bind that comes");
    assert!(!ensure_endpoint(&mut s, false));
    assert!(s.late_bind.wanted, "and a later try that does not ask does not take it back");
    assert!(s.endpoint.is_none());
}

#[test]
fn an_endpoint_with_a_connection_is_in_use_and_one_without_is_not() {
    crate::l10n::in_english();
    let mut s = MeetState::new("Ada", make_kit());
    assert!(!endpoint_in_use(&s));
    s.direct.in_call = true;
    assert!(endpoint_in_use(&s));
    s.direct.in_call = false;
    s.chat_pending.push((1, "peer".to_string()));
    assert!(endpoint_in_use(&s));
}
