//! The entry points the fuzz targets of `examples/azcloud-kit/fuzz` call (ARCH28 RF50 / AR5b-5),
//! run here on inputs that need no fuzzer: empty and short ones, a few hundred of a fixed
//! pseudo-random sequence, text that looks like what the parsers read, and valid samples with a
//! byte changed. What a token server seals into a paid drive's sign-up and what the other devices
//! of a synced folder write into its index are bytes other people wrote, and `panic = "abort"` is
//! the shipped profile: a panic is a failed test here and an abort of the app there. The fuzzer
//! looks further than this does; these keep the entry points panic-free in the meantime and show
//! the fuzz targets what they call.

use super::bundle;
use crate::{
    bundle::DriveBundle,
    claim::ClaimKey,
    sync::remote::{RemoteFile, RemoteIndex},
};

/// A paid drive's sign-up: the text a token server seals to the checkout's claim key (it knows the
/// public half and seals what it likes), read as a drive bundle; and the same bytes as the sealed
/// text itself, opened with a key of its own.
pub(crate) fn sealed_signup(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let _ = DriveBundle::parse(text);
    if let Ok(key) = ClaimKey::generate() {
        let _ = key.open("ck_1", text);
    }
}

/// The index of a synced folder, as the drive holds it: written by any device of the folder.
pub(crate) fn sync_index(data: &[u8]) {
    if let Ok(index) = RemoteIndex::parse(data) {
        for stand_ins in [false, true] {
            let _ = index.unreadable(stand_ins);
        }
        let _ = index.unknown_features();
        let _ = index.to_bytes();
    }
}

fn every_entry(data: &[u8]) {
    sealed_signup(data);
    sync_index(data);
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
fn x_ar5b_5_the_kits_fuzz_entry_points_take_any_bytes_without_a_panic() {
    for len in [
        0usize, 1, 2, 15, 16, 31, 32, 47, 48, 49, 100, 255, 500, 4096,
    ] {
        every_entry(&vec![0u8; len]);
        every_entry(&vec![0xffu8; len]);
        for seed in 1..20u64 {
            every_entry(&noise(seed * 7919 + len as u64, len));
        }
    }
    // Text that looks like what the parsers read.
    for text in [
        &br#"{"drive": null, "credentials": [], "drive_token": 7}"#[..],
        br#"{"drive": {"id": "d_1", "location": {"kind": "s3", "auth": {"type": "azlin"}}}}"#,
        br#"{"drive": {}, "period_tokens": {"checkout_id": 1, "issue_key": [], "months": -1}}"#,
        br#"{"format": "azcloud.sync-index", "version": 1, "files": {"a": {}}, "deleted": 7}"#,
        br#"{"format": "azcloud.sync-index", "version": 99999999999, "features": [1, "x"]}"#,
        br#"{"format": "azcloud.sync-index", "version": 1, "folders": {"../": {}}, "files": []}"#,
        b"[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[",
        b"\xef\xbb\xbf{}",
        b"null",
    ] {
        every_entry(text);
    }
}

/// Valid samples with a byte changed at every few places, and cut short: a drive bundle (a
/// sign-up), an index with a file, a tombstone-free folder and a feature. The entry points answer
/// an error or a value, never a panic.
#[test]
fn x_ar5b_5_a_valid_kit_sample_with_a_byte_changed_is_an_error_or_a_value_and_never_a_panic() {
    let sign_up = bundle("AKID1", "2026-10-08T09:30:00Z", "dt_f.0.aaa").into_bytes();

    let mut index = RemoteIndex::empty();
    index.files.insert(
        String::from("photos/a.jpg"),
        RemoteFile {
            hash: "ab".repeat(32),
            size: 12,
            mtime: 1_700_000_000,
            gen: 1,
            device: String::from("dev-1"),
            ..RemoteFile::default()
        },
    );
    let index = index.to_bytes();

    for (sample, entry) in [
        (&sign_up, sealed_signup as fn(&[u8])),
        (&index, sync_index as fn(&[u8])),
    ] {
        entry(sample);
        for at in (0..sample.len()).step_by(3) {
            for change in [0x01u8, 0x20, 0x80] {
                let mut bad = sample.clone();
                if let Some(byte) = bad.get_mut(at) {
                    *byte ^= change;
                }
                entry(&bad);
            }
        }
        for cut in (0..sample.len()).step_by(7) {
            entry(&sample[..cut]);
        }
    }
}
