//! The generator: passwords (length, upper / lower case, digits, symbols, "avoid similar"),
//! passphrases (words of [`crate::words::WORDS`], a separator, optionally capitalised with a
//! digit) and PINs, drawn from the OS random source without modulo bias; and the strength the
//! window shows - the exact entropy of a generated secret, an estimate for a typed one (character
//! classes, repeats and sequences, a list of the most common passwords). zxcvbn is a later step.

use std::fmt;

use zeroize::Zeroize;

use crate::crypto::{random_bytes, VaultError};
use crate::words::WORDS;

/// The symbols a password may have.
pub const SYMBOLS: &str = "!#$%&*+-=?@^_~.,:;()[]{}/";
/// The characters "avoid similar" leaves out.
pub const SIMILAR: &str = "Il1|O0o";
/// The bounds of a password's length, a PIN's and a passphrase's words.
pub const PASSWORD_LENGTH: (usize, usize) = (4, 128);
pub const PIN_LENGTH: (usize, usize) = (4, 16);
pub const PASSPHRASE_WORDS: (usize, usize) = (3, 12);

/// What the generator makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Password,
    Passphrase,
    Pin,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Password, Mode::Passphrase, Mode::Pin];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Mode::Password => "Password",
            Mode::Passphrase => "Passphrase",
            Mode::Pin => "PIN",
        }
    }
}

/// The generator's settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    pub mode: Mode,
    /// Characters of a password or digits of a PIN.
    pub length: usize,
    pub upper: bool,
    pub lower: bool,
    pub digits: bool,
    pub symbols: bool,
    pub avoid_similar: bool,
    /// Words of a passphrase.
    pub words: usize,
    pub separator: String,
    /// A passphrase's words start with a capital letter.
    pub capitalize: bool,
    /// A passphrase ends with a digit.
    pub add_digit: bool,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            mode: Mode::Password,
            length: 20,
            upper: true,
            lower: true,
            digits: true,
            symbols: true,
            avoid_similar: false,
            words: 6,
            separator: "-".to_string(),
            capitalize: false,
            add_digit: false,
        }
    }
}

/// A source of random 32-bit numbers.
pub trait Random {
    fn next_u32(&mut self) -> u32;
}

/// The OS random source, read 256 bytes at a time (the buffer is wiped when dropped).
pub struct OsRandom {
    buf: [u8; 256],
    pos: usize,
}

impl OsRandom {
    pub fn new() -> Result<OsRandom, VaultError> {
        todo!("GREEN")
    }
}

impl Drop for OsRandom {
    fn drop(&mut self) {
        self.buf.zeroize();
    }
}

impl Random for OsRandom {
    fn next_u32(&mut self) -> u32 {
        todo!("GREEN")
    }
}

/// A uniform number in `0..n` (rejection sampling: no modulo bias). `n` = 0 gives 0.
pub fn below(rng: &mut impl Random, n: u32) -> u32 {
    let _ = (rng, n);
    todo!("GREEN")
}

/// Shuffles `items` uniformly (Fisher-Yates).
pub fn shuffle<T>(rng: &mut impl Random, items: &mut [T]) {
    let _ = (rng, items);
    todo!("GREEN")
}

/// The character sets a password draws from (each chosen set, without the similar characters
/// when asked); lower case when none is chosen.
#[must_use]
pub fn pools(options: &Options) -> Vec<Vec<char>> {
    let _ = options;
    todo!("GREEN")
}

/// A secret by `options` from `rng`.
pub fn generate_with(options: &Options, rng: &mut impl Random) -> String {
    let _ = (options, rng);
    todo!("GREEN")
}

/// A secret by `options` from the OS random source.
pub fn generate(options: &Options) -> Result<String, VaultError> {
    let mut rng = OsRandom::new()?;
    Ok(generate_with(options, &mut rng))
}

/// The entropy in bits of a secret `options` generates.
#[must_use]
pub fn entropy(options: &Options) -> f64 {
    let _ = options;
    todo!("GREEN")
}

/// An estimate in bits of a password's strength against guessing.
#[must_use]
pub fn estimate(password: &str) -> f64 {
    let _ = password;
    todo!("GREEN")
}

/// The strength's words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    VeryWeak,
    Weak,
    Medium,
    Strong,
    VeryStrong,
}

impl Strength {
    /// The strength of `bits`: under 28 very weak, under 36 weak, under 60 medium, under 100
    /// strong, else very strong.
    #[must_use]
    pub fn of_bits(bits: f64) -> Strength {
        let _ = bits;
        todo!("GREEN")
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Strength::VeryWeak => "very weak",
            Strength::Weak => "weak",
            Strength::Medium => "medium",
            Strength::Strong => "strong",
            Strength::VeryStrong => "very strong",
        }
    }
}

impl fmt::Display for Strength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// The strength bar's fill: `bits` of 128, 0..=100.
#[must_use]
pub fn percent(bits: f64) -> f32 {
    ((bits / 128.0) * 100.0).clamp(0.0, 100.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Numbers counting up: deterministic, covers every residue.
    struct Counter(u32);

    impl Random for Counter {
        fn next_u32(&mut self) -> u32 {
            self.0 = self.0.wrapping_add(0x9e37_79b9);
            self.0
        }
    }

    fn os() -> OsRandom {
        OsRandom::new().expect("the OS random source")
    }

    #[test]
    fn a_password_has_its_length_and_only_characters_of_the_chosen_sets() {
        let options = Options {
            symbols: false,
            ..Options::default()
        };
        for _ in 0..50 {
            let p = generate_with(&options, &mut os());
            assert_eq!(p.chars().count(), 20);
            assert!(p.chars().all(|c| c.is_ascii_alphanumeric()), "{p}");
        }
    }

    #[test]
    fn every_chosen_set_appears_in_a_password_long_enough_to_hold_them() {
        let options = Options {
            length: 4,
            ..Options::default()
        };
        for _ in 0..200 {
            let p = generate_with(&options, &mut os());
            assert!(p.chars().any(|c| c.is_ascii_uppercase()), "{p}");
            assert!(p.chars().any(|c| c.is_ascii_lowercase()), "{p}");
            assert!(p.chars().any(|c| c.is_ascii_digit()), "{p}");
            assert!(p.chars().any(|c| SYMBOLS.contains(c)), "{p}");
        }
    }

    #[test]
    fn avoid_similar_leaves_out_il1_o0_and_the_bar() {
        let options = Options {
            length: 128,
            avoid_similar: true,
            ..Options::default()
        };
        for _ in 0..20 {
            let p = generate_with(&options, &mut os());
            assert!(!p.chars().any(|c| SIMILAR.contains(c)), "{p}");
        }
    }

    #[test]
    fn a_pin_is_digits_of_its_length() {
        let options = Options {
            mode: Mode::Pin,
            length: 6,
            ..Options::default()
        };
        let pin = generate_with(&options, &mut os());
        assert_eq!(pin.len(), 6);
        assert!(pin.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn a_passphrase_is_words_of_the_list_joined_by_the_separator() {
        let options = Options {
            mode: Mode::Passphrase,
            words: 5,
            separator: ".".into(),
            ..Options::default()
        };
        let phrase = generate_with(&options, &mut os());
        let words: Vec<&str> = phrase.split('.').collect();
        assert_eq!(words.len(), 5);
        assert!(
            words.iter().all(|w| WORDS.binary_search(w).is_ok()),
            "{phrase}"
        );

        let fancy = Options {
            capitalize: true,
            add_digit: true,
            ..options
        };
        let phrase = generate_with(&fancy, &mut os());
        assert!(
            phrase
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase()),
            "{phrase}"
        );
        assert!(
            phrase.chars().last().is_some_and(|c| c.is_ascii_digit()),
            "{phrase}"
        );
    }

    #[test]
    fn no_set_chosen_falls_back_to_lower_case_and_lengths_keep_their_bounds() {
        let none = Options {
            upper: false,
            lower: false,
            digits: false,
            symbols: false,
            ..Options::default()
        };
        let p = generate_with(&none, &mut os());
        assert!(p.chars().all(|c| c.is_ascii_lowercase()), "{p}");
        assert_eq!(
            generate_with(
                &Options {
                    length: 1,
                    ..Options::default()
                },
                &mut os()
            )
            .len(),
            4
        );
        assert_eq!(
            generate_with(
                &Options {
                    length: 500,
                    ..Options::default()
                },
                &mut os()
            )
            .len(),
            128
        );
        let few = Options {
            mode: Mode::Passphrase,
            words: 1,
            ..Options::default()
        };
        assert_eq!(generate_with(&few, &mut os()).split('-').count(), 3);
    }

    #[test]
    fn below_stays_under_n_and_reaches_every_value() {
        let mut rng = Counter(0);
        let mut seen = [false; 7];
        for _ in 0..1000 {
            let v = below(&mut rng, 7);
            assert!(v < 7);
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|s| *s));
        assert_eq!(below(&mut rng, 0), 0);
        assert_eq!(below(&mut rng, 1), 0);
        let mut items: Vec<u32> = (0..10).collect();
        shuffle(&mut rng, &mut items);
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..10).collect::<Vec<u32>>());
    }

    #[test]
    fn the_entropy_of_a_generated_secret_is_exact() {
        let all = Options::default();
        let pool = 26 + 26 + 10 + SYMBOLS.chars().count();
        assert!((entropy(&all) - 20.0 * (pool as f64).log2()).abs() < 1e-9);
        let phrase = Options {
            mode: Mode::Passphrase,
            words: 6,
            ..Options::default()
        };
        assert!(
            (entropy(&phrase) - 60.0).abs() < 1e-9,
            "1024 words: 10 bits each"
        );
        let pin = Options {
            mode: Mode::Pin,
            length: 6,
            ..Options::default()
        };
        assert!((entropy(&pin) - 6.0 * 10f64.log2()).abs() < 1e-9);
        assert_eq!(Strength::of_bits(entropy(&all)), Strength::VeryStrong);
    }

    #[test]
    fn common_repeated_and_short_passwords_estimate_low() {
        for weak in [
            "password",
            "Password1",
            "123456",
            "qwerty123",
            "aaaaaaaaaaaa",
            "abcdefgh",
            "hunter2",
        ] {
            assert!(estimate(weak) < 36.0, "{weak}: {}", estimate(weak));
        }
        assert!(estimate("q7#Rt!vW2mZp8&Lk") >= 100.0);
        assert!(estimate("") == 0.0);
        assert_eq!(Strength::of_bits(10.0), Strength::VeryWeak);
        assert_eq!(Strength::of_bits(30.0), Strength::Weak);
        assert_eq!(Strength::of_bits(50.0), Strength::Medium);
        assert_eq!(Strength::of_bits(80.0), Strength::Strong);
        assert_eq!(percent(256.0), 100.0);
    }

    #[test]
    fn the_word_list_has_1024_distinct_sorted_lower_case_words() {
        assert_eq!(WORDS.len(), 1024);
        assert!(
            WORDS.windows(2).all(|w| w[0] < w[1]),
            "sorted, no two alike"
        );
        assert!(WORDS
            .iter()
            .all(|w| (3..=8).contains(&w.len()) && w.chars().all(|c| c.is_ascii_lowercase())));
    }
}
