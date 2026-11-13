//! An adaptive binary range coder: the one LZMA uses (11-bit probabilities, shift-5 adaptation,
//! a carry cache in the encoder). It stores preflate's corrections in place of the original
//! crate's CABAC coder, which comes from an LGPL-licensed crate.
//!
//! The encoder and decoder are adapted from lzma-rs 0.3.0 (`src/encode/rangecoder.rs` and
//! `src/decode/rangecoder.rs`), Copyright (c) 2017 - 2018 Guillaume Endignoux, MIT License:
//!
//! > Permission is hereby granted, free of charge, to any person obtaining a copy of this
//! > software and associated documentation files (the "Software"), to deal in the Software
//! > without restriction, including without limitation the rights to use, copy, modify, merge,
//! > publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons
//! > to whom the Software is furnished to do so, subject to the following conditions: The above
//! > copyright notice and this permission notice shall be included in all copies or substantial
//! > portions of the Software. THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND.
//!
//! Changes: the encoder writes into a `Vec` (no I/O errors), and the decoder reads from a slice
//! and reads zeros past its end instead of failing. A damaged stream then decodes to wrong
//! corrections, which the caller's checks reject; it never panics or reads out of bounds.

/// One adaptive probability: the chance of a 0 bit, in 1/2048.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prob(u16);

impl Default for Prob {
    fn default() -> Self {
        Prob(0x400)
    }
}

const PROB_BITS: u32 = 11;
const PROB_ONE: u16 = 1 << PROB_BITS;
const ADAPT_SHIFT: u32 = 5;
const TOP: u32 = 1 << 24;

/// The bits of a unary count written at most (a `u32`'s bit length is at most 32).
pub const MAX_UNARY: usize = 32;

pub struct RangeEncoder {
    out: Vec<u8>,
    range: u32,
    low: u64,
    cache: u8,
    cache_size: u64,
}

impl Default for RangeEncoder {
    fn default() -> Self {
        RangeEncoder {
            out: Vec::new(),
            range: 0xFFFF_FFFF,
            low: 0,
            cache: 0,
            cache_size: 1,
        }
    }
}

impl RangeEncoder {
    fn shift_low(&mut self) {
        if self.low < 0xFF00_0000 || self.low > 0xFFFF_FFFF {
            let carry = (self.low >> 32) as u8;
            let mut byte = self.cache;
            loop {
                self.out.push(byte.wrapping_add(carry));
                byte = 0xFF;
                self.cache_size -= 1;
                if self.cache_size == 0 {
                    break;
                }
            }
            self.cache = (self.low >> 24) as u8;
        }
        self.cache_size += 1;
        self.low = (self.low << 8) & 0xFFFF_FFFF;
    }

    /// Codes `bit` with the probability `prob`, then adapts it.
    pub fn put(&mut self, bit: bool, prob: &mut Prob) {
        let bound = (self.range >> PROB_BITS) * u32::from(prob.0);
        if bit {
            prob.0 -= prob.0 >> ADAPT_SHIFT;
            self.low += u64::from(bound);
            self.range -= bound;
        } else {
            prob.0 += (PROB_ONE - prob.0) >> ADAPT_SHIFT;
            self.range = bound;
        }
        while self.range < TOP {
            self.range <<= 8;
            self.shift_low();
        }
    }

    /// `count` (at most [`MAX_UNARY`]) as that many 1 bits and a closing 0 (none after the
    /// largest count); bit `i` uses `contexts[min(i, N - 1)]`.
    pub fn put_unary<const N: usize>(&mut self, count: usize, contexts: &mut [Prob; N]) {
        let count = count.min(MAX_UNARY);
        for i in 0..count {
            self.put(true, &mut contexts[i.min(N - 1)]);
        }
        if count < MAX_UNARY {
            self.put(false, &mut contexts[count.min(N - 1)]);
        }
    }

    /// The low `bits` bits of `value`, the highest first; bit `i` uses `contexts[min(i, N - 1)]`.
    pub fn put_bits<const N: usize>(&mut self, value: u32, bits: usize, contexts: &mut [Prob; N]) {
        let bits = bits.min(32);
        for i in 0..bits {
            let bit = (u64::from(value) >> (bits - 1 - i)) & 1 == 1;
            self.put(bit, &mut contexts[i.min(N - 1)]);
        }
    }

    /// The coded bytes.
    pub fn finish(mut self) -> Vec<u8> {
        for _ in 0..5 {
            self.shift_low();
        }
        self.out
    }
}

pub struct RangeDecoder<'a> {
    data: &'a [u8],
    pos: usize,
    range: u32,
    code: u32,
}

impl<'a> RangeDecoder<'a> {
    pub fn new(data: &'a [u8]) -> RangeDecoder<'a> {
        let mut decoder = RangeDecoder {
            data,
            pos: 0,
            range: 0xFFFF_FFFF,
            code: 0,
        };
        // The encoder's first byte is always the empty cache.
        decoder.next_byte();
        for _ in 0..4 {
            decoder.code = (decoder.code << 8) | u32::from(decoder.next_byte());
        }
        decoder
    }

    fn next_byte(&mut self) -> u8 {
        let byte = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos = self.pos.saturating_add(1);
        byte
    }

    /// The next bit, with the probability `prob`, which then adapts as the encoder's did.
    pub fn get(&mut self, prob: &mut Prob) -> bool {
        let bound = (self.range >> PROB_BITS) * u32::from(prob.0);
        let bit = if self.code < bound {
            prob.0 += (PROB_ONE - prob.0) >> ADAPT_SHIFT;
            self.range = bound;
            false
        } else {
            prob.0 -= prob.0 >> ADAPT_SHIFT;
            self.code = self.code.wrapping_sub(bound);
            self.range -= bound;
            true
        };
        while self.range < TOP {
            self.range <<= 8;
            self.code = (self.code << 8) | u32::from(self.next_byte());
        }
        bit
    }

    /// A count written by [`RangeEncoder::put_unary`].
    pub fn get_unary<const N: usize>(&mut self, contexts: &mut [Prob; N]) -> usize {
        let mut count = 0;
        while count < MAX_UNARY && self.get(&mut contexts[count.min(N - 1)]) {
            count += 1;
        }
        count
    }

    /// A value written by [`RangeEncoder::put_bits`].
    pub fn get_bits<const N: usize>(&mut self, bits: usize, contexts: &mut [Prob; N]) -> u32 {
        let bits = bits.min(32);
        let mut value = 0u64;
        for i in 0..bits {
            value = (value << 1) | u64::from(self.get(&mut contexts[i.min(N - 1)]));
        }
        value as u32
    }

    /// Whether the decoder read past the coded bytes (beyond the encoder's 5-byte flush).
    pub fn overran(&self) -> bool {
        self.pos > self.data.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_counts_and_values_come_back() {
        let mut seed = 0x1234_5678_9ABC_DEF0u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let ops: Vec<(u8, u32)> = (0..20_000)
            .map(|_| {
                let r = next();
                match r % 3 {
                    0 => (0, (r >> 8) as u32 & u32::from(r % 7 == 0)),
                    1 => (1, (r >> 8) as u32 % 33),
                    _ => (2, (r >> 8) as u32),
                }
            })
            .collect();
        let mut encoder = RangeEncoder::default();
        let (mut a, mut b, mut c) = ([Prob::default(); 1], [Prob::default(); 32], [Prob::default(); 32]);
        for &(op, v) in &ops {
            match op {
                0 => encoder.put(v == 1, &mut a[0]),
                1 => encoder.put_unary(v as usize, &mut b),
                _ => encoder.put_bits(v, 32, &mut c),
            }
        }
        let bytes = encoder.finish();
        let mut decoder = RangeDecoder::new(&bytes);
        let (mut a, mut b, mut c) = ([Prob::default(); 1], [Prob::default(); 32], [Prob::default(); 32]);
        for &(op, v) in &ops {
            let got = match op {
                0 => u32::from(decoder.get(&mut a[0])),
                1 => decoder.get_unary(&mut b) as u32,
                _ => decoder.get_bits(32, &mut c),
            };
            assert_eq!(got, v);
        }
        assert!(!decoder.overran());
    }

    #[test]
    fn garbage_decodes_without_panicking() {
        for len in 0..64 {
            let data: Vec<u8> = (0..len).map(|i| (i * 37 + 11) as u8).collect();
            let mut decoder = RangeDecoder::new(&data);
            let mut contexts = [Prob::default(); 32];
            for _ in 0..200 {
                let n = decoder.get_unary(&mut contexts);
                let _ = decoder.get_bits(n, &mut contexts);
            }
        }
    }
}
