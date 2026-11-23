//! A build WITH the cargo feature `test-hooks` serves the test switches the dll installed once, at
//! the start (ARCH28 RF38; env pass 2026-11-23): the twin of `run_mode_release.rs`, which proves a
//! build without the feature keeps none.
//!
//! This crate reads no environment: the hooks are built from an `EnvSource` (here a map; the dll
//! hands the process's) and installed. The file is a test binary of its own, so nothing asked
//! for the hooks before the install. A build without the feature skips the target
//! (`required-features` in core/Cargo.toml).

#![cfg(all(feature = "std", feature = "test-hooks"))]

use std::ffi::OsStr;

use azul_core::run_mode::{self, test_hooks, TestHooks};
use azul_css::env::MapEnv;

#[test]
fn a_build_with_the_feature_serves_the_hooks_installed_once() {
    // Nothing is installed yet: no hook, and nothing was read from anywhere.
    assert_eq!(test_hooks(), &TestHooks::NONE);

    let env = MapEnv::new()
        .with("AZ_BACKEND", "headless")
        .with("AZ_KEYRING", "memory")
        .with("AZ_KEYRING_FILE", "/tmp/a-stack/keyring.json")
        .with("AZ_AUTH_SESSION_REDIRECT", "cancel");
    run_mode::install(TestHooks::from_lookup(&env));

    // `black_box`: the constant is known to be true here, and a plain `assert!(true)` is what
    // clippy rejects as an assertion that can never fail.
    assert!(std::hint::black_box(run_mode::AVAILABLE));
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

    // Installed once: a later install (another environment) is not seen.
    run_mode::install(TestHooks::from_lookup(&MapEnv::new()));
    assert!(std::ptr::eq(test_hooks(), hooks));
    assert!(test_hooks().fake_sign_in);
}
