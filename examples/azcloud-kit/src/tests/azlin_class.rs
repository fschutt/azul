//! The token server carries drive tokens, recovery keys and payments: a shipped build reaches it
//! through the Azlin transport (whose pool trusts the pinned authorities) and at an Azlin host
//! only (RF39 2c, ENT27-M16). A test build - this crate's tests are one - may use any transport
//! and a stack on this computer.

use azul_storage::{
    trust::{admits, TrustClass},
    Transport,
};

use super::{Fake, TOKEN};
use crate::{
    token::{check_azlin, TokenError},
    TokenServer,
};

const LIVE: &str = "https://token.azlin.io";
const TRIAL: &str = "https://token-trial.azlin.io";

fn config_text(result: Result<(), TokenError>) -> String {
    match result {
        Err(TokenError::Config(why)) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// Outside a test build only the Azlin class reaches the token server.
#[test]
fn a_shipped_build_reaches_the_token_server_through_the_azlin_transport_only() {
    assert!(check_azlin(LIVE, TrustClass::Azlin, false).is_ok());
    assert!(check_azlin(TRIAL, TrustClass::Azlin, false).is_ok());
    let why = config_text(check_azlin(LIVE, TrustClass::Foreign, false));
    assert!(why.contains("Azlin transport"), "{why}");
}

/// ... and only at an Azlin host: a file, a variable or a stranger's answer names a token server,
/// and the drive token follows it.
#[test]
fn a_shipped_build_reaches_no_token_server_the_service_does_not_run() {
    for url in [
        "https://evil.example",
        "https://token.azlin.io.evil.example",
        "https://evilazlin.io",
        "http://127.0.0.1:8081",
        "http://localhost:8081",
    ] {
        let why = config_text(check_azlin(url, TrustClass::Azlin, false));
        assert!(why.contains("not an Azlin host"), "{url}: {why}");
    }
}

/// An address two parsers read differently (a browser's takes the backslash for a slash and goes
/// to `evil.example`) is refused in every build, not classified by the parser that likes it.
#[test]
fn an_address_that_parsers_read_differently_reaches_no_token_server() {
    for url in [
        "https://evil.example\\@token.azlin.io",
        "https://token.azlin.io\\@evil.example",
    ] {
        for test_build in [false, true] {
            let why = config_text(check_azlin(url, TrustClass::Azlin, test_build));
            assert!(why.contains("names no host"), "{url}: {why}");
        }
    }
}

/// A test build uses any transport and a stack on this computer.
#[test]
fn a_test_build_reaches_a_stack_on_this_computer_through_any_transport() {
    for class in [TrustClass::Azlin, TrustClass::Foreign] {
        assert!(check_azlin("http://127.0.0.1:8081", class, true).is_ok());
        assert!(check_azlin("https://token.example", class, true).is_ok());
    }
}

/// The build under test is a test build (azul-storage's `testing` is a dev-dependency): a fake
/// transport that says nothing - foreign - still makes a server, and the class it reports is
/// the default.
#[test]
fn the_build_under_test_is_a_test_build_and_a_transport_that_says_nothing_is_foreign() {
    assert!(azul_storage::trust::test_build());
    let fake = Fake::new(|_, _| Err(String::from("no network")));
    assert_eq!(Shared(fake.clone()).trust_class(), TrustClass::Foreign);
    assert!(TokenServer::new(TOKEN, &Shared(fake)).is_ok());
    // The host rule alone, as the Azlin class applies it.
    assert!(admits(TrustClass::Azlin, LIVE, false).is_ok());
}

use super::Shared;
