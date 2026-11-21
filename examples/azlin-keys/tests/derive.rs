//! C5 (the derivation known answers) and C6 (the registry of contexts).
//!
//! `tests/vectors/derive_kat.json` pins, for a fixed key, the key derived under every context of
//! `CONTEXTS`, the public derivation of fixed material under every context, and the key ids of
//! two bundles: changing a context string, the order of the table, the derivation or the key id
//! rule fails `c5_derive_known_answers_pinned`. The file is written with `UPDATE_VECTORS=1`
//! (never when `CI` is set) and reviewed like code.
//!
//! Implements planning/architecture.md section 5.5 ("BLAKE3 `derive_key` with separate contexts
//! gives `K_seal` and `K_chunk`").

mod common;

use std::fs;
use std::path::PathBuf;

use azlin_keys::derive::{derive, derive_public, Context, CONTEXTS, KEY_ID, K_CHUNK, K_SEAL};
use azlin_keys::{KeyId, Secret};

use common::{to_hex, Row};

/// The key of the known answers: the bytes 0, 1, ... 31.
fn kat_key() -> [u8; 32] {
    let mut key = [0u8; 32];
    for (i, b) in key.iter_mut().enumerate() {
        *b = i as u8;
    }
    key
}

const PUBLIC_MATERIAL: &[u8] = b"azlin public material for the known answers";

fn computed_rows() -> Vec<Row> {
    let key = Secret::<32>::from_bytes(kat_key());
    let mut rows = Vec::new();
    for ctx in CONTEXTS {
        rows.push(Row::new(
            ctx.name(),
            "derive",
            ctx.as_str(),
            &to_hex(&kat_key()),
            "",
            &to_hex(derive(ctx, &key).expose()),
        ));
    }
    for ctx in CONTEXTS {
        rows.push(Row::new(
            &format!("{}_PUBLIC", ctx.name()),
            "derive_public",
            ctx.as_str(),
            &to_hex(PUBLIC_MATERIAL),
            "",
            &to_hex(&derive_public(ctx, PUBLIC_MATERIAL)),
        ));
    }
    // Key ids: an X25519-only bundle and a hybrid one (the pq key is 40 bytes here, so the file
    // stays small; the rule does not look at its length).
    let kx = {
        let mut b = [0u8; 32];
        for (i, x) in b.iter_mut().enumerate() {
            *x = 0x40 + i as u8;
        }
        b
    };
    let pq: Vec<u8> = (0..40u8).map(|i| 0x80 + i).collect();
    rows.push(Row::new(
        "key_id_x25519_only",
        "key_id",
        KEY_ID.as_str(),
        &to_hex(&kx),
        "",
        &KeyId::of_bundle(&kx, None).to_string(),
    ));
    rows.push(Row::new(
        "key_id_hybrid",
        "key_id",
        KEY_ID.as_str(),
        &to_hex(&kx),
        &to_hex(&pq),
        &KeyId::of_bundle(&kx, Some(&pq)).to_string(),
    ));
    rows
}

#[test]
fn c5_derive_known_answers_pinned() {
    let rows = computed_rows();
    // Every context of the table has a `derive` row and a `derive_public` row, in table order.
    assert_eq!(rows.len(), 2 * CONTEXTS.len() + 2);
    common::check_or_update("derive_kat.json", &rows);
}

#[test]
fn c5_the_pinned_answers_are_blake3_derive_key_and_nothing_else() {
    let key = Secret::<32>::from_bytes(kat_key());
    for ctx in CONTEXTS {
        let direct = blake3::derive_key(ctx.as_str(), &kat_key());
        assert_eq!(
            derive(ctx, &key).expose(),
            &direct,
            "derive is derive_key: {}",
            ctx.name()
        );
        assert_eq!(
            derive_public(ctx, &kat_key()),
            direct,
            "derive_public is derive_key: {}",
            ctx.name()
        );
        assert_ne!(
            derive(ctx, &key).expose(),
            &kat_key(),
            "a derived key is not its parent"
        );
    }
    // The pinned file agrees with that definition independently of this crate's own code: read
    // it back and compare each `derive` row with blake3 called directly. (Not while the file is
    // being written by the other test of this binary.)
    if std::env::var_os("UPDATE_VECTORS").is_some() {
        return;
    }
    let text = fs::read_to_string(common::vector_path("derive_kat.json")).expect("the vector file");
    for row in common::parse_json(&text)
        .iter()
        .filter(|r| r.kind == "derive")
    {
        let key = common::from_hex(&row.hex);
        let want = blake3::derive_key(&row.context, &key);
        assert_eq!(to_hex(&want), row.expect, "row {}", row.name);
    }
}

fn hamming(a: &[u8; 32], b: &[u8; 32]) -> u32 {
    a.iter().zip(b).map(|(x, y)| (x ^ y).count_ones()).sum()
}

#[test]
fn c5_contexts_give_unrelated_keys_and_the_same_context_the_same_key() {
    let key = Secret::<32>::from_bytes(kat_key());
    let derived: Vec<[u8; 32]> = CONTEXTS.iter().map(|c| *derive(c, &key).expose()).collect();

    // Two different contexts give unrelated keys: all distinct, and each pair differs in about
    // half of its 256 bits (a loose bound that a related-key bug would break).
    for (i, (a, ctx_a)) in derived.iter().zip(CONTEXTS).enumerate() {
        for (b, ctx_b) in derived.iter().zip(CONTEXTS).skip(i + 1) {
            assert_ne!(a, b, "{} and {}", ctx_a.name(), ctx_b.name());
            let d = hamming(a, b);
            assert!(
                (64..=192).contains(&d),
                "{} vs {}: {d} bits differ",
                ctx_a.name(),
                ctx_b.name()
            );
        }
    }

    // The same context and key always give the same key.
    for (ctx, first) in CONTEXTS.iter().zip(&derived) {
        assert_eq!(derive(ctx, &key).expose(), first);
        assert_eq!(derive(ctx, &key.duplicate()).expose(), first);
    }

    // Another key gives another derived key, and one flipped bit changes about half the output.
    let base = *derive(&K_SEAL, &key).expose();
    for bit in 0..256usize {
        let mut other = kat_key();
        other[bit / 8] ^= 1 << (bit % 8);
        let derived_other = *derive(&K_SEAL, &Secret::from_bytes(other)).expose();
        assert_ne!(derived_other, base, "bit {bit}");
        let d = hamming(&derived_other, &base);
        assert!((64..=192).contains(&d), "bit {bit}: {d} bits differ");
    }

    // K_seal and K_chunk of one key are different keys (one key, one purpose).
    assert_ne!(
        derive(&K_SEAL, &key).expose(),
        derive(&K_CHUNK, &key).expose()
    );
}

// ---------------------------------------------------------------------------------------------
// C6: the registry of contexts
// ---------------------------------------------------------------------------------------------

#[test]
fn c6_context_registry_is_unique_and_documented() {
    let table: &[Context] = std::hint::black_box(CONTEXTS);
    assert!(
        table.len() >= 13,
        "the table holds the contexts of the keys family"
    );

    for (i, a) in CONTEXTS.iter().enumerate() {
        let text = a.as_str();
        // The form `azlin <...> v<N>`: lowercase printable ASCII, single spaces, no edge spaces.
        assert!(
            text.starts_with("azlin "),
            "{} starts with `azlin `: {text}",
            a.name()
        );
        let (_, version) = text.rsplit_once(" v").unwrap_or(("", ""));
        assert!(
            !version.is_empty() && version.bytes().all(|b| b.is_ascii_digit()),
            "{} ends with ` v<N>`: {text}",
            a.name()
        );
        assert!(
            text.bytes().all(|b| b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || b == b' '
                || b == b'_'
                || b == b'-'),
            "{} holds a character outside [a-z0-9 _-]: {text}",
            a.name()
        );
        assert!(
            !text.contains("  ") && !text.ends_with(' '),
            "{}: {text}",
            a.name()
        );

        // The constant's name is the identifier: upper case, digits, underscores.
        assert!(
            !a.name().is_empty()
                && a.name()
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'),
            "name {:?}",
            a.name()
        );

        // No two contexts equal or prefix-equal (either way round), no two with one name.
        for b in &CONTEXTS[i + 1..] {
            assert_ne!(
                a.as_str(),
                b.as_str(),
                "{} and {} are equal",
                a.name(),
                b.name()
            );
            assert!(
                !a.as_str().starts_with(b.as_str()),
                "{} starts with {}",
                a.name(),
                b.name()
            );
            assert!(
                !b.as_str().starts_with(a.as_str()),
                "{} starts with {}",
                b.name(),
                a.name()
            );
            assert_ne!(a.name(), b.name());
        }
    }

    // The plan's own two strings and the other strings of the content family belong to
    // azlin-content's table (S14b's C20 asserts the tables are disjoint): none of them is here.
    // DRIFT X-DELTA-W1S15A-5.
    for content in [
        "azlin chunk id v1",
        "azlin chunk key v1",
        "azlin 2026-11 fastcdc gear v1",
        "azlin 2026-11 fastcdc gear keyed v1",
        "azlin 2026-11 pack trailer key v1",
        "azlin 2026-11 pack padding key v1",
        "azlin 2026-11 chunk list seal v1",
    ] {
        assert!(
            CONTEXTS.iter().all(|c| c.as_str() != content),
            "{content} is a content-family context and stays in azlin-content's table"
        );
    }

    // Every entry of the table in the source documents the section of the plan and the lane, and
    // the source table is exactly CONTEXTS (same names, same strings, same order).
    let entries = source_entries();
    assert_eq!(
        entries.len(),
        CONTEXTS.len(),
        "the source table and CONTEXTS list the same entries"
    );
    for ((name, text, doc), ctx) in entries.iter().zip(CONTEXTS) {
        assert_eq!((name.as_str(), text.as_str()), (ctx.name(), ctx.as_str()));
        assert!(
            doc.contains("planning/architecture.md section "),
            "{name}: the doc names no section of the plan"
        );
        assert!(doc.contains("lane W1-S"), "{name}: the doc names no lane");
        assert!(doc.contains("Caller:"), "{name}: the doc names no caller");
    }
}

/// The entries of the `contexts! { ... }` table in `src/derive.rs`: name, string, doc text.
fn source_entries() -> Vec<(String, String, String)> {
    let source =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/derive.rs"))
            .expect("src/derive.rs");
    let start =
        source.find("\ncontexts! {\n").expect("the contexts! table") + "\ncontexts! {\n".len();
    let end = start + source[start..].find("\n}\n").expect("the end of the table");
    let mut entries = Vec::new();
    let mut doc = String::new();
    for line in source[start..end].lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("///") {
            doc.push_str(rest.trim());
            doc.push(' ');
        } else if let Some((name, value)) = t.split_once(" = ") {
            let text = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix("\";"))
                .unwrap_or_else(|| panic!("an entry `NAME = \"text\";`: {t}"));
            entries.push((name.to_string(), text.to_string(), std::mem::take(&mut doc)));
        } else {
            assert!(t.is_empty(), "unexpected line in the table: {t}");
            doc.clear();
        }
    }
    entries
}

#[test]
fn c6_a_context_is_plain_copyable_data_with_a_name() {
    let a: Context = K_SEAL;
    let b = a; // Copy
    assert_eq!(a, b);
    assert_eq!(a.name(), "K_SEAL");
    assert_eq!(a.as_str(), "azlin 2026-11 space k_seal v1");
    assert_ne!(K_SEAL, K_CHUNK);
    assert_eq!(KEY_ID.as_str(), "azlin key id v1");
    assert!(format!("{a:?}").contains("K_SEAL"));
    // The table is a `const` slice of exactly the constants.
    assert!(
        CONTEXTS.contains(&K_SEAL) && CONTEXTS.contains(&K_CHUNK) && CONTEXTS.contains(&KEY_ID)
    );
}
