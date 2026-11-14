//! C9 to C17: the AZL2 chunk's rules, one by one (planning/architecture.md section 5.7 "Chunk (AZL2)").
//!
//! C9 the synthetic nonce is keyed-BLAKE3 of the codec byte and the stored bytes; C10 the associated
//! data binds the space, the chunk and the codec, and any flipped bit is an authentication failure;
//! C11 ids differ between spaces and do not depend on the codec; C12 a member checks the content
//! hash; C13 decompression is bounded by the declared raw length; C14 the compression rule is 5 % and
//! unknown codecs are refused by name; C15 sealing is deterministic and takes no entropy; C16 the blob's
//! shape is strict; C17 the secrets are secrets.

mod azl2_common;

use azl2_common::{
    independent_seal, k_chunk, rle_encode, runs, space, stream, BombDouble, FailingDouble,
    RleDouble, SizedDouble, WrongLengthDouble,
};
use azlin_content::chunk::blob;
use azlin_content::limits::{BLOB_OVERHEAD, MAX_BLOB, MAX_CHUNK};
use azlin_content::{
    identity, open_chunk, seal_chunk, seal_stored, ChunkError, ChunkId, ChunkLen, ChunkRef, Codec,
    CodecError, CompressPolicy, Compressor, Full, OpenAs, PackId, PackOffset, SealedChunk,
    StoredOnly,
};
use azlin_keys::SpaceId;
use zeroize::Zeroizing;

/// Opens `blob` as a member of the space of `k`.
fn open_member(
    sp: &SpaceId,
    k: &azlin_content::KChunk,
    full: &Full,
    id: &ChunkId,
    blob: &[u8],
    raw_len: usize,
    c: &dyn Compressor,
) -> Result<Zeroizing<Vec<u8>>, ChunkError> {
    open_chunk(
        sp,
        full,
        id,
        blob,
        ChunkLen::new(raw_len as u32).expect("in range"),
        OpenAs::Member(k),
        c,
    )
}

#[test]
fn c9_nonce_is_keyed_blake3_of_codec_and_stored_bytes() {
    let k = k_chunk(1);
    let sp = space(2);
    let plain = runs(90, 1_000);
    let (full, id) = identity(&k, &plain);
    let stored0 = plain.clone();
    let stored1 = rle_encode(&plain);
    assert!(
        stored1.len() < stored0.len(),
        "the run-length stream is shorter"
    );
    let blob0 = seal_stored(&sp, &full, &id, Codec::Stored, &stored0).expect("seals");
    let blob1 = seal_stored(&sp, &full, &id, Codec::Zstd, &stored1).expect("seals");

    // An independent recomputation: blake3 calls only.
    let key = blake3::derive_key("azlin chunk key v1", full.expose());
    let nonce_of = |codec: u8, stored: &[u8]| {
        let mut hasher = blake3::Hasher::new_keyed(&key);
        hasher.update(&[codec]);
        hasher.update(stored);
        let hash = hasher.finalize();
        hash.as_bytes()[..24].to_vec()
    };
    let (nonce0, nonce1) = (nonce_of(0, &stored0), nonce_of(1, &stored1));
    assert_eq!(
        &blob0.as_bytes()[1..25],
        &nonce0[..],
        "the nonce of codec 0"
    );
    assert_eq!(
        &blob1.as_bytes()[1..25],
        &nonce1[..],
        "the nonce of codec 1"
    );

    // The same plaintext and the same chunk key under two codecs: two nonces, so no keystream is reused.
    assert_ne!(nonce0, nonce1);
    let ct0 = &blob0.as_bytes()[25..25 + stored0.len()];
    let ct1 = &blob1.as_bytes()[25..25 + stored1.len()];
    let n = ct0.len().min(ct1.len());
    // With one keystream, ct0 ^ ct1 would be stored0 ^ stored1 at every byte; with two it is so at
    // about one byte in 256.
    let same = (0..n)
        .filter(|&i| ct0[i] ^ ct1[i] == stored0[i] ^ stored1[i])
        .count();
    assert!(same * 20 < n, "{same} of {n} bytes show a reused keystream");

    // Deterministic, and every input moves the nonce: a bit of the stored bytes, the codec byte, the key.
    let again = seal_stored(&sp, &full, &id, Codec::Stored, &stored0).expect("seals");
    assert_eq!(
        again.as_bytes(),
        blob0.as_bytes(),
        "the same inputs, the same blob"
    );
    let mut other_stored = stored0.clone();
    other_stored[500] ^= 0x10;
    let moved = seal_stored(&sp, &full, &id, Codec::Stored, &other_stored).expect("seals");
    assert_ne!(
        &moved.as_bytes()[1..25],
        &blob0.as_bytes()[1..25],
        "a stored bit moves the nonce"
    );
    let as_brotli = seal_stored(&sp, &full, &id, Codec::Brotli, &stored0).expect("seals");
    assert_ne!(
        &as_brotli.as_bytes()[1..25],
        &blob0.as_bytes()[1..25],
        "the codec moves the nonce"
    );
    let (full_b, id_b) = identity(&k_chunk(9), &plain);
    let other_key = seal_stored(&sp, &full_b, &id_b, Codec::Stored, &stored0).expect("seals");
    assert_ne!(
        &other_key.as_bytes()[1..25],
        &blob0.as_bytes()[1..25],
        "the key moves the nonce"
    );

    // The library agrees with the independent chunk on the whole blob.
    let independent = independent_seal(&[1; 32], &[2; 16], &plain, 0, &stored0, b"AZL2");
    assert_eq!(blob0.as_bytes(), &independent.blob[..]);
}

#[test]
fn c10_aad_binds_space_chunk_and_codec() {
    let k = k_chunk(3);
    let sp = space(4);
    let plain = stream(100, 259); // a 300-byte blob: 259 + 41
    let (full, id) = identity(&k, &plain);
    let blob = seal_stored(&sp, &full, &id, Codec::Stored, &plain).expect("seals");
    let bytes = blob.as_bytes().to_vec();
    assert_eq!(bytes.len(), 300);
    let open = |sp: &SpaceId, full: &Full, id: &ChunkId, blob: &[u8]| {
        open_member(sp, &k, full, id, blob, 259, &StoredOnly)
    };
    assert!(open(&sp, &full, &id, &bytes).is_ok());

    // Another space, another chunk id, another key: all authentication failures.
    assert_eq!(
        open(&space(5), &full, &id, &bytes).unwrap_err(),
        ChunkError::Auth
    );
    assert_eq!(
        open(&sp, &full, &ChunkId::from_bytes([0xAB; 16]), &bytes).unwrap_err(),
        ChunkError::Auth
    );
    let (other_full, other_id) = identity(&k, b"another chunk");
    assert_eq!(
        open(&sp, &other_full, &other_id, &bytes).unwrap_err(),
        ChunkError::Auth
    );
    assert_eq!(
        open(&sp, &other_full, &id, &bytes).unwrap_err(),
        ChunkError::Auth
    );

    // A flipped codec byte in the header: another known codec, the reserved one, an unknown one. The
    // associated data names the codec, so it is `Auth` and never `BadCodec` before authentication.
    for codec_byte in [1u8, 2, 3, 4, 5, 0x80, 255] {
        let mut flipped = bytes.clone();
        flipped[0] = codec_byte;
        assert_eq!(
            open(&sp, &full, &id, &flipped).unwrap_err(),
            ChunkError::Auth,
            "a codec byte of {codec_byte}"
        );
    }

    // Exhaustive: every single bit of the 300 bytes. Any of them is an authentication failure.
    for byte in 0..bytes.len() {
        for bit in 0..8 {
            let mut flipped = bytes.clone();
            flipped[byte] ^= 1 << bit;
            assert_eq!(
                open(&sp, &full, &id, &flipped).unwrap_err(),
                ChunkError::Auth,
                "bit {bit} of byte {byte}"
            );
        }
    }
    // The same blob opens for a share recipient, who holds `full` and nothing else; the associated
    // data binds her opening to the space and chunk too.
    let as_share = |sp: &SpaceId, id: &ChunkId| {
        open_chunk(
            sp,
            &full,
            id,
            &bytes,
            ChunkLen::new(259).expect("in range"),
            OpenAs::Share,
            &StoredOnly,
        )
    };
    assert!(as_share(&sp, &id).is_ok());
    assert_eq!(as_share(&space(5), &id).unwrap_err(), ChunkError::Auth);
}

#[test]
fn c11_ids_differ_between_spaces_and_not_by_codec() {
    let plain = runs(110, 4_000);
    let (full_a, id_a) = identity(&k_chunk(1), &plain);
    let (full_b, id_b) = identity(&k_chunk(2), &plain);
    // Two K_chunk: different hashes, ids and keys.
    assert!(full_a != full_b);
    assert_ne!(id_a, id_b);
    let key_a = blake3::derive_key("azlin chunk key v1", full_a.expose());
    let key_b = blake3::derive_key("azlin chunk key v1", full_b.expose());
    assert_ne!(key_a, key_b);

    // Under one K_chunk the id and the key do not depend on the codec: the chunk the pipeline records
    // is the same chunk whichever codec the time budget chose (convergence inside one space).
    let (meta_stored, _) = seal_chunk(
        &space(7),
        &k_chunk(1),
        &plain,
        &StoredOnly,
        CompressPolicy::plan(),
    )
    .expect("seals");
    let (meta_rle, _) = seal_chunk(
        &space(7),
        &k_chunk(1),
        &plain,
        &RleDouble,
        CompressPolicy::plan(),
    )
    .expect("seals");
    assert_eq!(meta_stored.codec, Codec::Stored);
    assert_eq!(meta_rle.codec, Codec::Zstd);
    assert!(
        meta_stored.full == meta_rle.full,
        "the hash does not depend on the codec"
    );
    assert_eq!(
        meta_stored.id, meta_rle.id,
        "the id does not depend on the codec"
    );
    assert!(meta_stored.full == full_a && meta_stored.id == id_a);

    // The id and the key come from separate contexts: the id says nothing of the key.
    assert_ne!(
        id_a.as_bytes()[..],
        key_a[..16],
        "the id is not a prefix of the key"
    );
    assert_ne!(
        blake3::derive_key("azlin chunk id v1", full_a.expose()),
        key_a,
        "the two contexts give two different values"
    );
}

#[test]
fn c12_member_open_checks_the_content_hash() {
    let k = k_chunk(6);
    let sp = space(8);
    let p = stream(120, 500);
    let q = stream(121, 500); // the same length, other content
    let (full, id) = identity(&k, &p);
    let raw = ChunkLen::new(500).expect("in range");

    // A blob that authenticates (made under the chunk key of `full`) but holds Q, not P.
    let blob = seal_stored(&sp, &full, &id, Codec::Stored, &q).expect("seals");
    assert_eq!(
        open_chunk(
            &sp,
            &full,
            &id,
            blob.as_bytes(),
            raw,
            OpenAs::Member(&k),
            &StoredOnly
        )
        .unwrap_err(),
        ChunkError::ContentMismatch
    );
    // A share recipient has no K_chunk: the blob opens; the file-level BLAKE3 of the pointer is the
    // check there (documented in `OpenAs`).
    let opened = open_chunk(
        &sp,
        &full,
        &id,
        blob.as_bytes(),
        raw,
        OpenAs::Share,
        &StoredOnly,
    )
    .expect("a share recipient opens it");
    assert!(opened.as_slice() == q.as_slice());
    // The honest blob opens for both.
    let honest = seal_stored(&sp, &full, &id, Codec::Stored, &p).expect("seals");
    for who in [OpenAs::Member(&k), OpenAs::Share] {
        let opened =
            open_chunk(&sp, &full, &id, honest.as_bytes(), raw, who, &StoredOnly).expect("opens");
        assert!(opened.as_slice() == p.as_slice());
    }

    // A wrong id is refused for both: sealed under the wrong id (so it authenticates with it), it is not
    // the id of `full`; opened under the right id it is an authentication failure.
    let wrong = ChunkId::from_bytes([0xEE; 16]);
    let under_wrong = seal_stored(&sp, &full, &wrong, Codec::Stored, &p).expect("seals");
    for who in [OpenAs::Member(&k), OpenAs::Share] {
        assert_eq!(
            open_chunk(
                &sp,
                &full,
                &wrong,
                under_wrong.as_bytes(),
                raw,
                who,
                &StoredOnly
            )
            .unwrap_err(),
            ChunkError::IdMismatch
        );
        assert_eq!(
            open_chunk(
                &sp,
                &full,
                &id,
                under_wrong.as_bytes(),
                raw,
                who,
                &StoredOnly
            )
            .unwrap_err(),
            ChunkError::Auth
        );
    }
}

#[test]
fn c13_decompression_is_bounded_by_the_declared_raw_length() {
    let k = k_chunk(7);
    let sp = space(9);
    let p = stream(130, 1_000);
    let (full, id) = identity(&k, &p);
    // An authentic blob of codec 1 with five stored bytes: the compressor is asked for them.
    let blob = seal_stored(&sp, &full, &id, Codec::Zstd, &[1, 2, 3, 4, 5]).expect("seals");
    let raw = ChunkLen::new(1_000).expect("in range");
    let open = |c: &dyn Compressor, who: OpenAs<'_>| {
        open_chunk(&sp, &full, &id, blob.as_bytes(), raw, who, c)
    };

    // A compressor that would produce 1 GiB is stopped at the declared raw length: it is told the
    // length, and it produced no more than one byte past it.
    let bomb = BombDouble::default();
    assert_eq!(
        open(&bomb, OpenAs::Member(&k)).unwrap_err(),
        ChunkError::Codec(CodecError::TooLong)
    );
    assert_eq!(
        bomb.max_len_seen.get(),
        1_000,
        "the compressor is told the pointer's raw length"
    );
    assert!(
        bomb.produced.get() <= 1_001,
        "it produced {} bytes",
        bomb.produced.get()
    );

    // A stream shorter or longer than declared, from a compressor that does not obey the seam.
    assert_eq!(
        open(&WrongLengthDouble(-1), OpenAs::Share).unwrap_err(),
        ChunkError::LengthMismatch
    );
    assert_eq!(
        open(&WrongLengthDouble(1), OpenAs::Share).unwrap_err(),
        ChunkError::LengthMismatch
    );
    let exact = open(&WrongLengthDouble(0), OpenAs::Share).expect("the declared length");
    assert_eq!(exact.len(), 1_000);
    assert_eq!(
        open(&WrongLengthDouble(0), OpenAs::Member(&k)).unwrap_err(),
        ChunkError::ContentMismatch,
        "the right length is not the right content"
    );
    // A compressor that fails.
    assert_eq!(
        open(&FailingDouble, OpenAs::Member(&k)).unwrap_err(),
        ChunkError::Codec(CodecError::Damaged)
    );

    // A zero raw length cannot be declared at all; one past the largest chunk is refused; stored
    // bytes longer than the declared length are refused before the compressor is called or anything
    // is allocated for them.
    assert!(ChunkLen::new(0).is_none());
    let too_big = ChunkLen::new(MAX_CHUNK.get() + 1).expect("below the largest blob");
    assert_eq!(
        open_chunk(
            &sp,
            &full,
            &id,
            blob.as_bytes(),
            too_big,
            OpenAs::Share,
            &StoredOnly
        )
        .unwrap_err(),
        ChunkError::TooLong
    );
    let bomb = BombDouble::default();
    let four = ChunkLen::new(4).expect("in range");
    assert_eq!(
        open_chunk(&sp, &full, &id, blob.as_bytes(), four, OpenAs::Share, &bomb).unwrap_err(),
        ChunkError::LengthMismatch
    );
    assert_eq!(
        bomb.max_len_seen.get(),
        0,
        "the compressor was never called"
    );

    // A stored chunk allocates what it says: the buffer is as long as the declared length.
    let honest = seal_stored(&sp, &full, &id, Codec::Stored, &p).expect("seals");
    let opened = open_chunk(
        &sp,
        &full,
        &id,
        honest.as_bytes(),
        raw,
        OpenAs::Member(&k),
        &StoredOnly,
    )
    .expect("opens");
    assert_eq!(opened.len(), 1_000);
    assert!(opened.capacity() <= 1_000, "capacity {}", opened.capacity());
}

#[test]
fn c14_compression_rule_is_five_percent() {
    let k = k_chunk(8);
    let sp = space(10);
    let p = stream(140, 1_000);
    // Stored bytes at 94.9 %, 95.0 % and 95.1 % of the plaintext: codec 1, 1, 0.
    for (stream_len, want) in [(949, Codec::Zstd), (950, Codec::Zstd), (951, Codec::Stored)] {
        let (meta, blob) = seal_chunk(
            &sp,
            &k,
            &p,
            &SizedDouble(stream_len),
            CompressPolicy::plan(),
        )
        .expect("seals");
        assert_eq!(meta.codec, want, "a stream of {stream_len} bytes of 1000");
        let stored = if want == Codec::Stored {
            1_000
        } else {
            stream_len
        };
        assert_eq!(blob.as_bytes().len(), BLOB_OVERHEAD + stored);
        assert_eq!(blob.as_bytes()[0], want.byte());
    }
    // The boundary on a short chunk: 20 bytes, 19 saves 5 %, 20 does not.
    let short = stream(141, 20);
    for (stream_len, want) in [(19, Codec::Zstd), (20, Codec::Stored)] {
        let (meta, _) = seal_chunk(
            &sp,
            &k,
            &short,
            &SizedDouble(stream_len),
            CompressPolicy::plan(),
        )
        .expect("seals");
        assert_eq!(meta.codec, want, "a stream of {stream_len} bytes of 20");
    }
    // An incompressible chunk is stored as it is (the run-length double only grows random bytes).
    let (meta, _) = seal_chunk(&sp, &k, &p, &RleDouble, CompressPolicy::plan()).expect("seals");
    assert_eq!(meta.codec, Codec::Stored);
    // A compressor that answers "stored" with bytes is ignored; an empty stream is damaged; a failing
    // compressor stops the seal.
    struct SaysStored;
    impl Compressor for SaysStored {
        fn compress(
            &self,
            _plaintext: &[u8],
        ) -> Result<Option<azlin_content::Compressed>, CodecError> {
            Ok(Some((Codec::Stored, Zeroizing::new(vec![1, 2, 3]))))
        }
        fn decompress(
            &self,
            codec: Codec,
            _stored: &[u8],
            _max_len: ChunkLen,
        ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
            Err(CodecError::Unsupported(codec.byte()))
        }
    }
    let (meta, _) = seal_chunk(&sp, &k, &p, &SaysStored, CompressPolicy::plan()).expect("seals");
    assert_eq!(meta.codec, Codec::Stored);
    assert_eq!(
        seal_chunk(&sp, &k, &p, &SizedDouble(0), CompressPolicy::plan()).unwrap_err(),
        ChunkError::Codec(CodecError::Damaged)
    );
    assert_eq!(
        seal_chunk(&sp, &k, &p, &FailingDouble, CompressPolicy::plan()).unwrap_err(),
        ChunkError::Codec(CodecError::Failed)
    );

    // An unknown codec byte is refused by name: `Codec::from_byte`, and an authentic blob that carries
    // one (made here independently) once it has authenticated.
    for byte in [3u8, 5, 255] {
        assert_eq!(Codec::from_byte(byte), Err(CodecError::Unsupported(byte)));
        let sealed = independent_seal(&[8; 32], &[10; 16], &p, byte, &p, b"AZL2");
        let (full, id) = identity(&k, &p);
        assert_eq!(
            open_chunk(
                &sp,
                &full,
                &id,
                &sealed.blob,
                ChunkLen::new(1_000).expect("in range"),
                OpenAs::Share,
                &StoredOnly
            )
            .unwrap_err(),
            ChunkError::BadCodec(byte),
            "codec byte {byte}"
        );
    }
}

#[test]
fn c15_seal_is_deterministic_and_draws_no_entropy() {
    let k = k_chunk(11);
    let sp = space(12);
    let p = runs(150, 5_000);
    let first = seal_chunk(&sp, &k, &p, &RleDouble, CompressPolicy::plan()).expect("seals");
    let second = seal_chunk(&sp, &k, &p, &RleDouble, CompressPolicy::plan()).expect("seals");
    assert_eq!(
        first.1.as_bytes(),
        second.1.as_bytes(),
        "two runs, the same bytes"
    );
    assert!(first.0 == second.0);
    // The signature has no entropy parameter: a doctest in `chunk.rs` pins the function type. Here:
    // the space id is the only other input that moves the blob, and it moves only the tag (the
    // synthetic nonce and the ciphertext do not depend on the space, the tag authenticates it).
    let other_space =
        seal_chunk(&space(13), &k, &p, &RleDouble, CompressPolicy::plan()).expect("seals");
    let (a, b) = (first.1.as_bytes(), other_space.1.as_bytes());
    assert_eq!(a.len(), b.len());
    assert_eq!(
        a[..a.len() - 16],
        b[..b.len() - 16],
        "the same nonce and ciphertext"
    );
    assert_ne!(
        a[a.len() - 16..],
        b[b.len() - 16..],
        "another space, another tag"
    );
}

#[test]
fn c16_sealed_blob_shape_is_strict() {
    // The layout constants, exported and pinned.
    assert_eq!(blob::CODEC_LEN, 1);
    assert_eq!(blob::NONCE_LEN, 24);
    assert_eq!(blob::TAG_LEN, 16);
    assert_eq!(blob::OVERHEAD, 41);
    assert_eq!(
        blob::CODEC_LEN + blob::NONCE_LEN + blob::TAG_LEN,
        blob::OVERHEAD
    );

    let k = k_chunk(14);
    let sp = space(15);
    let p = stream(160, 100);
    let (full, id) = identity(&k, &p);
    let raw = ChunkLen::new(100).expect("in range");
    // Shorter than the framing (1 + 24 + 16): truncated, whatever the bytes.
    for len in 0..BLOB_OVERHEAD {
        let bytes = stream(1_600 + len as u64, len);
        assert_eq!(
            open_chunk(&sp, &full, &id, &bytes, raw, OpenAs::Share, &StoredOnly).unwrap_err(),
            ChunkError::Truncated,
            "a blob of {len} bytes"
        );
    }
    // The framing alone has the shape and no tag that verifies.
    let framing = stream(1_641, BLOB_OVERHEAD);
    assert_eq!(
        open_chunk(&sp, &full, &id, &framing, raw, OpenAs::Share, &StoredOnly).unwrap_err(),
        ChunkError::Auth
    );
    // The stored length of the description is the blob's length, for chunks of every size.
    for len in [1usize, 2, 100, 255, 4_096, 70_000] {
        let plain = stream(170 + len as u64, len);
        let (meta, blob) =
            seal_chunk(&sp, &k, &plain, &StoredOnly, CompressPolicy::plan()).expect("seals");
        assert_eq!(meta.stored_len.as_usize(), blob.as_bytes().len());
        assert_eq!(meta.stored_len.as_usize(), len + BLOB_OVERHEAD);
        assert_eq!(blob.len().as_usize(), blob.as_bytes().len());
        assert_eq!(blob.stored_len(), meta.stored_len);
        // A pointer entry whose stored length is not the blob's is not a chunk of that plaintext.
        let reference = meta
            .duplicate()
            .into_ref(PackId::from_bytes([1; 16]), PackOffset::new(24));
        assert_eq!(reference.check(), Ok(()));
        let wrong = ChunkRef {
            stored_len: ChunkLen::new(len as u32 + BLOB_OVERHEAD as u32 + 1).expect("in range"),
            ..reference.duplicate()
        };
        assert_eq!(wrong.check(), Err(ChunkError::LengthMismatch));
    }
    // Longer than the largest blob: too long, before anything is read; the largest blob is fine in shape.
    let huge = vec![0u8; MAX_BLOB.as_usize() + 1];
    assert_eq!(
        open_chunk(
            &sp,
            &full,
            &id,
            &huge,
            ChunkLen::new(100).expect("in range"),
            OpenAs::Share,
            &StoredOnly
        )
        .unwrap_err(),
        ChunkError::TooLong
    );
    assert_eq!(
        SealedChunk::from_stored(&huge).unwrap_err(),
        ChunkError::TooLong
    );
}

#[test]
fn c17_secrets_are_secrets() {
    let k = k_chunk(16);
    let sp = space(17);
    let p = stream(180, 300);
    let (meta, blob) = seal_chunk(&sp, &k, &p, &StoredOnly, CompressPolicy::plan()).expect("seals");
    // Nothing that holds `full` or `K_chunk` prints it (the types have no `Clone`, `Display` or `Default`:
    // compile_fail doctests in `ids.rs` and `chunk.rs`).
    let printed = format!("{meta:?} {k:?} {:?}", meta.full);
    assert!(
        printed.contains("Full(***)") && printed.contains("KChunk(***)"),
        "{printed}"
    );
    for needle in [
        azl2_common::to_hex(meta.full.expose()),
        azl2_common::to_hex(&[16u8; 32]),
        format!("{:?}", meta.full.expose()),
    ] {
        assert!(!printed.contains(&needle), "a secret is printed: {needle}");
    }
    // The plaintext comes back in a wiped-on-drop buffer: the type is `Zeroizing<Vec<u8>>` (a doctest
    // in `chunk.rs` pins the function type; here a value of that type).
    let opened: Zeroizing<Vec<u8>> = open_chunk(
        &sp,
        &meta.full,
        &meta.id,
        blob.as_bytes(),
        meta.raw_len,
        OpenAs::Member(&k),
        &StoredOnly,
    )
    .expect("opens");
    assert!(opened.as_slice() == p.as_slice());
    // `duplicate` is the one copy: equal, and a different hash is not equal.
    assert!(meta.full.duplicate() == meta.full);
    assert!(Full::from_bytes([0; 32]) != meta.full);
    // Compressor doubles hand back `Zeroizing` buffers too (the trait says so).
    let compressed = RleDouble
        .compress(&[7; 10])
        .expect("compresses")
        .expect("a stream");
    let _: Zeroizing<Vec<u8>> = compressed.1;
}
