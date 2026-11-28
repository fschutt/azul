//! The bytes the `files.*` rows write and the digest the open handler logs are the SAME numbers the emulator proof computes in Python
//! (`scripts/ci/android_files_proof.py`): the constants below were computed there, so a change on either side fails here.

use super::*;

/// Python: `fnv1a64(b"")`, `b"a"`, `b"foobar"` (the reference vectors of the algorithm).
#[test]
fn the_digest_is_fnv_1a_over_64_bits() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn the_files_of_the_rows_are_the_bytes_the_harness_expects() {
    let one = share_one_bytes();
    assert_eq!(one.len(), 1047);
    assert!(one.starts_with(b"AzSelfTest share probe\n"));
    assert_eq!(fnv1a64(&one), 0x9cbe_42b1_df16_258d);
    let (a, b) = (share_many_bytes(false), share_many_bytes(true));
    assert_eq!((a.len(), fnv1a64(&a)), (68, 0x6628_3302_1bbb_1b4d));
    assert_eq!((b.len(), fnv1a64(&b)), (74, 0xd661_26d0_7fc5_db05));
    assert_ne!(a, b, "two files of one name must not be one content: the receiver has to tell them apart");
}

#[test]
fn the_rows_write_under_the_folder_of_the_app() {
    let dir = folder();
    assert!(dir.ends_with(FOLDER));
    let path = write("probe-test", "x.bin", b"xyz").unwrap();
    assert!(path.contains(FOLDER) && path.ends_with("x.bin"));
    assert_eq!(std::fs::read(&path).unwrap(), b"xyz");
    drop(std::fs::remove_dir_all(dir.join("probe-test")));
}

#[test]
fn the_open_handler_appends_one_line_per_request_to_its_log() {
    let log = folder().join(OPEN_LOG);
    drop(std::fs::remove_file(&log));
    append_open_log("{\"action\":\"View\"}");
    append_open_log("{\"action\":\"Send\"}");
    let text = std::fs::read_to_string(&log).unwrap();
    assert_eq!(text, "{\"action\":\"View\"}\n{\"action\":\"Send\"}\n");
    drop(std::fs::remove_file(&log));
}
