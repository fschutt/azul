//! Helpers the tests of the key hierarchy (`member`, `group`, `space`) share: byte patterns, a
//! scripted entropy source and a recording one, devices with a directory that knows them, and the
//! vector file format (`vectors`).
//!
//! Everything here builds on the crate's public API; nothing reads the crate's private state.
#![allow(dead_code)]

pub mod vectors;

use azlin_keys::member::KeyBundle;
use azlin_keys::{DeviceId, Entropy, EntropyError, MapDirectory, SignKeypair};

/// `len` bytes counting up from `start` (wrapping): the patterns of the known-answer inputs, so
/// that an independent program can rebuild them without a file.
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

/// Whether `a` and `b` share a window of `len` bytes: two independent random draws never do.
pub fn share_a_window(a: &[u8], b: &[u8], len: usize) -> bool {
    a.windows(len)
        .any(|window| b.windows(len).any(|other| other == window))
}

/// SplitMix64: the seeded generator of the property tests (no `rand` in wave-1 crates). A failing
/// case prints its seed and index, and `SplitMix64::new(seed)` replays it.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> SplitMix64 {
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

/// Wraps an entropy source and records the size of every draw.
pub struct Recording<E: Entropy> {
    pub inner: E,
    pub draws: Vec<usize>,
}

impl<E: Entropy> Recording<E> {
    pub fn new(inner: E) -> Recording<E> {
        Recording {
            inner,
            draws: Vec::new(),
        }
    }

    /// The bytes drawn so far.
    pub fn total(&self) -> usize {
        self.draws.iter().sum()
    }
}

impl<E: Entropy> Entropy for Recording<E> {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        self.inner.fill(out)?;
        self.draws.push(out.len());
        Ok(())
    }
}

/// A device: its id and its keys (the same shape as a member key, drawn independently).
pub struct Device {
    pub id: DeviceId,
    pub keys: KeyBundle,
}

impl Device {
    pub fn new(ent: &mut dyn Entropy) -> Device {
        let id = DeviceId::generate(ent).expect("a device id");
        let keys = KeyBundle::generate(ent).expect("device keys");
        Device { id, keys }
    }

    pub fn signer(&self) -> SignKeypair {
        self.keys.signer()
    }
}

/// A directory that knows these devices as signers, valid at every time and position.
pub fn directory(devices: &[&Device]) -> MapDirectory {
    let mut dir = MapDirectory::new();
    for device in devices {
        dir.insert_active(device.id, device.keys.public().verify_key());
    }
    dir
}
