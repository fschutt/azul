//! C3 (every field is authenticated), C4 (the context binds), C5 (the signature rules), C7 (low
//! order points) and C8 (a wrong recipient never opens).
//!
//! The second half of C3, "the AEAD alone also fails on every flip", needs the private
//! decryption step and lives in `src/envelope/internal_tests.rs`.
//!
//! Implements planning/architecture.md section 5.5 (Envelope v2: `ctx` is the associated data,
//! "Envelopes carry the signer"; Epochs must be authorized).

#![cfg(feature = "test-support")]
#![allow(unused_imports, dead_code)]

mod envelope_common;

use std::cell::Cell;

use azlin_keys::envelope::{
    check_shape, open, verify_signature, wrap_hybrid, wrap_x25519, Alg, Ctx, CtxError, CtxKind,
    Envelope, EnvelopeError, OpenError, Recipient, RecipientHybrid, RecipientSecret,
    RecipientX25519, RecoveryMethod, WrapError,
};
use azlin_keys::sign::{self, signing_input, ENVELOPE_V2};
use azlin_keys::{
    AccountId, DeviceDirectory, DeviceId, DeviceState, DirectoryEntry, DirectoryError, Epoch,
    GroupId, MapDirectory, MemberId, Secret, SeededEntropy, SignKeypair, Signature, SpaceId,
    UnixTime, VerifyAt, VerifyKey,
};
use envelope_common::{from_hex, pat, pat16, pat32, to_hex, Fixture};

// ---------------------------------------------------------------------------------------------
// C3: every field is authenticated (the wire-level half)
// ---------------------------------------------------------------------------------------------

/// What a refused flip may be: the reader's refusal, or one of the documented answers of `open`.
fn documented(e: &OpenError) -> bool {
    matches!(
        e,
        OpenError::CtxMismatch
            | OpenError::NotForMe
            | OpenError::UnknownSigner
            | OpenError::BadSignature
            | OpenError::Malformed(_)
    )
}

/// Flips each bit of `bits` of `bytes` in turn and requires that the result is refused, by the
/// reader or by `open`, with a documented error: never a panic and never success. The text of
/// every refusal is kept for the scan of check C14.
fn every_flip_is_refused(
    f: &Fixture,
    bytes: &[u8],
    bits: impl Iterator<Item = usize>,
    texts: &mut Vec<String>,
) -> usize {
    let mut count = 0;
    for bit in bits {
        let mut flipped = bytes.to_vec();
        flipped[bit / 8] ^= 1 << (bit % 8);
        match Envelope::from_cbor(&flipped) {
            Err(e) => {
                assert_ne!(
                    e,
                    EnvelopeError::NotCanonical,
                    "bit {bit}: a decoded envelope must always write back"
                );
                texts.push(e.to_string());
                texts.push(format!("{e:?}"));
            }
            Ok(envelope) => match f.open(&envelope) {
                Err(e) => {
                    assert!(
                        documented(&e),
                        "bit {bit}: {e:?} is not a documented answer"
                    );
                    texts.push(e.to_string());
                    texts.push(format!("{e:?}"));
                }
                Ok(_) => panic!("bit {bit}: a flipped envelope opened"),
            },
        }
        count += 1;
    }
    count
}

/// The texts of the refusals show neither the plaintext nor a key: the plaintext of the test is
/// 32 distinctive bytes and the keys are the fixture's patterns.
fn no_secret_in(texts: &[String], plaintext: &[u8], secrets: &[Vec<u8>]) {
    let mut needles = vec![to_hex(plaintext)];
    needles.extend(secrets.iter().map(|s| to_hex(s)));
    for text in texts {
        for needle in &needles {
            assert!(!text.contains(needle.as_str()), "{text:?} shows {needle}");
        }
    }
}

#[test]
fn c3_every_bit_of_an_alg1_envelope_is_authenticated() {
    let f = Fixture::new(3);
    let plaintext = pat(0xa0, 32);
    let env = f.wrap1(&plaintext, 31);
    let bytes = env.to_cbor().expect("bytes");
    let mut texts = Vec::new();
    // EVERY bit of rcpt, epk, nonce, ct, ctx, signer, alg, sig and the framing around them.
    let count = every_flip_is_refused(&f, &bytes, 0..bytes.len() * 8, &mut texts);
    assert_eq!(count, bytes.len() * 8);
    no_secret_in(
        &texts,
        &plaintext,
        &[pat(3, 32), pat(3 ^ 0x5a, 64), pat(3 ^ 0x77, 32)],
    );
}

/// Algorithm 2: every bit but the ML-KEM ciphertext (one bit in each of its 1,088 bytes here,
/// every bit of it in the second half of check C3, `internal_tests.rs`, and in the signature).
#[cfg(feature = "pq")]
#[test]
fn c3_every_bit_of_an_alg2_envelope_is_authenticated() {
    let f = Fixture::new(3);
    let plaintext = pat(0xa0, 32);
    let env = f.wrap2(&plaintext, 32);
    let bytes = env.to_cbor().expect("bytes");
    let kem_ct = env.kem_ct.clone().expect("a kem_ct");
    let start = bytes
        .windows(kem_ct.len())
        .position(|w| w == kem_ct.as_slice())
        .expect("the KEM ciphertext is in the bytes");
    let end = start + kem_ct.len();
    let mut texts = Vec::new();
    let outside = (0..bytes.len() * 8).filter(|bit| !(start..end).contains(&(bit / 8)));
    let inside = (start..end).map(|byte| byte * 8 + byte % 8);
    every_flip_is_refused(&f, &bytes, outside, &mut texts);
    every_flip_is_refused(&f, &bytes, inside, &mut texts);
    no_secret_in(
        &texts,
        &plaintext,
        &[pat(3, 32), pat(3 ^ 0x5a, 64), pat(3 ^ 0x77, 32)],
    );
}

// ---------------------------------------------------------------------------------------------
// C4: the context binds
// ---------------------------------------------------------------------------------------------

/// A directory that counts the questions it is asked: a refusal that comes before the signature
/// check never asks.
struct CountingDirectory<'a> {
    inner: &'a MapDirectory,
    calls: Cell<usize>,
}

impl<'a> CountingDirectory<'a> {
    fn new(inner: &'a MapDirectory) -> CountingDirectory<'a> {
        CountingDirectory {
            inner,
            calls: Cell::new(0),
        }
    }
}

impl DeviceDirectory for CountingDirectory<'_> {
    fn verify_key(&self, device: &DeviceId, at: VerifyAt) -> Result<VerifyKey, DirectoryError> {
        self.calls.set(self.calls.get() + 1);
        self.inner.verify_key(device, at)
    }
}

#[test]
fn c4_ctx_binding_prevents_cross_use() {
    let f = Fixture::new(4);
    let env = f.wrap1(b"a space key", 41);
    let epoch4 = Ctx::space_epoch(&f.space, Epoch::new(4).expect("epoch 4"));
    let other_space = Ctx::space_epoch(
        &SpaceId::from_bytes(pat16(0xee)),
        Epoch::new(3).expect("epoch 3"),
    );
    // `group/A/epoch/3`: the same id bytes and epoch as `space/A/epoch/3`, another kind.
    let group = Ctx::group_epoch(
        &GroupId::from_bytes(*f.space.as_bytes()),
        Epoch::new(3).expect("epoch 3"),
    );
    let chain = Ctx::chain(&f.space, Epoch::new(3).expect("epoch 3"));
    for expect in [&epoch4, &other_space, &group, &chain] {
        let counting = CountingDirectory::new(&f.dir);
        assert_eq!(
            open(&env, &f.me, expect, &counting, VerifyAt::Now).err(),
            Some(OpenError::CtxMismatch),
            "{expect}"
        );
        assert_eq!(
            counting.calls.get(),
            0,
            "{expect}: refused before the signature was checked, so before any decryption"
        );
    }
    // The exact context opens.
    assert!(f.open(&env).is_ok());

    // The order of the checks: the context first, then the recipient, then the signature.
    let counting = CountingDirectory::new(&f.dir);
    assert_eq!(
        open(&env, &f.stranger, &epoch4, &counting, VerifyAt::Now).err(),
        Some(OpenError::CtxMismatch),
        "a wrong context and a wrong recipient: the context is named"
    );
    let mut forged = env.clone();
    forged.sig = Signature::from_bytes([0u8; 64]);
    assert_eq!(
        open(&forged, &f.me, &epoch4, &counting, VerifyAt::Now).err(),
        Some(OpenError::CtxMismatch),
        "a wrong context and a forged signature: the context is named"
    );
    assert_eq!(
        open(&forged, &f.stranger, &f.ctx, &counting, VerifyAt::Now).err(),
        Some(OpenError::NotForMe),
        "a wrong recipient and a forged signature: the recipient is named"
    );
    assert_eq!(
        counting.calls.get(),
        0,
        "no signature was ever checked above"
    );
    assert_eq!(
        open(&forged, &f.me, &f.ctx, &counting, VerifyAt::Now).err(),
        Some(OpenError::BadSignature)
    );
    assert_eq!(counting.calls.get(), 1);
}

const ID: &str = "00112233445566778899aabbccddeeff";

#[test]
fn c4_every_ctx_constructor_makes_one_canonical_text_that_parses_back() {
    let space = SpaceId::from_bytes(pat16(0x00));
    let group = GroupId::from_bytes(pat16(0x10));
    let member = MemberId::from_bytes(pat16(0x20));
    let account = AccountId::from_bytes(pat16(0x30));
    let e = |n: u32| Epoch::new(n).expect("a nonzero epoch");
    let cases: Vec<(Ctx, String, CtxKind)> = vec![
        (
            Ctx::space_epoch(&space, e(3)),
            format!("space/{space}/epoch/3"),
            CtxKind::Space,
        ),
        (
            Ctx::group_epoch(&group, e(1)),
            format!("group/{group}/epoch/1"),
            CtxKind::Group,
        ),
        (
            Ctx::member_key(&member),
            format!("member/{member}/key"),
            CtxKind::Member,
        ),
        (
            Ctx::chain(&space, e(4_294_967_295)),
            format!("chain/{space}/epoch/4294967295"),
            CtxKind::Chain,
        ),
        (
            Ctx::seal(&account, e(12)),
            format!("seal/{account}/epoch/12"),
            CtxKind::Seal,
        ),
        (
            Ctx::escrow(&member),
            format!("escrow/{member}"),
            CtxKind::Escrow,
        ),
        (
            Ctx::recovery(&member, RecoveryMethod::Code),
            format!("recovery/{member}/code"),
            CtxKind::Recovery,
        ),
        (
            Ctx::recovery(&member, RecoveryMethod::Prf),
            format!("recovery/{member}/prf"),
            CtxKind::Recovery,
        ),
        (
            Ctx::recovery(&member, RecoveryMethod::Share),
            format!("recovery/{member}/share"),
            CtxKind::Recovery,
        ),
    ];
    for (ctx, text, kind) in cases {
        assert_eq!(ctx.as_str(), text);
        assert_eq!(ctx.to_string(), text);
        assert_eq!(ctx.kind(), kind);
        assert_eq!(Ctx::parse(&text), Ok(ctx.clone()), "{text}");
        assert_eq!(text.parse::<Ctx>(), Ok(ctx.clone()));
        assert!(ctx.as_str().len() <= azlin_keys::envelope::MAX_CTX_LEN);
    }
    // The accessors.
    let c = Ctx::chain(&space, e(7));
    assert_eq!(c.id_bytes(), space.as_bytes());
    assert_eq!(c.epoch(), Some(e(7)));
    assert_eq!(c.method(), None);
    let r = Ctx::recovery(&member, RecoveryMethod::Share);
    assert_eq!(r.method(), Some(RecoveryMethod::Share));
    assert_eq!(r.epoch(), None);
    assert_eq!(Ctx::member_key(&member).epoch(), None);
}

#[test]
fn c4_the_ctx_parser_is_strict() {
    let bad: Vec<(String, CtxError)> = vec![
        // Uppercase anywhere.
        (format!("Space/{ID}/epoch/3"), CtxError::BadCharacter),
        (
            format!("space/{}/epoch/3", ID.to_uppercase()),
            CtxError::BadCharacter,
        ),
        // Characters outside letters, digits and `/`: dots, signs, space, newline, non-ASCII.
        ("space/../epoch/3".into(), CtxError::BadCharacter),
        (format!("space/{ID}/epoch/-1"), CtxError::BadCharacter),
        (format!("space/{ID}/epoch/+1"), CtxError::BadCharacter),
        (format!("space/{ID}/epoch/3 "), CtxError::BadCharacter),
        (format!("space/{ID}/epoch/3\n"), CtxError::BadCharacter),
        (format!("space/{ID}/epoch/\u{e9}"), CtxError::BadCharacter),
        (format!("space/{ID}/epoch/\u{663}"), CtxError::BadCharacter),
        // Empty segments.
        (String::new(), CtxError::EmptySegment),
        (format!("space/{ID}/epoch/"), CtxError::EmptySegment),
        (format!("space/{ID}//3"), CtxError::EmptySegment),
        ("space//epoch/3".into(), CtxError::EmptySegment),
        (format!("/space/{ID}/epoch/3"), CtxError::EmptySegment),
        (format!("space/{ID}/epoch/3/"), CtxError::EmptySegment),
        // Epochs that are not plain decimal numbers from 1 to 4,294,967,295.
        (format!("space/{ID}/epoch/0"), CtxError::BadEpoch),
        (format!("space/{ID}/epoch/03"), CtxError::BadEpoch),
        (format!("space/{ID}/epoch/4294967296"), CtxError::BadEpoch),
        (format!("space/{ID}/epoch/x"), CtxError::BadEpoch),
        (format!("space/{ID}/epoch/1e3"), CtxError::BadEpoch),
        // Ids that are not 32 lowercase hex digits.
        (format!("space/{}/epoch/3", &ID[..30]), CtxError::BadId),
        (format!("space/{ID}00/epoch/3"), CtxError::BadId),
        (format!("space/{}g/epoch/3", &ID[..31]), CtxError::BadId),
        // The right kind with the wrong segments.
        (format!("space/{ID}"), CtxError::BadShape),
        (format!("space/{ID}/epoch/3/extra"), CtxError::BadShape),
        (format!("space/{ID}/era/3"), CtxError::BadShape),
        (format!("member/{ID}/keys"), CtxError::BadShape),
        (format!("member/{ID}"), CtxError::BadShape),
        (format!("escrow/{ID}/code"), CtxError::BadShape),
        (format!("recovery/{ID}"), CtxError::BadShape),
        ("space".into(), CtxError::BadShape),
        // Unknown kinds and methods.
        (format!("moon/{ID}/epoch/3"), CtxError::UnknownKind),
        (format!("recovery/{ID}/other"), CtxError::BadMethod),
        (format!("recovery/{ID}/Code"), CtxError::BadCharacter),
        // The length limit: 129 bytes is too long, whatever else is wrong with it.
        ("a".repeat(129), CtxError::TooLong),
        (
            format!("space/{ID}/epoch/{}", "1".repeat(100)),
            CtxError::TooLong,
        ),
        // 128 bytes are not too long, and this is no context either.
        ("a".repeat(128), CtxError::UnknownKind),
    ];
    for (text, error) in bad {
        assert_eq!(Ctx::parse(&text).err(), Some(error), "{text:?}");
    }
}

// ---------------------------------------------------------------------------------------------
// C5: the signature rules
// ---------------------------------------------------------------------------------------------

/// The group order L of the Ed25519 base point, little-endian.
const L_LE: [u8; 32] = [
    0xed, 0xd3, 0xf5, 0x5c, 0x1a, 0x63, 0x12, 0x58, 0xd6, 0x9c, 0xf7, 0xa2, 0xde, 0xf9, 0xde, 0x14,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x10,
];

/// The signature with `S + L` in place of `S`: the same equation holds, so a verifier that does
/// not demand a canonical `S` accepts it (malleability).
fn add_l(sig: &Signature) -> Signature {
    let bytes = sig.as_bytes();
    let mut out = *bytes;
    let mut carry = 0u16;
    for ((o, s), l) in out[32..].iter_mut().zip(&bytes[32..]).zip(L_LE.iter()) {
        let sum = u16::from(*s) + u16::from(*l) + carry;
        *o = sum as u8;
        carry = sum >> 8;
    }
    assert_eq!(carry, 0, "S + L fits in 32 bytes for every S below L");
    Signature::from_bytes(out)
}

#[test]
fn c5_each_signature_failure_is_its_own_error() {
    let f = Fixture::new(5);
    let env = f.wrap1(b"a space key", 51);
    assert_eq!(verify_signature(&env, &f.dir, VerifyAt::Now), Ok(()));

    let other_key = SignKeypair::from_seed(&Secret::from_bytes(pat32(0xf0)));
    let message = env.signing_input().expect("the signing input");

    // Unsigned: no signature at all (all zero).
    let mut unsigned = env.clone();
    unsigned.sig = Signature::from_bytes([0u8; 64]);
    // Forged: a valid signature of another key over the same bytes.
    let mut forged = env.clone();
    forged.sig = sign::sign(&ENVELOPE_V2, &message, &other_key);
    // Wrong signer: signed with another key than the one the directory has for the named device.
    let mut wrong_signer = env.clone();
    wrong_signer.sign(&other_key).expect("signs");
    // Unknown signer: validly signed, by a device the directory has never heard of.
    let mut unknown = env.clone();
    unknown.signer = DeviceId::from_bytes(pat16(0xb0));
    unknown.sign(&f.signer).expect("signs");

    // Revoked at `at`, not yet active at `at`.
    let mut timed = MapDirectory::new();
    let mut entry = DirectoryEntry::active(f.signer.verify_key());
    entry.added_at = UnixTime::from_secs(100);
    entry.removed_at = Some(UnixTime::from_secs(200));
    timed.insert(f.device, entry);
    let at_time = |t: u64| VerifyAt::Time(UnixTime::from_secs(t));
    assert_eq!(verify_signature(&env, &timed, at_time(150)), Ok(()));
    // Unavailable: the directory cannot answer.
    let mut down = f.dir.clone();
    down.set_unavailable(true);

    let results = [
        (
            "unsigned",
            verify_signature(&unsigned, &f.dir, VerifyAt::Now),
            EnvelopeError::BadSignature,
        ),
        (
            "forged",
            verify_signature(&forged, &f.dir, VerifyAt::Now),
            EnvelopeError::BadSignature,
        ),
        (
            "wrong signer",
            verify_signature(&wrong_signer, &f.dir, VerifyAt::Now),
            EnvelopeError::BadSignature,
        ),
        (
            "unknown signer",
            verify_signature(&unknown, &f.dir, VerifyAt::Now),
            EnvelopeError::UnknownSigner,
        ),
        (
            "revoked at `at`",
            verify_signature(&env, &timed, at_time(200)),
            EnvelopeError::SignerRevoked,
        ),
        (
            "not yet active at `at`",
            verify_signature(&env, &timed, at_time(99)),
            EnvelopeError::SignerNotYetActive,
        ),
        (
            "directory unavailable",
            verify_signature(&env, &down, VerifyAt::Now),
            EnvelopeError::DirectoryUnavailable,
        ),
    ];
    for (what, got, want) in &results {
        assert_eq!(got, &Err(*want), "{what}");
    }
    // The four directory answers and the bad signature are five different errors.
    let mut distinct: Vec<EnvelopeError> = results.iter().map(|(_, _, want)| *want).collect();
    distinct.sort_by_key(|e| format!("{e:?}"));
    distinct.dedup();
    assert_eq!(distinct.len(), 5);

    // The same refusals through `open`, by name.
    let by_open =
        |e: &Envelope, d: &MapDirectory, at: VerifyAt| open(e, &f.me, &f.ctx, d, at).err();
    assert_eq!(
        by_open(&unsigned, &f.dir, VerifyAt::Now),
        Some(OpenError::BadSignature)
    );
    assert_eq!(
        by_open(&forged, &f.dir, VerifyAt::Now),
        Some(OpenError::BadSignature)
    );
    assert_eq!(
        by_open(&wrong_signer, &f.dir, VerifyAt::Now),
        Some(OpenError::BadSignature)
    );
    assert_eq!(
        by_open(&unknown, &f.dir, VerifyAt::Now),
        Some(OpenError::UnknownSigner)
    );
    assert_eq!(
        by_open(&env, &timed, at_time(200)),
        Some(OpenError::SignerRevoked)
    );
    assert_eq!(
        by_open(&env, &timed, at_time(99)),
        Some(OpenError::SignerNotYetActive)
    );
    assert_eq!(
        by_open(&env, &down, VerifyAt::Now),
        Some(OpenError::DirectoryUnavailable)
    );
    // And by the state of a device now.
    let mut states = MapDirectory::new();
    let mut revoked = DirectoryEntry::active(f.signer.verify_key());
    revoked.now = DeviceState::Revoked;
    states.insert(f.device, revoked);
    assert_eq!(
        by_open(&env, &states, VerifyAt::Now),
        Some(OpenError::SignerRevoked)
    );
}

/// The signature covers the ciphertext: two envelopes with an identical header (the same
/// entropy, so the same ephemeral key and nonce) and different plaintexts; the ciphertext of one
/// put into the other is refused. A signature over the header alone would accept it.
#[test]
fn c5_the_signature_covers_the_ciphertext() {
    let f = Fixture::new(5);
    let a = f.wrap1(b"AAAA", 52);
    let b = f.wrap1(b"BBBB", 52);
    assert_eq!(a.epk, b.epk);
    assert_eq!(a.nonce, b.nonce);
    assert_eq!(a.rcpt, b.rcpt);
    assert_eq!(a.ctx, b.ctx);
    assert_eq!(a.signer, b.signer);
    assert_eq!(a.header_ad(), b.header_ad(), "an identical header");
    assert_ne!(a.ct, b.ct);
    assert_eq!(verify_signature(&a, &f.dir, VerifyAt::Now), Ok(()));
    assert_eq!(verify_signature(&b, &f.dir, VerifyAt::Now), Ok(()));
    let mut swapped = a.clone();
    swapped.ct = b.ct.clone();
    assert_eq!(
        verify_signature(&swapped, &f.dir, VerifyAt::Now),
        Err(EnvelopeError::BadSignature)
    );
    assert_eq!(
        open(&swapped, &f.me, &f.ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::BadSignature)
    );
    // The nonce is covered too, and so is every unknown key.
    let mut other_nonce = a.clone();
    other_nonce.nonce[0] ^= 1;
    assert_eq!(
        verify_signature(&other_nonce, &f.dir, VerifyAt::Now),
        Err(EnvelopeError::BadSignature)
    );
}

/// The signing rule: the signature is Ed25519 over `azlin-envelope-v2 0x00` and the canonical
/// bytes without `sig`, exactly.
#[test]
fn c5_the_signature_is_over_the_domain_then_nul_then_the_canonical_bytes_without_sig() {
    let f = Fixture::new(5);
    let env = f.wrap1(b"a space key", 53);
    let message = env.signing_input().expect("the signing input");
    let preimage = signing_input(&ENVELOPE_V2, &message);
    assert!(preimage.starts_with(b"azlin-envelope-v2\0"));
    assert_eq!(&preimage[18..], message.as_slice());
    assert_eq!(
        sign::verify_preimage(&preimage, &f.signer.verify_key(), &env.sig),
        Ok(())
    );
    // The signing input is the envelope's bytes without the `sig` entry (key 10): decode it and
    // see which keys it has.
    let decoded = azlin_cbor::decode(&message, &azlin_cbor::Limits::OBJECT).expect("canonical");
    let keys: Vec<u64> = decoded
        .as_map()
        .expect("a map")
        .iter()
        .map(|(k, _)| k.as_uint().expect("an integer key"))
        .collect();
    assert_eq!(
        keys,
        vec![1, 2, 3, 4, 6, 7, 8, 9],
        "no key 10, no key 5, no key 16"
    );
    // The domain is the one of the registry, and no other domain's signature verifies.
    assert!(sign::verify(
        &sign::POINTER_V2,
        &message,
        &f.signer.verify_key(),
        &env.sig
    )
    .is_err());
}

/// `verify_strict`: a signature that is not canonical (`S + L`, a second encoding of the same
/// signature) is refused; so is a signer key of small order.
#[test]
fn c5_malleable_signatures_and_small_order_keys_are_refused() {
    let f = Fixture::new(5);
    let env = f.wrap1(b"a space key", 54);
    let mut malleable = env.clone();
    malleable.sig = add_l(&env.sig);
    assert_ne!(malleable.sig, env.sig);
    assert_eq!(verify_signature(&env, &f.dir, VerifyAt::Now), Ok(()));
    assert_eq!(
        verify_signature(&malleable, &f.dir, VerifyAt::Now),
        Err(EnvelopeError::BadSignature)
    );
    assert_eq!(
        open(&malleable, &f.me, &f.ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::BadSignature)
    );

    // A signer whose key is of small order (the identity point): refused whatever the signature.
    let mut identity = [0u8; 32];
    identity[0] = 1;
    let mut weak = MapDirectory::new();
    weak.insert_active(f.device, VerifyKey::from_bytes(identity));
    let mut zero_sig = env.clone();
    zero_sig.sig = Signature::from_bytes([0u8; 64]);
    assert_eq!(
        verify_signature(&zero_sig, &weak, VerifyAt::Now),
        Err(EnvelopeError::BadSignature)
    );
    // And a key that is no point at all.
    let mut no_point = MapDirectory::new();
    no_point.insert_active(f.device, VerifyKey::from_bytes([0xffu8; 32]));
    assert_eq!(
        verify_signature(&env, &no_point, VerifyAt::Now),
        Err(EnvelopeError::BadSignature)
    );
}

// ---------------------------------------------------------------------------------------------
// C7: low order points
// ---------------------------------------------------------------------------------------------

/// The twelve encodings of the points of small order on Curve25519 (the order 1, 2, 4 and 8
/// points, in their canonical and non-canonical encodings and with the high bit set: the list
/// every X25519 library blacklists). `scratch/W1-S15b/kat_ref.py low-order` shows with an
/// independent ladder that each one gives the all-zero shared secret.
const LOW_ORDER: [&str; 12] = [
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0100000000000000000000000000000000000000000000000000000000000000",
    "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b800",
    "5f9c95bca3508c24b1d0b1559c83ef5b04445cc4581c8e86d8224eddd09f1157",
    "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
    "edffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
    "eeffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff7f",
    "0000000000000000000000000000000000000000000000000000000000000080",
    "0100000000000000000000000000000000000000000000000000000000000080",
    "e0eb7a7c3b41b8ae1656e3faf19fc46ada098deb9c32b1fd866205165f49b880",
    "5f9c95bca3508c24b1d0b1559c83ef5b04445cc4581c8e86d8224eddd09f11d7",
    "ecffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
];

fn low_order_points() -> Vec<(&'static str, [u8; 32])> {
    LOW_ORDER
        .iter()
        .map(|h| (*h, <[u8; 32]>::try_from(from_hex(h)).expect("32 bytes")))
        .collect()
}

#[test]
fn c7_a_recipient_key_of_low_order_is_refused_at_wrap_time() {
    let f = Fixture::new(7);
    for (hex, kx) in low_order_points() {
        let mut ent = SeededEntropy::from_u64(1);
        let r = wrap_x25519(
            &RecipientX25519 { kx },
            &f.ctx,
            b"key",
            &f.device,
            &f.signer,
            &mut ent,
        );
        assert_eq!(r.err(), Some(WrapError::NonContributory), "{hex}");
    }
}

#[cfg(feature = "pq")]
#[test]
fn c7_a_hybrid_recipient_key_of_low_order_is_refused_at_wrap_time() {
    let f = Fixture::new(7);
    let pq = f.me.pq_public().expect("an ML-KEM key").to_vec();
    for (hex, kx) in low_order_points() {
        let mut ent = SeededEntropy::from_u64(1);
        let r = wrap_hybrid(
            &RecipientHybrid { kx, pq: pq.clone() },
            &f.ctx,
            b"key",
            &f.device,
            &f.signer,
            &mut ent,
        );
        assert_eq!(r.err(), Some(WrapError::NonContributory), "{hex}");
    }
}

/// An ephemeral key of low order in a correctly signed envelope: `open` refuses it as
/// `NonContributory` (it depends on the public key alone) and never keys anything with the
/// all-zero secret.
#[test]
fn c7_an_ephemeral_key_of_low_order_is_refused_at_open_time() {
    let f = Fixture::new(7);
    for (hex, point) in low_order_points() {
        let mut env = f.wrap1(b"a space key", 71);
        env.epk = Some(point);
        env.sign(&f.signer).expect("signs");
        assert_eq!(
            f.open(&env).err(),
            Some(OpenError::NonContributory),
            "{hex}"
        );
        // Through the bytes of the wire as well.
        let again = Envelope::from_cbor(&env.to_cbor().expect("bytes")).expect("reads");
        assert_eq!(
            f.open(&again).err(),
            Some(OpenError::NonContributory),
            "{hex}"
        );
    }
}

#[cfg(feature = "pq")]
#[test]
fn c7_a_hybrid_envelope_with_an_ephemeral_key_of_low_order_is_refused_too() {
    let f = Fixture::new(7);
    for (hex, point) in low_order_points() {
        let mut env = f.wrap2(b"a space key", 72);
        env.epk = Some(point);
        env.sign(&f.signer).expect("signs");
        assert_eq!(
            f.open(&env).err(),
            Some(OpenError::NonContributory),
            "{hex}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// C8: a wrong recipient never opens
// ---------------------------------------------------------------------------------------------

#[test]
fn c8_a_wrong_recipient_never_opens_and_the_error_is_one_word() {
    let f = Fixture::new(8);
    let env = f.wrap1(b"a member key", 81);
    let mut answers = Vec::new();

    // A different recipient secret.
    answers.push(open(&env, &f.stranger, &f.ctx, &f.dir, VerifyAt::Now).err());
    // An envelope for a member opened with a device key: two secrets of one person.
    let member = RecipientSecret::new(&Secret::from_bytes(pat32(0x81)), None);
    let device_key = RecipientSecret::new(&Secret::from_bytes(pat32(0x82)), None);
    let mut ent = SeededEntropy::from_u64(82);
    let for_member = wrap_x25519(
        &member.recipient_x25519(),
        &f.ctx,
        b"a member key",
        &f.device,
        &f.signer,
        &mut ent,
    )
    .expect("wraps");
    assert!(open(&for_member, &member, &f.ctx, &f.dir, VerifyAt::Now).is_ok());
    answers.push(open(&for_member, &device_key, &f.ctx, &f.dir, VerifyAt::Now).err());
    // A wrong `rcpt`: every bit of it flipped, the envelope signed again so that only the
    // recipient check can refuse it.
    for bit in 0..256 {
        let mut wrong = env.clone();
        let mut rcpt = *wrong.rcpt.as_bytes();
        rcpt[bit / 8] ^= 1 << (bit % 8);
        wrong.rcpt = azlin_keys::KeyId::from_bytes(rcpt);
        wrong.sign(&f.signer).expect("signs");
        answers.push(f.open(&wrong).err());
    }
    // The recipient's id of the OTHER algorithm: an envelope of algorithm 1 whose `rcpt` names
    // the bundle of kx and pq (the id of an algorithm 2 envelope).
    #[cfg(feature = "pq")]
    {
        let mut wrong = env.clone();
        wrong.rcpt = f.me.hybrid_id().expect("a hybrid id");
        wrong.sign(&f.signer).expect("signs");
        answers.push(f.open(&wrong).err());
    }
    for answer in &answers {
        assert_eq!(
            answer,
            &Some(OpenError::NotForMe),
            "every wrong recipient gets the same one word"
        );
    }
    assert!(answers.len() >= 258);
    // And the right recipient still opens it.
    assert_eq!(f.open(&env).expect("opens").as_slice(), b"a member key");
}

#[cfg(feature = "pq")]
#[test]
fn c8_a_hybrid_envelope_is_not_for_the_x25519_key_alone_and_the_reverse() {
    let f = Fixture::new(8);
    let hybrid = f.wrap2(b"a member key", 83);
    let alg1 = f.wrap1(b"a member key", 84);
    // The recipient with only the X25519 key (the same one) is not the recipient of algorithm 2.
    let x25519_only = RecipientSecret::new(&Secret::from_bytes(pat32(8)), None);
    assert_eq!(x25519_only.kx_public(), f.me.kx_public());
    assert_eq!(
        open(&hybrid, &x25519_only, &f.ctx, &f.dir, VerifyAt::Now).err(),
        Some(OpenError::NotForMe)
    );
    // It IS the recipient of algorithm 1 envelopes to that key.
    assert!(open(&alg1, &x25519_only, &f.ctx, &f.dir, VerifyAt::Now).is_ok());
    // A recipient with both keys opens both kinds, each under its own id.
    assert!(f.open(&hybrid).is_ok());
    assert!(f.open(&alg1).is_ok());
    assert_ne!(f.me.x25519_id(), f.me.hybrid_id().expect("a hybrid id"));
}
