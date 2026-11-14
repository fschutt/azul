//! Checks R19 and R20 of lane W1-S46a: blocked members (planning/architecture.md section 3.9: "a member blocked in
//! AzChat or AzDrive can no longer reach the blocker: no messages, shares or invites"; Apple guideline 1.2).
//! Every test enters through the public API of the crate.

mod common;

use azlin_cbor::{DecodeError, ShapeError, Value};
use azlin_reports::{BlockError, BlockList, Contact, MAX_BLOCKED};

use common::*;

const KINDS: [Contact; 4] = [
    Contact::Message,
    Contact::Share,
    Contact::Invite,
    Contact::Request,
];

/// A member key: distinct for distinct `n`, ascending in `n`.
fn member(n: u32) -> [u8; 32] {
    let mut key = [0x5A; 32];
    key[28..].copy_from_slice(&n.to_be_bytes());
    key[0] = 0;
    key
}

fn members_value(keys: &[Vec<u8>]) -> Value {
    Value::Array(keys.iter().map(|k| Value::Bytes(k.clone())).collect())
}

fn list_bytes(version: u64, keys: &[Vec<u8>]) -> Vec<u8> {
    cbor(&raw_map(vec![
        (1, Value::Uint(version)),
        (2, members_value(keys)),
    ]))
}

// ---------------------------------------------------------------- R19

#[test]
fn r19_a_blocked_member_cannot_reach_the_blocker() {
    let mut list = BlockList::new();
    assert!(list.is_empty());
    let x = member(7);
    let other = member(8);
    for kind in KINDS {
        assert!(list.may_reach(&x, kind), "nobody is blocked yet: {kind:?}");
    }
    list.block(x).unwrap();
    for kind in KINDS {
        assert!(
            !list.may_reach(&x, kind),
            "a blocked member reached the blocker by {kind:?}"
        );
        assert!(
            list.may_reach(&other, kind),
            "an unblocked member was affected: {kind:?}"
        );
    }
    // A key that differs from the blocked one in a single bit is another member.
    let mut near = x;
    near[31] ^= 1;
    assert!(list.may_reach(&near, Contact::Message));
    // Blocking twice is fine and changes nothing.
    list.block(x).unwrap();
    assert_eq!(list.len(), 1);
    // Unblock restores, for every kind; unblocking a member nobody blocked does nothing.
    list.unblock(&other);
    assert_eq!(list.len(), 1);
    list.unblock(&x);
    for kind in KINDS {
        assert!(list.may_reach(&x, kind), "unblock did not restore {kind:?}");
    }
    assert!(list.is_empty());
}

#[test]
fn r19_ten_thousand_blocks_are_the_limit() {
    assert_eq!(MAX_BLOCKED, 10_000);
    assert_eq!(azlin_reports::BLOCK_VERSION, 1);
    let mut list = BlockList::new();
    for n in 0..MAX_BLOCKED as u32 {
        list.block(member(n))
            .unwrap_or_else(|e| panic!("block {n}: {e}"));
    }
    assert_eq!(list.len(), MAX_BLOCKED);
    // 10,001 blocks are refused, and nothing changed.
    assert_eq!(
        list.block(member(MAX_BLOCKED as u32)),
        Err(BlockError::Full)
    );
    assert_eq!(list.len(), MAX_BLOCKED);
    assert!(list.may_reach(&member(MAX_BLOCKED as u32), Contact::Message));
    // A member that is blocked already is not a new one, so it is fine at the limit.
    assert_eq!(list.block(member(5)), Ok(()));
    // Unblocking makes room.
    list.unblock(&member(5));
    assert_eq!(list.block(member(MAX_BLOCKED as u32)), Ok(()));
    assert!(!list.may_reach(&member(MAX_BLOCKED as u32), Contact::Invite));
    // The full list round-trips through its byte form.
    let bytes = list.to_bytes().unwrap();
    assert_eq!(BlockList::parse(&bytes).unwrap(), list);
}

const GOLDEN_BLOCKLIST: &str = "a2010102825820010101010101010101010101010101010101010101010101010101010101010158200202020202020202020202020202020202020202020202020202020202020202";

#[test]
fn r19_the_byte_form_round_trips_strict_and_bounded() {
    for count in [0u32, 1, 2, 50, 3_000] {
        let mut list = BlockList::new();
        for n in 0..count {
            list.block(member(n * 3 + 1)).unwrap();
        }
        let bytes = list.to_bytes().unwrap();
        let back = BlockList::parse(&bytes).unwrap_or_else(|e| panic!("{count} members: {e}"));
        assert_eq!(back, list);
        assert_eq!(
            back.to_bytes().unwrap(),
            bytes,
            "{count} members: not canonical"
        );
        for n in 0..count {
            assert!(!back.may_reach(&member(n * 3 + 1), Contact::Share));
            assert!(back.may_reach(&member(n * 3), Contact::Share));
        }
    }
    // The wire form is pinned (made by an independent encoder: scratch golden.py): a2 (map of 2) 01 01 (version
    // 1) 02 82 (two members) 58 20 (32 bytes) 01 x32, 58 20 02 x32.
    let mut list = BlockList::new();
    list.block([2; 32]).unwrap();
    list.block([1; 32]).unwrap();
    assert_eq!(hex(&list.to_bytes().unwrap()), GOLDEN_BLOCKLIST);
    assert_eq!(BlockList::parse(&unhex(GOLDEN_BLOCKLIST)).unwrap(), list);
}

#[test]
fn r19_the_byte_form_refuses_what_is_not_the_strict_form() {
    let key = |n: u8| vec![n; 32];
    let ok = list_bytes(1, &[key(1), key(2)]);
    assert_eq!(hex(&ok), GOLDEN_BLOCKLIST);
    assert!(BlockList::parse(&ok).is_ok());
    let thousands = |n: u32| -> Vec<Vec<u8>> { (0..n).map(|i| member(i).to_vec()).collect() };

    let mut trailing = ok.clone();
    trailing.push(0);
    let cases = [
        (
            "a wrong version",
            list_bytes(2, &[key(1)]),
            BlockError::Version(2),
        ),
        (
            "members out of order",
            list_bytes(1, &[key(2), key(1)]),
            BlockError::NotAscending,
        ),
        (
            "a duplicate member",
            list_bytes(1, &[key(1), key(1)]),
            BlockError::NotAscending,
        ),
        (
            "a member of 31 bytes",
            list_bytes(1, &[key(1), vec![2; 31]]),
            BlockError::MemberLength(1),
        ),
        (
            "a member of 33 bytes",
            list_bytes(1, &[vec![1; 33]]),
            BlockError::MemberLength(0),
        ),
        (
            "a member that is no byte string",
            cbor(&raw_map(vec![
                (1, Value::Uint(1)),
                (2, Value::Array(vec![Value::Bytes(key(1)), Value::Uint(5)])),
            ])),
            BlockError::MemberLength(1),
        ),
        (
            "10,001 members",
            list_bytes(1, &thousands(MAX_BLOCKED as u32 + 1)),
            BlockError::TooMany,
        ),
        (
            "11,000 members",
            list_bytes(1, &thousands(11_000)),
            BlockError::Cbor(DecodeError::TooManyItems),
        ),
        (
            "trailing bytes",
            trailing,
            BlockError::Cbor(DecodeError::TrailingBytes),
        ),
        (
            "no members",
            cbor(&raw_map(vec![(1, Value::Uint(1))])),
            BlockError::Shape(ShapeError::Missing(2)),
        ),
        (
            "no version",
            cbor(&raw_map(vec![(2, members_value(&[]))])),
            BlockError::Shape(ShapeError::Missing(1)),
        ),
        (
            "members that are no array",
            cbor(&raw_map(vec![
                (1, Value::Uint(1)),
                (2, Value::Bytes(key(1))),
            ])),
            BlockError::Shape(ShapeError::WrongType(2)),
        ),
        (
            "empty input",
            vec![],
            BlockError::Cbor(DecodeError::Truncated),
        ),
        (
            "a float",
            vec![0xa1, 0x01, 0xf9, 0x3c, 0x00],
            BlockError::Cbor(DecodeError::DisallowedType),
        ),
        (
            "an unknown key in crit",
            cbor(&raw_map(vec![
                (0, Value::Array(vec![Value::Uint(9)])),
                (1, Value::Uint(1)),
                (2, members_value(&[])),
                (9, Value::Null),
            ])),
            BlockError::Shape(ShapeError::CritUnknown(9)),
        ),
        (
            "a list over 512 KiB",
            vec![0; 512 * 1024 + 1],
            BlockError::Cbor(DecodeError::InputTooLarge),
        ),
    ];
    for (name, bytes, expected) in &cases {
        assert_eq!(
            BlockList::parse(bytes).unwrap_err(),
            *expected,
            "the malformed list '{name}' failed with another error"
        );
    }
    // The limit itself is fine.
    assert!(BlockList::parse(&list_bytes(1, &thousands(MAX_BLOCKED as u32))).is_ok());
}

#[test]
fn r19_a_device_that_does_not_know_a_field_keeps_it() {
    // A newer writer added key 9 and lists key 2 (the members) in crit: this reader understands the members,
    // ignores key 9, and writes both back (rule R18: a copy keeps crit and the unknown fields).
    let bytes = cbor(&raw_map(vec![
        (0, Value::Array(vec![Value::Uint(2)])),
        (1, Value::Uint(1)),
        (2, members_value(&[vec![1; 32]])),
        (9, Value::Text("a field of a newer app".into())),
    ]));
    let mut list = BlockList::parse(&bytes).unwrap();
    assert_eq!(
        list.to_bytes().unwrap(),
        bytes,
        "a copy of a structure with crit equals the original bytes"
    );
    // Blocking one more keeps them as well.
    list.block([3; 32]).unwrap();
    let again = BlockList::parse(&list.to_bytes().unwrap()).unwrap();
    assert_eq!(again, list);
    let rewritten = list.to_bytes().unwrap();
    let value = azlin_cbor::decode(&rewritten, &azlin_cbor::Limits::OBJECT).unwrap();
    let map = value.as_map().unwrap();
    assert!(
        map.get(&Value::Uint(9)).is_some(),
        "the unknown field was dropped"
    );
    assert!(map.get(&Value::Uint(0)).is_some(), "crit was dropped");
}

#[test]
fn r19_debug_prints_the_number_of_members_and_not_who() {
    let mut list = BlockList::new();
    list.block([0x7E; 32]).unwrap();
    let printed = format!("{list:?}");
    assert_eq!(printed, "BlockList { members: 1 }");
    assert!(!printed.contains("7e"));
}

// ---------------------------------------------------------------- R20

#[test]
fn r20_the_block_list_is_order_independent() {
    let mut rng = Rng::new(20);
    let mut keys: Vec<[u8; 32]> = (0..60).map(|_| rng.bytes::<32>()).collect();
    let mut sorted = BlockList::new();
    let mut ascending = keys.clone();
    ascending.sort();
    for k in &ascending {
        sorted.block(*k).unwrap();
    }
    let reference = sorted.to_bytes().unwrap();
    for seed in 0..500u64 {
        let mut rng = Rng::new(seed);
        rng.shuffle(&mut keys);
        let mut list = BlockList::new();
        for k in &keys {
            list.block(*k).unwrap();
            // Blocking a member twice, now and then, changes nothing either.
            if rng.one_in(5) {
                list.block(*k).unwrap();
            }
        }
        assert_eq!(
            list.to_bytes().unwrap(),
            reference,
            "seed {seed}: the bytes depend on the order of blocking"
        );
    }
    // The members come out sorted.
    let listed: Vec<[u8; 32]> = sorted.members().copied().collect();
    assert_eq!(listed, ascending);
}
