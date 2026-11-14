//! C10 and C14: key material is wiped and never printed, and the crate's hygiene holds for the files of
//! this lane: no panicking construct, clock, file, thread or random source in library code (rules 6 and 7
//! of the launch rules), every public item names its caller or the later unit it is a seam of (rule 9),
//! the chunk hashes are read with `decode_secret` and written through a `SecretValue` (rule R17), and
//! nothing that holds a chunk hash has `Clone`.
//!
//! `cargo clippy --all-targets -- -D warnings`, the build on Rust 1.91 and on 1.99, `cargo check --target
//! wasm32-unknown-unknown` and `rustfmt --check` are the lane's holds: cargo cannot run inside `cargo test`.

mod pointer_common;

use std::fs;
use std::path::PathBuf;

use azlin_cbor::Value;
use azlin_content::pointer::{
    decode_chunk_list, ChunkList, Decoded, PointerError, PointerV2, SealError, UnknownFields,
};
use azlin_content::{ChunkRef, Full};
use azlin_keys::sign::{ALL_DOMAINS, POINTER_V2};
use azlin_keys::VerifyAt;
use pointer_common::*;
use zeroize::Zeroizing;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The library files of this lane.
const FILES: [&str; 6] = ["mod", "error", "wire", "sign", "validate", "chunk_list"];

fn source(name: &str) -> String {
    fs::read_to_string(root().join(format!("src/pointer/{name}.rs"))).expect("a source file")
}

/// The library code of a file: everything before a `#[cfg(test)]` module.
fn library_code(text: &str) -> &str {
    text.split("#[cfg(test)]").next().expect("a first part")
}

/// The code lines of a file's library code, comments left out, joined.
fn code_only(text: &str) -> String {
    library_code(text)
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------------------------
// C10
// ---------------------------------------------------------------------------------------------

#[test]
fn c10_secrets_are_wiped_and_never_printed() {
    // `to_cbor` hands the bytes back in a buffer that is wiped on drop, and the chunk list's reader a
    // wiped plaintext (compile-time checks of the signatures).
    let _to_cbor: fn(&PointerV2) -> Result<Zeroizing<Vec<u8>>, PointerError> = PointerV2::to_cbor;

    // A pointer with a chunk hash of a recognisable pattern and an attribute value that is a secret word.
    let mut p = full_with_unknown();
    match &mut p.chunks {
        ChunkList::Inline(refs) => refs[0].full = Full::from_bytes([0xC7; 32]),
        ChunkList::Object(_) => panic!("inline"),
    }
    p.xattrs = vec![("user.secret".to_string(), b"SECRETXATTRVALUE".to_vec())];
    p.unknown =
        UnknownFields::from_entries(vec![(40, Value::Bytes(b"SECRETUNKNOWNVALUE".to_vec()))])
            .expect("keys");
    let p = signed(p);

    let forbidden: Vec<(String, String)> = vec![
        ("the hash as hex".to_string(), "c7c7".to_string()),
        ("the hash as bytes".to_string(), "199, 199".to_string()),
        (
            "the attribute value".to_string(),
            "SECRETXATTRVALUE".to_string(),
        ),
        (
            "the attribute value as bytes".to_string(),
            "83, 69, 67, 82, 69, 84".to_string(),
        ),
        (
            "an unknown value".to_string(),
            "SECRETUNKNOWNVALUE".to_string(),
        ),
        (
            "an unknown value as bytes".to_string(),
            "83, 69, 67, 82, 69, 84, 85".to_string(),
        ),
        ("the file's BLAKE3".to_string(), to_hex(&p.blake3)),
        (
            "the file's BLAKE3 as bytes".to_string(),
            p.blake3
                .iter()
                .take(6)
                .map(|b| b.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ),
    ];
    let mut outputs: Vec<(String, String)> = vec![
        ("{:?} of the pointer".to_string(), format!("{p:?}")),
        ("{:#?} of the pointer".to_string(), format!("{p:#?}")),
        ("{:?} of its chunks".to_string(), format!("{:?}", p.chunks)),
        (
            "{:?} of its unknown fields".to_string(),
            format!("{:?}", p.unknown),
        ),
        ("{:?} of the Decoded".to_string(), {
            let bytes = p.to_cbor().expect("writes");
            format!(
                "{:?}",
                PointerV2::from_cbor(&bytes, &OBJECT).expect("reads")
            )
        }),
        ("{:?} of the witness".to_string(), {
            let verified = p
                .verify(&space(), &directory(), VerifyAt::Now)
                .expect("verifies");
            format!("{verified:?}")
        }),
    ];
    if let ChunkList::Inline(refs) = &p.chunks {
        outputs.push((
            "{:?} of a chunk reference".to_string(),
            format!("{:?}", refs[0]),
        ));
        let r: &ChunkRef = &refs[0];
        outputs.push(("{:?} of its hash".to_string(), format!("{:?}", r.full)));
    }
    // The errors name keys and limits, never data.
    for error in [
        PointerError::BadSignature,
        PointerError::SizeMismatch,
        PointerError::Seal(SealError::Auth),
    ] {
        outputs.push(("an error".to_string(), format!("{error:?} {error}")));
    }
    for (what, text) in &outputs {
        for (secret, needle) in &forbidden {
            assert!(
                !text.contains(needle.as_str()),
                "{what} shows {secret}: {text}"
            );
        }
    }
    // The hash prints as its redaction.
    assert!(format!("{p:?}").contains("blake3: \"***\""));
    assert_eq!(format!("{:?}", Full::from_bytes([0xC7; 32])), "Full(***)");
}

#[test]
fn c10_a_chunk_list_that_fails_leaves_no_secret_in_its_errors() {
    // The errors of the object reader carry no data either.
    let (sealed, list, _, sealer) = {
        let sealer = TestSealer::new();
        let refs = huge_chunks(1025);
        let (sealed, list) =
            azlin_content::pointer::encode_chunk_list(&refs, &space(), &sealer).expect("encodes");
        (sealed, list, refs, sealer)
    };
    let err = decode_chunk_list(&sealed, &list, &other_space(), &sealer)
        .expect_err("another space is refused");
    let text = format!("{err:?} {err}");
    assert!(!text.contains("777"), "{text}");
    assert!(text.len() < 200, "{text}");
}

#[test]
fn c10_nothing_that_holds_a_chunk_hash_has_clone() {
    // The derive lines of every type that holds a `ChunkRef`: no `Clone` (a copy is `duplicate`), in the
    // source; the compile_fail doctests of the module hold it for the compiler.
    let text = source("mod");
    for name in [
        "PointerV2",
        "ChunkList",
        "Preview",
        "StreamRef",
        "DerivedRef",
        "UnknownFields",
    ] {
        // The definition itself (`pub struct X {`, `pub struct X(`, `pub enum X {`), not a longer name
        // that starts with the same letters (`ChunkListRef` is not `ChunkList`).
        let at = [
            format!("pub struct {name} {{"),
            format!("pub struct {name}("),
            format!("pub enum {name} {{"),
        ]
        .iter()
        .find_map(|pattern| text.find(pattern.as_str()))
        .unwrap_or_else(|| panic!("{name} is defined in mod.rs"));
        let block = text[..at].rsplit("\n\n").next().expect("the block above");
        let derives: Vec<&str> = block
            .lines()
            .filter(|l| l.trim_start().starts_with("#[derive"))
            .collect();
        assert!(!derives.is_empty(), "{name} has a derive line");
        assert!(
            derives
                .iter()
                .all(|l| !l.contains("Clone") && !l.contains("Copy")),
            "{name}: {derives:?}"
        );
    }
    // No line of the library code copies a hash by `clone`.
    for file in FILES {
        let text = source(file);
        for (n, line) in library_code(&text).lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            assert!(
                !(t.contains("full") && t.contains("clone()")),
                "src/pointer/{file}.rs line {}: a hash is copied with clone: {t}",
                n + 1
            );
        }
    }
    // A decoded `Decoded` and a `Verified` are not copyable either.
    let bytes = signed(full()).to_cbor().expect("writes");
    assert!(matches!(
        PointerV2::from_cbor(&bytes, &OBJECT),
        Ok(Decoded::Current(_))
    ));
}

#[test]
fn c10_the_hashes_go_through_the_wiping_tools() {
    // Rule R17: a format whose decoded bytes are key material is read with `decode_secret`, never with
    // `decode`, and a tree built from key material is wrapped in a `SecretValue` before it is encoded.
    for file in ["wire", "chunk_list"] {
        let code = code_only(&source(file));
        assert!(
            code.contains("decode_secret("),
            "{file}.rs reads with decode_secret"
        );
        assert!(
            !code.replace("decode_secret(", "").contains("decode("),
            "{file}.rs must not call azlin_cbor::decode"
        );
        assert!(
            code.contains("SecretValue::new("),
            "{file}.rs wraps its trees"
        );
    }
    // The preimage of the signature is built in wiped buffers and framed by the one definition of the
    // signing rule; the raw layer of azlin-keys is used so no unwiped copy of the body is made.
    let code = code_only(&source("sign"));
    assert!(
        code.contains("signing_input(&POINTER_V2"),
        "the framing is signing_input"
    );
    assert!(code.contains("sign_preimage("));
    assert!(code.contains("verify_preimage("));
    assert!(code.matches("Zeroizing::new(").count() >= 2);
    // The encoded buffer is wiped.
    assert!(code_only(&source("wire")).contains("Zeroizing::new(encode("));
}

// ---------------------------------------------------------------------------------------------
// C14
// ---------------------------------------------------------------------------------------------

#[test]
fn c14_crate_hygiene() {
    // No panicking construct, no clock, file, network, thread, random source or lenient default in the
    // library code of the lane's files (rules 6 and 7 of the launch rules).
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
        "allow(",
        "#[derive(Default",
    ];
    for file in FILES {
        let text = source(file);
        for (n, line) in library_code(&text).lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            for b in banned {
                assert!(
                    !t.contains(b),
                    "src/pointer/{file}.rs: `{b}` in library code: {t} (near line {})",
                    n + 1
                );
            }
            // Indexing and slicing of bytes that came from outside is a panic in waiting: the crate root
            // denies `clippy::indexing_slicing`, and this keeps the lane's code to `get` and iterators.
            assert!(
                !t.contains("[..") && !t.contains("..]"),
                "src/pointer/{file}.rs: a slice range in library code: {t} (near line {})",
                n + 1
            );
        }
    }

    // The domain this lane signs under is well formed (D-W1S15A: `Domain::new` does not validate), and is
    // the one azlin-keys registers; the context the object is sealed under is plain ASCII.
    assert!(POINTER_V2.is_wellformed());
    assert_eq!(POINTER_V2.as_str(), "azlin-pointer-v2");
    assert!(ALL_DOMAINS.contains(&POINTER_V2));
    let context = azlin_content::limits::LIST_SEAL.as_str();
    assert!(
        !context.is_empty() && context.len() <= 64 && context.is_ascii() && !context.contains('\0')
    );
    assert!(azlin_content::CONTEXTS.contains(&azlin_content::limits::LIST_SEAL));
}

/// Rule 9 of the launch rules: every public item carries a doc line `Caller: ...` or `Seam: ...`.
/// Checked for every line that starts (after its indentation) with `pub fn`, `pub const`, `pub struct`,
/// `pub enum`, `pub trait`, `pub type` or `pub static`.
#[test]
fn c14_every_public_item_names_its_caller_or_its_seam() {
    let mut checked = 0usize;
    for file in FILES {
        let text = source(file);
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            let is_item = [
                "pub fn ",
                "pub const ",
                "pub struct ",
                "pub enum ",
                "pub trait ",
                "pub type ",
                "pub static ",
            ]
            .iter()
            .any(|prefix| t.starts_with(prefix));
            if !is_item {
                continue;
            }
            // The doc block above (attributes in between are skipped).
            let mut doc = String::new();
            let mut j = i;
            while j > 0 {
                j -= 1;
                let above = lines[j].trim();
                if let Some(rest) = above.strip_prefix("///") {
                    doc.push_str(rest);
                    doc.push('\n');
                } else if above.starts_with("#[") {
                    continue;
                } else {
                    break;
                }
            }
            assert!(
                doc.contains("Caller:") || doc.contains("Seam:"),
                "src/pointer/{file}.rs line {}: `{t}` has no `Caller:` or `Seam:` doc line",
                i + 1
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 60,
        "the scan found only {checked} public items: has the layout changed?"
    );
}
