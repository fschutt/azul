//! C20 and C21: the table of derive contexts and the numbers of the plan.
//!
//! C20: the seven contexts of this crate are unique, well formed and disjoint from the contexts of
//! `azlin-keys`, so no key of the keys family and no key of the content family can ever be derived
//! under the same purpose string.
//!
//! C21: the sizes of the pipeline are the numbers of planning/architecture.md section 5.7, typed here
//! from the plan's own words (INT-PLAN-TABLES compares them to the plan's text later).

use std::collections::BTreeSet;

use azlin_content::limits::{
    AVG_CHUNK, BLOB_CODEC_LEN, BLOB_NONCE_LEN, BLOB_OVERHEAD, BLOB_TAG_LEN, COMPRESS_MIN_SAVING,
    CONTEXTS, MAX_BLOB, MAX_CHUNK, MAX_FILE_CHUNKS, MIN_CHUNK, NORMAL_CHUNK, PACK_PAD_STEP,
    PACK_REPO_PAD, PACK_TARGET,
};
use azlin_content::{ChunkLen, CompressPolicy, PackLen, Percent};
use azlin_keys::Secret;

#[test]
fn c20_contexts_are_disjoint_from_azlin_keys() {
    // The seven contexts of the brief's section 4, by name and by string, in this order.
    let want: [(&str, &str); 7] = [
        ("CHUNK_ID", "azlin chunk id v1"),
        ("CHUNK_KEY", "azlin chunk key v1"),
        ("GEAR_PUBLIC", "azlin 2026-11 fastcdc gear v1"),
        ("GEAR_KEYED", "azlin 2026-11 fastcdc gear keyed v1"),
        ("K_PACK_TRAILER", "azlin 2026-11 pack trailer key v1"),
        ("PACK_PADDING", "azlin 2026-11 pack padding key v1"),
        ("LIST_SEAL", "azlin 2026-11 chunk list seal v1"),
    ];
    let got: Vec<(&str, &str)> = CONTEXTS.iter().map(|c| (c.name(), c.as_str())).collect();
    assert_eq!(got, want, "the table of contexts, pinned");

    // Unique by name and by string.
    let names: BTreeSet<&str> = CONTEXTS.iter().map(|c| c.name()).collect();
    let texts: BTreeSet<&str> = CONTEXTS.iter().map(|c| c.as_str()).collect();
    assert_eq!(names.len(), CONTEXTS.len(), "context names are unique");
    assert_eq!(texts.len(), CONTEXTS.len(), "context strings are unique");

    // Well formed: printable ASCII, no NUL, ends in `v<digits>`.
    for context in CONTEXTS {
        let text = context.as_str();
        assert!(
            !text.is_empty() && text.bytes().all(|b| (0x20..0x7f).contains(&b)),
            "{}: printable ASCII only",
            context.name()
        );
        let (_, version) = text.rsplit_once(" v").expect("a trailing ` v<N>`");
        assert!(
            !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit()),
            "{}: ends in v<N>",
            context.name()
        );
    }

    // Disjoint from the keys family, by string and by name.
    let keys_texts: BTreeSet<&str> = azlin_keys::derive::CONTEXTS
        .iter()
        .map(|c| c.as_str())
        .collect();
    let keys_names: BTreeSet<&str> = azlin_keys::derive::CONTEXTS
        .iter()
        .map(|c| c.name())
        .collect();
    assert!(
        azlin_keys::derive::CONTEXTS.len() >= 13,
        "the keys family's table has its contexts"
    );
    for context in CONTEXTS {
        assert!(
            !keys_texts.contains(context.as_str()),
            "{} ({}) is also a context of azlin-keys",
            context.name(),
            context.as_str()
        );
        assert!(
            !keys_names.contains(context.name()),
            "{} is also a constant name of azlin-keys",
            context.name()
        );
    }
}

#[test]
fn c20_a_context_derives_what_blake3_derive_key_derives() {
    // `derive` and `derive_public` are BLAKE3's `derive_key` under the context string, nothing else.
    let material = [0x5au8; 32];
    for context in CONTEXTS {
        let want = blake3::derive_key(context.as_str(), &material);
        assert_eq!(context.derive_public(&material), want, "{}", context.name());
        let derived = context.derive(&Secret::from_bytes(material));
        assert_eq!(derived.expose(), &want, "{}", context.name());
    }
    // Different contexts give different keys from the same material.
    let outputs: BTreeSet<[u8; 32]> = CONTEXTS
        .iter()
        .map(|c| c.derive_public(&material))
        .collect();
    assert_eq!(outputs.len(), CONTEXTS.len());
}

#[test]
fn c21_plan_constants_are_the_plans() {
    // Planning/architecture.md section 5.7 pipeline step 2: "FastCDC over the transformed stream
    // (average 1 MiB, minimum 256 KiB, maximum 4 MiB; a file under 256 KiB is one chunk)".
    assert_eq!(MIN_CHUNK.get(), 262_144, "minimum 256 KiB");
    assert_eq!(AVG_CHUNK.get(), 1_048_576, "average 1 MiB");
    assert_eq!(MAX_CHUNK.get(), 4_194_304, "maximum 4 MiB");
    // Step 3: "per-chunk zstd when it saves 5 % (the existing rule)".
    assert_eq!(
        COMPRESS_MIN_SAVING,
        Percent::new(5).expect("5 is a percentage")
    );
    assert_eq!(CompressPolicy::plan().min_saving().get(), 5);
    // Step 5: "packing into 4 MiB packs"; section 5.15 M4: "4 MiB packs, the last one padded to 256 KiB
    // steps; repository objects to 4 KiB".
    assert_eq!(PACK_TARGET, PackLen::new(4_194_304));
    assert_eq!(PACK_PAD_STEP, PackLen::new(262_144));
    assert_eq!(PACK_REPO_PAD, PackLen::new(4_096));

    // The layout of a chunk blob (brief P-1): codec (1), nonce (24), ciphertext, tag (16).
    assert_eq!(BLOB_CODEC_LEN, 1);
    assert_eq!(BLOB_NONCE_LEN, 24, "XChaCha20 takes a 24-byte nonce");
    assert_eq!(BLOB_TAG_LEN, 16, "Poly1305 tag");
    assert_eq!(BLOB_OVERHEAD, 41);
    assert_eq!(
        MAX_BLOB.as_usize(),
        MAX_CHUNK.as_usize() + BLOB_OVERHEAD,
        "the largest blob is the largest chunk and the framing"
    );
    assert_eq!(MAX_BLOB, ChunkLen::new(4_194_304 + 41).expect("in range"));
    assert!(
        ChunkLen::new(4_194_304 + 42).is_none(),
        "ChunkLen stops at MAX_BLOB"
    );

    // The relations between them.
    assert!(MIN_CHUNK < NORMAL_CHUNK && NORMAL_CHUNK < AVG_CHUNK && AVG_CHUNK < MAX_CHUNK);
    assert_eq!(
        NORMAL_CHUNK.get(),
        868_352,
        "the normal size: 848 KiB (DRIFT X-DELTA-W1S14B-6)"
    );
    assert_eq!(
        MAX_FILE_CHUNKS, 16_777_216,
        "2^24 chunks (DRIFT X-DELTA-W1S14B-7)"
    );
    // A file of the most chunks, each of the largest size, still has a size a u64 holds; at the
    // shortest chunk it is 4 TiB, at the largest 64 TiB.
    assert_eq!(
        u64::from(MAX_FILE_CHUNKS) * u64::from(MIN_CHUNK.get()),
        1 << 42
    );
    assert_eq!(
        u64::from(MAX_FILE_CHUNKS) * u64::from(MAX_CHUNK.get()),
        1 << 46
    );
}
