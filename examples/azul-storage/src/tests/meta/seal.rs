use crate::meta::{Sealer, TestSealer};

const KEY: [u8; 32] = [7; 32];

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn a_sealed_object_opens_with_its_context() {
    let sealer = TestSealer::new(KEY);
    let sealed = sealer.seal(b".azlin/meta/manifest", b"head 7").unwrap();
    assert_eq!(sealer.open(b".azlin/meta/manifest", &sealed).unwrap(), b"head 7");
    let empty = sealer.seal(b"ctx", b"").unwrap();
    assert_eq!(sealer.open(b"ctx", &empty).unwrap(), b"");
}

#[test]
fn a_sealed_object_does_not_open_under_another_context() {
    let sealer = TestSealer::new(KEY);
    let sealed = sealer.seal(b".azlin/meta/log/1", b"entry").unwrap();
    assert!(sealer.open(b".azlin/meta/log/2", &sealed).is_err());
}

#[test]
fn a_changed_byte_is_refused() {
    let sealer = TestSealer::new(KEY);
    let sealed = sealer.seal(b"ctx", b"the folder tree").unwrap();
    for at in 0..sealed.len() {
        let mut bad = sealed.clone();
        bad[at] ^= 1;
        assert!(sealer.open(b"ctx", &bad).is_err(), "byte {at}");
    }
    assert!(sealer.open(b"ctx", &sealed[..sealed.len() - 1]).is_err());
}

#[test]
fn another_key_does_not_open_it() {
    let sealed = TestSealer::new(KEY).seal(b"ctx", b"secret names").unwrap();
    assert!(TestSealer::new([8; 32]).open(b"ctx", &sealed).is_err());
    // The same key in another sealer (another device) opens it.
    assert_eq!(
        TestSealer::new(KEY).open(b"ctx", &sealed).unwrap(),
        b"secret names"
    );
}

#[test]
fn sealing_the_same_bytes_twice_gives_different_objects() {
    let sealer = TestSealer::new(KEY);
    let a = sealer.seal(b"ctx", b"same").unwrap();
    let b = sealer.seal(b"ctx", b"same").unwrap();
    assert_ne!(a, b);
    let other_device = TestSealer::new(KEY).seal(b"ctx", b"same").unwrap();
    assert_ne!(a, other_device);
}

#[test]
fn the_sealed_bytes_do_not_contain_the_plaintext() {
    let sealer = TestSealer::new(KEY);
    let plaintext = b"Holiday photos/beach.jpg Holiday photos/beach.jpg".repeat(20);
    let sealed = sealer.seal(b"ctx", &plaintext).unwrap();
    assert!(!contains(&sealed, b"Holiday"));
    assert!(!contains(&sealed, b"beach"));
}

#[test]
fn a_name_hash_is_the_same_on_every_device_and_differs_between_keys() {
    let a = TestSealer::new(KEY).name_hash(b"pack bytes");
    let b = TestSealer::new(KEY).name_hash(b"pack bytes");
    let c = TestSealer::new([8; 32]).name_hash(b"pack bytes");
    let d = TestSealer::new(KEY).name_hash(b"other pack");
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_ne!(a, d);
}
