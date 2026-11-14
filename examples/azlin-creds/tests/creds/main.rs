//! The behaviour checks of azlin-creds, through its public API only (so every name the other lanes code
//! against is proved reachable from outside the crate): S9E-01 to S9E-06, -10 (the credential issuer),
//! -12, -13, -16 to -18, -20, -21, -22 of the brief. Each test is named `s9e_NN_<what_it_asserts>`.
//!
//! The crate's hygiene (S9E-15 and S9E-19) is `tests/hygiene.rs`; the witness types' `compile_fail`
//! doctests (S9E-11) are in the documentation of the types.

mod assertion_tests;
mod chain;
mod costs;
mod domains;
mod entitlement_tests;
mod expiry;
mod fields;
mod hostile;
mod issuer_probe;
mod prop;
mod refresh_tests;
mod round_trip;
mod scope_tests;
mod support;
mod vectors;
mod vectors_signed;
