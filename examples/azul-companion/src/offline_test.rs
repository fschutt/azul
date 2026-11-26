//! The connection of a phone that has no network layer yet: it never finds a Mac, and says so, instead of pretending.

use super::*;
use azul_review_link::view::FailureKind;

#[test]
fn it_starts_unpaired_with_nothing_on_it() {
    let phone = OfflinePhone::new();
    assert_eq!(phone.view().session, Session::Unpaired);
    assert!(phone.view().pen.is_none() && phone.view().mac_name.is_empty());
}

#[test]
fn looking_for_a_mac_searches_a_moment_and_then_says_it_found_none() {
    let mut phone = OfflinePhone::new();
    phone.retry(1_000);
    assert_eq!(phone.view().session, Session::Searching { attempt: 1 });
    assert!(!phone.poll(1_500), "still looking");
    assert!(phone.poll(1_000 + SEARCH_MS), "the end of the search is a change");
    assert_eq!(phone.view().session, Session::Failed { kind: FailureKind::NotFound });
    assert!(!phone.poll(9_000), "and then nothing more happens");
}

#[test]
fn another_try_counts_up() {
    let mut phone = OfflinePhone::new();
    phone.retry(0);
    phone.poll(SEARCH_MS);
    phone.retry(5_000);
    assert_eq!(phone.view().session, Session::Searching { attempt: 2 });
    phone.poll(5_000 + SEARCH_MS);
    assert_eq!(phone.view().session, Session::Failed { kind: FailureKind::NotFound });
}

#[test]
fn a_pairing_text_that_is_no_offer_is_refused_and_forgetting_goes_back_to_the_start() {
    let mut phone = OfflinePhone::new();
    assert!(phone.begin_pairing(0, "this is no offer").is_err());
    assert_eq!(phone.view().session, Session::Unpaired, "a refused text starts nothing");
    phone.retry(0);
    phone.forget_mac(10);
    assert_eq!(phone.view().session, Session::Unpaired);
    assert!(!phone.poll(SEARCH_MS * 2), "the search that was under way is over");
}

#[test]
fn everything_the_screens_can_ask_of_it_is_harmless() {
    let mut phone = OfflinePhone::new();
    phone.gesture(0, 1, azul_review_link::frame::Phase::Begin, (0.0, 0.0), 1.0, azul_review_link::frame::Focus::Pen);
    phone.fit(1);
    phone.page_step(2, 1);
    phone.reset_view(3);
    phone.continue_reviewing(4);
    phone.open_file(5, "a.rs", None);
    phone.press_button(6, 0);
    assert!(phone.set_buttons(7, &azul_review_link::frame::ButtonSet::defaults()).is_empty());
    phone.list_dir(8, "src", 0);
    assert!(phone.view().files.error.is_some() && !phone.view().files.loading(), "a folder cannot be listed with no Mac");
    assert_eq!(phone.view().session, Session::Unpaired);
}

#[test]
fn cancelling_the_search_goes_back_to_the_start_and_the_search_is_over() {
    let mut phone = OfflinePhone::new();
    phone.retry(0);
    phone.cancel(10);
    assert_eq!(phone.view().session, Session::Unpaired);
    assert!(!phone.poll(SEARCH_MS * 2));
}
