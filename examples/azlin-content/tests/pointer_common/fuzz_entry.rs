//! The entry points of the fuzz targets `pointer_decode` and `chunk_list_decode`
//! (`fuzz/fuzz_targets/*.rs` include this file by path) and of the seeded run of C13: the same code, so
//! what the fuzzer is told to find, the ordinary tests look for on 20,000 fixed inputs.
//!
//! It uses only the public API of the crate and the dependencies of the fuzz project (azlin-cbor,
//! azlin-keys with its test doubles, zeroize), and holds one rule against every input: nothing here may
//! panic, and what `from_cbor` accepts writes back to exactly the bytes that were read (the signature
//! rule: the signature covers canonical bytes).
#![allow(dead_code)]

use std::sync::OnceLock;

use azlin_cbor::{Limits, Value};
use azlin_content::pointer::{
    decode_chunk_list, encode_chunk_list, object_name, ChunkList, ChunkListRef, Decoded,
    ObjectSealer, PointerV2, Preview, SealError, UnknownFields,
};
use azlin_content::{ChunkLen, ChunkRef, FileSize, FileTime, Full, PackId, PackOffset};
use azlin_keys::{DeviceId, MapDirectory, Secret, SignKeypair, SpaceId, UnixTime, VerifyAt};
use zeroize::Zeroizing;

/// A sealer that seals nothing: the sealed object is the plaintext (and the name is the BLAKE3 of the
/// plaintext). It lets the entry reach the CBOR reader of the list with any bytes.
pub struct IdentitySealer;

impl ObjectSealer for IdentitySealer {
    fn seal(&self, _context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        Ok(plaintext.to_vec())
    }

    fn open(&self, _context: &[u8], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        Ok(Zeroizing::new(sealed.to_vec()))
    }
}

fn fuzz_space() -> SpaceId {
    SpaceId::from_bytes([1; 16])
}

fn fuzz_author() -> DeviceId {
    DeviceId::from_bytes([2; 16])
}

fn fuzz_key() -> SignKeypair {
    SignKeypair::from_seed(&Secret::from_bytes([3; 32]))
}

fn fuzz_directory() -> MapDirectory {
    let mut d = MapDirectory::new();
    d.insert_active(fuzz_author(), fuzz_key().verify_key());
    d
}

fn fuzz_chunk(tag: u8, raw: u32) -> ChunkRef {
    ChunkRef {
        full: Full::from_bytes([tag; 32]),
        raw_len: ChunkLen::new(raw).expect("a length"),
        pack: PackId::from_bytes([tag; 16]),
        offset: PackOffset::new(u32::from(tag) * 64),
        stored_len: ChunkLen::new(raw + 41).expect("a length"),
    }
}

fn build_valid_pointer() -> Vec<u8> {
    let mut p = PointerV2 {
        size: FileSize::new(1500),
        mtime: FileTime::new(-5, 7).expect("a time"),
        mode: 0o644,
        blake3: [9; 32],
        transform: None,
        chunks: ChunkList::Inline(vec![fuzz_chunk(1, 1000), fuzz_chunk(2, 500)]),
        author_device: fuzz_author(),
        sig: None,
        previews: vec![Preview {
            kind: 1,
            w: 64,
            h: 64,
            chunks: vec![fuzz_chunk(3, 300)],
        }],
        streams: Vec::new(),
        xattrs: vec![("user.a".to_string(), vec![1, 2, 3])],
        labels: vec!["label".to_string()],
        derived: Vec::new(),
        created_at: UnixTime::from_secs(1_700_000_000),
        unknown: UnknownFields::from_entries(vec![(40, Value::Text("newer".to_string()))])
            .expect("ascending keys"),
        crit: vec![7],
    };
    p.sign(&fuzz_space(), &fuzz_key())
        .expect("the seed pointer signs");
    p.to_cbor().expect("the seed pointer writes").to_vec()
}

fn build_valid_list() -> (Vec<u8>, ChunkListRef) {
    let mut refs = Vec::with_capacity(1025);
    for i in 0..1025u32 {
        refs.push(fuzz_chunk((i % 250) as u8 + 1, 100 + i % 5));
    }
    encode_chunk_list(&refs, &fuzz_space(), &IdentitySealer).expect("the list encodes")
}

static VALID_POINTER: OnceLock<Vec<u8>> = OnceLock::new();
static VALID_LIST: OnceLock<(Vec<u8>, ChunkListRef)> = OnceLock::new();

/// A valid signed pointer with a preview, an attribute, a label, a `crit` list and a field a newer minor
/// version wrote: the seed of the structured inputs.
pub fn valid_pointer_bytes() -> &'static [u8] {
    VALID_POINTER.get_or_init(build_valid_pointer).as_slice()
}

/// A valid chunk-list object (1,025 chunks) in its plaintext form (the identity sealer: the sealed
/// bytes are the plaintext), with the reference that names it.
pub fn valid_list() -> &'static (Vec<u8>, ChunkListRef) {
    VALID_LIST.get_or_init(build_valid_list)
}

/// What every input must satisfy: no panic; an accepted pointer writes back to the input and can be
/// verified (any answer, no panic).
fn check_pointer(bytes: &[u8], dir: &MapDirectory) {
    match PointerV2::from_cbor(bytes, &Limits::OBJECT) {
        Ok(Decoded::Current(p)) => {
            let again = p.to_cbor().expect("an accepted pointer writes back");
            assert_eq!(
                again.as_slice(),
                bytes,
                "an accepted pointer must be in its one canonical encoding"
            );
            let _ = p.verify(&fuzz_space(), dir, VerifyAt::Now);
            let _ = p.max_decoded_len();
            let _ = p.chunk_refs().count();
        }
        Ok(Decoded::Newer(_)) | Err(_) => {}
    }
}

/// One fuzz input of `pointer_decode`: the bytes themselves are tried as a pointer; and as a list of
/// `(position, xor)` triples that damage a valid signed pointer (so the inputs reach the fields, the
/// limits, the sums and the signature check, which random bytes almost never do).
pub fn pointer_decode(data: &[u8]) {
    let dir = fuzz_directory();
    check_pointer(data, &dir);

    let valid = valid_pointer_bytes();
    let mut bytes = valid.to_vec();
    for triple in data.as_chunks::<3>().0 {
        let at = usize::from(u16::from_be_bytes([triple[0], triple[1]])) % bytes.len();
        bytes[at] ^= triple[2];
    }
    check_pointer(&bytes, &dir);

    // The first byte of the input also picks a truncation or an extension.
    if let Some(first) = data.first() {
        let cut = usize::from(*first) % (valid.len() + 1);
        check_pointer(&valid[..cut], &dir);
        let mut longer = valid.to_vec();
        longer.extend_from_slice(data);
        check_pointer(&longer, &dir);
    }
}

/// The cheap part of `chunk_list_decode`: `selector (1) || count offset (2, big-endian) || bytes`. The
/// bytes are handed to `decode_chunk_list` as a sealed object (the identity sealer makes them the
/// plaintext) under a declared count of 1,025 to 1,124 and the name the bytes have, so the checks past
/// the name run.
pub fn chunk_list_decode_bytes(data: &[u8]) {
    let Some((selector, rest)) = data.split_first() else {
        return;
    };
    let Some((count_bytes, bytes)) = rest.split_first_chunk::<2>() else {
        return;
    };
    let count = 1025 + u32::from(u16::from_be_bytes(*count_bytes)) % 100;
    let declared = ChunkListRef {
        name: object_name(bytes),
        count,
        total_raw: FileSize::new(u64::from(count) * 100 + u64::from(*selector)),
    };
    let result = decode_chunk_list(bytes, &declared, &fuzz_space(), &IdentitySealer);
    if let Ok(refs) = &result {
        assert_eq!(refs.len(), count as usize, "an accepted list has its count");
    }
}

/// The heavy part: a valid object (about 70 KB) damaged by the input's `(position, xor)` triples. The name
/// follows the damaged bytes, so only the object's own checks can refuse it; the undamaged object under
/// the damaged one's name is a name mismatch.
pub fn chunk_list_decode_damaged(data: &[u8]) {
    let (valid, list) = valid_list();
    let mut damaged = valid.clone();
    for triple in data.as_chunks::<3>().0 {
        let at = usize::from(u16::from_be_bytes([triple[0], triple[1]])) % damaged.len();
        damaged[at] ^= triple[2];
    }
    let declared = ChunkListRef {
        name: object_name(&damaged),
        count: list.count,
        total_raw: list.total_raw,
    };
    let _ = decode_chunk_list(&damaged, &declared, &fuzz_space(), &IdentitySealer);
    let _ = decode_chunk_list(valid, &declared, &fuzz_space(), &IdentitySealer);
}

/// One fuzz input of `chunk_list_decode`: both parts.
pub fn chunk_list_decode(data: &[u8]) {
    chunk_list_decode_bytes(data);
    chunk_list_decode_damaged(data);
}
