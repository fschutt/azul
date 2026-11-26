//! SHA-256 against the official test vectors (FIPS 180-4, the examples of NIST), and the hash of the lines a code
//! reference covers. No window, no azul.

use super::*;

#[test]
fn the_official_vectors_of_sha_256_are_matched() {
    assert_eq!(hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    assert_eq!(hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert_eq!(
        hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        "the 448-bit message of FIPS 180-4"
    );
    assert_eq!(
        hex(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"),
        "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        "the 896-bit message"
    );
    assert_eq!(hex(&vec![b'a'; 1_000_000]), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0", "a million a");
}

#[test]
fn a_hash_is_written_with_its_name_and_is_known_again() {
    let h = tagged(b"abc");
    assert_eq!(h, "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    assert!(is_tagged(&h));
    for bad in ["", "sha256:", "sha256:ba78", "md5:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "sha256:BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD", "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ag"] {
        assert!(!is_tagged(bad), "{bad:?}");
    }
}

#[test]
fn the_hash_of_lines_is_the_hash_of_the_text_with_a_line_feed_after_each() {
    let lines = vec!["fn main() {".to_string(), "    run();".to_string(), "}".to_string()];
    assert_eq!(lines_tagged(&lines), tagged(b"fn main() {\n    run();\n}\n"));
    // A carriage return at the end of a line is the file's line end, not the code's.
    let windows = vec!["fn main() {\r".to_string(), "    run();\r".to_string(), "}\r".to_string()];
    assert_eq!(lines_tagged(&windows), lines_tagged(&lines));
    // Order, content and count each change it.
    let swapped = vec![lines[1].clone(), lines[0].clone(), lines[2].clone()];
    assert_ne!(lines_tagged(&swapped), lines_tagged(&lines));
    assert_ne!(lines_tagged(&lines[..2]), lines_tagged(&lines));
    assert_ne!(lines_tagged(&[String::new()]), lines_tagged(&[]), "an empty line is a line");
    assert_eq!(lines_tagged(&[]), tagged(b""));
}
