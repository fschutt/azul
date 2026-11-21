//! C9 (the text forms: the contact key, the sealed share, the printed share, the recovery request
//! and the reply) of lane W1-S15d.
//!
//! Implements planning/architecture.md section 5.5 ("its 2-of-3 Shamir shares to contacts") and
//! section 7.5 ("the contacts' shares"). The known answers are rows of `tests/vectors/contacts.json`
//! (made by the independent program of the lane); the forms are today's `crypto/contacts.rs` forms
//! with the changes the drift register names (X-DELTA-W1S15D-7, -13).

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code, clippy::needless_range_loop)]

mod recovery_common;

use azlin_keys::envelope::{Alg, Ctx, Envelope, RecipientSecret, RecoveryMethod};
use azlin_keys::recovery::{
    combine, new_contact_secret, printed_share_from_text, printed_share_text, reply_from_text,
    reply_text, share_from_text, share_text, split, ContactKey, RecoveryCode, RecoveryError,
    RequestKey, Share, CONTACT_PREFIX, MAX_TEXT_LEN, REPLY_PREFIX, REQUEST_PREFIX, SHARE_PREFIX,
};
use azlin_keys::{EntropyError, FailingEntropy, Secret, SeededEntropy};
use recovery_common::vectors::{self, row, Row};
use recovery_common::{
    from_hex, input, kat_code, kat_contact_secret, pat, pat16, pat32, to_hex, word,
    ScriptedEntropy, SplitMix64,
};

/// The text a row holds (its bytes are ASCII).
fn text_of(r: &Row) -> String {
    String::from_utf8(from_hex(&r.hex)).expect("an ASCII text")
}

fn rows_of_form<'a>(rows: &'a [Row], form: &'a str) -> impl Iterator<Item = &'a Row> {
    rows.iter()
        .filter(move |r| input(&r.inputs, "form") == form)
}

fn kat_envelope(name: &str) -> Envelope {
    let rows = vectors::read("recovery_wrap.json");
    Envelope::from_cbor(&from_hex(&row(&rows, name).hex)).expect("an envelope")
}

/// C9: the contact key: `azlin-contact:<64 hex>` (and `:<2368 hex>` with an ML-KEM key) parses and
/// prints byte for byte; a wrong prefix, an odd or wrong number of digits, upper case hex and a
/// digit that is none are each refused with their word.
#[test]
fn c9_contact_text_forms() {
    let rows = vectors::read("contacts.json");
    let mut seen = 0;
    for r in rows_of_form(&rows, "contact") {
        let text = text_of(r);
        if r.valid {
            let key = ContactKey::from_text(&text).unwrap_or_else(|e| panic!("{}: {e:?}", r.name));
            assert_eq!(key.to_text(), r.expect, "{}", r.name);
            assert_eq!(
                key.kx(),
                kat_contact_secret(false).kx_public(),
                "{}",
                r.name
            );
            assert_eq!(key.pq().is_some(), r.name == "contact_hybrid", "{}", r.name);
            if r.name == "contact_hybrid" && cfg!(feature = "pq") {
                assert_eq!(
                    key.pq(),
                    kat_contact_secret(true).pq_public(),
                    "the ML-KEM key of the seed"
                );
                assert_eq!(key, ContactKey::of_secret(&kat_contact_secret(true)));
            }
            // The key's recipient names the bundle it was sealed to.
            assert_eq!(
                key.key_id(),
                ContactKey::new(*key.kx(), key.pq().map(<[u8]>::to_vec))
                    .expect("a key")
                    .key_id()
            );
        } else {
            let error = ContactKey::from_text(&text).expect_err(&r.name);
            assert_eq!(word(&error), r.expect, "{}", r.name);
        }
        seen += 1;
    }
    assert!(seen >= 12, "{seen} contact rows");
    assert_eq!(CONTACT_PREFIX, "azlin-contact:");
}

/// C9: the recovery request: `azlin-recover:<64 hex>`; a contact key is no request and a request is no
/// contact key.
#[test]
fn c9_request_text_forms() {
    let rows = vectors::read("contacts.json");
    let request_pub = *RecipientSecret::new(&Secret::from_bytes(pat32(0x70)), None).kx_public();
    let mut seen = 0;
    for r in rows_of_form(&rows, "request") {
        let text = text_of(r);
        if r.valid {
            let key = RequestKey::from_text(&text).unwrap_or_else(|e| panic!("{}: {e:?}", r.name));
            assert_eq!(key.to_text(), r.expect, "{}", r.name);
            assert_eq!(key.kx(), &request_pub);
            assert_eq!(key, RequestKey::new(request_pub));
            assert_eq!(key.as_contact().kx(), &request_pub);
            assert!(key.as_contact().pq().is_none());
        } else {
            let error = RequestKey::from_text(&text).expect_err(&r.name);
            assert_eq!(word(&error), r.expect, "{}", r.name);
        }
        seen += 1;
    }
    assert!(seen >= 3, "{seen} request rows");
    assert_eq!(REQUEST_PREFIX, "azlin-recover:");
}

/// C9: the sealed share and the reply: the hex of an algorithm 5 envelope under their prefixes; the
/// text round trips to the same envelope; an envelope of another algorithm, a truncated one, odd,
/// upper case or foreign digits and a wrong prefix are refused.
#[test]
fn c9_sealed_share_and_reply_text_forms() {
    let rows = vectors::read("contacts.json");
    let sealed = kat_envelope("alg5_over_alg1_share_2_of_3");
    let mut seen = 0;
    for (form, parse, print) in [
        (
            "share",
            share_from_text as fn(&str) -> Result<Envelope, RecoveryError>,
            share_text as fn(&Envelope) -> Result<String, RecoveryError>,
        ),
        ("reply", reply_from_text, reply_text),
    ] {
        for r in rows_of_form(&rows, form) {
            let text = text_of(r);
            if r.valid {
                let env = parse(&text).unwrap_or_else(|e| panic!("{}: {e:?}", r.name));
                assert_eq!(print(&env).expect("a text"), r.expect, "{}", r.name);
                assert_eq!(env, sealed, "{}: the envelope of the vector file", r.name);
            } else {
                let error = parse(&text).expect_err(&r.name);
                assert_eq!(word(&error), r.expect, "{}", r.name);
            }
            seen += 1;
        }
    }
    assert!(seen >= 8, "{seen} share and reply rows");
    assert_eq!(SHARE_PREFIX, "azlin-share:");
    assert_eq!(REPLY_PREFIX, "azlin-share-reply:");
    // The text is the prefix and the hex of the envelope's canonical bytes, nothing else.
    let bytes = sealed.to_cbor().expect("bytes");
    assert_eq!(
        share_text(&sealed).expect("a text"),
        format!("azlin-share:{}", to_hex(&bytes))
    );
    assert_eq!(
        reply_text(&sealed).expect("a text"),
        format!("azlin-share-reply:{}", to_hex(&bytes))
    );
    // A share's text is not a reply's, and the other way round.
    let as_share = share_text(&sealed).expect("a text");
    assert_eq!(
        reply_from_text(&as_share).err(),
        Some(RecoveryError::TextPrefix)
    );
    assert_eq!(
        share_from_text(&reply_text(&sealed).expect("a text")).err(),
        Some(RecoveryError::TextPrefix)
    );
    // Only an algorithm 5 envelope has such a text.
    let prf = kat_envelope("alg4_32_bytes");
    assert_eq!(share_text(&prf), Err(RecoveryError::WrongAlg(Alg::Prf)));
    assert_eq!(reply_text(&prf), Err(RecoveryError::WrongAlg(Alg::Prf)));
    let code = kat_envelope("alg3_test_32_bytes");
    assert_eq!(
        share_text(&code),
        Err(RecoveryError::WrongAlg(Alg::Argon2id))
    );
}

/// C9: the printed share `S<x>-<k>-<n>-<split id>-<the share as a recovery code's text>` parses and
/// prints byte for byte, is read as people type it, and its refusals have their words.
#[test]
fn c9_printed_share_text_forms() {
    let rows = vectors::read("contacts.json");
    let mut seen = 0;
    for r in rows_of_form(&rows, "printed") {
        let text = text_of(r);
        if r.valid {
            let share =
                printed_share_from_text(&text).unwrap_or_else(|e| panic!("{}: {e:?}", r.name));
            assert_eq!(printed_share_text(&share).as_str(), r.expect, "{}", r.name);
            if r.inputs.contains(" x=") {
                assert_eq!(share.x.to_string(), input(&r.inputs, "x"), "{}", r.name);
                assert_eq!(share.k.to_string(), input(&r.inputs, "k"), "{}", r.name);
                assert_eq!(share.n.to_string(), input(&r.inputs, "n"), "{}", r.name);
                assert_eq!(
                    to_hex(&share.split_id),
                    input(&r.inputs, "split_id"),
                    "{}",
                    r.name
                );
                assert_eq!(
                    to_hex(share.y.expose()),
                    input(&r.inputs, "y"),
                    "{}",
                    r.name
                );
            }
        } else {
            let error = printed_share_from_text(&text).expect_err(&r.name);
            assert_eq!(word(&error), r.expect, "{}", r.name);
        }
        seen += 1;
    }
    assert!(seen >= 14, "{seen} printed share rows");
}

/// C9: every share of a split prints and parses back (indices up to 255, thresholds up to 255), the
/// text has the shape `S<x>-<k>-<n>-<8 upper case hex digits>-` and a recovery code's text, and
/// printed shares recombine to the code.
#[test]
fn c9_the_printed_share_round_trips() {
    let mut ent = SeededEntropy::from_u64(9);
    for (k, n) in [(2u8, 3u8), (3, 5), (5, 12), (2, 255), (255, 255), (7, 9)] {
        let code = RecoveryCode::generate(&mut ent).expect("a code");
        let shares = split(code.as_secret(), k, n, &mut ent).expect("a split");
        let mut parsed = Vec::new();
        for share in &shares {
            let text = printed_share_text(share);
            let prefix = format!("S{}-{}-{}-", share.x, share.k, share.n);
            assert!(text.starts_with(&prefix), "{}", text.as_str());
            let rest = &text[prefix.len()..];
            let (id, code_text) = rest.split_at(8);
            assert_eq!(id, to_hex(&share.split_id).to_uppercase());
            assert!(code_text.starts_with('-'));
            let groups: Vec<usize> = code_text[1..].split('-').map(str::len).collect();
            assert_eq!(groups, [5, 5, 5, 5, 6]);
            let back = printed_share_from_text(&text)
                .unwrap_or_else(|e| panic!("{}: {e:?}", text.as_str()));
            assert_eq!(
                (back.x, back.k, back.n, back.split_id),
                (share.x, share.k, share.n, share.split_id)
            );
            assert!(back.y.ct_eq(&share.y));
            // As typed: lower case, spaces, `o` for 0 and `l` for 1 in the id and the symbols.
            let typed = text.to_lowercase().replace('-', " ");
            let typed_back =
                printed_share_from_text(&typed).unwrap_or_else(|e| panic!("{typed}: {e:?}"));
            assert!(typed_back.y.ct_eq(&share.y));
            parsed.push(back);
        }
        // The first k printed shares give the code back.
        let first_k: Vec<Share> = parsed
            .iter()
            .take(usize::from(k))
            .map(Share::duplicate)
            .collect();
        let recovered = combine(&first_k).expect("k printed shares");
        assert!(
            RecoveryCode::from_secret(recovered).ct_eq(&code),
            "{k} of {n}"
        );
    }
}

/// C9: contact keys of the two kinds, requests and sealed shares made by the code round trip through
/// their text, have the lengths of their form, and a contact key is one per owner: a contact's app
/// that makes a key for each owner makes different keys (two owners cannot tell they share a contact).
#[test]
fn c9_generated_keys_round_trip_and_a_contact_key_is_one_per_owner() {
    let mut ent = SeededEntropy::from_u64(99);
    let mut kinds = vec![false];
    if cfg!(feature = "pq") {
        kinds.push(true);
    }
    for with_pq in kinds {
        let for_owner_a = new_contact_secret(&mut ent, with_pq).expect("a contact key");
        let for_owner_b = new_contact_secret(&mut ent, with_pq).expect("a contact key");
        let (a, b) = (
            ContactKey::of_secret(&for_owner_a),
            ContactKey::of_secret(&for_owner_b),
        );
        assert_ne!(a, b, "two owners' contact keys of one contact differ");
        assert_ne!(a.to_text(), b.to_text());
        assert_ne!(a.kx(), b.kx());
        assert_ne!(a.key_id(), b.key_id());
        for key in [&a, &b] {
            let text = key.to_text();
            assert_eq!(
                text.len(),
                if with_pq { 14 + 64 + 1 + 2368 } else { 14 + 64 }
            );
            assert_eq!(ContactKey::from_text(&text).as_ref(), Ok(key));
            assert_eq!(key.pq().is_some(), with_pq);
            assert_eq!(key.recipient().key_id(), key.key_id());
        }
        // The same entropy gives the same key: the draws are the 32-byte secret, then the 64-byte seed.
        let mut scripted = ScriptedEntropy::new([pat(1, 32), pat(50, 64)].concat());
        let secret = new_contact_secret(&mut scripted, with_pq).expect("a key");
        assert_eq!(
            scripted.draws,
            if with_pq { vec![32, 64] } else { vec![32] }
        );
        assert_eq!(
            secret.kx_public(),
            RecipientSecret::new(&Secret::from_bytes(pat32(1)), None).kx_public()
        );
    }
    // A failing source makes no key.
    assert_eq!(
        new_contact_secret(&mut FailingEntropy::after(31), false).err(),
        Some(RecoveryError::Entropy(EntropyError::Exhausted))
    );
    // The request key of a recovering computer is a key like that, X25519 only.
    let request = RequestKey::new(
        *new_contact_secret(&mut ent, false)
            .expect("a key")
            .kx_public(),
    );
    assert_eq!(request.to_text().len(), 14 + 64);
    assert_eq!(RequestKey::from_text(&request.to_text()), Ok(request));
}

/// C9: the limits and the odd inputs of every parser: a text over `MAX_TEXT_LEN`, white space, the
/// wrong case of the prefix, a key of the wrong length, and text that is no ASCII.
#[test]
fn c9_the_parsers_have_limits_and_refuse_odd_inputs() {
    let contact = ContactKey::of_secret(&kat_contact_secret(false)).to_text();
    let request = RequestKey::new(*kat_contact_secret(false).kx_public()).to_text();
    let long = format!("{contact}{}", "0".repeat(MAX_TEXT_LEN));
    assert_eq!(
        ContactKey::from_text(&long).err(),
        Some(RecoveryError::TextTooLong)
    );
    assert_eq!(
        RequestKey::from_text(&long).err(),
        Some(RecoveryError::TextTooLong)
    );
    assert_eq!(
        share_from_text(&long).err(),
        Some(RecoveryError::TextTooLong)
    );
    assert_eq!(
        reply_from_text(&long).err(),
        Some(RecoveryError::TextTooLong)
    );
    assert_eq!(
        printed_share_from_text(&"S".repeat(300)).err(),
        Some(RecoveryError::TextTooLong)
    );
    // A share's text with more hex than any envelope has.
    let big = format!("{SHARE_PREFIX}{}", "00".repeat(5_000));
    assert_eq!(
        share_from_text(&big).err(),
        Some(RecoveryError::TextTooLong)
    );
    // The prefix is exact: another case, a missing colon, a space inside it.
    for bad in [
        contact.to_uppercase(),
        contact.replace("azlin-", "Azlin-"),
        contact.replace(':', ""),
        contact.replace("contact", "con tact"),
        String::new(),
        "azlin-contact".to_string(),
    ] {
        assert!(ContactKey::from_text(&bad).is_err(), "{bad:?}");
    }
    // White space around the text and inside the digits of a request or a contact key is fine, as a
    // pasted text has it; between the prefix and the digits too.
    assert_eq!(
        RequestKey::from_text(&format!(" \n{request}\t")),
        RequestKey::from_text(&request)
    );
    assert!(RequestKey::from_text(&request.replace("recover:", "recover: ")).is_ok());
    let split_digits = format!("{}\n{}", &contact[..40], &contact[40..]);
    assert_eq!(
        ContactKey::from_text(&split_digits),
        ContactKey::from_text(&contact)
    );
    // Non-ASCII text is a digit that is none, or a wrong length.
    assert!(ContactKey::from_text(&contact.replace("azlin-contact:", "azlin-contact:é")).is_err());
    assert!(RequestKey::from_text(&format!("{request}€")).is_err());
    assert!(printed_share_from_text("S2-2-3-é-é").is_err());
    // A contact key that is not 32 bytes, or whose ML-KEM key is not 1,184.
    assert_eq!(
        ContactKey::new([1u8; 32], Some(vec![0u8; 1183])).err(),
        Some(RecoveryError::TextLength)
    );
    assert_eq!(
        ContactKey::new([1u8; 32], Some(Vec::new())).err(),
        Some(RecoveryError::TextLength)
    );
    assert!(ContactKey::new([1u8; 32], Some(vec![0u8; 1184])).is_ok());
    assert!(ContactKey::new([1u8; 32], None).is_ok());
}
