//! Helpers the pack tests share: a seeded generator (SplitMix64, no `rand`), hex, the one-object-per-
//! line vector files (written only under `UPDATE_VECTORS=1`, never when `CI` is set), the scale of the
//! heavy tests, the entropy doubles, a way to make real chunk blobs, and the REFERENCES: an independent
//! implementation of the AZP2 layout that calls only `blake3`, `chacha20poly1305` and hand-written CBOR
//! (not the crate's pack functions), so that the golden vectors and the builder are held to the plan's
//! text and not to themselves, plus an incomplete-gamma routine for the chi-square tail of C12.
//!
//! A heavy test runs at the size of the brief in a release build (`cargo test --release`) and at a
//! smaller size in a debug build (the dependencies are unoptimised there: a 4 MiB keystream takes about a
//! second).
#![allow(dead_code)]

pub mod fuzz_entry;

use std::path::PathBuf;

use azlin_content::pack::{AddOutcome, PackBuilder, PackGeometry, Padding, SealKey, SealedPack};
use azlin_content::{
    seal_chunk, ChunkId, CompressPolicy, KChunk, PackClass, PackLen, SealedChunk, StoredOnly,
};
use azlin_keys::{Entropy, EntropyError, Epoch, Secret, SeededEntropy, SpaceId};
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

// ---------------------------------------------------------------------------------------------
// Generators, hex, scale
// ---------------------------------------------------------------------------------------------

/// SplitMix64: the seeded generator of every test stream. A failing case prints its seed and
/// `SplitMix64::new(seed)` replays it.
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

    /// Fills `out` with the generator's stream, eight little-endian bytes per output.
    pub fn fill(&mut self, out: &mut [u8]) {
        for piece in out.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            let n = piece.len();
            piece.copy_from_slice(&bytes[..n]);
        }
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut v = vec![0u8; n];
        self.fill(&mut v);
        v
    }
}

/// `len` bytes of the SplitMix64 stream of `seed`.
pub fn stream(seed: u64, len: usize) -> Vec<u8> {
    SplitMix64::new(seed).bytes(len)
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd hex length");
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("a hex digit"))
        .collect()
}

/// Whether the heavy tests run at the size of the brief: a build with optimisations
/// (`cargo test --release`), or `AZLIN_CONTENT_FULL=1`.
pub fn full_scale() -> bool {
    !cfg!(debug_assertions) || std::env::var_os("AZLIN_CONTENT_FULL").is_some()
}

/// `full` in a full-scale run, `debug` otherwise; prints which one ran (the report names it).
pub fn scaled(what: &str, full: usize, debug: usize) -> usize {
    let n = if full_scale() { full } else { debug };
    println!(
        "{what}: scale {n} ({})",
        if full_scale() { "full" } else { "debug" }
    );
    n
}

// ---------------------------------------------------------------------------------------------
// Entropy doubles, keys, chunks, geometries
// ---------------------------------------------------------------------------------------------

/// An entropy source that gives every draw these bytes (repeated if the draw is longer): the one
/// draw of a pack is its 24-byte nonce, so `FixedNonce(nonce)` makes a pack with that nonce.
pub struct FixedNonce(pub [u8; 24]);

impl Entropy for FixedNonce {
    fn fill(&mut self, out: &mut [u8]) -> Result<(), EntropyError> {
        for (dst, src) in out.iter_mut().zip(self.0.iter().cycle()) {
            *dst = *src;
        }
        Ok(())
    }
}

pub fn space(byte: u8) -> SpaceId {
    SpaceId::from_bytes([byte; 16])
}

pub fn k_chunk(byte: u8) -> KChunk {
    KChunk::from_secret(Secret::from_bytes([byte; 32]))
}

/// `K_seal` of epoch `epoch` whose 32 bytes are all `byte`.
pub fn seal_key(epoch: u32, byte: u8) -> SealKey {
    SealKey {
        epoch: Epoch::new(epoch).expect("an epoch"),
        key: Secret::from_bytes([byte; 32]),
    }
}

/// A real chunk blob of exactly this plaintext, sealed by `seal_chunk` with no compression, so the blob
/// is `plain.len() + 41` bytes and its first byte is the codec 0. Two chunks of one plaintext have one
/// id (equal chunks are stored once), so a test that wants distinct chunks gives distinct plaintexts.
pub fn chunk_of(sp: &SpaceId, k: &KChunk, plain: &[u8]) -> (ChunkId, SealedChunk) {
    let (meta, blob) =
        seal_chunk(sp, k, plain, &StoredOnly, CompressPolicy::plan()).expect("seals a chunk");
    (meta.id, blob)
}

/// A real chunk blob: `plain_len` plaintext bytes of the stream of `seed` (distinct seeds give distinct
/// plaintexts, hence distinct ids, from 8 bytes on), sealed as by [`chunk_of`].
pub fn chunk(sp: &SpaceId, k: &KChunk, seed: u64, plain_len: usize) -> (ChunkId, SealedChunk) {
    chunk_of(sp, k, &stream(seed, plain_len))
}

/// A geometry of small packs, so that the property tests make thousands of them: 32 KiB, at most
/// 40 KiB for a lone oversize chunk, 4 KiB steps, 40 lines.
pub fn small_geometry() -> PackGeometry {
    PackGeometry {
        target: PackLen::new(32 * 1024),
        hard_max: PackLen::new(40 * 1024),
        pad_step: PackLen::new(4 * 1024),
        max_chunks: 40,
    }
}

/// The builder's pack of `chunks` (all of which must fit one pack of `g`), the nonce drawn from `ent`.
#[allow(clippy::too_many_arguments)]
pub fn build_with(
    sp: &SpaceId,
    class: PackClass,
    epoch: u32,
    key_byte: u8,
    g: PackGeometry,
    padding: Padding,
    last: bool,
    chunks: &[(ChunkId, SealedChunk)],
    ent: &mut dyn Entropy,
) -> SealedPack {
    let mut builder = PackBuilder::new(*sp, class, seal_key(epoch, key_byte), g, padding);
    for (id, blob) in chunks {
        assert_eq!(
            builder.add(*id, blob).expect("adds"),
            AddOutcome::Fits,
            "the chunks fit one pack"
        );
    }
    builder.finish(last, ent).expect("finishes")
}

/// [`build_with`] with the seeded entropy double: the same `nonce_seed` gives the same pack.
#[allow(clippy::too_many_arguments)]
pub fn build(
    sp: &SpaceId,
    class: PackClass,
    epoch: u32,
    key_byte: u8,
    g: PackGeometry,
    padding: Padding,
    last: bool,
    chunks: &[(ChunkId, SealedChunk)],
    nonce_seed: u64,
) -> SealedPack {
    build_with(
        sp,
        class,
        epoch,
        key_byte,
        g,
        padding,
        last,
        chunks,
        &mut SeededEntropy::from_u64(nonce_seed),
    )
}

// ---------------------------------------------------------------------------------------------
// The vector files: one object per line, written only under UPDATE_VECTORS=1
// ---------------------------------------------------------------------------------------------

pub fn vector_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/vectors/{name}.json"))
}

/// The text of a vector file: `[`, one line per row, `]`.
pub fn render_rows(rows: &[String]) -> String {
    let mut out = String::from("[\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str("  ");
        out.push_str(row);
        out.push_str(if i + 1 < rows.len() { ",\n" } else { "\n" });
    }
    out.push_str("]\n");
    out
}

/// Writes the vector file under `UPDATE_VECTORS=1` (refused when `CI` is set), else compares the file
/// with `want` and fails with the first difference.
pub fn check_vector_file(name: &str, want: &str) {
    let path = vector_path(name);
    if std::env::var("UPDATE_VECTORS").is_ok_and(|v| v == "1") {
        assert!(
            std::env::var_os("CI").is_none(),
            "UPDATE_VECTORS=1 is refused when CI is set"
        );
        std::fs::create_dir_all(path.parent().expect("a parent folder")).expect("tests/vectors");
        std::fs::write(&path, want).expect("writes the vector file");
        return;
    }
    let have = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("tests/vectors/{name}.json is missing: write it once with UPDATE_VECTORS=1 (never in CI)")
    });
    if have != want {
        let at = have
            .bytes()
            .zip(want.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| have.len().min(want.len()));
        let line = have[..at.min(have.len())].matches('\n').count() + 1;
        panic!(
            "tests/vectors/{name}.json differs from the table of the test, first at byte {at} (line {line}); UPDATE_VECTORS=1 rewrites it, never in CI"
        );
    }
}

/// The rows of a vector file as the lines between `[` and `]`, without their trailing commas.
pub fn read_rows(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(vector_path(name)).expect("the vector file exists");
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with('{'))
        .map(|l| l.trim_end_matches(',').to_string())
        .collect()
}

/// The string field `"key": "value"` of a row (values are hex or plain words: no escapes).
pub fn field(row: &str, key: &str) -> String {
    let marker = format!("\"{key}\": \"");
    let start = row
        .find(&marker)
        .unwrap_or_else(|| panic!("no field {key} in {row}"))
        + marker.len();
    let end = row[start..].find('"').expect("a closing quote") + start;
    row[start..end].to_string()
}

/// The number field `"key": 123` of a row.
pub fn number(row: &str, key: &str) -> u64 {
    let marker = format!("\"{key}\": ");
    let start = row
        .find(&marker)
        .unwrap_or_else(|| panic!("no field {key} in {row}"))
        + marker.len();
    let digits: String = row[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().expect("a number")
}

// ---------------------------------------------------------------------------------------------
// The reference: the layout of the plan and the brief, written out without the crate's pack code
// ---------------------------------------------------------------------------------------------

/// What the reference builds a pack from.
#[derive(Clone)]
pub struct RefParams {
    pub space: [u8; 16],
    pub epoch: u32,
    pub k_seal: [u8; 32],
    pub class: u8,
    pub pad_len: usize,
    pub nonce: [u8; 24],
}

/// One chunk blob and the id its line carries.
#[derive(Clone)]
pub struct RefChunk {
    pub id: [u8; 16],
    pub blob: Vec<u8>,
}

/// A pack made by the reference, in its parts.
pub struct RefPack {
    pub bytes: Vec<u8>,
    pub header: [u8; 24],
    pub body: Vec<u8>,
    pub padding: Vec<u8>,
    pub trailer: Vec<u8>,
    pub plain: Vec<u8>,
}

/// The CBOR head of major type `major` for the number `n`, in the shortest form.
pub fn cbor_head(major: u8, n: u64) -> Vec<u8> {
    let m = major << 5;
    if n < 24 {
        vec![m | n as u8]
    } else if n < 256 {
        vec![m | 24, n as u8]
    } else if n < 65_536 {
        let mut v = vec![m | 25];
        v.extend_from_slice(&(n as u16).to_be_bytes());
        v
    } else if n < (1u64 << 32) {
        let mut v = vec![m | 26];
        v.extend_from_slice(&(n as u32).to_be_bytes());
        v
    } else {
        let mut v = vec![m | 27];
        v.extend_from_slice(&n.to_be_bytes());
        v
    }
}

/// The lines `(id, offset, length)` of chunks laid back to back after the 24-byte header.
pub fn ref_lines(chunks: &[RefChunk]) -> Vec<([u8; 16], u32, u32)> {
    let mut at = 24u32;
    let mut lines = Vec::new();
    for c in chunks {
        let len = c.blob.len() as u32;
        lines.push((c.id, at, len));
        at += len;
    }
    lines
}

/// The plaintext of a trailer, by hand: `{1: [[id, offset, length], ...], 2: body hash}` in
/// deterministic CBOR (`a2 01 <array> 02 58 20 <hash>`).
pub fn ref_plain(lines: &[([u8; 16], u32, u32)], body_hash: &[u8; 32]) -> Vec<u8> {
    let mut out = vec![0xa2, 0x01];
    out.extend(cbor_head(4, lines.len() as u64));
    for (id, offset, length) in lines {
        out.push(0x83);
        out.push(0x50);
        out.extend_from_slice(id);
        out.extend(cbor_head(0, u64::from(*offset)));
        out.extend(cbor_head(0, u64::from(*length)));
    }
    out.extend_from_slice(&[0x02, 0x58, 0x20]);
    out.extend_from_slice(body_hash);
    out
}

/// A pack whose trailer seals `plain` as it is, whatever it says: the header carries `chunk_count`,
/// the body and `p.pad_len` bytes of padding. The pieces are the plan's: the key of the trailer is
/// `derive_key("azlin 2026-11 pack trailer key v1", K_seal)`, the padding is the ciphertext of zero
/// bytes under `derive_key("azlin 2026-11 pack padding key v1", K_seal)` and the trailer's nonce, the
/// associated data is `"AZP2" || space || header`.
pub fn ref_seal(p: &RefParams, chunk_count: u32, body: &[u8], plain: &[u8]) -> RefPack {
    let mut header = [0u8; 24];
    header[0..4].copy_from_slice(b"AZP2");
    header[4] = 1;
    header[5] = p.class;
    header[8..12].copy_from_slice(&p.epoch.to_le_bytes());
    header[12..16].copy_from_slice(&chunk_count.to_le_bytes());
    header[16..20].copy_from_slice(&(body.len() as u32).to_le_bytes());
    header[20..24].copy_from_slice(&(p.pad_len as u32).to_le_bytes());

    let padding_key = blake3::derive_key("azlin 2026-11 pack padding key v1", &p.k_seal);
    let mut padding = vec![0u8; p.pad_len];
    if !padding.is_empty() {
        XChaCha20Poly1305::new(Key::from_slice(&padding_key))
            .encrypt_in_place_detached(XNonce::from_slice(&p.nonce), b"", &mut padding)
            .expect("pads");
    }

    let trailer_key = blake3::derive_key("azlin 2026-11 pack trailer key v1", &p.k_seal);
    let aad = [b"AZP2".as_slice(), p.space.as_slice(), header.as_slice()].concat();
    let mut ciphertext = plain.to_vec();
    let tag = XChaCha20Poly1305::new(Key::from_slice(&trailer_key))
        .encrypt_in_place_detached(XNonce::from_slice(&p.nonce), &aad, &mut ciphertext)
        .expect("seals");
    let mut trailer = p.nonce.to_vec();
    trailer.extend_from_slice(&ciphertext);
    trailer.extend_from_slice(&tag);

    let mut bytes = header.to_vec();
    bytes.extend_from_slice(body);
    bytes.extend_from_slice(&padding);
    bytes.extend_from_slice(&trailer);
    RefPack {
        bytes,
        header,
        body: body.to_vec(),
        padding,
        trailer,
        plain: plain.to_vec(),
    }
}

/// The pack of `chunks`, as the plan and the brief say it is made.
pub fn ref_pack(p: &RefParams, chunks: &[RefChunk]) -> RefPack {
    let body: Vec<u8> = chunks.iter().flat_map(|c| c.blob.iter().copied()).collect();
    let plain = ref_plain(&ref_lines(chunks), blake3::hash(&body).as_bytes());
    ref_seal(p, chunks.len() as u32, &body, &plain)
}

/// The length of the pack of blobs of these lengths before padding: the header, the blobs and the
/// trailer (a nonce, the plaintext of the lines and the hash, a tag), by the layout.
pub fn ref_unpadded_from_lens(lens: &[usize]) -> usize {
    let mut at = 24u32;
    let mut lines = Vec::new();
    for &len in lens {
        lines.push(([0u8; 16], at, len as u32));
        at += len as u32;
    }
    let plain = ref_plain(&lines, &[0u8; 32]).len();
    24 + lens.iter().sum::<usize>() + 24 + plain + 16
}

/// The length of the pack of `chunks` before padding (header, body, trailer).
pub fn ref_unpadded_len(chunks: &[RefChunk]) -> usize {
    let lens: Vec<usize> = chunks.iter().map(|c| c.blob.len()).collect();
    ref_unpadded_from_lens(&lens)
}

/// The padded length by the rules of the plan and the brief, written again: a pack finished with
/// `last = false`, or under `none`, is unchanged; `buckets` rounds up to the step (4 KiB for the
/// repository class, `step` for the others), `full` goes to the cap; the cap is `target`, or
/// `hard_max` for a pack that is already above `target`; nothing goes above the cap.
pub fn ref_padded_len(
    unpadded: usize,
    class: u8,
    policy: &str,
    last: bool,
    g: &PackGeometry,
) -> usize {
    if !last || policy == "none" {
        return unpadded;
    }
    let target = g.target.as_usize();
    let cap = if unpadded <= target {
        target
    } else {
        g.hard_max.as_usize()
    };
    match policy {
        "buckets" => {
            let step = if class == 2 {
                4096
            } else {
                g.pad_step.as_usize()
            };
            (unpadded.div_ceil(step) * step).min(cap)
        }
        "full" => cap,
        other => panic!("no padding policy {other}"),
    }
}

// ---------------------------------------------------------------------------------------------
// The chi-square tail, for C12: an incomplete-gamma routine of the test's own
// ---------------------------------------------------------------------------------------------

/// ln of the gamma function, by the Lanczos approximation with g = 7 and nine terms (about 1e-13
/// absolute at the arguments used here).
pub fn ln_gamma(x: f64) -> f64 {
    const COEF: [f64; 9] = [
        0.9999999999998099,
        676.5203681218851,
        -1259.1392167224028,
        771.3234287776531,
        -176.6150291621406,
        12.507343278686905,
        -0.13857109526572012,
        9.984369578019572e-6,
        1.5056327351493116e-7,
    ];
    let x = x - 1.0;
    let t = x + 7.5;
    let mut a = COEF[0];
    for (i, c) in COEF.iter().enumerate().skip(1) {
        a += c / (x + i as f64);
    }
    0.5 * (2.0 * std::f64::consts::PI).ln() + (x + 0.5) * t.ln() - t + a.ln()
}

/// The probability that a chi-square variable with `k` degrees of freedom exceeds `x`: the regularised
/// upper incomplete gamma Q(k/2, x/2), by the series for P (then 1 - P) below the mean and the
/// continued fraction (modified Lentz) for Q above it.
pub fn chi2_upper_tail(x: f64, k: f64) -> f64 {
    let a = k / 2.0;
    let xx = x / 2.0;
    if xx < a + 1.0 {
        let mut ap = a;
        let mut total = 1.0 / a;
        let mut delta = total;
        for _ in 0..100_000 {
            ap += 1.0;
            delta *= xx / ap;
            total += delta;
            if delta.abs() < total.abs() * 1e-16 {
                break;
            }
        }
        return 1.0 - total * (-xx + a * xx.ln() - ln_gamma(a)).exp();
    }
    let tiny = 1e-300;
    let mut b = xx + 1.0 - a;
    let mut c = 1.0 / tiny;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..100_000 {
        let an = -(i as f64) * (i as f64 - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < tiny {
            d = tiny;
        }
        c = b + an / c;
        if c.abs() < tiny {
            c = tiny;
        }
        d = 1.0 / d;
        let delta = d * c;
        h *= delta;
        if (delta - 1.0).abs() < 1e-16 {
            break;
        }
    }
    (-xx + a * xx.ln() - ln_gamma(a)).exp() * h
}

/// The `x` whose upper tail is `p`, by bisection.
pub fn chi2_quantile(p: f64, k: f64) -> f64 {
    let (mut lo, mut hi) = (k, 20.0 * k);
    for _ in 0..300 {
        let mid = 0.5 * (lo + hi);
        if chi2_upper_tail(mid, k) > p {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Pearson's statistic of the byte histogram of `sample` against the uniform distribution, the long
/// way: `sum (c - n/256)^2 / (n/256)`.
pub fn chi_square_of(sample: &[u8]) -> f64 {
    let mut counts = [0u64; 256];
    for &b in sample {
        counts[usize::from(b)] += 1;
    }
    let expected = sample.len() as f64 / 256.0;
    counts
        .iter()
        .map(|&c| {
            let d = c as f64 - expected;
            d * d / expected
        })
        .sum()
}
