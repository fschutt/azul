//! T8-T10: cosignatures.

use azlin_tlog::{Cosigner, Note, NoteError, NoteSig, SigKind, SignError, UnixTime};
use ed25519_dalek::{Signature, VerifyingKey};

use crate::common::{hex_string, key, kname, log_key, t, witness_key, LOG_KEY_NAME};
use crate::notes::{log_verifier, GOLDEN_NOTE, GOLDEN_TEXT};

/// The cosignature of witness `w1` (seed `[9; 32]`) at 1,700,000,000 over [`GOLDEN_TEXT`], made outside the
/// crate (OpenSSL).
const GOLDEN_COSIG_LINE: &str = "\u{2014} witness.example/w1 JLXZ/gAAAABlU/EAg87K2qx1XCwNZ05Vy0wKfnl2m9Xj08J4MivPI1DzaXf/P+YrMg11VifseyzHhfkgm04mUeqbwKJCDQbFvR1OBA==";
const GOLDEN_COSIG_SIGNATURE: &str = "83cecadaac755c2c0d674e55cb4c0a7e79769bd5e3d3c278322bcf2350f36977ff3fe62b320d755627ec7b2cc785f9209b4e2651ea9bc0a2420d06c5bd1d4e04";
const TIME: u64 = 1_700_000_000;

#[test]
fn t8_cosignature_layout() {
    let w = witness_key(0);
    let sig = w.cosign(GOLDEN_TEXT, t(TIME)).unwrap();
    // payload = time (8 bytes, big endian) || signature (64 bytes); the key hash is the type-4 hash
    assert_eq!(sig.payload().len(), 8 + 64);
    let (time, signature) = sig.payload().split_at(8);
    assert_eq!(time, TIME.to_be_bytes());
    assert_eq!(hex_string(time), "000000006553f100");
    assert_eq!(hex_string(sig.key_hash().as_bytes()), "24b5d9fe");
    assert_eq!(sig.name(), &kname("witness.example/w1"));
    // the signed message, built here and not by the library
    let message = format!("cosignature/v1\ntime {TIME}\n{GOLDEN_TEXT}");
    let public = VerifyingKey::from_bytes(&w.public()).unwrap();
    let raw: [u8; 64] = signature.try_into().unwrap();
    public
        .verify_strict(message.as_bytes(), &Signature::from_bytes(&raw))
        .expect("the layout of tlog-cosignature");
    assert_eq!(
        hex_string(signature),
        GOLDEN_COSIG_SIGNATURE,
        "OpenSSL's signature of the same message"
    );
    // the line as a note carries it
    let note = Note::parse(GOLDEN_NOTE.as_bytes())
        .unwrap()
        .with_sig(sig)
        .unwrap();
    let text = String::from_utf8(note.to_bytes()).unwrap();
    assert!(text.ends_with(&format!("{GOLDEN_COSIG_LINE}\n")), "{text}");
    assert_eq!(Note::parse(text.as_bytes()).unwrap(), note);
    // verified as a cosignature it returns the time
    let verified = note
        .verify(&[log_verifier(), w.verifier(SigKind::Cosignature)])
        .unwrap();
    assert_eq!(verified.len(), 2);
    assert_eq!(verified[0].time, None);
    assert_eq!(verified[1].kind, SigKind::Cosignature);
    assert_eq!(verified[1].time, Some(UnixTime::from_secs(TIME)));
    // the time is part of the line for every value, and the text must be a checkpoint text
    for time in [0u64, 1, 255, 256, u64::MAX] {
        let sig = w.cosign(GOLDEN_TEXT, t(time)).unwrap();
        assert_eq!(&sig.payload()[..8], time.to_be_bytes());
        let note = Note::parse(GOLDEN_NOTE.as_bytes())
            .unwrap()
            .with_sig(sig)
            .unwrap();
        let verified = note.verify(&[w.verifier(SigKind::Cosignature)]).unwrap();
        assert_eq!(verified[0].time, Some(t(time)));
    }
    for not_a_checkpoint in [
        "hello\n",
        "",
        "a\nb\nc\n",
        "azlin.io/release-log/v1\n012\nAAAA\n",
    ] {
        assert_eq!(
            w.cosign(not_a_checkpoint, t(TIME)).err(),
            Some(SignError::NotACheckpoint),
            "{not_a_checkpoint:?}"
        );
    }
}

/// The golden note with a cosignature line made by the log's own key (name and seed of the log): the same key
/// used as a log signature (type 0x01) and as a cosignature (type 0x04).
#[test]
fn t9_a_cosignature_is_not_a_log_signature() {
    let k = log_key();
    let log_v = k.verifier(SigKind::Ed25519);
    let cosig_v = k.verifier(SigKind::Cosignature);
    assert_ne!(
        log_v.key_hash(),
        cosig_v.key_hash(),
        "two key hashes for one key"
    );
    assert_eq!(log_v.public, cosig_v.public);
    let base = Note::parse(GOLDEN_NOTE.as_bytes()).unwrap();
    let log_sig = base.sigs()[0].clone();
    let cosig = k.cosign(GOLDEN_TEXT, t(TIME)).unwrap();
    assert_eq!(cosig.key_hash(), cosig_v.key_hash());
    assert_eq!(log_sig.key_hash(), log_v.key_hash());
    // both lines in one note: each key finds its own line
    let both = base.with_sig(cosig.clone()).unwrap();
    assert_eq!(both.verify(std::slice::from_ref(&log_v)).unwrap().len(), 1);
    assert_eq!(
        both.verify(std::slice::from_ref(&cosig_v)).unwrap().len(),
        1
    );
    assert_eq!(
        both.verify(&[log_v.clone(), cosig_v.clone()])
            .unwrap()
            .len(),
        2
    );
    // a cosignature line verified as type 0x01 fails, and the reverse
    let only_cosig = Note::new(GOLDEN_TEXT.to_string(), vec![cosig.clone()]).unwrap();
    assert_eq!(
        only_cosig.verify(std::slice::from_ref(&log_v)),
        Err(NoteError::NoKnownSignature)
    );
    assert_eq!(
        base.verify(std::slice::from_ref(&cosig_v)),
        Err(NoteError::NoKnownSignature)
    );
    // relabelled lines: the cosignature (72 bytes) under the type-1 hash, the log signature (64 bytes) under
    // the type-4 hash: the key hash matches, so the signature is checked, and it is refused
    let as_log = NoteSig::new(
        kname(LOG_KEY_NAME),
        log_v.key_hash(),
        cosig.payload().to_vec(),
    )
    .unwrap();
    let relabelled = Note::new(GOLDEN_TEXT.to_string(), vec![as_log]).unwrap();
    assert!(matches!(
        relabelled.verify(std::slice::from_ref(&log_v)),
        Err(NoteError::BadSignature { .. })
    ));
    let as_cosig = NoteSig::new(
        kname(LOG_KEY_NAME),
        cosig_v.key_hash(),
        log_sig.payload().to_vec(),
    )
    .unwrap();
    let relabelled = Note::new(GOLDEN_TEXT.to_string(), vec![as_cosig]).unwrap();
    assert!(matches!(
        relabelled.verify(std::slice::from_ref(&cosig_v)),
        Err(NoteError::BadSignature { .. })
    ));
    // the right length, the wrong message: a time of zero in front of the log's signature of the text, and the
    // cosignature's signature bytes alone as a log signature
    let mut with_time = vec![0u8; 8];
    with_time.extend_from_slice(log_sig.payload());
    let as_cosig = NoteSig::new(kname(LOG_KEY_NAME), cosig_v.key_hash(), with_time).unwrap();
    let relabelled = Note::new(GOLDEN_TEXT.to_string(), vec![as_cosig]).unwrap();
    assert!(matches!(
        relabelled.verify(&[cosig_v]),
        Err(NoteError::BadSignature { .. })
    ));
    let as_log = NoteSig::new(
        kname(LOG_KEY_NAME),
        log_v.key_hash(),
        cosig.payload()[8..].to_vec(),
    )
    .unwrap();
    let relabelled = Note::new(GOLDEN_TEXT.to_string(), vec![as_log]).unwrap();
    assert!(matches!(
        relabelled.verify(&[log_v]),
        Err(NoteError::BadSignature { .. })
    ));
}

#[test]
fn t10_the_cosignature_time_is_signed() {
    let w = witness_key(0);
    let v = w.verifier(SigKind::Cosignature);
    let good = w.cosign(GOLDEN_TEXT, t(TIME)).unwrap();
    let base = Note::parse(GOLDEN_NOTE.as_bytes()).unwrap();
    let with = |sig: NoteSig| {
        Note::new(GOLDEN_TEXT.to_string(), vec![base.sigs()[0].clone(), sig]).unwrap()
    };
    assert!(with(good.clone())
        .verify(&[log_verifier(), v.clone()])
        .is_ok());
    // any bit of the 8 time bytes
    for byte in 0..8 {
        for bit in [0u8, 3, 7] {
            let mut payload = good.payload().to_vec();
            payload[byte] ^= 1 << bit;
            let bad = NoteSig::new(good.name().clone(), good.key_hash(), payload).unwrap();
            assert_eq!(
                with(bad).verify(&[log_verifier(), v.clone()]),
                Err(NoteError::BadSignature {
                    name: kname("witness.example/w1")
                }),
                "time byte {byte} bit {bit}"
            );
        }
    }
    // one second more or less, as a witness that lies about the time would write it
    for time in [TIME - 1, TIME + 1, 0, u64::MAX] {
        let mut payload = good.payload().to_vec();
        payload[..8].copy_from_slice(&time.to_be_bytes());
        let bad = NoteSig::new(good.name().clone(), good.key_hash(), payload).unwrap();
        assert!(matches!(
            with(bad).verify(&[log_verifier(), v.clone()]),
            Err(NoteError::BadSignature { .. })
        ));
    }
    // the signature bytes and the text are covered as well
    let mut payload = good.payload().to_vec();
    payload[40] ^= 1;
    let bad = NoteSig::new(good.name().clone(), good.key_hash(), payload).unwrap();
    assert!(matches!(
        with(bad).verify(std::slice::from_ref(&v)),
        Err(NoteError::BadSignature { .. })
    ));
    let other_text = GOLDEN_TEXT.replacen("12", "13", 1);
    let moved = Note::new(other_text, vec![good.clone()]).unwrap();
    assert!(matches!(
        moved.verify(&[v]),
        Err(NoteError::BadSignature { .. })
    ));
    // another witness's key does not verify this witness's line even under its own name: the name and hash
    // are those of the first witness
    let impostor = key("witness.example/w1", 99)
        .cosign(GOLDEN_TEXT, t(TIME))
        .unwrap();
    assert_ne!(
        impostor.key_hash(),
        good.key_hash(),
        "the key hash holds the public key"
    );
}
