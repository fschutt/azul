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
    /// A refill failed: what was drawn since is not random ([`generate`] then fails).
    failed: bool,
}

impl OsRandom {
    pub fn new() -> Result<OsRandom, VaultError> {
        let mut rng = OsRandom {
            buf: [0u8; 256],
            pos: 0,
            failed: false,
        };
        random_bytes(&mut rng.buf)?;
        Ok(rng)
    }

    /// Whether a refill failed since this source was made.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.failed
    }
}

impl Drop for OsRandom {
    fn drop(&mut self) {
        self.buf.zeroize();
    }
}

impl Random for OsRandom {
    fn next_u32(&mut self) -> u32 {
        if self.pos + 4 > self.buf.len() {
            if random_bytes(&mut self.buf).is_err() {
                self.failed = true;
            }
            self.pos = 0;
        }
        let at = self.pos;
        self.pos += 4;
        u32::from_le_bytes([
            self.buf[at],
            self.buf[at + 1],
            self.buf[at + 2],
            self.buf[at + 3],
        ])
    }
}

/// A uniform number in `0..n` (rejection sampling: no modulo bias). `n` = 0 gives 0.
pub fn below(rng: &mut impl Random, n: u32) -> u32 {
    if n <= 1 {
        return 0;
    }
    // 2^32 mod n: the numbers under it would make the low residues likelier; they are drawn again.
    let reject_under = n.wrapping_neg() % n;
    loop {
        let r = rng.next_u32();
        if r >= reject_under {
            return r % n;
        }
    }
}

/// Shuffles `items` uniformly (Fisher-Yates).
pub fn shuffle<T>(rng: &mut impl Random, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = below(rng, (i + 1) as u32) as usize;
        items.swap(i, j);
    }
}

/// The character sets a password draws from (each chosen set, without the similar characters
/// when asked); lower case when none is chosen.
#[must_use]
pub fn pools(options: &Options) -> Vec<Vec<char>> {
    const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
    const DIGITS: &str = "0123456789";
    let set = |chars: &str| -> Vec<char> {
        chars
            .chars()
            .filter(|c| !(options.avoid_similar && SIMILAR.contains(*c)))
            .collect()
    };
    let mut out: Vec<Vec<char>> = [
        (options.upper, UPPER),
        (options.lower, LOWER),
        (options.digits, DIGITS),
        (options.symbols, SYMBOLS),
    ]
    .into_iter()
    .filter(|(chosen, _)| *chosen)
    .map(|(_, chars)| set(chars))
    .filter(|s| !s.is_empty())
    .collect();
    if out.is_empty() {
        out.push(set(LOWER));
    }
    out
}

/// One character of `pool`.
fn pick(rng: &mut impl Random, pool: &[char]) -> char {
    pool[below(rng, pool.len() as u32) as usize]
}

/// A random decimal digit.
fn digit(rng: &mut impl Random) -> char {
    char::from(b'0' + below(rng, 10) as u8)
}

/// A secret by `options` from `rng`.
pub fn generate_with(options: &Options, rng: &mut impl Random) -> String {
    match options.mode {
        Mode::Password => {
            let length = options.length.clamp(PASSWORD_LENGTH.0, PASSWORD_LENGTH.1);
            let pools = pools(options);
            let all: Vec<char> = pools.concat();
            let mut chars: Vec<char> = Vec::with_capacity(length);
            // One of each chosen set first (when they fit), the rest from all of them, shuffled.
            if length >= pools.len() {
                for pool in &pools {
                    chars.push(pick(rng, pool));
                }
            }
            while chars.len() < length {
                chars.push(pick(rng, &all));
            }
            shuffle(rng, &mut chars);
            chars.into_iter().collect()
        }
        Mode::Pin => {
            let length = options.length.clamp(PIN_LENGTH.0, PIN_LENGTH.1);
            (0..length).map(|_| digit(rng)).collect()
        }
        Mode::Passphrase => {
            let count = options.words.clamp(PASSPHRASE_WORDS.0, PASSPHRASE_WORDS.1);
            let words: Vec<String> = (0..count)
                .map(|_| {
                    let word = WORDS[below(rng, WORDS.len() as u32) as usize];
                    if options.capitalize {
                        let mut chars = word.chars();
                        chars
                            .next()
                            .map(|c| c.to_ascii_uppercase().to_string() + chars.as_str())
                            .unwrap_or_default()
                    } else {
                        word.to_string()
                    }
                })
                .collect();
            let mut phrase = words.join(&options.separator);
            if options.add_digit {
                phrase.push(digit(rng));
            }
            phrase
        }
    }
}

/// A secret by `options` from the OS random source.
pub fn generate(options: &Options) -> Result<String, VaultError> {
    let mut rng = OsRandom::new()?;
    let secret = generate_with(options, &mut rng);
    if rng.failed() {
        return Err(VaultError::Random);
    }
    Ok(secret)
}

/// The entropy in bits of a secret `options` generates.
#[must_use]
pub fn entropy(options: &Options) -> f64 {
    match options.mode {
        Mode::Password => {
            let length = options.length.clamp(PASSWORD_LENGTH.0, PASSWORD_LENGTH.1);
            let pool: usize = pools(options).iter().map(Vec::len).sum();
            length as f64 * (pool as f64).log2()
        }
        Mode::Pin => options.length.clamp(PIN_LENGTH.0, PIN_LENGTH.1) as f64 * 10f64.log2(),
        Mode::Passphrase => {
            let count = options.words.clamp(PASSPHRASE_WORDS.0, PASSPHRASE_WORDS.1);
            let digit = if options.add_digit { 10f64.log2() } else { 0.0 };
            count as f64 * (WORDS.len() as f64).log2() + digit
        }
    }
}

/// Passwords found first in every guessing list (lower case; a password that is one of them
/// with digits or `!` after it counts as one).
const COMMON: [&str; 44] = [
    "password",
    "passw0rd",
    "p@ssw0rd",
    "123456",
    "1234567",
    "12345678",
    "123456789",
    "1234567890",
    "111111",
    "000000",
    "654321",
    "qwerty",
    "qwertz",
    "azerty",
    "asdfgh",
    "abc123",
    "letmein",
    "welcome",
    "monkey",
    "dragon",
    "football",
    "baseball",
    "iloveyou",
    "admin",
    "login",
    "master",
    "sunshine",
    "princess",
    "shadow",
    "superman",
    "trustno1",
    "hunter",
    "changeme",
    "secret",
    "starwars",
    "whatever",
    "computer",
    "default",
    "guest",
    "root",
    "test",
    "pass",
    "hello",
    "freedom",
];

/// An estimate in bits of a password's strength against guessing: 8 bits for a password of
/// the common list; else the length - repeats and runs (`aaa`, `abc`, `321`) counting little -
/// times the bits of the character classes it uses.
#[must_use]
pub fn estimate(password: &str) -> f64 {
    if password.is_empty() {
        return 0.0;
    }
    let lower = password.to_lowercase();
    let stem = lower.trim_end_matches(|c: char| c.is_ascii_digit() || c == '!');
    if COMMON.contains(&lower.as_str()) || COMMON.contains(&stem) {
        return 8.0;
    }
    let (mut lowers, mut uppers, mut digits, mut symbols, mut others) =
        (false, false, false, false, false);
    let mut effective = 0.0;
    let mut previous: Option<char> = None;
    for c in password.chars() {
        if c.is_ascii_lowercase() {
            lowers = true;
        } else if c.is_ascii_uppercase() {
            uppers = true;
        } else if c.is_ascii_digit() {
            digits = true;
        } else if c.is_ascii() {
            symbols = true;
        } else {
            others = true;
        }
        effective += match previous {
            Some(p) if p == c => 0.2,
            Some(p) if u32::from(p).abs_diff(u32::from(c)) == 1 => 0.3,
            _ => 1.0,
        };
        previous = Some(c);
    }
    let pool = [
        (lowers, 26),
        (uppers, 26),
        (digits, 10),
        (symbols, 33),
        (others, 100),
    ]
    .iter()
    .filter(|(present, _)| *present)
    .map(|(_, n)| *n)
    .sum::<u32>();
    effective * f64::from(pool.max(2)).log2()
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
        if bits < 28.0 {
            Strength::VeryWeak
        } else if bits < 36.0 {
            Strength::Weak
        } else if bits < 60.0 {
            Strength::Medium
        } else if bits < 100.0 {
            Strength::Strong
        } else {
            Strength::VeryStrong
        }
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
