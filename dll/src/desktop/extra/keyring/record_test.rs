use super::*;

#[test]
fn a_protected_item_says_its_level_in_its_comment_and_an_unprotected_one_says_nothing() {
    assert_eq!(comment_of(KeyringProtection::DeviceUnlocked), None);
    let user_presence = comment_of(KeyringProtection::UserPresence).expect("a record");
    let biometric = comment_of(KeyringProtection::Biometric).expect("a record");
    assert_ne!(user_presence, biometric);
    assert_eq!(
        protection_of_comment(Some(&user_presence)),
        KeyringProtection::UserPresence
    );
    assert_eq!(
        protection_of_comment(Some(&biometric)),
        KeyringProtection::Biometric
    );
}

#[test]
fn a_credential_with_no_record_of_ours_is_unprotected() {
    assert_eq!(protection_of_comment(None), KeyringProtection::DeviceUnlocked);
    assert_eq!(protection_of_comment(Some("")), KeyringProtection::DeviceUnlocked);
    // Another program's comment, a version of the record we do not know, a level that does not
    // exist: none of them reads as a protected item of ours.
    for other in [
        "my note",
        "azul-keyring:v2;p=1",
        "azul-keyring:v1;p=",
        "azul-keyring:v1;p=9",
        "azul-keyring:v1;p=-1",
        "azul-keyring:v1;p=one",
    ] {
        assert_eq!(
            protection_of_comment(Some(other)),
            KeyringProtection::DeviceUnlocked,
            "{other:?}"
        );
    }
}

#[test]
fn the_record_survives_surrounding_space_in_the_number() {
    assert_eq!(
        protection_of_comment(Some("azul-keyring:v1;p= 2 ")),
        KeyringProtection::Biometric
    );
}

#[test]
fn a_hello_account_is_a_short_stable_name_of_the_service_and_the_key() {
    let a = hello_account("io.azlin.shared", "vault-key");
    assert_eq!(a, hello_account("io.azlin.shared", "vault-key"), "stable");
    assert_ne!(a, hello_account("io.azlin.shared", "other-key"));
    assert_ne!(a, hello_account("org.example", "vault-key"));
    // `service NUL key` is not `service key` run together.
    assert_ne!(hello_account("ab", "c"), hello_account("a", "bc"));
    assert!(a.starts_with("azul-keyring-"));
    assert_eq!(a.len(), "azul-keyring-".len() + 16);
    // A name of any length stays short.
    let long = hello_account(&"s".repeat(10_000), &"k".repeat(10_000));
    assert_eq!(long.len(), a.len());
}

#[test]
fn the_hello_status_numbers_mean_what_windows_documents() {
    assert_eq!(hello_status(0), HelloStatus::Success);
    assert_eq!(hello_status(1), HelloStatus::Other);
    assert_eq!(hello_status(2), HelloStatus::NotFound);
    assert_eq!(hello_status(3), HelloStatus::Cancelled);
    assert_eq!(hello_status(4), HelloStatus::Cancelled, "UserPrefersPassword is a cancel");
    assert_eq!(hello_status(5), HelloStatus::Other);
    assert_eq!(hello_status(6), HelloStatus::LockedOut);
    assert_eq!(hello_status(7), HelloStatus::Other, "a status Windows adds later");
    assert_eq!(hello_status(-1), HelloStatus::Other);
}

#[test]
fn the_hello_challenge_is_fixed_and_not_empty() {
    assert!(!HELLO_CHALLENGE.is_empty());
    assert_eq!(HELLO_CHALLENGE, b"azul-keyring-hello-challenge-v1");
}

// ───────── the library gate ─────────────────────────────────────────────

use std::cell::RefCell;

/// A fake fingerprint reader and authorisation dialog: what they answer, and what was asked.
struct Fake {
    fingers: Fingers,
    verify: BiometricResult,
    authorize: Authorization,
    asked: RefCell<Vec<&'static str>>,
}

impl Fake {
    fn new() -> Self {
        Self {
            fingers: Fingers::Enrolled("a".into()),
            verify: BiometricResult::Authenticated,
            authorize: Authorization::Granted,
            asked: RefCell::new(Vec::new()),
        }
    }
}

impl UserCheck for Fake {
    fn fingers(&self) -> Fingers {
        self.asked.borrow_mut().push("fingers");
        self.fingers.clone()
    }
    fn verify_finger(&self) -> BiometricResult {
        self.asked.borrow_mut().push("verify");
        self.verify
    }
    fn authorize(&self, _reason: &str) -> Authorization {
        self.asked.borrow_mut().push("authorize");
        self.authorize
    }
}

fn record(protection: KeyringProtection, enrolled: Option<&str>) -> GateRecord {
    GateRecord {
        protection,
        enrolled: enrolled.map(str::to_string),
    }
}

#[test]
fn a_record_reads_back_what_it_wrote() {
    for r in [
        record(KeyringProtection::UserPresence, None),
        record(KeyringProtection::Biometric, Some("0123456789abcdef")),
        record(KeyringProtection::Biometric, None),
    ] {
        assert_eq!(GateRecord::parse(&r.text()), Some(r));
    }
}

#[test]
fn a_text_that_is_no_record_of_ours_is_none() {
    for text in ["", "v2;p=1", "v1", "v1;p=", "v1;p=7", "v1;q=1", "p=1", "hunter2"] {
        assert_eq!(GateRecord::parse(text), None, "{text:?}");
    }
}

#[test]
fn the_record_of_an_item_has_a_key_no_app_name_collides_with() {
    assert_eq!(meta_key("vault"), "vault\u{1f}azul-meta");
    assert_ne!(meta_key("a"), meta_key("b"));
    assert!(!"vault".contains(META_SUFFIX));
}

#[test]
fn the_fingers_digest_does_not_depend_on_the_order_and_changes_with_the_set() {
    let two = fingers_digest(&["left-index-finger".into(), "right-thumb".into()]);
    assert_eq!(two, fingers_digest(&["right-thumb".into(), "left-index-finger".into()]));
    assert_ne!(two, fingers_digest(&["left-index-finger".into()]));
    assert_ne!(two, fingers_digest(&[
        "left-index-finger".into(),
        "right-thumb".into(),
        "right-index-finger".into(),
    ]));
    assert_ne!(fingers_digest(&["ab".into(), "c".into()]), fingers_digest(&["a".into(), "bc".into()]));
    assert_eq!(fingers_digest(&[]), fingers_digest(&[]));
}

#[test]
fn an_unprotected_item_asks_nothing() {
    let fake = Fake::new();
    assert_eq!(run_check(&record(KeyringProtection::DeviceUnlocked, None), &fake, "r"), Ok(()));
    assert!(fake.asked.borrow().is_empty());
}

#[test]
fn user_presence_goes_through_the_authorisation_dialog_and_never_the_reader() {
    let mut fake = Fake::new();
    let r = record(KeyringProtection::UserPresence, None);
    assert_eq!(run_check(&r, &fake, "Unlock"), Ok(()));
    assert_eq!(*fake.asked.borrow(), vec!["authorize"]);
    for (answer, want) in [
        (Authorization::Cancelled, KeyringResult::Cancelled),
        (Authorization::Unregistered, KeyringResult::NotEnrolled),
        (Authorization::Unavailable, KeyringResult::Unavailable),
        (Authorization::Error, KeyringResult::Error),
    ] {
        fake.authorize = answer;
        assert_eq!(run_check(&r, &fake, "Unlock"), Err(want), "{answer:?}");
    }
}

#[test]
fn biometric_needs_the_reader_and_a_finger_and_the_same_fingers_as_when_it_was_stored() {
    let mut fake = Fake::new();
    let r = record(KeyringProtection::Biometric, Some("a"));
    assert_eq!(run_check(&r, &fake, "Unlock"), Ok(()));
    assert_eq!(*fake.asked.borrow(), vec!["fingers", "verify"]);

    fake.fingers = Fingers::Enrolled("b".into());
    assert_eq!(
        run_check(&r, &fake, "Unlock"),
        Err(KeyringResult::Invalidated),
        "a finger was added or removed since the item was stored"
    );
    fake.fingers = Fingers::None;
    assert_eq!(run_check(&r, &fake, "Unlock"), Err(KeyringResult::NotEnrolled));
    fake.fingers = Fingers::Absent;
    assert_eq!(run_check(&r, &fake, "Unlock"), Err(KeyringResult::Unavailable));
}

#[test]
fn a_reader_that_answers_is_told_apart_from_the_user_who_did_not_pass() {
    let mut fake = Fake::new();
    let r = record(KeyringProtection::Biometric, None);
    for (answer, want) in [
        (BiometricResult::Authenticated, Ok(())),
        (BiometricResult::FellBackToPasscode, Ok(())),
        (BiometricResult::Failed, Err(KeyringResult::Failed)),
        (BiometricResult::Cancelled, Err(KeyringResult::Cancelled)),
        (BiometricResult::LockedOut, Err(KeyringResult::LockedOut)),
        (BiometricResult::NotEnrolled, Err(KeyringResult::NotEnrolled)),
        (BiometricResult::Unavailable, Err(KeyringResult::Unavailable)),
        (BiometricResult::Error, Err(KeyringResult::Error)),
    ] {
        fake.verify = answer;
        assert_eq!(run_check(&r, &fake, "Unlock"), want, "{answer:?}");
    }
}

#[test]
fn a_record_without_fingers_accepts_the_enrolled_ones() {
    // An item stored before the digest was kept (or while the reader did not list them).
    let fake = Fake::new();
    assert_eq!(
        run_check(&record(KeyringProtection::Biometric, None), &fake, "Unlock"),
        Ok(())
    );
}

#[test]
fn the_need_of_a_record_is_its_level_unless_the_fingers_changed() {
    use azul_core::keyring::KeyringNeed;
    let enrolled = |s: &str| Fingers::Enrolled(s.into());
    assert_eq!(
        need_of_record(&record(KeyringProtection::DeviceUnlocked, None), &Fingers::Absent),
        KeyringNeed::Nothing
    );
    assert_eq!(
        need_of_record(&record(KeyringProtection::UserPresence, None), &Fingers::Absent),
        KeyringNeed::UserPresence
    );
    assert_eq!(
        need_of_record(&record(KeyringProtection::Biometric, Some("a")), &enrolled("a")),
        KeyringNeed::Biometric
    );
    assert_eq!(
        need_of_record(&record(KeyringProtection::Biometric, Some("a")), &enrolled("b")),
        KeyringNeed::Invalidated
    );
    assert_eq!(
        need_of_record(&record(KeyringProtection::Biometric, Some("a")), &Fingers::Absent),
        KeyringNeed::Biometric,
        "no word from the service is no proof of a change"
    );
}
