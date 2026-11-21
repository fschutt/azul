//! Check 8 of the brief: the fingerprint phrase (planning/architecture.md section 7.4: "an existing
//! device scans it, compares the fingerprint phrase and wraps the member key"; section 5.11: the
//! inviter's device confirms the joiner's phrase before the `member_join` takes effect).
//!
//! The expected phrase of the known-answer test is derived in the test by an INDEPENDENT
//! implementation of the documented layout (`blake3::Hasher` directly, bit by bit), not by the
//! function under test.

mod common;

use azlin_enroll::{
    confirm_join, confirm_phrase, phrase_of, release, Confirmer, DeviceKeys, Evidence,
    EvidenceKind, JoinRefusal, Path, Phrase, PhraseMismatch, Reason, Release, StampCheck,
    Transcript, Wordlist, WordlistGap, PHRASE_CONTEXT, PHRASE_WORDS, WORDLIST_LEN, WORD_BITS,
};
use azlin_seams::sim::Rng;
use azlin_seams::{hash32, MemberId, Secret, VerifyKey};
use common::{add_evidence, at, device, member, Scene};

/// Everything that goes into a phrase, as plain data that can be copied and flipped.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Inputs {
    new_sign: [u8; 32],
    new_kx: [u8; 32],
    new_pq: Option<Vec<u8>>,
    approver_sign: [u8; 32],
    approver_kx: [u8; 32],
    approver_pq: Option<Vec<u8>>,
    qr_secret: [u8; 32],
}

impl Inputs {
    /// A fixed set of inputs: new device with a post-quantum key of 40 bytes, approver without.
    fn fixed() -> Inputs {
        let mut rng = Rng::new(7);
        let mut bytes32 = || {
            let mut out = [0u8; 32];
            rng.fill_bytes(&mut out);
            out
        };
        Inputs {
            new_sign: bytes32(),
            new_kx: bytes32(),
            new_pq: Some((0..40u8).collect()),
            approver_sign: bytes32(),
            approver_kx: bytes32(),
            approver_pq: None,
            qr_secret: bytes32(),
        }
    }

    /// The transcript both devices build from these inputs (each side builds its own).
    fn transcript(&self) -> Transcript {
        Transcript {
            new_device: DeviceKeys {
                sign_pub: VerifyKey::from_bytes(self.new_sign),
                kx_pub: self.new_kx,
                pq_pub: self.new_pq.clone(),
            },
            approver: DeviceKeys {
                sign_pub: VerifyKey::from_bytes(self.approver_sign),
                kx_pub: self.approver_kx,
                pq_pub: self.approver_pq.clone(),
            },
            qr_secret: Secret::from_bytes(self.qr_secret),
        }
    }

    /// The number of bits of input that can be flipped.
    fn bit_count(&self) -> usize {
        let pq_bits = |pq: &Option<Vec<u8>>| match pq {
            Some(bytes) => bytes.len() * 8,
            None => 0,
        };
        // Four 32-byte keys and the 32-byte QR secret, and the post-quantum keys.
        5 * 256 + pq_bits(&self.new_pq) + pq_bits(&self.approver_pq)
    }

    /// Flips bit number `n` of the inputs, counting from the first byte of the new device's
    /// signature key.
    fn flip(&mut self, n: usize) {
        let mut at = n;
        let mut fields: Vec<&mut [u8]> = vec![
            &mut self.new_sign,
            &mut self.new_kx,
            &mut self.approver_sign,
            &mut self.approver_kx,
            &mut self.qr_secret,
        ];
        if let Some(pq) = self.new_pq.as_mut() {
            fields.push(pq.as_mut_slice());
        }
        if let Some(pq) = self.approver_pq.as_mut() {
            fields.push(pq.as_mut_slice());
        }
        for field in fields {
            let bits = field.len() * 8;
            if at < bits {
                field[at / 8] ^= 1 << (at % 8);
                return;
            }
            at -= bits;
        }
        panic!("bit {n} is out of range");
    }
}

/// The phrase by an independent implementation of the documented layout: BLAKE3 in `derive_key`
/// mode with the context, each part preceded by its length as a little-endian u64; the parts are,
/// for each device, the signature key, the key-exchange key and a tag (0 without, 1 with a
/// post-quantum key, followed by that key), then the QR secret; the phrase is the first 66 bits of
/// the digest, most significant bit first, as six numbers of 11 bits.
fn independent_phrase(inputs: &Inputs) -> [u16; 6] {
    let mut parts: Vec<Vec<u8>> = Vec::new();
    for (sign, kx, pq) in [
        (&inputs.new_sign, &inputs.new_kx, &inputs.new_pq),
        (
            &inputs.approver_sign,
            &inputs.approver_kx,
            &inputs.approver_pq,
        ),
    ] {
        parts.push(sign.to_vec());
        parts.push(kx.to_vec());
        match pq {
            Some(key) => {
                parts.push(vec![1]);
                parts.push(key.clone());
            }
            None => parts.push(vec![0]),
        }
    }
    parts.push(inputs.qr_secret.to_vec());
    let mut hasher = blake3::Hasher::new_derive_key(PHRASE_CONTEXT);
    for part in &parts {
        hasher.update(&(part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let digest = hasher.finalize();
    let mut bits = Vec::new();
    for byte in digest.as_bytes().iter().take(9) {
        for shift in (0..8).rev() {
            bits.push((byte >> shift) & 1);
        }
    }
    let mut words = [0u16; 6];
    for (i, word) in words.iter_mut().enumerate() {
        let mut value = 0u16;
        for bit in &bits[i * 11..(i + 1) * 11] {
            value = (value << 1) | u16::from(*bit);
        }
        *word = value;
    }
    words
}

#[test]
fn phrase_is_the_same_on_both_devices_known_answer() {
    let inputs = Inputs::fixed();
    // Each device builds the transcript on its own and computes the phrase.
    let on_the_new_device = phrase_of(&inputs.transcript());
    let on_the_approver = phrase_of(&inputs.transcript());
    assert_eq!(on_the_new_device, on_the_approver);
    // 66 bits: six numbers of 11 bits.
    assert_eq!(PHRASE_WORDS, 6);
    assert_eq!(WORD_BITS, 11);
    assert_eq!(PHRASE_WORDS as u32 * WORD_BITS, 66);
    assert_eq!(WORDLIST_LEN, 2048);
    assert!(on_the_new_device.is_wellformed());
    // The known answer of the documented layout, by an independent implementation.
    assert_eq!(on_the_new_device, Phrase(independent_phrase(&inputs)));
    // And the pinned value: a change of the layout (the context, the framing, the bits) changes
    // every phrase of every linked device, so it is a deliberate act that turns this red.
    assert_eq!(on_the_new_device, Phrase(KNOWN_PHRASE));
    // The same transcript with the post-quantum key absent on both is another phrase, and the
    // independent implementation agrees on it too.
    let mut without = inputs.clone();
    without.new_pq = None;
    assert_ne!(phrase_of(&without.transcript()), on_the_new_device);
    assert_eq!(
        phrase_of(&without.transcript()),
        Phrase(independent_phrase(&without))
    );
}

/// The phrase of `Inputs::fixed()` (pinned: see the test above).
const KNOWN_PHRASE: [u16; 6] = [1529, 1053, 1149, 1976, 1858, 415];

#[test]
fn phrase_changes_when_any_input_bit_changes() {
    let base = Inputs::fixed();
    let expected = phrase_of(&base.transcript());
    let bits = base.bit_count();
    assert_eq!(bits, 4 * 256 + 256 + 40 * 8, "the inputs under test");
    // 2,000 seeded flips of one bit anywhere in the inputs: the phrase is another.
    for seed in 0..2_000u64 {
        let mut rng = Rng::new(seed);
        let bit = usize::try_from(rng.below(bits as u64).expect("a non-empty range")).unwrap();
        let mut flipped = base.clone();
        flipped.flip(bit);
        assert_ne!(flipped, base);
        assert_ne!(
            phrase_of(&flipped.transcript()),
            expected,
            "seed {seed}: flipping bit {bit} left the phrase as it was"
        );
    }
    // Every bit of the 32-byte keys and of the QR secret, once each (not only the sampled ones).
    for bit in 0..(4 * 256 + 256) {
        let mut flipped = base.clone();
        flipped.flip(bit);
        assert_ne!(phrase_of(&flipped.transcript()), expected, "bit {bit}");
    }
    // The presence of a post-quantum key, and its length, are inputs as well.
    let mut absent = base.clone();
    absent.approver_pq = Some(Vec::new());
    assert_ne!(phrase_of(&absent.transcript()), expected);
    assert_ne!(
        phrase_of(&absent.transcript()),
        phrase_of(&Inputs::fixed().transcript())
    );
    // The two devices are not interchangeable: swapping the roles is another transcript.
    let mut swapped = base.clone();
    std::mem::swap(&mut swapped.new_sign, &mut swapped.approver_sign);
    std::mem::swap(&mut swapped.new_kx, &mut swapped.approver_kx);
    assert_ne!(phrase_of(&swapped.transcript()), expected);
}

/// A word list for the tests: "w0" to "w2047".
struct TestWords;

impl Wordlist for TestWords {
    fn word(&self, index: u16) -> Option<&str> {
        // Static strings for the first words only: the rest has no word.
        const WORDS: [&str; 4] = ["w0", "w1", "w2", "w3"];
        WORDS.get(usize::from(index)).copied()
    }
}

#[test]
fn a_wrong_phrase_refuses_the_wrap() {
    let inputs = Inputs::fixed();
    let expected = phrase_of(&inputs.transcript());
    assert_eq!(confirm_phrase(&expected, &expected), Ok(()));
    // Any single word that differs: the wrap is refused.
    for position in 0..PHRASE_WORDS {
        let mut shown = expected;
        shown.0[position] = (shown.0[position] + 1) % WORDLIST_LEN;
        assert_eq!(
            confirm_phrase(&expected, &shown),
            Err(PhraseMismatch),
            "word {position}"
        );
        assert_eq!(confirm_phrase(&shown, &expected), Err(PhraseMismatch));
    }
    // A man in the middle who swapped a key shows another phrase.
    let mut swapped = inputs.clone();
    swapped.new_kx[0] ^= 1;
    assert_eq!(
        confirm_phrase(&expected, &phrase_of(&swapped.transcript())),
        Err(PhraseMismatch)
    );
    // Words outside the list are no phrase, even when both sides show the same ones.
    let bad = Phrase([2048, 0, 0, 0, 0, 0]);
    assert!(!bad.is_wellformed());
    assert_eq!(confirm_phrase(&bad, &bad), Err(PhraseMismatch));
    // The error says nothing of either phrase.
    let text = PhraseMismatch.to_string();
    assert!(!text.chars().any(|c| c.is_ascii_digit()), "{text}");

    // In the request: the approving device wraps (and the group releases) only after the phrase
    // was confirmed. A wrong phrase gives no confirmation, so nothing is released.
    let mut scene = Scene::business();
    let mut request = scene.open(Path::OwnDevice, at(0));
    let shown = Phrase([1, 2, 3, 4, 5, 6]);
    if confirm_phrase(&expected, &shown).is_ok() {
        add_evidence(
            &mut request,
            Evidence::PhraseConfirmed(Confirmer::Device(device(1))),
            at(0),
        )
        .unwrap();
    }
    match release(&request, at(0), StampCheck::Agreed) {
        Release::Withheld(Reason::EvidenceMissing(EvidenceKind::FingerprintPhrase)) => {}
        other => panic!("a wrong phrase must not release: {other:?}"),
    }
    // The right phrase gives the confirmation, and then it releases.
    if confirm_phrase(&expected, &expected).is_ok() {
        add_evidence(
            &mut request,
            Evidence::PhraseConfirmed(Confirmer::Device(device(1))),
            at(0),
        )
        .unwrap();
    }
    assert!(release(&request, at(0), StampCheck::Agreed).is_granted());
}

#[test]
fn words_come_from_the_uis_list() {
    let phrase = Phrase([0, 1, 2, 3, 0, 1]);
    assert_eq!(
        phrase.render(&TestWords).unwrap(),
        vec!["w0", "w1", "w2", "w3", "w0", "w1"]
    );
    let missing = Phrase([0, 1, 2, 3, 4, 5]);
    assert_eq!(
        missing.render(&TestWords).unwrap_err(),
        WordlistGap { index: 4 }
    );
}

#[test]
fn invite_join_needs_the_inviters_confirmation_of_the_phrase() {
    // A mailed invite is a bearer secret: anyone who reads the mailbox could use it first. The
    // joiner's device shows its phrase, and the INVITER's device confirms it.
    let inputs = Inputs::fixed();
    let expected = phrase_of(&inputs.transcript());
    let inviter: MemberId = member(5);
    let invite = hash32(
        "azlin test invite",
        &[b"the secret of the invite".as_slice()],
    );
    let joiner = VerifyKey::from_bytes([7; 32]);
    // The inviter, with the right phrase: confirmed.
    let confirmation =
        confirm_join(&expected, &expected, inviter, inviter, invite, joiner).expect("the inviter");
    assert_eq!(confirmation.inviter(), inviter);
    // The confirmation is for ONE join: it names the invite and the key of the joiner, so the
    // team log can accept it only for a `member_join` of that invite by that key.
    assert_eq!(confirmation.invite(), invite);
    assert_eq!(confirmation.joiner(), joiner);
    // Somebody else (the owner of the mailbox, an admin, the joiner): no confirmation.
    for other in [member(6), member(1), member(10)] {
        assert_eq!(
            confirm_join(&expected, &expected, inviter, other, invite, joiner).unwrap_err(),
            JoinRefusal::NotTheInviter
        );
    }
    // The inviter with another phrase: no confirmation (the phrase of an attacker's device).
    let other = Phrase([9, 9, 9, 9, 9, 9]);
    assert_eq!(
        confirm_join(&expected, &other, inviter, inviter, invite, joiner).unwrap_err(),
        JoinRefusal::Mismatch(PhraseMismatch)
    );
    // Two joins are two confirmations: another invite or another joiner is what the confirmation
    // says, not what the first one said.
    let other_invite = hash32("azlin test invite", &[b"another invite".as_slice()]);
    let second = confirm_join(
        &expected,
        &expected,
        inviter,
        inviter,
        other_invite,
        VerifyKey::from_bytes([8; 32]),
    )
    .unwrap();
    assert_ne!(second.invite(), confirmation.invite());
    assert_ne!(second.joiner(), confirmation.joiner());
}
