//! The registry of requests that wait for the platform: ids, types, the limit, the lost ones, and one registry per App.

use std::time::Duration;

use super::*;

#[test]
fn a_request_is_taken_back_once_as_the_type_it_was_parked_as() {
    let registry = RequestRegistry::new();
    let id = registry.register(String::from("one")).unwrap();
    assert_ne!(id, 0, "0 is no id: a platform's default request code must not name a request");
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.take::<String>(id).as_deref(), Some("one"));
    assert!(registry.take::<String>(id).is_none(), "an answer that comes twice finds nothing");
    assert!(registry.is_empty());
}

#[test]
fn ids_count_up_and_a_taken_id_is_not_handed_out_again_at_once() {
    let registry = RequestRegistry::new();
    let a = registry.register(1_u8).unwrap();
    let b = registry.register(2_u8).unwrap();
    assert_ne!(a, b);
    registry.take::<u8>(a).unwrap();
    let c = registry.register(3_u8).unwrap();
    assert!(c > b, "a late answer for a taken id must not finish another request");
}

#[test]
fn the_ids_fit_an_android_request_code_and_never_land_on_a_request_that_waits() {
    let registry = RequestRegistry::new();
    // one request waits for ever; 140 000 others come and go: the ids wrap twice and never touch the waiting one
    let waiting = registry.register(String::from("waiting")).unwrap();
    for n in 0..140_000_u32 {
        let id = registry.register(n).unwrap();
        assert!((1..=MAX_ID).contains(&id), "{id}");
        assert_ne!(id, waiting);
        assert_eq!(registry.take::<u32>(id), Some(n));
    }
    assert_eq!(registry.take::<String>(waiting).as_deref(), Some("waiting"));
}

#[test]
fn another_type_leaves_the_request_where_it_is() {
    let registry = RequestRegistry::new();
    let id = registry.register(String::from("x")).unwrap();
    assert!(registry.take::<u64>(id).is_none());
    assert_eq!(registry.len(), 1, "the request is still parked");
    assert_eq!(registry.take::<String>(id).as_deref(), Some("x"));
}

#[test]
fn a_full_registry_hands_the_request_back_to_be_answered() {
    let registry = RequestRegistry::new();
    let ids: Vec<u64> = (0..MAX_PENDING).map(|n| registry.register(n).unwrap()).collect();
    assert_eq!(registry.len(), MAX_PENDING);
    assert_eq!(registry.register(99_usize), Err(99_usize), "refused, and the value comes back");
    assert_eq!(registry.take::<usize>(ids[3]), Some(3));
    assert!(registry.register(100_usize).is_ok(), "a slot is free again");
}

#[test]
fn clones_share_one_registry_and_two_registries_share_nothing() {
    let first = RequestRegistry::new();
    let second = RequestRegistry::new();
    let shared = first.clone();
    let id = shared.register(7_u32).unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 0, "another App's registry does not see it");
    assert!(second.take::<u32>(id).is_none());
    assert_eq!(first.take::<u32>(id), Some(7));
}

#[test]
fn what_waited_too_long_is_handed_over_and_the_rest_stays() {
    let registry = RequestRegistry::new();
    registry.register(String::from("old")).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let young = registry.register(String::from("young")).unwrap();
    let lost = registry.take_lost(Duration::from_millis(20));
    assert_eq!(lost.len(), 1);
    assert_eq!(lost.into_iter().next().unwrap().downcast::<String>().unwrap().as_str(), "old");
    assert_eq!(registry.len(), 1);
    assert_eq!(registry.take::<String>(young).as_deref(), Some("young"));
    assert!(registry.take_lost(Duration::ZERO).is_empty());
}

#[test]
fn a_parked_request_moves_between_threads() {
    let registry = RequestRegistry::new();
    let id = registry.register(vec![1_u8, 2, 3]).unwrap();
    let worker = registry.clone();
    let got = std::thread::spawn(move || worker.take::<Vec<u8>>(id)).join().unwrap();
    assert_eq!(got, Some(vec![1, 2, 3]));
}
