//! What azlin-compress relies on: every deflate stream the analysis accepts is rebuilt exactly,
//! and damaged input on either side (the deflate stream a user's file holds, the corrections a
//! damaged container holds) is an error, never a panic, a hang or a large allocation.

use std::{
    io::Write,
    panic::{catch_unwind, AssertUnwindSafe},
};

use azlin_preflate::{preflate_whole_deflate_stream, recreate_whole_deflate_stream, PreflateConfig};

/// xorshift64*: seeded, so every run checks the same cases.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

fn deflate(data: &[u8], level: u32) -> Vec<u8> {
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(level));
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// Plain texts of the shapes deflate meets: prose, random bytes, runs, and a mix.
fn plains(rng: &mut Rng) -> Vec<Vec<u8>> {
    const WORDS: &[&str] = &["the", "file", "drive", "stored", "bytes", "and", "back", "again", "every", "one"];
    let mut prose = Vec::new();
    while prose.len() < 12_000 {
        prose.extend_from_slice(WORDS[rng.below(WORDS.len())].as_bytes());
        prose.push(if rng.below(9) == 0 { b'\n' } else { b' ' });
    }
    let mut mixed = rng.bytes(3000);
    mixed.extend(std::iter::repeat_n(7u8, 2000));
    mixed.extend_from_slice(&prose[..4000]);
    vec![prose, rng.bytes(6000), vec![0u8; 9000], mixed, b"x".to_vec()]
}

fn streams(rng: &mut Rng) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for plain in plains(rng) {
        for level in [0, 1, 6, 9] {
            out.push(deflate(&plain, level));
        }
    }
    out
}

fn config() -> PreflateConfig {
    PreflateConfig {
        verify_compression: true,
        ..PreflateConfig::default()
    }
}

#[test]
fn every_stream_the_analysis_accepts_is_rebuilt_exactly() {
    let mut rng = Rng(0x5EED_0001);
    for (i, stream) in streams(&mut rng).iter().enumerate() {
        let (chunk, plain) = preflate_whole_deflate_stream(stream, &config())
            .unwrap_or_else(|e| panic!("stream {i}: {e}"));
        assert_eq!(chunk.compressed_size, stream.len(), "stream {i}");
        let rebuilt = recreate_whole_deflate_stream(plain.text(), &chunk.corrections, stream.len())
            .unwrap_or_else(|e| panic!("stream {i}: {e}"));
        assert_eq!(rebuilt, *stream, "stream {i}");
        // The length is a hard cap: one byte less is an error.
        assert!(recreate_whole_deflate_stream(plain.text(), &chunk.corrections, stream.len() - 1).is_err());
    }
}

/// `bytes` damaged one of several ways.
fn mutate(rng: &mut Rng, bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    match rng.below(6) {
        0 if !out.is_empty() => {
            let at = rng.below(out.len());
            out[at] ^= 1 << rng.below(8);
        }
        1 => out.truncate(rng.below(out.len() + 1)),
        2 => {
            let len = 1 + rng.below(64);
            out.extend(rng.bytes(len));
        }
        3 if !out.is_empty() => {
            let at = rng.below(out.len());
            let len = (1 + rng.below(16)).min(out.len() - at);
            out[at..at + len].copy_from_slice(&rng.bytes(len));
        }
        4 => {
            let len = rng.below(bytes.len() + 64);
            out = rng.bytes(len);
        }
        _ => {
            for _ in 0..1 + rng.below(4) {
                if !out.is_empty() {
                    let at = rng.below(out.len());
                    out[at] = rng.next() as u8;
                }
            }
        }
    }
    out
}

#[test]
fn damaged_corrections_are_an_error_never_a_panic() {
    let mut rng = Rng(0x5EED_0002);
    for (i, stream) in streams(&mut rng).iter().enumerate() {
        let (chunk, plain) = preflate_whole_deflate_stream(stream, &config()).unwrap();
        for case in 0..150 {
            let corrections = mutate(&mut rng, &chunk.corrections);
            let text = if case % 5 == 0 { mutate(&mut rng, plain.text()) } else { plain.text().to_vec() };
            let result = catch_unwind(AssertUnwindSafe(|| {
                recreate_whole_deflate_stream(&text, &corrections, stream.len())
            }));
            match result {
                Ok(Ok(rebuilt)) => assert!(rebuilt.len() <= stream.len(), "stream {i} case {case}"),
                Ok(Err(_)) => {}
                Err(_) => panic!("stream {i} case {case}: the rebuild panicked"),
            }
        }
    }
}

#[test]
fn damaged_deflate_data_is_an_error_never_a_panic_and_what_is_accepted_is_rebuilt() {
    let mut rng = Rng(0x5EED_0003);
    let mut inputs: Vec<Vec<u8>> = Vec::new();
    for stream in streams(&mut rng) {
        for _ in 0..40 {
            inputs.push(mutate(&mut rng, &stream));
        }
    }
    for _ in 0..200 {
        let len = rng.below(3000);
        inputs.push(rng.bytes(len));
    }
    for (i, input) in inputs.iter().enumerate() {
        let result = catch_unwind(AssertUnwindSafe(|| preflate_whole_deflate_stream(input, &config())));
        match result {
            Ok(Ok((chunk, plain))) => {
                let rebuilt = recreate_whole_deflate_stream(plain.text(), &chunk.corrections, chunk.compressed_size)
                    .unwrap_or_else(|e| panic!("input {i}: accepted but not rebuilt: {e}"));
                assert_eq!(rebuilt, input[..chunk.compressed_size], "input {i}");
            }
            Ok(Err(_)) => {}
            Err(_) => panic!("input {i}: the analysis panicked"),
        }
    }
}
