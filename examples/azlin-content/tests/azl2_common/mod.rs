//! Helpers the chunker and AZL2 test files share: a seeded generator (SplitMix64, no `rand`), hex,
//! the one-object-per-line vector files (written only under `UPDATE_VECTORS=1`, never when `CI` is
//! set; a tiny reader of the fields this crate writes, no JSON dependency), the scale of the heavy
//! tests, and the doubles of the `Compressor` seam.
//!
//! A heavy test runs at the size of the brief in a release build (`cargo test --release`) and, where a
//! debug build would take minutes (C5, C6), at a smaller size there: the lane's final hold runs both,
//! the other lanes' holds (which run the whole crate's tests) run the smaller one.
#![allow(dead_code)]

pub mod fuzz_entry;

use std::cell::Cell;
use std::path::PathBuf;

use azlin_content::{ChunkLen, Codec, CodecError, Compressed, Compressor, KChunk};
use azlin_keys::{Secret, SpaceId};
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

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

/// Data that compresses well under `RleDouble`: runs of 1 to 600 equal bytes.
pub fn runs(seed: u64, len: usize) -> Vec<u8> {
    let mut rng = SplitMix64::new(seed);
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        let run = 1 + rng.below(600) as usize;
        let byte = rng.next_u64().to_le_bytes()[0];
        let n = run.min(len - out.len());
        out.resize(out.len() + n, byte);
    }
    out
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

pub fn space(byte: u8) -> SpaceId {
    SpaceId::from_bytes([byte; 16])
}

pub fn k_chunk(byte: u8) -> KChunk {
    KChunk::from_secret(Secret::from_bytes([byte; 32]))
}

// ---------------------------------------------------------------------------------------------
// The scale of the heavy tests
// ---------------------------------------------------------------------------------------------

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

pub const MIB: usize = 1024 * 1024;

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
// The doubles of the Compressor seam
// ---------------------------------------------------------------------------------------------

/// Run-length coding as the codec `Zstd`: pairs of `(count 1..=255, byte)`. It compresses what
/// `runs` makes, and obeys `max_len`.
pub struct RleDouble;

pub fn rle_encode(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let byte = data[i];
        let mut n = 1;
        while i + n < data.len() && data[i + n] == byte && n < 255 {
            n += 1;
        }
        out.push(n as u8);
        out.push(byte);
        i += n;
    }
    out
}

impl Compressor for RleDouble {
    fn compress(&self, plaintext: &[u8]) -> Result<Option<Compressed>, CodecError> {
        Ok(Some((Codec::Zstd, Zeroizing::new(rle_encode(plaintext)))))
    }

    fn decompress(
        &self,
        _codec: Codec,
        stored: &[u8],
        max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
        if !stored.len().is_multiple_of(2) {
            return Err(CodecError::Damaged);
        }
        let mut out = Zeroizing::new(Vec::new());
        for pair in stored.chunks_exact(2) {
            let (count, byte) = (usize::from(pair[0]), pair[1]);
            if count == 0 {
                return Err(CodecError::Damaged);
            }
            if out.len() + count > max_len.as_usize() {
                return Err(CodecError::TooLong);
            }
            let end = out.len() + count;
            out.resize(end, byte);
        }
        Ok(out)
    }
}

/// Never compresses; reads any codec as "the stored bytes are the plaintext" (for blobs whose
/// codec byte is only a label), obeying `max_len`.
pub struct IdentityDouble;

impl Compressor for IdentityDouble {
    fn compress(&self, _plaintext: &[u8]) -> Result<Option<Compressed>, CodecError> {
        Ok(None)
    }

    fn decompress(
        &self,
        _codec: Codec,
        stored: &[u8],
        max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
        if stored.len() > max_len.as_usize() {
            return Err(CodecError::TooLong);
        }
        Ok(Zeroizing::new(stored.to_vec()))
    }
}

/// "Compresses" to a stream of exactly `n` bytes (zeros), for the tests of the 5 % rule; reads
/// nothing back.
pub struct SizedDouble(pub usize);

impl Compressor for SizedDouble {
    fn compress(&self, _plaintext: &[u8]) -> Result<Option<Compressed>, CodecError> {
        Ok(Some((Codec::Zstd, Zeroizing::new(vec![0u8; self.0]))))
    }

    fn decompress(
        &self,
        codec: Codec,
        _stored: &[u8],
        _max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
        Err(CodecError::Unsupported(codec.byte()))
    }
}

/// A decompression bomb: it would produce 1 GiB. It obeys `max_len` as the seam asks (it stops
/// at the first byte past it) and counts how many bytes it produced.
#[derive(Default)]
pub struct BombDouble {
    pub produced: Cell<usize>,
    pub max_len_seen: Cell<usize>,
}

impl Compressor for BombDouble {
    fn compress(&self, _plaintext: &[u8]) -> Result<Option<Compressed>, CodecError> {
        Ok(None)
    }

    fn decompress(
        &self,
        _codec: Codec,
        _stored: &[u8],
        max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
        self.max_len_seen.set(max_len.as_usize());
        let mut out = Zeroizing::new(Vec::new());
        for produced in 0..(1usize << 30) {
            if produced >= max_len.as_usize() {
                self.produced.set(produced + 1);
                return Err(CodecError::TooLong);
            }
            out.push(0xAA);
        }
        self.produced.set(out.len());
        Ok(out)
    }
}

/// Gives back a stream of the length it chooses, whatever `max_len` says (a compressor that does not
/// obey the seam): the opener's own length check is the last line.
pub struct WrongLengthDouble(pub isize);

impl Compressor for WrongLengthDouble {
    fn compress(&self, _plaintext: &[u8]) -> Result<Option<Compressed>, CodecError> {
        Ok(None)
    }

    fn decompress(
        &self,
        _codec: Codec,
        _stored: &[u8],
        max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
        let n = (max_len.as_usize() as isize + self.0).max(0) as usize;
        Ok(Zeroizing::new(vec![0x55; n]))
    }
}

// ---------------------------------------------------------------------------------------------
// The references: the plan's text, recomputed here without the lane's functions
// ---------------------------------------------------------------------------------------------

/// The numbers of the chunker, typed from the plan and the lane's documentation, not read from the
/// crate: a change of the crate's constants is a failing test, not a silently moved reference.
pub const MIN: usize = 262_144;
pub const NORMAL: usize = 868_352;
pub const MAX: usize = 4_194_304;
pub const STRICT: u64 = 0xFFFF_FC00_0000_0000;
pub const LOOSE: u64 = 0xFFFF_C000_0000_0000;

/// A gear table recomputed with blake3 calls only: the `derive_key`-mode XOF of `context` over
/// `material`, 2,048 bytes read as 256 little-endian `u64`.
pub fn reference_table(context: &str, material: &[u8]) -> [u64; 256] {
    let mut hasher = blake3::Hasher::new_derive_key(context);
    hasher.update(material);
    let mut bytes = [0u8; 2048];
    hasher.finalize_xof().fill(&mut bytes);
    let mut table = [0u64; 256];
    for (i, entry) in table.iter_mut().enumerate() {
        let mut word = [0u8; 8];
        word.copy_from_slice(&bytes[8 * i..8 * i + 8]);
        *entry = u64::from_le_bytes(word);
    }
    table
}

pub fn public_reference_table() -> [u64; 256] {
    reference_table("azlin 2026-11 fastcdc gear v1", &[])
}

pub fn keyed_reference_table(k_chunk: &[u8; 32]) -> [u64; 256] {
    reference_table("azlin 2026-11 fastcdc gear keyed v1", k_chunk)
}

/// The end offsets of the chunks of `data`, by the documented algorithm written out per chunk with
/// explicit indices (the library runs a one-pass state machine; this is the other formulation):
/// at most `MIN` bytes left is the last chunk; else the 63 bytes at lengths `MIN - 64 .. MIN - 1` are
/// hashed, and from the length `MIN` on each byte is hashed and the fingerprint tested against the
/// strict mask below `NORMAL` and the loose mask from there; the cut is at `MAX` at the latest.
pub fn reference_cuts(data: &[u8], gear: &[u64; 256]) -> Vec<usize> {
    let mut ends = Vec::new();
    let mut start = 0usize;
    while start < data.len() {
        let rest = &data[start..];
        let n = rest.len();
        let len = if n <= MIN {
            n
        } else {
            let limit = n.min(MAX);
            let mut fp = 0u64;
            for &byte in &rest[MIN - 64..MIN - 1] {
                fp = (fp << 1).wrapping_add(gear[usize::from(byte)]);
            }
            let mut cut = limit;
            for (i, &byte) in rest.iter().enumerate().take(limit).skip(MIN - 1) {
                fp = (fp << 1).wrapping_add(gear[usize::from(byte)]);
                let length = i + 1;
                let mask = if length < NORMAL { STRICT } else { LOOSE };
                if fp & mask == 0 {
                    cut = length;
                    break;
                }
            }
            cut
        };
        start += len;
        ends.push(start);
    }
    ends
}

/// A chunk of the plan (5.7) recomputed with blake3 and chacha20poly1305 calls only.
pub struct Sealed {
    pub full: [u8; 32],
    pub id: [u8; 16],
    pub key: [u8; 32],
    pub nonce: [u8; 24],
    pub blob: Vec<u8>,
}

/// `full = keyed-BLAKE3(K_chunk, plaintext)`, `id = derive_key("azlin chunk id v1", full)[0..16]`,
/// `key = derive_key("azlin chunk key v1", full)`, `nonce = keyed-BLAKE3(key, codec || stored)[0..24]`,
/// the associated data `magic || space || id || codec`, XChaCha20-Poly1305: the blob
/// `codec || nonce || ciphertext || tag`. `identity_of` is what the chunk is named after (the
/// plaintext), `stored` what is encrypted (the plaintext itself for codec 0, or other bytes).
pub fn independent_seal(
    k_chunk: &[u8; 32],
    space: &[u8; 16],
    identity_of: &[u8],
    codec: u8,
    stored: &[u8],
    magic: &[u8],
) -> Sealed {
    let full = *blake3::keyed_hash(k_chunk, identity_of).as_bytes();
    let id_hash = blake3::derive_key("azlin chunk id v1", &full);
    let mut id = [0u8; 16];
    id.copy_from_slice(&id_hash[..16]);
    let key = blake3::derive_key("azlin chunk key v1", &full);
    let mut hasher = blake3::Hasher::new_keyed(&key);
    hasher.update(&[codec]);
    hasher.update(stored);
    let hash = hasher.finalize();
    let mut nonce = [0u8; 24];
    nonce.copy_from_slice(&hash.as_bytes()[..24]);
    let aad = [magic, &space[..], &id[..], &[codec][..]].concat();
    let cipher = XChaCha20Poly1305::new(Key::from_slice(&key));
    let mut ciphertext = stored.to_vec();
    let tag = cipher
        .encrypt_in_place_detached(XNonce::from_slice(&nonce), &aad, &mut ciphertext)
        .expect("encrypts");
    let mut blob = vec![codec];
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    blob.extend_from_slice(&tag);
    Sealed {
        full,
        id,
        key,
        nonce,
        blob,
    }
}

/// A compressor that always fails.
pub struct FailingDouble;

impl Compressor for FailingDouble {
    fn compress(&self, _plaintext: &[u8]) -> Result<Option<Compressed>, CodecError> {
        Err(CodecError::Failed)
    }

    fn decompress(
        &self,
        _codec: Codec,
        _stored: &[u8],
        _max_len: ChunkLen,
    ) -> Result<Zeroizing<Vec<u8>>, CodecError> {
        Err(CodecError::Damaged)
    }
}
