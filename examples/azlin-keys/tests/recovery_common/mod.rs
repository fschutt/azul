//! Helpers the recovery tests share: byte patterns, a seeded generator, a scripted entropy source
//! that records its draws, the fixed inputs of the known-answer vectors, and the word of an error.
//!
//! Everything here builds on the crate's public API; nothing reads the crate's private state. The
//! inputs of the known-answer rows are PATTERNS (`pat(start, len)`: `start, start+1, ...`), so the
//! independent program (`s15d_ref.py`) rebuilds them without a file.
#![allow(dead_code)]

pub mod fuzz_entry;
pub mod vectors;

use azlin_keys::envelope::{Ctx, RecipientSecret, RecoveryMethod};
use azlin_keys::recovery::{RecoveryCode, RecoveryError};
use azlin_keys::{DeviceId, Entropy, EntropyError, MapDirectory, MemberId, Secret, SignKeypair};

/// `len` bytes counting up from `start` (wrapping).
pub fn pat(start: u8, len: usize) -> Vec<u8> {
    (0..len).map(|i| start.wrapping_add(i as u8)).collect()
}

pub fn pat16(start: u8) -> [u8; 16] {
    <[u8; 16]>::try_from(pat(start, 16)).expect("16 bytes")
}

pub fn pat32(start: u8) -> [u8; 32] {
    <[u8; 32]>::try_from(pat(start, 32)).expect("32 bytes")
}

pub fn pat64(start: u8) -> [u8; 64] {
    <[u8; 64]>::try_from(pat(start, 64)).expect("64 bytes")
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd hex length: {text}");
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("a hex digit"))
        .collect()
}

/// SplitMix64: the seeded generator of every property test (no `rand`). A failing case prints its
/// seed and index, and `SplitMix64::new(seed)` replays it.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        SplitMix64(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n` (`n > 0`); the modulo bias is irrelevant for tests.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next_u64().to_le_bytes()[0]).collect()
    }
}

/// An entropy source that serves a fixed byte string in order and records the size of each draw:
/// the known-answer tests pin the ORDER and the SIZES of a construction's draws with it.
pub struct ScriptedEntropy {
    bytes: Vec<u8>,
    at: usize,
    /// The size of every draw so far, in order.
    pub draws: Vec<usize>,
}

impl ScriptedEntropy {
    pub fn new(bytes: Vec<u8>) -> ScriptedEntropy {
        ScriptedEntropy {
            bytes,
            at: 0,
            draws: Vec::new(),
        }
    }

    /// The bytes not served yet.
    pub fn left(&self) -> usize {
        self.bytes.len() - self.at
    }
}

impl Entropy for ScriptedEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        let end = self
            .at
            .checked_add(out.len())
            .ok_or(EntropyError::Exhausted)?;
        let chunk = self
            .bytes
            .get(self.at..end)
            .ok_or(EntropyError::Exhausted)?;
        out.copy_from_slice(chunk);
        self.at = end;
        self.draws.push(out.len());
        Ok(())
    }
}

/// The word of an error as the vector files write it: its `Debug` text (`BelowFloor`,
/// `Open(CtxMismatch)`, `WrongAlg(Prf)`).
pub fn word(error: &RecoveryError) -> String {
    format!("{error:?}")
}

// ---------------------------------------------------------------------------------------------
// The fixed inputs of the known-answer vectors
// ---------------------------------------------------------------------------------------------

/// The signer's seed is `pat(0x90, 32)`, the device `pat(0xd0, 16)`, the member `pat(0x71, 16)`, the
/// other member `pat(0x72, 16)`, the recovery code `pat(0x10, 16)`.
pub const KAT_SIGNER: u8 = 0x90;
pub const KAT_DEVICE: u8 = 0xd0;
pub const KAT_MEMBER: u8 = 0x71;
pub const KAT_OTHER_MEMBER: u8 = 0x72;
pub const KAT_CODE: u8 = 0x10;

pub fn kat_signer() -> SignKeypair {
    SignKeypair::from_seed(&Secret::from_bytes(pat32(KAT_SIGNER)))
}

pub fn kat_device() -> DeviceId {
    DeviceId::from_bytes(pat16(KAT_DEVICE))
}

pub fn kat_member() -> MemberId {
    MemberId::from_bytes(pat16(KAT_MEMBER))
}

pub fn kat_other_member() -> MemberId {
    MemberId::from_bytes(pat16(KAT_OTHER_MEMBER))
}

/// The directory of the known-answer vectors: the one device, valid always.
pub fn kat_directory() -> MapDirectory {
    let mut dir = MapDirectory::new();
    dir.insert_active(kat_device(), kat_signer().verify_key());
    dir
}

pub fn kat_ctx(method: RecoveryMethod) -> Ctx {
    Ctx::recovery(&kat_member(), method)
}

pub fn kat_code() -> RecoveryCode {
    RecoveryCode::from_secret(Secret::from_bytes(pat16(KAT_CODE)))
}

/// The recipient secret of the contact in the vectors: X25519 `pat(0x20, 32)` and, with `pq`, the
/// ML-KEM seed `pat(0x60, 64)`.
pub fn kat_contact_secret(with_pq: bool) -> RecipientSecret {
    let kx = Secret::from_bytes(pat32(0x20));
    if with_pq {
        RecipientSecret::new(&kx, Some(&Secret::from_bytes(pat64(0x60))))
    } else {
        RecipientSecret::new(&kx, None)
    }
}

/// The token `name=value` of the `inputs` text of a vector row.
pub fn input<'a>(inputs: &'a str, name: &str) -> &'a str {
    let prefix = format!("{name}=");
    inputs
        .split([' ', ';'])
        .find_map(|token| token.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("no `{name}=` in the inputs: {inputs}"))
}
