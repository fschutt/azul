//! What the phone companion shows besides its switch, built without a window: the key that makes a pairing code (or why there is none),
//! the code that is open, the phones that are paired and the bar that asks about the six digits.

use azul::{prelude::*, str::String as AzString};
use azul_review_link::{
    access::TrustRecord,
    client::{ClientConfig, PhoneClient},
    mock::{Knobs, MockNet},
    pairing::SeededEntropy,
    phone::PhoneApi,
    store::MemoryStore,
};

use super::*;
use crate::{companion_io::Loaded, companion_store::Stored, lib_test::app_with_file};

fn nodes(dom: &Dom) -> Vec<&Dom> {
    let mut out = vec![dom];
    for child in dom.children.as_slice() {
        out.extend(nodes(child));
    }
    out
}

fn texts(dom: &Dom) -> Vec<String> {
    nodes(dom).into_iter().filter_map(|n| n.root.node_type.get_text().into_option()).map(|t| t.as_str().to_string()).filter(|t| !t.trim().is_empty()).collect()
}

fn has_id(dom: &Dom, id: &AzString) -> bool {
    nodes(dom).into_iter().any(|n| n.root.has_id(id.clone()))
}

/// An app whose companion file said `stored`.
fn app(stored: &Stored) -> AppState {
    crate::l10n::in_english();
    let s = app_with_file();
    s.companion.borrow_mut().apply_loaded(Loaded { text: Some(stored.to_json()), problem: None });
    s
}

fn on() -> Stored {
    let mut stored = Stored::default();
    stored.enabled = true;
    stored
}

fn net() -> MockNet {
    MockNet::with_knobs(Knobs { latency_ms: 2, discovery: true, ..Knobs::default() })
}

/// Binds the app's companion over the mock network and lets it learn its address (when `address`).
fn listening(s: &AppState, net: &MockNet, address: bool) {
    let mut g = s.companion.borrow_mut();
    g.bind_link(Box::new(net.endpoint("mac")), Box::new(SeededEntropy(7)));
    if address {
        for now in 0..60 {
            g.poll_at(now);
        }
    }
}

fn any() -> RefAny {
    RefAny::new(0u8)
}

#[test]
fn there_is_nothing_to_show_while_it_is_off_and_no_phone_is_kept() {
    let s = app(&Stored::default());
    assert!(phones_block(&s, &any()).is_none());
    assert!(pairing_bar(&s, &any()).is_none());
}

#[test]
fn the_phones_that_are_kept_are_listed_with_a_key_each_to_forget_them_even_while_it_is_off() {
    let mut stored = Stored::default();
    stored.phones = vec![TrustRecord::phone("a", "First phone"), TrustRecord::phone("b", "Second phone")];
    let s = app(&stored);
    let block = phones_block(&s, &any()).expect("two phones to list");
    assert!(has_id(&block, &ids::COMPANION_PHONES));
    for index in 0..2 {
        assert!(has_id(&block, &AzString::from(format!("__azreview_companion-phone-{index}"))), "{index}");
        assert!(has_id(&block, &AzString::from(format!("__azreview_companion-forget-{index}"))), "{index}");
    }
    let said = texts(&block);
    assert!(said.contains(&"First phone".to_string()) && said.contains(&"Second phone".to_string()), "{said:?}");
    assert!(said.iter().any(|t| t == "not connected"), "{said:?}");
    assert!(!has_id(&block, &ids::COMPANION_PAIR), "no pairing while nothing listens");
}

#[test]
fn while_the_address_is_not_known_there_is_no_key_to_pair_and_the_block_says_it_waits() {
    let s = app(&on());
    listening(&s, &net(), false);
    let block = phones_block(&s, &any()).expect("listening");
    assert!(!has_id(&block, &ids::COMPANION_PAIR));
    assert!(texts(&block).iter().any(|t| t.contains("Waiting for this computer's address")), "{:?}", texts(&block));
}

#[test]
fn with_an_address_and_room_the_key_that_makes_a_code_is_there_and_no_phone_is_said_to_be_paired() {
    let s = app(&on());
    listening(&s, &net(), true);
    let block = phones_block(&s, &any()).expect("listening");
    assert!(has_id(&block, &ids::COMPANION_PAIR));
    assert!(texts(&block).iter().any(|t| t == "No phone is paired yet."), "{:?}", texts(&block));
}

#[test]
fn with_four_phones_paired_there_is_no_key_and_the_block_says_why() {
    let mut stored = on();
    stored.phones = (0..4).map(|i| TrustRecord::phone(&format!("p{i}"), "Phone")).collect();
    let s = app(&stored);
    listening(&s, &net(), true);
    let block = phones_block(&s, &any()).expect("listening");
    assert!(!has_id(&block, &ids::COMPANION_PAIR));
    assert!(texts(&block).iter().any(|t| t.starts_with("Four phones are paired")), "{:?}", texts(&block));
}

#[test]
fn an_open_code_is_shown_as_text_with_its_picture_and_the_keys_that_copy_and_close_it() {
    let s = app(&on());
    listening(&s, &net(), true);
    s.companion.borrow_mut().begin_pairing(61).expect("a code");
    let (text, has_qr) = {
        let g = s.companion.borrow();
        let offer = g.offer().expect("open");
        (offer.text.clone(), offer.qr.is_some())
    };
    let block = phones_block(&s, &any()).expect("listening");
    assert!(has_id(&block, &ids::COMPANION_OFFER) && has_id(&block, &ids::COMPANION_OFFER_TEXT));
    assert!(has_id(&block, &ids::COMPANION_COPY) && has_id(&block, &ids::COMPANION_CLOSE));
    assert!(!has_id(&block, &ids::COMPANION_PAIR), "a code is open: the key that makes one is not");
    assert_eq!(has_id(&block, &ids::COMPANION_QR), has_qr, "a picture when the code is short enough for one");
    assert!(texts(&block).contains(&text), "the code as text, to copy");
    assert!(!texts(&block).iter().any(|t| t == "Copied. It leaves the clipboard after a minute."));
    s.companion.borrow_mut().mark_copied();
    let copied = phones_block(&s, &any()).expect("listening");
    assert!(texts(&copied).iter().any(|t| t == "Copied. It leaves the clipboard after a minute."), "{:?}", texts(&copied));
}

#[test]
fn a_phone_that_asks_is_a_bar_with_its_name_the_six_digits_and_the_keys_that_say_yes_and_no() {
    let net = net();
    let s = app(&on());
    listening(&s, &net, true);
    s.companion.borrow_mut().begin_pairing(61).expect("a code");
    let text = s.companion.borrow().offer().expect("open").text.clone();
    let mut phone = PhoneClient::new(net.endpoint("phone"), ClientConfig { phone_name: "Test phone".to_string(), app_version: "0.1.0".to_string() }, Box::new(MemoryStore::new()));
    phone.begin_pairing(61, &text).expect("offer");
    for now in 61..400 {
        s.companion.borrow_mut().poll_at(now);
        phone.poll(now);
    }
    let code = s.companion.borrow().asked().expect("the phone asks").code;
    let bar = pairing_bar(&s, &any()).expect("a bar");
    assert!(has_id(&bar, &ids::COMPANION_ASK) && has_id(&bar, &ids::COMPANION_ACCEPT) && has_id(&bar, &ids::COMPANION_DECLINE));
    let said = texts(&bar);
    assert!(said.iter().any(|t| t == "A phone asks to be paired: Test phone"), "{said:?}");
    assert!(said.contains(&format_code(code)), "the six digits as people read them: {said:?}");
    // answered: the bar is gone
    s.companion.borrow_mut().decline(400);
    assert!(pairing_bar(&s, &any()).is_none());
}
