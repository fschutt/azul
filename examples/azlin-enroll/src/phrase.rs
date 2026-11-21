//! The fingerprint phrase: six words, 66 bits, that the two devices of an enrollment show and the
//! people compare (planning/architecture.md section 7.4 "Keys on devices" ("an existing device
//! scans it, compares the fingerprint phrase and wraps the member key"), section 5.11 "invites and
//! the fingerprint confirmation", section 7.5; build unit S17, lane W1-S17e).
//!
//! WHAT THE PHRASE IS. BLAKE3 (through `hash32`, a length-framed `derive_key`) of everything both
//! devices must agree on: the new device's public keys, the approving device's public keys and the
//! secret of the QR code, in this order, under the context [`PHRASE_CONTEXT`]; the first 66 bits
//! of the digest are six 11-bit numbers, the indexes of six words of a list of 2,048 words. The
//! WORDS belong to the UI ([`Wordlist`]); the crate knows the numbers. If any bit of any input
//! differs (a public key swapped by a man in the middle, another QR secret) the phrases differ:
//! 66 bits make a swapped key a one-in-7.4e19 chance. The approving device compares what the
//! other device shows with what it computed ([`confirm_phrase`]) and wraps the member key only
//! when they are the same.
//!
//! The QR secret is a one-time secret that the QR code itself shows on a screen. The crate holds
//! it in a `Secret` (never printed, wiped on drop); the BLAKE3 hasher that absorbs it lives on the
//! stack for the one call and is not wiped (the crate has no `zeroize`: drift row of the lane).
//!
//! THE INVITE. A mailed invite is a bearer secret that anyone who reads the mailbox could use
//! first, so the joiner's device shows its phrase and the INVITER's device confirms it before the
//! `member_join` entry takes effect (section 5.11). [`confirm_join`] is that confirmation: it
//! accepts only the inviter, only with the right phrase, and gives a [`JoinConfirmation`], a
//! witness for that invite and that joiner's key that the team log needs before it lets the join
//! take effect.
//!
//! ```compile_fail
//! // A confirmation cannot be copied: only `confirm_join` makes one.
//! fn takes_a_confirmation(c: azlin_enroll::JoinConfirmation) {
//!     let _second = c.clone();
//! }
//! ```
//!
//! ```compile_fail
//! // A confirmation cannot be made out of nothing.
//! let _c = azlin_enroll::JoinConfirmation::default();
//! ```

use azlin_seams::{hash32, Hash32, MemberId, Secret, VerifyKey};

/// The BLAKE3 `derive_key` context of the phrase (a hash context of this crate, not a signature
/// domain; the style of the keys family: `azlin 2026-11 <purpose> v<N>`).
///
/// Implements planning/architecture.md section 7.4 (the fingerprint phrase) and section 5.5 (one
/// context for one purpose).
///
/// Caller: `phrase_of`.
pub const PHRASE_CONTEXT: &str = "azlin 2026-11 enrollment fingerprint phrase v1";

/// The words of a phrase: six.
///
/// Implements planning/architecture.md section 7.4 (the fingerprint phrase).
///
/// Caller: `Phrase`.
pub const PHRASE_WORDS: usize = 6;

/// The bits of one word of a phrase: 11 (an index into 2,048 words).
///
/// Implements planning/architecture.md section 7.4 (the fingerprint phrase).
///
/// Caller: `phrase_of`.
pub const WORD_BITS: u32 = 11;

/// The words of the list a phrase indexes: 2,048.
///
/// Implements planning/architecture.md section 7.4 (the fingerprint phrase).
///
/// Caller: `Phrase::is_wellformed`. Seam: the UI's word list.
pub const WORDLIST_LEN: u16 = 2048;

/// A fingerprint phrase: six indexes of 11 bits (66 bits). The numbers are public to a caller so
/// that a UI can show and read words; `confirm_phrase` refuses a phrase with an index outside the
/// list.
///
/// Implements planning/architecture.md section 7.4 (the fingerprint phrase) and section 5.11 (the
/// inviter's confirmation).
///
/// Caller: `phrase_of`, `confirm_phrase`, `confirm_join`. Seam: the AzWorkspace linking and invite
/// dialogs (show the words, read the words).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Phrase(pub [u16; 6]);

impl Phrase {
    /// Whether every index is inside the word list (below [`WORDLIST_LEN`]).
    pub fn is_wellformed(&self) -> bool {
        self.0.iter().all(|index| *index < WORDLIST_LEN)
    }

    /// The six words of this phrase from the UI's list. A phrase with an index the list does not
    /// know has no words.
    pub fn render<'a>(&self, list: &'a dyn Wordlist) -> Result<Vec<&'a str>, WordlistGap> {
        let mut words = Vec::with_capacity(PHRASE_WORDS);
        for index in self.0 {
            match list.word(index) {
                Some(word) => words.push(word),
                None => return Err(WordlistGap { index }),
            }
        }
        Ok(words)
    }
}

/// The UI's list of words: the word of each index of a phrase.
///
/// Implements planning/architecture.md section 7.4 (the fingerprint phrase: the words are shown to
/// people).
///
/// Seam: the UI of AzWorkspace and AzDrive (one list of 2,048 words, in the person's language).
pub trait Wordlist {
    /// The word of this index, or `None` when the list has none.
    fn word(&self, index: u16) -> Option<&str>;
}

/// The list has no word for an index of the phrase.
///
/// Implements planning/architecture.md section 7.4.
///
/// Caller: `Phrase::render`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the word list has no word for the index {index}")]
pub struct WordlistGap {
    /// The index without a word.
    pub index: u16,
}

/// The public keys of one device as they go into a transcript: the signature key, the key-exchange
/// key, and the post-quantum key if the device has one.
///
/// Implements planning/architecture.md section 5.3 (`reg_device`: `sign_pub`, `kx_pub`, `pq_pub`)
/// and section 7.4 ("BLAKE3 of both devices' public keys").
///
/// Caller: `Transcript`. Seam: the linking flow of AzWorkspace (the keys of both devices).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceKeys {
    /// The device's Ed25519 signature key.
    pub sign_pub: VerifyKey,
    /// The device's X25519 key-exchange key.
    pub kx_pub: [u8; 32],
    /// The device's ML-KEM-768 encapsulation key, when it has one.
    pub pq_pub: Option<Vec<u8>>,
}

/// What both devices of an enrollment agree on: the new device's keys, the approving device's keys
/// and the secret of the QR code. Both devices build the same transcript, so both compute the same
/// phrase.
///
/// Implements planning/architecture.md section 7.4 ("The new device shows a QR code; an existing
/// device scans it, compares the fingerprint phrase").
///
/// Caller: `phrase_of`. Seam: the linking flow of AzWorkspace.
#[derive(Debug)]
pub struct Transcript {
    /// The keys of the device that shows the QR code (the new device).
    pub new_device: DeviceKeys,
    /// The keys of the device that scans it (the approving device).
    pub approver: DeviceKeys,
    /// The secret of the QR code.
    pub qr_secret: Secret<32>,
}

/// The tag of a device without a post-quantum key.
const NO_PQ: &[u8] = &[0];
/// The tag of a device with a post-quantum key (the key follows).
const WITH_PQ: &[u8] = &[1];

/// The parts of the hash of one device's keys.
fn device_parts<'a>(keys: &'a DeviceKeys, out: &mut Vec<&'a [u8]>) {
    out.push(keys.sign_pub.as_bytes().as_slice());
    out.push(keys.kx_pub.as_slice());
    match &keys.pq_pub {
        Some(pq) => {
            out.push(WITH_PQ);
            out.push(pq.as_slice());
        }
        None => out.push(NO_PQ),
    }
}

/// The phrase of a transcript: the first 66 bits of its hash as six 11-bit indexes. The same
/// transcript gives the same phrase on both devices; any other input gives another one.
///
/// Implements planning/architecture.md section 7.4 ("compares the fingerprint phrase") and section
/// 5.11 ("the joiner's device shows its fingerprint phrase").
///
/// Caller: none yet. Seam: the linking and invite flows of AzWorkspace (both devices call it).
pub fn phrase_of(t: &Transcript) -> Phrase {
    let mut parts: Vec<&[u8]> = Vec::with_capacity(9);
    device_parts(&t.new_device, &mut parts);
    device_parts(&t.approver, &mut parts);
    parts.push(t.qr_secret.expose().as_slice());
    let digest = hash32(PHRASE_CONTEXT, &parts);
    // The first nine bytes are 72 bits; the top 66 of them are the phrase.
    let mut acc: u128 = 0;
    for byte in digest.as_bytes().iter().take(9) {
        acc = (acc << 8) | u128::from(*byte);
    }
    let bits = acc >> 6;
    let mut words = [0u16; PHRASE_WORDS];
    for (position, word) in words.iter_mut().enumerate() {
        let shift = WORD_BITS * (PHRASE_WORDS as u32 - 1 - position as u32);
        *word = ((bits >> shift) & 0x7ff) as u16;
    }
    Phrase(words)
}

/// The phrase that the other device shows is not the one this device computed: no wrap. The error
/// carries neither phrase.
///
/// Implements planning/architecture.md section 7.4 (the approver compares before it wraps).
///
/// Caller: `confirm_phrase`, `confirm_join`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the phrases are not the same: the key is not wrapped")]
pub struct PhraseMismatch;

/// The approving device compares the phrase it computed with the one the other device shows.
/// They must be the same, and both must be well-formed; otherwise the wrap is refused. All six
/// words are compared whatever the first ones say.
///
/// Implements planning/architecture.md section 7.4 ("compares the fingerprint phrase and wraps the
/// member key") and section 7.5 ("check the fingerprint phrase with the person out of band").
///
/// Caller: none yet. Seam: the approving device's flow (a wrong phrase refuses the wrap); the
/// `Evidence::PhraseConfirmed` of the request is made only after this returned `Ok`.
pub fn confirm_phrase(expected: &Phrase, shown_to_approver: &Phrase) -> Result<(), PhraseMismatch> {
    let mut difference: u16 = 0;
    for (a, b) in expected.0.iter().zip(shown_to_approver.0.iter()) {
        difference |= a ^ b;
    }
    if difference == 0 && expected.is_wellformed() && shown_to_approver.is_wellformed() {
        Ok(())
    } else {
        Err(PhraseMismatch)
    }
}

/// A witness that the INVITER's device confirmed the joiner's phrase: only `confirm_join` makes
/// one (private fields, no `Clone`, `Default` or `From`), and the team log lets a `member_join`
/// take effect only against one. It is the confirmation of ONE join: it names the invite (the
/// hash of its secret) and the joiner's key, and the team log accepts it only for a `member_join`
/// of that invite by that key.
///
/// Implements planning/architecture.md section 5.11 ("the inviter's device confirms it ... before
/// the `member_join` entry takes effect").
///
/// Caller: `confirm_join`. Seam: TEAMLOG-APPLY (a `member_join` takes effect only with it).
#[derive(Debug)]
pub struct JoinConfirmation {
    inviter: MemberId,
    invite: Hash32,
    joiner: VerifyKey,
}

impl JoinConfirmation {
    /// The inviter who confirmed.
    pub fn inviter(&self) -> MemberId {
        self.inviter
    }

    /// The invite that was confirmed: the hash of its secret, as the `member_invite` entry carries
    /// it.
    pub fn invite(&self) -> Hash32 {
        self.invite
    }

    /// The key of the joiner whose phrase was confirmed.
    pub fn joiner(&self) -> VerifyKey {
        self.joiner
    }
}

/// Why a join is not confirmed.
///
/// Implements planning/architecture.md section 5.11.
///
/// Caller: `confirm_join`. Seam: TEAMLOG-APPLY (refuses the `member_join`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JoinRefusal {
    /// The confirmation did not come from the inviter: a mailed invite alone is a bearer secret.
    #[error("only the inviter's device may confirm the joiner's phrase")]
    NotTheInviter,
    /// The phrases are not the same.
    #[error(transparent)]
    Mismatch(#[from] PhraseMismatch),
}

/// The inviter's device confirms the phrase that the joiner's device shows (in person, by phone or
/// in a call): `confirmed_by` must be the inviter and the phrases must be the same. `invite` is the
/// hash of the invite's secret and `joiner` the joiner's key: the confirmation is for that join.
///
/// Implements planning/architecture.md section 5.11 ("so the joiner's device shows its fingerprint
/// phrase and the inviter's device confirms it, as device linking does (7.4), before the
/// `member_join` entry takes effect").
///
/// Caller: none yet. Seam: TEAMLOG-APPLY and the invite flow (the `member_join` waits for the
/// witness).
pub fn confirm_join(
    expected: &Phrase,
    shown_to_inviter: &Phrase,
    inviter: MemberId,
    confirmed_by: MemberId,
    invite: Hash32,
    joiner: VerifyKey,
) -> Result<JoinConfirmation, JoinRefusal> {
    if confirmed_by != inviter {
        return Err(JoinRefusal::NotTheInviter);
    }
    confirm_phrase(expected, shown_to_inviter)?;
    Ok(JoinConfirmation {
        inviter,
        invite,
        joiner,
    })
}
