//! The entry points the fuzz targets of `examples/azul-storage/fuzz` call (ARCH28 RF50 / AR5b-5),
//! run here on inputs that need no fuzzer: empty and short ones, a few thousand of a fixed
//! pseudo-random sequence, and valid samples with single bits flipped. A panic is a failed test
//! here and an abort in the shipped profile; the fuzzer looks further than this does, and finds
//! what these inputs cannot - these keep the entry points panic-free in the meantime and show the
//! fuzz targets what they call.

use crate::{
    crypto::{
        azl1::{self, OpenObject, WriteOptions},
        keys::{KeyChain, MemberWrap, RecoveryCode, RecoveryKdf, RecoveryWrap},
        DriveKey, FileKey, ObjectId,
    },
    http1,
    meta::{pointer, PackIndex, TestSealer},
};

/// An AZL1 object read with a file key and an object id the input names: `key (32) || id (16) ||
/// object`.
pub(crate) fn azl1_object(data: &[u8]) {
    let Some((key, rest)) = data.split_at_checked(32) else {
        return;
    };
    let Some((id, object)) = rest.split_at_checked(16) else {
        return;
    };
    let Some(file_key) = FileKey::from_slice(key) else {
        return;
    };
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(id);
    let id = ObjectId(bytes);
    let _ = azl1::decrypt(object, &id, &file_key);
    if let Ok(open) = OpenObject::open_expecting(object, &id, &file_key, object.len() as u64) {
        let _ = open.read_all(object);
        let _ = open.read_range(object, 0, 100);
        let _ = open.read_range(object, 1 << 20, (1 << 20) + 100);
    }
}

/// The key files of `.azlin/keys/`.
pub(crate) fn key_files(data: &[u8]) {
    let _ = RecoveryWrap::parse(data);
    let _ = MemberWrap::parse(data);
    let _ = KeyChain::parse(data);
}

/// A node's answer as HTTP/1.1 bytes.
pub(crate) fn http1_answer(data: &[u8]) {
    for head_request in [false, true] {
        let _ = http1::decode_reply(data, head_request);
    }
}

/// A pointer blob of the drive index.
pub(crate) fn pointer_blob(data: &[u8]) {
    let _ = pointer::decode(data);
}

/// A sealed pack index of the drive index.
pub(crate) fn pack_index(data: &[u8]) {
    let sealer = TestSealer::new([5; 32]);
    let _ = PackIndex::open(&sealer, "pack", data);
}

fn every_entry(data: &[u8]) {
    azl1_object(data);
    key_files(data);
    http1_answer(data);
    pointer_blob(data);
    pack_index(data);
}

/// A fixed pseudo-random byte sequence (xorshift): the same inputs on every run.
fn noise(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn x_ar5b_5_the_fuzz_entry_points_take_any_bytes_without_a_panic() {
    for len in [0usize, 1, 2, 15, 16, 31, 32, 47, 48, 49, 100, 255, 500, 4096] {
        every_entry(&vec![0u8; len]);
        every_entry(&vec![0xffu8; len]);
        for seed in 1..40u64 {
            every_entry(&noise(seed * 7919 + len as u64, len));
        }
    }
    // Text that looks like what the parsers read.
    for text in [
        &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nffffffffffffffff\r\n"[..],
        b"HTTP/1.1 200 OK\r\nContent-Length: 99999999999999999999\r\n\r\n",
        b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 100 Continue\r\n\r\n",
        b"{\"format\":\"azul-storage.keys\",\"version\":1,\"kind\":\"recovery\",\"kdf\":{}}",
        b"{\"links\":[{},{},{}]}",
        b"azlin-pointer\nsize 18446744073709551615\n",
        b"\r\n\r\n\r\n",
    ] {
        every_entry(text);
    }
}

/// Valid samples with a bit flipped at every few bytes: the header, the sealed info, the segments,
/// the trailer of an object; the fields of the key files; the head, a chunk line and the body of an
/// answer. The entry points answer an error or a value, never a panic.
#[test]
fn x_ar5b_5_a_valid_sample_with_a_bit_flipped_is_an_error_or_a_value_and_never_a_panic() {
    let drive_key = DriveKey::from_bytes([7; 32]);
    let id = ObjectId([3; 16]);
    let plain: Vec<u8> = b"Dear diary, the numbers look fine. ".repeat(200);
    let options = WriteOptions {
        segment_size: 1024,
        ..WriteOptions::default()
    };
    let (object, summary) = azl1::encrypt(&plain, id, &drive_key, &options).unwrap();
    let file_key = drive_key.unwrap_file_key(&summary.wrapped_key, &id).unwrap();
    let mut prefix = file_key.as_bytes().to_vec();
    prefix.extend_from_slice(&id.0);
    let with_key = |bytes: &[u8]| -> Vec<u8> {
        let mut input = prefix.clone();
        input.extend_from_slice(bytes);
        input
    };
    azl1_object(&with_key(&object));
    for at in (0..object.len()).step_by(5) {
        for bit in [0x01u8, 0x80] {
            let mut bad = object.clone();
            bad[at] ^= bit;
            azl1_object(&with_key(&bad));
        }
    }
    for cut in (0..object.len()).step_by(37) {
        azl1_object(&with_key(&object[..cut]));
    }

    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    let wrap = RecoveryWrap::seal(&drive_key, "d_1", &RecoveryCode::generate().unwrap(), kdf)
        .unwrap()
        .to_bytes();
    let mut chain = KeyChain::new();
    chain
        .add("d_1", &DriveKey::from_bytes([1; 32]), &drive_key)
        .unwrap();
    let chain = chain.to_bytes();
    let answer = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nx: y\r\n\r\n5\r\nhello\r\n3;e=1\r\nabc\r\n0\r\n\r\n".to_vec();
    for sample in [&wrap, &chain, &answer] {
        key_files(sample);
        http1_answer(sample);
        for at in 0..sample.len() {
            let mut bad = sample.clone();
            bad[at] ^= 0x20;
            key_files(&bad);
            http1_answer(&bad);
        }
        for cut in 0..sample.len() {
            key_files(&sample[..cut]);
            http1_answer(&sample[..cut]);
        }
    }
}
