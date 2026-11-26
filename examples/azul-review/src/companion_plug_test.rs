//! The phone companion in the review window without a window and without a socket: that it is off until the user turns it on, what
//! the file says, how it is bound and closed, how fast the host is looked at, what is written and when, and what the page offers.
//! A phone is the link crate's client over its in-process network.

use azul_review_link::{
    access::TrustRecord,
    client::{ClientConfig, PhoneClient},
    limits::{CONFIRM_TIMEOUT_MS, PAIRING_WINDOW_MS},
    mock::{Knobs, MockLink, MockNet},
    pairing::{PairingOffer, SeededEntropy},
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

// ---- the pairing

/// A phone that has no Mac yet.
fn new_phone(net: &MockNet, name: &str) -> PhoneClient<MockLink> {
    PhoneClient::new(net.endpoint(name), ClientConfig { phone_name: name.to_string(), app_version: "0.1.0".to_string() }, Box::new(MemoryStore::new()))
}

/// Both sides run for `ms` milliseconds from `from`; what the glue said over that time (any change, any attention, any save), and the clock.
fn pump(g: &mut Glue, phone: &mut PhoneClient<MockLink>, from: Millis, ms: u64) -> (Polled, Millis) {
    let mut all = Polled::default();
    for now in from..from + ms {
        let polled = g.poll_at(now);
        all.changed |= polled.changed;
        all.attention |= polled.attention;
        all.save |= polled.save;
        phone.poll(now);
    }
    (all, from + ms)
}

/// A bound companion that knows its address, and the clock after that.
fn listening(net: &MockNet, stored: &Stored) -> (Glue, Millis) {
    let mut g = glue(stored);
    g.bind_link(Box::new(net.endpoint("mac")), Box::new(SeededEntropy(7)));
    for now in 0..60 {
        g.poll_at(now);
    }
    assert!(g.has_address(), "the endpoint said where it is");
    (g, 60)
}

#[test]
fn no_code_is_made_before_the_companion_is_bound_or_knows_its_address_or_has_room_for_a_phone() {
    let mut off = glue(&Stored::default());
    assert_eq!(off.begin_pairing(0), Err(PairError::NotListening));
    let net = net();
    let mut early = glue(&on());
    early.bind_link(Box::new(net.endpoint("mac")), Box::new(SeededEntropy(7)));
    assert_eq!(early.begin_pairing(0), Err(PairError::NoAddress), "a moment after the bind the endpoint does not know its address");
    assert!(early.offer().is_none());
    let mut full = on();
    full.phones = (0..MAX_PAIRED_PHONES).map(|i| TrustRecord::phone(&format!("p{i}"), "Phone")).collect();
    let (mut g, _) = listening(&net, &full);
    assert_eq!(g.begin_pairing(61), Err(PairError::TooManyPhones));
}

#[test]
fn a_code_is_an_offer_a_phone_can_read_and_a_new_one_replaces_the_old() {
    let net = net();
    let (mut g, now) = listening(&net, &on());
    g.begin_pairing(now).expect("a code");
    let first = g.offer().expect("open").text.clone();
    let offer = PairingOffer::parse(&first).expect("the text is an offer the phone reads");
    assert!(!offer.ticket.is_empty());
    assert!(g.offer().is_some_and(|o| o.opened_at == now && !o.copied));
    assert!(!g.log().iter().any(|l| l.contains(&first)), "the code holds the secret: it is not in the log");
    g.begin_pairing(now + 10).expect("another");
    assert_ne!(g.offer().expect("open").text, first, "the one-time secret is new: the old code does not work");
    g.cancel_pairing();
    assert!(g.offer().is_none());
}

#[test]
fn a_phone_that_proves_the_secret_is_asked_about_and_pairs_when_the_person_says_the_digits_are_the_same() {
    let net = net();
    let (mut g, now) = listening(&net, &on());
    g.begin_pairing(now).expect("a code");
    let text = g.offer().expect("open").text.clone();
    let mut phone = new_phone(&net, "phone");
    phone.begin_pairing(now, &text).expect("the phone takes the offer");
    let (said, now) = pump(&mut g, &mut phone, now, 300);
    let asked = g.asked().expect("the phone asks").clone();
    assert!(said.attention && said.changed, "the window is told: {said:?}");
    assert_eq!(asked.peer, MockNet::key("phone"));
    assert_eq!(asked.name, "phone");
    assert!(asked.code < 1_000_000, "six digits");
    assert!(!g.log().iter().any(|l| l.contains(&format!("{:06}", asked.code)) || l.contains(&text)), "neither the digits nor the code are logged");
    // yes
    g.accept(now).expect("paired");
    assert_eq!(g.stored.phones, vec![TrustRecord::phone(&MockNet::key("phone"), "phone")], "kept at once");
    assert!(g.asked().is_none() && g.offer().is_none(), "the window is closed: the secret is spent");
    let (after, _) = pump(&mut g, &mut phone, now, 300);
    assert!(!after.save, "the event that follows finds it kept already: no second write");
    assert!(phone.is_active(), "the phone is served");
    assert_eq!(g.status(), Status::Listening { paired: 1, here: 1 });
    let (_, bytes) = g.to_write().expect("read");
    assert!(String::from_utf8(bytes).unwrap().contains(&MockNet::key("phone")), "and it is in what is written");
}

#[test]
fn a_phone_the_person_declines_is_told_no_and_nothing_is_paired() {
    let net = net();
    let (mut g, now) = listening(&net, &on());
    g.begin_pairing(now).expect("a code");
    let text = g.offer().expect("open").text.clone();
    let mut phone = new_phone(&net, "phone");
    phone.begin_pairing(now, &text).expect("offer");
    let (_, now) = pump(&mut g, &mut phone, now, 300);
    assert!(g.asked().is_some());
    g.decline(now);
    assert!(g.asked().is_none());
    pump(&mut g, &mut phone, now, 300);
    assert!(g.stored.phones.is_empty(), "nothing is paired");
    assert!(!phone.is_active(), "the phone was told no");
    assert!(g.log().iter().any(|l| l.contains("declined")), "{:?}", g.log());
}

#[test]
fn nothing_is_paired_when_nobody_waits_and_declining_nobody_is_nothing() {
    let net = net();
    let (mut g, now) = listening(&net, &on());
    assert_eq!(g.accept(now), Err(ConfirmError::NobodyWaiting), "no phone asks");
    g.decline(now);
    assert!(g.stored.phones.is_empty());
}

#[test]
fn a_forgotten_phone_is_gone_from_what_is_kept_and_from_the_host() {
    let net = net();
    let mut stored = on();
    stored.phones = vec![TrustRecord::phone(&MockNet::key("phone"), "Test phone"), TrustRecord::phone("other", "Other")];
    let (mut g, now) = listening(&net, &stored);
    let mut phone = phone_of(&net, "phone");
    let (_, now) = pump(&mut g, &mut phone, now, 400);
    assert_eq!(g.status(), Status::Listening { paired: 2, here: 1 });
    assert!(g.forget(now, &MockNet::key("phone")));
    assert_eq!(g.stored.phones, vec![TrustRecord::phone("other", "Other")]);
    assert_eq!(g.host_mut().map(|h| h.paired().len()), Some(1), "the host refuses it from now on");
    assert!(!g.forget(now, "never paired"), "a phone that was not paired is not forgotten");
    pump(&mut g, &mut phone, now, 600);
    assert_eq!(g.status(), Status::Listening { paired: 1, here: 0 });
}

#[test]
fn the_phones_are_listed_from_what_is_kept_while_nothing_is_bound_and_a_phone_is_forgotten_from_it() {
    let mut stored = on();
    stored.phones = vec![TrustRecord::phone("a", "First"), TrustRecord::phone("b", "Second")];
    let mut g = glue(&stored);
    let listed: Vec<(String, bool)> = g.phones().into_iter().map(|p| (p.name, p.connected)).collect();
    assert_eq!(listed, vec![("First".to_string(), false), ("Second".to_string(), false)]);
    assert!(g.forget(0, "a"));
    assert_eq!(g.phones().len(), 1);
    assert_eq!(g.stored.phones.len(), 1);
}

#[test]
fn a_code_is_good_for_as_long_as_the_host_keeps_its_window_open_and_a_phone_waits_two_minutes_for_the_person() {
    let net = net();
    let (mut g, now) = listening(&net, &on());
    g.begin_pairing(now).expect("a code");
    let polled = g.poll_at(now + PAIRING_WINDOW_MS + 1);
    assert!(g.offer().is_none(), "the code went out of date");
    assert!(polled.changed);
    // a phone asks and the person does not answer
    let (mut g, now) = listening(&net, &on());
    g.begin_pairing(now).expect("a code");
    let text = g.offer().expect("open").text.clone();
    let mut phone = new_phone(&net, "phone");
    phone.begin_pairing(now, &text).expect("offer");
    let (_, now) = pump(&mut g, &mut phone, now, 300);
    assert!(g.asked().is_some());
    g.poll_at(now + CONFIRM_TIMEOUT_MS + 1);
    assert!(g.asked().is_none(), "nobody answered: the bar goes");
}

#[test]
fn a_phone_that_goes_while_it_is_asked_about_is_no_longer_asked_about() {
    let net = net();
    let (mut g, now) = listening(&net, &on());
    g.begin_pairing(now).expect("a code");
    let text = g.offer().expect("open").text.clone();
    let mut phone = new_phone(&net, "phone");
    phone.begin_pairing(now, &text).expect("offer");
    let (_, now) = pump(&mut g, &mut phone, now, 300);
    assert!(g.asked().is_some());
    phone.close(now);
    pump(&mut g, &mut phone, now, 300);
    assert!(g.asked().is_none(), "it left");
}

#[test]
fn the_copy_of_a_code_is_marked_and_handed_back_and_a_closed_window_has_nothing_to_copy() {
    let net = net();
    let (mut g, now) = listening(&net, &on());
    assert_eq!(g.mark_copied(), None, "no code is open");
    g.begin_pairing(now).expect("a code");
    let text = g.offer().expect("open").text.clone();
    assert_eq!(g.mark_copied(), Some(text));
    assert!(g.offer().is_some_and(|o| o.copied));
    g.cancel_pairing();
    assert_eq!(g.mark_copied(), None);
}

#[test]
fn turning_the_companion_off_closes_the_window_and_drops_the_question() {
    let net = net();
    let (mut g, now) = listening(&net, &on());
    g.begin_pairing(now).expect("a code");
    assert!(g.set_enabled(false));
    assert!(g.offer().is_none() && g.asked().is_none());
}
