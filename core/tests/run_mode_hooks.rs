//! A build WITH the cargo feature `test-hooks` reads the test switches once, at the first question
//! (ARCH28 RF38): the twin of `run_mode_release.rs`, which proves a build without the feature reads
//! none.
//!
//! The file is a test binary of its own, so the variables are set before anything asks for the
//! hooks. A build without the feature compiles this file empty.

#![cfg(all(feature = "std", feature = "test-hooks"))]

use std::ffi::OsStr;

use azul_core::run_mode::{self, test_hooks};

#[test]
fn a_build_with_the_feature_reads_the_switches_of_its_environment_once() {
    std::env::set_var("AZ_BACKEND", "headless");
    std::env::set_var("AZ_KEYRING", "memory");
    std::env::set_var("AZ_KEYRING_FILE", "/tmp/a-stack/keyring.json");
    std::env::set_var("AZ_AUTH_SESSION_REDIRECT", "cancel");
    std::env::remove_var("AZ_E2E_TEST");
    std::env::remove_var("AZ_E2E");

    assert!(run_mode::AVAILABLE);
    let hooks = test_hooks();
    assert!(hooks.fake_sign_in && hooks.open_nothing && hooks.no_biometric);
    assert!(hooks.no_sensors && hooks.no_speech && hooks.keyring_headless);
    assert!(
        hooks.mocks && !hooks.scripted,
        "a bare headless launch has no script"
    );
    assert_eq!(hooks.keyring_choice.as_deref(), Some(OsStr::new("memory")));
    assert_eq!(
        hooks.keyring_file.as_deref(),
        Some(OsStr::new("/tmp/a-stack/keyring.json"))
    );
    assert_eq!(hooks.fake_redirect.as_deref(), Some("cancel"));

    // Read once: a change of the environment afterwards is not seen.
    std::env::remove_var("AZ_BACKEND");
    assert!(std::ptr::eq(test_hooks(), hooks));
    assert!(test_hooks().fake_sign_in);
}
