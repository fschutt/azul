//! C8: the known answers of the AZL2 chunk (planning/architecture.md section 5.7 "Chunk (AZL2)"),
//! pinned in `tests/vectors/azl2_chunk.json`.
//!
//! Every positive vector gives a fixed `K_chunk`, a space id and a plaintext (a seed and a length of
//! a seeded stream: the file stays small), the codec, and the `full` hash, the chunk id, the chunk key
//! (as hex), the synthetic nonce and the whole blob (as hex when it is short, else its length and its
//! BLAKE3 hash). The sizes are 1, 255, 256 KiB, 1 MiB and 4 MiB; the codecs 0, 1, 2 and 4 (codec 1
//! chosen by the 5 % rule through a run-length double, and forced through `seal_stored`).
//!
//! The file is written only under `UPDATE_VECTORS=1`, never when `CI` is set. Its values are the ones of
//! an INDEPENDENT recomputation in this test (`independent_seal`: blake3 and chacha20poly1305 calls
//! only), and the lane's own functions are held to them: `seal_chunk` and `seal_stored` make the same
//! bytes, and `open_chunk` opens every blob back to its plaintext, as a member and as a share
//! recipient. The negative vectors give a blob and the refusal it gets.

mod azl2_common;

use azl2_common::{
    check_vector_file, field, from_hex, independent_seal, k_chunk, number, read_rows, render_rows,
    rle_encode, runs, space, stream, to_hex, IdentityDouble, RleDouble, Sealed,
};
use azlin_content::{
    identity, open_chunk, seal_chunk, seal_stored, ChunkLen, ChunkMeta, Codec, CompressPolicy,
    Compressor, OpenAs, StoredOnly,
};
use azlin_keys::SpaceId;

const K: u8 = 0x31;
const SPACE: u8 = 0x52;

/// How a positive vector's blob is made.
#[derive(Clone, Copy)]
enum How {
    /// `seal_chunk` with `StoredOnly`: codec 0.
    ChunkStored,
    /// `seal_chunk` with the run-length double: codec 1 because it saves more than 5 %.
    ChunkRle,
    /// `seal_stored` with this codec byte and the plaintext as the stored bytes.
    Forced(u8),
}

impl How {
    fn word(self) -> &'static str {
        match self {
            How::ChunkStored => "chunk_stored",
            How::ChunkRle => "chunk_rle",
            How::Forced(_) => "forced",
        }
    }

    fn codec_byte(self) -> u8 {
        match self {
            How::ChunkStored => 0,
            How::ChunkRle => 1,
            How::Forced(codec) => codec,
        }
    }
}

/// `(name, generator, seed, length, how)`.
fn cases() -> Vec<(&'static str, &'static str, u64, usize, How)> {
    vec![
        ("codec0_size1", "random", 11, 1, How::ChunkStored),
        ("codec0_size255", "random", 12, 255, How::ChunkStored),
        ("codec0_size262144", "random", 13, 262_144, How::ChunkStored),
        (
            "codec0_size1048576",
            "random",
            14,
            1_048_576,
            How::ChunkStored,
        ),
        (
            "codec0_size4194304",
            "random",
            15,
            4_194_304,
            How::ChunkStored,
        ),
        ("codec1_forced_size1", "random", 21, 1, How::Forced(1)),
        ("codec1_forced_size255", "random", 22, 255, How::Forced(1)),
        (
            "codec1_policy_size262144",
            "runs",
            23,
            262_144,
            How::ChunkRle,
        ),
        (
            "codec1_policy_size1048576",
            "runs",
            24,
            1_048_576,
            How::ChunkRle,
        ),
        (
            "codec1_policy_size4194304",
            "runs",
            25,
            4_194_304,
            How::ChunkRle,
        ),
        ("codec2_forced_size255", "random", 31, 255, How::Forced(2)),
        ("codec4_forced_size100", "random", 41, 100, How::Forced(4)),
    ]
}

fn plaintext(generator: &str, seed: u64, len: usize) -> Vec<u8> {
    match generator {
        "random" => stream(seed, len),
        "runs" => runs(seed, len),
        other => panic!("unknown generator {other}"),
    }
}

/// The stored bytes of a vector, by the vector's own rules (not the library's).
fn stored_of(how: How, plaintext: &[u8]) -> Vec<u8> {
    match how {
        How::ChunkRle => rle_encode(plaintext),
        How::ChunkStored | How::Forced(_) => plaintext.to_vec(),
    }
}

/// The description and the blob the library makes of a case.
fn library_blob(how: How, plaintext: &[u8]) -> (ChunkMeta, Vec<u8>) {
    let (k, sp) = (k_chunk(K), space(SPACE));
    match how {
        How::ChunkStored => {
            let (meta, blob) =
                seal_chunk(&sp, &k, plaintext, &StoredOnly, CompressPolicy::plan()).expect("seals");
            (meta, blob.into_bytes())
        }
        How::ChunkRle => {
            let (meta, blob) =
                seal_chunk(&sp, &k, plaintext, &RleDouble, CompressPolicy::plan()).expect("seals");
            (meta, blob.into_bytes())
        }
        How::Forced(codec_byte) => {
            let (full, id) = identity(&k, plaintext);
            let codec = Codec::from_byte(codec_byte).expect("a codec");
            let blob = seal_stored(&sp, &full, &id, codec, plaintext).expect("seals");
            let meta = ChunkMeta {
                full,
                id,
                raw_len: ChunkLen::from_len(plaintext.len()).expect("in range"),
                stored_len: blob.stored_len(),
                codec,
            };
            (meta, blob.into_bytes())
        }
    }
}

fn positive_row(
    name: &str,
    generator: &str,
    seed: u64,
    len: usize,
    how: How,
    sealed: &Sealed,
) -> String {
    let blob_hex = if sealed.blob.len() <= 600 {
        to_hex(&sealed.blob)
    } else {
        String::new()
    };
    format!(
        "{{\"name\": \"{name}\", \"valid\": true, \"k_chunk\": \"{}\", \"space\": \"{}\", \"gen\": \"{generator}\", \"seed\": {seed}, \"len\": {len}, \"how\": \"{}\", \"codec\": {}, \"full\": \"{}\", \"id\": \"{}\", \"key\": \"{}\", \"nonce\": \"{}\", \"blob_len\": {}, \"blob_blake3\": \"{}\", \"hex\": \"{blob_hex}\"}}",
        to_hex(&[K; 32]),
        to_hex(&[SPACE; 16]),
        how.word(),
        how.codec_byte(),
        to_hex(&sealed.full),
        to_hex(&sealed.id),
        to_hex(&sealed.key),
        to_hex(&sealed.nonce),
        sealed.blob.len(),
        blake3::hash(&sealed.blob).to_hex(),
    )
}

const NEGATIVE_PLAINTEXT: &[u8] = b"the plaintext of the negative vectors of the AZL2 chunk";

/// A negative vector: a blob and the refusal it gets when opened in `open_space` with this
/// declared raw length.
fn negative_row(name: &str, blob: &[u8], raw_len: usize, open_space: u8, expect: &str) -> String {
    format!(
        "{{\"name\": \"{name}\", \"valid\": false, \"k_chunk\": \"{}\", \"space\": \"{}\", \"open_space\": \"{}\", \"plaintext\": \"{}\", \"raw_len\": {raw_len}, \"hex\": \"{}\", \"expect\": \"{expect}\"}}",
        to_hex(&[K; 32]),
        to_hex(&[SPACE; 16]),
        to_hex(&[open_space; 16]),
        to_hex(NEGATIVE_PLAINTEXT),
        to_hex(blob),
    )
}

fn negative_rows() -> Vec<String> {
    let k = [K; 32];
    let sp = [SPACE; 16];
    let len = NEGATIVE_PLAINTEXT.len();
    let valid = independent_seal(&k, &sp, NEGATIVE_PLAINTEXT, 0, NEGATIVE_PLAINTEXT, b"AZL2");
    let mut flipped_tag = valid.blob.clone();
    let last = flipped_tag.len() - 1;
    flipped_tag[last] ^= 1;
    // Authentic blobs that say something else: codec 3 (reserved), and the old format's tag.
    let codec3 = independent_seal(&k, &sp, NEGATIVE_PLAINTEXT, 3, NEGATIVE_PLAINTEXT, b"AZL2");
    let old_magic = independent_seal(&k, &sp, NEGATIVE_PLAINTEXT, 0, NEGATIVE_PLAINTEXT, b"AZL1");
    vec![
        negative_row(
            "neg_truncated_blob",
            &valid.blob[..40],
            len,
            SPACE,
            "Truncated",
        ),
        negative_row("neg_flipped_tag_bit", &flipped_tag, len, SPACE, "Auth"),
        negative_row(
            "neg_opened_in_another_space",
            &valid.blob,
            len,
            0x53,
            "Auth",
        ),
        negative_row(
            "neg_authentic_blob_with_reserved_codec_3",
            &codec3.blob,
            len,
            SPACE,
            "BadCodec(3)",
        ),
        negative_row(
            "neg_blob_of_the_old_format_AZL1",
            &old_magic.blob,
            len,
            SPACE,
            "Auth",
        ),
        negative_row(
            "neg_stored_longer_than_the_declared_length",
            &valid.blob,
            len - 1,
            SPACE,
            "LengthMismatch",
        ),
    ]
}

#[test]
fn c8_azl2_known_answers_pinned() {
    // The table: for every case the independent recomputation, the library's blob (it must be the
    // same bytes) and the two openers.
    let (k_bytes, sp_bytes) = ([K; 32], [SPACE; 16]);
    let (k, sp) = (k_chunk(K), space(SPACE));
    let mut rows = Vec::new();
    for (name, generator, seed, len, how) in cases() {
        let plain = plaintext(generator, seed, len);
        let stored = stored_of(how, &plain);
        let sealed = independent_seal(
            &k_bytes,
            &sp_bytes,
            &plain,
            how.codec_byte(),
            &stored,
            b"AZL2",
        );

        let (meta, blob) = library_blob(how, &plain);
        assert!(
            blob == sealed.blob,
            "{name}: the library's blob is not the independent one"
        );
        assert_eq!(meta.codec.byte(), how.codec_byte(), "{name}: the codec");
        assert_eq!(meta.full.expose(), &sealed.full, "{name}: full");
        assert_eq!(meta.id.as_bytes(), &sealed.id, "{name}: id");
        assert_eq!(meta.raw_len.as_usize(), len, "{name}: raw_len");
        assert_eq!(
            meta.stored_len.as_usize(),
            blob.len(),
            "{name}: stored_len is the blob's length"
        );
        assert_eq!(
            blob[0],
            how.codec_byte(),
            "{name}: the codec byte opens the blob"
        );
        assert_eq!(
            blob[1..25],
            sealed.nonce,
            "{name}: the nonce follows the codec byte"
        );

        let double: &dyn Compressor = match how {
            How::ChunkRle => &RleDouble,
            _ => &IdentityDouble,
        };
        for who in [OpenAs::Member(&k), OpenAs::Share] {
            let opened = open_chunk(&sp, &meta.full, &meta.id, &blob, meta.raw_len, who, double)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(
                opened.as_slice() == plain.as_slice(),
                "{name}: the opened plaintext"
            );
        }
        rows.push(positive_row(name, generator, seed, len, how, &sealed));
    }
    rows.extend(negative_rows());
    check_vector_file("azl2_chunk", &render_rows(&rows));

    // The file: at least 8 positive vectors with codecs 0 and 1 and the five sizes, at least three
    // negative ones; the short blobs of the file open, and every negative one is refused as it says.
    let file = read_rows("azl2_chunk");
    let positives: Vec<&String> = file
        .iter()
        .filter(|r| r.contains("\"valid\": true"))
        .collect();
    let negatives: Vec<&String> = file
        .iter()
        .filter(|r| r.contains("\"valid\": false"))
        .collect();
    assert!(positives.len() >= 8, "{} positive vectors", positives.len());
    assert!(negatives.len() >= 3, "{} negative vectors", negatives.len());
    for codec in [0u64, 1] {
        assert!(
            positives.iter().any(|r| number(r, "codec") == codec),
            "a vector with codec {codec}"
        );
    }
    for size in [1u64, 255, 262_144, 1_048_576, 4_194_304] {
        assert!(
            positives.iter().any(|r| number(r, "len") == size),
            "a vector of size {size}"
        );
    }

    for row in &positives {
        let name = field(row, "name");
        let blob_hex = field(row, "hex");
        if blob_hex.is_empty() {
            continue; // a long blob: its length and hash were compared with the independent ones
        }
        let blob = from_hex(&blob_hex);
        let plain = plaintext(
            &field(row, "gen"),
            number(row, "seed"),
            number(row, "len") as usize,
        );
        let (full, id) = identity(&k, &plain);
        assert_eq!(field(row, "full"), to_hex(full.expose()), "{name}: full");
        assert_eq!(field(row, "id"), to_hex(id.as_bytes()), "{name}: id");
        let raw_len = ChunkLen::from_len(plain.len()).expect("in range");
        let opened = open_chunk(
            &sp,
            &full,
            &id,
            &blob,
            raw_len,
            OpenAs::Member(&k),
            &IdentityDouble,
        )
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(opened.as_slice() == plain.as_slice(), "{name}");
        assert_eq!(blob[0], number(row, "codec") as u8, "{name}");
        assert_eq!(field(row, "nonce"), to_hex(&blob[1..25]), "{name}");
    }

    for row in &negatives {
        let name = field(row, "name");
        let k = k_chunk(from_hex(&field(row, "k_chunk"))[0]);
        let plain = from_hex(&field(row, "plaintext"));
        let (full, id) = identity(&k, &plain);
        let open_space = SpaceId::from_bytes(
            from_hex(&field(row, "open_space"))
                .try_into()
                .expect("16 bytes"),
        );
        let raw_len = ChunkLen::new(number(row, "raw_len") as u32).expect("in range");
        let blob = from_hex(&field(row, "hex"));
        for who in [OpenAs::Member(&k), OpenAs::Share] {
            let refusal = open_chunk(
                &open_space,
                &full,
                &id,
                &blob,
                raw_len,
                who,
                &IdentityDouble,
            )
            .expect_err(&format!("{name}: must be refused"));
            assert_eq!(format!("{refusal:?}"), field(row, "expect"), "{name}");
        }
    }
}
