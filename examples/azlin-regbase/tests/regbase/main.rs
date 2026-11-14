//! The behaviour checks of azlin-regbase, through its public API only (so every name the other
//! lanes code against is proved reachable from outside the crate): S9A-02 (the `DayStart` half),
//! -23, -27, -28, -29 and -30 of the brief, plus the vocabulary checks. Each test is named
//! `s9a_NN_<what_it_asserts>`.
//!
//! The crate's hygiene (S9A-21 and S9A-26, regbase half) is `tests/hygiene.rs`.

mod anchors;
mod bytes;
mod common;
mod enums;
mod ids;
mod proof;
mod time;
