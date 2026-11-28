//! The seam the phone finds its Mac through: what it saved, and who the neighbourhood says is there.

use super::*;
use crate::advert::{encode, project_hash};
use crate::frame::Role;

#[test]
fn saved_addresses_keep_the_newest_first_without_doubles_and_never_more_than_four() {
    let mut saved = SavedAddresses::new();
    assert!(saved.candidates().is_empty());
    for t in ["a", "b", "c", "d", "e"] {
        saved.remember(t);
    }
    assert_eq!(saved.candidates(), ["e", "d", "c", "b"], "the oldest is forgotten");
    saved.remember("c");
    assert_eq!(saved.candidates(), ["c", "e", "d", "b"], "a ticket seen again moves to the front");
    saved.remember("");
    assert_eq!(saved.candidates(), ["c", "e", "d", "b"], "an empty ticket is no ticket");
    saved.clear();
    assert!(saved.candidates().is_empty());
}

#[test]
fn what_was_kept_comes_back_in_the_same_order_and_is_cut_to_the_cap() {
    let kept: Vec<String> = ["new", "mid", "old"].iter().map(ToString::to_string).collect();
    assert_eq!(SavedAddresses::from_saved(&kept).candidates(), ["new", "mid", "old"]);
    let many: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
    assert_eq!(SavedAddresses::from_saved(&many).candidates(), ["t0", "t1", "t2", "t3"]);
}

#[test]
fn a_neighbour_that_announced_one_of_our_texts_is_read_and_any_other_text_is_only_a_key() {
    let ours = encode(&Advert { project: project_hash("the repo"), name: "Studio Mac".into(), user: "Felix".into(), role: Some(Role::Host) });
    let near = Nearby::from_announcement("abc", &ours, vec!["192.168.1.5:4000".into()]);
    assert_eq!(near.advert.as_ref().map(|a| a.name.as_str()), Some("Studio Mac"));
    let foreign = Nearby::from_announcement("def", "<script>alert(1)</script>", Vec::new());
    assert_eq!(foreign.advert, None, "what a stranger writes is not read");
    assert_eq!(foreign.endpoint_id, "def");
}

#[test]
fn a_mock_neighbourhood_says_who_came_and_who_went_once() {
    let mut mock = MockDiscovery::new();
    assert!(mock.nearby().is_empty() && mock.poll(0).is_empty());
    let a = Nearby::from_announcement("a", "", vec![]);
    mock.appear(a.clone());
    assert_eq!(mock.nearby(), std::slice::from_ref(&a));
    assert_eq!(mock.poll(1), [NearbyEvent::Appeared(a.clone())]);
    assert!(mock.poll(2).is_empty(), "news is told once");
    mock.vanish("a");
    mock.vanish("a");
    assert_eq!(mock.poll(3), [NearbyEvent::Gone("a".into())], "gone once, whatever the times it is said");
    assert!(mock.nearby().is_empty());
    mock.vanish("never-there");
    assert!(mock.poll(4).is_empty());
}

#[test]
fn a_neighbour_that_appears_again_replaces_the_old_entry() {
    let mock = MockDiscovery::new();
    mock.appear(Nearby::from_announcement("a", "", vec!["x".into()]));
    mock.appear(Nearby::from_announcement("a", "", vec!["y".into()]));
    assert_eq!(mock.nearby().len(), 1);
    assert_eq!(mock.nearby()[0].addresses, ["y"]);
}

#[test]
fn clones_see_the_same_neighbourhood() {
    let test = MockDiscovery::new();
    let mut owned = test.clone();
    test.appear(Nearby::from_announcement("a", "", vec![]));
    assert_eq!(owned.nearby().len(), 1);
    assert_eq!(owned.poll(0).len(), 1);
    assert!(test.clone().poll(1).is_empty(), "the news was taken by the first reader");
}

#[test]
fn the_news_of_a_neighbourhood_that_never_stops_is_kept_to_the_cap_and_the_oldest_give_way() {
    let mut queue = Vec::new();
    for i in 0..MAX_NEWS + 5 {
        push_news(&mut queue, NearbyEvent::Gone(format!("key-{i}")));
    }
    assert_eq!(queue.len(), MAX_NEWS, "never more than the cap");
    assert_eq!(queue.first(), Some(&NearbyEvent::Gone("key-5".to_string())), "the five oldest are gone");
    assert_eq!(queue.last(), Some(&NearbyEvent::Gone(format!("key-{}", MAX_NEWS + 4))), "and the newest is kept");
    // below the cap nothing is dropped, and the order is the order the news came in
    let mut few = Vec::new();
    push_news(&mut few, NearbyEvent::Gone("a".into()));
    push_news(&mut few, NearbyEvent::Gone("b".into()));
    assert_eq!(few, [NearbyEvent::Gone("a".to_string()), NearbyEvent::Gone("b".to_string())]);
}

#[test]
fn the_addresses_of_a_neighbour_are_the_ones_listed_for_its_own_key() {
    let listed = vec![
        Nearby::from_announcement("first", "", vec!["10.0.0.1:1".into()]),
        Nearby::from_announcement("second", "", vec!["10.0.0.2:2".into(), "10.0.0.2:3".into()]),
    ];
    assert_eq!(addresses_of(&listed, "second"), ["10.0.0.2:2", "10.0.0.2:3"]);
    assert_eq!(addresses_of(&listed, "first"), ["10.0.0.1:1"]);
    assert!(addresses_of(&listed, "third").is_empty(), "a neighbour that is not listed has no addresses");
    assert!(addresses_of(&[], "first").is_empty());
}
