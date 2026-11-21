//! C1: the known answers of the pointer v2 and of the chunk-list object (planning/architecture.md
//! section 5.7 "Pointer v2"), pinned in `tests/vectors/pointer_v2.json` and `tests/vectors/chunk_list.json`.
//!
//! The positive vectors are five pointers, each signed with a fixed test key for a fixed space: the
//! minimal one (an empty file), a small file in one chunk, one with every field (a time before 1970, a
//! transform, previews, streams, attributes, labels, derived records), the same with a field of a newer
//! minor version and a `crit` list, and the pointer of a huge file with a chunk-list object. The bytes of
//! each are also written by the INDEPENDENT reference encoder of `pointer_common` (a mini CBOR emitter
//! written from the documented layout), and the smallest one by HAND below; the signature is checked with
//! the preimage spelled out and azlin-keys' public `verify`, not with the code of the module under test.
//!
//! The negative vectors give bytes and the word of the refusal they get; their words are written here, in
//! the table of the test, never taken from the library's answer. The files are written only under
//! `UPDATE_VECTORS=1`, never when `CI` is set; then every row is read back from the file and checked
//! against the library.

mod pointer_common;

use azlin_cbor::Value;
use azlin_content::pointer::{
    decode_chunk_list, encode_chunk_list, object_name, ChunkListRef, Decoded, ObjectSealer,
    PointerV2,
};
use azlin_keys::sign::{signing_input, verify as keys_verify, POINTER_V2};
use azlin_keys::VerifyAt;
use pointer_common::*;

/// The smallest pointer, derived BY HAND from the layout of the module documentation: a map of eight
/// entries (keys 1, 2, 3, 4, 5, 7, 8 and 15); `v` 2; `size` 0; `mtime` `[1_700_000_000, 0]`
/// (1_700_000_000 = 0x6553F100); `mode` 0o644 = 420 = 0x01A4; `blake3` 32 bytes of 0x10; `chunks` the empty
/// array; `author_device` 16 bytes of 0xA1; `created_at` 1_700_000_100 = 0x6553F164.
const MINIMAL_EMPTY_UNSIGNED: &str = concat!(
    "a8",
    "0102",
    "0200",
    "03821a6553f10000",
    "041901a4",
    "055820",
    "1010101010101010101010101010101010101010101010101010101010101010",
    "0780",
    "0850a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
    "0f1a6553f164",
);

/// `haystack` with the one occurrence of `needle` replaced by `with`.
fn replace_once(haystack: &[u8], needle: &[u8], with: &[u8]) -> Vec<u8> {
    let at: Vec<usize> = (0..=haystack.len() - needle.len())
        .filter(|i| &haystack[*i..*i + needle.len()] == needle)
        .collect();
    assert_eq!(at.len(), 1, "the needle occurs exactly once");
    let mut out = haystack[..at[0]].to_vec();
    out.extend_from_slice(with);
    out.extend_from_slice(&haystack[at[0] + needle.len()..]);
    out
}

/// The one word of what reading `bytes` gives.
fn outcome(bytes: &[u8]) -> String {
    match PointerV2::from_cbor(bytes, &OBJECT) {
        Ok(Decoded::Current(_)) => "current".to_string(),
        Ok(Decoded::Newer(v)) => format!("newer_{v}"),
        Err(e) => format!("{e:?}"),
    }
}

/// The signature checked without the module under test: the preimage spelled out, and azlin-keys'
/// public `verify` over `space || body`.
fn check_signature_independently(p: &PointerV2) {
    let mut message = space().as_bytes().to_vec();
    message.extend_from_slice(&reference_encode(p, false));
    assert_eq!(
        signing_input(&POINTER_V2, &message),
        reference_preimage(&space(), p),
        "the preimage is domain, NUL, space id, body"
    );
    let sig = p.sig.expect("a signed pointer");
    keys_verify(&POINTER_V2, &message, &author_key().verify_key(), &sig)
        .expect("an independent verifier accepts the signature");
}

#[test]
fn c1_the_smallest_pointer_by_hand() {
    // The bytes written by hand are what the library writes and the reference encoder writes.
    let expected = from_hex(MINIMAL_EMPTY_UNSIGNED);
    let p = minimal_empty();
    assert_eq!(
        reference_encode(&p, false),
        expected,
        "the reference encoder"
    );
    assert_eq!(p.to_cbor().expect("writes").as_slice(), expected.as_slice());
    // ... and reading them gives the pointer back.
    match PointerV2::from_cbor(&expected, &OBJECT).expect("reads") {
        Decoded::Current(q) => assert_eq!(*q, p),
        Decoded::Newer(v) => panic!("newer {v}"),
    }
    // Signed: key 14 joins the map (nine entries), and nothing else moves.
    let signed = signed(minimal_empty());
    let with_sig = signed.to_cbor().expect("writes");
    assert_eq!(with_sig[0], 0xa9, "nine entries now");
    assert_eq!(
        with_sig.len(),
        expected.len() + 3 + 64,
        "key 14, a byte string head of two bytes, 64 bytes"
    );
    check_signature_independently(&signed);
}

#[test]
fn c1_pointer_golden_vectors() {
    let sealer = TestSealer::new();
    let (huge_pointer, _, _) = huge(&sealer);
    let positives: Vec<(&str, PointerV2)> = vec![
        ("minimal_empty", minimal_empty()),
        ("minimal_one_chunk", minimal_one_chunk()),
        ("full", full()),
        ("full_with_unknown_and_crit", full_with_unknown()),
        ("huge_object", huge_pointer),
    ];

    let mut rows: Vec<String> = Vec::new();
    let mut bytes_of_case = std::collections::BTreeMap::new();
    for (name, p) in positives {
        let p = signed(p);
        let bytes = p.to_cbor().expect("writes").to_vec();
        assert_eq!(
            bytes,
            reference_encode(&p, true),
            "{name}: the library writes what the documented layout says"
        );
        // Signing again gives the same bytes: the signature is deterministic.
        let mut again = p.duplicate();
        again.sign(&space(), &author_key()).expect("signs again");
        assert_eq!(
            again.to_cbor().expect("writes").as_slice(),
            bytes.as_slice()
        );
        // Reading gives the pointer back, and it writes back to the same bytes and verifies.
        match PointerV2::from_cbor(&bytes, &OBJECT).expect("reads") {
            Decoded::Current(q) => {
                assert_eq!(*q, p, "{name}");
                assert_eq!(q.to_cbor().expect("writes").as_slice(), bytes.as_slice());
                q.verify(&space(), &directory(), VerifyAt::Now)
                    .expect("verifies");
            }
            Decoded::Newer(v) => panic!("{name}: newer {v}"),
        }
        check_signature_independently(&p);
        let sig = p.sig.expect("signed");
        rows.push(format!(
            "{{\"name\": \"{name}\", \"valid\": true, \"expect\": \"current\", \"size\": {}, \"chunks\": \"{:?}\", \"sig\": \"{}\", \"hex\": \"{}\"}}",
            p.size.get(),
            p.chunks,
            to_hex(sig.as_bytes()),
            to_hex(&bytes)
        ));
        bytes_of_case.insert(name, bytes);
    }

    // The negative vectors: (name, bytes, the word of the refusal), and two newer pointers.
    let full_bytes = bytes_of_case["full"].clone();
    let unknown_bytes = bytes_of_case["full_with_unknown_and_crit"].clone();
    let one_chunk_bytes = bytes_of_case["minimal_one_chunk"].clone();
    let minimal = from_hex(MINIMAL_EMPTY_UNSIGNED);
    let table: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "truncated_half",
            full_bytes[..full_bytes.len() / 2].to_vec(),
            "Cbor(Truncated)",
        ),
        ("truncated_empty_input", Vec::new(), "Cbor(Truncated)"),
        (
            "trailing_byte",
            {
                let mut longer = full_bytes.clone();
                longer.push(0);
                longer
            },
            "Cbor(TrailingBytes)",
        ),
        (
            "version_1",
            with_entry(&full_bytes, 1, Some(Value::Uint(1))),
            "UnsupportedVersion(1)",
        ),
        (
            "version_0",
            with_entry(&full_bytes, 1, Some(Value::Uint(0))),
            "UnsupportedVersion(0)",
        ),
        (
            "version_3_is_newer",
            with_entry(&full_bytes, 1, Some(Value::Uint(3))),
            "newer_3",
        ),
        (
            "version_255_is_newer_whatever_follows",
            // {1: 255, 2: "xxx"}
            from_hex("a20118ff0263787878"),
            "newer_255",
        ),
        (
            "version_256_is_damage",
            with_entry(&full_bytes, 1, Some(Value::Uint(256))),
            "BadVersion",
        ),
        (
            "wrong_type_size",
            with_entry(&full_bytes, 2, Some(Value::Text("x".to_string()))),
            "Shape(WrongType(2))",
        ),
        (
            "missing_blake3",
            with_entry(&full_bytes, 5, None),
            "Shape(Missing(5))",
        ),
        (
            "wrong_length_blake3",
            with_entry(&full_bytes, 5, Some(Value::Bytes(vec![0; 31]))),
            "Shape(WrongLength(5))",
        ),
        (
            "crit_names_a_key_this_build_does_not_know",
            with_entry(
                &unknown_bytes,
                0,
                Some(Value::Array(vec![Value::Uint(7), Value::Uint(40)])),
            ),
            "Shape(CritUnknown(40))",
        ),
        (
            "duplicate_map_key",
            from_hex("a201020102"),
            "Cbor(DuplicateMapKey)",
        ),
        (
            "unsorted_map_keys",
            from_hex("a202000102"),
            "Cbor(UnsortedMapKeys)",
        ),
        (
            "non_shortest_size",
            replace_once(&minimal, &[0x02, 0x00, 0x03], &[0x02, 0x18, 0x00, 0x03]),
            "Cbor(NotShortest)",
        ),
        (
            "float_for_size",
            replace_once(
                &minimal,
                &[0x02, 0x00, 0x03],
                &[0x02, 0xf9, 0x00, 0x00, 0x03],
            ),
            "Cbor(DisallowedType)",
        ),
        (
            "empty_previews_array_is_not_canonical",
            with_entry(&full_bytes, 9, Some(Value::Array(Vec::new()))),
            "NotCanonical",
        ),
        (
            "mode_above_twelve_bits",
            with_entry(&full_bytes, 4, Some(Value::Uint(4096))),
            "BadMode",
        ),
        (
            "sizes_do_not_add_up",
            with_entry(&one_chunk_bytes, 2, Some(Value::Uint(1001))),
            "SizeMismatch",
        ),
    ];
    for (name, bytes, expect) in &table {
        assert_eq!(&outcome(bytes), expect, "negative vector {name}");
        let valid = expect.starts_with("newer_");
        rows.push(format!(
            "{{\"name\": \"{name}\", \"valid\": {valid}, \"expect\": \"{expect}\", \"hex\": \"{}\"}}",
            to_hex(bytes)
        ));
    }
    assert!(
        table
            .iter()
            .filter(|(_, _, e)| !e.starts_with("newer_"))
            .count()
            >= 3,
        "at least three negative vectors"
    );

    check_vector_file("pointer_v2", &render_rows(&rows));

    // The rows as the file has them: every one is read by the library, the positive ones verify.
    let from_file = read_rows("pointer_v2");
    assert_eq!(from_file.len(), rows.len());
    for row in &from_file {
        let name = field(row, "name");
        let bytes = from_hex(&field(row, "hex"));
        assert_eq!(outcome(&bytes), field(row, "expect"), "{name}");
        if field(row, "expect") == "current" {
            let Decoded::Current(p) = PointerV2::from_cbor(&bytes, &OBJECT).expect("reads") else {
                panic!("{name}: not current");
            };
            p.verify(&space(), &directory(), VerifyAt::Now)
                .expect("the pinned pointer verifies");
            assert_eq!(to_hex(p.sig.expect("signed").as_bytes()), field(row, "sig"));
            assert_eq!(p.to_cbor().expect("writes").as_slice(), bytes.as_slice());
        }
    }
}

#[test]
fn c1_chunk_list_golden_vectors() {
    let sealer = TestSealer::new();
    let refs = huge_chunks(1025);
    let (sealed, list) = encode_chunk_list(&refs, &space(), &sealer).expect("encodes");

    // The context the object is sealed under: the purpose string and the space id.
    let context = [
        b"azlin 2026-11 chunk list seal v1".as_slice(),
        space().as_bytes().as_slice(),
    ]
    .concat();
    assert_eq!(sealer.contexts.borrow().len(), 1);
    assert_eq!(sealer.contexts.borrow()[0], context);

    // The plaintext is the object of the documented layout: {1: 1, 2: [chunk references]}.
    let reference = cbor::map(&[
        (1, cbor::uint(1)),
        (
            2,
            cbor::array(&refs.iter().map(ref_bytes).collect::<Vec<_>>()),
        ),
    ]);
    let plaintext = sealer.open(&context, &sealed).expect("opens");
    assert_eq!(plaintext.as_slice(), reference.as_slice());
    assert_eq!(
        sealed.len(),
        plaintext.len() + 40,
        "the double adds its magic and tag"
    );
    assert_eq!(
        list.name,
        *blake3::hash(&sealed).as_bytes(),
        "the name is the BLAKE3 of the sealed bytes"
    );
    assert_eq!(list.name, object_name(&sealed));
    assert_eq!(list.count, 1025);
    assert_eq!(list.total_raw.get(), total_raw(&refs));

    let mut rows: Vec<String> = vec![format!(
        "{{\"name\": \"huge_1025\", \"valid\": true, \"expect\": \"current\", \"count\": {}, \"total_raw\": {}, \"plaintext_len\": {}, \"plaintext_blake3\": \"{}\", \"sealed_len\": {}, \"object_name\": \"{}\", \"sealer\": \"TSEAL001, tag keyed BLAKE3 key 5e*32\", \"hex\": \"{}\"}}",
        list.count,
        list.total_raw.get(),
        plaintext.len(),
        to_hex(blake3::hash(&plaintext).as_bytes()),
        sealed.len(),
        to_hex(&list.name),
        to_hex(&plaintext[..96]),
    )];

    // Negative vectors: short plaintexts a hostile server could have sealed (it holds no key, but the
    // pointer's name binds the object and the checks must hold even for an authentic one), with the
    // refusal each gets under a declared count of 1,025.
    let negatives: Vec<(&str, Vec<u8>, &str)> = vec![
        ("newer_list_version", from_hex("a201020280"), "NewerList(2)"),
        (
            "list_version_0",
            from_hex("a201000280"),
            "UnsupportedList(0)",
        ),
        (
            "list_version_missing",
            from_hex("a10280"),
            "Shape(Missing(1))",
        ),
        (
            "list_array_missing",
            from_hex("a10101"),
            "Shape(Missing(2))",
        ),
        (
            "list_array_wrong_type",
            from_hex("a20101026141"),
            "Shape(WrongType(2))",
        ),
        ("truncated_list", from_hex("a201010299"), "Cbor(Truncated)"),
        ("too_few_chunks", from_hex("a201010280"), "ListCount"),
    ];
    for (name, plain, expect) in &negatives {
        let sealed = sealer.seal(&context, plain).expect("seals");
        let declared = ChunkListRef {
            name: object_name(&sealed),
            count: 1025,
            total_raw: azlin_content::FileSize::new(1025 * 1000),
        };
        let got = match decode_chunk_list(&sealed, &declared, &space(), &sealer) {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("{e:?}"),
        };
        assert_eq!(&got, expect, "negative vector {name}");
        rows.push(format!(
            "{{\"name\": \"{name}\", \"valid\": false, \"expect\": \"{expect}\", \"count\": 1025, \"hex\": \"{}\"}}",
            to_hex(plain)
        ));
    }
    assert!(negatives.len() >= 3, "at least three negative vectors");

    check_vector_file("chunk_list", &render_rows(&rows));

    // The positive row as the file has it: the object reads back to the chunks it was made from.
    let from_file = read_rows("chunk_list");
    assert_eq!(from_file.len(), rows.len());
    let decoded = decode_chunk_list(&sealed, &list, &space(), &sealer).expect("decodes");
    assert_eq!(decoded, refs);
    for row in &from_file {
        assert!(!field(row, "name").is_empty());
        if flag(row, "valid") {
            assert_eq!(field(row, "object_name"), to_hex(&list.name));
            assert_eq!(
                field(row, "plaintext_blake3"),
                to_hex(blake3::hash(&plaintext).as_bytes())
            );
        }
    }
}
