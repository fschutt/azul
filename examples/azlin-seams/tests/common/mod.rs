//! Helpers the test files share: hex, and two entropy sources that need no test-support feature of
//! azlin-keys (this crate's manifest does not forward it: DRIFT X-DELTA-W1S17A-1).
#![allow(dead_code)]

use azlin_seams::{Entropy, EntropyError};

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd hex length: {text}");
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("hex digit"))
        .collect()
}

pub fn from_hex_array<const N: usize>(text: &str) -> [u8; N] {
    let bytes = from_hex(text);
    <[u8; N]>::try_from(bytes.as_slice()).unwrap_or_else(|_| panic!("{text}: not {N} bytes"))
}

/// An entropy source that counts: byte `i` of the stream is `i + 1` (mod 256).
pub struct CountingEntropy(u8);

impl CountingEntropy {
    pub fn new() -> CountingEntropy {
        CountingEntropy(0)
    }
}

impl Entropy for CountingEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        for byte in out {
            self.0 = self.0.wrapping_add(1);
            *byte = self.0;
        }
        Ok(())
    }
}

/// An entropy source that always fails.
pub struct DeadEntropy;

impl Entropy for DeadEntropy {
    fn fill(&mut self, _out: &mut [u8]) -> Result<(), EntropyError> {
        Err(EntropyError::Exhausted)
    }
}
