//! T1-T7: signed notes and checkpoints.

use azlin_tlog::{
    sign_checkpoint, Checkpoint, CheckpointError, Cosigner, Hash, KeyHash, NameError, Note,
    NoteError, NoteSig, NoteSigner, NumberError, Origin, SigKind, TreeSize, Verifier,
    MAX_EXTENSIONS, MAX_EXTENSION_BYTES, MAX_SIGNATURES, MAX_TEXT_BYTES,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha2::{Digest as _, Sha256};

use crate::common::{hex32, hex_string, key, kname, log_key, sha, LOG_KEY_NAME};

/// T1's note: made outside the crate (hashlib and OpenSSL), seed `[7; 32]`, name `azlin.io/release-log`,
/// origin `azlin.io/release-log/v1`, size 12, root `SHA-256("azlin tlog golden root")`.
pub const GOLDEN_NOTE: &str = "azlin.io/release-log/v1\n12\njZPkPQbfCRtfZaB1K3XGimmFvx/RdAd8Qbv+ugdgmbA=\n\n\u{2014} azlin.io/release-log nVUlOw3S+cJ9GN2aSacm/0IlQAMvBD1yRihCoDQMtCVq+cWdKiUUTeWolPm9ipAXwpVIj8NqBJuXk84IXCHlC+FIXA0=\n";

/// The text of [`GOLDEN_NOTE`].
pub const GOLDEN_TEXT: &str =
    "azlin.io/release-log/v1\n12\njZPkPQbfCRtfZaB1K3XGimmFvx/RdAd8Qbv+ugdgmbA=\n";

const GOLDEN_SIG_B64: &str =
    "nVUlOw3S+cJ9GN2aSacm/0IlQAMvBD1yRihCoDQMtCVq+cWdKiUUTeWolPm9ipAXwpVIj8NqBJuXk84IXCHlC+FIXA0=";

/// The public key of seed `[7; 32]` as OpenSSL derived it.
pub const LOG_PUBLIC: &str = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c";

/// The golden log key as a verifier, built from the pinned public key and not from `MemoryKey`.
pub fn log_verifier() -> Verifier {
    Verifier {
        name: kname(LOG_KEY_NAME),
        kind: SigKind::Ed25519,
        public: hex32(LOG_PUBLIC),
    }
}

fn golden_root() -> Hash {
    Hash(sha(b"azlin tlog golden root"))
}

fn sig_line(name: &str, b64: &str) -> String {
    format!("\u{2014} {name} {b64}")
}

/// A note from a text and signature lines, as bytes.
fn note_bytes(text: &str, lines: &[String]) -> Vec<u8> {
    let mut out = format!("{text}\n");
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    out.into_bytes()
}

#[test]
fn t1_a_checkpoint_note_is_golden() {
    let cp = Checkpoint::new(Origin::azlin(), TreeSize(12), golden_root(), vec![]).unwrap();
    assert_eq!(cp.text(), GOLDEN_TEXT);
    let note = sign_checkpoint(&cp, &log_key()).unwrap();
    assert_eq!(note.to_bytes(), GOLDEN_NOTE.as_bytes(), "the pinned bytes");
    // parse(to_bytes(x)) == x, and the pinned bytes parse to the same note
    assert_eq!(Note::parse(&note.to_bytes()).unwrap(), note);
    assert_eq!(Note::parse(GOLDEN_NOTE.as_bytes()).unwrap(), note);
    assert_eq!(Checkpoint::parse_text(note.text()).unwrap(), cp);
    // the key the goldens were made with
    assert_eq!(hex_string(&log_key().public()), LOG_PUBLIC);
    // the verifier built from the independent public key accepts it
    let verified = note.verify(&[log_verifier()]).unwrap();
    assert_eq!(verified.len(), 1);
    assert_eq!(verified[0].kind, SigKind::Ed25519);
    assert_eq!(verified[0].time, None);
}

#[test]
fn t1_the_c2sp_example_note_and_vkey_are_read_and_verified() {
    // The example of c2sp.org/signed-note (checked with OpenSSL when this test was written).
    let vkey = "example.com/foo+530d903a+AekyeRrm56hApGFkyQR4ZCbV54Id2LKaANYcrnKv3U2k";
    let text = "This is an example message.\n\n\u{2014} example.com/foo Uw2QOkn8srV1yJGh2VYRlL1Tnagv1YEq6TfXppzi2ONncAlTgK7Ztg1ERYNZXsYjOBH3mFXmRKuwHjG1Yu72IneyaQM=\n";
    let verifier = Verifier::from_vkey(vkey).unwrap();
    assert_eq!(verifier.kind, SigKind::Ed25519);
    assert_eq!(verifier.vkey(), vkey, "the vkey text round-trips");
    let note = Note::parse(text.as_bytes()).unwrap();
    assert_eq!(note.text(), "This is an example message.\n");
    assert_eq!(note.to_bytes(), text.as_bytes());
    let verified = note.verify(std::slice::from_ref(&verifier)).unwrap();
    assert_eq!(verified.len(), 1);
    // a vkey whose key id is not the computed one, a type that is not 0x01 or 0x04, bad parts
    for bad in [
        "example.com/foo+530d903b+AekyeRrm56hApGFkyQR4ZCbV54Id2LKaANYcrnKv3U2k",
        "example.com/foo+530d903a+AukyeRrm56hApGFkyQR4ZCbV54Id2LKaANYcrnKv3U2k",
        "example.com/foo+530D903A+AekyeRrm56hApGFkyQR4ZCbV54Id2LKaANYcrnKv3U2k",
        "example.com/foo+530d903a",
        "example.com/foo",
        "+530d903a+AekyeRrm56hApGFkyQR4ZCbV54Id2LKaANYcrnKv3U2k",
        "example.com/foo+530d903a+AekyeRrm56hApGFkyQR4ZCbV54Id2LKaANYcrnKv3U2",
    ] {
        assert!(Verifier::from_vkey(bad).is_err(), "{bad}");
    }
}

#[test]
fn t2_note_parse_is_strict() {
    let many = |n: usize| -> Vec<String> {
        (0..n)
            .map(|i| {
                sig_line(
                    &format!("k{i}.example/key"),
                    &STANDARD.encode([i as u8; 68]),
                )
            })
            .collect()
    };
    let good = sig_line(LOG_KEY_NAME, GOLDEN_SIG_B64);
    let long_text = format!("{}\n", "a".repeat(MAX_TEXT_BYTES));
    let cases: Vec<(&str, Vec<u8>, NoteError)> = vec![
        (
            "no blank line",
            format!("{GOLDEN_TEXT}{good}\n").into_bytes(),
            NoteError::NoBlankLine,
        ),
        (
            "no final newline",
            format!("{GOLDEN_TEXT}\n{good}").into_bytes(),
            NoteError::NoFinalNewline,
        ),
        (
            "a hyphen for the em dash",
            note_bytes(GOLDEN_TEXT, &[good.replacen('\u{2014}', "-", 1)]),
            NoteError::NotASignatureLine,
        ),
        (
            "no space between the dash and the name",
            note_bytes(GOLDEN_TEXT, &[good.replacen("\u{2014} ", "\u{2014}", 1)]),
            NoteError::NotASignatureLine,
        ),
        (
            "no space between the name and the signature",
            note_bytes(
                GOLDEN_TEXT,
                &[format!("\u{2014} {LOG_KEY_NAME}{GOLDEN_SIG_B64}")],
            ),
            NoteError::MissingSpace,
        ),
        (
            "base64 without padding",
            note_bytes(
                GOLDEN_TEXT,
                &[sig_line(LOG_KEY_NAME, GOLDEN_SIG_B64.trim_end_matches('='))],
            ),
            NoteError::BadBase64,
        ),
        (
            "base64 with a character outside the alphabet",
            note_bytes(GOLDEN_TEXT, &[sig_line(LOG_KEY_NAME, "nVUl*w3S")]),
            NoteError::BadBase64,
        ),
        (
            "a payload of 3 bytes",
            note_bytes(GOLDEN_TEXT, &[sig_line(LOG_KEY_NAME, "AAAA")]),
            NoteError::ShortPayload,
        ),
        (
            "a payload of 4 bytes (a key hash and no signature)",
            note_bytes(GOLDEN_TEXT, &[sig_line(LOG_KEY_NAME, "AAAAAA==")]),
            NoteError::ShortPayload,
        ),
        (
            "a payload over the limit",
            note_bytes(
                GOLDEN_TEXT,
                &[sig_line(
                    LOG_KEY_NAME,
                    &STANDARD.encode(vec![1u8; 4 + 4097]),
                )],
            ),
            NoteError::TooLarge,
        ),
        (
            "a control character in the text",
            note_bytes(
                "azlin.io/release-log/v1\n12\u{1}\nroot\n",
                std::slice::from_ref(&good),
            ),
            NoteError::ControlCharacter,
        ),
        (
            "invalid UTF-8",
            {
                let mut bytes = note_bytes(GOLDEN_TEXT, std::slice::from_ref(&good));
                bytes[3] = 0xFF;
                bytes
            },
            NoteError::NotUtf8,
        ),
        (
            "a key name with a space",
            note_bytes(
                GOLDEN_TEXT,
                &[sig_line("azlin io/release-log", GOLDEN_SIG_B64)],
            ),
            NoteError::BadName(NameError::Space),
        ),
        (
            "a key name with a Unicode space",
            note_bytes(
                GOLDEN_TEXT,
                &[sig_line("azlin\u{00A0}io/release-log", GOLDEN_SIG_B64)],
            ),
            NoteError::BadName(NameError::Space),
        ),
        (
            "a key name with a plus",
            note_bytes(
                GOLDEN_TEXT,
                &[sig_line("azlin+io/release-log", GOLDEN_SIG_B64)],
            ),
            NoteError::BadName(NameError::Plus),
        ),
        (
            "an empty key name",
            note_bytes(GOLDEN_TEXT, &[format!("\u{2014}  {GOLDEN_SIG_B64}")]),
            NoteError::BadName(NameError::Empty),
        ),
        (
            "17 signatures",
            note_bytes(GOLDEN_TEXT, &many(MAX_SIGNATURES + 1)),
            NoteError::TooMany,
        ),
        (
            "16 KiB + 1 of text",
            note_bytes(&long_text, std::slice::from_ref(&good)),
            NoteError::TooLarge,
        ),
        (
            "the same signature line twice",
            note_bytes(GOLDEN_TEXT, &[good.clone(), good.clone()]),
            NoteError::DuplicateSignature,
        ),
        (
            "text after the last signature",
            format!("{GOLDEN_TEXT}\n{good}\nmore text\n").into_bytes(),
            NoteError::NotASignatureLine,
        ),
        (
            "CRLF line ends",
            note_bytes(
                &GOLDEN_TEXT.replace('\n', "\r\n"),
                std::slice::from_ref(&good),
            ),
            NoteError::ControlCharacter,
        ),
        (
            "a carriage return at the end of a signature line",
            format!("{GOLDEN_TEXT}\n{good}\r\n").into_bytes(),
            NoteError::ControlCharacter,
        ),
        (
            "a control character in a key name",
            note_bytes(
                GOLDEN_TEXT,
                &[sig_line("azlin\u{1}io/release-log", GOLDEN_SIG_B64)],
            ),
            NoteError::ControlCharacter,
        ),
        (
            "an empty text",
            format!("\n\n{good}\n").into_bytes(),
            NoteError::EmptyText,
        ),
        (
            "no signature at all",
            format!("{GOLDEN_TEXT}\n").into_bytes(),
            NoteError::NoSignature,
        ),
        ("nothing", Vec::new(), NoteError::NoFinalNewline),
    ];
    assert!(cases.len() >= 16);
    for (what, bytes, want) in cases {
        assert_eq!(Note::parse(&bytes), Err(want), "{what}");
    }
    // what is allowed: 16 signatures; the text of 16 KiB; an empty line inside the text (the text ends at the
    // LAST empty line); a text line that looks like a signature
    assert_eq!(
        Note::parse(&note_bytes(GOLDEN_TEXT, &many(MAX_SIGNATURES)))
            .unwrap()
            .sigs()
            .len(),
        MAX_SIGNATURES
    );
    let at_limit = format!("{}\n", "a".repeat(MAX_TEXT_BYTES - 1));
    assert!(Note::parse(&note_bytes(&at_limit, std::slice::from_ref(&good))).is_ok());
    let inside = Note::parse(&note_bytes(
        "one\n\n\u{2014} a b\ntwo\n",
        std::slice::from_ref(&good),
    ))
    .unwrap();
    assert_eq!(inside.text(), "one\n\n\u{2014} a b\ntwo\n");
    assert_eq!(Note::parse(&inside.to_bytes()).unwrap(), inside);
    // a text that ends with an empty line survives the round trip
    let blank_end = Note::parse(&note_bytes("one\n\n", std::slice::from_ref(&good))).unwrap();
    assert_eq!(blank_end.text(), "one\n\n");
    assert_eq!(Note::parse(&blank_end.to_bytes()).unwrap(), blank_end);
    // the constructor holds the same rules
    let sig = Note::parse(GOLDEN_NOTE.as_bytes()).unwrap().sigs()[0].clone();
    assert_eq!(
        Note::new("no newline".to_string(), vec![sig.clone()]),
        Err(NoteError::NoFinalNewline)
    );
    assert_eq!(
        Note::new("tab\there\n".to_string(), vec![sig.clone()]),
        Err(NoteError::ControlCharacter)
    );
    assert_eq!(
        Note::new("\n\n".to_string(), vec![sig.clone()]),
        Err(NoteError::EmptyText)
    );
    assert_eq!(
        Note::new("x\n".to_string(), vec![]),
        Err(NoteError::NoSignature)
    );
    assert_eq!(
        Note::new("x\n".to_string(), vec![sig.clone(), sig]),
        Err(NoteError::DuplicateSignature)
    );
}

#[test]
fn t3_the_key_hash_is_the_spec_formula() {
    // The formula of c2sp.org/signed-note, computed here with sha2 and not with the library.
    let formula = |name: &str, kind: u8, public: &[u8; 32]| -> [u8; 4] {
        let mut hasher = Sha256::new();
        hasher.update(name.as_bytes());
        hasher.update([0x0A]);
        hasher.update([kind]);
        hasher.update(public);
        let digest = hasher.finalize();
        [digest[0], digest[1], digest[2], digest[3]]
    };
    for seed in [0u8, 1, 7, 200, 255] {
        for name in [
            LOG_KEY_NAME,
            "witness.example/w1",
            "a",
            "k\u{e9}y.example/\u{4e2d}",
        ] {
            let k = key(name, seed);
            let public = k.public();
            let ed = KeyHash::of(&kname(name), SigKind::Ed25519, &public);
            let co = KeyHash::of(&kname(name), SigKind::Cosignature, &public);
            assert_eq!(
                *ed.as_bytes(),
                formula(name, 0x01, &public),
                "{name} {seed}"
            );
            assert_eq!(
                *co.as_bytes(),
                formula(name, 0x04, &public),
                "{name} {seed}"
            );
            assert_ne!(ed, co, "the two types of one key have two hashes");
            // every way to ask for the hash agrees
            assert_eq!(NoteSigner::key_hash(&k), ed);
            assert_eq!(Cosigner::key_hash(&k), co);
            assert_eq!(k.hash_for(SigKind::Ed25519), ed);
            assert_eq!(k.verifier(SigKind::Ed25519).key_hash(), ed);
            assert_eq!(k.verifier(SigKind::Cosignature).key_hash(), co);
        }
    }
    // pinned: the values hashlib gave for the golden key (seed [7; 32], name azlin.io/release-log)
    let k = log_key();
    assert_eq!(hex_string(NoteSigner::key_hash(&k).as_bytes()), "9d55253b");
    assert_eq!(hex_string(Cosigner::key_hash(&k).as_bytes()), "476a2d24");
    // the type byte values and their inverse
    assert_eq!(SigKind::Ed25519.type_byte(), 0x01);
    assert_eq!(SigKind::Cosignature.type_byte(), 0x04);
    assert_eq!(SigKind::from_type_byte(0x01), Some(SigKind::Ed25519));
    assert_eq!(SigKind::from_type_byte(0x04), Some(SigKind::Cosignature));
    for other in [0x00u8, 0x02, 0x03, 0x05, 0x06, 0xff] {
        assert_eq!(SigKind::from_type_byte(other), None);
    }
}

#[test]
fn t4_a_signature_covers_the_text_exactly() {
    let note = Note::parse(GOLDEN_NOTE.as_bytes()).unwrap();
    let verifier = log_verifier();
    note.verify(std::slice::from_ref(&verifier)).unwrap();
    let text = note.text().as_bytes().to_vec();
    // any one byte of the text, flipped
    let mut refused_by_signature = 0;
    for i in 0..text.len() {
        let mut mutated = text.clone();
        mutated[i] ^= 0x01;
        let Ok(bad) = Note::new(String::from_utf8(mutated).unwrap(), note.sigs().to_vec()) else {
            assert_eq!(text[i], b'\n', "only a newline becomes a control character");
            continue;
        };
        match bad.verify(std::slice::from_ref(&verifier)) {
            Err(NoteError::BadSignature { name }) => {
                assert_eq!(name, kname(LOG_KEY_NAME));
                refused_by_signature += 1;
            }
            other => panic!("byte {i} flipped: {other:?}"),
        }
    }
    assert_eq!(
        refused_by_signature,
        text.len() - 3,
        "all but the 3 newlines"
    );
    // one newline more at the end, or the last newline taken away
    let longer = Note::new(format!("{}\n", note.text()), note.sigs().to_vec()).unwrap();
    assert!(matches!(
        longer.verify(std::slice::from_ref(&verifier)),
        Err(NoteError::BadSignature { .. })
    ));
    let shorter = note.text().strip_suffix('\n').unwrap().to_string();
    assert_eq!(
        Note::new(shorter, note.sigs().to_vec()),
        Err(NoteError::NoFinalNewline)
    );
    // a byte deleted, a byte inserted, the origin line changed
    for edited in [
        note.text().replacen("12", "1", 1),
        note.text().replacen("12", "121", 1),
        note.text().replacen("release-log/v1", "release-log/v2", 1),
        note.text().replacen("azlin.io", "azlin.xx", 1),
        note.text().replacen("12\n", "13\n", 1),
    ] {
        let bad = Note::new(edited.clone(), note.sigs().to_vec()).unwrap();
        assert!(
            matches!(
                bad.verify(std::slice::from_ref(&verifier)),
                Err(NoteError::BadSignature { .. })
            ),
            "{edited}"
        );
    }
    // the signature of another text does not verify this one
    let other = Checkpoint::new(Origin::azlin(), TreeSize(13), golden_root(), vec![]).unwrap();
    let other_note = sign_checkpoint(&other, &log_key()).unwrap();
    let swapped = Note::new(note.text().to_string(), other_note.sigs().to_vec()).unwrap();
    assert!(matches!(
        swapped.verify(&[verifier]),
        Err(NoteError::BadSignature { .. })
    ));
}

#[test]
fn t5_unknown_signers_are_ignored_known_keys_must_verify() {
    let note = Note::parse(GOLDEN_NOTE.as_bytes()).unwrap();
    let verifier = log_verifier();
    let log_hash = verifier.key_hash();
    let junk = vec![9u8; 64];
    // an extra unknown signature: ignored
    let extra = NoteSig::new(
        kname("other.example/key"),
        KeyHash::from_bytes([1, 2, 3, 4]),
        junk.clone(),
    )
    .unwrap();
    let with_extra = note.with_sig(extra).unwrap();
    let verified = with_extra.verify(std::slice::from_ref(&verifier)).unwrap();
    assert_eq!(verified.len(), 1);
    assert_eq!(verified[0].name, kname(LOG_KEY_NAME));
    // the same name with another key hash is another key: ignored, even though the name is known
    let same_name = NoteSig::new(
        kname(LOG_KEY_NAME),
        KeyHash::from_bytes([0, 0, 0, 0]),
        junk.clone(),
    )
    .unwrap();
    assert_eq!(
        with_extra
            .with_sig(same_name)
            .unwrap()
            .verify(std::slice::from_ref(&verifier))
            .unwrap()
            .len(),
        1
    );
    // the same key hash under another name is another key: ignored
    let same_hash = NoteSig::new(kname("x.example/y"), log_hash, junk.clone()).unwrap();
    assert_eq!(
        with_extra
            .with_sig(same_hash)
            .unwrap()
            .verify(std::slice::from_ref(&verifier))
            .unwrap()
            .len(),
        1
    );
    // a known key (name and hash) with a wrong signature is an error, never skipped, even beside a good one
    let good = note.sigs()[0].clone();
    for flip in [0usize, 31, 63] {
        let mut payload = good.payload().to_vec();
        payload[flip] ^= 0x80;
        let bad = NoteSig::new(good.name().clone(), good.key_hash(), payload).unwrap();
        let broken = Note::new(note.text().to_string(), vec![bad]).unwrap();
        assert_eq!(
            broken.verify(std::slice::from_ref(&verifier)),
            Err(NoteError::BadSignature {
                name: kname(LOG_KEY_NAME)
            })
        );
    }
    // a signature of the wrong length for its type
    for len in [0usize, 63, 65, 72] {
        let payload = if len == 0 { vec![] } else { vec![5u8; len] };
        if payload.is_empty() {
            assert_eq!(
                NoteSig::new(good.name().clone(), good.key_hash(), payload),
                Err(NoteError::ShortPayload)
            );
            continue;
        }
        let odd = NoteSig::new(good.name().clone(), good.key_hash(), payload).unwrap();
        let broken = Note::new(note.text().to_string(), vec![odd]).unwrap();
        assert!(matches!(
            broken.verify(std::slice::from_ref(&verifier)),
            Err(NoteError::BadSignature { .. })
        ));
    }
    // no signature of a known key: refused
    let only_unknown = Note::new(
        note.text().to_string(),
        vec![NoteSig::new(
            kname("other.example/key"),
            KeyHash::from_bytes([1, 2, 3, 4]),
            junk,
        )
        .unwrap()],
    )
    .unwrap();
    assert_eq!(
        only_unknown.verify(std::slice::from_ref(&verifier)),
        Err(NoteError::NoKnownSignature)
    );
    assert_eq!(note.verify(&[]), Err(NoteError::NoKnownSignature));
    // a public key of small order (the identity point) is refused as a key, not read as a bad signature
    let mut weak = verifier;
    let mut identity = [0u8; 32];
    identity[0] = 1;
    weak.public = identity;
    assert_eq!(note.verify(&[weak]), Err(NoteError::BadKey));
}

#[test]
fn t6_checkpoint_text_round_trip() {
    let roots = [
        Hash([0; 32]),
        Hash([0xFF; 32]),
        golden_root(),
        Hash(sha(b"other")),
    ];
    let exts: [Vec<String>; 4] = [
        vec![],
        vec!["one".into()],
        vec!["zeta".into(), "alpha".into(), "mid dle".into()],
        (0..MAX_EXTENSIONS).map(|i| format!("ext-{i}")).collect(),
    ];
    for (i, size) in [1u64, 2, 12, 4_294_967_296, 99_999].into_iter().enumerate() {
        let root = roots[i % roots.len()];
        let extensions = exts[i % exts.len()].clone();
        let cp =
            Checkpoint::new(Origin::azlin(), TreeSize(size), root, extensions.clone()).unwrap();
        let text = cp.text();
        let parsed = Checkpoint::parse_text(&text).unwrap();
        assert_eq!(parsed, cp);
        assert_eq!(parsed.size(), TreeSize(size));
        assert_eq!(parsed.root(), root);
        assert_eq!(
            parsed.extensions(),
            extensions.as_slice(),
            "extensions keep their order"
        );
        assert_eq!(parsed.origin(), &Origin::azlin());
        let mut want = format!(
            "azlin.io/release-log/v1\n{size}\n{}\n",
            STANDARD.encode(root.0)
        );
        for line in &extensions {
            want.push_str(line);
            want.push('\n');
        }
        assert_eq!(text, want);
    }
    // the empty log
    let empty = azlin_merkle::empty_root();
    let cp = Checkpoint::new(Origin::azlin(), TreeSize(0), empty, vec![]).unwrap();
    assert_eq!(Checkpoint::parse_text(&cp.text()).unwrap(), cp);
    assert!(cp.text().contains("\n0\n"));
    // another origin, with the characters the name rule allows
    let origin = Origin::new("sigsum.example/v1/tree/00ff~\u{e9}").unwrap();
    let cp = Checkpoint::new(origin.clone(), TreeSize(3), golden_root(), vec![]).unwrap();
    assert_eq!(
        Checkpoint::parse_text(&cp.text()).unwrap().origin(),
        &origin
    );
}

#[test]
fn t7_checkpoint_is_strict() {
    let root_b64 = STANDARD.encode(golden_root().0);
    let cp = |origin: &str, size: &str, root: &str, exts: &[&str]| -> String {
        let mut text = format!("{origin}\n{size}\n{root}\n");
        for line in exts {
            text.push_str(line);
            text.push('\n');
        }
        text
    };
    let o = "azlin.io/release-log/v1";
    let long_ext = "e".repeat(MAX_EXTENSION_BYTES + 1);
    let nine: Vec<String> = (0..MAX_EXTENSIONS + 1).map(|i| format!("x{i}")).collect();
    let nine_refs: Vec<&str> = nine.iter().map(String::as_str).collect();
    let empty_root = STANDARD.encode(azlin_merkle::empty_root().0);
    let cases: Vec<(&str, String, CheckpointError)> = vec![
        (
            "leading zeros in the size",
            cp(o, "012", &root_b64, &[]),
            CheckpointError::Size(NumberError::LeadingZero),
        ),
        (
            "a size of zeros",
            cp(o, "00", &root_b64, &[]),
            CheckpointError::Size(NumberError::LeadingZero),
        ),
        (
            "a size over u64",
            cp(o, "18446744073709551616", &root_b64, &[]),
            CheckpointError::Size(NumberError::Overflow),
        ),
        (
            "a size of 100 digits",
            cp(o, &"9".repeat(100), &root_b64, &[]),
            CheckpointError::Size(NumberError::Overflow),
        ),
        (
            "a size over 2^32 leaves",
            cp(o, "4294967297", &root_b64, &[]),
            CheckpointError::TooManyLeaves,
        ),
        (
            "u64::MAX",
            cp(o, "18446744073709551615", &root_b64, &[]),
            CheckpointError::TooManyLeaves,
        ),
        (
            "a sign in the size",
            cp(o, "+5", &root_b64, &[]),
            CheckpointError::Size(NumberError::NotDecimal),
        ),
        (
            "a space in the size",
            cp(o, " 5", &root_b64, &[]),
            CheckpointError::Size(NumberError::NotDecimal),
        ),
        (
            "a word for the size",
            cp(o, "five", &root_b64, &[]),
            CheckpointError::Size(NumberError::NotDecimal),
        ),
        (
            "an empty size",
            cp(o, "", &root_b64, &[]),
            CheckpointError::Size(NumberError::NotDecimal),
        ),
        (
            "a root of 31 bytes",
            cp(o, "5", &STANDARD.encode([1u8; 31]), &[]),
            CheckpointError::RootLength,
        ),
        (
            "a root of 33 bytes",
            cp(o, "5", &STANDARD.encode([1u8; 33]), &[]),
            CheckpointError::RootLength,
        ),
        (
            "an empty root",
            cp(o, "5", "", &[]),
            CheckpointError::RootLength,
        ),
        (
            "a root that is no base64",
            cp(o, "5", "!!!!", &[]),
            CheckpointError::RootNotBase64,
        ),
        (
            "a root without padding",
            cp(o, "5", root_b64.trim_end_matches('='), &[]),
            CheckpointError::RootNotBase64,
        ),
        (
            "a root with non-zero trailing bits",
            cp(
                o,
                "5",
                &format!("{}B=", &root_b64[..root_b64.len() - 2]),
                &[],
            ),
            CheckpointError::RootNotBase64,
        ),
        (
            "an empty origin",
            cp("", "5", &root_b64, &[]),
            CheckpointError::BadOrigin(NameError::Empty),
        ),
        (
            "an origin with a space",
            cp("azlin io", "5", &root_b64, &[]),
            CheckpointError::BadOrigin(NameError::Space),
        ),
        (
            "an origin with a plus",
            cp("azlin+io", "5", &root_b64, &[]),
            CheckpointError::BadOrigin(NameError::Plus),
        ),
        (
            "an origin of 256 bytes",
            cp(&"o".repeat(256), "5", &root_b64, &[]),
            CheckpointError::BadOrigin(NameError::TooLong),
        ),
        (
            "9 extension lines",
            cp(o, "5", &root_b64, &nine_refs),
            CheckpointError::TooManyExtensions,
        ),
        (
            "an empty extension line",
            cp(o, "5", &root_b64, &["a", "", "b"]),
            CheckpointError::EmptyExtension,
        ),
        (
            "a 257-byte extension line",
            cp(o, "5", &root_b64, &[long_ext.as_str()]),
            CheckpointError::ExtensionTooLong,
        ),
        (
            "a control character in an extension",
            cp(o, "5", &root_b64, &["a\tb"]),
            CheckpointError::ExtensionControl,
        ),
        (
            "size zero with a root that is not the empty tree's",
            cp(o, "0", &root_b64, &[]),
            CheckpointError::EmptyTreeRoot,
        ),
        (
            "no final newline",
            format!("{o}\n5\n{root_b64}"),
            CheckpointError::NoFinalNewline,
        ),
        (
            "two lines",
            format!("{o}\n5\n"),
            CheckpointError::TooFewLines,
        ),
        ("one line", format!("{o}\n"), CheckpointError::TooFewLines),
        ("nothing", String::new(), CheckpointError::NoFinalNewline),
        (
            "over the note limit",
            format!("{o}\n5\n{root_b64}\n{}\n", "x".repeat(MAX_TEXT_BYTES)),
            CheckpointError::TooLarge,
        ),
    ];
    for (what, text, want) in cases {
        assert_eq!(Checkpoint::parse_text(&text), Err(want), "{what}");
    }
    // the edges that are fine: 8 extension lines of 256 bytes, size zero with the empty root, 2^32 leaves
    let max_ext = "e".repeat(MAX_EXTENSION_BYTES);
    let eight: Vec<&str> = (0..MAX_EXTENSIONS).map(|_| max_ext.as_str()).collect();
    assert!(Checkpoint::parse_text(&cp(o, "5", &root_b64, &eight)).is_ok());
    assert!(Checkpoint::parse_text(&cp(o, "0", &empty_root, &[])).is_ok());
    assert!(Checkpoint::parse_text(&cp(o, "4294967296", &root_b64, &[])).is_ok());
    // the constructor holds the same rules
    assert_eq!(
        Checkpoint::new(
            Origin::azlin(),
            TreeSize(5),
            golden_root(),
            vec!["a\nb".into()]
        ),
        Err(CheckpointError::ExtensionControl)
    );
    assert_eq!(
        Checkpoint::new(Origin::azlin(), TreeSize(u64::MAX), golden_root(), vec![]),
        Err(CheckpointError::TooManyLeaves)
    );
}
