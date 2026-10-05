//! Money input widget - an amount of money in a currency, typed the way the
//! user's locale writes numbers, held as an INTEGER count of the currency's
//! minor units (cents, pence, yen): never a float, so 0.10 + 0.20 is 0.30.
//!
//! ```text
//!   +-----------------------+-----+
//!   |            1.234,56   | EUR |      de: '.' groups, ',' is the decimal point
//!   +-----------------------+-----+
//! ```
//!
//! THE FIELD IS A [`TextInput`] (as [`super::number_input::NumberInput`]'s is):
//! the caret, the selection, the clipboard and the IME are the text field's.
//! The money input decides only which edits it accepts and what they mean:
//!
//! - a text that is an amount, or can still become one (`-`, `12,`), is
//!   accepted - the amount is `None` while it is incomplete;
//! - a text that can never become an amount (a letter, a third decimal for a
//!   two-decimal currency, a `-` where negatives are off, a number past the
//!   64-bit range) is refused, the keystroke does not happen;
//! - `min` / `max` never refuse a keystroke (typing `1` on the way to `150`
//!   with a minimum of `50` must work): the state says `BelowMin` /
//!   `AboveMax` and the app decides.
//!
//! LOCALE: [`MoneyLocale`] is the decimal separator, the grouping separator
//! and where the currency goes. [`MoneyLocale::from_tag`] knows the common
//! locales; [`MoneyLocale::from_sample`] reads the separators off a number
//! the app's localizer formatted (ICU `format_decimal(123456789, 2)` ->
//! `"1.234.567,89"`), so the widget follows whatever the platform says.
//! Parsing is lenient where it cannot be wrong: grouping is optional, a lone
//! `.` or `,` that cannot be a group separator (it is not followed by three
//! digits) is the decimal point - the numeric keypad's key in every locale.
//!
//! CURRENCY: [`MoneyCurrency`] is the ISO 4217 code, the symbol and the
//! number of minor digits (`EUR` 2, `JPY` 0, `KWD` 3);
//! [`MoneyCurrency::from_code`] knows the common codes. The code shows in an
//! addon box beside the field, on the side the locale puts the currency.
//!
//! When the field loses focus, the text is rewritten in its canonical form
//! (`1234,5` -> `1.234,50`) and `on_commit` reports the amount; every
//! accepted edit reports through `on_change`.
//!
//! Key types: [`MoneyInput`], [`MoneyInputState`], [`MoneyCurrency`],
//! [`MoneyLocale`], [`MoneyInputError`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::Update,
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec},
    json::OptionI64,
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    AzString, OptionString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        text_input::{
            OnTextInputReturn, TextInput, TextInputOnFocusLostCallbackType,
            TextInputOnTextInputCallbackType, TextInputState, TextInputValid,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

// ---- classes ----

/// The widget's root: the field and the currency addon in a row.
pub const MONEY_INPUT_CLASS: &str = "__azul-native-money-input";
/// The currency addon (the code beside the field).
pub const MONEY_INPUT_ADDON_CLASS: &str = "__azul-native-money-input-addon";

/// U+00A0 NO-BREAK SPACE: the space between an amount and its currency, and
/// the grouping separator of several locales.
const NBSP: char = '\u{a0}';
/// U+202F NARROW NO-BREAK SPACE: French grouping.
const NNBSP: char = '\u{202f}';
/// U+2019 RIGHT SINGLE QUOTATION MARK: Swiss grouping (`1'234.50`).
const SWISS_GROUP: char = '\u{2019}';
/// U+2212 MINUS SIGN: typographic minus, read as `-`.
const MINUS_SIGN: char = '\u{2212}';

// ---- the types the app sees ----

/// Where a locale writes the currency: `$1.00` or `1,00 EUR`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum MoneySymbolPosition {
    /// Before the amount (`$1.00`, `CHF 1.00`).
    #[default]
    Before,
    /// After the amount (`1,00 EUR`).
    After,
}

/// How a locale writes an amount: its decimal point, its grouping separator
/// and where the currency goes. Separators are Unicode code points (`u32`,
/// as `TextInputState::text` holds them); a grouping separator of `0` means
/// the locale does not group.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MoneyLocale {
    /// The decimal point: `.` (en) or `,` (de, fr).
    pub decimal_separator: u32,
    /// The thousands separator: `,` (en), `.` (de), U+202F (fr), U+2019
    /// (de-CH); `0` = no grouping.
    pub group_separator: u32,
    /// Where the currency goes.
    pub symbol_position: MoneySymbolPosition,
    /// A space between the amount and the currency (`1,00 EUR`, not
    /// `1,00EUR`).
    pub symbol_spaced: bool,
}

impl Default for MoneyLocale {
    fn default() -> Self {
        Self::en_us()
    }
}

/// A currency: its ISO 4217 code, its symbol and how many minor units a
/// major one has (as a power of ten: `EUR` 2 - 100 cents, `JPY` 0, `KWD` 3).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MoneyCurrency {
    /// The ISO 4217 code (`EUR`), what the addon shows.
    pub code: AzString,
    /// The symbol (the euro sign, `$`), for [`MoneyInput::format_amount`].
    pub symbol: AzString,
    /// Digits after the decimal point (0 to 4).
    pub minor_digits: u8,
}

impl Default for MoneyCurrency {
    fn default() -> Self {
        Self::from_code(AzString::from_const_str("USD"))
    }
}

/// What is wrong with the field's text, if anything.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum MoneyInputError {
    /// Nothing: the text is an amount (or empty).
    #[default]
    None,
    /// Not an amount yet, but it can become one (`-`, `1,` in en).
    Incomplete,
    /// Not an amount and it cannot become one.
    Invalid,
    /// More decimals than the currency has.
    TooManyDecimals,
    /// Negative, and negatives are not allowed.
    Negative,
    /// Below the minimum.
    BelowMin,
    /// Above the maximum.
    AboveMax,
    /// Outside the 64-bit range of minor units.
    TooLarge,
}

/// An amount read from a text: the minor units, or `None` with the reason.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoneyParseResult {
    /// The amount in minor units; `None` for an empty text or an error.
    pub amount: OptionI64,
    /// Why there is no amount (`None` for an amount or an empty text).
    pub error: MoneyInputError,
}

/// The state of a [`MoneyInput`]: the amount, the bounds, what is wrong.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoneyInputState {
    /// The amount in minor units; `None` while the field is empty or holds no
    /// amount (yet).
    pub amount: OptionI64,
    /// The smallest amount the app accepts (minor units), `None` = no bound.
    pub min: OptionI64,
    /// The largest amount the app accepts (minor units), `None` = no bound.
    pub max: OptionI64,
    /// What is wrong with the field's text (`None`: nothing).
    pub error: MoneyInputError,
    /// Whether a negative amount may be typed (default: yes).
    pub allow_negative: bool,
}

impl Default for MoneyInputState {
    fn default() -> Self {
        Self {
            amount: OptionI64::None,
            min: OptionI64::None,
            max: OptionI64::None,
            error: MoneyInputError::None,
            allow_negative: true,
        }
    }
}

// ---- the locale table ----

impl MoneyLocale {
    /// A locale writing `decimal_separator` as its decimal point and
    /// `group_separator` between thousands (`0`: none), the currency at
    /// `symbol_position`, spaced from the amount.
    #[must_use]
    pub const fn create(
        decimal_separator: u32,
        group_separator: u32,
        symbol_position: MoneySymbolPosition,
    ) -> Self {
        Self {
            decimal_separator,
            group_separator,
            symbol_position,
            symbol_spaced: true,
        }
    }

    /// `1,234.56`, `$1,234.56` (English, the default).
    #[must_use]
    pub const fn en_us() -> Self {
        Self {
            decimal_separator: '.' as u32,
            group_separator: ',' as u32,
            symbol_position: MoneySymbolPosition::Before,
            symbol_spaced: false,
        }
    }

    /// `1.234,56`, `1.234,56 EUR` (German and most of continental Europe).
    #[must_use]
    pub const fn de_de() -> Self {
        Self::create(',' as u32, '.' as u32, MoneySymbolPosition::After)
    }

    /// `1 234,56 EUR` with a narrow no-break space (French).
    #[must_use]
    pub const fn fr_fr() -> Self {
        Self::create(',' as u32, NNBSP as u32, MoneySymbolPosition::After)
    }

    /// `CHF 1'234.56` with a typographic apostrophe (Swiss German).
    #[must_use]
    pub const fn de_ch() -> Self {
        Self::create('.' as u32, SWISS_GROUP as u32, MoneySymbolPosition::Before)
    }

    /// The same locale with other separators.
    #[must_use]
    pub const fn with_separators(mut self, decimal_separator: u32, group_separator: u32) -> Self {
        self.decimal_separator = decimal_separator;
        self.group_separator = group_separator;
        self
    }

    /// The locale of a BCP 47 tag (`de-DE`, `fr`, `pt_BR`, `en-US`): the
    /// common locales' separators and currency position; English for a tag
    /// it does not know. For the platform's exact answer use
    /// [`Self::from_sample`] with a number the app's localizer formatted.
    #[must_use]
    pub fn from_tag(tag: AzString) -> Self {
        let tag = tag.as_str().trim().to_ascii_lowercase().replace('_', "-");
        let mut parts = tag.split('-');
        let lang = parts.next().unwrap_or("");
        // The region is the first 2-letter (or 3-digit) subtag after the
        // language - a script subtag (`Latn`) sits in between sometimes.
        let region = parts
            .find(|p| p.len() == 2 || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit())))
            .unwrap_or("");
        let comma_dot_after = Self::de_de();
        let comma_space_after = Self::create(',' as u32, NBSP as u32, MoneySymbolPosition::After);
        let comma_dot_before = Self::create(',' as u32, '.' as u32, MoneySymbolPosition::Before);
        if matches!(region, "ch" | "li") {
            return Self::de_ch();
        }
        match lang {
            "de" | "it" | "da" | "el" | "id" | "tr" | "ro" | "hr" | "sl" | "sr" | "ca" | "gl"
            | "eu" | "vi" | "is" => comma_dot_after,
            "es" if matches!(
                region,
                "mx" | "us" | "pr" | "gt" | "hn" | "ni" | "sv" | "pa"
            ) =>
            {
                Self::en_us()
            }
            "es" => comma_dot_after,
            "nl" => comma_dot_before,
            "pt" if region == "br" => comma_dot_before,
            "pt" => comma_space_after,
            "fr" => Self::fr_fr(),
            "nb" | "no" | "nn" | "sv" | "fi" | "cs" | "sk" | "pl" | "ru" | "uk" | "hu" | "bg"
            | "lt" | "lv" | "et" | "be" | "kk" => comma_space_after,
            _ => Self::en_us(),
        }
    }

    /// The separators read off `sample`, the number 1234567.89 as a
    /// localizer writes it (`"1,234,567.89"`, `"1.234.567,89"`,
    /// `"1 234 567,89"`, `"1'234'567.89"`): the decimal point is the
    /// separator before the last two digits, the grouping separator the
    /// first one before it. The currency goes after the amount in a locale
    /// with a decimal comma, before it otherwise. English when the sample
    /// has no decimal point.
    #[must_use]
    pub fn from_sample(sample: AzString) -> Self {
        // The separators: every run of non-digits BETWEEN two digits (a sign
        // or a currency around the number is not one).
        let chars: Vec<char> = sample.as_str().trim().chars().collect();
        let (Some(first), Some(last)) = (
            chars.iter().position(char::is_ascii_digit),
            chars.iter().rposition(char::is_ascii_digit),
        ) else {
            return Self::en_us();
        };
        let mut seps: Vec<(usize, char)> = Vec::new();
        for (i, c) in chars.iter().enumerate().take(last).skip(first) {
            if !c.is_ascii_digit() {
                seps.push((i, *c));
            }
        }
        let Some(&(at, decimal)) = seps.last() else {
            return Self::en_us();
        };
        // A separator before three digits, like every other one, is grouping
        // of a number without a decimal point: nothing to read.
        let digits_after = last - at;
        if digits_after == 3 && seps.iter().all(|(_, c)| *c == decimal) {
            return Self::en_us();
        }
        let group = match seps.first() {
            Some(&(_, g)) if seps.len() > 1 && g != decimal => g as u32,
            Some(_) if seps.len() > 1 => return Self::en_us(),
            _ => 0,
        };
        let english = decimal == '.' && group == ',' as u32;
        Self {
            decimal_separator: decimal as u32,
            group_separator: group,
            symbol_position: if decimal == ',' {
                MoneySymbolPosition::After
            } else {
                MoneySymbolPosition::Before
            },
            symbol_spaced: !english,
        }
    }

    /// The decimal point as a `char` (`.` for a code point that is none).
    #[must_use]
    pub(crate) fn decimal(&self) -> char {
        char::from_u32(self.decimal_separator).unwrap_or('.')
    }

    /// The grouping separator as a `char`, `None` when the locale does not
    /// group.
    #[must_use]
    pub(crate) fn group(&self) -> Option<char> {
        if self.group_separator == 0 {
            None
        } else {
            char::from_u32(self.group_separator)
        }
    }
}

// ---- the currency table ----

impl MoneyCurrency {
    /// A currency `code` written `symbol` with `minor_digits` decimals
    /// (clamped to 4).
    #[must_use]
    pub fn create(code: AzString, symbol: AzString, minor_digits: u8) -> Self {
        Self {
            code,
            symbol,
            minor_digits: minor_digits.min(4),
        }
    }

    /// The currency of an ISO 4217 `code` (`EUR`, `usd`): its symbol and
    /// minor digits for the common codes; an unknown code is its own symbol
    /// with two decimals.
    #[must_use]
    pub fn from_code(code: AzString) -> Self {
        let code = code.as_str().trim().to_ascii_uppercase();
        let (symbol, minor_digits) = match code.as_str() {
            "USD" | "MXN" | "ARS" | "COP" => ("$", 2),
            "CLP" => ("$", 0),
            "EUR" => ("\u{20ac}", 2),
            "GBP" => ("\u{a3}", 2),
            "JPY" => ("\u{a5}", 0),
            "CNY" => ("\u{a5}", 2),
            "KRW" => ("\u{20a9}", 0),
            "INR" => ("\u{20b9}", 2),
            "RUB" => ("\u{20bd}", 2),
            "UAH" => ("\u{20b4}", 2),
            "TRY" => ("\u{20ba}", 2),
            "ILS" => ("\u{20aa}", 2),
            "THB" => ("\u{e3f}", 2),
            "VND" => ("\u{20ab}", 0),
            "PLN" => ("z\u{142}", 2),
            "CZK" => ("K\u{10d}", 2),
            "HUF" => ("Ft", 2),
            "SEK" | "NOK" | "DKK" => ("kr", 2),
            "ISK" => ("kr", 0),
            "BRL" => ("R$", 2),
            "ZAR" => ("R", 2),
            "CAD" => ("CA$", 2),
            "AUD" => ("A$", 2),
            "NZD" => ("NZ$", 2),
            "HKD" => ("HK$", 2),
            "SGD" => ("S$", 2),
            "IDR" => ("Rp", 2),
            "KWD" | "BHD" | "OMR" | "JOD" | "TND" | "LYD" | "IQD" => (code.as_str(), 3),
            "CLF" => (code.as_str(), 4),
            other => (other, 2),
        };
        let symbol = AzString::from(String::from(symbol));
        Self::create(AzString::from(code.clone()), symbol, minor_digits)
    }
}

// ---- parsing and formatting (the pure half) ----

/// Reads `text` as an amount of `currency` written in `locale`: `Ok(None)`
/// for an empty text, `Ok(Some(minor units))` for an amount, `Err` with the
/// reason otherwise ([`MoneyInputError::Incomplete`] for a text that can
/// still become an amount). Negatives are allowed here; the widget applies
/// its own `allow_negative` and bounds ([`check_amount`]).
pub(crate) fn parse_money(
    text: &str,
    locale: &MoneyLocale,
    currency: &MoneyCurrency,
) -> Result<Option<i64>, MoneyInputError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    let affixes = strip_affixes(text, currency)?;
    let number = read_number(affixes.number, locale, currency.minor_digits)?;
    if affixes.unclosed_paren {
        // `(12`: the closing parenthesis is still to come.
        return Err(MoneyInputError::Incomplete);
    }
    let magnitude = match number {
        Number::Complete(m) => m,
        Number::Incomplete => return Err(MoneyInputError::Incomplete),
    };
    let value = if affixes.negative {
        // i64::MIN's magnitude is one past i64::MAX.
        if magnitude > u128::from(i64::MAX.unsigned_abs()) + 1 {
            return Err(MoneyInputError::TooLarge);
        }
        i64::try_from(-(magnitude as i128)).map_err(|_| MoneyInputError::TooLarge)?
    } else {
        i64::try_from(magnitude).map_err(|_| MoneyInputError::TooLarge)?
    };
    Ok(Some(value))
}

/// What surrounds the digits of an amount: its sign (a `-` before or after,
/// a typographic minus, parentheses) and the currency (its code or symbol,
/// before or after), each at most once.
struct Affixes<'a> {
    /// The text between the affixes: digits and separators.
    number: &'a str,
    negative: bool,
    /// An opening parenthesis without its closing one (yet).
    unclosed_paren: bool,
}

/// `text` without its sign and currency ([`Affixes`]): `Invalid` for a sign
/// or a currency given twice, a `)` without its `(`.
fn strip_affixes<'a>(
    text: &'a str,
    currency: &MoneyCurrency,
) -> Result<Affixes<'a>, MoneyInputError> {
    let mut body = text;
    let mut negative = false;
    let mut sign_seen = false;
    let mut paren = false;
    let mut currency_seen = false;
    let currency_marks = [currency.symbol.as_str(), currency.code.as_str()];

    // Before the digits.
    loop {
        body = body.trim_start();
        if let Some(rest) = body.strip_prefix('(') {
            if paren || sign_seen {
                return Err(MoneyInputError::Invalid);
            }
            paren = true;
            negative = true;
            body = rest;
        } else if let Some(rest) = body
            .strip_prefix('-')
            .or_else(|| body.strip_prefix(MINUS_SIGN))
        {
            if sign_seen || paren {
                return Err(MoneyInputError::Invalid);
            }
            sign_seen = true;
            negative = true;
            body = rest;
        } else if let Some(rest) = body.strip_prefix('+') {
            if sign_seen || paren {
                return Err(MoneyInputError::Invalid);
            }
            sign_seen = true;
            body = rest;
        } else if let Some(rest) = currency_marks.iter().find_map(|m| strip_prefix_ci(body, m)) {
            if currency_seen {
                return Err(MoneyInputError::Invalid);
            }
            currency_seen = true;
            body = rest;
        } else {
            break;
        }
    }

    // After the digits.
    let mut closed = false;
    loop {
        body = body.trim_end();
        if let Some(rest) = body.strip_suffix(')') {
            if !paren || closed {
                return Err(MoneyInputError::Invalid);
            }
            closed = true;
            body = rest;
        } else if let Some(rest) = body
            .strip_suffix('-')
            .or_else(|| body.strip_suffix(MINUS_SIGN))
        {
            // A trailing minus needs digits before it (`1-` but not `--`).
            if sign_seen || paren || !body.chars().any(|c| c.is_ascii_digit()) {
                return Err(MoneyInputError::Invalid);
            }
            sign_seen = true;
            negative = true;
            body = rest;
        } else if let Some(rest) = currency_marks.iter().find_map(|m| strip_suffix_ci(body, m)) {
            if currency_seen {
                return Err(MoneyInputError::Invalid);
            }
            currency_seen = true;
            body = rest;
        } else {
            break;
        }
    }

    if paren && closed && !body.chars().any(|c| c.is_ascii_digit()) {
        // `()`: closed around nothing.
        return Err(MoneyInputError::Invalid);
    }
    Ok(Affixes {
        number: body,
        negative,
        unclosed_paren: paren && !closed,
    })
}

/// `s` without the prefix `mark`, compared ignoring ASCII case; `None` for
/// an empty mark or no match.
fn strip_prefix_ci<'a>(s: &'a str, mark: &str) -> Option<&'a str> {
    if mark.is_empty() {
        return None;
    }
    let head = s.get(..mark.len())?;
    head.eq_ignore_ascii_case(mark).then(|| &s[mark.len()..])
}

/// `s` without the suffix `mark`, compared ignoring ASCII case.
fn strip_suffix_ci<'a>(s: &'a str, mark: &str) -> Option<&'a str> {
    if mark.is_empty() || s.len() < mark.len() {
        return None;
    }
    let cut = s.len() - mark.len();
    let tail = s.get(cut..)?;
    tail.eq_ignore_ascii_case(mark).then(|| &s[..cut])
}

/// Spaces that separate thousands: a typed space stands for any of them.
const fn is_space_like(c: char) -> bool {
    matches!(c, ' ' | NBSP | NNBSP | '\u{2009}')
}

/// Apostrophes that separate thousands (Swiss): a typed `'` stands for any.
const fn is_apostrophe_like(c: char) -> bool {
    matches!(c, '\'' | SWISS_GROUP | '\u{2bc}')
}

/// `c` is the grouping separator `group` (or a character the keyboard types
/// for it).
fn is_group(c: char, group: Option<char>) -> bool {
    group.is_some_and(|g| {
        c == g
            || (is_space_like(c) && is_space_like(g))
            || (is_apostrophe_like(c) && is_apostrophe_like(g))
    })
}

/// The digits of an amount: its magnitude in minor units, or not finished.
enum Number {
    Complete(u128),
    Incomplete,
}

/// One character of the number part.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tok {
    Digit(u8),
    /// The locale's decimal point, or the other of `.` / `,` where the
    /// locale does not group with it (the keypad's key).
    Decimal,
    Group,
}

/// The magnitude in minor units of `number` (no sign, no currency) in
/// `locale` for a currency of `minor_digits` decimals (module docs: the
/// grammar and the lenient decimal point).
fn read_number(
    number: &str,
    locale: &MoneyLocale,
    minor_digits: u8,
) -> Result<Number, MoneyInputError> {
    let decimal = locale.decimal();
    let group = locale.group();
    let mut toks: Vec<Tok> = Vec::with_capacity(number.len());
    for c in number.chars() {
        let tok = if let Some(d) = c.to_digit(10) {
            Tok::Digit(d as u8)
        } else if c == decimal {
            Tok::Decimal
        } else if is_group(c, group) {
            Tok::Group
        } else if matches!(c, '.' | ',') {
            Tok::Decimal
        } else {
            return Err(MoneyInputError::Invalid);
        };
        toks.push(tok);
    }

    // The lenient decimal point: no decimal point, ONE group separator, and
    // between one and `minor_digits` digits after it - it cannot be grouping
    // (that takes three), so it is the point.
    let decimals = toks.iter().filter(|t| **t == Tok::Decimal).count();
    let groups: Vec<usize> = toks
        .iter()
        .enumerate()
        .filter(|(_, t)| **t == Tok::Group)
        .map(|(i, _)| i)
        .collect();
    if decimals == 0 && groups.len() == 1 {
        let after = toks.len() - groups[0] - 1;
        if (1..=usize::from(minor_digits)).contains(&after) && after != 3 {
            toks[groups[0]] = Tok::Decimal;
        }
    }
    if toks.iter().filter(|t| **t == Tok::Decimal).count() > 1 {
        return Err(MoneyInputError::Invalid);
    }

    // The integer part, in groups; the fraction.
    let point = toks.iter().position(|t| *t == Tok::Decimal);
    let (int_toks, frac_toks) = match point {
        Some(p) => (&toks[..p], &toks[p + 1..]),
        None => (&toks[..], &toks[toks.len()..]),
    };
    if frac_toks.iter().any(|t| *t == Tok::Group) {
        return Err(MoneyInputError::Invalid);
    }
    let mut runs: Vec<Vec<u8>> = alloc::vec![Vec::new()];
    for t in int_toks {
        match t {
            Tok::Digit(d) => {
                if let Some(run) = runs.last_mut() {
                    run.push(*d);
                }
            }
            Tok::Group => runs.push(Vec::new()),
            Tok::Decimal => {}
        }
    }
    let mut incomplete = false;
    if runs.len() > 1 {
        let last = runs.len() - 1;
        for (i, run) in runs.iter().enumerate() {
            let ok = match i {
                0 => (1..=3).contains(&run.len()),
                _ if i == last && point.is_none() && run.len() < 3 => {
                    // `1,234,5` / `1,`: the last group is still being typed.
                    incomplete = true;
                    true
                }
                _ => run.len() == 3,
            };
            if !ok {
                return Err(MoneyInputError::Invalid);
            }
        }
    }
    let int_digits: Vec<u8> = runs.concat();
    let frac_digits: Vec<u8> = frac_toks
        .iter()
        .filter_map(|t| match t {
            Tok::Digit(d) => Some(*d),
            _ => None,
        })
        .collect();
    if int_digits.is_empty() && frac_digits.is_empty() {
        return Ok(Number::Incomplete);
    }

    // Decimals past the currency's: refused unless they are zeros.
    let minor = usize::from(minor_digits);
    if frac_digits.len() > minor && frac_digits[minor..].iter().any(|d| *d != 0) {
        return Err(MoneyInputError::TooManyDecimals);
    }
    if int_digits.len() > 30 {
        return Err(MoneyInputError::TooLarge);
    }
    let mut magnitude: u128 = 0;
    for d in &int_digits {
        magnitude = magnitude * 10 + u128::from(*d);
    }
    for k in 0..minor {
        magnitude = magnitude * 10 + u128::from(frac_digits.get(k).copied().unwrap_or(0));
    }
    if magnitude > u128::from(u64::MAX) {
        return Err(MoneyInputError::TooLarge);
    }
    if incomplete {
        return Ok(Number::Incomplete);
    }
    Ok(Number::Complete(magnitude))
}

/// `amount` minor units of a currency with `minor_digits` decimals, written
/// in `locale`: grouped, the decimal point and every decimal, a leading `-`
/// for a negative amount (`-1.234,50`). With `symbol`, the currency on the
/// locale's side of it, spaced by a no-break space where the locale spaces
/// it (`1.234,50 EUR`, `$1,234.50`).
pub(crate) fn format_money(
    amount: i64,
    locale: &MoneyLocale,
    minor_digits: u8,
    symbol: Option<&str>,
) -> String {
    let minor = u32::from(minor_digits.min(18));
    // `unsigned_abs`: i64::MIN has no positive twin in i64.
    let magnitude = amount.unsigned_abs();
    let scale = 10_u64.pow(minor);
    let (whole, fraction) = (magnitude / scale, magnitude % scale);

    let digits = alloc::format!("{whole}");
    let mut number = String::with_capacity(digits.len() * 2 + minor as usize + 1);
    let lead = digits.len() % 3;
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (i + 3 - lead) % 3 == 0 {
            if let Some(g) = locale.group() {
                number.push(g);
            }
        }
        number.push(c);
    }
    if minor > 0 {
        number.push(locale.decimal());
        number.push_str(&alloc::format!(
            "{fraction:0width$}",
            width = minor as usize
        ));
    }

    let mut out = String::with_capacity(number.len() + 8);
    if amount < 0 {
        out.push('-');
    }
    match symbol.filter(|s| !s.is_empty()) {
        Some(sym) => {
            let space = if locale.symbol_spaced {
                Some(NBSP)
            } else {
                None
            };
            match locale.symbol_position {
                MoneySymbolPosition::Before => {
                    out.push_str(sym);
                    out.extend(space);
                    out.push_str(&number);
                }
                MoneySymbolPosition::After => {
                    out.push_str(&number);
                    out.extend(space);
                    out.push_str(sym);
                }
            }
        }
        None => out.push_str(&number),
    }
    out
}

/// `amount` against the state's rules: `Negative` when negatives are off,
/// `BelowMin` / `AboveMax` outside the bounds, `None` otherwise.
pub(crate) fn check_amount(amount: i64, state: &MoneyInputState) -> MoneyInputError {
    if amount < 0 && !state.allow_negative {
        return MoneyInputError::Negative;
    }
    if let OptionI64::Some(min) = state.min {
        if amount < min {
            return MoneyInputError::BelowMin;
        }
    }
    if let OptionI64::Some(max) = state.max {
        if amount > max {
            return MoneyInputError::AboveMax;
        }
    }
    MoneyInputError::None
}

/// What one edit of the field does: whether the keystroke is accepted, and
/// the state after it (the state before it when refused).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MoneyEdit {
    /// The keystroke happens.
    pub(crate) accepted: bool,
    /// The state after the edit.
    pub(crate) state: MoneyInputState,
}

/// The field's text became `text`: accept it or not, and the new state
/// (module docs: what is refused, what is only reported).
pub(crate) fn edit_money(
    text: &str,
    state: MoneyInputState,
    locale: &MoneyLocale,
    currency: &MoneyCurrency,
) -> MoneyEdit {
    let refused = MoneyEdit {
        accepted: false,
        state,
    };
    let accepted = |amount: OptionI64, error: MoneyInputError| MoneyEdit {
        accepted: true,
        state: MoneyInputState {
            amount,
            error,
            ..state
        },
    };
    // A sign where negatives are off can never become an allowed amount.
    if !state.allow_negative && text.chars().any(|c| matches!(c, '-' | '(' | MINUS_SIGN)) {
        return refused;
    }
    match parse_money(text, locale, currency) {
        Ok(None) => accepted(OptionI64::None, MoneyInputError::None),
        Ok(Some(amount)) => match check_amount(amount, &state) {
            MoneyInputError::Negative => refused,
            error => accepted(OptionI64::Some(amount), error),
        },
        Err(MoneyInputError::Incomplete) => accepted(OptionI64::None, MoneyInputError::Incomplete),
        Err(_) => refused,
    }
}

// ---- the callbacks ----

/// Callback invoked on every accepted edit that changes the state (the
/// amount or what is wrong with the text).
pub type MoneyInputOnChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, MoneyInputState) -> Update;
impl_widget_callback!(
    MoneyInputOnChange,
    OptionMoneyInputOnChange,
    MoneyInputOnChangeCallback,
    MoneyInputOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        MoneyInputOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MONEY_INPUT_ON_CHANGE_INVOKER,
    invoker_ty:     AzMoneyInputOnChangeCallbackInvoker,
    thunk_fn:       az_money_input_on_change_callback_thunk,
    setter_fn:      AzApp_setMoneyInputOnChangeCallbackInvoker,
    from_handle_fn: AzMoneyInputOnChangeCallback_createFromHostHandle,
    from_handle_byref_fn: AzMoneyInputOnChangeCallback_createFromHostHandleByref,
    extra_args:     [ state: MoneyInputState ],
}

/// Callback invoked when the field loses focus: the amount the user settled
/// on (the text is then shown in its canonical form).
pub type MoneyInputOnCommitCallbackType =
    extern "C" fn(RefAny, CallbackInfo, MoneyInputState) -> Update;
impl_widget_callback!(
    MoneyInputOnCommit,
    OptionMoneyInputOnCommit,
    MoneyInputOnCommitCallback,
    MoneyInputOnCommitCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        MoneyInputOnCommitCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MONEY_INPUT_ON_COMMIT_INVOKER,
    invoker_ty:     AzMoneyInputOnCommitCallbackInvoker,
    thunk_fn:       az_money_input_on_commit_callback_thunk,
    setter_fn:      AzApp_setMoneyInputOnCommitCallbackInvoker,
    from_handle_fn: AzMoneyInputOnCommitCallback_createFromHostHandle,
    from_handle_byref_fn: AzMoneyInputOnCommitCallback_createFromHostHandleByref,
    extra_args:     [ state: MoneyInputState ],
}

/// [`MoneyInputState`] with the hooks it reports to.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MoneyInputStateWrapper {
    /// The amount, the bounds, what is wrong.
    pub inner: MoneyInputState,
    /// Every accepted edit that changes `inner`.
    pub on_change: OptionMoneyInputOnChange,
    /// The field lost focus.
    pub on_commit: OptionMoneyInputOnCommit,
}

// ---- the widget ----

/// An amount of money in a currency (module docs).
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoneyInput {
    /// The amount, the bounds and the hooks.
    pub money_state: MoneyInputStateWrapper,
    /// The text field the amount is typed into (its placeholder and style
    /// are the caller's to set; its text and hooks are the money input's).
    pub text_input: TextInput,
    /// The currency: its minor digits and the code the addon shows.
    pub currency: MoneyCurrency,
    /// What this control is CALLED, for assistive technology (the field is
    /// announced as "<name> (<code>)").
    pub accessibility_name: OptionString,
    /// How the amount is written.
    pub locale: MoneyLocale,
    /// The widget theme, or `None` to follow the app theme.
    pub theme: OptionUiTheme,
    /// Show the currency code in an addon beside the field (default on).
    pub show_currency: bool,
}

impl Default for MoneyInput {
    fn default() -> Self {
        Self::create_empty(MoneyCurrency::default())
    }
}

/// What a theme decides about a money input: the row's and the addon's
/// paint. Built by `themes::flat::money_input_skin` and
/// `themes::flora::money_input_skin`; the field is the text input's own.
#[derive(Debug, Clone)]
pub(crate) struct MoneyInputSkin {
    /// The row holding the field and the addon.
    pub(crate) root: Vec<CssPropertyWithConditions>,
    /// The currency addon: its face, edge, ink and corners.
    pub(crate) addon: Vec<CssPropertyWithConditions>,
    /// The theme's marker class on the root, if it has one.
    pub(crate) marker: Option<&'static str>,
}

/// The row's structure, the same in every theme: the field and the addon
/// side by side, as tall as each other.
fn root_base() -> Vec<CssPropertyWithConditions> {
    use azul_css::props::{
        basic::PixelValue,
        layout::{LayoutAlignItems, LayoutColumnGap, LayoutFlexDirection},
        property::{CssProperty, LayoutColumnGapValue},
    };

    use crate::widgets::themes::decl;
    alloc::vec![
        decl::display_flex(),
        decl::flex_direction(LayoutFlexDirection::Row),
        decl::simple(CssProperty::const_align_items(LayoutAlignItems::Stretch)),
        decl::simple(CssProperty::ColumnGap(LayoutColumnGapValue::Exact(
            LayoutColumnGap {
                inner: PixelValue::const_px(4),
            }
        ))),
    ]
}

/// The field's slot: it takes the row's width the addon leaves.
fn field_slot_base() -> Vec<CssPropertyWithConditions> {
    use crate::widgets::themes::decl;
    alloc::vec![decl::display_flex(), decl::grow(1), decl::px_min_width(0.0)]
}

/// The addon's structure: its code centred on the field's midline, never
/// growing, never wrapping.
fn addon_base() -> Vec<CssPropertyWithConditions> {
    use azul_css::props::layout::LayoutAlignItems;

    use crate::widgets::themes::decl;
    alloc::vec![
        decl::display_flex(),
        decl::simple(azul_css::props::property::CssProperty::const_align_items(
            LayoutAlignItems::Center
        )),
        decl::grow(0),
        decl::no_shrink(),
        decl::nowrap(),
    ]
}

/// What every money-input hook shares: the state, the currency and the
/// locale the text is read in.
pub(crate) struct MoneyInputData {
    pub(crate) state: MoneyInputStateWrapper,
    pub(crate) currency: MoneyCurrency,
    pub(crate) locale: MoneyLocale,
}

impl MoneyInput {
    /// A money input holding `amount` minor units of `currency` (written in
    /// English until [`Self::with_locale`]).
    #[must_use]
    pub fn create(amount: i64, currency: MoneyCurrency) -> Self {
        let mut m = Self::create_empty(currency);
        m.money_state.inner.amount = OptionI64::Some(amount);
        m
    }

    /// An empty money input for `currency`.
    #[must_use]
    pub fn create_empty(currency: MoneyCurrency) -> Self {
        Self {
            money_state: MoneyInputStateWrapper::default(),
            text_input: TextInput::create(),
            currency,
            accessibility_name: OptionString::None,
            locale: MoneyLocale::en_us(),
            theme: OptionUiTheme::None,
            show_currency: true,
        }
    }

    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// The amount in minor units (`None`: empty).
    pub fn set_amount(&mut self, amount: OptionI64) {
        self.money_state.inner.amount = amount;
    }

    /// [`Self::set_amount`] for the builder chain.
    #[must_use]
    pub fn with_amount(mut self, amount: OptionI64) -> Self {
        self.set_amount(amount);
        self
    }

    /// How the amount is written (separators, the currency's side).
    pub fn set_locale(&mut self, locale: MoneyLocale) {
        self.locale = locale;
    }

    /// [`Self::set_locale`] for the builder chain.
    #[must_use]
    pub fn with_locale(mut self, locale: MoneyLocale) -> Self {
        self.set_locale(locale);
        self
    }

    /// The smallest amount the app accepts, in minor units (reported as
    /// `BelowMin`, never refused while typing).
    pub fn set_min(&mut self, min: i64) {
        self.money_state.inner.min = OptionI64::Some(min);
    }

    /// [`Self::set_min`] for the builder chain.
    #[must_use]
    pub fn with_min(mut self, min: i64) -> Self {
        self.set_min(min);
        self
    }

    /// The largest amount the app accepts, in minor units (reported as
    /// `AboveMax`).
    pub fn set_max(&mut self, max: i64) {
        self.money_state.inner.max = OptionI64::Some(max);
    }

    /// [`Self::set_max`] for the builder chain.
    #[must_use]
    pub fn with_max(mut self, max: i64) -> Self {
        self.set_max(max);
        self
    }

    /// Whether a negative amount may be typed (default: yes).
    pub fn set_allow_negative(&mut self, allow_negative: bool) {
        self.money_state.inner.allow_negative = allow_negative;
    }

    /// [`Self::set_allow_negative`] for the builder chain.
    #[must_use]
    pub fn with_allow_negative(mut self, allow_negative: bool) -> Self {
        self.set_allow_negative(allow_negative);
        self
    }

    /// The prompt of the empty field (default: zero in the locale, `0.00`).
    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.text_input.set_placeholder(placeholder);
    }

    /// [`Self::set_placeholder`] for the builder chain.
    #[must_use]
    pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    /// Show the currency code beside the field (default on).
    pub fn set_show_currency(&mut self, show_currency: bool) {
        self.show_currency = show_currency;
    }

    /// [`Self::set_show_currency`] for the builder chain.
    #[must_use]
    pub fn with_show_currency(mut self, show_currency: bool) -> Self {
        self.set_show_currency(show_currency);
        self
    }

    /// The callback every accepted edit that changes the state reports to.
    pub fn set_on_change<C: Into<MoneyInputOnChangeCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.money_state.on_change = Some(MoneyInputOnChange::create(data, callback)).into();
    }

    /// [`Self::set_on_change`] for the builder chain.
    #[must_use]
    pub fn with_on_change<C: Into<MoneyInputOnChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_change(data, callback);
        self
    }

    /// The callback the field reports to when it loses focus.
    pub fn set_on_commit<C: Into<MoneyInputOnCommitCallback>>(
        &mut self,
        data: RefAny,
        callback: C,
    ) {
        self.money_state.on_commit = Some(MoneyInputOnCommit::create(data, callback)).into();
    }

    /// [`Self::set_on_commit`] for the builder chain.
    #[must_use]
    pub fn with_on_commit<C: Into<MoneyInputOnCommitCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_commit(data, callback);
        self
    }

    /// Pin the widget theme. Unset (`None`), it follows the app theme.
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Replaces `self` with an empty money input and returns the original.
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// `amount` minor units of `currency` as `locale` writes them, with the
    /// currency's symbol on its side (`1.234,56 EUR`, `$1,234.56`): for a
    /// table, a label, a total.
    #[must_use]
    pub fn format_amount(amount: i64, currency: MoneyCurrency, locale: MoneyLocale) -> AzString {
        AzString::from(format_money(
            amount,
            &locale,
            currency.minor_digits,
            Some(currency.symbol.as_str()),
        ))
    }

    /// `text` read as an amount of `currency` written in `locale` (an
    /// imported CSV cell, a pasted value): the minor units, or the reason
    /// there are none.
    #[must_use]
    pub fn parse_amount(
        text: AzString,
        currency: MoneyCurrency,
        locale: MoneyLocale,
    ) -> MoneyParseResult {
        match parse_money(text.as_str(), &locale, &currency) {
            Ok(amount) => MoneyParseResult {
                amount: amount.map_or(OptionI64::None, OptionI64::Some),
                error: MoneyInputError::None,
            },
            Err(error) => MoneyParseResult {
                amount: OptionI64::None,
                error,
            },
        }
    }

    /// The field's text for the state's amount, canonical (grouped, every
    /// decimal); `None` without an amount - the field then keeps the text it
    /// was given (what the user typed, handed back on a rebuild).
    fn amount_text(&self) -> Option<String> {
        match self.money_state.inner.amount {
            OptionI64::Some(a) => Some(format_money(
                a,
                &self.locale,
                self.currency.minor_digits,
                None,
            )),
            OptionI64::None => None,
        }
    }

    /// Renders the money input: the field and the currency addon, in the
    /// theme's skin (pinned, or both skins merged to follow the app theme).
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::{flat, flora, theme_blocks::skins_of};
        let skins = skins_of(self.theme, flat::money_input_skin, flora::money_input_skin);
        self.build(&skins)
    }

    /// The DOM in `skins` (`theme_blocks::skins_of`).
    pub(crate) fn build(mut self, skins: &[MoneyInputSkin]) -> Dom {
        use crate::widgets::themes::theme_blocks::{part_of, structure_skin};

        let marker = structure_skin(skins, self.theme).and_then(|s| s.marker);
        let root_style = part_of(skins, |s| {
            let mut v = root_base();
            v.extend(s.root.iter().cloned());
            v
        });
        let addon_style = part_of(skins, |s| {
            let mut v = addon_base();
            v.extend(s.addon.iter().cloned());
            v
        });

        // The field: the amount's canonical text, the locale's zero as its
        // prompt, named with the currency, the two hooks.
        if let Some(text) = self.amount_text() {
            self.text_input.set_text(AzString::from(text));
        }
        if self.text_input.text_input_state.inner.placeholder.is_none() {
            let zero = format_money(0, &self.locale, self.currency.minor_digits, None);
            self.text_input.set_placeholder(AzString::from(zero));
        }
        let name = match self.accessibility_name.as_ref() {
            Some(n) => alloc::format!("{} ({})", n.as_str(), self.currency.code.as_str()),
            None => alloc::format!("Amount ({})", self.currency.code.as_str()),
        };
        self.text_input.accessibility_name = OptionString::Some(AzString::from(name));
        if let Some(theme) = self.theme.into_option() {
            self.text_input.set_theme(theme);
        }
        let data = RefAny::new(MoneyInputData {
            state: self.money_state,
            currency: self.currency.clone(),
            locale: self.locale,
        });
        let on_input: TextInputOnTextInputCallbackType = on_money_text_input;
        self.text_input.set_on_text_input(data.clone(), on_input);
        let on_blur: TextInputOnFocusLostCallbackType = on_money_focus_lost;
        self.text_input.set_on_focus_lost(data, on_blur);
        let field = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(field_slot_base()))
            .with_child(self.text_input.dom());

        let mut children = Vec::with_capacity(2);
        let addon = self.show_currency.then(|| {
            crate::widgets::widget_p_with_text(self.currency.code.clone())
                .with_ids_and_classes(IdOrClassVec::from_vec(alloc::vec![Class(
                    AzString::from_const_str(MONEY_INPUT_ADDON_CLASS)
                )]))
                .with_css_props(addon_style)
        });
        match (addon, self.locale.symbol_position) {
            (Some(addon), MoneySymbolPosition::Before) => {
                children.push(addon);
                children.push(field);
            }
            (Some(addon), MoneySymbolPosition::After) => {
                children.push(field);
                children.push(addon);
            }
            (None, _) => children.push(field),
        }

        let mut classes: Vec<IdOrClass> =
            alloc::vec![Class(AzString::from_const_str(MONEY_INPUT_CLASS))];
        if let Some(marker) = marker {
            classes.push(Class(AzString::from_const_str(marker)));
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css_props(root_style)
            .with_children(children.into())
    }
}

impl From<MoneyInput> for Dom {
    fn from(m: MoneyInput) -> Self {
        m.dom()
    }
}

/// The field's text changed: accept the edit or refuse it (module docs),
/// store the new state and tell the app when it changed.
extern "C" fn on_money_text_input(
    mut data: RefAny,
    info: CallbackInfo,
    field: TextInputState,
) -> OnTextInputReturn {
    let text = field.get_text();
    let (state, hook) = {
        let Some(mut d) = data.downcast_mut::<MoneyInputData>() else {
            return OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            };
        };
        let edit = edit_money(&text, d.state.inner, &d.locale, &d.currency);
        if !edit.accepted {
            return OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::No,
            };
        }
        let changed = edit.state != d.state.inner;
        d.state.inner = edit.state;
        let hook = if changed {
            d.state.on_change.clone()
        } else {
            OptionMoneyInputOnChange::None
        };
        (edit.state, hook)
    };
    let update = match hook.as_ref() {
        Some(MoneyInputOnChange { callback, refany }) => {
            callback.invoke(refany.clone(), info, state)
        }
        None => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The field lost focus: read its final text once more, show an amount in
/// its canonical form (`1234,5` -> `1.234,50`) and report the commit.
extern "C" fn on_money_focus_lost(
    mut data: RefAny,
    mut info: CallbackInfo,
    field: TextInputState,
) -> Update {
    let container = info.get_hit_node();
    let text = field.get_text();
    let (state, canonical, hook) = {
        let Some(mut d) = data.downcast_mut::<MoneyInputData>() else {
            return Update::DoNothing;
        };
        let edit = edit_money(&text, d.state.inner, &d.locale, &d.currency);
        if edit.accepted {
            d.state.inner = edit.state;
        }
        let canonical = match d.state.inner.amount {
            OptionI64::Some(a) => Some(format_money(a, &d.locale, d.currency.minor_digits, None)),
            OptionI64::None => None,
        };
        (d.state.inner, canonical, d.state.on_commit.clone())
    };
    if let Some(canonical) = canonical.filter(|c| *c != text) {
        TextInput::set_text_in(&mut info, container, AzString::from(canonical));
    }
    match hook.as_ref() {
        Some(MoneyInputOnCommit { callback, refany }) => {
            callback.invoke(refany.clone(), info, state)
        }
        None => Update::DoNothing,
    }
}

#[cfg(test)]
mod money_tests {
    use super::*;

    fn eur() -> MoneyCurrency {
        MoneyCurrency::from_code(AzString::from_const_str("EUR"))
    }

    fn usd() -> MoneyCurrency {
        MoneyCurrency::from_code(AzString::from_const_str("USD"))
    }

    fn jpy() -> MoneyCurrency {
        MoneyCurrency::from_code(AzString::from_const_str("JPY"))
    }

    fn en(text: &str) -> Result<Option<i64>, MoneyInputError> {
        parse_money(text, &MoneyLocale::en_us(), &usd())
    }

    fn de(text: &str) -> Result<Option<i64>, MoneyInputError> {
        parse_money(text, &MoneyLocale::de_de(), &eur())
    }

    #[test]
    fn an_english_amount_reads_as_minor_units() {
        assert_eq!(en("1,234.56"), Ok(Some(123_456)));
        assert_eq!(en("1234.56"), Ok(Some(123_456)));
        assert_eq!(en("0.5"), Ok(Some(50)));
        assert_eq!(en(".5"), Ok(Some(50)));
        assert_eq!(en("12"), Ok(Some(1200)));
        assert_eq!(en("12."), Ok(Some(1200)));
        assert_eq!(en("1,234,567.89"), Ok(Some(123_456_789)));
    }

    #[test]
    fn a_german_amount_reads_with_the_comma_as_its_decimal_point() {
        assert_eq!(de("1.234,56"), Ok(Some(123_456)));
        assert_eq!(de("1234,5"), Ok(Some(123_450)));
        assert_eq!(de("0,05"), Ok(Some(5)));
        assert_eq!(de("1.234.567"), Ok(Some(123_456_700)));
    }

    #[test]
    fn a_french_amount_reads_with_spaces_between_its_groups() {
        let fr = MoneyLocale::fr_fr();
        for text in ["1\u{202f}234,56", "1 234,56", "1\u{a0}234,56"] {
            assert_eq!(
                parse_money(text, &fr, &eur()),
                Ok(Some(123_456)),
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_swiss_amount_reads_with_the_apostrophe_grouping() {
        let ch = MoneyLocale::de_ch();
        let chf = MoneyCurrency::from_code(AzString::from_const_str("CHF"));
        for text in ["1\u{2019}234.50", "1'234.50", "1234.50"] {
            assert_eq!(parse_money(text, &ch, &chf), Ok(Some(123_450)), "{text:?}");
        }
    }

    #[test]
    fn an_empty_text_is_no_amount_and_no_error() {
        assert_eq!(en(""), Ok(None));
        assert_eq!(en("   "), Ok(None));
    }

    #[test]
    fn a_negative_amount_reads_with_a_minus_or_in_parentheses() {
        assert_eq!(en("-12.50"), Ok(Some(-1250)));
        assert_eq!(en("\u{2212}12.50"), Ok(Some(-1250)));
        assert_eq!(en("(12.50)"), Ok(Some(-1250)));
        assert_eq!(en("12.50-"), Ok(Some(-1250)));
        assert_eq!(en("+12.50"), Ok(Some(1250)));
        assert_eq!(de("-1.234,50"), Ok(Some(-123_450)));
    }

    #[test]
    fn the_currency_code_or_symbol_may_be_typed_around_the_amount() {
        assert_eq!(de("\u{20ac} 12,50"), Ok(Some(1250)));
        assert_eq!(de("12,50 EUR"), Ok(Some(1250)));
        assert_eq!(de("12,50\u{20ac}"), Ok(Some(1250)));
        assert_eq!(en("$12.50"), Ok(Some(1250)));
        assert_eq!(en("12.50 usd"), Ok(Some(1250)));
        assert_eq!(en("-$12.50"), Ok(Some(-1250)));
    }

    #[test]
    fn more_decimals_than_the_currency_has_are_refused_unless_they_are_zeros() {
        assert_eq!(en("1.234"), Err(MoneyInputError::TooManyDecimals));
        assert_eq!(en("1.230"), Ok(Some(123)));
        let yen = |t: &str| parse_money(t, &MoneyLocale::en_us(), &jpy());
        assert_eq!(yen("12.5"), Err(MoneyInputError::TooManyDecimals));
        assert_eq!(yen("1,234"), Ok(Some(1234)));
        assert_eq!(yen("12.0"), Ok(Some(12)));
    }

    #[test]
    fn a_lone_separator_that_cannot_group_is_the_decimal_point() {
        // The numeric keypad's '.' in German, and ',' in English.
        assert_eq!(de("12.5"), Ok(Some(1250)));
        assert_eq!(de("12.50"), Ok(Some(1250)));
        assert_eq!(en("12,5"), Ok(Some(1250)));
        assert_eq!(en("12,50"), Ok(Some(1250)));
        // Followed by three digits it IS grouping.
        assert_eq!(de("1.234"), Ok(Some(123_400)));
        assert_eq!(en("1,234"), Ok(Some(123_400)));
        // French groups with spaces: a '.' is the decimal point.
        assert_eq!(
            parse_money("12.50", &MoneyLocale::fr_fr(), &eur()),
            Ok(Some(1250))
        );
    }

    #[test]
    fn grouping_must_come_in_threes() {
        assert_eq!(en("1,23,456.00"), Err(MoneyInputError::Invalid));
        assert_eq!(en("12,3456"), Err(MoneyInputError::Invalid));
        assert_eq!(en(",123"), Err(MoneyInputError::Invalid));
        assert_eq!(en("1,,234"), Err(MoneyInputError::Invalid));
        assert_eq!(de("1.234,5.6"), Err(MoneyInputError::Invalid));
    }

    #[test]
    fn a_text_on_the_way_to_an_amount_is_incomplete() {
        for text in ["-", "+", "(", ".", "-.", "1,", "$", "-$", "(12"] {
            assert_eq!(en(text), Err(MoneyInputError::Incomplete), "{text:?}");
        }
        assert_eq!(de("1."), Err(MoneyInputError::Incomplete));
        assert_eq!(de("\u{20ac}"), Err(MoneyInputError::Incomplete));
    }

    #[test]
    fn letters_and_doubled_points_are_invalid() {
        for text in ["12a", "1.2.3", "--1", "1-2", "abc", "1 2", "12)", "()"] {
            assert_eq!(en(text), Err(MoneyInputError::Invalid), "{text:?}");
        }
    }

    #[test]
    fn an_amount_past_the_64_bit_range_is_too_large() {
        assert_eq!(en("99999999999999999999"), Err(MoneyInputError::TooLarge));
        assert_eq!(en("92233720368547758.08"), Err(MoneyInputError::TooLarge));
        assert_eq!(en("92233720368547758.07"), Ok(Some(i64::MAX)));
    }

    #[test]
    fn an_amount_writes_with_the_locales_grouping_and_every_decimal() {
        let en = MoneyLocale::en_us();
        assert_eq!(format_money(123_456, &en, 2, None), "1,234.56");
        assert_eq!(
            format_money(123_456, &MoneyLocale::de_de(), 2, None),
            "1.234,56"
        );
        assert_eq!(
            format_money(123_456, &MoneyLocale::fr_fr(), 2, None),
            "1\u{202f}234,56"
        );
        assert_eq!(
            format_money(123_456, &MoneyLocale::de_ch(), 2, None),
            "1\u{2019}234.56"
        );
        assert_eq!(format_money(-150, &en, 2, None), "-1.50");
        assert_eq!(format_money(5, &en, 2, None), "0.05");
        assert_eq!(format_money(-5, &en, 2, None), "-0.05");
        assert_eq!(format_money(1_234_567, &en, 0, None), "1,234,567");
        assert_eq!(format_money(0, &MoneyLocale::de_de(), 2, None), "0,00");
        assert_eq!(format_money(123, &en, 3, None), "0.123");
        assert_eq!(
            format_money(i64::MIN, &en, 2, None),
            "-92,233,720,368,547,758.08"
        );
        let ungrouped = MoneyLocale::en_us().with_separators('.' as u32, 0);
        assert_eq!(format_money(123_456, &ungrouped, 2, None), "1234.56");
    }

    #[test]
    fn the_symbol_goes_on_the_locales_side() {
        let en = MoneyLocale::en_us();
        assert_eq!(format_money(123_456, &en, 2, Some("$")), "$1,234.56");
        assert_eq!(format_money(-123_456, &en, 2, Some("$")), "-$1,234.56");
        assert_eq!(
            format_money(123_456, &MoneyLocale::de_de(), 2, Some("\u{20ac}")),
            "1.234,56\u{a0}\u{20ac}"
        );
        assert_eq!(
            format_money(123_456, &MoneyLocale::de_ch(), 2, Some("CHF")),
            "CHF\u{a0}1\u{2019}234.56"
        );
    }

    #[test]
    fn formatting_then_parsing_gives_back_the_amount() {
        let locales = [
            MoneyLocale::en_us(),
            MoneyLocale::de_de(),
            MoneyLocale::fr_fr(),
            MoneyLocale::de_ch(),
        ];
        for locale in locales {
            for amount in [
                0_i64,
                1,
                99,
                100,
                123_456,
                -987_654_321,
                i64::MAX,
                i64::MIN + 1,
            ] {
                let text = format_money(amount, &locale, 2, None);
                assert_eq!(
                    parse_money(&text, &locale, &eur()),
                    Ok(Some(amount)),
                    "{locale:?} {text:?}"
                );
                let with_symbol = format_money(amount, &locale, 2, Some("\u{20ac}"));
                assert_eq!(
                    parse_money(&with_symbol, &locale, &eur()),
                    Ok(Some(amount)),
                    "{locale:?} {with_symbol:?}"
                );
            }
        }
    }

    #[test]
    fn a_locale_tag_picks_its_separators() {
        let tag = |t: &'static str| MoneyLocale::from_tag(AzString::from_const_str(t));
        assert_eq!(tag("en-US"), MoneyLocale::en_us());
        assert_eq!(tag("de-DE"), MoneyLocale::de_de());
        assert_eq!(tag("de"), MoneyLocale::de_de());
        assert_eq!(tag("de_AT"), MoneyLocale::de_de());
        assert_eq!(tag("DE-de"), MoneyLocale::de_de());
        assert_eq!(tag("fr-FR"), MoneyLocale::fr_fr());
        assert_eq!(tag("de-CH"), MoneyLocale::de_ch());
        assert_eq!(tag("ja-JP"), MoneyLocale::en_us());
        assert_eq!(tag("xx"), MoneyLocale::en_us());
        assert_eq!(tag(""), MoneyLocale::en_us());
        let br = tag("pt-BR");
        assert_eq!((br.decimal(), br.group()), (',', Some('.')));
        assert_eq!(br.symbol_position, MoneySymbolPosition::Before);
        let sv = tag("sv-SE");
        assert_eq!((sv.decimal(), sv.group()), (',', Some('\u{a0}')));
        assert_eq!(sv.symbol_position, MoneySymbolPosition::After);
    }

    #[test]
    fn a_formatted_sample_gives_its_separators() {
        let sample = |s: &'static str| MoneyLocale::from_sample(AzString::from_const_str(s));
        let en = sample("1,234,567.89");
        assert_eq!((en.decimal(), en.group()), ('.', Some(',')));
        assert_eq!(en.symbol_position, MoneySymbolPosition::Before);
        let de = sample("1.234.567,89");
        assert_eq!((de.decimal(), de.group()), (',', Some('.')));
        assert_eq!(de.symbol_position, MoneySymbolPosition::After);
        let fr = sample("1\u{202f}234\u{202f}567,89");
        assert_eq!((fr.decimal(), fr.group()), (',', Some('\u{202f}')));
        let ch = sample("1\u{2019}234\u{2019}567.89");
        assert_eq!((ch.decimal(), ch.group()), ('.', Some('\u{2019}')));
        let plain = sample("1234567.89");
        assert_eq!((plain.decimal(), plain.group()), ('.', None));
        assert_eq!(sample("garbage"), MoneyLocale::en_us());
        assert_eq!(sample(""), MoneyLocale::en_us());
    }

    #[test]
    fn a_currency_code_knows_its_minor_digits_and_symbol() {
        let eur = eur();
        assert_eq!(
            (eur.code.as_str(), eur.symbol.as_str(), eur.minor_digits),
            ("EUR", "\u{20ac}", 2)
        );
        let yen = MoneyCurrency::from_code(AzString::from_const_str("jpy"));
        assert_eq!(
            (yen.code.as_str(), yen.symbol.as_str(), yen.minor_digits),
            ("JPY", "\u{a5}", 0)
        );
        let kwd = MoneyCurrency::from_code(AzString::from_const_str("KWD"));
        assert_eq!(kwd.minor_digits, 3);
        let gbp = MoneyCurrency::from_code(AzString::from_const_str("GBP"));
        assert_eq!(gbp.symbol.as_str(), "\u{a3}");
        let odd = MoneyCurrency::from_code(AzString::from_const_str("XYZ"));
        assert_eq!((odd.symbol.as_str(), odd.minor_digits), ("XYZ", 2));
        let capped = MoneyCurrency::create(
            AzString::from_const_str("X"),
            AzString::from_const_str("x"),
            9,
        );
        assert_eq!(capped.minor_digits, 4);
    }

    #[test]
    fn an_amount_is_checked_against_the_sign_and_the_bounds() {
        let mut state = MoneyInputState::default();
        assert_eq!(check_amount(-1, &state), MoneyInputError::None);
        state.allow_negative = false;
        assert_eq!(check_amount(-1, &state), MoneyInputError::Negative);
        state.min = OptionI64::Some(500);
        state.max = OptionI64::Some(10_000);
        assert_eq!(check_amount(499, &state), MoneyInputError::BelowMin);
        assert_eq!(check_amount(500, &state), MoneyInputError::None);
        assert_eq!(check_amount(10_000, &state), MoneyInputError::None);
        assert_eq!(check_amount(10_001, &state), MoneyInputError::AboveMax);
    }

    fn edit(text: &str, state: MoneyInputState) -> MoneyEdit {
        edit_money(text, state, &MoneyLocale::de_de(), &eur())
    }

    #[test]
    fn an_edit_to_an_amount_is_accepted_and_stored() {
        let e = edit("12,50", MoneyInputState::default());
        assert!(e.accepted);
        assert_eq!(e.state.amount, OptionI64::Some(1250));
        assert_eq!(e.state.error, MoneyInputError::None);
    }

    #[test]
    fn an_edit_on_the_way_to_an_amount_is_accepted_without_one() {
        let before = MoneyInputState {
            amount: OptionI64::Some(100),
            ..MoneyInputState::default()
        };
        let e = edit("-", before);
        assert!(e.accepted);
        assert_eq!(e.state.amount, OptionI64::None);
        assert_eq!(e.state.error, MoneyInputError::Incomplete);
        let e = edit("", before);
        assert!(e.accepted);
        assert_eq!(e.state.amount, OptionI64::None);
        assert_eq!(e.state.error, MoneyInputError::None);
    }

    #[test]
    fn an_edit_that_can_never_be_an_amount_is_refused_and_changes_nothing() {
        let before = MoneyInputState {
            amount: OptionI64::Some(100),
            ..MoneyInputState::default()
        };
        for text in ["1a", "1,234", "99999999999999999999"] {
            let e = edit(text, before);
            assert!(!e.accepted, "{text:?}");
            assert_eq!(e.state, before, "{text:?}");
        }
    }

    #[test]
    fn a_minus_is_refused_where_negatives_are_off() {
        let off = MoneyInputState {
            allow_negative: false,
            ..MoneyInputState::default()
        };
        assert!(!edit("-", off).accepted);
        assert!(!edit("-1", off).accepted);
        assert!(edit("1", off).accepted);
    }

    #[test]
    fn the_bounds_are_reported_but_never_refuse_a_keystroke() {
        let bounded = MoneyInputState {
            min: OptionI64::Some(5000),
            max: OptionI64::Some(10_000),
            ..MoneyInputState::default()
        };
        let e = edit("1", bounded);
        assert!(e.accepted, "typing 1 on the way to 150 must work");
        assert_eq!(e.state.amount, OptionI64::Some(100));
        assert_eq!(e.state.error, MoneyInputError::BelowMin);
        let e = edit("150", bounded);
        assert!(e.accepted);
        assert_eq!(e.state.error, MoneyInputError::AboveMax);
        let e = edit("75", bounded);
        assert_eq!(e.state.error, MoneyInputError::None);
    }
}

// ==== fixtures (the widget manifest's sample) ====

/// Samples for the widget manifest (`widgets::label_convention`) and the
/// tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// 1.234,56 EUR in German, named "Amount".
    pub(crate) fn sample() -> MoneyInput {
        MoneyInput::create(
            123_456,
            MoneyCurrency::from_code(AzString::from_const_str("EUR")),
        )
        .with_locale(MoneyLocale::de_de())
        .with_accessibility_name("Amount")
    }

    /// An empty dollar amount in English: the currency before the field.
    pub(crate) fn empty_dollars() -> MoneyInput {
        MoneyInput::create_empty(MoneyCurrency::from_code(AzString::from_const_str("USD")))
            .with_accessibility_name("Price")
    }
}

#[cfg(test)]
mod dom_tests {
    use std::sync::{Arc, Mutex};

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, NodeType},
        events::FocusEventFilter,
        id::NodeId,
        styled_dom::{NodeHierarchyItemId, StyledDom},
    };

    use super::{fixtures::*, *};
    use crate::{
        callbacks::CallbackChange,
        widgets::{
            roving::test_support as rv,
            text_input::{TextInputStateWrapper, TEXT_INPUT_CONTAINER_CLASS},
            themes::{theme_blocks::checks, theme_checks as tc},
        },
    };

    /// The text input's own state, as the field carries it.
    fn field_state(dom: &Dom) -> TextInputStateWrapper {
        let field = tc::find(dom, TEXT_INPUT_CONTAINER_CLASS).expect("the field");
        let mut data = field
            .root
            .get_dataset()
            .cloned()
            .expect("the field's state");
        let state = data
            .downcast_ref::<TextInputStateWrapper>()
            .map(|s| (*s).clone())
            .expect("a text input state");
        state
    }

    /// The texts of the tree, in order.
    fn texts(dom: &Dom) -> Vec<String> {
        tc::nodes(dom)
            .into_iter()
            .filter_map(|(_, n)| match n.root.get_node_type() {
                NodeType::Text(s) => Some(s.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_field_shows_the_amount_in_the_locales_canonical_form() {
        let state = field_state(&sample().dom());
        assert_eq!(state.inner.get_text(), "1.234,56");
    }

    #[test]
    fn the_currency_code_sits_on_the_locales_side_of_the_field() {
        let de = sample().with_theme(UiTheme::Flat).dom();
        let kids = de.children.as_ref();
        assert_eq!(kids.len(), 2);
        assert!(
            tc::find(&kids[0], TEXT_INPUT_CONTAINER_CLASS).is_some(),
            "de: field first"
        );
        assert!(
            tc::has_class(&kids[1], MONEY_INPUT_ADDON_CLASS),
            "de: EUR after"
        );
        let en = empty_dollars().with_theme(UiTheme::Flat).dom();
        let kids = en.children.as_ref();
        assert!(
            tc::has_class(&kids[0], MONEY_INPUT_ADDON_CLASS),
            "en: USD before"
        );
        assert!(tc::find(&kids[1], TEXT_INPUT_CONTAINER_CLASS).is_some());
        assert!(texts(&en).iter().any(|t| t == "USD"));
    }

    #[test]
    fn without_the_currency_there_is_no_addon() {
        let dom = sample().with_show_currency(false).dom();
        assert!(tc::find(&dom, MONEY_INPUT_ADDON_CLASS).is_none());
        assert_eq!(dom.children.as_ref().len(), 1);
    }

    #[test]
    fn the_field_is_named_with_its_currency() {
        let dom = sample().dom();
        let field = tc::find(&dom, TEXT_INPUT_CONTAINER_CLASS).expect("the field");
        let name = field.root.get_accessibility_info().and_then(|a| {
            a.accessibility_name
                .as_ref()
                .map(|n| n.as_str().to_string())
        });
        assert_eq!(name.as_deref(), Some("Amount (EUR)"));
    }

    #[test]
    fn the_empty_field_prompts_with_the_locales_zero_and_keeps_its_text() {
        let state = field_state(&empty_dollars().dom());
        assert_eq!(
            state
                .inner
                .placeholder
                .as_ref()
                .map(|p| p.as_str().to_string()),
            Some(String::from("0.00"))
        );
        let mut typed = empty_dollars();
        typed.text_input.set_text(AzString::from_const_str("12,"));
        assert_eq!(field_state(&typed.dom()).inner.get_text(), "12,");
    }

    #[test]
    fn the_field_carries_the_money_hooks() {
        let state = field_state(&sample().dom());
        assert!(state.on_text_input.as_ref().is_some(), "the keystroke rule");
        assert!(state.on_focus_lost.as_ref().is_some(), "the commit");
    }

    type Log = Arc<Mutex<Vec<MoneyInputState>>>;

    extern "C" fn record(mut data: RefAny, _info: CallbackInfo, state: MoneyInputState) -> Update {
        if let Some(log) = data.downcast_ref::<Log>() {
            log.lock().expect("log").push(state);
        }
        Update::RefreshDom
    }

    #[test]
    fn leaving_the_field_shows_the_amount_canonical_and_reports_the_commit() {
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        let mut m =
            MoneyInput::create_empty(MoneyCurrency::from_code(AzString::from_const_str("EUR")))
                .with_locale(MoneyLocale::de_de())
                .with_theme(UiTheme::Flat)
                .with_on_commit(
                    RefAny::new(log.clone()),
                    record as MoneyInputOnCommitCallbackType,
                );
        m.text_input.set_text(AzString::from_const_str("1234,5"));
        let styled = StyledDom::create_from_dom(m.dom());
        let container = styled
            .node_data
            .as_ref()
            .iter()
            .position(|n| n.has_class(TEXT_INPUT_CONTAINER_CLASS))
            .expect("the field");
        let target = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(container))),
        };
        let (update, changes) = rv::fire(
            &styled,
            target,
            EventFilter::Focus(FocusEventFilter::FocusLost),
        )
        .expect("the field hears the blur");
        assert_eq!(update, Update::RefreshDom, "the app's commit hook ran");
        let written: Vec<String> = changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeText { text, .. } => Some(text.as_str().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(written, vec![String::from("1.234,50")]);
        let committed = log.lock().expect("log").clone();
        assert_eq!(committed.len(), 1);
        assert_eq!(committed[0].amount, OptionI64::Some(123_450));
        assert_eq!(committed[0].error, MoneyInputError::None);
    }

    #[test]
    fn a_money_input_follows_the_app_theme() {
        checks::assert_follows_the_app_theme(
            "money input",
            || sample().dom(),
            |theme| sample().with_theme(theme).dom(),
        );
    }

    #[test]
    fn a_pinned_money_input_keeps_its_theme_invariants() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            tc::assert_theme_invariants("money input", &sample().with_theme(theme).dom());
        }
    }

    #[test]
    fn the_amount_formats_and_parses_through_the_api() {
        let eur = MoneyCurrency::from_code(AzString::from_const_str("EUR"));
        assert_eq!(
            MoneyInput::format_amount(123_456, eur.clone(), MoneyLocale::de_de()).as_str(),
            "1.234,56\u{a0}\u{20ac}"
        );
        let parsed = MoneyInput::parse_amount(
            AzString::from_const_str("1.234,56 \u{20ac}"),
            eur.clone(),
            MoneyLocale::de_de(),
        );
        assert_eq!(parsed.amount, OptionI64::Some(123_456));
        assert_eq!(parsed.error, MoneyInputError::None);
        let bad =
            MoneyInput::parse_amount(AzString::from_const_str("1,2,3"), eur, MoneyLocale::de_de());
        assert_eq!(bad.amount, OptionI64::None);
        assert_eq!(bad.error, MoneyInputError::Invalid);
    }
}
