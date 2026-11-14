//! C14 (secrets are secrets and are never logged), C15 (the readers never panic: the entry
//! function of the fuzz target on 20,000 seeded inputs and their bit flips, and the key exchange
//! and the AEAD on envelopes signed by the legitimate device) and C16 (crate hygiene: the
//! library code of `src/envelope/` names no panicking call, clock, file, network, thread or
//! random source; every public item names its caller; the contexts are the registered ones).
//!
//! Implements planning/architecture.md section 1.4 (rule 3: every parser has a fuzz target;
//! rule 4: everything has a limit) and the QUALITY rules 6, 8 and 9 of the launch rules.
//!
//! `cargo clippy --all-targets -- -D warnings`, the builds with `--no-default-features`, with
//! `--all-features` and for `wasm32-unknown-unknown` are the lane's holds: cargo cannot run
//! inside `cargo test`.

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code)]

mod common;
mod envelope_common;

use std::fs;
use std::path::PathBuf;

use azlin_keys::derive::{CONTEXTS, ENVELOPE_ALG1_KEY, ENVELOPE_ALG2_KEY};
use azlin_keys::envelope::{
    Alg, CtxError, EnvelopeError, OpenError, ParamsError, RecipientSecret, WrapError,
};
use azlin_keys::{EntropyError, Secret};
use common::SplitMix64;
use envelope_common::{pat, pat32, pat64, to_hex};

// ---------------------------------------------------------------------------------------------
// C14: secrets are secrets
// ---------------------------------------------------------------------------------------------

/// Negative-trait checks: `<T as AmbiguousIfX<_>>::check()` compiles only when `T` does NOT
/// implement the trait (the ambiguity of two blanket impls). If someone adds the impl, this test
/// target stops compiling, which is the failure.
trait AmbiguousIfClone<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfClone<()> for T {}
impl<T: Clone> AmbiguousIfClone<u8> for T {}

trait AmbiguousIfCopy<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfCopy<()> for T {}
impl<T: Copy> AmbiguousIfCopy<u8> for T {}

trait AmbiguousIfPartialEq<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfPartialEq<()> for T {}
impl<T: ?Sized + PartialEq> AmbiguousIfPartialEq<u8> for T {}

trait AmbiguousIfDefault<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfDefault<()> for T {}
impl<T: Default> AmbiguousIfDefault<u8> for T {}

#[test]
fn c14_the_recipient_secret_is_not_clone_copy_default_or_comparable() {
    <RecipientSecret as AmbiguousIfClone<_>>::check();
    <RecipientSecret as AmbiguousIfCopy<_>>::check();
    <RecipientSecret as AmbiguousIfPartialEq<_>>::check();
    <RecipientSecret as AmbiguousIfDefault<_>>::check();
}

#[test]
fn c14_the_debug_of_a_recipient_secret_shows_no_key_and_no_public_key() {
    let kx = pat32(0x21);
    let seed = pat64(0x42);
    let me = RecipientSecret::new(&Secret::from_bytes(kx), Some(&Secret::from_bytes(seed)));
    for shown in [
        format!("{me:?}"),
        format!("{me:#?}"),
        format!("{:?}", Some(&me)),
        format!("{:?}", vec![&me]),
    ] {
        assert!(shown.contains("RecipientSecret(***)"), "{shown}");
        for needle in [to_hex(&kx), to_hex(&seed), to_hex(me.kx_public())] {
            assert!(!shown.contains(&needle), "{shown} shows {needle}");
        }
    }
    assert_eq!(format!("{me:?}"), "RecipientSecret(***)");
}

/// Every error of the module, formatted both ways: a short sentence with numbers in it and no run
/// of hexadecimal digits that could be a key, a nonce or a plaintext.
#[test]
fn c14_no_error_text_holds_bytes() {
    use azlin_cbor::{DecodeError, EncodeError, ShapeError};

    let mut texts = Vec::new();
    let mut add = |display: String, debug: String| {
        texts.push(display);
        texts.push(debug);
    };
    for alg in Alg::ALL {
        add(
            WrapError::Unsupported(alg).to_string(),
            format!("{:?}", WrapError::Unsupported(alg)),
        );
        add(
            WrapError::WrongAlg(alg).to_string(),
            format!("{:?}", WrapError::WrongAlg(alg)),
        );
        add(
            OpenError::Unsupported(alg).to_string(),
            format!("{:?}", OpenError::Unsupported(alg)),
        );
        add(
            OpenError::WrongAlg(alg).to_string(),
            format!("{:?}", OpenError::WrongAlg(alg)),
        );
    }
    let wraps = [
        WrapError::Entropy(EntropyError::Os),
        WrapError::Entropy(EntropyError::Exhausted),
        WrapError::BadRecipientKey,
        WrapError::NonContributory,
        WrapError::PlaintextTooLarge,
        WrapError::Cipher,
        WrapError::Kem,
        WrapError::Envelope(EnvelopeError::BadSignature),
    ];
    for e in wraps {
        add(e.to_string(), format!("{e:?}"));
    }
    let opens = [
        OpenError::Malformed(EnvelopeError::UnknownAlg(9)),
        OpenError::CtxMismatch,
        OpenError::NotForMe,
        OpenError::BadSignature,
        OpenError::UnknownSigner,
        OpenError::SignerNotYetActive,
        OpenError::SignerRevoked,
        OpenError::DirectoryUnavailable,
        OpenError::NonContributory,
        OpenError::Auth,
    ];
    for e in opens {
        add(e.to_string(), format!("{e:?}"));
    }
    let envelopes = [
        EnvelopeError::Cbor(DecodeError::Truncated),
        EnvelopeError::Shape(ShapeError::WrongLength(4)),
        EnvelopeError::UnsupportedVersion(3),
        EnvelopeError::UnknownAlg(255),
        EnvelopeError::FieldRequired(5),
        EnvelopeError::FieldForbidden(16),
        EnvelopeError::BadCtx(CtxError::BadId),
        EnvelopeError::BadParams(ParamsError::UnknownKey(9)),
        EnvelopeError::BadUnknown,
        EnvelopeError::BadCrit,
        EnvelopeError::TooLarge,
        EnvelopeError::Encode(EncodeError::InputTooLarge),
        EnvelopeError::NotCanonical,
        EnvelopeError::BadSignature,
        EnvelopeError::UnknownSigner,
        EnvelopeError::SignerNotYetActive,
        EnvelopeError::SignerRevoked,
        EnvelopeError::DirectoryUnavailable,
    ];
    for e in envelopes {
        add(e.to_string(), format!("{e:?}"));
    }
    for e in [
        ParamsError::NotAMap,
        ParamsError::Shape(ShapeError::Missing(1)),
        ParamsError::UnknownKey(5),
        ParamsError::OutOfRange(2),
        ParamsError::WrongKind,
    ] {
        add(e.to_string(), format!("{e:?}"));
    }
    for e in [
        CtxError::TooLong,
        CtxError::BadCharacter,
        CtxError::EmptySegment,
        CtxError::UnknownKind,
        CtxError::BadShape,
        CtxError::BadId,
        CtxError::BadEpoch,
        CtxError::BadMethod,
    ] {
        add(e.to_string(), format!("{e:?}"));
    }
    assert!(texts.len() > 100);
    for text in &texts {
        assert!(text.len() < 120, "{text:?} is long");
        let longest_hex_run = text
            .split(|c: char| !c.is_ascii_hexdigit())
            .map(str::len)
            .max()
            .unwrap_or(0);
        // Words like "decade" or "faced" are hexadecimal letters too: a run of 16 digits is what
        // a key, a nonce or a hash would be.
        assert!(longest_hex_run < 16, "{text:?} has a run of hex digits");
    }
}

// ---------------------------------------------------------------------------------------------
// C15: the readers never panic
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "test-support")]
mod fuzz {
    use super::*;
    use crate::common::SplitMix64;
    use crate::envelope_common::fuzz_entry::envelope_decode_entry;
    use crate::envelope_common::{pat, Fixture};
    use azlin_keys::envelope::{open, wrap_x25519, Envelope};
    use azlin_keys::{KeyId, Signature, VerifyAt};

    /// One random edit of `bytes`: flip a bit, replace a byte, insert, delete, truncate, extend.
    fn edit(bytes: &mut Vec<u8>, rng: &mut SplitMix64) {
        let len = bytes.len() as u64;
        match rng.below(7) {
            0 if len > 0 => {
                let i = rng.below(len) as usize;
                bytes[i] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let i = rng.below(len) as usize;
                bytes[i] = rng.next_u64() as u8;
            }
            2 => {
                let i = rng.below(len + 1) as usize;
                bytes.insert(i, rng.next_u64() as u8);
            }
            3 if len > 0 => {
                let i = rng.below(len) as usize;
                bytes.remove(i);
            }
            4 if len > 0 => {
                let keep = rng.below(len) as usize;
                bytes.truncate(keep);
            }
            5 => {
                let count = 1 + rng.below(8) as usize;
                let extra = rng.bytes(count);
                bytes.extend(extra);
            }
            _ => {
                // A byte that a CBOR head makes interesting.
                let heads = [
                    0x00u8, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1f, 0x40, 0x58, 0x59, 0x5f, 0x78, 0xa0,
                    0xa9, 0xbf, 0xf6, 0xf9, 0xff,
                ];
                let i = rng.below(len + 1) as usize;
                let head = heads[rng.below(heads.len() as u64) as usize];
                if i < bytes.len() {
                    bytes[i] = head;
                } else {
                    bytes.push(head);
                }
            }
        }
    }

    #[test]
    fn c15_fuzz_inputs_never_panic() {
        // The empty input, every 1-byte and every 2-byte input.
        envelope_decode_entry(&[]);
        for a in 0..=255u8 {
            envelope_decode_entry(&[a]);
            for b in 0..=255u8 {
                envelope_decode_entry(&[a, b]);
            }
        }
        // 20,000 seeded random inputs; a third start like an envelope (a map of nine), so that
        // the reader gets past its first byte.
        let mut rng = SplitMix64::new(0xC15);
        for _ in 0..20_000 {
            let len = rng.below(400) as usize;
            let mut data = rng.bytes(len);
            if rng.below(3) == 0 && !data.is_empty() {
                data[0] = 0xa9;
            }
            envelope_decode_entry(&data);
        }
        // Valid envelopes with one to three edits: the inputs next to the format, where a lenient
        // reader would let a second spelling through. The unedited bytes must be accepted.
        let f = Fixture::new(15);
        let alg1 = [
            f.wrap1(b"", 1),
            f.wrap1(&pat(0, 32), 2),
            f.wrap1(&pat(0, 200), 3),
        ];
        for env in &alg1 {
            let bytes = env.to_cbor().expect("bytes");
            envelope_decode_entry(&bytes);
            assert!(Envelope::from_cbor(&bytes).is_ok());
            for _ in 0..7_000 {
                let mut edited = bytes.clone();
                for _ in 0..1 + rng.below(3) {
                    edit(&mut edited, &mut rng);
                }
                envelope_decode_entry(&edited);
            }
        }
        #[cfg(feature = "pq")]
        {
            let env = f.wrap2(&pat(0, 68), 4);
            let bytes = env.to_cbor().expect("bytes");
            envelope_decode_entry(&bytes);
            for _ in 0..3_000 {
                let mut edited = bytes.clone();
                for _ in 0..1 + rng.below(3) {
                    edit(&mut edited, &mut rng);
                }
                envelope_decode_entry(&edited);
            }
        }
    }

    /// The key exchange and the AEAD on garbage that carries a valid signature: the fields of a
    /// real envelope replaced by random bytes, the envelope signed again by the legitimate
    /// device, and opened by the recipient it names. Nothing panics; nothing but the unchanged
    /// envelope opens.
    #[test]
    fn c15_open_never_panics_on_signed_garbage() {
        let f = Fixture::new(15);
        let mut rng = SplitMix64::new(0xC15A);
        let original = f.wrap1(&pat(0, 40), 5);
        assert!(f.open(&original).is_ok());
        let mut refused = 0;
        for _ in 0..3_000 {
            let mut env = original.clone();
            if rng.below(2) == 0 {
                let mut epk = [0u8; 32];
                epk.copy_from_slice(&rng.bytes(32));
                env.epk = Some(epk);
            }
            if rng.below(2) == 0 {
                env.nonce.copy_from_slice(&rng.bytes(24));
            }
            if rng.below(2) == 0 {
                let len = 16 + rng.below(80) as usize;
                env.ct = rng.bytes(len);
            }
            if rng.below(4) == 0 {
                let mut rcpt = [0u8; 32];
                rcpt.copy_from_slice(&rng.bytes(32));
                env.rcpt = KeyId::from_bytes(rcpt);
            }
            env.sign(&f.signer).expect("signs");
            if env == original {
                continue;
            }
            match f.open(&env) {
                Ok(_) => panic!("a changed envelope opened"),
                Err(_) => refused += 1,
            }
        }
        assert!(refused > 2_500);
        // The same for algorithm 2 (the decapsulation of random ciphertexts).
        #[cfg(feature = "pq")]
        {
            let original = f.wrap2(&pat(0, 40), 6);
            for _ in 0..500 {
                let mut env = original.clone();
                let len = azlin_keys::envelope::KEM_CT_LEN;
                if rng.below(2) == 0 {
                    env.kem_ct = Some(rng.bytes(len));
                } else if let Some(kem_ct) = env.kem_ct.as_mut() {
                    let i = rng.below(len as u64) as usize;
                    kem_ct[i] ^= 1 << rng.below(8);
                }
                if rng.below(2) == 0 {
                    let mut epk = [0u8; 32];
                    epk.copy_from_slice(&rng.bytes(32));
                    env.epk = Some(epk);
                }
                env.sign(&f.signer).expect("signs");
                assert!(f.open(&env).is_err());
            }
        }
    }
}

/// The fuzz target exists, is declared in the fuzz project and calls the entry function this
/// file runs.
#[test]
fn c15_the_fuzz_target_exists_and_calls_the_tested_entry() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = fs::read_to_string(root.join("fuzz/fuzz_targets/envelope_decode.rs"))
        .expect("fuzz/fuzz_targets/envelope_decode.rs");
    assert!(target.contains("#![no_main]"));
    assert!(target.contains("fuzz_target!"));
    assert!(target.contains("../../tests/envelope_common/fuzz_entry.rs"));
    assert!(target.contains("envelope_decode_entry(data)"));
    let manifest = fs::read_to_string(root.join("fuzz/Cargo.toml")).expect("fuzz/Cargo.toml");
    assert!(manifest.contains("name = \"envelope_decode\""));
    assert!(manifest.contains("path = \"fuzz_targets/envelope_decode.rs\""));
    let entry =
        fs::read_to_string(root.join("tests/envelope_common/fuzz_entry.rs")).expect("entry");
    for call in [
        "check_shape(",
        "Envelope::from_cbor(",
        "verify_signature(",
        "open(",
    ] {
        assert!(entry.contains(call), "the entry does not call {call}");
    }
}

// ---------------------------------------------------------------------------------------------
// C16: crate hygiene
// ---------------------------------------------------------------------------------------------

fn envelope_sources() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/envelope");
    let mut files: Vec<(String, String)> = fs::read_dir(&dir)
        .expect("src/envelope")
        .map(|entry| entry.expect("an entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "rs"))
        .map(|path| {
            let name = path
                .file_name()
                .expect("a file name")
                .to_string_lossy()
                .to_string();
            let text = fs::read_to_string(&path).expect("a source file");
            (name, text)
        })
        .collect();
    files.sort();
    files
}

/// The library part of a source file: everything before its first `#[cfg(test)]`.
fn library_code(text: &str) -> &str {
    text.split("#[cfg(test)]").next().expect("a first part")
}

/// The library part of a source file without its comment lines (`//`, `///`, `//!`).
fn code_without_comments(text: &str) -> String {
    library_code(text)
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn c16_the_library_code_names_no_panic_clock_file_network_thread_or_random_source() {
    let banned = [
        "unsafe",
        "std::fs",
        "std::net",
        "std::time",
        "std::thread",
        "std::process",
        "std::env",
        "SystemTime",
        "Instant",
        "rand::",
        "getrandom",
        "OsEntropy",
        "serde",
        ".unwrap()",
        ".expect(",
        "panic!",
        "unreachable!",
        "todo!",
        "unimplemented!",
        "unwrap_or_default",
        "unwrap_unchecked",
        "get_unchecked",
        "mem::transmute",
        "println!",
        "dbg!",
        ".ok()",
        "let _ =",
    ];
    let files = envelope_sources();
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    for want in [
        "mod.rs",
        "wire.rs",
        "alg_table.rs",
        "aead.rs",
        "x25519.rs",
        "hybrid.rs",
        "combiner.rs",
        "verify.rs",
        "ctx.rs",
        "error.rs",
        "recipient.rs",
        "opening.rs",
    ] {
        assert!(names.contains(&want), "src/envelope/{want} is missing");
    }
    let mut scanned = 0;
    for (name, text) in &files {
        if name == "internal_tests.rs" {
            continue;
        }
        for (n, line) in library_code(text).lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for b in banned {
                assert!(
                    !t.contains(b),
                    "src/envelope/{name}: `{b}` in library code: {t} (line {})",
                    n + 1
                );
            }
            scanned += 1;
        }
    }
    assert!(scanned > 500, "the scan saw only {scanned} lines");
}

/// Rule R17: the one place bytes are decoded is `decode_secret`; no plain `decode(`.
#[test]
fn c16_the_wire_reader_decodes_through_decode_secret() {
    let files = envelope_sources();
    let wire = &files
        .iter()
        .find(|(n, _)| n == "wire.rs")
        .expect("wire.rs")
        .1;
    assert!(code_without_comments(wire).contains("decode_secret("));
    for (name, text) in &files {
        if name == "internal_tests.rs" {
            continue;
        }
        let code = code_without_comments(text);
        assert!(
            !code.contains("decode("),
            "src/envelope/{name} calls a plain decode("
        );
    }
}

/// Rule 9: every public item at the start of a line carries a doc line `Caller:` or `Seam:`.
#[test]
fn c16_every_public_item_names_its_caller_or_its_seam() {
    let mut checked = 0usize;
    for (name, text) in envelope_sources() {
        if name == "internal_tests.rs" || name == "mod.rs" {
            continue;
        }
        let lines: Vec<&str> = library_code(&text).lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let is_item = line.starts_with("pub ")
                && !line.starts_with("pub use ")
                && !line.starts_with("pub mod ");
            if !is_item {
                continue;
            }
            let mut doc = String::new();
            let mut j = i;
            while j > 0 {
                j -= 1;
                let t = lines[j].trim();
                if let Some(rest) = t.strip_prefix("///") {
                    doc.push_str(rest);
                    doc.push('\n');
                } else if t.starts_with("#[") {
                    continue;
                } else {
                    break;
                }
            }
            assert!(
                doc.contains("Caller:") || doc.contains("Seam:"),
                "src/envelope/{name} line {}: `{}` has no `Caller:` or `Seam:` doc line",
                i + 1,
                line.trim()
            );
            checked += 1;
        }
    }
    assert!(checked >= 40, "the scan found only {checked} public items");
}

/// The module cites the plan by section (the code cites its sources).
#[test]
fn c16_every_source_file_cites_the_plan_by_section() {
    for (name, text) in envelope_sources() {
        if name == "internal_tests.rs" {
            continue;
        }
        let head: String = text
            .lines()
            .take_while(|l| l.starts_with("//!"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            head.contains("planning/") || head.contains("architecture.md"),
            "src/envelope/{name}: the module docs do not cite the plan"
        );
    }
}

/// The contexts used here are entries of the registry of `derive.rs`, are pinned, and differ from
/// every context of today's azul-storage keys (which stay as they are until INT-S15-1).
#[test]
fn c16_the_contexts_are_the_registered_ones_and_none_is_an_old_string() {
    let registered: Vec<&str> = CONTEXTS.iter().map(|c| c.as_str()).collect();
    assert!(registered.contains(&ENVELOPE_ALG1_KEY.as_str()));
    assert!(registered.contains(&ENVELOPE_ALG2_KEY.as_str()));
    assert_eq!(
        ENVELOPE_ALG1_KEY.as_str(),
        "azlin 2026-11 envelope v2 alg1 key v1"
    );
    assert_eq!(
        ENVELOPE_ALG2_KEY.as_str(),
        "azlin 2026-11 envelope v2 alg2 key v1"
    );
    assert_ne!(ENVELOPE_ALG1_KEY.as_str(), ENVELOPE_ALG2_KEY.as_str());
    // The strings of examples/azul-storage/src/crypto (keys.rs, contacts.rs, mod.rs, drops.rs,
    // share.rs, azl1.rs): the member wrap among them.
    let old = [
        "Azlin 2026-10-08 member wrap: X25519 to drive key wrapping",
        "Azlin 2026-10-08 member id",
        "Azlin 2026-10-08 recovery code: drive key wrapping",
        "Azlin 2026-11-04 key chain: older drive key wrapping",
        "Azlin 2026-10-10 trusted contacts: split id of a recovery code",
        "Azlin 2026-10-10 trusted contacts: share sealed to a contact",
        "Azlin 2026-10-10 trusted contacts: share sealed to a request",
        "Azlin 2026-10-10 trusted contacts: safety number of a request",
        "Azlin 2026-10-10 share key: manifest sealing",
        "Azlin 2026-10-10 drive key: drop key wrapping",
        "Azlin 2026-10-08 AZL1 segment key",
        "Azlin 2026-10-08 AZL1 header info key",
        "Azlin 2026-10-08 AZL1 trailer key",
        "Azlin 2026-10-08 drive key: key id",
        "Azlin 2026-10-08 drive key: file key wrapping",
        "Azlin 2026-10-08 share key: key id",
        "Azlin 2026-10-08 share key: file key wrapping",
    ];
    for old in old {
        assert!(!registered.contains(&old), "{old} is registered here");
        assert_ne!(ENVELOPE_ALG1_KEY.as_str(), old);
        assert_ne!(ENVELOPE_ALG2_KEY.as_str(), old);
    }
    // And the envelope sources derive only through the registered constants: no context string
    // of their own, and `derive_key` only in the combiner.
    for (name, text) in envelope_sources() {
        if name == "internal_tests.rs" {
            continue;
        }
        let code = code_without_comments(&text);
        assert!(
            !code.contains("\"azlin 2026"),
            "src/envelope/{name} holds a context string"
        );
        if name != "combiner.rs" {
            assert!(
                !code.contains("derive_key"),
                "src/envelope/{name} derives a key outside the combiner"
            );
        }
    }
}
