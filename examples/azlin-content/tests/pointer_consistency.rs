//! C5, C6 (the canonical choice between an inline list and a chunk-list object), C8 and C11: the rules a
//! valid pointer satisfies beyond its shape. Every rule is checked from BOTH sides: what `to_cbor` refuses
//! to write and what `from_cbor` refuses to read (the bytes of the second come from the independent
//! reference encoder, which writes anything), and the two always agree.

mod pointer_common;

use std::cell::Cell;

use azlin_cbor::Value;
use azlin_content::pointer::{
    encode_chunk_list, ChunkList, ChunkListRef, Decoded, DerivedRef, Limit, Part, PointerError,
    PointerV2, Preview, StreamRef, TransformRef,
};
use azlin_content::{
    identity, open_chunk, seal_stored, ChunkError, ChunkLen, ChunkRef, Codec, CodecError,
    Compressed, Compressor, FileSize, KChunk, OpenAs, PackOffset,
};
use azlin_keys::Secret;
use pointer_common::*;
use zeroize::Zeroizing;

fn tiny(i: usize) -> ChunkRef {
    chunk((i % 250) as u8 + 1, 100, 141)
}

fn refs(n: usize) -> Vec<ChunkRef> {
    (0..n).map(tiny).collect()
}

fn previews(n: usize) -> Vec<Preview> {
    (0..n)
        .map(|i| Preview {
            kind: 1,
            w: 1,
            h: 1,
            chunks: vec![tiny(i)],
        })
        .collect()
}

fn streams(n: usize) -> Vec<StreamRef> {
    (0..n)
        .map(|i| StreamRef {
            kind: 1,
            w: 0,
            h: 0,
            bandwidth: 1,
            chunks: vec![tiny(i)],
        })
        .collect()
}

fn derived(n: usize) -> Vec<DerivedRef> {
    (0..n)
        .map(|i| DerivedRef {
            extractor: format!("e{i}"),
            model_id: String::new(),
            model_version: String::new(),
            record: tiny(i),
        })
        .collect()
}

fn with_transform(p: &mut PointerV2, params: Value) {
    p.transform = Some(TransformRef {
        id: 1,
        out_len: FileSize::new(1000),
        params,
    });
}

// ---------------------------------------------------------------------------------------------
// C5
// ---------------------------------------------------------------------------------------------

#[test]
fn c5_consistency_rules() {
    // The lengths of the chunks add up to the size when there is no transform.
    let mut p = minimal_one_chunk();
    assert_eq!(p.check_consistency(), Ok(()));
    p.size = FileSize::new(1001);
    assert_eq!(p.check_consistency(), Err(PointerError::SizeMismatch));
    p.size = FileSize::new(999);
    assert_eq!(p.check_consistency(), Err(PointerError::SizeMismatch));

    // With a transform they add up to the transformed length, whatever the size is.
    let mut t = minimal_one_chunk();
    t.size = FileSize::new(5000);
    with_transform(&mut t, Value::Null);
    assert_eq!(t.check_consistency(), Ok(()));
    assert_eq!(both(&t), (None, None));
    t.transform.as_mut().expect("a transform").out_len = FileSize::new(1001);
    assert_eq!(t.check_consistency(), Err(PointerError::SizeMismatch));
    // ... and a size that equals the chunks' sum is no help when the transform says otherwise.
    t.size = FileSize::new(1000);
    assert_eq!(t.check_consistency(), Err(PointerError::SizeMismatch));

    // An empty file has an empty chunk list; a list for no bytes, or no list for some bytes, is wrong.
    let empty = minimal_empty();
    assert_eq!(empty.check_consistency(), Ok(()));
    assert_eq!(both(&empty), (None, None));
    let mut one_for_nothing = minimal_empty();
    one_for_nothing.chunks = ChunkList::Inline(vec![chunk(0x21, 100, 141)]);
    assert_eq!(
        one_for_nothing.check_consistency(),
        Err(PointerError::SizeMismatch)
    );
    let mut nothing_for_some = minimal_empty();
    nothing_for_some.size = FileSize::new(5);
    assert_eq!(
        nothing_for_some.check_consistency(),
        Err(PointerError::SizeMismatch)
    );

    // The same chunk twice is legal (repeated blocks), and the sum counts it twice.
    let mut repeated = minimal_empty();
    repeated.size = FileSize::new(200);
    repeated.chunks = ChunkList::Inline(vec![chunk(0x21, 100, 141), chunk(0x21, 100, 141)]);
    assert_eq!(repeated.check_consistency(), Ok(()));
    assert_eq!(both(&repeated), (None, None));
    let bytes = repeated.to_cbor().expect("writes");
    let Decoded::Current(back) = PointerV2::from_cbor(&bytes, &OBJECT).expect("reads") else {
        panic!("current");
    };
    assert!(*back == repeated);
}

#[test]
fn c5_a_chunk_reference_must_be_able_to_exist() {
    let overflowing = |offset: u32, stored: u32, raw: u32| {
        let mut r = chunk(0x21, raw, stored);
        r.offset = PackOffset::new(offset);
        r
    };
    // offset + stored_len past 2^32 - 1 is an error, at the edge it is not.
    let mut p = minimal_empty();
    p.size = FileSize::new(100);
    p.chunks = ChunkList::Inline(vec![overflowing(u32::MAX - 10, 141, 100)]);
    assert_eq!(
        p.check_consistency(),
        Err(PointerError::BadChunk(ChunkError::TooLong))
    );
    assert_eq!(
        both(&p),
        (
            Some(PointerError::BadChunk(ChunkError::TooLong)),
            Some(PointerError::BadChunk(ChunkError::TooLong))
        )
    );
    p.chunks = ChunkList::Inline(vec![overflowing(u32::MAX - 141, 141, 100)]);
    assert_eq!(p.check_consistency(), Ok(()));
    p.chunks = ChunkList::Inline(vec![overflowing(u32::MAX - 140, 141, 100)]);
    assert!(p.check_consistency().is_err());

    // A stored length longer than the plaintext plus the blob's framing cannot be a chunk; one that is too
    // short for the framing cannot either (a zero length does not even exist as a `ChunkLen`).
    assert!(ChunkLen::new(0).is_none());
    let mut longer = minimal_empty();
    longer.size = FileSize::new(100);
    longer.chunks = ChunkList::Inline(vec![chunk(0x21, 100, 142)]);
    assert_eq!(
        longer.check_consistency(),
        Err(PointerError::BadChunk(ChunkError::LengthMismatch))
    );
    let mut shorter = minimal_empty();
    shorter.size = FileSize::new(100);
    shorter.chunks = ChunkList::Inline(vec![chunk(0x21, 100, 41)]);
    assert_eq!(
        shorter.check_consistency(),
        Err(PointerError::BadChunk(ChunkError::Truncated))
    );
    let mut too_big = minimal_empty();
    too_big.size = FileSize::new(4 * 1024 * 1024 + 1);
    too_big.chunks = ChunkList::Inline(vec![chunk(0x21, 4 * 1024 * 1024 + 1, 4 * 1024 * 1024 + 1)]);
    assert_eq!(
        too_big.check_consistency(),
        Err(PointerError::BadChunk(ChunkError::TooLong))
    );

    // The references of the previews, the streams and the derived records are checked like the chunks.
    let bad = || chunk(0x22, 100, 142);
    let mut a = minimal_one_chunk();
    a.previews = vec![Preview {
        kind: 1,
        w: 1,
        h: 1,
        chunks: vec![bad()],
    }];
    let mut b = minimal_one_chunk();
    b.streams = vec![StreamRef {
        kind: 1,
        w: 1,
        h: 1,
        bandwidth: 1,
        chunks: vec![bad()],
    }];
    let mut c = minimal_one_chunk();
    c.derived = vec![DerivedRef {
        extractor: "e".to_string(),
        model_id: String::new(),
        model_version: String::new(),
        record: bad(),
    }];
    for (what, p) in [("preview", a), ("stream", b), ("derived", c)] {
        let e = PointerError::BadChunk(ChunkError::LengthMismatch);
        assert_eq!(both(&p), (Some(e), Some(e)), "{what}");
    }
}

#[test]
fn c5_chunk_refs_names_every_reference_the_pointer_holds() {
    // The live set of the prune reads the references from here: inline chunks, previews, streams, derived
    // records; the chunks of an object are in the object.
    let p = full();
    assert_eq!(p.chunk_refs().count(), 3 + 3 + 3 + 2);
    let tags: Vec<u8> = p.chunk_refs().map(|r| r.pack.as_bytes()[0]).collect();
    assert_eq!(
        tags,
        vec![0x41, 0x42, 0x43, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x71, 0x72]
    );
    let sealer = TestSealer::new();
    let (huge_pointer, _, _) = huge(&sealer);
    assert_eq!(huge_pointer.chunk_refs().count(), 0);
}

// ---------------------------------------------------------------------------------------------
// C6
// ---------------------------------------------------------------------------------------------

#[test]
fn c6_canonical_representation_inline_or_object() {
    // 1,024 chunks fit inline: that is the encoding of such a file.
    let list = huge_chunks(1024);
    let mut inline = minimal_empty();
    inline.size = FileSize::new(total_raw(&list));
    inline.chunks = ChunkList::Inline(list);
    assert_eq!(inline.check_consistency(), Ok(()));
    assert_eq!(both(&inline), (None, None));

    // 1,025 inline are refused (TooManyInline), written or read.
    let list = huge_chunks(1025);
    let mut too_many = minimal_empty();
    too_many.size = FileSize::new(total_raw(&list));
    too_many.chunks = ChunkList::Inline(list);
    let e = PointerError::TooManyInline;
    assert_eq!(both(&too_many), (Some(e), Some(e)));

    // An object for 1,024 chunks or fewer is refused (ObjectTooSmall): such a file has them inline.
    for count in [0u32, 1, 1000, 1024] {
        let mut object = minimal_empty();
        object.size = FileSize::new(u64::from(count) * 1000);
        object.chunks = ChunkList::Object(ChunkListRef {
            name: [0xCC; 32],
            count,
            total_raw: FileSize::new(u64::from(count) * 1000),
        });
        let e = PointerError::ObjectTooSmall;
        assert_eq!(both(&object), (Some(e), Some(e)), "count {count}");
    }
    // ... and one for 1,025 is the canonical form.
    let mut object = minimal_empty();
    object.size = FileSize::new(1025 * 1000);
    object.chunks = ChunkList::Object(ChunkListRef {
        name: [0xCC; 32],
        count: 1025,
        total_raw: FileSize::new(1025 * 1000),
    });
    assert_eq!(both(&object), (None, None));
    // The writer of the object agrees on the threshold.
    let sealer = TestSealer::new();
    assert_eq!(
        encode_chunk_list(&huge_chunks(1024), &space(), &sealer).err(),
        Some(PointerError::ObjectTooSmall)
    );
    assert!(encode_chunk_list(&huge_chunks(1025), &space(), &sealer).is_ok());
}

// ---------------------------------------------------------------------------------------------
// C8
// ---------------------------------------------------------------------------------------------

type Build = Box<dyn Fn(&mut PointerV2)>;

/// One case of the table: its name, how to build the pointer from a valid one, and what writing and
/// reading it must say (`None`: it is accepted).
type Case = (&'static str, Build, Option<PointerError>);

fn case(
    list: &mut Vec<Case>,
    name: &'static str,
    expected: Option<PointerError>,
    f: impl Fn(&mut PointerV2) + 'static,
) {
    list.push((name, Box::new(f), expected));
}

#[test]
fn c8_bounds_of_every_collection() {
    let over = |limit: Limit| Some(PointerError::OverLimit(limit));
    let mut cases: Vec<Case> = Vec::new();
    let c = &mut cases;

    case(c, "previews at the limit", None, |p| {
        p.previews = previews(64)
    });
    case(c, "previews over the limit", over(Limit::Previews), |p| {
        p.previews = previews(65)
    });
    case(c, "streams at the limit", None, |p| p.streams = streams(32));
    case(c, "streams over the limit", over(Limit::Streams), |p| {
        p.streams = streams(33)
    });
    case(c, "xattrs at the limit", None, |p| {
        p.xattrs = (0..64)
            .map(|i| (format!("user.{i}"), vec![i as u8]))
            .collect()
    });
    case(c, "xattrs over the limit", over(Limit::Xattrs), |p| {
        p.xattrs = (0..65)
            .map(|i| (format!("user.{i}"), vec![i as u8]))
            .collect()
    });
    case(c, "labels at the limit", None, |p| {
        p.labels = (0..32).map(|i| format!("l{i}")).collect()
    });
    case(c, "labels over the limit", over(Limit::Labels), |p| {
        p.labels = (0..33).map(|i| format!("l{i}")).collect()
    });
    case(c, "derived at the limit", None, |p| p.derived = derived(64));
    case(c, "derived over the limit", over(Limit::Derived), |p| {
        p.derived = derived(65)
    });
    case(c, "preview chunks at the limit", None, |p| {
        p.previews = vec![Preview {
            kind: 1,
            w: 1,
            h: 1,
            chunks: refs(16),
        }]
    });
    case(
        c,
        "preview chunks over the limit",
        over(Limit::PreviewChunks),
        |p| {
            p.previews = vec![Preview {
                kind: 1,
                w: 1,
                h: 1,
                chunks: refs(17),
            }]
        },
    );
    case(c, "stream chunks at the limit", None, |p| {
        p.streams = vec![StreamRef {
            kind: 1,
            w: 1,
            h: 1,
            bandwidth: 1,
            chunks: refs(1024),
        }]
    });
    case(
        c,
        "stream chunks over the limit",
        over(Limit::StreamChunks),
        |p| {
            p.streams = vec![StreamRef {
                kind: 1,
                w: 1,
                h: 1,
                bandwidth: 1,
                chunks: refs(1025),
            }]
        },
    );
    case(
        c,
        "a preview without chunks",
        Some(PointerError::EmptyList(Part::Preview)),
        |p| {
            p.previews = vec![Preview {
                kind: 1,
                w: 1,
                h: 1,
                chunks: Vec::new(),
            }]
        },
    );
    case(
        c,
        "a stream without chunks",
        Some(PointerError::EmptyList(Part::Stream)),
        |p| {
            p.streams = vec![StreamRef {
                kind: 1,
                w: 1,
                h: 1,
                bandwidth: 1,
                chunks: Vec::new(),
            }]
        },
    );
    case(c, "text at the limit", None, |p| {
        p.derived = vec![DerivedRef {
            extractor: "e".repeat(256),
            model_id: "m".repeat(256),
            model_version: "v".repeat(256),
            record: tiny(0),
        }]
    });
    for which in 0..3 {
        let name: &'static str = [
            "an extractor over the limit",
            "a model id over the limit",
            "a model version over the limit",
        ][which];
        case(c, name, over(Limit::Text), move |p| {
            let mut d = DerivedRef {
                extractor: "e".to_string(),
                model_id: String::new(),
                model_version: String::new(),
                record: tiny(0),
            };
            match which {
                0 => d.extractor = "e".repeat(257),
                1 => d.model_id = "m".repeat(257),
                _ => d.model_version = "v".repeat(257),
            }
            p.derived = vec![d];
        });
    }
    case(c, "an empty extractor", Some(PointerError::BadText), |p| {
        p.derived = vec![DerivedRef {
            extractor: String::new(),
            model_id: String::new(),
            model_version: String::new(),
            record: tiny(0),
        }]
    });
    case(c, "a NUL in a model id", Some(PointerError::BadText), |p| {
        p.derived = vec![DerivedRef {
            extractor: "e".to_string(),
            model_id: "a\0b".to_string(),
            model_version: String::new(),
            record: tiny(0),
        }]
    });
    case(c, "a label at the limit", None, |p| {
        p.labels = vec!["l".repeat(128)]
    });
    case(c, "a label over the limit", over(Limit::Label), |p| {
        p.labels = vec!["l".repeat(129)]
    });
    case(c, "an empty label", Some(PointerError::BadText), |p| {
        p.labels = vec![String::new()]
    });
    case(c, "a NUL in a label", Some(PointerError::BadText), |p| {
        p.labels = vec!["a\0b".to_string()]
    });
    case(c, "an xattr name at the limit", None, |p| {
        p.xattrs = vec![("n".repeat(255), vec![1])]
    });
    case(
        c,
        "an xattr name over the limit",
        over(Limit::XattrName),
        |p| p.xattrs = vec![("n".repeat(256), vec![1])],
    );
    case(
        c,
        "an empty xattr name",
        Some(PointerError::BadXattrName),
        |p| p.xattrs = vec![(String::new(), vec![1])],
    );
    case(
        c,
        "a NUL in an xattr name",
        Some(PointerError::BadXattrName),
        |p| p.xattrs = vec![("user.a\0b".to_string(), vec![1])],
    );
    case(
        c,
        "a duplicate xattr name",
        Some(PointerError::DuplicateXattr),
        |p| {
            p.xattrs = vec![
                ("user.a".to_string(), vec![1]),
                ("user.b".to_string(), vec![2]),
                ("user.a".to_string(), vec![3]),
            ]
        },
    );
    case(c, "an xattr value at the limit", None, |p| {
        p.xattrs = vec![("a".to_string(), vec![0; 65_536])]
    });
    case(
        c,
        "an xattr value over the limit",
        over(Limit::XattrValue),
        |p| p.xattrs = vec![("a".to_string(), vec![0; 65_537])],
    );
    case(c, "all xattrs together at the limit", None, |p| {
        p.xattrs = vec![
            ("a".to_string(), vec![0; 65_536]),
            ("b".to_string(), vec![0; 65_536]),
            ("c".to_string(), vec![0; 65_536]),
            ("d".to_string(), vec![0; 65_532]),
        ]
    });
    case(
        c,
        "all xattrs together over the limit",
        over(Limit::XattrTotal),
        |p| {
            p.xattrs = vec![
                ("a".to_string(), vec![0; 65_536]),
                ("b".to_string(), vec![0; 65_536]),
                ("c".to_string(), vec![0; 65_536]),
                ("d".to_string(), vec![0; 65_533]),
            ]
        },
    );
    // A byte string of n bytes encodes to n + 3 bytes for 256 <= n < 65,536.
    case(c, "transform parameters at the limit", None, |p| {
        with_transform(p, Value::Bytes(vec![0; 4093]))
    });
    case(
        c,
        "transform parameters over the limit",
        over(Limit::TransformParams),
        |p| with_transform(p, Value::Bytes(vec![0; 4094])),
    );
    case(c, "a dimension at the limit", None, |p| {
        p.previews = vec![Preview {
            kind: 255,
            w: 65_535,
            h: 65_535,
            chunks: vec![tiny(0)],
        }];
        p.streams = vec![StreamRef {
            kind: 255,
            w: 65_535,
            h: 65_535,
            bandwidth: u32::MAX,
            chunks: vec![tiny(1)],
        }];
    });
    case(
        c,
        "a preview width over the limit",
        Some(PointerError::BadDimension),
        |p| {
            p.previews = vec![Preview {
                kind: 1,
                w: 65_536,
                h: 1,
                chunks: vec![tiny(0)],
            }]
        },
    );
    case(
        c,
        "a preview height over the limit",
        Some(PointerError::BadDimension),
        |p| {
            p.previews = vec![Preview {
                kind: 1,
                w: 1,
                h: 65_536,
                chunks: vec![tiny(0)],
            }]
        },
    );
    case(
        c,
        "a stream width over the limit",
        Some(PointerError::BadDimension),
        |p| {
            p.streams = vec![StreamRef {
                kind: 1,
                w: 65_536,
                h: 1,
                bandwidth: 1,
                chunks: vec![tiny(0)],
            }]
        },
    );
    case(
        c,
        "a stream height over the limit",
        Some(PointerError::BadDimension),
        |p| {
            p.streams = vec![StreamRef {
                kind: 1,
                w: 1,
                h: 65_536,
                bandwidth: 1,
                chunks: vec![tiny(0)],
            }]
        },
    );

    assert!(cases.len() >= 42, "{} cases", cases.len());
    for (name, build, expected) in &cases {
        let mut p = minimal_one_chunk();
        build(&mut p);
        // Written and read agree, and both give the answer the table says.
        assert_eq!(both(&p), (*expected, *expected), "{name}");
        if expected.is_none() {
            // What is at the limit is a pointer: it writes, reads back equal, and verifies once signed.
            let bytes = p.to_cbor().expect("writes");
            let Decoded::Current(back) = PointerV2::from_cbor(&bytes, &OBJECT).expect("reads")
            else {
                panic!("{name}: current");
            };
            assert!(*back == p, "{name}: read back equal");
            let signed_copy = signed(p.duplicate());
            assert_eq!(
                signed_copy
                    .verify(&space(), &directory(), azlin_keys::VerifyAt::Now)
                    .map(|_| ()),
                Ok(()),
                "{name}"
            );
        }
    }
}

#[test]
fn c8_a_hostile_pointer_with_a_million_tiny_previews_is_refused_by_the_limits() {
    // {1: 2, 9: an array that claims a million elements}: refused by the codec's item limit before any
    // element is read.
    assert_eq!(
        PointerV2::from_cbor(&from_hex("a20102099a000f4240"), &OBJECT).err(),
        Some(PointerError::Cbor(azlin_cbor::DecodeError::TooManyItems))
    );
    // 65,000 tiny previews that are really there (nearly the most one array may hold): refused by the
    // pointer's own limit of 64 before a single preview is read.
    let tiny_preview = Value::Array(vec![
        Value::Uint(0),
        Value::Uint(0),
        Value::Uint(0),
        Value::Array(Vec::new()),
    ]);
    let bytes = with_entry(
        &signed(minimal_one_chunk()).to_cbor().expect("writes"),
        9,
        Some(Value::Array(vec![tiny_preview; 65_000])),
    );
    assert_eq!(
        PointerV2::from_cbor(&bytes, &OBJECT).err(),
        Some(PointerError::OverLimit(Limit::Previews))
    );
    // The same for the other collections.
    for (key, limit, n) in [
        (10u64, Limit::Streams, 33usize),
        (11, Limit::Xattrs, 65),
        (12, Limit::Labels, 33),
        (13, Limit::Derived, 65),
    ] {
        let junk = Value::Array(vec![Value::Null; n]);
        let bytes = with_entry(
            &signed(minimal_one_chunk()).to_cbor().expect("writes"),
            key,
            Some(junk),
        );
        assert_eq!(
            PointerV2::from_cbor(&bytes, &OBJECT).err(),
            Some(PointerError::OverLimit(limit)),
            "key {key}: refused by count, before any of the {n} entries is read"
        );
    }
    // More than INLINE_MAX_CHUNKS inline: refused by count too.
    let junk = Value::Array(vec![Value::Null; 1025]);
    let bytes = with_entry(
        &signed(minimal_one_chunk()).to_cbor().expect("writes"),
        7,
        Some(junk),
    );
    assert_eq!(
        PointerV2::from_cbor(&bytes, &OBJECT).err(),
        Some(PointerError::TooManyInline)
    );
}

#[test]
fn c8_the_largest_valid_pointer_fits_the_codec_limits() {
    // Every collection at its limit at once and every chunk reference with the longest numbers a valid one
    // can have (raw 4 MiB, the longest blob, the last offset that fits 32 bits): the writer still writes it
    // (the codec's OBJECT limits: 4 MiB, 65,536 items per container, 1,048,576 items in all) and the reader
    // reads it back. So no pointer that passes `validate` is one the codec refuses, whatever combination of
    // legal collections it holds (an offline count gives 34,880 references, about 2.7 MB and 210,000 items).
    let big = |i: usize| -> ChunkRef {
        let mut r = chunk((i % 250) as u8 + 1, 4 * 1024 * 1024, 4 * 1024 * 1024 + 41);
        r.offset = PackOffset::new(u32::MAX - (4 * 1024 * 1024 + 41));
        r
    };
    let mut p = minimal_empty();
    p.size = FileSize::new(1 << 50);
    p.transform = Some(TransformRef {
        id: u16::MAX,
        out_len: FileSize::new(1024 * 4 * 1024 * 1024),
        params: Value::Bytes(vec![0; 4093]),
    });
    p.chunks = ChunkList::Inline((0..1024).map(big).collect());
    p.mtime = azlin_content::FileTime::new(-62_167_219_200, 999_999_999).expect("a time");
    p.mode = 0o7777;
    p.created_at = azlin_keys::UnixTime::from_secs(253_402_300_799);
    p.previews = (0..64)
        .map(|i| Preview {
            kind: 255,
            w: 65_535,
            h: 65_535,
            chunks: (0..16).map(|j| big(i + j)).collect(),
        })
        .collect();
    p.streams = (0..32)
        .map(|i| StreamRef {
            kind: 255,
            w: 65_535,
            h: 65_535,
            bandwidth: u32::MAX,
            chunks: (0..1024).map(|j| big(i + j)).collect(),
        })
        .collect();
    p.xattrs = (0..64)
        .map(|i| (format!("{i:0>255}"), vec![0u8; 3840]))
        .collect();
    p.labels = (0..32).map(|i| format!("{i:l<128}")).collect();
    p.derived = (0..64)
        .map(|i| DerivedRef {
            extractor: format!("{i:e<256}"),
            model_id: "m".repeat(256),
            model_version: "v".repeat(256),
            record: big(i),
        })
        .collect();
    p.unknown = azlin_content::pointer::UnknownFields::from_entries(vec![(
        40,
        Value::Bytes(vec![1; 1000]),
    )])
    .expect("keys");
    p.crit = vec![2, 7];

    let bytes = p.to_cbor().expect("the largest valid pointer writes");
    assert!(bytes.len() < 4 * 1024 * 1024, "{} bytes", bytes.len());
    assert!(
        bytes.len() > 2 * 1024 * 1024,
        "{} bytes: smaller than intended",
        bytes.len()
    );
    assert_eq!(bytes.as_slice(), reference_encode(&p, true).as_slice());
    let Decoded::Current(back) = PointerV2::from_cbor(&bytes, &OBJECT).expect("reads") else {
        panic!("current");
    };
    assert!(*back == p);
    // ... and it signs and verifies like any other.
    let signed_copy = signed(p.duplicate());
    signed_copy
        .verify(&space(), &directory(), azlin_keys::VerifyAt::Now)
        .expect("verifies");
}

// ---------------------------------------------------------------------------------------------
// C11
// ---------------------------------------------------------------------------------------------

/// A decompression bomb: a stream that would produce 1 GiB. It obeys `max_len` as the seam asks (it stops
/// at the first byte past it) and counts what it produced.
#[derive(Default)]
struct Bomb {
    produced: Cell<usize>,
    max_len_seen: Cell<usize>,
}

impl Compressor for Bomb {
    fn compress(&self, _plaintext: &[u8]) -> Result<Option<Compressed>, CodecError> {
        Ok(None)
    }

    fn decompress(
        &self,
        _codec: Codec,
        _stored: &[u8],
        max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
        self.max_len_seen.set(max_len.as_usize());
        let mut out = Zeroizing::new(Vec::new());
        for produced in 0..(1usize << 30) {
            if produced >= max_len.as_usize() {
                self.produced.set(produced + 1);
                return Err(CodecError::TooLong);
            }
            out.push(0xAA);
        }
        self.produced.set(out.len());
        Ok(out)
    }
}

#[test]
fn c11_max_decoded_len_is_the_declared_size() {
    // The bound is the pointer's `size`: with and without a transform, for an empty file, a small one, a
    // file with every field and the pointer of a huge file.
    let sealer = TestSealer::new();
    let (huge_pointer, _, _) = huge(&sealer);
    for p in [minimal_empty(), minimal_one_chunk(), full(), huge_pointer] {
        assert_eq!(p.max_decoded_len(), p.size);
    }
    // With a transform it is still the size of the ORIGINAL, not the length of the transformed stream.
    let p = full();
    assert_eq!(p.max_decoded_len().get(), 3_000_000);
    assert_eq!(
        p.transform.as_ref().expect("a transform").out_len.get(),
        2_600_000
    );

    // A decoder bound by it stops a stream of size + 1 bytes: a one-chunk file of 1,000 bytes whose chunk
    // claims a compressor and meets a bomb. open_chunk hands the bomb the pointer's bound as `max_len`.
    let pointer = minimal_one_chunk();
    let bound = ChunkLen::new(u32::try_from(pointer.max_decoded_len().get()).expect("small"))
        .expect("a length");
    let k = KChunk::from_secret(Secret::from_bytes([1; 32]));
    let plaintext = vec![7u8; 1000];
    let (full_hash, id) = identity(&k, &plaintext);
    // The chunk's stored bytes are a few bytes that "decompress" (by the bomb) to far more.
    let blob = seal_stored(&space(), &full_hash, &id, Codec::Zstd, &[1, 2, 3, 4]).expect("seals");
    let bomb = Bomb::default();
    let result = open_chunk(
        &space(),
        &full_hash,
        &id,
        blob.as_bytes(),
        bound,
        OpenAs::Member(&k),
        &bomb,
    );
    assert_eq!(result.err(), Some(ChunkError::Codec(CodecError::TooLong)));
    assert_eq!(
        bomb.max_len_seen.get(),
        1000,
        "the bound the decoder was given is the declared size"
    );
    assert_eq!(
        bomb.produced.get(),
        1001,
        "it stopped at the first byte past the declared size"
    );
}
