//! C7: the sealed chunk-list object of a huge file. It round-trips through the sealer seam, its name is
//! the BLAKE3 of its sealed bytes, a spliced object (another list, another space, one bit flipped) is
//! refused, the declared count and total are checked against the decoded list, and a list that claims more
//! than the cap is refused before anything is opened or allocated.

mod pointer_common;

use azlin_content::limits::MAX_FILE_CHUNKS;
use azlin_content::pointer::{
    decode_chunk_list, encode_chunk_list, object_name, ChunkList, ChunkListRef, ObjectSealer,
    PointerError, SealError, MAX_SEAL_OVERHEAD,
};
use azlin_content::{ChunkError, ChunkRef, FileSize};
use azlin_keys::VerifyAt;
use pointer_common::*;
use zeroize::Zeroizing;

/// The context the object is sealed under, spelled out.
fn context() -> Vec<u8> {
    [
        b"azlin 2026-11 chunk list seal v1".as_slice(),
        space().as_bytes().as_slice(),
    ]
    .concat()
}

fn made(n: u32) -> (Vec<u8>, ChunkListRef, Vec<ChunkRef>, TestSealer) {
    let sealer = TestSealer::new();
    let refs = huge_chunks(n);
    let (sealed, list) = encode_chunk_list(&refs, &space(), &sealer).expect("encodes");
    (sealed, list, refs, sealer)
}

/// Seals a plaintext of the test's choosing under the real context, and the reference that names it with
/// a declared total that is possible.
fn sealed_plaintext(sealer: &TestSealer, plain: &[u8]) -> (Vec<u8>, ChunkListRef) {
    let sealed = sealer.seal(&context(), plain).expect("seals");
    let list = ChunkListRef {
        name: object_name(&sealed),
        count: 1025,
        total_raw: FileSize::new(total_raw(&huge_chunks(1025))),
    };
    (sealed, list)
}

fn entries_bytes() -> Vec<Vec<u8>> {
    huge_chunks(1025).iter().map(ref_bytes).collect()
}

fn object_bytes(entries: &[(u64, Vec<u8>)]) -> Vec<u8> {
    cbor::map(entries)
}

#[test]
fn c7_chunk_list_object_seals_and_binds() {
    let (sealed, list, refs, sealer) = made(1025);

    // The round trip through the sealer double.
    let decoded = decode_chunk_list(&sealed, &list, &space(), &sealer).expect("decodes");
    assert_eq!(decoded, refs);
    assert_eq!(decoded.len(), 1025);

    // The name is the BLAKE3 of the sealed bytes, the count and the total are the list's.
    assert_eq!(list.name, *blake3::hash(&sealed).as_bytes());
    assert_eq!(list.count, 1025);
    assert_eq!(list.total_raw.get(), total_raw(&refs));
    // The sealer was given the purpose string and the space as its context.
    assert_eq!(sealer.contexts.borrow().len(), 1);
    assert_eq!(sealer.contexts.borrow()[0], context());

    // Another LIST: an object of the same size with other chunks does not pass for this one, in either
    // direction (the name is the object's own hash).
    let other_refs: Vec<ChunkRef> = (5000..6025).map(chunk_i).collect();
    let (other_sealed, other_list) =
        encode_chunk_list(&other_refs, &space(), &sealer).expect("encodes");
    assert_ne!(other_list.name, list.name);
    assert_eq!(
        decode_chunk_list(&other_sealed, &list, &space(), &sealer).err(),
        Some(PointerError::ListName)
    );
    assert_eq!(
        decode_chunk_list(&sealed, &other_list, &space(), &sealer).err(),
        Some(PointerError::ListName)
    );

    // Another SPACE: the name still matches, the sealer refuses (the context carries the space id).
    assert_eq!(
        decode_chunk_list(&sealed, &list, &other_space(), &sealer).err(),
        Some(PointerError::Seal(SealError::Auth))
    );
    // An object sealed for another space is not the object of this one even if it is the same list.
    let (elsewhere, elsewhere_list) =
        encode_chunk_list(&refs, &other_space(), &sealer).expect("encodes");
    assert_ne!(
        elsewhere_list.name, list.name,
        "another context, other sealed bytes"
    );
    assert_eq!(
        decode_chunk_list(&elsewhere, &elsewhere_list, &space(), &sealer).err(),
        Some(PointerError::Seal(SealError::Auth))
    );

    // One bit flipped, anywhere: the name no longer matches. An attacker who also fixes the name still
    // meets the sealer's own check.
    let mut rng = SplitMix64::new(7);
    for _ in 0..40 {
        let mut flipped = sealed.clone();
        let at = rng.below(flipped.len() as u64) as usize;
        flipped[at] ^= 1 << rng.below(8);
        assert_eq!(
            decode_chunk_list(&flipped, &list, &space(), &sealer).err(),
            Some(PointerError::ListName),
            "bit flip at byte {at}"
        );
        let renamed = ChunkListRef {
            name: object_name(&flipped),
            ..list
        };
        let err = decode_chunk_list(&flipped, &renamed, &space(), &sealer).err();
        assert!(
            matches!(err, Some(PointerError::Seal(SealError::Auth))),
            "a flipped bit with a fixed name at byte {at}: {err:?}"
        );
    }

    // The declared count and the declared total are checked against the decoded list.
    let wrong_count = ChunkListRef {
        count: 1026,
        ..list
    };
    assert_eq!(
        decode_chunk_list(&sealed, &wrong_count, &space(), &sealer).err(),
        Some(PointerError::ListCount)
    );
    let wrong_count = ChunkListRef {
        count: 1024,
        ..list
    };
    assert_eq!(
        decode_chunk_list(&sealed, &wrong_count, &space(), &sealer).err(),
        Some(PointerError::ObjectTooSmall)
    );
    let wrong_total = ChunkListRef {
        total_raw: FileSize::new(list.total_raw.get() + 1),
        ..list
    };
    assert_eq!(
        decode_chunk_list(&sealed, &wrong_total, &space(), &sealer).err(),
        Some(PointerError::ListTotal)
    );
    let wrong_total = ChunkListRef {
        total_raw: FileSize::new(list.total_raw.get() - 1),
        ..list
    };
    assert_eq!(
        decode_chunk_list(&sealed, &wrong_total, &space(), &sealer).err(),
        Some(PointerError::ListTotal)
    );
    // An object with one entry more than declared: refused too (the room is one entry, so it is the count).
    let (longer, longer_list) =
        encode_chunk_list(&huge_chunks(1026), &space(), &sealer).expect("encodes");
    let declared = ChunkListRef {
        count: 1025,
        ..longer_list
    };
    assert_eq!(
        decode_chunk_list(&longer, &declared, &space(), &sealer).err(),
        Some(PointerError::ListCount)
    );
    // ... and with two more it is refused by the codec's item limit.
    let (longest, longest_list) =
        encode_chunk_list(&huge_chunks(1027), &space(), &sealer).expect("encodes");
    let declared = ChunkListRef {
        count: 1025,
        ..longest_list
    };
    assert!(decode_chunk_list(&longest, &declared, &space(), &sealer).is_err());
}

#[test]
fn c7_a_list_that_claims_more_than_the_cap_is_refused_before_anything_is_opened() {
    let (sealed, list, _, sealer) = made(1025);
    let opens_before = sealer.opens.get();
    let max = MAX_FILE_CHUNKS;
    // The declared count is checked first: nothing is hashed, opened or allocated for such a list.
    for (count, expected) in [
        (max + 1, PointerError::TooManyChunks),
        (u32::MAX, PointerError::TooManyChunks),
        (1024, PointerError::ObjectTooSmall),
        (0, PointerError::ObjectTooSmall),
    ] {
        let r = ChunkListRef { count, ..list };
        assert_eq!(
            decode_chunk_list(&sealed, &r, &space(), &sealer).err(),
            Some(expected),
            "count {count}"
        );
    }
    // A total that cannot belong to that many chunks, or is above 2^50, is refused before the sealer is asked.
    for total in [
        0u64,
        1024,
        1025 * 4 * 1024 * 1024 + 1,
        (1 << 50) + 1,
        u64::MAX,
    ] {
        let r = ChunkListRef {
            total_raw: FileSize::new(total),
            ..list
        };
        assert!(
            decode_chunk_list(&sealed, &r, &space(), &sealer).is_err(),
            "total {total}"
        );
    }
    // Sealed bytes longer than a list of that many chunks can be are refused before they are hashed.
    let too_long = vec![0u8; 16 + 67 * 1026 + MAX_SEAL_OVERHEAD + 1];
    assert_eq!(
        decode_chunk_list(&too_long, &list, &space(), &sealer).err(),
        Some(PointerError::ListTooLong)
    );
    // Even a claim of the largest list costs nothing: the check is on the numbers, then on the name.
    let r = ChunkListRef {
        name: [0xAB; 32],
        count: max,
        total_raw: FileSize::new(u64::from(max) * 4 * 1024 * 1024),
    };
    assert_eq!(
        decode_chunk_list(&sealed, &r, &space(), &sealer).err(),
        Some(PointerError::ListName),
        "at the cap the count is fine; the object is not the one named"
    );
    assert_eq!(
        sealer.opens.get(),
        opens_before,
        "the sealer was not asked for any of these"
    );
}

#[test]
fn c7_what_the_object_may_hold() {
    let sealer = TestSealer::new();
    let refs = huge_chunks(1025);
    let array = cbor::array(&entries_bytes());
    let decode = |plain: &[u8]| {
        let (sealed, list) = sealed_plaintext(&sealer, plain);
        decode_chunk_list(&sealed, &list, &space(), &sealer).map(|v| v.len())
    };

    // The object of the documented layout.
    assert_eq!(
        decode(&object_bytes(&[(1, cbor::uint(1)), (2, array.clone())])),
        Ok(1025)
    );
    // A newer object is not damage; version 0 and a version that does not fit a byte are.
    assert_eq!(
        decode(&object_bytes(&[(1, cbor::uint(2)), (2, array.clone())])),
        Err(PointerError::NewerList(2))
    );
    assert_eq!(
        decode(&object_bytes(&[
            (1, cbor::uint(255)),
            (2, cbor::text("whatever"))
        ])),
        Err(PointerError::NewerList(255))
    );
    assert_eq!(
        decode(&object_bytes(&[(1, cbor::uint(0)), (2, array.clone())])),
        Err(PointerError::UnsupportedList(0))
    );
    assert_eq!(
        decode(&object_bytes(&[(1, cbor::uint(256)), (2, array.clone())])),
        Err(PointerError::BadVersion)
    );
    // A field this build does not know is ignored, unless `crit` lists it.
    assert_eq!(
        decode(&object_bytes(&[
            (1, cbor::uint(1)),
            (2, array.clone()),
            (9, cbor::text("x"))
        ])),
        Ok(1025)
    );
    assert_eq!(
        decode(&object_bytes(&[
            (0, cbor::array(&[cbor::uint(9)])),
            (1, cbor::uint(1)),
            (2, array.clone()),
            (9, cbor::text("x")),
        ])),
        Err(PointerError::Shape(azlin_cbor::ShapeError::CritUnknown(9)))
    );
    // Missing keys, a wrong type, bytes after the object, a non-canonical integer.
    assert_eq!(
        decode(&object_bytes(&[(2, array.clone())])),
        Err(PointerError::Shape(azlin_cbor::ShapeError::Missing(1)))
    );
    assert_eq!(
        decode(&object_bytes(&[(1, cbor::uint(1))])),
        Err(PointerError::Shape(azlin_cbor::ShapeError::Missing(2)))
    );
    assert_eq!(
        decode(&object_bytes(&[(1, cbor::uint(1)), (2, cbor::text("x"))])),
        Err(PointerError::Shape(azlin_cbor::ShapeError::WrongType(2)))
    );
    let mut trailing = object_bytes(&[(1, cbor::uint(1)), (2, array.clone())]);
    trailing.push(0);
    assert_eq!(
        decode(&trailing),
        Err(PointerError::Cbor(azlin_cbor::DecodeError::TrailingBytes))
    );
    // {1: 1 written as 0x1801, 2: [entries]}
    let mut long_form = vec![0xa2, 0x01, 0x18, 0x01, 0x02];
    long_form.extend_from_slice(&array);
    assert_eq!(
        decode(&long_form),
        Err(PointerError::Cbor(azlin_cbor::DecodeError::NotShortest))
    );
    // An entry of the wrong shape, an entry that cannot be a chunk.
    let mut entries = entries_bytes();
    entries[10] = cbor::uint(5);
    assert_eq!(
        decode(&object_bytes(&[
            (1, cbor::uint(1)),
            (2, cbor::array(&entries))
        ])),
        Err(PointerError::BadShape(
            azlin_content::pointer::Part::ChunkRef
        ))
    );
    let mut entries = entries_bytes();
    entries[10] = cbor::array(&[
        cbor::bytes(&[1; 32]),
        cbor::uint(1),
        cbor::bytes(&[2; 16]),
        cbor::uint(0),
    ]);
    assert_eq!(
        decode(&object_bytes(&[
            (1, cbor::uint(1)),
            (2, cbor::array(&entries))
        ])),
        Err(PointerError::BadShape(
            azlin_content::pointer::Part::ChunkRef
        ))
    );
    let mut entries = entries_bytes();
    entries[10] = cbor::array(&[
        cbor::bytes(&[1; 32]),
        cbor::uint(0),
        cbor::bytes(&[2; 16]),
        cbor::uint(0),
        cbor::uint(100),
    ]);
    assert_eq!(
        decode(&object_bytes(&[
            (1, cbor::uint(1)),
            (2, cbor::array(&entries))
        ])),
        Err(PointerError::BadChunk(ChunkError::EmptyChunk))
    );
    // An entry changed to a different length: the entries no longer add up to the declared total.
    let mut changed = refs;
    changed[3].raw_len =
        azlin_content::ChunkLen::new(changed[3].raw_len.get() + 1).expect("a length");
    changed[3].stored_len =
        azlin_content::ChunkLen::new(changed[3].stored_len.get() + 1).expect("a length");
    let entries: Vec<Vec<u8>> = changed.iter().map(ref_bytes).collect();
    assert_eq!(
        decode(&object_bytes(&[
            (1, cbor::uint(1)),
            (2, cbor::array(&entries))
        ])),
        Err(PointerError::ListTotal)
    );
}

/// A sealer that adds far more than `MAX_SEAL_OVERHEAD` bytes (10,000: more than the slack a list of that
/// many chunks has under its bound).
struct FatSealer;

impl ObjectSealer for FatSealer {
    fn seal(&self, _context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        let mut out = plaintext.to_vec();
        out.resize(plaintext.len() + 10_000, 0);
        Ok(out)
    }

    fn open(&self, _context: &[u8], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        Ok(Zeroizing::new(sealed.to_vec()))
    }
}

#[test]
fn c7_the_writer_refuses_what_the_reader_would_refuse() {
    let sealer = TestSealer::new();
    // Too few chunks for an object, a chunk that cannot exist, a sealer that fails, a sealer that adds
    // more than it may.
    assert_eq!(
        encode_chunk_list(&huge_chunks(1024), &space(), &sealer).err(),
        Some(PointerError::ObjectTooSmall)
    );
    assert_eq!(
        encode_chunk_list(&[], &space(), &sealer).err(),
        Some(PointerError::ObjectTooSmall)
    );
    let mut refs = huge_chunks(1025);
    refs[500].stored_len =
        azlin_content::ChunkLen::new(refs[500].raw_len.get() + 42).expect("a length");
    assert_eq!(
        encode_chunk_list(&refs, &space(), &sealer).err(),
        Some(PointerError::BadChunk(ChunkError::LengthMismatch))
    );
    for error in [SealError::Auth, SealError::TooLong, SealError::Failed] {
        assert_eq!(
            encode_chunk_list(&huge_chunks(1025), &space(), &RefusingSealer(error)).err(),
            Some(PointerError::Seal(error))
        );
    }
    assert_eq!(
        encode_chunk_list(&huge_chunks(1025), &space(), &FatSealer).err(),
        Some(PointerError::ListTooLong)
    );
    // The reader passes a sealer's refusal on.
    let (sealed, list, _, _) = made(1025);
    for error in [SealError::Auth, SealError::TooLong, SealError::Failed] {
        assert_eq!(
            decode_chunk_list(&sealed, &list, &space(), &RefusingSealer(error)).err(),
            Some(PointerError::Seal(error))
        );
    }
}

#[test]
fn c7_a_signed_pointer_names_its_object_and_cannot_be_given_another() {
    // The pointer carries the object's name and is signed: the list a signed pointer names cannot be
    // swapped for another without breaking the signature.
    let sealer = TestSealer::new();
    let (p, sealed, refs) = huge(&sealer);
    let p = signed(p);
    p.verify(&space(), &directory(), VerifyAt::Now)
        .expect("verifies");
    let ChunkList::Object(list) = &p.chunks else {
        panic!("an object");
    };
    assert_eq!(
        decode_chunk_list(&sealed, list, &space(), &sealer).expect("decodes"),
        refs
    );

    let other_refs: Vec<ChunkRef> = (9000..10025).map(chunk_i).collect();
    let (_, other_list) = encode_chunk_list(&other_refs, &space(), &sealer).expect("encodes");
    let mut swapped = p.duplicate();
    swapped.chunks = ChunkList::Object(other_list);
    swapped.size = other_list.total_raw;
    assert_eq!(
        swapped.verify(&space(), &directory(), VerifyAt::Now).err(),
        Some(PointerError::BadSignature)
    );
    // The same list under another name is another pointer too.
    let mut renamed = p.duplicate();
    renamed.chunks = ChunkList::Object(ChunkListRef {
        name: [0xAB; 32],
        ..*list
    });
    assert_eq!(
        renamed.verify(&space(), &directory(), VerifyAt::Now).err(),
        Some(PointerError::BadSignature)
    );
}
