//! The names a user TYPES: `sin`, `sqrt`, `pi`, `x` in the decimal modes,
//! `and`, `xor`, `shl` in Programmer mode.
//!
//! The keyboard gives the calculator one character at a time, so a name
//! arrives letter by letter. The calculator keeps the letters typed so far
//! (its `word`, shown where a number being typed is shown) and asks this
//! table what they are:
//!
//! - still the start of a name (`s`, `si`, `sq`) - wait for the next letter;
//! - a whole name that no longer name extends (`sqrt`, `pi`, `ln`, `x`) - it
//!   goes in at once;
//! - a whole name that a longer one extends (`sin` / `sinh`, `log` / `log2`,
//!   `e` / `exp`) - it waits, and the next character decides;
//! - the start of nothing (`sinx`, `ex+`) - the longest name it begins with
//!   goes in and the rest is read again (`sin` + `x`, `e` + `x`).
//!
//! Pure data and string tests; [`crate::calc::Calculator::type_char`] does
//! the typing.

use crate::expr::{BinOp, Const, Func};

/// What a typed name means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Word {
    /// A function: opens `name(` (`sin(`, `√(`).
    Call(Func),
    Const(Const),
    /// The graph's variable `x`.
    Var,
    /// A binary operator written as a word (`mod`, `and`, `shl`).
    Op(BinOp),
    /// Bitwise NOT.
    Not,
    /// `y`, which only means something before `=`: `y = sin(x)` defines a
    /// function for the graph.
    Define,
}

/// The decimal modes' names (Standard, Scientific / graphing).
pub const DECIMAL_NAMES: &[(&str, Word)] = &[
    ("sin", Word::Call(Func::Sin)),
    ("cos", Word::Call(Func::Cos)),
    ("tan", Word::Call(Func::Tan)),
    ("asin", Word::Call(Func::Asin)),
    ("acos", Word::Call(Func::Acos)),
    ("atan", Word::Call(Func::Atan)),
    ("arcsin", Word::Call(Func::Asin)),
    ("arccos", Word::Call(Func::Acos)),
    ("arctan", Word::Call(Func::Atan)),
    ("sinh", Word::Call(Func::Sinh)),
    ("cosh", Word::Call(Func::Cosh)),
    ("tanh", Word::Call(Func::Tanh)),
    ("asinh", Word::Call(Func::Asinh)),
    ("acosh", Word::Call(Func::Acosh)),
    ("atanh", Word::Call(Func::Atanh)),
    ("ln", Word::Call(Func::Ln)),
    ("log", Word::Call(Func::Log)),
    ("lg", Word::Call(Func::Log)),
    ("log10", Word::Call(Func::Log)),
    ("log2", Word::Call(Func::Log2)),
    ("lb", Word::Call(Func::Log2)),
    ("ld", Word::Call(Func::Log2)),
    ("sqrt", Word::Call(Func::Sqrt)),
    ("root", Word::Call(Func::Sqrt)),
    ("cbrt", Word::Call(Func::Cbrt)),
    ("abs", Word::Call(Func::Abs)),
    ("exp", Word::Call(Func::Exp)),
    ("pow10", Word::Call(Func::Pow10)),
    ("recip", Word::Call(Func::Recip)),
    ("pi", Word::Const(Const::Pi)),
    ("e", Word::Const(Const::E)),
    ("x", Word::Var),
    ("mod", Word::Op(BinOp::Mod)),
    ("y", Word::Define),
];

/// Programmer mode's names (a-f are digits there when the base takes them).
pub const PROGRAMMER_NAMES: &[(&str, Word)] = &[
    ("and", Word::Op(BinOp::And)),
    ("or", Word::Op(BinOp::Or)),
    ("xor", Word::Op(BinOp::Xor)),
    ("nand", Word::Op(BinOp::Nand)),
    ("nor", Word::Op(BinOp::Nor)),
    ("not", Word::Not),
    ("mod", Word::Op(BinOp::Mod)),
    ("shl", Word::Op(BinOp::Shl)),
    ("shr", Word::Op(BinOp::Shr)),
    ("lsh", Word::Op(BinOp::Shl)),
    ("rsh", Word::Op(BinOp::Shr)),
    ("rol", Word::Op(BinOp::Rol)),
    ("ror", Word::Op(BinOp::Ror)),
    // Windows' Programmer calculator: x is the times sign.
    ("x", Word::Op(BinOp::Mul)),
];

/// The name `word` is, exactly.
#[must_use]
pub fn exact(names: &[(&str, Word)], word: &str) -> Option<Word> {
    names.iter().find(|(n, _)| *n == word).map(|(_, w)| *w)
}

/// Whether some name starts with `word` (or is it).
#[must_use]
pub fn is_prefix(names: &[(&str, Word)], word: &str) -> bool {
    !word.is_empty() && names.iter().any(|(n, _)| n.starts_with(word))
}

/// Whether a LONGER name starts with `word`: the word has to wait for the
/// next character before it means anything.
#[must_use]
pub fn extends(names: &[(&str, Word)], word: &str) -> bool {
    names.iter().any(|(n, _)| n.len() > word.len() && n.starts_with(word))
}

/// The longest name `word` begins with, and its length.
#[must_use]
pub fn longest_prefix(names: &[(&str, Word)], word: &str) -> Option<(usize, Word)> {
    names
        .iter()
        .filter(|(n, _)| word.starts_with(n))
        .max_by_key(|(n, _)| n.len())
        .map(|(n, w)| (n.len(), *w))
}

/// The names a user can type in a mode, for the shortcut list and hints.
#[must_use]
pub fn listed(names: &'static [(&'static str, Word)]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for (n, w) in names {
        if *w == Word::Define || out.contains(n) {
            continue;
        }
        out.push(*n);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_waits_while_a_longer_name_extends_it() {
        assert!(is_prefix(DECIMAL_NAMES, "si"));
        assert_eq!(exact(DECIMAL_NAMES, "sin"), Some(Word::Call(Func::Sin)));
        assert!(extends(DECIMAL_NAMES, "sin"), "sinh");
        assert!(extends(DECIMAL_NAMES, "log"), "log2, log10");
        assert!(extends(DECIMAL_NAMES, "e"), "exp");
        assert!(!extends(DECIMAL_NAMES, "sqrt"));
        assert!(!extends(DECIMAL_NAMES, "pi"));
        assert!(!extends(DECIMAL_NAMES, "x"), "x goes in at once");
        assert!(extends(PROGRAMMER_NAMES, "x"), "xor waits in Programmer mode");
    }

    #[test]
    fn a_word_no_name_starts_with_splits_at_its_longest_name() {
        assert!(!is_prefix(DECIMAL_NAMES, "sinx"));
        assert_eq!(longest_prefix(DECIMAL_NAMES, "sinx"), Some((3, Word::Call(Func::Sin))));
        assert_eq!(longest_prefix(DECIMAL_NAMES, "ex"), Some((1, Word::Const(Const::E))));
        assert_eq!(longest_prefix(DECIMAL_NAMES, "log2x"), Some((4, Word::Call(Func::Log2))));
        assert_eq!(longest_prefix(DECIMAL_NAMES, "q"), None);
        assert!(!is_prefix(DECIMAL_NAMES, ""));
    }

    #[test]
    fn programmer_words_are_operators() {
        assert_eq!(exact(PROGRAMMER_NAMES, "xor"), Some(Word::Op(BinOp::Xor)));
        assert_eq!(exact(PROGRAMMER_NAMES, "not"), Some(Word::Not));
        assert_eq!(exact(PROGRAMMER_NAMES, "sin"), None);
        assert!(listed(DECIMAL_NAMES).contains(&"sqrt"));
        assert!(!listed(DECIMAL_NAMES).contains(&"y"));
    }
}
