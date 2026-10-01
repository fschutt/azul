//! AzCalculator: the calculator of the Azlin apps (the plan:
//! azul-apps/planning/core/calculator.md).
//!
//! The model is plain Rust, tested without a window:
//! - [`num`]: exact decimals (bigdecimal), formatting;
//! - [`expr`]: tokens, the precedence parser, the decimal and integer evaluators;
//! - [`programmer`]: word sizes, bases, shifts, rotates, the bit field;
//! - [`calc`]: the input model (what a key does), the keyboard map;
//! - [`units`]: the converter's table;
//! - [`datecalc`]: date differences and date arithmetic;
//! - [`history`]: `calculator/history.jsonl` and the memory.
//!
//! [`ui`] is the window on top of it (azul + azul-appkit).

pub mod calc;
pub mod datecalc;
pub mod expr;
pub mod history;
pub mod num;
pub mod programmer;
pub mod units;

/// The window (azul's UtilityShell, the keypads, the panels, the settings page).
pub mod ui;

/// Starts AzCalculator (the switches are read from the command line).
pub fn start() {
    ui::start();
}
