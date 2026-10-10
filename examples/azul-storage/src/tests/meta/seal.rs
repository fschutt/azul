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

/// The drive key as the repository's sealer (feature `encryption`).
#[cfg(feature = "encryption")]
mod drive_key {
    use super::contains;
    use crate::{
        crypto::DriveKey,
        meta::{keys, KeyWindow, MemoryBucket, MetaError, MetaStore, Publish, RefUpdate, Sealer},
    };

    const MAIN: &str = "refs/heads/main";

    #[test]
    fn a_rotation_window_seals_with_the_new_key_and_opens_the_old_keys_objects_while_open() {
        let old = DriveKey::generate().unwrap();
        let new = DriveKey::generate().unwrap();
        let before = old.seal(b"ctx", b"from before the rotation").unwrap();
        let window = KeyWindow::new(new.clone(), Some(old.clone()));
        assert_eq!(window.open(b"ctx", &before).unwrap(), b"from before the rotation");
        assert!(window.open(b"other", &before).is_err(), "still bound to its place");

        let after = window.seal(b"ctx", b"after").unwrap();
        assert_eq!(&after[4..20], &new.id().0);
        assert_eq!(new.open(b"ctx", &after).unwrap(), b"after");
        assert!(old.open(b"ctx", &after).is_err());
        assert_eq!(window.name_hash(b"pack"), new.name_hash(b"pack"));

        // A third key's object stays shut, and the old key's once the window is closed.
        let stranger = DriveKey::generate().unwrap().seal(b"ctx", b"x").unwrap();
        assert!(window.open(b"ctx", &stranger).is_err());
        let closed = KeyWindow::new(new, None);
        assert!(closed.open(b"ctx", &before).is_err());
    }

    #[test]
    fn the_drive_key_seals_an_object_that_opens_only_in_its_place() {
        let key = DriveKey::generate().unwrap();
        let sealed = key.seal(b".azlin/meta/manifest", b"Holiday photos").unwrap();
        assert_eq!(&sealed[..4], b"AZM1");
        assert_eq!(&sealed[4..20], &key.id().0);
        assert!(!contains(&sealed, b"Holiday"));
        assert_eq!(key.open(b".azlin/meta/manifest", &sealed).unwrap(), b"Holiday photos");
        assert!(key.open(b".azlin/meta/log/1", &sealed).is_err());
        for at in 0..sealed.len() {
            let mut bad = sealed.clone();
            bad[at] ^= 1;
            assert!(key.open(b".azlin/meta/manifest", &bad).is_err(), "byte {at}");
        }
        assert_ne!(sealed, key.seal(b".azlin/meta/manifest", b"Holiday photos").unwrap());
    }

    #[test]
    fn another_drive_key_does_not_open_it_and_names_differ() {
        let key = DriveKey::generate().unwrap();
        let other = DriveKey::generate().unwrap();
        let sealed = key.seal(b"ctx", b"x").unwrap();
        assert!(other.open(b"ctx", &sealed).is_err());
        let same = DriveKey::from_bytes(*key.as_bytes());
        assert_eq!(same.open(b"ctx", &sealed).unwrap(), b"x");
        assert_eq!(key.name_hash(b"pack"), same.name_hash(b"pack"));
        assert_ne!(key.name_hash(b"pack"), other.name_hash(b"pack"));
    }

    #[test]
    fn a_repository_sealed_with_the_drive_key_opens_on_another_device_holding_it() {
        let key = DriveKey::generate().unwrap();
        let bucket = MemoryBucket::new();
        let mut laptop = MetaStore::create(bucket.clone(), key.clone(), "laptop").unwrap();
        laptop
            .publish(|state| {
                Ok(Some(Publish {
                    pack: None,
                    updates: vec![RefUpdate {
                        name: MAIN.to_string(),
                        old: state.refs.get(MAIN).cloned(),
                        new: Some("c1".to_string()),
                    }],
                    message: "Holiday photos".to_string(),
                }))
            })
            .unwrap();
        let phone = MetaStore::open(bucket.clone(), key, "phone").unwrap();
        assert_eq!(phone.state(), laptop.state());
        assert!(matches!(
            MetaStore::open(bucket.clone(), DriveKey::generate().unwrap(), "stranger"),
            Err(MetaError::Sealed { .. })
        ));
        for (name, bytes) in bucket.objects() {
            assert!(name.starts_with(keys::ROOT));
            assert!(!contains(&bytes, b"Holiday"));
            assert!(!contains(&bytes, MAIN.as_bytes()));
        }
    }
}
