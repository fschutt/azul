//! C1, C2 and C8: the header and the layout of an AZP2 pack, pinned (planning/architecture.md section 5.7
//! "Pack (AZP2)", section 2.5 "Every pack must have the AZP2 header (magic, version, class, sizes)").
//!
//! C1: the known answers in `tests/vectors/azp2_pack.json`. Every positive vector is a pack of fixed
//! chunks, a fixed `K_seal` and a fixed nonce, made for each of the three classes under each of the
//! three paddings (and a few more). Its bytes come from the INDEPENDENT reference of `pack_common`
//! (blake3, chacha20poly1305 and hand-written CBOR, not the crate's pack code), and the builder is held
//! to them byte for byte; the file is written only under `UPDATE_VECTORS=1`, never when `CI` is set. The
//! negative vectors give bytes and the typed refusal they get.
//!
//! C2: the header is strict, field by field, each refusal with its own typed error.
//!
//! C8: the name, the id and the storage key of a pack.

mod pack_common;

use std::collections::BTreeSet;

use azlin_content::pack::{
    check_header, pack_id, pack_name, read_trailer, rebuild_entries, storage_key, NotSealed,
    PackError, PackGeometry, PackHeader, Padding, SealKey, HARD_MAX, MAX_CHUNKS,
};
use azlin_content::{ChunkId, PackClass, PackId, PackLen, SealedChunk};
use azlin_keys::{Epoch, Secret, SpaceId};
use pack_common::{
    build, build_with, check_vector_file, chunk, field, from_hex, k_chunk, number, read_rows,
    ref_pack, ref_padded_len, ref_seal, ref_unpadded_len, render_rows, seal_key, space, stream,
    to_hex, FixedNonce, RefChunk, RefPack, RefParams, SplitMix64,
};

const S: u8 = 0x52;
const K: u8 = 0x31;
const KS: u8 = 0x44;

fn padding_of(policy: &str) -> Padding {
    match policy {
        "none" => Padding::None,
        "buckets" => Padding::Buckets,
        "full" => Padding::Full,
        other => panic!("no policy {other}"),
    }
}

/// The geometry of the vectors: small, so that a whole pack is a few KiB of hex.
fn vector_geometry() -> PackGeometry {
    PackGeometry {
        target: PackLen::new(2048),
        hard_max: PackLen::new(3072),
        pad_step: PackLen::new(512),
        max_chunks: 16,
    }
}

struct Case {
    name: String,
    class: u8,
    policy: &'static str,
    last: bool,
    default_geometry: bool,
    epoch: u32,
    plain_lens: Vec<usize>,
    seed: u64,
}

fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    let mut seed = 100u64;
    for (class, class_name) in [(0u8, "content"), (1, "derived"), (2, "repository")] {
        for policy in ["none", "buckets", "full"] {
            seed += 10;
            v.push(Case {
                name: format!("{class_name}_{policy}"),
                class,
                policy,
                last: true,
                default_geometry: false,
                epoch: 3,
                plain_lens: vec![80, 100],
                seed,
            });
        }
    }
    v.push(Case {
        name: "content_buckets_not_last".into(),
        class: 0,
        policy: "buckets",
        last: false,
        default_geometry: false,
        epoch: 3,
        plain_lens: vec![80, 100],
        seed: 301,
    });
    v.push(Case {
        name: "content_none_five_chunks_epoch7".into(),
        class: 0,
        policy: "none",
        last: true,
        default_geometry: false,
        epoch: 7,
        plain_lens: vec![1, 255, 256, 300, 17],
        seed: 311,
    });
    v.push(Case {
        name: "content_buckets_default_geometry".into(),
        class: 0,
        policy: "buckets",
        last: true,
        default_geometry: true,
        epoch: 3,
        plain_lens: vec![80, 100, 1000],
        seed: 321,
    });
    v.push(Case {
        name: "repository_buckets_default_geometry".into(),
        class: 2,
        policy: "buckets",
        last: true,
        default_geometry: true,
        epoch: 1,
        plain_lens: vec![500],
        seed: 331,
    });
    v
}

struct Made {
    params: RefParams,
    chunks: Vec<RefChunk>,
    reference: RefPack,
}

/// The pack of a case by the reference, and (asserted equal) by the builder.
fn make(c: &Case) -> Made {
    let sp = space(S);
    let k = k_chunk(K);
    let geometry = if c.default_geometry {
        PackGeometry::default()
    } else {
        vector_geometry()
    };
    let chunks: Vec<(ChunkId, SealedChunk)> = c
        .plain_lens
        .iter()
        .enumerate()
        .map(|(i, &len)| chunk(&sp, &k, c.seed + i as u64, len))
        .collect();
    let ref_chunks: Vec<RefChunk> = chunks
        .iter()
        .map(|(id, blob)| RefChunk {
            id: *id.as_bytes(),
            blob: blob.as_bytes().to_vec(),
        })
        .collect();
    let unpadded = ref_unpadded_len(&ref_chunks);
    let padded = ref_padded_len(unpadded, c.class, c.policy, c.last, &geometry);
    let nonce: [u8; 24] = stream(c.seed, 24).try_into().expect("24 bytes");
    let params = RefParams {
        space: [S; 16],
        epoch: c.epoch,
        k_seal: [KS; 32],
        class: c.class,
        pad_len: padded - unpadded,
        nonce,
    };
    let reference = ref_pack(&params, &ref_chunks);

    let lib = build_with(
        &sp,
        PackClass::from_byte(c.class).expect("a class"),
        c.epoch,
        KS,
        geometry,
        padding_of(c.policy),
        c.last,
        &chunks,
        &mut FixedNonce(nonce),
    );
    assert!(
        lib.bytes() == reference.bytes.as_slice(),
        "{}: the builder's pack is not the reference's ({} and {} bytes)",
        c.name,
        lib.bytes().len(),
        reference.bytes.len()
    );
    assert_eq!(lib.name(), pack_name(&reference.bytes), "{}", c.name);
    assert_eq!(lib.entries().len(), chunks.len(), "{}", c.name);
    Made {
        params,
        chunks: ref_chunks,
        reference,
    }
}

fn positive_row(c: &Case, m: &Made) -> String {
    let pack = &m.reference.bytes;
    let ids: Vec<String> = m.chunks.iter().map(|c| to_hex(&c.id)).collect();
    let blobs: Vec<String> = m.chunks.iter().map(|c| to_hex(&c.blob)).collect();
    let name = blake3::hash(pack);
    let id = &name.as_bytes()[..16];
    let hex = if pack.len() <= 4096 {
        to_hex(pack)
    } else {
        String::new()
    };
    format!(
        "{{\"name\": \"{}\", \"valid\": true, \"expect\": \"ok\", \"class\": {}, \"padding\": \"{}\", \"last\": {}, \"geometry\": \"{}\", \"epoch\": {}, \"space\": \"{}\", \"k_seal\": \"{}\", \"nonce\": \"{}\", \"ids\": \"{}\", \"blobs\": \"{}\", \"chunk_count\": {}, \"body_len\": {}, \"pad_len\": {}, \"pack_len\": {}, \"header\": \"{}\", \"trailer\": \"{}\", \"pack_blake3\": \"{}\", \"pack_id\": \"{}\", \"storage_key\": \"s/{}/p/{:02x}/{}\", \"hex\": \"{hex}\"}}",
        c.name,
        c.class,
        c.policy,
        u8::from(c.last),
        if c.default_geometry { "default" } else { "vector" },
        c.epoch,
        to_hex(&m.params.space),
        to_hex(&m.params.k_seal),
        to_hex(&m.params.nonce),
        ids.join(","),
        blobs.join(","),
        m.chunks.len(),
        m.reference.body.len(),
        m.params.pad_len,
        pack.len(),
        to_hex(&m.reference.header),
        to_hex(&m.reference.trailer),
        name.to_hex(),
        to_hex(id),
        to_hex(&m.params.space),
        id[0],
        to_hex(id),
    )
}

/// A vector that is not a family of its own: bytes, what to do with them (`parse` the header of a pack
/// of `total_len` bytes, or `rebuild` the entries of a whole pack) and what comes out (`ok`, or the
/// typed refusal).
struct Odd {
    name: &'static str,
    valid: bool,
    op: &'static str,
    bytes: Vec<u8>,
    total_len: u32,
    expect: String,
}

fn odd_row(n: &Odd) -> String {
    format!(
        "{{\"name\": \"{}\", \"valid\": {}, \"op\": \"{}\", \"epoch\": 3, \"space\": \"{}\", \"k_seal\": \"{}\", \"total_len\": {}, \"hex\": \"{}\", \"expect\": \"{}\"}}",
        n.name,
        n.valid,
        n.op,
        to_hex(&[S; 16]),
        to_hex(&[KS; 32]),
        n.total_len,
        to_hex(&n.bytes),
        n.expect
    )
}

/// The base of the odd vectors: two chunks (blobs of 121 and 141 bytes, whose lines take 22 bytes each),
/// epoch 3, class content, padded to 512 bytes by the buckets of the vector geometry (104 bytes).
fn odd_base() -> Made {
    let case = Case {
        name: "odd_base".into(),
        class: 0,
        policy: "buckets",
        last: true,
        default_geometry: false,
        epoch: 3,
        plain_lens: vec![80, 100],
        seed: 900,
    };
    let m = make(&case);
    assert_eq!(m.reference.bytes.len(), 512, "the base pack is 512 bytes");
    assert_eq!(m.params.pad_len, 104, "of which 104 bytes of padding");
    assert_eq!(m.reference.trailer.len(), 122, "and a trailer of 122 bytes");
    m
}

/// The base pack with its trailer sealed over another plaintext (and the same header, body, padding).
fn with_plain(m: &Made, plain: &[u8]) -> Vec<u8> {
    ref_seal(&m.params, 2, &m.reference.body, plain).bytes
}

fn odd_vectors(m: &Made) -> Vec<Odd> {
    let pack = m.reference.bytes.clone();
    let total = pack.len() as u32;
    let header = m.reference.header;
    let parse = |name: &'static str, edit: &dyn Fn(&mut [u8; 24]), expect: &str| {
        let mut h = header;
        edit(&mut h);
        Odd {
            name,
            valid: false,
            op: "parse",
            bytes: h.to_vec(),
            total_len: total,
            expect: expect.to_string(),
        }
    };
    let rebuild = |name: &'static str, bytes: Vec<u8>, valid: bool, expect: &str| Odd {
        name,
        valid,
        op: "rebuild",
        total_len: bytes.len() as u32,
        bytes,
        expect: expect.to_string(),
    };

    // The standard plaintext, to be changed in place: `a2 01 82`, the line of chunk 1 at 3..25 (its id
    // at 5..21, its offset 24 as `18 18` at 21..23), the line of chunk 2 at 25..47 (its id at 27..43),
    // then `02 58 20` and the hash.
    let base_plain = m.reference.plain.clone();
    assert_eq!(base_plain.len(), 82);
    assert_eq!(&base_plain[..5], &[0xa2, 0x01, 0x82, 0x83, 0x50]);
    assert_eq!(&base_plain[21..23], &[0x18, 0x18]);
    assert_eq!(&base_plain[25..27], &[0x83, 0x50]);
    assert_eq!(
        base_plain[24], 0x79,
        "the length of blob 1 (121) is the last byte of its line"
    );
    assert_eq!(
        base_plain[46], 0x8d,
        "the length of blob 2 (141) is the last byte of its line"
    );

    assert_eq!(
        base_plain[44], 0x91,
        "the offset of blob 2 (145) is the last byte of its offset head"
    );

    // Blob 1 "is" 40 bytes, less than its framing, and blob 2 takes the rest of the body (222 bytes, at
    // offset 64): the lines are back to back and end at the end of the body, so only the check of the
    // framing refuses them.
    let mut below_framing = base_plain.clone();
    below_framing[24] = 0x28;
    below_framing[44] = 0x40;
    below_framing[46] = 0xde;
    // Blob 2 "is" 140 bytes: the lines are back to back and stop one byte before the body ends, so only
    // the check of the end of the body refuses them.
    let mut short_of_the_end = base_plain.clone();
    short_of_the_end[46] = 0x8c;
    // Blob 2 starts one byte late (offset 146) and is one byte shorter (140): the lines end where the
    // body ends, so only the check that each blob starts where the last one ended refuses them.
    let mut with_a_gap = base_plain.clone();
    with_a_gap[44] = 0x92;
    with_a_gap[46] = 0x8c;
    // The header says three lines, the plaintext lists two (and an ignored key 9 of 20 bytes brings the
    // trailer to 144 bytes, the least that three lines make, so that the sizes of the header agree).
    let mut miscounted = vec![0xa3];
    miscounted.extend_from_slice(&base_plain[1..]);
    miscounted.extend_from_slice(&[0x09, 0x54]);
    miscounted.extend_from_slice(&[0u8; 20]);
    assert_eq!(miscounted.len(), 104);
    let mut wrong_type = base_plain.clone();
    wrong_type[4] = 0x70; // the id of chunk 1 is text, not bytes
    let mut off_by_one = base_plain.clone();
    off_by_one[22] = 0x19; // the first blob "starts" at 25
    let mut duplicate = base_plain.clone();
    let first_id = base_plain[5..21].to_vec();
    duplicate[27..43].copy_from_slice(&first_id); // chunk 2 has the id of chunk 1
    let mut crit = vec![0xa3, 0x00, 0x81, 0x09]; // key 0 (crit) lists key 9, which nobody knows
    crit.extend_from_slice(&base_plain[1..]);
    let mut unknown = vec![0xa3]; // a key 9 (null) nobody asked for, and crit does not list it
    unknown.extend_from_slice(&base_plain[1..]);
    unknown.extend_from_slice(&[0x09, 0xf6]);

    let mut flipped_trailer = pack.clone();
    *flipped_trailer.last_mut().expect("a byte") ^= 1;
    let mut flipped_body = pack.clone();
    flipped_body[100] ^= 0x10;
    let mut flipped_padding = pack.clone();
    flipped_padding[24 + m.reference.body.len() + 5] ^= 0x04;
    let mut extended = pack.clone();
    extended.push(0);

    vec![
        parse("neg_version_2", &|h| h[4] = 2, "UnsupportedVersion(2)"),
        parse("neg_bad_magic", &|h| h[3] = b'1', "BadMagic"),
        parse(
            "neg_reserved_bytes_not_zero",
            &|h| h[7] = 1,
            "ReservedNotZero",
        ),
        parse("neg_class_3", &|h| h[5] = 3, "BadClass(3)"),
        parse("neg_chunk_count_zero", &|h| h[12..16].fill(0), "NoChunks"),
        parse(
            "neg_body_len_past_the_pack",
            &|h| h[16..20].copy_from_slice(&total.to_le_bytes()),
            "SizesInconsistent",
        ),
        Odd {
            name: "neg_header_truncated",
            valid: false,
            op: "parse",
            bytes: header[..23].to_vec(),
            total_len: total,
            expect: "TooShort".to_string(),
        },
        rebuild(
            "neg_pack_truncated_by_one_byte",
            pack[..pack.len() - 1].to_vec(),
            false,
            "SizesInconsistent",
        ),
        rebuild("neg_pack_extended_by_one_byte", extended, false, "Auth"),
        rebuild("neg_flipped_trailer_bit", flipped_trailer, false, "Auth"),
        rebuild("neg_flipped_body_bit", flipped_body, false, "Auth"),
        rebuild("neg_flipped_padding_bit", flipped_padding, false, "Auth"),
        rebuild(
            "neg_lines_of_the_wrong_type",
            with_plain(m, &wrong_type),
            false,
            "BadTrailer",
        ),
        rebuild(
            "neg_first_blob_not_at_24",
            with_plain(m, &off_by_one),
            false,
            "BadTrailer",
        ),
        rebuild(
            "neg_lines_with_a_gap",
            with_plain(m, &with_a_gap),
            false,
            "BadTrailer",
        ),
        rebuild(
            "neg_chunk_id_twice",
            with_plain(m, &duplicate),
            false,
            "DuplicateChunk",
        ),
        rebuild(
            "neg_header_counts_three_lines_the_trailer_lists_two",
            ref_seal(&m.params, 3, &m.reference.body, &miscounted).bytes,
            false,
            "BadTrailer",
        ),
        rebuild(
            "neg_blob_below_its_framing",
            with_plain(m, &below_framing),
            false,
            "BadTrailer",
        ),
        rebuild(
            "neg_lines_stop_short_of_the_body_end",
            with_plain(m, &short_of_the_end),
            false,
            "BadTrailer",
        ),
        rebuild(
            "neg_crit_lists_an_unknown_key",
            with_plain(m, &crit),
            false,
            "BadTrailer",
        ),
        rebuild(
            "pos_unknown_key_is_ignored",
            with_plain(m, &unknown),
            true,
            "ok",
        ),
    ]
}

#[test]
fn c1_header_roundtrip_and_golden() {
    let mut rows = Vec::new();
    for c in cases() {
        let m = make(&c);
        // The header of the pack, field by field, and back.
        let total = PackLen::from_len(m.reference.bytes.len()).expect("fits");
        let header = PackHeader::parse(&m.reference.bytes, total)
            .unwrap_or_else(|e| panic!("{}: {e}", c.name));
        assert_eq!(header.class.byte(), c.class, "{}", c.name);
        assert_eq!(header.key_epoch.get(), c.epoch, "{}", c.name);
        assert_eq!(
            header.chunk_count as usize,
            c.plain_lens.len(),
            "{}",
            c.name
        );
        assert_eq!(
            header.body_len.as_usize(),
            m.reference.body.len(),
            "{}",
            c.name
        );
        assert_eq!(header.pad_len.as_usize(), m.params.pad_len, "{}", c.name);
        assert_eq!(
            header.to_bytes(),
            m.reference.header,
            "{}: round trip",
            c.name
        );
        rows.push(positive_row(&c, &m));
    }
    let base = odd_base();
    for n in odd_vectors(&base) {
        rows.push(odd_row(&n));
    }
    check_vector_file("azp2_pack", &render_rows(&rows));

    // The file, read back: every row is held to what it says.
    let file = read_rows("azp2_pack");
    let positives: Vec<&String> = file.iter().filter(|r| r.contains("\"padding\"")).collect();
    let odds: Vec<&String> = file.iter().filter(|r| r.contains("\"op\"")).collect();
    let negatives = odds
        .iter()
        .filter(|r| r.contains("\"valid\": false"))
        .count();
    assert!(positives.len() >= 9, "{} positive vectors", positives.len());
    assert!(negatives >= 3, "{negatives} negative vectors");
    for class in 0..=2u64 {
        for policy in ["none", "buckets", "full"] {
            assert!(
                positives
                    .iter()
                    .any(|r| number(r, "class") == class && field(r, "padding") == policy),
                "a vector of class {class} under {policy}"
            );
        }
    }

    for row in &positives {
        let name = field(row, "name");
        let seal = SealKey {
            epoch: Epoch::new(number(row, "epoch") as u32).expect("an epoch"),
            key: Secret::from_slice(&from_hex(&field(row, "k_seal"))).expect("a key"),
        };
        let sp = SpaceId::from_bytes(from_hex(&field(row, "space")).try_into().expect("16 bytes"));
        let total = PackLen::new(number(row, "pack_len") as u32);
        let header = PackHeader::parse(&from_hex(&field(row, "header")), total)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            u64::from(header.class.byte()),
            number(row, "class"),
            "{name}"
        );
        assert_eq!(
            u64::from(header.key_epoch.get()),
            number(row, "epoch"),
            "{name}"
        );
        assert_eq!(
            u64::from(header.chunk_count),
            number(row, "chunk_count"),
            "{name}"
        );
        assert_eq!(
            u64::from(header.body_len.get()),
            number(row, "body_len"),
            "{name}"
        );
        assert_eq!(
            u64::from(header.pad_len.get()),
            number(row, "pad_len"),
            "{name}"
        );

        // The ranged read of the trailer: its range, then its bytes.
        let range = header
            .trailer_range(total)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let trailer = from_hex(&field(row, "trailer"));
        assert_eq!(
            (range.end.get() - range.start.get()) as usize,
            trailer.len(),
            "{name}"
        );
        let ids: Vec<String> = field(row, "ids").split(',').map(str::to_string).collect();
        let blobs: Vec<Vec<u8>> = field(row, "blobs").split(',').map(from_hex).collect();
        let entries =
            read_trailer(&header, &trailer, &sp, &seal).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(entries.len(), ids.len(), "{name}");
        let mut at = 24usize;
        for ((entry, id), blob) in entries.iter().zip(&ids).zip(&blobs) {
            assert_eq!(&to_hex(entry.id.as_bytes()), id, "{name}");
            assert_eq!(entry.offset.as_usize(), at, "{name}");
            assert_eq!(entry.length.as_usize(), blob.len(), "{name}");
            at += blob.len();
        }

        // The whole pack, where the file holds it: the entries rebuild, the blobs are where the lines
        // say, the name, the id and the storage key are the ones of the row.
        let whole = from_hex(&field(row, "hex"));
        if whole.is_empty() {
            continue;
        }
        assert_eq!(whole.len() as u64, number(row, "pack_len"), "{name}");
        assert_eq!(
            rebuild_entries(&whole, &sp, &seal).unwrap_or_else(|e| panic!("{name}: {e}")),
            entries,
            "{name}"
        );
        for (entry, blob) in entries.iter().zip(&blobs) {
            let from = entry.offset.as_usize();
            assert_eq!(&whole[from..from + blob.len()], blob.as_slice(), "{name}");
        }
        let pack_hash = pack_name(&whole);
        assert_eq!(field(row, "pack_blake3"), pack_hash.to_string(), "{name}");
        assert_eq!(
            field(row, "pack_id"),
            pack_id(&pack_hash).to_string(),
            "{name}"
        );
        assert_eq!(
            field(row, "storage_key"),
            storage_key(&sp, &pack_id(&pack_hash)),
            "{name}"
        );
    }

    for row in &odds {
        let name = field(row, "name");
        let bytes = from_hex(&field(row, "hex"));
        let total = PackLen::new(number(row, "total_len") as u32);
        let sp = SpaceId::from_bytes(from_hex(&field(row, "space")).try_into().expect("16 bytes"));
        let seal = seal_key(number(row, "epoch") as u32, KS);
        let got = match field(row, "op").as_str() {
            "parse" => PackHeader::parse(&bytes, total).map(|_| ()),
            "rebuild" => rebuild_entries(&bytes, &sp, &seal).map(|_| ()),
            other => panic!("{name}: no op {other}"),
        };
        match (row.contains("\"valid\": true"), got) {
            (true, Ok(())) => assert_eq!(field(row, "expect"), "ok", "{name}"),
            (false, Err(e)) => assert_eq!(format!("{e:?}"), field(row, "expect"), "{name}"),
            (valid, got) => panic!("{name}: valid {valid} but {got:?}"),
        }
    }
}

/// A header and the length of a pack it fits: the base of the strictness table. No padding: the pack is
/// 24 + 121 + 141 + 122 = 408 bytes, a trailer of 122 bytes, the least that two lines make.
fn base_header() -> ([u8; 24], PackLen) {
    let sp = space(S);
    let k = k_chunk(K);
    let chunks: Vec<(ChunkId, SealedChunk)> =
        vec![chunk(&sp, &k, 900, 80), chunk(&sp, &k, 901, 100)];
    let pack = build(
        &sp,
        PackClass::Content,
        3,
        KS,
        vector_geometry(),
        Padding::None,
        true,
        &chunks,
        5,
    );
    let mut header = [0u8; 24];
    header.copy_from_slice(&pack.bytes()[..24]);
    assert_eq!(pack.bytes().len(), 24 + 121 + 141 + 122);
    (header, pack.len())
}

/// A header of version `version` whose other bytes are the stream of a seed: never read as anything.
fn garbage_after_version(version: u8) -> [u8; 24] {
    let mut h = [0u8; 24];
    h.copy_from_slice(&stream(77, 24));
    h[..4].copy_from_slice(b"AZP2");
    h[4] = version;
    h
}

#[test]
fn c2_header_is_strict() {
    let (header, total) = base_header();
    let t = total.get();
    // The base parses, and bytes after the header are ignored.
    let parsed = PackHeader::parse(&header, total).expect("the base header parses");
    assert_eq!(parsed.chunk_count, 2);
    assert_eq!(parsed.body_len.get(), 262);
    assert_eq!(parsed.pad_len.get(), 0);
    assert_eq!(parsed.to_bytes(), header);
    let mut longer = header.to_vec();
    longer.extend_from_slice(&[7; 40]);
    assert_eq!(PackHeader::parse(&longer, total), Ok(parsed));

    let with = |edit: &dyn Fn(&mut [u8; 24])| -> [u8; 24] {
        let mut h = header;
        edit(&mut h);
        h
    };
    let big = PackLen::new(HARD_MAX.get() + 1);
    let table = [
        (
            "magic, first byte",
            with(&|h| h[0] = b'X'),
            total,
            PackError::BadMagic,
        ),
        (
            "magic, last byte",
            with(&|h| h[3] = b'1'),
            total,
            PackError::BadMagic,
        ),
        (
            "the magic of AZL2",
            with(&|h| h[..4].copy_from_slice(b"AZL2")),
            total,
            PackError::BadMagic,
        ),
        (
            "version 0",
            with(&|h| h[4] = 0),
            total,
            PackError::UnsupportedVersion(0),
        ),
        (
            "version 2",
            with(&|h| h[4] = 2),
            total,
            PackError::UnsupportedVersion(2),
        ),
        (
            "version 255",
            with(&|h| h[4] = 255),
            total,
            PackError::UnsupportedVersion(255),
        ),
        (
            "class 3",
            with(&|h| h[5] = 3),
            total,
            PackError::BadClass(3),
        ),
        (
            "class 4",
            with(&|h| h[5] = 4),
            total,
            PackError::BadClass(4),
        ),
        (
            "class 255",
            with(&|h| h[5] = 255),
            total,
            PackError::BadClass(255),
        ),
        (
            "reserved byte 6",
            with(&|h| h[6] = 1),
            total,
            PackError::ReservedNotZero,
        ),
        (
            "reserved byte 7",
            with(&|h| h[7] = 0x80),
            total,
            PackError::ReservedNotZero,
        ),
        (
            "epoch 0",
            with(&|h| h[8..12].fill(0)),
            total,
            PackError::BadEpoch,
        ),
        (
            "no chunks, a body",
            with(&|h| h[12..16].fill(0)),
            total,
            PackError::NoChunks,
        ),
        (
            "no chunks, no body",
            with(&|h| {
                h[12..16].fill(0);
                h[16..20].fill(0);
            }),
            total,
            PackError::NoChunks,
        ),
        (
            "more chunks than the cap",
            with(&|h| h[12..16].copy_from_slice(&(MAX_CHUNKS + 1).to_le_bytes())),
            total,
            PackError::TooManyChunks,
        ),
        (
            "chunk count 2^32 - 1",
            with(&|h| h[12..16].fill(0xff)),
            total,
            PackError::TooManyChunks,
        ),
        (
            "the body is the whole pack",
            with(&|h| h[16..20].copy_from_slice(&t.to_le_bytes())),
            total,
            PackError::SizesInconsistent,
        ),
        (
            "body length near 2^32",
            with(&|h| h[16..20].fill(0xff)),
            total,
            PackError::SizesInconsistent,
        ),
        (
            "padding length near 2^32",
            with(&|h| h[20..24].fill(0xff)),
            total,
            PackError::SizesInconsistent,
        ),
        (
            "a body that cannot hold two chunks",
            with(&|h| h[16..20].copy_from_slice(&81u32.to_le_bytes())),
            total,
            PackError::SizesInconsistent,
        ),
        (
            "a body too long for one chunk",
            with(&|h| {
                h[12..16].copy_from_slice(&1u32.to_le_bytes());
                h[16..20].copy_from_slice(&4_194_346u32.to_le_bytes());
            }),
            PackLen::new(4_194_346 + 24 + 100),
            PackError::SizesInconsistent,
        ),
        (
            "a trailer one byte too short",
            with(&|h| h[20..24].copy_from_slice(&1u32.to_le_bytes())),
            total,
            PackError::SizesInconsistent,
        ),
        (
            "a trailer too long for two lines",
            header,
            PackLen::new(t + 13),
            PackError::SizesInconsistent,
        ),
        (
            "a pack above the longest pack",
            header,
            big,
            PackError::TooLong,
        ),
        (
            "a version from the future, the rest garbage",
            garbage_after_version(2),
            total,
            PackError::UnsupportedVersion(2),
        ),
    ];
    let mut kinds = BTreeSet::new();
    for (name, bytes, total_len, want) in &table {
        assert_eq!(PackHeader::parse(bytes, *total_len), Err(*want), "{name}");
        kinds.insert(
            format!("{want:?}")
                .split('(')
                .next()
                .expect("a name")
                .to_string(),
        );
    }
    assert!(
        kinds.len() >= 9,
        "the table meets {} kinds of refusal: {kinds:?}",
        kinds.len()
    );

    // Less than a header.
    for len in [0usize, 1, 5, 23] {
        assert_eq!(
            PackHeader::parse(&header[..len], total),
            Err(PackError::TooShort),
            "{len} bytes"
        );
    }

    // The node's view of the same refusals: three facts named, everything else `SizesInconsistent`.
    assert_eq!(check_header(&header[..10], total), Err(NotSealed::TooShort));
    assert_eq!(
        check_header(&with(&|h| h[0] = 0), total),
        Err(NotSealed::BadMagic)
    );
    assert_eq!(
        check_header(&with(&|h| h[4] = 9), total),
        Err(NotSealed::UnsupportedVersion(9))
    );
    assert_eq!(
        check_header(&with(&|h| h[5] = 9), total),
        Err(NotSealed::BadClass(9))
    );
    let sizes: [fn(&mut [u8; 24]); 4] = [
        |h| h[6] = 1,
        |h| h[8..12].fill(0),
        |h| h[12..16].fill(0),
        |h| h[16..20].fill(0xff),
    ];
    for edit in sizes {
        assert_eq!(
            check_header(&with(&edit), total),
            Err(NotSealed::SizesInconsistent)
        );
    }
    assert_eq!(
        check_header(&header, big),
        Err(NotSealed::SizesInconsistent)
    );
    assert_eq!(check_header(&header, total), Ok(parsed));

    // A header made by hand is held to the same sizes by `trailer_range` and `body_range`.
    let mut by_hand = parsed;
    let range = by_hand.trailer_range(total).expect("a range");
    assert_eq!((range.start.get(), range.end.get()), (24 + 262, t));
    let body = by_hand.body_range().expect("a body");
    assert_eq!((body.start.get(), body.end.get()), (24, 24 + 262));
    by_hand.chunk_count = 0;
    assert_eq!(by_hand.trailer_range(total), Err(PackError::NoChunks));
    by_hand.chunk_count = 2;
    by_hand.body_len = PackLen::new(u32::MAX);
    assert_eq!(
        by_hand.trailer_range(total),
        Err(PackError::SizesInconsistent)
    );
    assert_eq!(by_hand.body_range(), Err(PackError::SizesInconsistent));
    by_hand.body_len = parsed.body_len;
    assert_eq!(
        by_hand.trailer_range(PackLen::new(t - 1)),
        Err(PackError::SizesInconsistent)
    );
}

#[test]
fn c8_name_id_and_storage_key() {
    let sp = space(S);
    let k = k_chunk(K);
    let chunks: Vec<(ChunkId, SealedChunk)> = (0..3u64)
        .map(|i| chunk(&sp, &k, 40 + i, 100 + 50 * i as usize))
        .collect();
    let pack = build(
        &sp,
        PackClass::Content,
        2,
        KS,
        vector_geometry(),
        Padding::Buckets,
        true,
        &chunks,
        8,
    );

    // The name is the BLAKE3 hash of the bytes, the id its first 16 bytes.
    let hash = blake3::hash(pack.bytes());
    assert_eq!(pack_name(pack.bytes()).as_bytes(), hash.as_bytes());
    assert_eq!(pack.name(), pack_name(pack.bytes()));
    assert_eq!(pack_id(&pack.name()).as_bytes()[..], hash.as_bytes()[..16]);
    assert_eq!(pack.id(), pack_id(&pack.name()));
    assert_eq!(pack_name(b"").as_bytes(), blake3::hash(b"").as_bytes());

    // The storage key, strings pinned: the first byte of the id is the directory.
    let id = PackId::from_bytes([
        0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
        0x89,
    ]);
    assert_eq!(
        storage_key(&SpaceId::from_bytes([0x11; 16]), &id),
        "s/11111111111111111111111111111111/p/ab/abcdef0123456789abcdef0123456789"
    );
    let low = PackId::from_bytes([0x05; 16]);
    assert_eq!(
        storage_key(&SpaceId::from_bytes([0xfe; 16]), &low),
        "s/fefefefefefefefefefefefefefefefe/p/05/05050505050505050505050505050505"
    );
    let key = storage_key(&sp, &pack.id());
    let id_hex = to_hex(&hash.as_bytes()[..16]);
    assert_eq!(
        key,
        format!("s/{}/p/{}/{}", to_hex(&[S; 16]), &id_hex[..2], id_hex)
    );
    // A pack sent twice lands on one key.
    assert_eq!(storage_key(&sp, &pack.clone().id()), key);

    // A different byte anywhere is a different name, and (with overwhelming probability) a different id.
    let mut rng = SplitMix64::new(8);
    for _ in 0..200 {
        let mut bytes = pack.bytes().to_vec();
        let at = rng.below(bytes.len() as u64) as usize;
        bytes[at] ^= 1 << rng.below(8);
        assert_ne!(pack_name(&bytes), pack.name(), "byte {at}");
        assert_ne!(pack_id(&pack_name(&bytes)), pack.id(), "byte {at}");
    }
    // One more byte, one less byte.
    let mut longer = pack.bytes().to_vec();
    longer.push(0);
    assert_ne!(pack_name(&longer), pack.name());
    assert_ne!(pack_name(&pack.bytes()[1..]), pack.name());
}
