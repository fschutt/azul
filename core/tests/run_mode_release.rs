//! A build WITHOUT the cargo feature `test-hooks` reads no test switch (ARCH28 RF38; the
//! behavioural half of `run_mode_gate`): with every variable of a test run set in the process, the
//! hooks are none, so `Url::open` is not skipped, the sign-in is not answered by a variable and the
//! biometric prompt is not "unavailable".
//!
//! The file is a test binary of its own, so the variables are set before anything asks for the
//! hooks. A build WITH the feature compiles this file empty; `run_mode_hooks.rs` is its twin.

#![cfg(all(feature = "std", not(feature = "test-hooks")))]

use azul_core::run_mode::{self, test_hooks, TestHooks};

/// Every variable a test run is switched on by, with a value that turns its hook on.
const EVERY_SWITCH: [(&str, &str); 6] = [
    ("AZ_BACKEND", "headless"),
    ("AZ_E2E_TEST", "/tmp/scenario.json"),
    ("AZ_E2E", "/tmp/script.json"),
    ("AZ_KEYRING", "memory"),
    ("AZ_KEYRING_FILE", "/tmp/a-stack/keyring.json"),
    (
        "AZ_AUTH_SESSION_REDIRECT",
        "{redirect_uri}?code=x&state={state}",
    ),
];

#[test]
fn a_shipped_build_has_no_hook_whatever_the_environment_holds() {
    for (name, value) in EVERY_SWITCH {
        std::env::set_var(name, value);
    }
    // What the environment would ask for, were it read: every hook.
    let asked = TestHooks::from_env(&|name| std::env::var_os(name));
    assert!(
        asked.fake_sign_in && asked.open_nothing && asked.no_biometric && asked.mocks,
        "the process really holds the switches: {asked:?}"
    );

    // `black_box`: the constant is known to be false here, and a plain `assert!(!false)` is
    // what clippy rejects as an assertion that can never fail.
    assert!(
        !std::hint::black_box(run_mode::AVAILABLE),
        "the feature is off in this binary"
    );
    let hooks = test_hooks();
    assert_eq!(hooks, &TestHooks::NONE);
    assert!(!hooks.open_nothing, "Url::open is skipped by a variable");
    assert!(!hooks.fake_sign_in, "a sign-in is answered by a variable");
    assert!(!hooks.no_biometric, "the biometric prompt is stood in for");
    assert!(!hooks.no_sensors && !hooks.no_speech && !hooks.mocks && !hooks.scripted);
    assert!(!hooks.keyring_headless);
    assert_eq!(hooks.keyring_choice, None);
    assert_eq!(hooks.keyring_file, None);
    assert_eq!(hooks.fake_redirect, None);
}
