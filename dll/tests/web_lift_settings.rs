//! The settings of the web-lift transpiler read from a lookup: the cases of the unit-test module
//! `src/web/lift_settings_test.rs`, included here so that they run (`cargo test -p azul-dll
//! --features web --test web_lift_settings`) although the lib's own test build does not compile
//! with the `web` feature at the moment (the tests of `web/loader_js.rs` name pointer-lock items
//! that `web/eventloop.rs` does not have yet).

use azul::web::lift_settings::*;

#[path = "../src/web/lift_settings_test.rs"]
mod cases;
