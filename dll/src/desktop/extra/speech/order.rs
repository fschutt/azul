//! Which recognizer is asked first. The chain tries its recognizers in order and falls through on
//! a refusal, so the order decides who answers when both could. The rule, from what was measured
//! on this Mac (24 clips of real read speech; 12 TTS-voice sentences with a vocabulary):
//!
//! | what | system's recognizer | built-in model (Whistle) |
//! |------|---------------------|--------------------------|
//! | English word error rate | 10.0 % | **5.8 %** |
//! | German word error rate | **7.8 %** | 12.6 % |
//! | `CssError` in the vocabulary, written as spelt | 3 of 3 voices | 0 of 3 |
//! | `DataTableCell` in the vocabulary, written as spelt | 2 of 3 voices | 1 of 3 |
//!
//! So [`Preference::Automatic`] asks the built-in model first for English, the system's recognizer
//! first for every other language and for the system's own (an empty language), and the system's
//! recognizer first whenever the vocabulary holds an identifier ([`super::vocabulary`]); the
//! built-in model is never first on its own when it is not set up. An explicit preference
//! is obeyed (a refusal still falls through to the other recognizer). std only.

use super::recognizer::Kind;

/// What an app may ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preference {
    /// By the language and the vocabulary, as measured (see the module's table).
    #[default]
    Automatic,
    /// The system's recognizer first, the built-in model when it refuses.
    NativeFirst,
    /// The built-in model first, the system's recognizer when it refuses.
    BuiltinFirst,
}

/// Who is asked first, and the clause that says why (for a status line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Order {
    pub first: Kind,
    pub why: &'static str,
}

/// What an explicit preference says.
const AS_ASKED: &str = "as the app asked";

/// Who is asked first for a call in `language` (a BCP 47 tag; empty for the system's), when the
/// vocabulary does (`identifiers`) or does not hold an identifier and the built-in model is (or
/// is not) ready.
#[must_use]
pub fn choose(preference: Preference, language: &str, identifiers: bool, builtin_ready: bool) -> Order {
    match preference {
        Preference::NativeFirst => Order {
            first: Kind::Native,
            why: AS_ASKED,
        },
        Preference::BuiltinFirst => Order {
            first: Kind::Builtin,
            why: AS_ASKED,
        },
        Preference::Automatic => {
            if !builtin_ready {
                return Order {
                    first: Kind::Native,
                    why: "the built-in recognizer is not set up",
                };
            }
            if identifiers {
                return Order {
                    first: Kind::Native,
                    why: "an identifier is in the vocabulary: the system's recognizer writes it as spelt",
                };
            }
            let primary = language
                .trim()
                .split(['-', '_'])
                .next()
                .unwrap_or("")
                .to_ascii_lowercase();
            match primary.as_str() {
                "en" => Order {
                    first: Kind::Builtin,
                    why: "English: the built-in model made fewer errors",
                },
                "de" => Order {
                    first: Kind::Native,
                    why: "German: the system's recognizer made fewer errors",
                },
                "" => Order {
                    first: Kind::Native,
                    why: "the system's own language: the system's recognizer first",
                },
                _ => Order {
                    first: Kind::Native,
                    why: "not English or German: the system's recognizer first",
                },
            }
        }
    }
}

#[cfg(test)]
#[path = "order_test.rs"]
mod order_test;
