//! The phone companion in the review window without a window and without a socket: that it is off until the user turns it on, what
//! the file says, how it is bound and closed, how fast the host is looked at, what is written and when, and what the page offers.
//! A phone is the link crate's client over its in-process network.

use azul_review_link::{
    access::TrustRecord,
    client::{ClientConfig, PhoneClient},
    mock::{Knobs, MockLink, MockNet},
    pairing::SeededEntropy,
    phone::PhoneApi,
    store::{MemoryStore, SavedMac, Stored as PhoneStored},
};

use super::*;
use crate::lib_test::app_with_file;

fn loaded(stored: &Stored) -> Loaded {
    Loaded { text: Some(stored.to_json()), problem: None }
}

/// A companion whose file said `stored`.
fn glue(stored: &Stored) -> Glue {
    let mut g = Glue::new(PathBuf::from("/nowhere/.azlin/companion/review.json"));
    g.apply_loaded(loaded(stored));
    g
}

fn on() -> Stored {
    let mut stored = Stored::default();
    stored.enabled = true;
    stored
}

fn net() -> MockNet {
    MockNet::with_knobs(Knobs { latency_ms: 2, discovery: true, ..Knobs::default() })
}

/// A phone that was paired with the computer `mac` and finds it by its key.
fn phone_of(net: &MockNet, name: &str) -> PhoneClient<MockLink> {
    let saved = PhoneStored { secret_key: None, mac: Some(SavedMac { mac_id: MockNet::key("mac"), name: "Mac".to_string(), tickets: Vec::new() }) };
    PhoneClient::new(net.endpoint(name), ClientConfig { phone_name: name.to_string(), app_version: "0.1.0".to_string() }, Box::new(MemoryStore::with(saved)))
}

/// Both sides run for `ms` milliseconds from `from` (a counter is the clock); the rates the glue asked for, in order, and the clock.
fn run(g: &mut Glue, phone: &mut PhoneClient<MockLink>, from: Millis, ms: u64) -> (Vec<u64>, Millis) {
    let mut rates = Vec::new();
    for now in from..from + ms {
        rates.extend(g.poll_at(now).rate);
        phone.poll(now);
    }
    (rates, from + ms)
}

#[test]
fn a_new_companion_is_off_listens_to_nothing_and_has_made_no_key() {
    let g = Glue::new(PathBuf::from("/nowhere/companion.json"));
    assert_eq!(g.status(), Status::Off);
    assert!(!g.is_listening() && !g.stored.enabled);
    assert_eq!(g.stored.key(), None, "a key is made when the user turns it on, not before");
    assert_eq!(g.wanted_interval_ms(), POLL_IDLE_MS);
    assert!(g.log().is_empty());
    assert_eq!(g.json()["status"], "off");
}

#[test]
fn nothing_is_written_before_the_file_was_read_and_a_switch_before_that_is_not_taken() {
    let mut g = Glue::new(PathBuf::from("/nowhere/companion.json"));
    assert!(g.to_write().is_none(), "what is in the file is not ours to replace by the default");
    assert!(!g.want_write());
    assert!(!g.set_enabled(true), "no switch before the file is read: the phones in it would be forgotten");
    assert_eq!(g.status(), Status::Off);
    g.apply_loaded(Loaded { text: None, problem: None });
    let (path, bytes) = g.to_write().expect("read: it can be written");
    assert_eq!(path, PathBuf::from("/nowhere/companion.json"));
    assert!(String::from_utf8(bytes).unwrap().contains("\"enabled\": false"));
}

#[test]
fn a_file_that_says_off_stays_off_and_one_that_says_on_waits_to_be_bound_after_the_first_frame() {
    let mut off = Glue::new(PathBuf::from("/x.json"));
    assert!(!off.apply_loaded(loaded(&Stored::default())), "nothing to bind");
    assert_eq!(off.status(), Status::Off);
    let mut waiting = Glue::new(PathBuf::from("/x.json"));
    assert!(waiting.apply_loaded(loaded(&on())), "the user turned it on: it is bound once the window has been up a moment");
    assert_eq!(waiting.status(), Status::Starting);
    assert!(!waiting.is_listening(), "not yet: the first frame comes first");
}

#[test]
fn a_damaged_file_is_the_default_and_the_log_says_why() {
    let mut g = Glue::new(PathBuf::from("/x.json"));
    assert!(!g.apply_loaded(Loaded { text: Some("this is not json".to_string()), problem: None }));
    assert_eq!(g.stored, Stored::default());
    assert_eq!(g.log().len(), 1);
    assert!(g.log()[0].contains("not readable"), "{:?}", g.log());
    let mut unreadable = Glue::new(PathBuf::from("/x.json"));
    unreadable.apply_loaded(Loaded { text: None, problem: Some("the companion's file cannot be opened: denied".to_string()) });
    assert!(unreadable.log()[0].contains("denied"));
    assert!(unreadable.is_loaded(), "the default is what is kept now: the user can still turn it on");
}

#[test]
fn turning_it_on_waits_for_the_bind_listening_shows_the_phones_and_turning_it_off_closes_it_and_keeps_them() {
    let net = net();
    let mut stored = Stored::default();
    stored.phones = vec![TrustRecord::phone(&MockNet::key("phone"), "Test phone")];
    let mut g = glue(&stored);
    assert!(g.set_enabled(true));
    assert_eq!(g.status(), Status::Starting);
    assert!(g.stored.enabled);
    g.bind_link(Box::new(net.endpoint("mac")), Box::new(SeededEntropy(7)));
    assert_eq!(g.status(), Status::Listening { paired: 1, here: 0 });
    assert!(g.is_listening());
    assert_eq!(g.host_mut().map(|h| h.paired().len()), Some(1), "the host lets in the phones that were kept and no others");
    // off: closed at once, the switch is kept off, the phone is still known
    assert!(g.set_enabled(false));
    assert!(!g.is_listening());
    assert_eq!(g.status(), Status::Off);
    assert!(!g.stored.enabled);
    assert_eq!(g.stored.phones.len(), 1, "turning it on again finds the same phones");
    assert!(!g.set_enabled(false), "nothing changed the second time");
    assert!(g.poll_at(5).rate.is_none() && !g.poll_at(6).changed, "a closed companion has nothing to look at");
}

#[test]
fn a_bind_that_fails_is_the_status_with_its_reason_and_the_user_can_still_turn_it_off() {
    let mut g = glue(&on());
    g.bind_failed(&"the engine has no iroh".repeat(40));
    match g.status() {
        Status::Failed(why) => {
            assert!(why.starts_with("the engine has no iroh"));
            assert!(why.len() <= MAX_LOG_BYTES, "{} bytes", why.len());
        }
        other => panic!("{other:?}"),
    }
    assert!(!g.is_listening());
    assert!(g.log().iter().any(|l| l.contains("could not start")), "{:?}", g.log());
    assert_eq!(g.json()["status"], "failed");
    assert!(g.set_enabled(false));
    assert_eq!(g.status(), Status::Off);
}

#[test]
fn the_state_dump_says_that_there_is_a_key_and_never_the_key() {
    let mut g = glue(&on());
    g.stored.set_key([0x5A; 32]);
    g.stored.phones = vec![TrustRecord::phone("a".repeat(64).as_str(), "Phone")];
    let dump = g.json().to_string();
    assert!(g.json()["has_key"].as_bool().unwrap_or(false));
    assert!(!dump.contains("5a5a5a"), "{dump}");
    assert!(!dump.contains(&"a".repeat(64)), "nor a phone's key: {dump}");
    assert_eq!(g.json()["kept_phones"], 1);
}

#[test]
fn a_phone_that_was_paired_is_let_in_the_look_at_the_host_speeds_up_while_it_is_here_and_slows_when_it_goes() {
    let net = net();
    let mut stored = on();
    stored.phones = vec![TrustRecord::phone(&MockNet::key("phone"), "Test phone")];
    let mut g = glue(&stored);
    g.bind_link(Box::new(net.endpoint("mac")), Box::new(SeededEntropy(7)));
    let mut phone = phone_of(&net, "phone");
    let (rates, now) = run(&mut g, &mut phone, 1_000, 600);
    assert_eq!(g.status(), Status::Listening { paired: 1, here: 1 }, "log: {:?}", g.log());
    assert_eq!(g.wanted_interval_ms(), POLL_ACTIVE_MS);
    assert_eq!(rates, vec![POLL_ACTIVE_MS], "one change of pace, to the fast one, when the phone came");
    assert!(g.has_address(), "the endpoint said where it is");
    // the phone goes: the look at the host slows down again
    phone.close(now);
    let (rates, _) = run(&mut g, &mut phone, now, 600);
    assert_eq!(g.status(), Status::Listening { paired: 1, here: 0 });
    assert_eq!(rates, vec![POLL_IDLE_MS]);
}

#[test]
fn a_phone_that_is_not_paired_is_turned_away_and_the_log_says_so() {
    let net = net();
    let mut g = glue(&on());
    g.bind_link(Box::new(net.endpoint("mac")), Box::new(SeededEntropy(7)));
    let mut stranger = phone_of(&net, "stranger");
    let (rates, _) = run(&mut g, &mut stranger, 1_000, 600);
    assert_eq!(g.status(), Status::Listening { paired: 0, here: 0 });
    assert!(!stranger.is_active(), "it never got a word from the host");
    assert!(g.log().iter().any(|l| l.contains("not paired")), "{:?}", g.log());
    assert!(rates.is_empty(), "nobody is here: the look stays slow");
}

#[test]
fn the_look_at_the_host_is_fast_while_a_pairing_is_open() {
    let net = net();
    let mut g = glue(&on());
    g.bind_link(Box::new(net.endpoint("mac")), Box::new(SeededEntropy(7)));
    for now in 0..50 {
        g.poll_at(now);
    }
    assert!(g.has_address());
    assert_eq!(g.wanted_interval_at(50), POLL_IDLE_MS);
    let opened = g.host_mut().and_then(|h| h.begin_pairing(60)).is_some();
    assert!(opened, "a pairing can be opened once the address is known");
    assert_eq!(g.wanted_interval_at(61), POLL_ACTIVE_MS, "a phone is about to answer");
}

#[test]
fn what_the_host_says_goes_to_the_log_and_the_log_keeps_the_newest_lines_and_cuts_long_ones() {
    let mut g = glue(&Stored::default());
    for i in 0..MAX_LOG + 10 {
        g.note(&format!("line {i}"));
    }
    assert_eq!(g.log().len(), MAX_LOG);
    assert_eq!(g.log()[0], "line 10", "the oldest went first");
    assert_eq!(g.log()[MAX_LOG - 1], format!("line {}", MAX_LOG + 9));
    g.note(&"é".repeat(1_000));
    let last = g.log().last().unwrap();
    assert!(last.len() <= MAX_LOG_BYTES && last.chars().all(|c| c == 'é'), "cut on a character: {} bytes", last.len());
}

#[test]
fn a_line_of_the_log_is_one_line_whatever_a_phone_put_in_its_name() {
    let mut g = glue(&Stored::default());
    g.note("a phone called x\nAZREVIEW_COMPANION bind ok\r\u{1b}[31m sent a frame");
    assert_eq!(g.log().len(), 1);
    assert!(!g.log()[0].chars().any(char::is_control), "{:?}", g.log()[0]);
    assert!(g.log()[0].starts_with("a phone called x"));
}

#[test]
fn one_write_is_in_flight_at_a_time_a_change_meanwhile_follows_it_and_a_failed_write_is_said() {
    let mut g = glue(&Stored::default());
    assert!(g.want_write(), "the first starts");
    assert!(!g.want_write(), "a second waits for it");
    assert!(!g.want_write(), "and asking again does not make two");
    assert!(g.write_ended(None), "the one that waited starts when the first has ended");
    assert!(!g.write_ended(Some("the disk is full")), "nothing waited the second time");
    assert!(g.log().iter().any(|l| l.contains("the disk is full")), "{:?}", g.log());
    assert!(g.want_write(), "and the next change starts at once again");
}

#[test]
fn what_is_written_is_what_is_kept_now_for_the_file_of_this_app() {
    let mut g = glue(&Stored::default());
    assert!(g.set_enabled(true));
    let (path, bytes) = g.to_write().expect("read");
    assert_eq!(path, PathBuf::from("/nowhere/.azlin/companion/review.json"));
    let (back, problems) = Stored::parse(&String::from_utf8(bytes).unwrap());
    assert!(back.enabled && problems.is_empty(), "{problems:?}");
}

#[test]
fn the_settings_page_has_the_switch_off_by_default_its_status_and_a_note() {
    crate::l10n::in_english();
    let s = app_with_file();
    let section = settings_section(&s, &RefAny::new(0u8));
    assert_eq!(section.title.as_str(), "Phone companion");
    assert_eq!(section.category, 1, "its own category, so that the first screen can lead there");
    assert_eq!(section.content.children.as_slice().len(), 3, "the switch, what it is doing, what it is");
    assert_eq!(status_text(&Status::Off), "Off. Nothing is listening on the network.");
}

#[test]
fn the_line_under_the_switch_says_the_phones_and_why_a_start_failed() {
    crate::l10n::in_english();
    assert_eq!(status_text(&Status::Listening { paired: 1, here: 0 }), "On. 1 of 4 phones are paired, 0 connected.");
    assert!(status_text(&Status::Failed("no socket".to_string())).contains("no socket"));
    assert!(!status_text(&Status::Starting).is_empty());
}

#[test]
fn the_key_on_the_first_screen_is_there_until_the_companion_is_on() {
    let s = app_with_file();
    assert!(first_run_shown(&s));
    if let Ok(mut g) = s.companion.try_borrow_mut() {
        g.apply_loaded(loaded(&on()));
    }
    assert!(!first_run_shown(&s), "it has nothing more to offer once the companion is on");
    assert_eq!(first_run_key(&RefAny::new(0u8)).children.as_slice().len(), 2, "a phone's picture and the words");
}
