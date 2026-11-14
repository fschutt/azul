//! C11 (entropy and secrets hygiene), C13 (the readers never panic: the entry function of the fuzz
//! target on 20,000 seeded inputs and their bit flips, and the refusals of a signed envelope with
//! arbitrary parameters) and C14 (crate hygiene: the library code of `src/recovery/` names no
//! panicking call, clock, file, network, thread or random source; every public item names its
//! caller; the contexts are the registered ones or reported; no test allocates 256 MiB) of lane
//! W1-S15d.
//!
//! Implements planning/architecture.md section 1.4 (rule 1: randomness is a parameter; rule 3: every
//! parser has a fuzz target; rule 4: everything has a limit) and the QUALITY rules 6, 8 and 9 of the
//! launch rules.
//!
//! `cargo clippy --all-targets -- -D warnings`, the builds with `--no-default-features` and
//! `--all-features` and for `wasm32-unknown-unknown` are the lane's holds: cargo cannot run inside
//! `cargo test`.

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code, clippy::needless_range_loop)]

mod recovery_common;

use std::fs;
use std::path::PathBuf;

use azlin_keys::derive::{CONTEXTS, PRF_WRAP, RECOVERY_CHECK, RECOVERY_WRAP};
use azlin_keys::envelope::{Alg, Ctx, Envelope, Params, RecoveryMethod};
use azlin_keys::recovery::{
    new_contact_secret, new_prf_salt, printed_share_text, seal_share_to_contact, split,
    unwrap_with_code, wrap_with_code, wrap_with_prf, ContactKey, KdfFloor, KdfProfile, Kib,
    OpenedShare, Quiz, RecoveryCode, RecoveryError, Share, LOCAL_CONTEXTS,
};
use azlin_keys::{EntropyError, FailingEntropy, Secret, SeededEntropy, VerifyAt};
use recovery_common::vectors;
use recovery_common::{
    from_hex, kat_code, kat_contact_secret, kat_ctx, kat_device, kat_directory, kat_signer, pat32,
    ScriptedEntropy, SplitMix64,
};
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------------------------
// C11: entropy and secrets
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

/// C11: the recovery code, a share and an opened share are not `Clone`, `Copy`, `Default` or
/// comparable with `==` (a second owner is `Share::duplicate`, a comparison is `ct_eq`).
#[test]
fn c11_secrets_are_not_clone_copy_default_or_comparable() {
    <RecoveryCode as AmbiguousIfClone<_>>::check();
    <RecoveryCode as AmbiguousIfCopy<_>>::check();
    <RecoveryCode as AmbiguousIfPartialEq<_>>::check();
    <RecoveryCode as AmbiguousIfDefault<_>>::check();
    <Share as AmbiguousIfClone<_>>::check();
    <Share as AmbiguousIfCopy<_>>::check();
    <Share as AmbiguousIfPartialEq<_>>::check();
    <Share as AmbiguousIfDefault<_>>::check();
    <OpenedShare as AmbiguousIfClone<_>>::check();
    <OpenedShare as AmbiguousIfPartialEq<_>>::check();
}

/// C11: the text of a code and of a share is made in a buffer that is wiped when dropped, and no
/// `Debug` prints a byte of a secret.
#[test]
fn c11_the_text_is_zeroizing_and_debug_prints_no_secret() {
    let code = kat_code();
    let text: Zeroizing<String> = code.to_text();
    let shares = split(code.as_secret(), 2, 3, &mut SeededEntropy::from_u64(3)).expect("a split");
    let printed: Zeroizing<String> = printed_share_text(&shares[1]);
    assert_eq!(format!("{code:?}"), "RecoveryCode(***)");
    assert_eq!(text.len(), 30);
    let share_debug = format!("{:?}", shares[1]);
    assert!(
        share_debug.starts_with("Share(x = 2, 2 of 3, split "),
        "{share_debug}"
    );
    assert!(share_debug.ends_with(", y ***)"), "{share_debug}");
    assert!(!share_debug.contains(&hex_of(shares[1].y.expose())));
    for group in printed.rsplitn(6, '-').take(5) {
        assert!(
            !share_debug.contains(group),
            "{group} of the share's text is in its Debug"
        );
    }
    // An opened share prints the words (public) and no secret.
    let me = kat_contact_secret(false);
    let words = "Felix";
    let ctx = kat_ctx(RecoveryMethod::Share);
    let env = seal_share_to_contact(
        &shares[1],
        &ContactKey::of_secret(&me),
        words,
        &ctx,
        &kat_device(),
        &kat_signer(),
        &mut SeededEntropy::from_u64(4),
    )
    .expect("sealed");
    let opened = azlin_keys::recovery::open_share(&env, &me, &ctx, &kat_directory(), VerifyAt::Now)
        .expect("opens");
    let opened_debug = format!("{opened:?}");
    assert!(opened_debug.contains("Felix"));
    assert!(
        !opened_debug.contains(&hex_of(opened.share.y.expose())),
        "{opened_debug}"
    );
    // The errors say what happened and hold no byte.
    for error in [
        RecoveryError::Auth,
        RecoveryError::CheckMismatch,
        RecoveryError::BelowFloor,
        RecoveryError::NotEnough,
        RecoveryError::Entropy(EntropyError::Os),
    ] {
        let shown = format!("{error} {error:?}");
        assert!(shown.len() < 200, "{shown}");
    }
}

fn hex_of(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// C11: a failing entropy source aborts every generator of the module: `generate`, `split`, the
/// three wraps, the quiz, the PRF salt and a contact key return an error and make nothing, at any
/// point of their draws.
#[test]
fn c11_entropy_and_secrets_hygiene() {
    let exhausted = RecoveryError::Entropy(EntropyError::Exhausted);
    let code = kat_code();
    let (device, signer) = (kat_device(), kat_signer());
    let key = Secret::<32>::from_bytes(pat32(0x50));
    let secret_16 = Secret::<16>::from_bytes([3u8; 16]);

    assert_eq!(
        RecoveryCode::generate(&mut FailingEntropy::after(0)).err(),
        Some(exhausted)
    );
    assert_eq!(
        RecoveryCode::generate(&mut FailingEntropy::after(15)).err(),
        Some(exhausted)
    );
    assert!(RecoveryCode::generate(&mut FailingEntropy::after(16)).is_ok());
    assert_eq!(
        split(&secret_16, 2, 3, &mut FailingEntropy::after(0)).err(),
        Some(exhausted)
    );
    assert_eq!(
        Quiz::new(&mut FailingEntropy::after(0)).err(),
        Some(exhausted)
    );
    assert_eq!(
        new_prf_salt(&mut FailingEntropy::after(0)).err(),
        Some(exhausted)
    );
    assert_eq!(
        new_contact_secret(&mut FailingEntropy::after(0), false).err(),
        Some(exhausted)
    );
    assert_eq!(
        new_contact_secret(&mut FailingEntropy::after(32), true).err(),
        Some(exhausted)
    );
    assert_eq!(
        wrap_with_code(
            &key,
            &code,
            &kat_ctx(RecoveryMethod::Code),
            KdfProfile::Test,
            &device,
            &signer,
            &mut FailingEntropy::after(0)
        )
        .err(),
        Some(exhausted)
    );
    assert_eq!(
        wrap_with_prf(
            &key,
            &key,
            &pat32(1),
            &pat32(2),
            &kat_ctx(RecoveryMethod::Prf),
            &device,
            &signer,
            &mut FailingEntropy::after(0)
        )
        .err(),
        Some(exhausted)
    );
    let shares = split(&secret_16, 2, 3, &mut SeededEntropy::from_u64(1)).expect("a split");
    let contact = ContactKey::of_secret(&kat_contact_secret(false));
    for good in [0usize, 31, 32, 55] {
        assert_eq!(
            seal_share_to_contact(
                &shares[0],
                &contact,
                "w",
                &kat_ctx(RecoveryMethod::Share),
                &device,
                &signer,
                &mut FailingEntropy::after(good)
            )
            .err(),
            Some(exhausted),
            "{good} good bytes"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// C13: the readers never panic
// ---------------------------------------------------------------------------------------------

/// A random printable-ish text: the alphabets of the forms, separators and a few foreign
/// characters.
fn random_text(rng: &mut SplitMix64) -> Vec<u8> {
    const PARTS: [&str; 24] = [
        "azlin-contact:",
        "azlin-recover:",
        "azlin-share:",
        "azlin-share-reply:",
        "S",
        "s",
        "-",
        " ",
        "\n",
        "0",
        "1",
        "O",
        "L",
        "U",
        "g",
        "é",
        "€",
        "A",
        "z",
        "9",
        "f",
        ":",
        "2-2-3-",
        "00ff",
    ];
    let mut out = String::new();
    for _ in 0..rng.below(24) {
        out.push_str(PARTS[rng.below(PARTS.len() as u64) as usize]);
    }
    out.into_bytes()
}

/// C13: `RecoveryCode::parse`, `combine`, the contact, request, share, reply and printed share
/// parsers and the unwraps and the share open on 20,000 seeded inputs and their bit flips: random
/// bytes, random texts built from the alphabets of the forms, and the lane's own vectors (texts,
/// shares, envelopes) unchanged, with 1 to 4 flipped bits, cut and extended.
#[test]
fn c13_fuzz_inputs_never_panic() {
    // The corpus: every row of the three vector files that has bytes or a text.
    let mut corpus: Vec<Vec<u8>> = Vec::new();
    for file in ["recovery_wrap.json", "shamir_kat.json", "contacts.json"] {
        for r in vectors::read(file) {
            let bytes = from_hex(&r.hex);
            if !bytes.is_empty() {
                corpus.push(bytes);
            }
        }
    }
    assert!(corpus.len() >= 100, "{} corpus entries", corpus.len());

    // Every entry once, unchanged (the valid algorithm 3 wraps reach Argon2id at the test cost).
    for entry in &corpus {
        recovery_common::fuzz_entry::contact_text_entry(entry);
    }

    let mut rng = SplitMix64::new(13);
    for _ in 0..20_000u32 {
        let input: Vec<u8> = match rng.below(6) {
            0 => {
                let len = rng.below(300) as usize;
                rng.bytes(len)
            }
            1 => random_text(&mut rng),
            2 => {
                let len = 23 * (1 + rng.below(4) as usize);
                rng.bytes(len)
            }
            _ => {
                // A corpus entry with 1 to 4 bits flipped, or cut, or extended.
                let mut bytes = corpus[rng.below(corpus.len() as u64) as usize].clone();
                match rng.below(5) {
                    0 => bytes.truncate(rng.below(bytes.len() as u64 + 1) as usize),
                    1 => {
                        let len = 1 + rng.below(8) as usize;
                        bytes.extend(rng.bytes(len));
                    }
                    _ => {
                        for _ in 0..1 + rng.below(4) {
                            if bytes.is_empty() {
                                break;
                            }
                            let at = rng.below(bytes.len() as u64) as usize;
                            bytes[at] ^= 1 << rng.below(8);
                        }
                    }
                }
                bytes
            }
        };
        recovery_common::fuzz_entry::contact_text_entry(&input);
    }
}

/// C13: `unwrap_with_code` on an envelope that the legitimate device signed with ARBITRARY
/// parameters (the cost a hostile server would name): the answer is the one of an independent
/// classification of the cost (above the limits, below the floor), and a cost that no device
/// derives with never reaches Argon2id. Only the costs that classify as refusals are run: a cost
/// within both bounds would hash for real.
#[test]
fn c13_unwrap_with_code_on_signed_envelopes_with_arbitrary_costs() {
    let rows = vectors::read("recovery_wrap.json");
    let base = Envelope::from_cbor(&from_hex(&vectors::row(&rows, "alg3_test_32_bytes").hex))
        .expect("an envelope");
    let ctx = kat_ctx(RecoveryMethod::Code);
    let dir = kat_directory();
    let signer = kat_signer();
    let mut rng = SplitMix64::new(77);
    let (mut above, mut below) = (0u32, 0u32);
    for case in 0..4_000u32 {
        let p = if rng.below(2) == 0 {
            1 + rng.below(8) as u32
        } else {
            1 + rng.below(255) as u32
        };
        let t = match rng.below(3) {
            0 => 1 + rng.below(1024) as u32,
            1 => 1 + rng.below(10) as u32,
            _ => 1 + rng.below(8) as u32,
        };
        let m = match rng.below(4) {
            0 => 8 * p + rng.below(10_000) as u32,
            1 => rng.below(4_194_304 - 8 * u64::from(p)) as u32 + 8 * p,
            2 => 1_048_576 + rng.below(4) as u32 - 1,
            _ => 8 * p,
        };
        if m > 4_194_304 || m < 8 * p {
            continue;
        }
        let above_limits =
            p > 8 || t > 8 || m > 1_048_576 || u64::from(m) * u64::from(t) > 4_194_304;
        let below_floor = m < 8_192 || t < 1;
        let want = if above_limits {
            RecoveryError::AboveLimit
        } else if below_floor {
            RecoveryError::BelowFloor
        } else {
            continue;
        };
        let mut env = base.clone();
        env.params = Params::Argon2id {
            salt: [case as u8; 16],
            m_kib: m,
            t,
            p,
        };
        env.sign(&signer).expect("signs");
        assert_eq!(
            unwrap_with_code(&env, &kat_code(), &ctx, &dir, VerifyAt::Now, KdfFloor::TEST)
                .map(|v| v.to_vec()),
            Err(want),
            "case {case}: {m} KiB, {t} passes, {p} lanes"
        );
        if above_limits {
            above += 1;
        } else {
            below += 1;
        }
    }
    assert!(
        above > 500 && below > 200,
        "{above} above the limits, {below} below the floor"
    );
}

/// C13: the fuzz target exists, includes the shared entry function and calls it.
#[test]
fn c13_the_fuzz_target_exists_and_calls_the_tested_entry() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target = fs::read_to_string(root.join("fuzz/fuzz_targets/contact_text.rs"))
        .expect("fuzz/fuzz_targets/contact_text.rs");
    assert!(target.contains("#![no_main]"));
    assert!(target.contains("fuzz_target!"));
    assert!(target.contains("../../tests/recovery_common/fuzz_entry.rs"));
    assert!(target.contains("contact_text_entry(data)"));
    let manifest = fs::read_to_string(root.join("fuzz/Cargo.toml")).expect("fuzz/Cargo.toml");
    assert!(manifest.contains("name = \"contact_text\""));
    assert!(manifest.contains("path = \"fuzz_targets/contact_text.rs\""));
    let entry =
        fs::read_to_string(root.join("tests/recovery_common/fuzz_entry.rs")).expect("entry");
    for call in [
        "RecoveryCode::parse(",
        "ContactKey::from_text(",
        "RequestKey::from_text(",
        "share_from_text(",
        "reply_from_text(",
        "printed_share_from_text(",
        "combine(",
        "unwrap_with_code(",
        "unwrap_with_prf(",
        "open_share(",
    ] {
        assert!(entry.contains(call), "the entry does not call {call}");
    }
}

// ---------------------------------------------------------------------------------------------
// C14: crate hygiene
// ---------------------------------------------------------------------------------------------

fn recovery_sources() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/recovery");
    let mut files: Vec<(String, String)> = fs::read_dir(&dir)
        .expect("src/recovery")
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

fn code_without_comments(text: &str) -> String {
    library_code(text)
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The files of the module, as the brief names them (and the ones this lane added).
#[test]
fn c14_the_module_has_the_files_of_the_brief() {
    let files = recovery_sources();
    let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
    for want in [
        "mod.rs",
        "code.rs",
        "passphrase.rs",
        "prf.rs",
        "shamir.rs",
        "contacts.rs",
        "quiz.rs",
        "sealed.rs",
        "open.rs",
        "error.rs",
        "internal_tests.rs",
    ] {
        assert!(names.contains(&want), "src/recovery/{want} is missing");
    }
}

#[test]
fn c14_the_library_code_names_no_panic_clock_file_network_thread_or_random_source() {
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
    let mut scanned = 0;
    for (name, text) in recovery_sources() {
        if name == "internal_tests.rs" {
            continue;
        }
        for (n, line) in library_code(&text).lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for b in banned {
                assert!(
                    !t.contains(b),
                    "src/recovery/{name}: `{b}` in library code: {t} (line {})",
                    n + 1
                );
            }
            scanned += 1;
        }
    }
    assert!(scanned > 800, "the scan saw only {scanned} lines");
}

/// Rule R17: the module decodes no CBOR of its own (the envelope reader decodes through
/// `decode_secret`): no `decode(` and no `Value` in its library code; a share's plaintext is a fixed
/// layout of 16 bytes and the words, never a CBOR value.
#[test]
fn c14_the_module_decodes_no_cbor_and_puts_no_secret_into_a_value() {
    for (name, text) in recovery_sources() {
        if name == "internal_tests.rs" {
            continue;
        }
        let code = code_without_comments(&text);
        assert!(
            !code.contains("decode("),
            "src/recovery/{name} calls decode("
        );
        assert!(
            !code.contains("azlin_cbor"),
            "src/recovery/{name} names azlin_cbor"
        );
        assert!(
            !code.contains("Value::"),
            "src/recovery/{name} builds a CBOR value"
        );
    }
}

/// Rule 9: every public item at the start of a line carries a doc line `Caller:` or `Seam:`.
#[test]
fn c14_every_public_item_names_its_caller_or_its_seam() {
    let mut checked = 0usize;
    for (name, text) in recovery_sources() {
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
                "src/recovery/{name} line {}: `{}` has no `Caller:` or `Seam:` doc line",
                i + 1,
                line.trim()
            );
            checked += 1;
        }
    }
    assert!(checked >= 50, "the scan found only {checked} public items");
}

/// Every source file cites the plan by section.
#[test]
fn c14_every_source_file_cites_the_plan_by_section() {
    for (name, text) in recovery_sources() {
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
            "src/recovery/{name}: the module docs do not cite the plan"
        );
    }
}

/// The contexts: the three the registry holds for this lane are the registered strings; the two this
/// module adds (DRIFT X-DELTA-W1S15D-10) are unique, well formed, `azlin 2026-11 <purpose> v1`, none
/// is registered or an old azul-storage string, and they are the only context strings in the
/// module's sources.
#[test]
fn c14_the_local_contexts_are_unique_and_none_is_registered() {
    let registered: Vec<&str> = CONTEXTS.iter().map(|c| c.as_str()).collect();
    for (ctx, text) in [
        (RECOVERY_WRAP, "azlin 2026-11 recovery code wrap v1"),
        (RECOVERY_CHECK, "azlin 2026-11 recovery code check v1"),
        (PRF_WRAP, "azlin 2026-11 passkey prf wrap v1"),
    ] {
        assert_eq!(ctx.as_str(), text);
        assert!(registered.contains(&text));
    }
    let mut texts = Vec::new();
    for (name, text) in LOCAL_CONTEXTS {
        assert!(
            name.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
            "{name}"
        );
        assert!(
            text.starts_with("azlin 2026-11 ") && text.ends_with(" v1"),
            "{text}"
        );
        assert!(text.bytes().all(|b| (0x20..0x7f).contains(&b)), "{text}");
        assert!(
            !registered.contains(text),
            "{text} is registered: remove it from LOCAL_CONTEXTS"
        );
        texts.push(*text);
    }
    texts.sort_unstable();
    texts.dedup();
    assert_eq!(
        texts.len(),
        LOCAL_CONTEXTS.len(),
        "a local context is used twice"
    );
    let old = [
        "Azlin 2026-10-08 member wrap: X25519 to drive key wrapping",
        "Azlin 2026-10-08 recovery code: drive key wrapping",
        "Azlin 2026-10-10 trusted contacts: split id of a recovery code",
        "Azlin 2026-10-10 trusted contacts: share sealed to a contact",
        "Azlin 2026-10-10 trusted contacts: share sealed to a request",
        "Azlin 2026-10-10 trusted contacts: safety number of a request",
        "Azlin 2026-10-10 passkey PRF: drive key wrapping",
    ];
    for text in &texts {
        assert!(!old.contains(text), "{text} is a string of today's keys");
    }
    // The context strings of the module's sources are exactly the two local ones.
    let mut in_sources = Vec::new();
    for (name, source) in recovery_sources() {
        if name == "internal_tests.rs" {
            continue;
        }
        for line in code_without_comments(&source).lines() {
            let mut rest = line;
            while let Some(at) = rest.find("\"azlin 2026-") {
                let tail = &rest[at + 1..];
                let end = tail.find('"').expect("a closing quote");
                in_sources.push(tail[..end].to_string());
                rest = &tail[end..];
            }
        }
    }
    in_sources.sort();
    assert_eq!(in_sources, texts, "the context strings of src/recovery");
}

/// No test allocates 256 MiB: the production profile is only ever named to read its numbers, and the
/// tests derive under the test profile.
#[test]
fn c14_no_test_runs_the_production_cost() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<PathBuf> = fs::read_dir(root.join("tests"))
        .expect("tests")
        .map(|e| e.expect("an entry").path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("recovery_"))
                && p.extension().is_some_and(|e| e == "rs")
        })
        .collect();
    files.push(root.join("src/recovery/internal_tests.rs"));
    let mut mentions = 0;
    for file in files {
        let text = fs::read_to_string(&file).expect("a test file");
        if file.ends_with("recovery_hygiene.rs") {
            continue;
        }
        for (n, line) in text.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") || !line.contains("KdfProfile::Production") {
                continue;
            }
            assert!(
                line.contains(".params()"),
                "{}:{}: the production profile is named other than to read its numbers: {t}",
                file.display(),
                n + 1
            );
            mentions += 1;
        }
    }
    assert!(
        mentions >= 1,
        "the production numbers are asserted somewhere"
    );
}

/// The secret-dependent code has no table indexed by a secret and no branch on one: `gf_mul` has no
/// index, no `if`, no `match`, no `?`; `gf_inv` has no index; the quiz and the code compare with
/// `ct_eq` and select with `conditional_select`.
#[test]
fn c14_the_secret_dependent_code_has_no_secret_index_branch_or_early_exit() {
    let sources = recovery_sources();
    let source = |name: &str| {
        code_without_comments(&sources.iter().find(|(n, _)| n == name).expect("a file").1)
    };
    let function = |text: &str, signature: &str| -> String {
        let start = text
            .find(signature)
            .unwrap_or_else(|| panic!("no {signature}"));
        let body = &text[start..];
        let end = body.find("\n}\n").expect("the end of the function");
        body[..end].to_string()
    };
    let shamir = source("shamir.rs");
    let gf_mul = function(&shamir, "pub fn gf_mul(");
    for banned in ["[", " if ", "match ", "?", "return", "while ", "loop "] {
        assert!(
            !gf_mul.contains(banned),
            "gf_mul contains `{banned}`:\n{gf_mul}"
        );
    }
    let gf_inv = function(&shamir, "pub fn gf_inv(");
    assert!(!gf_inv.contains('['), "gf_inv indexes:\n{gf_inv}");
    let symbol = function(&source("code.rs"), "pub(crate) fn symbol(");
    for banned in ["[", " if ", "match ", "?", "return"] {
        assert!(
            !symbol.contains(banned),
            "symbol contains `{banned}`:\n{symbol}"
        );
    }
    let decode = function(&source("code.rs"), "pub(crate) fn decode_symbol(");
    assert!(
        decode.contains("ct_eq(") && decode.contains("conditional_select("),
        "decode_symbol is constant time:\n{decode}"
    );
    let quiz = source("quiz.rs");
    assert!(quiz.contains("ct_eq("), "the quiz compares with ct_eq");
    assert!(
        source("passphrase.rs").contains("memory.zeroize();"),
        "the memory of Argon2id is wiped after the derivation"
    );
    assert!(
        source("code.rs").contains("fn ct_eq("),
        "RecoveryCode::ct_eq"
    );
}
