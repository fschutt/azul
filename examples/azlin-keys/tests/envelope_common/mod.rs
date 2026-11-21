//! Helpers the envelope tests share: byte patterns, a scripted entropy source that records its
//! draws, a fixture of one recipient, one signer and one directory, the known-answer cases, and
//! a tool that edits the CBOR map of an envelope (for the negative cases of the codec).
//!
//! Everything here builds on the crate's public API; nothing reads the crate's private state.
#![allow(dead_code)]

pub mod fuzz_entry;
pub mod vectors;

use azlin_cbor::{Limits, Map, Value};
#[cfg(feature = "pq")]
use azlin_keys::envelope::wrap_hybrid;
use azlin_keys::envelope::{wrap_x25519, Ctx, Envelope, OpenError, RecipientSecret};
use azlin_keys::{
    DeviceId, Entropy, EntropyError, Epoch, MapDirectory, MemberId, Secret, SeededEntropy,
    SignKeypair, SpaceId, VerifyAt,
};
use zeroize::Zeroizing;

/// `len` bytes counting up from `start` (wrapping): the patterns of the known-answer inputs, so
/// that an independent implementation can rebuild them without a file.
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

/// One recipient, one stranger, one signing device and a directory that knows it.
pub struct Fixture {
    pub me: RecipientSecret,
    pub stranger: RecipientSecret,
    pub device: DeviceId,
    pub signer: SignKeypair,
    pub dir: MapDirectory,
    pub space: SpaceId,
    pub ctx: Ctx,
}

impl Fixture {
    pub fn new(tag: u8) -> Fixture {
        let me = RecipientSecret::new(
            &Secret::from_bytes(pat32(tag)),
            Some(&Secret::from_bytes(pat64(tag ^ 0x5a))),
        );
        let stranger = RecipientSecret::new(
            &Secret::from_bytes(pat32(tag ^ 0x33)),
            Some(&Secret::from_bytes(pat64(tag ^ 0xa5))),
        );
        let signer = SignKeypair::from_seed(&Secret::from_bytes(pat32(tag ^ 0x77)));
        let device = DeviceId::from_bytes(pat16(tag ^ 0xd0));
        let mut dir = MapDirectory::new();
        dir.insert_active(device, signer.verify_key());
        let space = SpaceId::from_bytes(pat16(tag ^ 0x0a));
        let ctx = Ctx::space_epoch(&space, Epoch::new(3).expect("epoch 3"));
        Fixture {
            me,
            stranger,
            device,
            signer,
            dir,
            space,
            ctx,
        }
    }

    /// An envelope of algorithm 1 to the fixture's recipient, entropy from `seed`.
    pub fn wrap1(&self, plaintext: &[u8], seed: u64) -> Envelope {
        let mut ent = SeededEntropy::from_u64(seed);
        wrap_x25519(
            &self.me.recipient_x25519(),
            &self.ctx,
            plaintext,
            &self.device,
            &self.signer,
            &mut ent,
        )
        .expect("a wrap of algorithm 1")
    }

    /// An envelope of algorithm 2 to the fixture's recipient, entropy from `seed`.
    #[cfg(feature = "pq")]
    pub fn wrap2(&self, plaintext: &[u8], seed: u64) -> Envelope {
        let mut ent = SeededEntropy::from_u64(seed);
        wrap_hybrid(
            &self.me.recipient_hybrid().expect("an ML-KEM key"),
            &self.ctx,
            plaintext,
            &self.device,
            &self.signer,
            &mut ent,
        )
        .expect("a wrap of algorithm 2")
    }

    /// `open` as the fixture's recipient, for the fixture's context, now.
    pub fn open(&self, env: &Envelope) -> Result<Zeroizing<Vec<u8>>, OpenError> {
        azlin_keys::envelope::open(env, &self.me, &self.ctx, &self.dir, VerifyAt::Now)
    }
}

// ---------------------------------------------------------------------------------------------
// Editing the CBOR of an envelope
// ---------------------------------------------------------------------------------------------

/// The bytes of `bytes` with their map edited: decode (loosely: the object limits), let `edit`
/// change the entries, encode again. The result is canonical CBOR, so a refusal of it is the
/// envelope reader's refusal and not the codec's.
pub fn edit_map(bytes: &[u8], edit: impl FnOnce(&mut Vec<(Value, Value)>)) -> Vec<u8> {
    let value = azlin_cbor::decode(bytes, &Limits::OBJECT).expect("canonical bytes");
    let mut entries = match value {
        Value::Map(map) => map.into_entries(),
        other => panic!("an envelope is a map, not {other:?}"),
    };
    edit(&mut entries);
    let map = Map::from_entries(entries).expect("no duplicate keys");
    azlin_cbor::encode(&Value::Map(map), &Limits::OBJECT).expect("within the object limits")
}

/// Sets key `key` to `value` (adding it when it is absent).
pub fn put(entries: &mut Vec<(Value, Value)>, key: u64, value: Value) {
    remove(entries, key);
    entries.push((Value::Uint(key), value));
}

/// Removes key `key`.
pub fn remove(entries: &mut Vec<(Value, Value)>, key: u64) {
    entries.retain(|(k, _)| *k != Value::Uint(key));
}

/// The value of key `key`.
pub fn get(entries: &[(Value, Value)], key: u64) -> Option<&Value> {
    entries
        .iter()
        .find(|(k, _)| *k == Value::Uint(key))
        .map(|(_, v)| v)
}

// ---------------------------------------------------------------------------------------------
// The known-answer cases (the inputs are patterns, so an independent program can rebuild them)
// ---------------------------------------------------------------------------------------------

/// Where the fixed inputs of the known-answer vectors start: the recipient's X25519 secret is
/// `pat(0x20, 32)`, its ML-KEM seed `pat(0x60, 64)`, the signer's seed `pat(0x90, 32)` and the
/// device `pat(0xd0, 16)`.
pub const KAT_KX: u8 = 0x20;
pub const KAT_PQ: u8 = 0x60;
pub const KAT_SIGNER: u8 = 0x90;
pub const KAT_DEVICE: u8 = 0xd0;

pub struct KatCase {
    pub name: &'static str,
    /// 1 or 2.
    pub alg: u8,
    /// The draws, in order: the ephemeral secret `pat(eph, 32)`, for algorithm 2 the ML-KEM
    /// message `pat(m, 32)`, then the nonce `pat(nonce, 24)`.
    pub eph: u8,
    pub m: u8,
    pub nonce: u8,
    pub plaintext: Vec<u8>,
    pub ctx: Ctx,
}

impl KatCase {
    /// The inputs of the case as the text of the vector file.
    pub fn inputs(&self) -> String {
        let draws = if self.alg == 2 {
            format!(
                "eph=pat(0x{:02x},32) m=pat(0x{:02x},32) nonce=pat(0x{:02x},24)",
                self.eph, self.m, self.nonce
            )
        } else {
            format!(
                "eph=pat(0x{:02x},32) nonce=pat(0x{:02x},24)",
                self.eph, self.nonce
            )
        };
        format!(
            "alg={} {} kx=pat(0x{KAT_KX:02x},32) pq=pat(0x{KAT_PQ:02x},64) signer=pat(0x{KAT_SIGNER:02x},32) device=pat(0x{KAT_DEVICE:02x},16) ctx={} plaintext={}",
            self.alg,
            draws,
            self.ctx,
            to_hex(&self.plaintext)
        )
    }

    /// The bytes the entropy source serves.
    pub fn entropy(&self) -> ScriptedEntropy {
        let mut bytes = pat(self.eph, 32);
        if self.alg == 2 {
            bytes.extend(pat(self.m, 32));
        }
        bytes.extend(pat(self.nonce, 24));
        ScriptedEntropy::new(bytes)
    }
}

/// The six known-answer cases: three contexts and plaintexts for each of the two algorithms.
pub fn kat_cases() -> Vec<KatCase> {
    let space = SpaceId::from_bytes(pat16(0x70));
    let member = MemberId::from_bytes(pat16(0x71));
    let chain_space = SpaceId::from_bytes(pat16(0x72));
    let mut cases = Vec::new();
    for (alg, names) in [
        (
            1u8,
            [
                "alg1_empty_space_epoch_3",
                "alg1_32_bytes_member_key",
                "alg1_68_bytes_chain_epoch_7",
            ],
        ),
        (
            2u8,
            [
                "alg2_empty_space_epoch_3",
                "alg2_32_bytes_member_key",
                "alg2_68_bytes_chain_epoch_7",
            ],
        ),
    ] {
        for (j, name) in names.into_iter().enumerate() {
            let i = (alg as usize - 1) * 3 + j;
            let (plaintext, ctx) = match j {
                0 => (
                    Vec::new(),
                    Ctx::space_epoch(&space, Epoch::new(3).expect("epoch 3")),
                ),
                1 => (pat(0x00, 32), Ctx::member_key(&member)),
                _ => (
                    pat(0xa0, 68),
                    Ctx::chain(&chain_space, Epoch::new(7).expect("epoch 7")),
                ),
            };
            cases.push(KatCase {
                name,
                alg,
                eph: 0x10 + 4 * i as u8,
                m: 0x30 + 4 * i as u8,
                nonce: 0x40 + 4 * i as u8,
                plaintext,
                ctx,
            });
        }
    }
    cases
}

/// The recipient of the known-answer vectors.
pub fn kat_recipient() -> RecipientSecret {
    RecipientSecret::new(
        &Secret::from_bytes(pat32(KAT_KX)),
        Some(&Secret::from_bytes(pat64(KAT_PQ))),
    )
}

/// The signing key of the known-answer vectors.
pub fn kat_signer() -> SignKeypair {
    SignKeypair::from_seed(&Secret::from_bytes(pat32(KAT_SIGNER)))
}

/// The device of the known-answer vectors.
pub fn kat_device() -> DeviceId {
    DeviceId::from_bytes(pat16(KAT_DEVICE))
}

/// The directory of the known-answer vectors: the one device, valid always.
pub fn kat_directory() -> MapDirectory {
    let mut dir = MapDirectory::new();
    dir.insert_active(kat_device(), kat_signer().verify_key());
    dir
}

/// The envelope of a known-answer case: the draws are asserted to be exactly the case's script,
/// in the documented order and sizes.
#[cfg(feature = "pq")]
pub fn kat_envelope(case: &KatCase) -> Envelope {
    let me = kat_recipient();
    let mut ent = case.entropy();
    let env = if case.alg == 1 {
        wrap_x25519(
            &me.recipient_x25519(),
            &case.ctx,
            &case.plaintext,
            &kat_device(),
            &kat_signer(),
            &mut ent,
        )
    } else {
        wrap_hybrid(
            &me.recipient_hybrid().expect("an ML-KEM key"),
            &case.ctx,
            &case.plaintext,
            &kat_device(),
            &kat_signer(),
            &mut ent,
        )
    }
    .expect("a known-answer wrap");
    let want: Vec<usize> = if case.alg == 1 {
        vec![32, 24]
    } else {
        vec![32, 32, 24]
    };
    assert_eq!(
        ent.draws, want,
        "{}: the draws of the construction",
        case.name
    );
    assert_eq!(ent.left(), 0, "{}: every scripted byte is used", case.name);
    env
}
