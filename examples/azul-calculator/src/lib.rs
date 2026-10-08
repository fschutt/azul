//! AzCalculator: the calculator of the Azlin apps (the plan:
//! azul-apps/planning/core/calculator.md).
//!
//! The model is plain Rust, tested without a window:
//! - [`num`]: exact decimals (bigdecimal), formatting;
//! - [`expr`]: tokens, the precedence parser, the decimal and integer evaluators;
//! - [`programmer`]: word sizes, bases, shifts, rotates, the bit field;
//! - [`calc`]: the input model (what a key does, what a typed character does);
//! - [`typing`]: the names a user types (`sqrt`, `sin`, `pi`, `x`, `xor`);
//! - [`graph`]: the graph's viewport, ticks, `f64` functions and their curves;
//! - [`units`]: the converter's table;
//! - [`datecalc`]: date differences and date arithmetic;
//! - [`history`]: `calculator/history.jsonl` and the memory.
//!
//! [`ui`] is the window on top of it (azul + azul-appkit).

pub mod calc;
pub mod datecalc;
pub mod expr;
pub mod graph;
pub mod history;
/// The DOM ids and classes (`__azcalc_` prefix), each defined once.
pub mod ids;
pub mod num;
pub mod programmer;
pub mod typing;
pub mod units;

/// The themed styles of every part.
pub mod look;
/// The expression typeset as nested boxes.
pub mod mathview;

/// The window (azul's UtilityShell, the keypads, the panels, the settings page).
pub mod ui;

/// Starts AzCalculator (the switches are read from the command line).
pub fn start() {
    ui::start();
}
