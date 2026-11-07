//! The test switches of AzMail (RF38; LANE-AZ-3): the password to sign in with and the certificate to
//! trust come from the environment of a headless run in a TEST build only (`--features
//! test-hooks`). A shipped AzMail takes neither - the certificate in particular would make the
//! IMAP connection of a user's mail accept a stranger's server.

use crate::{test_ca, test_ca_in, test_secret, test_secret_in};

/// The environment `pairs` describe.
fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_string())
    }
}

/// A headless run with the password and the certificate of a test stack.
const HEADLESS_STACK: &[(&str, &str)] = &[
    ("AZ_BACKEND", "headless"),
    ("AZMAIL_TEST_PASSWORD", "pw-of-the-test-server"),
    ("AZMAIL_TEST_CA", "/tmp/a-stack/ca.pem"),
];

/// A TEST build takes both from a headless run.
#[test]
fn a_test_build_takes_the_password_and_the_certificate_of_a_headless_run() {
    let var = env(HEADLESS_STACK);
    assert_eq!(
        test_secret_in(true, &var).map(|s| s.expose().to_string()),
        Some(String::from("pw-of-the-test-server"))
    );
    assert_eq!(
        test_ca_in(true, &var),
        Some(std::path::PathBuf::from("/tmp/a-stack/ca.pem"))
    );
}

/// LANE-AZ-3: a SHIPPED build takes neither, whatever its environment holds - it used to take
/// both for `AZ_BACKEND=headless` in every build.
#[test]
fn a_shipped_build_takes_no_password_and_no_certificate_from_its_environment() {
    let var = env(HEADLESS_STACK);
    assert!(test_secret_in(false, &var).is_none());
    assert!(test_ca_in(false, &var).is_none());
}

/// A shipped build does not so much as ask for the password variable.
#[test]
fn a_shipped_build_does_not_read_the_variables() {
    let asked = std::cell::RefCell::new(Vec::new());
    let var = |name: &str| {
        asked.borrow_mut().push(name.to_string());
        Some(String::from("headless"))
    };
    assert!(test_secret_in(false, &var).is_none());
    assert!(test_ca_in(false, &var).is_none());
    assert!(asked.borrow().is_empty(), "read: {:?}", asked.borrow());
}

/// A test build still needs the HEADLESS backend: the password of a windowed run is the user's
/// own (the keyring's), and a blank certificate path is none.
#[test]
fn a_test_build_needs_the_headless_backend_and_a_real_path() {
    const WINDOWED: &[(&str, &str)] = &[
        ("AZ_BACKEND", "metal"),
        ("AZMAIL_TEST_PASSWORD", "pw"),
        ("AZMAIL_TEST_CA", "/tmp/ca.pem"),
    ];
    let windowed = env(WINDOWED);
    assert!(test_secret_in(true, &windowed).is_none());
    assert!(test_ca_in(true, &windowed).is_none());

    const NO_BACKEND: &[(&str, &str)] = &[
        ("AZMAIL_TEST_PASSWORD", "pw"),
        ("AZMAIL_TEST_CA", "/tmp/ca.pem"),
    ];
    let none = env(NO_BACKEND);
    assert!(test_secret_in(true, &none).is_none());
    assert!(test_ca_in(true, &none).is_none());

    const BLANK: &[(&str, &str)] = &[
        ("AZ_BACKEND", "headless"),
        ("AZMAIL_TEST_PASSWORD", ""),
        ("AZMAIL_TEST_CA", "  "),
    ];
    let blank = env(BLANK);
    assert!(
        test_secret_in(true, &blank).is_none(),
        "an empty password is none"
    );
    assert!(test_ca_in(true, &blank).is_none(), "a blank path is none");
}

/// The entry points of a build without the test hooks, with the variables really set in the
/// process: the process's own password and certificate are none.
#[cfg(not(feature = "test-hooks"))]
#[test]
fn this_shipped_process_takes_nothing_from_its_environment() {
    std::env::set_var("AZ_BACKEND", "headless");
    std::env::set_var("AZMAIL_TEST_PASSWORD", "pw-of-the-test-server");
    std::env::set_var("AZMAIL_TEST_CA", "/tmp/a-stack/ca.pem");
    let (secret, ca) = (test_secret(), test_ca());
    std::env::remove_var("AZMAIL_TEST_CA");
    std::env::remove_var("AZMAIL_TEST_PASSWORD");
    std::env::remove_var("AZ_BACKEND");
    assert!(
        secret.is_none(),
        "a shipped AzMail signed in with a variable"
    );
    assert!(
        ca.is_none(),
        "a shipped AzMail trusted a certificate a variable named"
    );
}
