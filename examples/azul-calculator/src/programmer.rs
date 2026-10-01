//! Programmer mode's integers: word sizes, bases, two's complement, shifts
//! and rotates, the bit field.
//!
//! A value is an `i128` holding a signed integer of the current word
//! (QWORD 64, DWORD 32, WORD 16, BYTE 8 bits). Every operation wraps to the
//! word as the hardware does; DEC shows the signed value, HEX / OCT / BIN the
//! word's bits (so -1 in a BYTE is `FF`).

use crate::num::CalcError;

/// The word size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WordSize {
    #[default]
    Qword,
    Dword,
    Word,
    Byte,
}

impl WordSize {
    pub const ALL: [WordSize; 4] = [WordSize::Qword, WordSize::Dword, WordSize::Word, WordSize::Byte];

    #[must_use]
    pub fn bits(self) -> u32 {
        match self {
            WordSize::Qword => 64,
            WordSize::Dword => 32,
            WordSize::Word => 16,
            WordSize::Byte => 8,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            WordSize::Qword => "QWORD",
            WordSize::Dword => "DWORD",
            WordSize::Word => "WORD",
            WordSize::Byte => "BYTE",
        }
    }

    /// The word's bits as a mask.
    #[must_use]
    pub fn mask(self) -> u128 {
        (1u128 << self.bits()) - 1
    }

    /// `v` cut to the word and sign-extended: what the register holds.
    #[must_use]
    pub fn wrap(self, v: i128) -> i128 {
        let shift = 128 - self.bits();
        (v << shift) >> shift
    }

    /// The word's bits of `v` as an unsigned number.
    #[must_use]
    pub fn unsigned(self, v: i128) -> u128 {
        (v as u128) & self.mask()
    }

    #[must_use]
    pub fn index(self) -> usize {
        WordSize::ALL.iter().position(|w| *w == self).unwrap_or(0)
    }
}

/// The input and display base.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Base {
    Hex,
    #[default]
    Dec,
    Oct,
    Bin,
}

impl Base {
    pub const ALL: [Base; 4] = [Base::Hex, Base::Dec, Base::Oct, Base::Bin];

    #[must_use]
    pub fn radix(self) -> u32 {
        match self {
            Base::Hex => 16,
            Base::Dec => 10,
            Base::Oct => 8,
            Base::Bin => 2,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Base::Hex => "HEX",
            Base::Dec => "DEC",
            Base::Oct => "OCT",
            Base::Bin => "BIN",
        }
    }

    /// Whether the digit `d` (0..=15) can be typed in this base.
    #[must_use]
    pub fn accepts(self, d: u8) -> bool {
        u32::from(d) < self.radix()
    }

    #[must_use]
    pub fn index(self) -> usize {
        Base::ALL.iter().position(|b| *b == self).unwrap_or(1)
    }
}

/// A literal in `base` (`2A5F`, `-17`, `1010`; `0x` / `0o` / `0b` prefixes
/// matching the base and `_` / blank separators are accepted), wrapped to the word.
pub fn parse_int(text: &str, base: Base, word: WordSize) -> Result<i128, CalcError> {
    let bad = || CalcError::Syntax(format!("{text:?} is not a {} number", base.label()));
    let t: String = text
        .chars()
        .filter(|c| !matches!(c, '_' | ' ' | ','))
        .map(|c| if c == '\u{2212}' { '-' } else { c })
        .collect();
    let (negative, digits) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t.as_str()),
    };
    let lower = digits.to_ascii_lowercase();
    let digits = match (base, lower.get(..2)) {
        (Base::Hex, Some("0x")) | (Base::Oct, Some("0o")) | (Base::Bin, Some("0b")) => &lower[2..],
        _ => lower.as_str(),
    };
    if digits.is_empty() {
        return Err(bad());
    }
    let mut value: u128 = 0;
    for c in digits.chars() {
        let d = c.to_digit(base.radix()).ok_or_else(bad)?;
        value = value
            .checked_mul(u128::from(base.radix()))
            .and_then(|v| v.checked_add(u128::from(d)))
            .ok_or(CalcError::Overflow)?;
        if value > u128::from(u64::MAX) * 2 {
            return Err(CalcError::Overflow);
        }
    }
    let v = value as i128;
    Ok(word.wrap(if negative { -v } else { v }))
}

/// Whether `text` (digits in `base`, maybe a leading `-`) still fits the
/// word's bits: the keypad refuses a digit that would not.
#[must_use]
pub fn fits(text: &str, base: Base, word: WordSize) -> bool {
    let digits = text.trim_start_matches('-');
    let mut value: u128 = 0;
    for c in digits.chars() {
        let Some(d) = c.to_digit(base.radix()) else {
            return false;
        };
        value = value * u128::from(base.radix()) + u128::from(d);
        if value > word.mask() {
            return false;
        }
    }
    true
}

/// The value as shown in `base`: DEC signed (grouped by thousands when
/// `grouping`), HEX / OCT / BIN the word's bits, upper-case, grouped by 4
/// (HEX, BIN) or 3 (OCT) when `grouping`.
#[must_use]
pub fn format_int(v: i128, base: Base, word: WordSize, grouping: bool) -> String {
    let v = word.wrap(v);
    match base {
        Base::Dec => {
            let text = v.unsigned_abs().to_string();
            let body = if grouping {
                crate::num::group_thousands(&text)
            } else {
                text
            };
            if v < 0 {
                format!("-{body}")
            } else {
                body
            }
        }
        Base::Hex | Base::Oct | Base::Bin => {
            let u = word.unsigned(v);
            let text = match base {
                Base::Hex => format!("{u:X}"),
                Base::Oct => format!("{u:o}"),
                _ => format!("{u:b}"),
            };
            if !grouping {
                return text;
            }
            let size = if base == Base::Oct { 3 } else { 4 };
            group_from_right(&text, size, ' ')
        }
    }
}

/// `1234567` by 3 with `,` -> `1,234,567`.
fn group_from_right(text: &str, size: usize, sep: char) -> String {
    let len = text.len();
    let mut out = String::with_capacity(len + len / size);
    for (i, c) in text.chars().enumerate() {
        if i > 0 && (len - i) % size == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out
}

/// Shift left by `n` bits within the word (bits shifted out are lost).
pub fn shift_left(v: i128, n: i128, word: WordSize) -> Result<i128, CalcError> {
    if n < 0 {
        return Err(CalcError::InvalidInput);
    }
    if n >= i128::from(word.bits()) {
        return Ok(0);
    }
    Ok(word.wrap(((word.unsigned(v)) << n) as i128))
}

/// Arithmetic shift right by `n` bits (the sign bit is copied in).
pub fn shift_right(v: i128, n: i128, word: WordSize) -> Result<i128, CalcError> {
    if n < 0 {
        return Err(CalcError::InvalidInput);
    }
    let v = word.wrap(v);
    if n >= i128::from(word.bits()) {
        return Ok(if v < 0 { -1 } else { 0 });
    }
    Ok(v >> n)
}

/// Rotate left by `n` bits within the word.
pub fn rotate_left(v: i128, n: i128, word: WordSize) -> Result<i128, CalcError> {
    if n < 0 {
        return Err(CalcError::InvalidInput);
    }
    let bits = u32::try_from(n % i128::from(word.bits())).unwrap_or(0);
    let u = word.unsigned(v);
    if bits == 0 {
        return Ok(word.wrap(u as i128));
    }
    let rotated = ((u << bits) | (u >> (word.bits() - bits))) & word.mask();
    Ok(word.wrap(rotated as i128))
}

/// Rotate right by `n` bits within the word.
pub fn rotate_right(v: i128, n: i128, word: WordSize) -> Result<i128, CalcError> {
    if n < 0 {
        return Err(CalcError::InvalidInput);
    }
    let bits = n % i128::from(word.bits());
    rotate_left(v, (i128::from(word.bits()) - bits) % i128::from(word.bits()), word)
}

/// Whether bit `i` (0 = least significant) is set.
#[must_use]
pub fn bit(v: i128, i: u32, word: WordSize) -> bool {
    i < word.bits() && (word.unsigned(v) >> i) & 1 == 1
}

/// `v` with bit `i` flipped (a bit outside the word changes nothing).
#[must_use]
pub fn toggle_bit(v: i128, i: u32, word: WordSize) -> i128 {
    if i >= word.bits() {
        return word.wrap(v);
    }
    word.wrap((word.unsigned(v) ^ (1u128 << i)) as i128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_value_shows_in_all_four_bases() {
        let v = parse_int("2A5F", Base::Hex, WordSize::Qword).unwrap();
        assert_eq!(v, 10847);
        assert_eq!(format_int(v, Base::Hex, WordSize::Qword, true), "2A5F");
        assert_eq!(format_int(v, Base::Dec, WordSize::Qword, true), "10,847");
        assert_eq!(format_int(v, Base::Oct, WordSize::Qword, false), "25137");
        assert_eq!(format_int(v, Base::Bin, WordSize::Qword, true), "10 1010 0101 1111");
    }

    #[test]
    fn negative_values_show_their_twos_complement_bits() {
        assert_eq!(format_int(-1, Base::Hex, WordSize::Byte, true), "FF");
        assert_eq!(format_int(-1, Base::Dec, WordSize::Byte, true), "-1");
        assert_eq!(format_int(-1, Base::Hex, WordSize::Qword, false), "FFFFFFFFFFFFFFFF");
        assert_eq!(format_int(-2, Base::Bin, WordSize::Byte, false), "11111110");
    }

    #[test]
    fn the_word_size_wraps_like_the_register() {
        assert_eq!(WordSize::Byte.wrap(255), -1);
        assert_eq!(WordSize::Byte.wrap(256), 0);
        assert_eq!(WordSize::Byte.wrap(128), -128);
        assert_eq!(WordSize::Word.wrap(65535), -1);
        assert_eq!(WordSize::Qword.wrap(i128::from(u64::MAX)), -1);
        assert_eq!(parse_int("FF", Base::Hex, WordSize::Byte).unwrap(), -1);
        assert_eq!(parse_int("1FF", Base::Hex, WordSize::Byte).unwrap(), -1);
    }

    #[test]
    fn literals_accept_their_base_prefix_and_refuse_foreign_digits() {
        assert_eq!(parse_int("0x2a5f", Base::Hex, WordSize::Qword).unwrap(), 10847);
        assert_eq!(parse_int("0b1010", Base::Bin, WordSize::Qword).unwrap(), 10);
        assert_eq!(parse_int("-17", Base::Dec, WordSize::Qword).unwrap(), -17);
        assert!(parse_int("12", Base::Bin, WordSize::Qword).is_err());
        assert!(parse_int("G", Base::Hex, WordSize::Qword).is_err());
        assert!(parse_int("", Base::Dec, WordSize::Qword).is_err());
    }

    #[test]
    fn a_digit_that_would_overflow_the_word_does_not_fit() {
        assert!(fits("FF", Base::Hex, WordSize::Byte));
        assert!(!fits("100", Base::Hex, WordSize::Byte));
        assert!(fits("255", Base::Dec, WordSize::Byte));
        assert!(!fits("256", Base::Dec, WordSize::Byte));
        assert!(fits("FFFFFFFFFFFFFFFF", Base::Hex, WordSize::Qword));
        assert!(!fits("10000000000000000", Base::Hex, WordSize::Qword));
    }

    #[test]
    fn shifts_lose_bits_and_the_right_shift_keeps_the_sign() {
        assert_eq!(shift_left(1, 3, WordSize::Qword).unwrap(), 8);
        assert_eq!(shift_left(0x80, 1, WordSize::Byte).unwrap(), 0);
        assert_eq!(shift_left(0x40, 1, WordSize::Byte).unwrap(), -128);
        assert_eq!(shift_left(1, 64, WordSize::Qword).unwrap(), 0);
        assert_eq!(shift_right(-8, 1, WordSize::Qword).unwrap(), -4);
        assert_eq!(shift_right(-8, 99, WordSize::Qword).unwrap(), -1);
        assert_eq!(shift_right(8, 2, WordSize::Qword).unwrap(), 2);
        assert!(shift_left(1, -1, WordSize::Qword).is_err());
    }

    #[test]
    fn rotates_wrap_the_bits_around_the_word() {
        assert_eq!(format_int(rotate_left(0x81, 1, WordSize::Byte).unwrap(), Base::Hex, WordSize::Byte, false), "3");
        assert_eq!(format_int(rotate_right(0x81, 1, WordSize::Byte).unwrap(), Base::Hex, WordSize::Byte, false), "C0");
        assert_eq!(rotate_left(5, 8, WordSize::Byte).unwrap(), 5);
        assert_eq!(rotate_right(5, 0, WordSize::Byte).unwrap(), 5);
        let v = parse_int("8000000000000001", Base::Hex, WordSize::Qword).unwrap();
        assert_eq!(rotate_left(v, 1, WordSize::Qword).unwrap(), 3);
    }

    #[test]
    fn toggling_a_bit_flips_exactly_that_bit() {
        assert_eq!(toggle_bit(0, 0, WordSize::Qword), 1);
        assert_eq!(toggle_bit(10847, 0, WordSize::Qword), 10846);
        assert_eq!(toggle_bit(0, 7, WordSize::Byte), -128);
        assert_eq!(toggle_bit(5, 8, WordSize::Byte), 5, "outside the word");
        assert!(bit(10847, 0, WordSize::Qword));
        assert!(!bit(10847, 5, WordSize::Qword));
        assert!(bit(-1, 63, WordSize::Qword));
        assert!(!bit(-1, 8, WordSize::Byte));
    }

    #[test]
    fn a_base_accepts_only_its_digits() {
        assert!(Base::Bin.accepts(1) && !Base::Bin.accepts(2));
        assert!(Base::Oct.accepts(7) && !Base::Oct.accepts(8));
        assert!(Base::Dec.accepts(9) && !Base::Dec.accepts(10));
        assert!(Base::Hex.accepts(15));
    }
}
