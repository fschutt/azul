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
