//! C3, C4 and C9: the author's signature. Every field of the pointer is covered (the unknown ones, `crit`
//! and every chunk reference included), the signature is bound to the space and to the author's key, an
//! unknown device and a revoked or unavailable directory are four different refusals, and the rule that
//! makes "ignore unknown fields" compatible with signatures holds: a copy keeps what it does not
//! understand and still verifies.
//!
//! The signatures of a "foreign" writer in the C9 tests are made by hand (`azlin_keys::sign::sign` over
//! the preimage the test assembles), never by `PointerV2::sign`.

mod pointer_common;

use azlin_cbor::Value;
use azlin_content::pointer::{
    ChunkList, ChunkListRef, Decoded, PointerError, PointerV2, UnknownFields,
};
use azlin_content::{ChunkLen, ChunkRef, FileSize, FileTime, Full, PackId, PackOffset};
use azlin_keys::sign::{sign as keys_sign, POINTER_V2};
use azlin_keys::{
    ChainSeq, DeviceId, DeviceState, DirectoryEntry, MapDirectory, Signature, UnixTime, VerifyAt,
};
use pointer_common::*;

fn verify_now(p: &PointerV2) -> Result<(), PointerError> {
    p.verify(&space(), &directory(), VerifyAt::Now).map(|_| ())
}

/// The signature a foreign writer makes: the preimage assembled by hand, azlin-keys' public `sign`.
fn hand_sign(p: &PointerV2, key: &azlin_keys::SignKeypair) -> Signature {
    let mut message = space().as_bytes().to_vec();
    message.extend_from_slice(&reference_encode(p, false));
    keys_sign(&POINTER_V2, &message, key)
}

fn inline(p: &mut PointerV2) -> &mut Vec<ChunkRef> {
    match &mut p.chunks {
        ChunkList::Inline(refs) => refs,
        ChunkList::Object(_) => panic!("an inline list"),
    }
}

type Flip = (&'static str, Box<dyn Fn(&mut PointerV2)>);

fn add(list: &mut Vec<Flip>, name: &'static str, f: impl Fn(&mut PointerV2) + 'static) {
    list.push((name, Box::new(f)));
}

/// A chunk reference with another value in every field than `chunk(0x99, 777, 818)`.
fn other_chunk() -> ChunkRef {
    chunk(0x99, 777, 818)
}

/// One change of one field at a time, over every field of a fully populated pointer (the list is
/// checked against the struct's fields at compile time below).
fn flips() -> Vec<Flip> {
    let mut v: Vec<Flip> = Vec::new();
    add(&mut v, "size", |p| p.size = FileSize::new(p.size.get() + 1));
    add(&mut v, "mtime seconds", |p| {
        p.mtime = FileTime::new(p.mtime.secs() + 1, p.mtime.nanos()).expect("a time")
    });
    add(&mut v, "mtime nanoseconds", |p| {
        p.mtime = FileTime::new(p.mtime.secs(), p.mtime.nanos() + 1).expect("a time")
    });
    add(&mut v, "mode", |p| p.mode ^= 1);
    add(&mut v, "blake3", |p| p.blake3[31] ^= 1);
    add(&mut v, "transform id", |p| {
        p.transform.as_mut().expect("a transform").id += 1
    });
    add(&mut v, "transform out_len", |p| {
        let t = p.transform.as_mut().expect("a transform");
        t.out_len = FileSize::new(t.out_len.get() + 1);
    });
    add(&mut v, "transform params", |p| {
        p.transform.as_mut().expect("a transform").params = Value::Text("other".to_string())
    });
    add(&mut v, "transform removed", |p| p.transform = None);
    add(&mut v, "chunk 0 full", |p| {
        inline(p)[0].full = Full::from_bytes([0xEE; 32])
    });
    add(&mut v, "chunk 0 raw_len", |p| {
        let r = &mut inline(p)[0];
        r.raw_len = ChunkLen::new(r.raw_len.get() - 1).expect("a length");
    });
    add(&mut v, "chunk 0 pack", |p| {
        inline(p)[0].pack = PackId::from_bytes([0xEE; 16])
    });
    add(&mut v, "chunk 0 offset", |p| {
        let r = &mut inline(p)[0];
        r.offset = PackOffset::new(r.offset.get() + 1);
    });
    add(&mut v, "chunk 0 stored_len", |p| {
        let r = &mut inline(p)[0];
        r.stored_len = ChunkLen::new(r.stored_len.get() - 1).expect("a length");
    });
    add(&mut v, "chunk added", |p| inline(p).push(other_chunk()));
    add(&mut v, "chunk removed", |p| {
        inline(p).pop();
    });
    add(&mut v, "chunks swapped", |p| inline(p).swap(0, 1));
    add(&mut v, "chunks as an object", |p| {
        p.chunks = ChunkList::Object(ChunkListRef {
            name: [0xCC; 32],
            count: 1025,
            total_raw: FileSize::new(2_600_000),
        })
    });
    add(&mut v, "preview 0 kind", |p| p.previews[0].kind += 1);
    add(&mut v, "preview 0 w", |p| p.previews[0].w += 1);
    add(&mut v, "preview 0 h", |p| p.previews[0].h += 1);
    add(&mut v, "preview 0 chunk", |p| {
        p.previews[0].chunks[0].full = Full::from_bytes([0xEE; 32])
    });
    add(&mut v, "preview chunk added", |p| {
        p.previews[0].chunks.push(other_chunk())
    });
    add(&mut v, "preview added", |p| {
        p.previews.push(azlin_content::pointer::Preview {
            kind: 9,
            w: 1,
            h: 1,
            chunks: vec![other_chunk()],
        })
    });
    add(&mut v, "preview removed", |p| {
        p.previews.pop();
    });
    add(&mut v, "previews swapped", |p| p.previews.swap(0, 1));
    add(&mut v, "stream 0 kind", |p| p.streams[0].kind += 1);
    add(&mut v, "stream 0 w", |p| p.streams[0].w += 1);
    add(&mut v, "stream 0 h", |p| p.streams[0].h += 1);
    add(&mut v, "stream 0 bandwidth", |p| {
        p.streams[0].bandwidth += 1
    });
    add(&mut v, "stream 0 chunk", |p| {
        p.streams[0].chunks[1].offset = PackOffset::new(1)
    });
    add(&mut v, "stream added", |p| {
        p.streams.push(azlin_content::pointer::StreamRef {
            kind: 9,
            w: 1,
            h: 1,
            bandwidth: 1,
            chunks: vec![other_chunk()],
        })
    });
    add(&mut v, "stream removed", |p| {
        p.streams.pop();
    });
    add(&mut v, "xattr name", |p| p.xattrs[0].0.push('x'));
    add(&mut v, "xattr value", |p| p.xattrs[0].1[0] ^= 1);
    add(&mut v, "xattr added", |p| {
        p.xattrs.push(("user.new".to_string(), vec![1]))
    });
    add(&mut v, "xattr removed", |p| {
        p.xattrs.pop();
    });
    add(&mut v, "xattrs swapped", |p| p.xattrs.swap(0, 1));
    add(&mut v, "label changed", |p| p.labels[0].push('x'));
    add(&mut v, "label added", |p| p.labels.push("new".to_string()));
    add(&mut v, "label removed", |p| {
        p.labels.pop();
    });
    add(&mut v, "labels swapped", |p| p.labels.swap(0, 1));
    add(&mut v, "derived extractor", |p| {
        p.derived[0].extractor.push('x')
    });
    add(&mut v, "derived model_id", |p| {
        p.derived[0].model_id.push('x')
    });
    add(&mut v, "derived model_version", |p| {
        p.derived[0].model_version.push('x')
    });
    add(&mut v, "derived record", |p| {
        p.derived[0].record.full = Full::from_bytes([0xEE; 32])
    });
    add(&mut v, "derived added", |p| {
        p.derived.push(azlin_content::pointer::DerivedRef {
            extractor: "new".to_string(),
            model_id: String::new(),
            model_version: String::new(),
            record: other_chunk(),
        })
    });
    add(&mut v, "derived removed", |p| {
        p.derived.pop();
    });
    add(&mut v, "created_at", |p| {
        p.created_at = UnixTime::from_secs(p.created_at.as_secs() + 1)
    });
    add(&mut v, "author_device", |p| p.author_device = other_id());
    add(&mut v, "unknown value", |p| {
        p.unknown = UnknownFields::from_entries(vec![
            (40, Value::Text("changed".to_string())),
            (
                41,
                Value::Array(vec![Value::Uint(1), Value::Bytes(vec![9, 9, 9])]),
            ),
        ])
        .expect("keys")
    });
    add(&mut v, "unknown key", |p| {
        p.unknown = UnknownFields::from_entries(vec![
            (42, Value::Text("from a newer minor version".to_string())),
            (
                43,
                Value::Array(vec![Value::Uint(1), Value::Bytes(vec![9, 9, 9])]),
            ),
        ])
        .expect("keys")
    });
    add(&mut v, "unknown added", |p| {
        p.unknown = UnknownFields::from_entries(vec![
            (40, Value::Text("from a newer minor version".to_string())),
            (
                41,
                Value::Array(vec![Value::Uint(1), Value::Bytes(vec![9, 9, 9])]),
            ),
            (44, Value::Null),
        ])
        .expect("keys")
    });
    add(&mut v, "unknown removed", |p| {
        p.unknown = UnknownFields::from_entries(vec![(
            40,
            Value::Text("from a newer minor version".to_string()),
        )])
        .expect("keys")
    });
    add(&mut v, "unknown all removed", |p| {
        p.unknown = UnknownFields::none()
    });
    add(&mut v, "crit added", |p| p.crit = vec![2, 7]);
    add(&mut v, "crit removed", |p| p.crit = Vec::new());
    add(&mut v, "sig", |p| {
        let mut bytes = *p.sig.expect("signed").as_bytes();
        bytes[10] ^= 1;
        p.sig = Some(Signature::from_bytes(bytes));
    });
    v
}

/// Every field of the pointer is named here: a field added to `PointerV2` stops this from compiling, and
/// the table of `flips` must then grow with it.
fn every_field_is_in_the_table(p: &PointerV2) {
    let PointerV2 {
        size: _,
        mtime: _,
        mode: _,
        blake3: _,
        transform: _,
        chunks: _,
        author_device: _,
        sig: _,
        previews: _,
        streams: _,
        xattrs: _,
        labels: _,
        derived: _,
        created_at: _,
        unknown: _,
        crit: _,
    } = p;
}

// ---------------------------------------------------------------------------------------------
// C3
// ---------------------------------------------------------------------------------------------

#[test]
fn c3_signature_roundtrip_and_every_field_is_covered() {
    let base = signed(full_with_unknown());
    every_field_is_in_the_table(&base);
    // Sign then verify passes (the control of every flip below).
    base.verify(&space(), &directory(), VerifyAt::Now)
        .expect("the signed pointer verifies");
    // ... also after a round trip through the bytes.
    let bytes = base.to_cbor().expect("writes");
    let Decoded::Current(read_back) = PointerV2::from_cbor(&bytes, &OBJECT).expect("reads") else {
        panic!("current");
    };
    assert_eq!(verify_now(&read_back), Ok(()));

    let flips = flips();
    assert!(flips.len() >= 55, "{} flips", flips.len());
    for (name, flip) in &flips {
        let mut changed = base.duplicate();
        flip(&mut changed);
        assert!(changed != base, "{name}: the flip changed nothing");
        assert_eq!(
            verify_now(&changed),
            Err(PointerError::BadSignature),
            "{name}: a changed field must break the signature"
        );
    }
}

#[test]
fn c3_a_pointer_without_a_signature_is_unsigned() {
    let p = full();
    assert_eq!(verify_now(&p), Err(PointerError::Unsigned));
    // Unsigned is the first answer, before the directory is asked.
    let mut down = directory();
    down.set_unavailable(true);
    assert_eq!(
        p.verify(&space(), &down, VerifyAt::Now).err(),
        Some(PointerError::Unsigned)
    );
    // Removing the signature of a signed pointer makes it unsigned again.
    let mut q = signed(full());
    q.sig = None;
    assert_eq!(verify_now(&q), Err(PointerError::Unsigned));
}

#[test]
fn c3_signing_is_deterministic_and_covers_the_whole_pointer_body() {
    let mut a = full_with_unknown();
    a.sign(&space(), &author_key()).expect("signs");
    let mut b = a.duplicate();
    b.sign(&space(), &author_key()).expect("signs again");
    assert_eq!(a.sig, b.sig, "Ed25519 is deterministic");
    // A different key signs differently, and a stale signature is replaced.
    let mut c = a.duplicate();
    c.sign(&space(), &other_key()).expect("signs");
    assert_ne!(a.sig, c.sig);
    let mut d = a.duplicate();
    d.mode ^= 1;
    d.sign(&space(), &author_key())
        .expect("signs the changed body");
    assert_ne!(a.sig, d.sig);
    assert_eq!(verify_now(&d), Ok(()));
    // The signature is over the body the reference encoder writes, after the space id.
    let mut message = space().as_bytes().to_vec();
    message.extend_from_slice(&reference_encode(&a, false));
    assert_eq!(a.sig, Some(keys_sign(&POINTER_V2, &message, &author_key())));
}

#[test]
fn c3_signing_refuses_a_pointer_that_would_not_be_written() {
    // A pointer this build would refuse to write is not signed, and keeps the signature it had.
    let mut p = full();
    p.size = FileSize::new(p.size.get() + 1);
    p.transform = None; // 3,000,001 against chunks of 2,600,000
    assert_eq!(
        p.sign(&space(), &author_key()),
        Err(PointerError::SizeMismatch)
    );
    assert_eq!(p.sig, None);
    let mut q = signed(full());
    let before = q.sig;
    q.mode = 0o17777;
    assert_eq!(q.sign(&space(), &author_key()), Err(PointerError::BadMode));
    assert_eq!(q.sig, before, "the old signature is left as it was");
}

#[test]
fn c3_the_content_is_judged_after_the_signature() {
    // A pointer that is validly signed and breaks a rule: a foreign signer can make one. `verify` says so
    // after the signature has passed; a flipped field of a valid pointer is a bad signature instead.
    let mut p = full();
    p.mode = 0o17777;
    p.sig = Some(hand_sign(&p, &author_key()));
    assert_eq!(verify_now(&p), Err(PointerError::BadMode));
    // The same bytes are refused at decode.
    assert_eq!(
        PointerV2::from_cbor(&reference_encode(&p, true), &OBJECT).err(),
        Some(PointerError::BadMode)
    );
}

// ---------------------------------------------------------------------------------------------
// C4
// ---------------------------------------------------------------------------------------------

#[test]
fn c4_signature_is_bound_to_space_and_author() {
    let p = signed(full());
    assert_eq!(verify_now(&p), Ok(()));

    // Another space: a pointer signed for one space is refused in another.
    assert_eq!(
        p.verify(&other_space(), &directory(), VerifyAt::Now).err(),
        Some(PointerError::BadSignature)
    );

    // The author replaced by another device of the directory: its key does not verify the signature.
    let mut swapped = p.duplicate();
    swapped.author_device = other_id();
    assert_eq!(verify_now(&swapped), Err(PointerError::BadSignature));

    // Signed by a key that is not the author's: the author's key does not verify it.
    let mut wrong_key = full();
    wrong_key.sign(&space(), &other_key()).expect("signs");
    assert_eq!(verify_now(&wrong_key), Err(PointerError::BadSignature));

    // A device the directory does not know: unknown is a state, never "valid".
    let mut stranger = p.duplicate();
    stranger.author_device = DeviceId::from_bytes([0xEE; 16]);
    assert_eq!(verify_now(&stranger), Err(PointerError::UnknownAuthor));
    // The directory is asked before the signature: a stranger with a bad signature is a stranger.
    stranger.mode ^= 1;
    assert_eq!(verify_now(&stranger), Err(PointerError::UnknownAuthor));
}

#[test]
fn c4_revoked_not_yet_active_and_unavailable_are_different_refusals() {
    let p = signed(full());
    let key = author_key().verify_key();
    let entry = |now: DeviceState| DirectoryEntry {
        key,
        now,
        added_at: UnixTime::from_secs(100),
        removed_at: Some(UnixTime::from_secs(500)),
        added_seq: ChainSeq::new(5),
        removed_seq: Some(ChainSeq::new(10)),
    };
    let mut dir = MapDirectory::new();
    dir.insert(author_id(), entry(DeviceState::Active));
    let check = |when: VerifyAt| p.verify(&space(), &dir, when).map(|_| ());

    // At a position of the team log or the space chain: the instant that matters (the repository log
    // entry's), not the pointer's own `created_at`.
    assert_eq!(check(VerifyAt::ChainSeq(ChainSeq::new(5))), Ok(()));
    assert_eq!(check(VerifyAt::ChainSeq(ChainSeq::new(9))), Ok(()));
    assert_eq!(
        check(VerifyAt::ChainSeq(ChainSeq::new(4))),
        Err(PointerError::AuthorNotYetActive)
    );
    assert_eq!(
        check(VerifyAt::ChainSeq(ChainSeq::new(10))),
        Err(PointerError::AuthorRevoked)
    );
    // At a time.
    assert_eq!(check(VerifyAt::Time(UnixTime::from_secs(300))), Ok(()));
    assert_eq!(
        check(VerifyAt::Time(UnixTime::from_secs(99))),
        Err(PointerError::AuthorNotYetActive)
    );
    assert_eq!(
        check(VerifyAt::Time(UnixTime::from_secs(500))),
        Err(PointerError::AuthorRevoked)
    );
    // A backdated pointer does not help a revoked device: `created_at` is not what is asked.
    let mut backdated = p.duplicate();
    backdated.created_at = UnixTime::from_secs(200);
    backdated.sign(&space(), &author_key()).expect("signs");
    assert_eq!(
        backdated
            .verify(&space(), &dir, VerifyAt::ChainSeq(ChainSeq::new(11)))
            .err(),
        Some(PointerError::AuthorRevoked)
    );

    // The state now.
    for (state, expected) in [
        (DeviceState::Active, Ok(())),
        (DeviceState::Pending, Err(PointerError::AuthorNotYetActive)),
        (DeviceState::Revoked, Err(PointerError::AuthorRevoked)),
    ] {
        let mut d = MapDirectory::new();
        d.insert(author_id(), entry(state));
        assert_eq!(p.verify(&space(), &d, VerifyAt::Now).map(|_| ()), expected);
    }

    // A directory that cannot answer is not an unknown device and not a valid one.
    let mut down = directory();
    down.set_unavailable(true);
    assert_eq!(
        p.verify(&space(), &down, VerifyAt::Now).err(),
        Some(PointerError::DirectoryUnavailable)
    );
}

#[test]
fn c4_the_witness_is_about_that_pointer_and_that_check() {
    let p = signed(full());
    let dir = directory();
    let at = VerifyAt::ChainSeq(ChainSeq::new(7));
    let verified = p.verify(&space(), &dir, at).expect("verifies");
    assert_eq!(verified.author(), author_id());
    assert_eq!(verified.space(), space());
    assert_eq!(verified.at(), at);
    assert!(
        std::ptr::eq(verified.pointer(), &p),
        "it borrows the pointer"
    );
    assert_eq!(verified.size, p.size, "it dereferences to the pointer");
    assert!(format!("{verified:?}").contains("Verified"));
}

// ---------------------------------------------------------------------------------------------
// C9 and rule R18
// ---------------------------------------------------------------------------------------------

#[test]
fn c9_copy_preserves_unknown_fields_and_signature() {
    // (a) A pointer with a field 40 that is not in `crit`, as a newer minor version writes it.
    let mut p = full();
    p.unknown = UnknownFields::from_entries(vec![(
        40,
        Value::Text("from a newer minor version".to_string()),
    )])
    .expect("keys");
    let p = signed(p);
    let bytes = p.to_cbor().expect("writes").to_vec();
    let Decoded::Current(q) = PointerV2::from_cbor(&bytes, &OBJECT).expect("decodes") else {
        panic!("current");
    };
    assert_eq!(q.unknown.len(), 1);
    assert_eq!(verify_now(&q), Ok(()));
    // Re-encoded byte for byte, also through a copy, and the copy still verifies.
    assert_eq!(q.to_cbor().expect("writes").as_slice(), bytes.as_slice());
    let copy = q.duplicate();
    assert_eq!(copy.to_cbor().expect("writes").as_slice(), bytes.as_slice());
    assert_eq!(verify_now(&copy), Ok(()));
    assert!(copy == *q);

    // (b) A pointer another implementation wrote and signed (the preimage by hand, not by
    // `PointerV2::sign`): this build reads it, keeps field 40 and verifies the signature.
    let mut foreign = full();
    foreign.unknown = UnknownFields::from_entries(vec![
        (
            40,
            Value::Text("a field this build has never heard of".to_string()),
        ),
        (
            50,
            Value::Map(
                azlin_cbor::Map::from_entries(vec![(Value::Uint(1), Value::Bytes(vec![1, 2, 3]))])
                    .expect("a map"),
            ),
        ),
    ])
    .expect("keys");
    let body = reference_encode(&foreign, false);
    let sig = hand_sign(&foreign, &author_key());
    let foreign_bytes = with_entry(&body, 14, Some(Value::Bytes(sig.as_bytes().to_vec())));
    let Decoded::Current(r) = PointerV2::from_cbor(&foreign_bytes, &OBJECT).expect("decodes")
    else {
        panic!("current");
    };
    assert_eq!(r.unknown.len(), 2);
    assert_eq!(verify_now(&r), Ok(()));
    assert_eq!(
        r.to_cbor().expect("writes").as_slice(),
        foreign_bytes.as_slice()
    );

    // (c) Why a copy must keep them: a copy that forgets an unknown field no longer verifies.
    let mut forgetful = r.duplicate();
    forgetful.unknown = UnknownFields::none();
    assert_eq!(verify_now(&forgetful), Err(PointerError::BadSignature));
}

#[test]
fn c9_a_copy_keeps_the_must_understand_list() {
    // Rule R18: a structure that is copied or re-written keeps `crit`; dropping it changes the signed
    // bytes. A copy of a structure with `crit` equals the original bytes.
    let mut p = full();
    p.crit = vec![2, 7];
    let p = signed(p);
    let bytes = p.to_cbor().expect("writes").to_vec();
    assert_eq!(bytes, reference_encode(&p, true));
    let Decoded::Current(q) = PointerV2::from_cbor(&bytes, &OBJECT).expect("decodes") else {
        panic!("current");
    };
    assert_eq!(q.crit, vec![2, 7]);
    assert_eq!(q.to_cbor().expect("writes").as_slice(), bytes.as_slice());
    let copy = q.duplicate();
    assert_eq!(copy.crit, vec![2, 7]);
    assert_eq!(copy.to_cbor().expect("writes").as_slice(), bytes.as_slice());
    assert_eq!(verify_now(&copy), Ok(()));
    // A copy that wrote the structure without `crit` is another structure.
    let mut dropped = q.duplicate();
    dropped.crit = Vec::new();
    assert_ne!(
        dropped.to_cbor().expect("writes").as_slice(),
        bytes.as_slice()
    );
    assert_eq!(verify_now(&dropped), Err(PointerError::BadSignature));
}
