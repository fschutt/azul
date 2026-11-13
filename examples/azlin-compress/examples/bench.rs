//! Measures the crate on a set of files: the saving, the encode and decode speed, and the
//! saving AZL1 gets today (zstd level 3 per 1 MiB segment with the 5 % rule, nothing for the
//! kinds its skip list names), for comparison. Every file is decoded and compared.
//!
//!     cargo run --release -p azlin-compress --example bench -- <upload|idle> <threads> <label> <files...>
//!
//! Peak memory is the process's, so run it once per set under `/usr/bin/time -l`.

use std::time::Instant;

use azlin_compress::{decode, encode, Options};

/// The magic numbers AZL1's skip list matches today (azul-storage `crypto/codec.rs`).
fn skipped_today(start: &[u8]) -> bool {
    const MAGIC: &[&[u8]] = &[
        b"\xFF\xD8\xFF", b"\x89PNG\r\n\x1A\n", b"GIF87a", b"GIF89a", b"PK\x03\x04", b"PK\x05\x06",
        b"PK\x07\x08", b"\x1F\x8B", b"\x28\xB5\x2F\xFD", b"\xFD7zXZ\x00", b"BZh",
        b"7z\xBC\xAF\x27\x1C", b"Rar!\x1A\x07", b"%PDF-", b"\x04\x22\x4D\x18", b"OggS", b"fLaC",
        b"ID3", b"\x1A\x45\xDF\xA3", b"\xFF\x0A", b"wOFF", b"wOF2", b"MSCF",
    ];
    MAGIC.iter().any(|m| start.starts_with(m))
        || (start.len() >= 12 && &start[4..8] == b"ftyp")
        || (start.len() >= 12 && start.starts_with(b"RIFF") && matches!(&start[8..12], b"WEBP" | b"AVI "))
}

/// The bytes AZL1 stores for `data` today (segment data only; framing left out on both sides).
fn stored_today(data: &[u8]) -> u64 {
    if skipped_today(data) {
        return data.len() as u64;
    }
    data.chunks(1 << 20)
        .map(|segment| {
            let compressed = zstd::bulk::compress(segment, 3).map_or(segment.len(), |c| c.len());
            if compressed * 100 <= segment.len() * 95 {
                compressed as u64
            } else {
                segment.len() as u64
            }
        })
        .sum()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 5 {
        eprintln!("usage: bench <upload|idle> <threads> <label> <files...>");
        std::process::exit(2);
    }
    let mut options = if args[1] == "idle" { Options::idle() } else { Options::upload() };
    options.threads = args[2].parse().unwrap_or(1);
    let label = &args[3];
    let (mut bytes_in, mut bytes_out, mut bytes_today) = (0u64, 0u64, 0u64);
    let (mut encode_secs, mut decode_secs) = (0f64, 0f64);
    let (mut files, mut transformed) = (0u32, 0u32);
    let mut savings = Vec::new();
    for path in &args[4..] {
        let Ok(data) = std::fs::read(path) else { continue };
        if data.is_empty() {
            continue;
        }
        let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
        let file_options = options.clone().with_extension(ext);
        let started = Instant::now();
        let encoded = match encode(&data, &file_options) {
            Ok(encoded) => encoded,
            Err(e) => {
                eprintln!("{path}: encode failed: {e}");
                continue;
            }
        };
        encode_secs += started.elapsed().as_secs_f64();
        let started = Instant::now();
        let decoded = decode(&encoded.bytes, &file_options).unwrap_or_else(|e| panic!("{path}: {e}"));
        decode_secs += started.elapsed().as_secs_f64();
        assert!(decoded[..] == data[..], "{path}: not bit-exact");
        files += 1;
        if encoded.report.transform != azlin_compress::TransformId::None {
            transformed += 1;
        }
        bytes_in += data.len() as u64;
        bytes_out += encoded.bytes.len() as u64;
        bytes_today += stored_today(&data);
        savings.push(100.0 * (1.0 - encoded.bytes.len() as f64 / data.len() as f64));
    }
    savings.sort_by(|a, b| a.total_cmp(b));
    let pick = |q: f64| savings.get(((savings.len().max(1) - 1) as f64 * q) as usize).copied().unwrap_or(0.0);
    let mb = bytes_in as f64 / 1e6;
    println!(
        "| {label} | {files} | {:.1} | {:.1} % | {:.1} % | {:.1} / {:.1} / {:.1} | {:.1} | {:.1} | {transformed} |",
        mb,
        100.0 * (1.0 - bytes_today as f64 / bytes_in.max(1) as f64),
        100.0 * (1.0 - bytes_out as f64 / bytes_in.max(1) as f64),
        pick(0.1),
        pick(0.5),
        pick(0.9),
        mb / encode_secs.max(1e-9),
        mb / decode_secs.max(1e-9),
    );
}
