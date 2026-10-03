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
    /// The symbol (`€`), for [`MoneyInput::format_amount`].
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

    /// `1.234,56`, `1.234,56 €` (German and most of continental Europe).
    #[must_use]
    pub const fn de_de() -> Self {
        Self::create(',' as u32, '.' as u32, MoneySymbolPosition::After)
    }

    /// `1 234,56 €` with a narrow no-break space (French).
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
        let _ = tag;
        Self::en_us()
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
        let _ = sample;
        Self::en_us()
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
        Self::create(code.clone(), code, 2)
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
    let _ = (text, locale, currency);
    Err(MoneyInputError::Invalid)
}

/// `amount` minor units of a currency with `minor_digits` decimals, written
/// in `locale`: grouped, the decimal point and every decimal, a leading `-`
/// for a negative amount (`-1.234,50`). With `symbol`, the currency on the
/// locale's side of it, spaced by a no-break space where the locale spaces
/// it (`1.234,50 €`, `$1,234.50`).
pub(crate) fn format_money(
    amount: i64,
    locale: &MoneyLocale,
    minor_digits: u8,
    symbol: Option<&str>,
) -> String {
    let _ = (amount, locale, minor_digits, symbol);
    String::new()
}

/// `amount` against the state's rules: `Negative` when negatives are off,
/// `BelowMin` / `AboveMax` outside the bounds, `None` otherwise.
pub(crate) fn check_amount(amount: i64, state: &MoneyInputState) -> MoneyInputError {
    let _ = (amount, state);
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
    let _ = (text, locale, currency);
    MoneyEdit {
        accepted: true,
        state,
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
