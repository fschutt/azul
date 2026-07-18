//! ICU4X-based internationalization support for Azul
//!
//! This module provides locale-aware formatting for:
//! - Numbers (decimal, currency, percentages)
//! - Dates and times
//! - Lists (and, or, unit)
//! - Plural rules
//!
//! The `IcuLocalizer` is initialized with the system locale at startup,
//! but can be overridden. It can optionally load additional locale data
//! from binary blob files at runtime.
//!
//! # Example
//!
//! ```rust,ignore
//! // In a callback:
//! let localizer = info.get_localizer();
//! let formatted = localizer.format_decimal(1234567);
//! // Returns "1,234,567" for en-US or "1.234.567" for de-DE
//! ```

use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Mutex;

use azul_css::AzString;

// ICU4X-only imports (not used in the macOS Foundation backend)
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
use icu::collator::{Collator, options::CollatorOptions};
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
use icu::decimal::input::Decimal;
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
use icu::decimal::DecimalFormatter;
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
use icu::list::{ListFormatter, options::ListFormatterOptions};
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
use icu::locale::Locale;
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
use icu::plurals::PluralRules;
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
use writeable::Writeable;

// macOS Foundation backend
#[cfg(all(target_os = "macos", feature = "icu_macos"))]
#[path = "icu_macos.rs"]
mod icu_macos;

// Windows NLS backend
#[cfg(all(target_os = "windows", feature = "icu_windows"))]
#[path = "icu_windows.rs"]
mod icu_windows;


// Re-export ICU4X locale/plural types (only available with the ICU4X backend)
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
pub use icu::locale::locale;
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
pub use icu::plurals::{PluralCategory as IcuPluralCategory, PluralRules as IcuPluralRules};

/// Error type for ICU operations
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct IcuError {
    pub message: AzString,
}

impl IcuError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: AzString::from(msg.into()),
        }
    }
}

/// Result type for ICU operations
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum IcuResult {
    Ok(AzString),
    Err(IcuError),
}

impl IcuResult {
    pub fn ok(s: impl Into<String>) -> Self {
        IcuResult::Ok(AzString::from(s.into()))
    }

    pub fn err(msg: impl Into<String>) -> Self {
        IcuResult::Err(IcuError::new(msg))
    }

    pub fn into_option(self) -> Option<AzString> {
        match self {
            IcuResult::Ok(s) => Some(s),
            IcuResult::Err(_) => None,
        }
    }

    pub fn unwrap_or(self, default: AzString) -> AzString {
        match self {
            IcuResult::Ok(s) => s,
            IcuResult::Err(_) => default,
        }
    }
}

/// The plural category for a number (used for translations)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
    #[default]
    Other,
}

#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
impl From<IcuPluralCategory> for PluralCategory {
    fn from(cat: IcuPluralCategory) -> Self {
        match cat {
            IcuPluralCategory::Zero => PluralCategory::Zero,
            IcuPluralCategory::One => PluralCategory::One,
            IcuPluralCategory::Two => PluralCategory::Two,
            IcuPluralCategory::Few => PluralCategory::Few,
            IcuPluralCategory::Many => PluralCategory::Many,
            IcuPluralCategory::Other => PluralCategory::Other,
        }
    }
}

// ─── Shared helpers for macOS / Windows backends ─────────────────────────────

/// Build a decimal string from `integer_part × 10^(-decimal_places)` without
/// going through `f64`, avoiding precision loss for large `i64` values.
#[cfg(any(
    all(target_os = "macos", feature = "icu_macos"),
    all(target_os = "windows", feature = "icu_windows"),
))]
pub(crate) fn decimal_string(integer_part: i64, decimal_places: i16) -> alloc::string::String {
    use alloc::string::String;

    if decimal_places <= 0 {
        let mut s = integer_part.to_string();
        // `-decimal_places` overflows i16 for `decimal_places == i16::MIN`
        // (`-i16::MIN` is not representable). Compute the zero count via
        // `unsigned_abs` in a wider type so the full i16 range is safe.
        let zeros = i32::from(decimal_places).unsigned_abs() as usize;
        for _ in 0..zeros {
            s.push('0');
        }
        return s;
    }

    let dp = decimal_places as usize;
    let negative = integer_part < 0;
    let abs_val = (integer_part as i128).unsigned_abs();
    let abs_str = alloc::format!("{abs_val}");

    let body = if abs_str.len() <= dp {
        let mut s = String::from("0.");
        for _ in 0..(dp - abs_str.len()) {
            s.push('0');
        }
        s.push_str(&abs_str);
        s
    } else {
        let split = abs_str.len() - dp;
        let mut s = String::new();
        s.push_str(&abs_str[..split]);
        s.push('.');
        s.push_str(&abs_str[split..]);
        s
    };

    if negative {
        alloc::format!("-{body}")
    } else {
        body
    }
}

// ─── CLDR plural rules ───────────────────────────────────────────────────────
//
// Shared between macOS and Windows backends. Covers the major plural-rule
// groups defined in CLDR without bundling any data file.

#[cfg(any(
    all(target_os = "macos", feature = "icu_macos"),
    all(target_os = "windows", feature = "icu_windows"),
))]
pub(crate) fn plural_for(n: i64, lang: &str) -> PluralCategory {
    let lang = lang.split(['-', '_']).next().unwrap_or(lang);
    match lang {
        // Arabic: six categories
        "ar" | "arz" | "ckb" => {
            let n100 = n.abs() % 100;
            if n == 0 {
                PluralCategory::Zero
            } else if n == 1 {
                PluralCategory::One
            } else if n == 2 {
                PluralCategory::Two
            } else if (3..=10).contains(&n100) {
                PluralCategory::Few
            } else if (11..=99).contains(&n100) {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Welsh: six categories
        "cy" => match n {
            0 => PluralCategory::Zero,
            1 => PluralCategory::One,
            2 => PluralCategory::Two,
            3 => PluralCategory::Few,
            6 => PluralCategory::Many,
            _ => PluralCategory::Other,
        },
        // East Slavic (Russian, Ukrainian, Belarusian, Serbian, Croatian, Bosnian)
        "ru" | "uk" | "be" | "sr" | "hr" | "bs" | "sh" => {
            let n10 = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n10 == 1 && n100 != 11 {
                PluralCategory::One
            } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Many
            }
        }
        // Polish
        "pl" => {
            let n10 = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n == 1 {
                PluralCategory::One
            } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Many
            }
        }
        // Czech, Slovak
        "cs" | "sk" => {
            if n == 1 {
                PluralCategory::One
            } else if (2..=4).contains(&n) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Slovenian
        "sl" => {
            let n100 = n.abs() % 100;
            if n100 == 1 {
                PluralCategory::One
            } else if n100 == 2 {
                PluralCategory::Two
            } else if (3..=4).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Lithuanian
        "lt" => {
            let n10 = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n10 == 1 && !(11..=19).contains(&n100) {
                PluralCategory::One
            } else if (2..=9).contains(&n10) && !(11..=19).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Latvian
        "lv" => {
            let n10 = n.abs() % 10;
            let n100 = n.abs() % 100;
            if n == 0 {
                PluralCategory::Zero
            } else if n10 == 1 && n100 != 11 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        // Romanian
        "ro" | "mo" => {
            let n100 = n.abs() % 100;
            if n == 1 {
                PluralCategory::One
            } else if n == 0 || (1..=19).contains(&n100) {
                PluralCategory::Few
            } else {
                PluralCategory::Other
            }
        }
        // Maltese
        "mt" => {
            let n100 = n.abs() % 100;
            if n == 1 {
                PluralCategory::One
            } else if n == 0 || (2..=10).contains(&n100) {
                PluralCategory::Few
            } else if (11..=19).contains(&n100) {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Hebrew / Yiddish
        "he" | "yi" | "iw" => {
            if n == 1 {
                PluralCategory::One
            } else if n == 2 {
                PluralCategory::Two
            } else if n != 0 && n % 10 == 0 {
                PluralCategory::Many
            } else {
                PluralCategory::Other
            }
        }
        // Irish (Gaelic)
        "ga" => match n {
            1 => PluralCategory::One,
            2 => PluralCategory::Two,
            3..=6 => PluralCategory::Few,
            7..=10 => PluralCategory::Many,
            _ => PluralCategory::Other,
        },
        // French, Kabyle: 0 and 1 are "one"
        "fr" | "ff" | "kab" => {
            if n == 0 || n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
        // Default: English-style (exactly 1 → one, everything else → other)
        _ => {
            if n == 1 {
                PluralCategory::One
            } else {
                PluralCategory::Other
            }
        }
    }
}

/// List formatting type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum ListType {
    /// "A, B, and C"
    And,
    /// "A, B, or C"
    Or,
    /// "A, B, C" (for units like "3 feet, 7 inches")
    Unit,
}

/// Date/time field set for formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum DateTimeFieldSet {
    /// Year, month, day (e.g., "January 15, 2025")
    YearMonthDay,
    /// Month and day only (e.g., "January 15")
    MonthDay,
    /// Year and month only (e.g., "January 2025")
    YearMonth,
    /// Hour and minute (e.g., "4:30 PM")
    HourMinute,
    /// Hour, minute, second (e.g., "4:30:45 PM")
    HourMinuteSecond,
    /// Full date and time
    Full,
}

/// Collation strength for string comparison
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(C)]
pub enum CollationStrength {
    /// Only primary differences (base letters) matter.
    /// e.g., "a" vs "b", but "a" == "A" and "a" == "à"
    Primary,
    /// Primary and secondary (accents) differences matter.
    /// e.g., "a" vs "à", but "a" == "A"
    Secondary,
    /// Primary, secondary, and tertiary (case) differences matter.
    /// e.g., "a" vs "A"
    #[default]
    Tertiary,
    /// All differences matter, including punctuation/whitespace.
    Quaternary,
}

/// Length/style for formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum FormatLength {
    /// Short format (e.g., "1/15/25")
    Short,
    /// Medium format (e.g., "Jan 15, 2025")
    Medium,
    /// Long format (e.g., "January 15, 2025")
    Long,
}

/// A simple date structure for ICU formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct IcuDate {
    pub year: i32,
    /// Month: 1-12
    pub month: u8,
    /// Day: 1-31
    pub day: u8,
}

/// A simple time structure for ICU formatting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct IcuTime {
    /// Hour: 0-23
    pub hour: u8,
    /// Minute: 0-59
    pub minute: u8,
    /// Second: 0-59
    pub second: u8,
}

/// A combined date and time structure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct IcuDateTime {
    pub date: IcuDate,
    pub time: IcuTime,
}

impl IcuDate {
    /// Create a new IcuDate from year, month, day.
    pub const fn new(year: i32, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }

    /// Get the current local date.
    #[cfg(feature = "icu_chrono")]
    pub fn now() -> Self {
        use chrono::Datelike;
        let now = chrono::Local::now();
        Self {
            year: now.year(),
            month: now.month() as u8,
            day: now.day() as u8,
        }
    }

    /// Get the current UTC date.
    #[cfg(feature = "icu_chrono")]
    pub fn now_utc() -> Self {
        use chrono::Datelike;
        let now = chrono::Utc::now();
        Self {
            year: now.year(),
            month: now.month() as u8,
            day: now.day() as u8,
        }
    }
}

impl IcuTime {
    /// Create a new IcuTime from hour, minute, second.
    pub const fn new(hour: u8, minute: u8, second: u8) -> Self {
        Self { hour, minute, second }
    }

    /// Get the current local time.
    #[cfg(feature = "icu_chrono")]
    pub fn now() -> Self {
        use chrono::Timelike;
        let now = chrono::Local::now();
        Self {
            hour: now.hour() as u8,
            minute: now.minute() as u8,
            second: now.second() as u8,
        }
    }

    /// Get the current UTC time.
    #[cfg(feature = "icu_chrono")]
    pub fn now_utc() -> Self {
        use chrono::Timelike;
        let now = chrono::Utc::now();
        Self {
            hour: now.hour() as u8,
            minute: now.minute() as u8,
            second: now.second() as u8,
        }
    }
}

impl IcuDateTime {
    /// Create a new IcuDateTime from date and time.
    pub const fn new(date: IcuDate, time: IcuTime) -> Self {
        Self { date, time }
    }

    /// Get the current local date and time.
    #[cfg(feature = "icu_chrono")]
    pub fn now() -> Self {
        Self {
            date: IcuDate::now(),
            time: IcuTime::now(),
        }
    }

    /// Get the current UTC date and time.
    #[cfg(feature = "icu_chrono")]
    pub fn now_utc() -> Self {
        Self {
            date: IcuDate::now_utc(),
            time: IcuTime::now_utc(),
        }
    }

    /// Get Unix timestamp in milliseconds (like JavaScript Date.now()).
    ///
    /// Returns the number of milliseconds since January 1, 1970 00:00:00 UTC.
    #[cfg(feature = "icu_chrono")]
    pub fn timestamp_now() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    /// Get Unix timestamp in seconds.
    #[cfg(feature = "icu_chrono")]
    pub fn timestamp_now_seconds() -> i64 {
        chrono::Utc::now().timestamp()
    }

    /// Convert a Unix timestamp (seconds) to IcuDateTime.
    #[cfg(feature = "icu_chrono")]
    pub fn from_timestamp(timestamp_secs: i64) -> Option<Self> {
        use chrono::{Datelike, TimeZone, Timelike};
        chrono::Utc.timestamp_opt(timestamp_secs, 0).single().map(|dt| {
            Self {
                date: IcuDate {
                    year: dt.year(),
                    month: dt.month() as u8,
                    day: dt.day() as u8,
                },
                time: IcuTime {
                    hour: dt.hour() as u8,
                    minute: dt.minute() as u8,
                    second: dt.second() as u8,
                },
            }
        })
    }

    /// Convert a Unix timestamp (milliseconds) to IcuDateTime.
    #[cfg(feature = "icu_chrono")]
    pub fn from_timestamp_millis(timestamp_millis: i64) -> Option<Self> {
        Self::from_timestamp(timestamp_millis / 1000)
    }
}

// ─── macOS Foundation backend ─────────────────────────────────────────────────
// When building for macOS with `icu_macos` feature, use Foundation formatters.
#[cfg(all(target_os = "macos", feature = "icu_macos"))]
pub use icu_macos::IcuLocalizer;

// ─── Windows NLS backend ──────────────────────────────────────────────────────
// When building for Windows with `icu_windows` feature, use Win32 NLS functions.
#[cfg(all(target_os = "windows", feature = "icu_windows"))]
pub use icu_windows::IcuLocalizer;

// ─── ICU4X backend ────────────────────────────────────────────────────────────
// Used on all other platforms (or when no OS-native backend is enabled).
#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
/// The main ICU localizer that holds formatters for the current locale.
///
/// This struct is thread-safe and can be shared across callbacks.
/// It lazily initializes formatters on first use.
pub struct IcuLocalizer {
    /// The current locale (BCP 47 format, e.g., "en-US", "de-DE")
    locale: Locale,
    /// The locale string for C API access
    locale_string: AzString,
    /// Optional binary data blob for additional locale data
    data_blob: Option<Vec<u8>>,
    /// Cached decimal formatter
    decimal_formatter: Option<DecimalFormatter>,
    /// Cached plural rules (cardinal)
    plural_rules_cardinal: Option<PluralRules>,
    /// Cached plural rules (ordinal)
    plural_rules_ordinal: Option<PluralRules>,
    /// Cached list formatter (and)
    list_formatter_and: Option<ListFormatter>,
    /// Cached list formatter (or)
    list_formatter_or: Option<ListFormatter>,
    /// Cached collator
    collator: Option<Collator>,
}

#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
impl core::fmt::Debug for IcuLocalizer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IcuLocalizer")
            .field("locale", &self.locale_string)
            .field("has_data_blob", &self.data_blob.is_some())
            .finish()
    }
}

#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
impl IcuLocalizer {
    /// Create a new localizer with the given locale string (BCP 47 format).
    ///
    /// # Arguments
    /// * `locale_str` - A BCP 47 locale string like "en-US", "de-DE", "ja-JP"
    ///
    /// # Returns
    /// A new IcuLocalizer, or falls back to "en-US" if parsing fails.
    pub fn new(locale_str: &str) -> Self {
        let locale = locale_str.parse::<Locale>().unwrap_or_else(|_| {
            // Fallback to en-US if parsing fails
            "en-US".parse().unwrap()
        });

        Self {
            locale_string: AzString::from(locale.to_string()),
            locale,
            data_blob: None,
            decimal_formatter: None,
            plural_rules_cardinal: None,
            plural_rules_ordinal: None,
            list_formatter_and: None,
            list_formatter_or: None,
            collator: None,
        }
    }

    /// Create a localizer from the system's detected language.
    ///
    /// Uses the language detected by `azul_css::system::detect_system_language()`.
    pub fn from_system_language(system_language: &AzString) -> Self {
        Self::new(system_language.as_str())
    }

    /// Load additional locale data from a binary blob.
    ///
    /// The blob should be generated using `icu4x-datagen` with the `--format blob` flag.
    /// This allows supporting locales that aren't compiled into the binary.
    pub fn load_data_blob(&mut self, data: Vec<u8>) {
        self.data_blob = Some(data);
        // Clear cached formatters so they'll be recreated with new data
        self.decimal_formatter = None;
        self.plural_rules_cardinal = None;
        self.plural_rules_ordinal = None;
        self.list_formatter_and = None;
        self.list_formatter_or = None;
        self.collator = None;
    }

    /// Get the current locale string (BCP 47 format).
    pub fn get_locale(&self) -> AzString {
        self.locale_string.clone()
    }

    /// Get the language part of the locale (e.g., "en" from "en-US").
    pub fn get_language(&self) -> AzString {
        AzString::from(self.locale.id.language.to_string())
    }

    /// Get the region/country part of the locale if present (e.g., "US" from "en-US").
    pub fn get_region(&self) -> Option<AzString> {
        self.locale.id.region.map(|r| AzString::from(r.to_string()))
    }

    /// Change the current locale.
    ///
    /// This clears all cached formatters.
    pub fn set_locale(&mut self, locale_str: &str) -> bool {
        match locale_str.parse::<Locale>() {
            Ok(locale) => {
                self.locale = locale;
                self.locale_string = AzString::from(locale_str.to_string());
                // Clear caches
                self.decimal_formatter = None;
                self.plural_rules_cardinal = None;
                self.plural_rules_ordinal = None;
                self.list_formatter_and = None;
                self.list_formatter_or = None;
                self.collator = None;
                true
            }
            Err(_) => false,
        }
    }

    // Number Formatting

    fn get_decimal_formatter(&mut self) -> &DecimalFormatter {
        if self.decimal_formatter.is_none() {
            // Try to create formatter, fall back to default locale if it fails
            let formatter = DecimalFormatter::try_new(self.locale.clone().into(), Default::default())
                .unwrap_or_else(|_| {
                    DecimalFormatter::try_new(Default::default(), Default::default())
                        .expect("default locale should always work")
                });
            self.decimal_formatter = Some(formatter);
        }
        self.decimal_formatter.as_ref().unwrap()
    }

    /// Format an integer with locale-appropriate grouping separators.
    ///
    /// # Example
    /// - en-US: 1234567 → "1,234,567"
    /// - de-DE: 1234567 → "1.234.567"
    /// - fr-FR: 1234567 → "1 234 567"
    pub fn format_integer(&mut self, value: i64) -> AzString {
        let decimal = Decimal::from(value);
        let formatter = self.get_decimal_formatter();
        let mut output = String::new();
        let _ = write!(output, "{}", formatter.format(&decimal));
        AzString::from(output)
    }

    /// Format a decimal number with locale-appropriate separators.
    ///
    /// # Arguments
    /// * `integer_part` - The integer part of the number
    /// * `decimal_places` - Number of decimal places (negative power of 10)
    ///
    /// # Example
    /// `format_decimal(123456, 2)` formats 1234.56
    /// - en-US: "1,234.56"
    /// - de-DE: "1.234,56"
    pub fn format_decimal(&mut self, integer_part: i64, decimal_places: i16) -> AzString {
        let mut decimal = Decimal::from(integer_part);
        decimal.multiply_pow10(-decimal_places);
        let formatter = self.get_decimal_formatter();
        let mut output = String::new();
        let _ = write!(output, "{}", formatter.format(&decimal));
        AzString::from(output)
    }

    // Plural Rules

    fn get_plural_rules_cardinal(&mut self) -> &PluralRules {
        if self.plural_rules_cardinal.is_none() {
            let rules = PluralRules::try_new(self.locale.clone().into(), Default::default())
                .unwrap_or_else(|_| {
                    PluralRules::try_new(Default::default(), Default::default())
                        .expect("default locale should always work")
                });
            self.plural_rules_cardinal = Some(rules);
        }
        self.plural_rules_cardinal.as_ref().unwrap()
    }

    /// Get the plural category for a number (cardinal: "1 item", "2 items").
    ///
    /// This is essential for proper pluralization in translations.
    ///
    /// # Example
    /// - English: 1 → One, 2 → Other
    /// - Polish: 1 → One, 2 → Few, 5 → Many
    /// - Arabic: 0 → Zero, 1 → One, 2 → Two, 3-10 → Few, 11-99 → Many
    pub fn get_plural_category(&mut self, value: i64) -> PluralCategory {
        let rules = self.get_plural_rules_cardinal();
        let abs_value = value.unsigned_abs() as usize;
        rules.category_for(abs_value).into()
    }

    /// Select the appropriate string based on plural category.
    ///
    /// # Arguments
    /// * `value` - The number to pluralize
    /// * `zero` - String for zero (if language supports it, otherwise uses `other`)
    /// * `one` - String for one
    /// * `two` - String for two (if language supports it, otherwise uses `other`)
    /// * `few` - String for few (if language supports it, otherwise uses `other`)
    /// * `many` - String for many (if language supports it, otherwise uses `other`)
    /// * `other` - String for other (fallback)
    ///
    /// # Example
    /// ```rust,ignore
    /// let msg = localizer.pluralize(
    ///     count,
    ///     "no items",    // zero
    ///     "1 item",      // one
    ///     "2 items",     // two
    ///     "{} items",    // few
    ///     "{} items",    // many
    ///     "{} items",    // other
    /// );
    /// ```
    pub fn pluralize(
        &mut self,
        value: i64,
        zero: &str,
        one: &str,
        two: &str,
        few: &str,
        many: &str,
        other: &str,
    ) -> AzString {
        let category = self.get_plural_category(value);
        let template = match category {
            PluralCategory::Zero => zero,
            PluralCategory::One => one,
            PluralCategory::Two => two,
            PluralCategory::Few => few,
            PluralCategory::Many => many,
            PluralCategory::Other => other,
        };
        // Replace {} placeholder with the actual value
        let result = template.replace("{}", &value.to_string());
        AzString::from(result)
    }

    // List Formatting

    fn get_list_formatter_and(&mut self) -> &ListFormatter {
        if self.list_formatter_and.is_none() {
            let formatter = ListFormatter::try_new_and(
                self.locale.clone().into(),
                ListFormatterOptions::default(),
            )
            .unwrap_or_else(|_| {
                ListFormatter::try_new_and(Default::default(), ListFormatterOptions::default())
                    .expect("default locale should always work")
            });
            self.list_formatter_and = Some(formatter);
        }
        self.list_formatter_and.as_ref().unwrap()
    }

    fn get_list_formatter_or(&mut self) -> &ListFormatter {
        if self.list_formatter_or.is_none() {
            let formatter = ListFormatter::try_new_or(
                self.locale.clone().into(),
                ListFormatterOptions::default(),
            )
            .unwrap_or_else(|_| {
                ListFormatter::try_new_or(Default::default(), ListFormatterOptions::default())
                    .expect("default locale should always work")
            });
            self.list_formatter_or = Some(formatter);
        }
        self.list_formatter_or.as_ref().unwrap()
    }

    /// Format a list of items with locale-appropriate conjunctions.
    ///
    /// # Arguments
    /// 
    /// * `items` - The items to format
    /// * `list_type` - The type of list (And, Or, Unit)
    ///
    /// # Example
    /// 
    /// - en-US And: ["A", "B", "C"] → "A, B, and C"
    /// - es-ES And: ["España", "Suiza", "Italia"] → "España, Suiza e Italia"
    /// - en-US Or: ["A", "B", "C"] → "A, B, or C"
    pub fn format_list(&mut self, items: &[AzString], list_type: ListType) -> AzString {
        let str_items: Vec<&str> = items.iter().map(|s| s.as_str()).collect();

        let formatted = match list_type {
            ListType::And => {
                let formatter = self.get_list_formatter_and();
                formatter.format(str_items.iter().copied())
            }
            ListType::Or => {
                let formatter = self.get_list_formatter_or();
                formatter.format(str_items.iter().copied())
            }
            ListType::Unit => {
                // Unit formatting uses comma separation without conjunction
                // For now, fall back to simple comma join
                // TODO: Use ListFormatter::try_new_unit when available
                return AzString::from(str_items.join(", "));
            }
        };

        let mut output = String::new();
        let _ = write!(output, "{}", formatted);
        AzString::from(output)
    }

    // Date/Time Formatting

    /// Format a date according to the current locale.
    ///
    /// # Arguments
    /// 
    /// * `date` - The date to format
    /// * `length` - The format length (Short, Medium, Long)
    ///
    /// # Example
    /// 
    /// For January 15, 2025:
    /// 
    /// - en-US Short: "1/15/25"
    /// - en-US Medium: "Jan 15, 2025"
    /// - en-US Long: "January 15, 2025"
    /// - de-DE Short: "15.01.25"
    /// - de-DE Medium: "15.01.2025"
    /// - de-DE Long: "15. Januar 2025"
    pub fn format_date(&mut self, date: IcuDate, length: FormatLength) -> IcuResult {
        use icu::datetime::fieldsets::YMD;
        use icu::datetime::input::Date;
        use icu::datetime::DateTimeFormatter;

        let icu_date = match Date::try_new_iso(date.year, date.month, date.day) {
            Ok(d) => d,
            Err(e) => return IcuResult::err(format!("Invalid date: {}", e)),
        };

        let field_set = match length {
            FormatLength::Short => YMD::short(),
            FormatLength::Medium => YMD::medium(),
            FormatLength::Long => YMD::long(),
        };

        let formatter = match DateTimeFormatter::try_new(self.locale.clone().into(), field_set) {
            Ok(f) => f,
            Err(e) => return IcuResult::err(format!("Failed to create formatter: {:?}", e)),
        };

        let mut output = String::new();
        let _ = write!(output, "{}", formatter.format(&icu_date));
        IcuResult::ok(output)
    }

    /// Format a time according to the current locale.
    ///
    /// # Example
    /// 
    /// For 16:30:45:
    /// 
    /// - en-US: "4:30 PM" or "4:30:45 PM"
    /// - de-DE: "16:30" or "16:30:45"
    pub fn format_time(&mut self, time: IcuTime, include_seconds: bool) -> IcuResult {
        use icu::datetime::fieldsets;
        use icu::datetime::input::Time;
        use icu::datetime::NoCalendarFormatter;

        let icu_time = match Time::try_new(time.hour, time.minute, time.second, 0) {
            Ok(t) => t,
            Err(e) => return IcuResult::err(format!("Invalid time: {}", e)),
        };

        let mut output = String::new();

        if include_seconds {
            let formatter: NoCalendarFormatter<fieldsets::T> =
                match NoCalendarFormatter::try_new(self.locale.clone().into(), fieldsets::T::medium()) {
                    Ok(f) => f,
                    Err(e) => return IcuResult::err(format!("Failed to create formatter: {:?}", e)),
                };
            let _ = write!(output, "{}", formatter.format(&icu_time));
        } else {
            let formatter: NoCalendarFormatter<fieldsets::T> =
                match NoCalendarFormatter::try_new(self.locale.clone().into(), fieldsets::T::short()) {
                    Ok(f) => f,
                    Err(e) => return IcuResult::err(format!("Failed to create formatter: {:?}", e)),
                };
            let _ = write!(output, "{}", formatter.format(&icu_time));
        }

        IcuResult::ok(output)
    }

    /// Format a date and time according to the current locale.
    pub fn format_datetime(&mut self, datetime: IcuDateTime, length: FormatLength) -> IcuResult {
        use icu::datetime::fieldsets::YMD;
        use icu::datetime::input::{Date, DateTime, Time};
        use icu::datetime::DateTimeFormatter;

        let icu_date = match Date::try_new_iso(datetime.date.year, datetime.date.month, datetime.date.day) {
            Ok(d) => d,
            Err(e) => return IcuResult::err(format!("Invalid date: {}", e)),
        };

        let icu_time = match Time::try_new(datetime.time.hour, datetime.time.minute, datetime.time.second, 0) {
            Ok(t) => t,
            Err(e) => return IcuResult::err(format!("Invalid time: {}", e)),
        };

        let icu_datetime = DateTime {
            date: icu_date,
            time: icu_time,
        };

        let field_set = match length {
            FormatLength::Short => YMD::short().with_time_hm(),
            FormatLength::Medium => YMD::medium().with_time_hm(),
            FormatLength::Long => YMD::long().with_time_hm(),
        };

        let formatter = match DateTimeFormatter::try_new(self.locale.clone().into(), field_set) {
            Ok(f) => f,
            Err(e) => return IcuResult::err(format!("Failed to create formatter: {:?}", e)),
        };

        let mut output = String::new();
        let _ = write!(output, "{}", formatter.format(&icu_datetime));
        IcuResult::ok(output)
    }

    // Collation (locale-aware string sorting)

    fn get_collator(&mut self) -> &Collator {
        if self.collator.is_none() {
            // try_new returns CollatorBorrowed<'static>, convert to owned
            let collator = Collator::try_new(self.locale.clone().into(), CollatorOptions::default())
                .map(|borrowed| borrowed.static_to_owned())
                .unwrap_or_else(|_| {
                    Collator::try_new(Default::default(), CollatorOptions::default())
                        .map(|borrowed| borrowed.static_to_owned())
                        .expect("default locale should always work")
                });
            self.collator = Some(collator);
        }
        self.collator.as_ref().unwrap()
    }

    /// Compare two strings according to locale-specific collation rules.
    ///
    /// Returns:
    /// - `Ordering::Less` if `a` comes before `b`
    /// - `Ordering::Equal` if `a` equals `b`
    /// - `Ordering::Greater` if `a` comes after `b`
    ///
    /// # Example
    /// ```rust,ignore
    /// let mut localizer = IcuLocalizer::new("es-ES");
    /// // Spanish: "ch" was historically treated as a single letter after "c"
    /// // (though modern Spanish may differ)
    /// let cmp = localizer.compare("coche", "cena");
    /// ```
    pub fn compare(&mut self, a: &str, b: &str) -> core::cmp::Ordering {
        self.get_collator().as_borrowed().compare(a, b)
    }

    /// Sort a vector of strings in place using locale-aware collation.
    ///
    /// This properly handles accented characters, case sensitivity, and
    /// language-specific sorting rules.
    ///
    /// # Example
    /// ```rust,ignore
    /// let mut localizer = IcuLocalizer::new("de-DE");
    /// let mut names = vec!["Österreich", "Andorra", "Ägypten"];
    /// localizer.sort_strings(&mut names);
    /// // German sorts Ä with A, Ö with O
    /// ```
    pub fn sort_strings(&mut self, strings: &mut [AzString]) {
        let collator = self.get_collator().as_borrowed();
        strings.sort_by(|a, b| collator.compare(a.as_str(), b.as_str()));
    }

    /// Sort a vector of strings and return a new sorted vector.
    pub fn sorted_strings(&mut self, strings: &[AzString]) -> Vec<AzString> {
        let mut result: Vec<AzString> = strings.to_vec();
        self.sort_strings(&mut result);
        result
    }

    /// Check if two strings are equal according to locale collation rules.
    ///
    /// This may return `true` for strings that differ in case or accents,
    /// depending on the collation strength.
    pub fn strings_equal(&mut self, a: &str, b: &str) -> bool {
        self.compare(a, b) == core::cmp::Ordering::Equal
    }

    /// Get the sort key for a string.
    ///
    /// Sort keys can be compared byte-by-byte for fast sorting of many strings.
    /// This is more efficient when sorting large collections.
    pub fn get_sort_key(&mut self, s: &str) -> Vec<u8> {
        let collator = self.get_collator().as_borrowed();
        let mut key = Vec::new();
        let _ = collator.write_sort_key_to(s, &mut key);
        key
    }
}

#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
impl Default for IcuLocalizer {
    fn default() -> Self {
        Self::new("en-US")
    }
}

#[cfg(all(feature = "icu", not(all(target_os = "macos", feature = "icu_macos")), not(all(target_os = "windows", feature = "icu_windows"))))]
impl Clone for IcuLocalizer {
    fn clone(&self) -> Self {
        // Clone without cached formatters (they'll be recreated on demand)
        Self {
            locale: self.locale.clone(),
            locale_string: self.locale_string.clone(),
            data_blob: self.data_blob.clone(),
            decimal_formatter: None,
            plural_rules_cardinal: None,
            plural_rules_ordinal: None,
            list_formatter_and: None,
            list_formatter_or: None,
            collator: None,
        }
    }
}

// Thread-safe wrapper for use in callbacks

/// Inner data for `IcuLocalizerHandle` — contains the locale cache and default locale.
///
/// This is heap-allocated and shared via manual reference counting in
/// `IcuLocalizerHandle` for FFI compatibility.
pub struct IcuLocalizerInner {
    cache: Mutex<BTreeMap<String, IcuLocalizer>>,
    /// Default locale to use when none is specified
    default_locale: Mutex<AzString>,
    /// Stored data blob for creating new localizers with custom data
    data_blob: Mutex<Option<Vec<u8>>>,
}

/// A thread-safe cache of ICU localizers for multiple locales.
///
/// This is passed to callbacks via `CallbackInfo` and `LayoutCallbackInfo`.
/// It uses manual reference counting (raw pointer + AtomicUsize) for FFI safety.
///
/// Each locale's IcuLocalizer is lazily created and cached on first use.
/// All methods take a `locale: &str` parameter to specify which locale to use.
#[repr(C)]
pub struct IcuLocalizerHandle {
    pub ptr: *const IcuLocalizerInner,
    pub copies: *const AtomicUsize,
    pub run_destructor: bool,
}

unsafe impl Send for IcuLocalizerHandle {}
unsafe impl Sync for IcuLocalizerHandle {}

impl Clone for IcuLocalizerHandle {
    fn clone(&self) -> Self {
        unsafe {
            self.copies
                .as_ref()
                .map(|m| m.fetch_add(1, AtomicOrdering::SeqCst));
        }
        Self {
            ptr: self.ptr,
            copies: self.copies,
            run_destructor: true,
        }
    }
}

impl Drop for IcuLocalizerHandle {
    fn drop(&mut self) {
        // Honor `run_destructor`. When this `#[repr(C)]` handle is moved across
        // the FFI boundary the codegen sets `run_destructor = false` on the
        // moved-from (bitwise) copy, so a C-side by-value copy can never run the
        // refcount decrement twice and double-free. Previously this impl ignored
        // the flag and freed unconditionally — that was the double-free hazard.
        // This now matches the canonical azul refcount pattern (`RefCount::drop`).
        //
        // NOTE: the field layout (ptr / copies / run_destructor) is pinned in
        // api.json (`IcuLocalizerHandle.struct_fields`), so the manual refcount
        // cannot be swapped for a plain `Arc` without an FFI ABI break + codegen
        // regen; honoring the flag is the ABI-safe equivalent fix.
        if !self.run_destructor || self.copies.is_null() {
            return;
        }
        unsafe {
            let copies = (*self.copies).fetch_sub(1, AtomicOrdering::SeqCst);
            if copies == 1 {
                let _ = Box::from_raw(self.ptr as *mut IcuLocalizerInner);
                let _ = Box::from_raw(self.copies as *mut AtomicUsize);
            }
        }
    }
}

impl core::fmt::Debug for IcuLocalizerHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let inner = unsafe { &*self.ptr };
        let default_locale = inner.default_locale.lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| AzString::from(""));
        f.debug_struct("IcuLocalizerHandle")
            .field("default_locale", &default_locale)
            .finish()
    }
}

impl Default for IcuLocalizerHandle {
    fn default() -> Self {
        Self {
            ptr: Box::into_raw(Box::new(IcuLocalizerInner {
                cache: Mutex::new(BTreeMap::new()),
                default_locale: Mutex::new(AzString::from("en-US")),
                data_blob: Mutex::new(None),
            })),
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
        }
    }
}

impl IcuLocalizerHandle {
    /// Create a new empty cache with a default locale.
    pub fn new(default_locale: &str) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(IcuLocalizerInner {
                cache: Mutex::new(BTreeMap::new()),
                default_locale: Mutex::new(AzString::from(default_locale)),
                data_blob: Mutex::new(None),
            })),
            copies: Box::into_raw(Box::new(AtomicUsize::new(1))),
            run_destructor: true,
        }
    }

    /// Get a reference to the inner data.
    #[inline]
    fn inner(&self) -> &IcuLocalizerInner {
        unsafe { &*self.ptr }
    }

    /// Create a cache initialized with the system language.
    pub fn from_system_language(language: &AzString) -> Self {
        Self::new(language.as_str())
    }

    /// Get the default locale string.
    pub fn get_default_locale(&self) -> AzString {
        self.inner().default_locale.lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| AzString::from("en-US"))
    }

    /// Set the default locale.
    pub fn set_default_locale(&mut self, locale: &str) {
        if let Ok(mut guard) = self.inner().default_locale.lock() {
            *guard = AzString::from(locale);
        }
    }

    /// Alias for set_default_locale for compatibility.
    pub fn set_locale(&mut self, locale: &str) {
        self.set_default_locale(locale);
    }

    /// Load additional locale data from a binary blob for all cached localizers.
    ///
    /// The blob should be generated using `icu4x-datagen` with the `--format blob` flag.
    /// This allows supporting locales that aren't compiled into the binary.
    ///
    /// Returns `true` if the data was successfully loaded.
    pub fn load_data_blob(&self, data: &[u8]) -> bool {
        let inner = self.inner();
        if let Ok(mut blob) = inner.data_blob.lock() {
            *blob = Some(data.to_vec());
            if let Ok(mut cache) = inner.cache.lock() {
                for localizer in cache.values_mut() {
                    localizer.load_data_blob(data.to_vec());
                }
            }
            true
        } else {
            false
        }
    }

    /// Get or create a localizer for the given locale, returning `fallback()` if
    /// the cache lock is poisoned.
    /// This is an internal helper that handles cache access.
    fn with_localizer_or<F, R, E>(&self, locale: &str, f: F, fallback: E) -> R
    where
        F: FnOnce(&mut IcuLocalizer) -> R,
        E: FnOnce() -> R,
    {
        let inner = self.inner();
        let blob = inner.data_blob.lock().ok().and_then(|b| b.clone());
        inner.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| {
                        let mut l = IcuLocalizer::new(locale);
                        if let Some(data) = blob {
                            l.load_data_blob(data);
                        }
                        l
                    });
                f(localizer)
            })
            .unwrap_or_else(|_| fallback())
    }

    /// Get or create a localizer for the given locale.
    /// This is an internal helper that handles cache access.
    fn with_localizer<F, R>(&self, locale: &str, f: F) -> R
    where
        F: FnOnce(&mut IcuLocalizer) -> R,
        R: Default,
    {
        self.with_localizer_or(locale, f, R::default)
    }

    /// Get the language part of a locale (e.g., "en" from "en-US").
    pub fn get_language(&self, locale: &str) -> AzString {
        self.with_localizer(locale, |l| l.get_language())
    }

    /// Format an integer with locale-appropriate grouping.
    ///
    /// # Example
    /// ```rust,ignore
    /// cache.format_integer("en-US", 1234567) // → "1,234,567"
    /// cache.format_integer("de-DE", 1234567) // → "1.234.567"
    /// ```
    pub fn format_integer(&self, locale: &str, value: i64) -> AzString {
        self.with_localizer(locale, |l| l.format_integer(value))
    }

    /// Format a decimal number.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string (e.g., "en-US", "de-DE")
    /// * `integer_part` - The full integer value (e.g., 123456 for 1234.56)
    /// * `decimal_places` - Number of decimal places (e.g., 2 for 1234.56)
    pub fn format_decimal(&self, locale: &str, integer_part: i64, decimal_places: i16) -> AzString {
        self.with_localizer(locale, |l| l.format_decimal(integer_part, decimal_places))
    }

    /// Get the plural category for a number.
    ///
    /// # Example
    /// ```rust,ignore
    /// cache.get_plural_category("en", 1)  // → PluralCategory::One
    /// cache.get_plural_category("pl", 5)  // → PluralCategory::Many
    /// ```
    pub fn get_plural_category(&self, locale: &str, value: i64) -> PluralCategory {
        self.with_localizer(locale, |l| l.get_plural_category(value))
    }

    /// Select a string based on plural rules.
    ///
    /// # Arguments
    /// * `locale` - BCP 47 locale string
    /// * `value` - The number to pluralize
    /// * `zero`, `one`, `two`, `few`, `many`, `other` - Strings for each category
    pub fn pluralize(
        &self,
        locale: &str,
        value: i64,
        zero: &str,
        one: &str,
        two: &str,
        few: &str,
        many: &str,
        other: &str,
    ) -> AzString {
        self.with_localizer_or(
            locale,
            |l| l.pluralize(value, zero, one, two, few, many, other),
            || AzString::from(other),
        )
    }

    /// Format a list of items with locale-appropriate conjunctions.
    ///
    /// # Example
    /// ```rust,ignore
    /// cache.format_list("en-US", &items, ListType::And) // → "A, B, and C"
    /// cache.format_list("de-DE", &items, ListType::And) // → "A, B und C"
    /// ```
    pub fn format_list(&self, locale: &str, items: &[AzString], list_type: ListType) -> AzString {
        self.with_localizer_or(
            locale,
            |l| l.format_list(items, list_type),
            || {
                let strs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
                AzString::from(strs.join(", "))
            },
        )
    }

    /// Format a date according to the specified locale.
    ///
    /// # Example
    /// ```rust,ignore
    /// let today = IcuDate::now();
    /// cache.format_date("en-US", today, FormatLength::Medium) // → "Jan 15, 2025"
    /// cache.format_date("de-DE", today, FormatLength::Medium) // → "15.01.2025"
    /// ```
    pub fn format_date(&self, locale: &str, date: IcuDate, length: FormatLength) -> IcuResult {
        self.with_localizer_or(
            locale,
            |l| l.format_date(date, length),
            || IcuResult::err("Lock error".to_string()),
        )
    }

    /// Format a time according to the specified locale.
    ///
    /// # Example
    /// ```rust,ignore
    /// let now = IcuTime::now();
    /// cache.format_time("en-US", now, false) // → "4:30 PM"
    /// cache.format_time("de-DE", now, false) // → "16:30"
    /// ```
    pub fn format_time(&self, locale: &str, time: IcuTime, include_seconds: bool) -> IcuResult {
        self.with_localizer_or(
            locale,
            |l| l.format_time(time, include_seconds),
            || IcuResult::err("Lock error".to_string()),
        )
    }

    /// Format a date and time according to the specified locale.
    pub fn format_datetime(&self, locale: &str, datetime: IcuDateTime, length: FormatLength) -> IcuResult {
        self.with_localizer_or(
            locale,
            |l| l.format_datetime(datetime, length),
            || IcuResult::err("Lock error".to_string()),
        )
    }

    // =========================================================================
    // Collation (locale-aware string comparison and sorting)
    // =========================================================================

    /// Compare two strings according to locale-specific collation rules.
    ///
    /// Returns -1 if a < b, 0 if a == b, 1 if a > b.
    ///
    /// # Example
    /// ```rust,ignore
    /// cache.compare_strings("de-DE", "Äpfel", "Banane") // → -1 (Ä sorts with A)
    /// cache.compare_strings("sv-SE", "Äpple", "Öl")     // → -1 (Swedish: Ä before Ö)
    /// ```
    pub fn compare_strings(&self, locale: &str, a: &str, b: &str) -> i32 {
        self.with_localizer(locale, |l| {
            match l.compare(a, b) {
                core::cmp::Ordering::Less => -1,
                core::cmp::Ordering::Equal => 0,
                core::cmp::Ordering::Greater => 1,
            }
        })
    }

    /// Sort a vector of strings using locale-aware collation.
    ///
    /// Returns a new sorted vector.
    ///
    /// # Example
    /// ```rust,ignore
    /// let sorted = cache.sort_strings("de-DE", &["Österreich", "Andorra", "Ägypten"]);
    /// // Result: ["Ägypten", "Andorra", "Österreich"] (Ä sorts with A, Ö with O)
    /// ```
    pub fn sort_strings(&self, locale: &str, strings: &[AzString]) -> IcuStringVec {
        self.with_localizer_or(
            locale,
            |l| IcuStringVec::from(l.sorted_strings(strings)),
            || IcuStringVec::from(strings.to_vec()),
        )
    }

    /// Check if two strings are equal according to locale collation rules.
    pub fn strings_equal(&self, locale: &str, a: &str, b: &str) -> bool {
        self.with_localizer_or(
            locale,
            |l| l.strings_equal(a, b),
            || a == b,
        )
    }

    /// Get the sort key for a string (for efficient bulk sorting).
    pub fn get_sort_key(&self, locale: &str, s: &str) -> Vec<u8> {
        self.with_localizer(locale, |l| l.get_sort_key(s))
    }

    /// Convenience function to format a localized message with plural support.
    ///
    /// This handles the common case of "{count} {item/items}" patterns.
    /// The `{}` placeholder in the template will be replaced with the formatted number.
    pub fn format_plural(&self, locale: &str, value: i64, zero: &str, one: &str, other: &str) -> AzString {
        let template = self.pluralize(locale, value, zero, one, other, other, other, other);
        let formatted_num = self.format_integer(locale, value);
        AzString::from(template.as_str().replace("{}", formatted_num.as_str()))
    }

    /// Format a list of strings conveniently.
    pub fn format_list_strings(&self, locale: &str, items: &[&str], list_type: ListType) -> AzString {
        let az_items: Vec<AzString> = items.iter().map(|s| AzString::from(*s)).collect();
        self.format_list(locale, &az_items, list_type)
    }

    /// Clear the cache (useful for memory management or locale data reload).
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.inner().cache.lock() {
            cache.clear();
        }
    }

    /// Get the number of cached locales.
    pub fn cached_locale_count(&self) -> usize {
        self.inner().cache
            .lock()
            .map(|cache| cache.len())
            .unwrap_or(0)
    }

    /// Get a list of all cached locale strings.
    pub fn cached_locales(&self) -> Vec<AzString> {
        self.inner().cache
            .lock()
            .map(|cache| cache.keys().map(|k| AzString::from(k.clone())).collect())
            .unwrap_or_default()
    }
}


// C-compatible Vec types for FFI

// OptionAzString is the same as OptionString from azul_css
pub type OptionAzString = azul_css::OptionString;

azul_css::impl_vec!(AzString, IcuStringVec, IcuStringVecDestructor, IcuStringVecDestructorType, IcuStringVecSlice, OptionAzString);
azul_css::impl_vec_clone!(AzString, IcuStringVec, IcuStringVecDestructor);
azul_css::impl_vec_debug!(AzString, IcuStringVec);

// ============================================================================
// Extension trait for LayoutCallbackInfo (from azul-core)
// ============================================================================

use azul_core::callbacks::LayoutCallbackInfo;

/// Extension trait to add ICU internationalization methods to LayoutCallbackInfo.
///
/// This trait is implemented for `LayoutCallbackInfo` when the `icu` feature is enabled.
/// Import this trait to use ICU methods on LayoutCallbackInfo in layout callbacks.
///
/// # Example
/// ```rust,ignore
/// use azul_layout::icu::LayoutCallbackInfoIcuExt;
///
/// fn my_layout(info: LayoutCallbackInfo) -> StyledDom {
///     let formatted = info.icu_format_integer(1234567);
///     // ...
/// }
/// ```
pub trait LayoutCallbackInfoIcuExt {
    /// Get the current locale string (BCP 47 format, e.g., "en-US", "de-DE").
    fn icu_get_locale(&self) -> AzString;

    /// Get the current language (e.g., "en" from "en-US").
    fn icu_get_language(&self) -> AzString;

    /// Format an integer with locale-appropriate grouping separators.
    fn icu_format_integer(&self, value: i64) -> AzString;

    /// Format a decimal number with locale-appropriate separators.
    fn icu_format_decimal(&self, integer_part: i64, decimal_places: i16) -> AzString;

    /// Get the plural category for a number.
    fn icu_get_plural_category(&self, value: i64) -> PluralCategory;

    /// Select a string based on plural rules.
    fn icu_pluralize(
        &self,
        value: i64,
        zero: &str,
        one: &str,
        two: &str,
        few: &str,
        many: &str,
        other: &str,
    ) -> AzString;

    /// Format a list of items with locale-appropriate conjunctions.
    fn icu_format_list(&self, items: &[AzString], list_type: ListType) -> AzString;

    /// Format a date according to the current locale.
    fn icu_format_date(&self, date: IcuDate, length: FormatLength) -> IcuResult;

    /// Format a time according to the current locale.
    fn icu_format_time(&self, time: IcuTime, include_seconds: bool) -> IcuResult;

    /// Format a date and time according to the current locale.
    fn icu_format_datetime(&self, datetime: IcuDateTime, length: FormatLength) -> IcuResult;

    /// Compare two strings according to locale-specific collation rules.
    /// Returns -1 if a < b, 0 if a == b, 1 if a > b.
    fn icu_compare_strings(&self, a: &str, b: &str) -> i32;

    /// Sort a list of strings using locale-aware collation.
    fn icu_sort_strings(&self, strings: &[AzString]) -> IcuStringVec;

    /// Check if two strings are equal according to locale collation rules.
    fn icu_strings_equal(&self, a: &str, b: &str) -> bool;
}

/// Process-wide ICU localizer shared by all `LayoutCallbackInfo` ICU calls.
///
/// Every `icu_*` method below used to allocate a throwaway `IcuLocalizerHandle`
/// per call. Because the handle's per-locale formatter cache lives *inside* the
/// handle, that rebuilt every ICU formatter (decimal, plural, list, collator, …)
/// from scratch on every call — the cache never survived. Caching one shared
/// handle here means each locale's formatters are built once and reused across
/// all calls; the per-call locale is passed explicitly to each method, so a
/// single shared handle serves every locale.
fn shared_localizer_handle() -> &'static IcuLocalizerHandle {
    use std::sync::OnceLock;
    static SHARED: OnceLock<IcuLocalizerHandle> = OnceLock::new();
    SHARED.get_or_init(|| IcuLocalizerHandle::new("en-US"))
}

impl LayoutCallbackInfoIcuExt for LayoutCallbackInfo {
    fn icu_get_locale(&self) -> AzString {
        // The active locale is the system language itself (the previous
        // implementation round-tripped it through a fresh handle's default
        // locale, which returned the same string).
        self.get_system_style().language.clone()
    }

    fn icu_get_language(&self) -> AzString {
        let system_style = self.get_system_style();
        shared_localizer_handle().get_language(system_style.language.as_str())
    }

    fn icu_format_integer(&self, value: i64) -> AzString {
        let system_style = self.get_system_style();
        shared_localizer_handle().format_integer(system_style.language.as_str(), value)
    }

    fn icu_format_decimal(&self, integer_part: i64, decimal_places: i16) -> AzString {
        let system_style = self.get_system_style();
        shared_localizer_handle().format_decimal(
            system_style.language.as_str(),
            integer_part,
            decimal_places,
        )
    }

    fn icu_get_plural_category(&self, value: i64) -> PluralCategory {
        let system_style = self.get_system_style();
        shared_localizer_handle().get_plural_category(system_style.language.as_str(), value)
    }

    fn icu_pluralize(
        &self,
        value: i64,
        zero: &str,
        one: &str,
        two: &str,
        few: &str,
        many: &str,
        other: &str,
    ) -> AzString {
        let system_style = self.get_system_style();
        shared_localizer_handle().pluralize(
            system_style.language.as_str(),
            value,
            zero,
            one,
            two,
            few,
            many,
            other,
        )
    }

    fn icu_format_list(&self, items: &[AzString], list_type: ListType) -> AzString {
        let system_style = self.get_system_style();
        shared_localizer_handle().format_list(system_style.language.as_str(), items, list_type)
    }

    fn icu_format_date(&self, date: IcuDate, length: FormatLength) -> IcuResult {
        let system_style = self.get_system_style();
        shared_localizer_handle().format_date(system_style.language.as_str(), date, length)
    }

    fn icu_format_time(&self, time: IcuTime, include_seconds: bool) -> IcuResult {
        let system_style = self.get_system_style();
        shared_localizer_handle().format_time(
            system_style.language.as_str(),
            time,
            include_seconds,
        )
    }

    fn icu_format_datetime(&self, datetime: IcuDateTime, length: FormatLength) -> IcuResult {
        let system_style = self.get_system_style();
        shared_localizer_handle().format_datetime(system_style.language.as_str(), datetime, length)
    }

    fn icu_compare_strings(&self, a: &str, b: &str) -> i32 {
        let system_style = self.get_system_style();
        shared_localizer_handle().compare_strings(system_style.language.as_str(), a, b)
    }

    fn icu_sort_strings(&self, strings: &[AzString]) -> IcuStringVec {
        let system_style = self.get_system_style();
        shared_localizer_handle().sort_strings(system_style.language.as_str(), strings)
    }

    fn icu_strings_equal(&self, a: &str, b: &str) -> bool {
        let system_style = self.get_system_style();
        shared_localizer_handle().strings_equal(system_style.language.as_str(), a, b)
    }
}

// Tests

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_integer_en_us() {
        let mut localizer = IcuLocalizer::new("en-US");
        assert_eq!(localizer.format_integer(1234567).as_str(), "1,234,567");
    }

    #[test]
    fn test_format_integer_de_de() {
        let mut localizer = IcuLocalizer::new("de-DE");
        let result = localizer.format_integer(1234567);
        // German uses period as thousand separator
        assert!(result.as_str().contains('.') || result.as_str().contains('\u{a0}'));
    }

    #[test]
    fn test_plural_category_english() {
        let mut localizer = IcuLocalizer::new("en-US");
        assert_eq!(localizer.get_plural_category(1), PluralCategory::One);
        assert_eq!(localizer.get_plural_category(2), PluralCategory::Other);
        assert_eq!(localizer.get_plural_category(0), PluralCategory::Other);
    }

    #[test]
    fn test_format_list_and() {
        let mut localizer = IcuLocalizer::new("en-US");
        let items = vec![
            AzString::from("A"),
            AzString::from("B"),
            AzString::from("C"),
        ];
        let result = localizer.format_list(&items, ListType::And);
        assert!(result.as_str().contains("and"));
    }

    #[test]
    fn test_format_date() {
        let mut localizer = IcuLocalizer::new("en-US");
        let date = IcuDate {
            year: 2025,
            month: 1,
            day: 15,
        };
        let result = localizer.format_date(date, FormatLength::Medium);
        assert!(matches!(result, IcuResult::Ok(_)));
    }

    #[test]
    fn test_cache_thread_safety() {
        let cache = IcuLocalizerHandle::from_system_language(&AzString::from("en-US"));

        // Test that we can clone and use from multiple "threads" (simulated)
        let cache2 = cache.clone();

        assert_eq!(
            cache.format_integer("en-US", 1000).as_str(), 
            cache2.format_integer("en-US", 1000).as_str()
        );
    }

    #[test]
    fn test_cache_multi_locale() {
        let cache = IcuLocalizerHandle::default();

        // Format with different locales - each should be cached separately
        let en = cache.format_integer("en-US", 1234567);
        let de = cache.format_integer("de-DE", 1234567);
        
        // US uses comma, German uses period
        assert!(en.as_str().contains(','));
        assert!(de.as_str().contains('.') || de.as_str().contains('\u{a0}'));
    }

    #[test]
    fn test_collation_compare() {
        let mut localizer = IcuLocalizer::new("en-US");
        assert_eq!(localizer.compare("apple", "banana"), core::cmp::Ordering::Less);
        assert_eq!(localizer.compare("banana", "apple"), core::cmp::Ordering::Greater);
        assert_eq!(localizer.compare("apple", "apple"), core::cmp::Ordering::Equal);
    }

    #[test]
    fn test_collation_sort() {
        let mut localizer = IcuLocalizer::new("en-US");
        let mut strings = vec![
            AzString::from("cherry"),
            AzString::from("apple"),
            AzString::from("banana"),
        ];
        localizer.sort_strings(&mut strings);
        assert_eq!(strings[0].as_str(), "apple");
        assert_eq!(strings[1].as_str(), "banana");
        assert_eq!(strings[2].as_str(), "cherry");
    }

    #[test]
    fn test_collation_german_umlauts() {
        let mut localizer = IcuLocalizer::new("de-DE");
        // In German, Ä sorts with A
        let result = localizer.compare("Ägypten", "Andorra");
        // Both start with A-like characters, so comparison depends on secondary differences
        assert!(result != core::cmp::Ordering::Equal);
    }

    #[test]
    fn test_sort_key() {
        let mut localizer = IcuLocalizer::new("en-US");
        let key_a = localizer.get_sort_key("apple");
        let key_b = localizer.get_sort_key("banana");
        // Sort keys should compare bytewise to give same ordering as compare()
        assert!(key_a < key_b);
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    // ─── IcuError / IcuResult ────────────────────────────────────────────────

    #[test]
    fn icu_error_new_preserves_message_verbatim() {
        assert_eq!(IcuError::new("").message.as_str(), "");
        assert_eq!(IcuError::new("boom").message.as_str(), "boom");
        // From<String> as well as From<&str>
        assert_eq!(
            IcuError::new(String::from("owned")).message.as_str(),
            "owned"
        );
    }

    #[test]
    fn icu_error_new_survives_hostile_payloads() {
        // Embedded NUL, combining marks, emoji, RTL override, unpaired-looking escapes
        let nasty = "a\0b\u{202e}\u{0301}\u{1f4a9}\u{fffd}";
        assert_eq!(IcuError::new(nasty).message.as_str(), nasty);

        // A megabyte-sized message must round-trip without truncation
        let huge = "x".repeat(1_000_000);
        let err = IcuError::new(huge.clone());
        assert_eq!(err.message.as_str().len(), 1_000_000);
        assert_eq!(err.message.as_str(), huge.as_str());
    }

    #[test]
    fn icu_result_ok_err_round_trip_through_into_option() {
        assert_eq!(
            IcuResult::ok("value").into_option().map(|s| s.as_str().to_string()),
            Some(String::from("value"))
        );
        assert_eq!(IcuResult::err("nope").into_option(), None);
        // Empty payloads are still `Ok`, not silently coerced into `None`
        assert_eq!(
            IcuResult::ok("").into_option().map(|s| s.as_str().to_string()),
            Some(String::new())
        );
    }

    #[test]
    fn icu_result_unwrap_or_only_uses_default_on_err() {
        let fallback = AzString::from("FALLBACK");
        assert_eq!(
            IcuResult::ok("real").unwrap_or(fallback.clone()).as_str(),
            "real"
        );
        // An empty `Ok` must win over the default — `unwrap_or` is not `unwrap_or_default`
        assert_eq!(IcuResult::ok("").unwrap_or(fallback.clone()).as_str(), "");
        assert_eq!(
            IcuResult::err("bad").unwrap_or(fallback.clone()).as_str(),
            "FALLBACK"
        );
    }

    #[test]
    fn icu_result_equality_distinguishes_ok_from_err_with_same_payload() {
        assert_eq!(IcuResult::ok("x"), IcuResult::ok("x"));
        assert_ne!(IcuResult::ok("x"), IcuResult::err("x"));
        assert_ne!(IcuResult::ok("x"), IcuResult::ok("y"));
    }

    // ─── Plain data types ────────────────────────────────────────────────────

    #[test]
    fn plural_category_and_collation_strength_defaults() {
        assert_eq!(PluralCategory::default(), PluralCategory::Other);
        assert_eq!(CollationStrength::default(), CollationStrength::Tertiary);
    }

    #[test]
    fn icu_date_time_constructors_store_fields_unvalidated() {
        // These are `const fn`s with no validation: extreme values must construct
        // fine (validation happens later, in `format_date` / `format_time`).
        let d = IcuDate::new(i32::MIN, 0, 0);
        assert_eq!((d.year, d.month, d.day), (i32::MIN, 0, 0));

        let d = IcuDate::new(i32::MAX, u8::MAX, u8::MAX);
        assert_eq!((d.year, d.month, d.day), (i32::MAX, 255, 255));

        let t = IcuTime::new(u8::MAX, u8::MAX, u8::MAX);
        assert_eq!((t.hour, t.minute, t.second), (255, 255, 255));

        let dt = IcuDateTime::new(IcuDate::new(2025, 1, 15), IcuTime::new(16, 30, 45));
        assert_eq!(dt.date, IcuDate::new(2025, 1, 15));
        assert_eq!(dt.time, IcuTime::new(16, 30, 45));
    }

    #[test]
    fn icu_date_new_is_usable_in_const_context() {
        const D: IcuDate = IcuDate::new(2025, 1, 15);
        const T: IcuTime = IcuTime::new(0, 0, 0);
        const DT: IcuDateTime = IcuDateTime::new(D, T);
        assert_eq!(DT.date.year, 2025);
        assert_eq!(DT.time.hour, 0);
    }

    // ─── IcuStringVec (FFI vec) ──────────────────────────────────────────────

    #[test]
    fn icu_string_vec_round_trips_a_rust_vec() {
        let items = vec![AzString::from("a"), AzString::from(""), AzString::from("\u{1f600}")];
        let v = IcuStringVec::from(items.clone());
        assert_eq!(v.len(), 3);
        assert_eq!(v.as_slice(), items.as_slice());

        let empty = IcuStringVec::from(Vec::<AzString>::new());
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert!(empty.get(0).is_none());
    }

    // ─── IcuLocalizerHandle: construction / accessors ────────────────────────

    #[test]
    fn handle_new_stores_default_locale_verbatim() {
        assert_eq!(IcuLocalizerHandle::new("de-DE").get_default_locale().as_str(), "de-DE");
        // Garbage / empty locales are accepted at construction (no validation here)
        assert_eq!(IcuLocalizerHandle::new("").get_default_locale().as_str(), "");
        let junk = "!!! not a locale !!!";
        assert_eq!(IcuLocalizerHandle::new(junk).get_default_locale().as_str(), junk);
        // A pathologically long locale string must not panic
        let huge = "x".repeat(100_000);
        assert_eq!(IcuLocalizerHandle::new(&huge).get_default_locale().as_str().len(), 100_000);
    }

    #[test]
    fn handle_default_and_from_system_language_agree_with_new() {
        assert_eq!(IcuLocalizerHandle::default().get_default_locale().as_str(), "en-US");
        let h = IcuLocalizerHandle::from_system_language(&AzString::from("ja-JP"));
        assert_eq!(h.get_default_locale().as_str(), "ja-JP");
        // Empty system language (detection failure) must not panic
        let h = IcuLocalizerHandle::from_system_language(&AzString::from(""));
        assert_eq!(h.get_default_locale().as_str(), "");
    }

    #[test]
    fn handle_set_locale_is_an_alias_for_set_default_locale() {
        let mut a = IcuLocalizerHandle::new("en-US");
        let mut b = IcuLocalizerHandle::new("en-US");
        a.set_default_locale("fr-FR");
        b.set_locale("fr-FR");
        assert_eq!(a.get_default_locale().as_str(), b.get_default_locale().as_str());
        assert_eq!(a.get_default_locale().as_str(), "fr-FR");

        // Setting an empty / invalid locale is silently accepted
        a.set_locale("");
        assert_eq!(a.get_default_locale().as_str(), "");
    }

    #[test]
    fn handle_inner_is_stable_across_clones() {
        let h = IcuLocalizerHandle::new("en-US");
        let h2 = h.clone();
        // Both handles must observe the *same* inner allocation
        assert!(core::ptr::eq(h.inner(), h2.inner()));
        assert_eq!(h.inner().default_locale.lock().unwrap().as_str(), "en-US");
    }

    // ─── IcuLocalizerHandle: locale cache ────────────────────────────────────

    #[test]
    fn handle_caches_one_localizer_per_distinct_locale_string() {
        let h = IcuLocalizerHandle::new("en-US");
        assert_eq!(h.cached_locale_count(), 0);
        assert!(h.cached_locales().is_empty());

        let _ = h.format_integer("en-US", 1);
        assert_eq!(h.cached_locale_count(), 1);
        // Second call with the same locale must reuse the cached localizer
        let _ = h.format_integer("en-US", 2);
        assert_eq!(h.cached_locale_count(), 1);

        let _ = h.format_integer("de-DE", 1);
        assert_eq!(h.cached_locale_count(), 2);

        let mut locales: Vec<String> = h
            .cached_locales()
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();
        locales.sort();
        assert_eq!(locales, vec![String::from("de-DE"), String::from("en-US")]);

        h.clear_cache();
        assert_eq!(h.cached_locale_count(), 0);
        assert!(h.cached_locales().is_empty());
        // The handle stays usable after a cache clear
        assert!(!h.format_integer("en-US", 1).as_str().is_empty());
        assert_eq!(h.cached_locale_count(), 1);
    }

    #[test]
    fn handle_cache_key_is_the_raw_locale_string_not_the_parsed_locale() {
        // The cache is keyed on the *unnormalized* string, so differently-spelled
        // spellings of the same locale each get their own entry.
        let h = IcuLocalizerHandle::new("en-US");
        let _ = h.get_language("en-US");
        let _ = h.get_language("EN-us");
        assert_eq!(h.cached_locale_count(), 2);
        // ...but both still resolve to the same language.
        assert_eq!(h.get_language("en-US").as_str(), h.get_language("EN-us").as_str());
    }

    #[test]
    fn handle_unparseable_locales_fall_back_instead_of_panicking() {
        let h = IcuLocalizerHandle::new("en-US");
        let absurdly_long = "en-US-x-".repeat(500);
        for locale in ["", "!!!", "x", "-", absurdly_long.as_str()] {
            let s = h.format_integer(locale, 1234);
            assert!(!s.as_str().is_empty(), "empty output for locale {locale:?}");
            let lang = h.get_language(locale);
            assert!(!lang.as_str().is_empty(), "empty language for locale {locale:?}");
        }
    }

    // ─── IcuLocalizerHandle: refcount / FFI drop semantics ───────────────────

    #[test]
    fn handle_clone_shares_the_cache_and_survives_the_original_being_dropped() {
        let h = IcuLocalizerHandle::new("en-US");
        let _ = h.format_integer("en-US", 1);
        let h2 = h.clone();
        assert_eq!(h2.cached_locale_count(), 1);

        drop(h);
        // The clone still owns the inner allocation (refcount > 0 at drop time)
        assert_eq!(h2.cached_locale_count(), 1);
        assert!(!h2.format_integer("en-US", 1000).as_str().is_empty());
    }

    #[test]
    fn handle_many_clones_drop_exactly_once() {
        let h = IcuLocalizerHandle::new("en-US");
        let clones: Vec<IcuLocalizerHandle> = (0..64).map(|_| h.clone()).collect();
        assert_eq!(
            unsafe { (*h.copies).load(AtomicOrdering::SeqCst) },
            65,
            "one refcount per live handle"
        );
        drop(clones);
        assert_eq!(unsafe { (*h.copies).load(AtomicOrdering::SeqCst) }, 1);
        // Still alive and usable — no double-free, no use-after-free.
        assert_eq!(h.get_default_locale().as_str(), "en-US");
    }

    #[test]
    fn handle_drop_honors_run_destructor_flag() {
        // This mirrors what codegen does across the FFI boundary: a bitwise copy of
        // the handle with `run_destructor = false` must NOT decrement the refcount.
        let h = IcuLocalizerHandle::new("en-US");
        let shallow = IcuLocalizerHandle {
            ptr: h.ptr,
            copies: h.copies,
            run_destructor: false,
        };
        drop(shallow);
        assert_eq!(
            unsafe { (*h.copies).load(AtomicOrdering::SeqCst) },
            1,
            "a non-owning FFI copy must not touch the refcount"
        );
        // `h` must still be alive (this would be a use-after-free if the flag were ignored)
        assert_eq!(h.get_default_locale().as_str(), "en-US");
    }

    #[test]
    fn handle_is_usable_from_multiple_threads() {
        let h = IcuLocalizerHandle::new("en-US");
        let handles: Vec<_> = (0..8)
            .map(|i| {
                let h = h.clone();
                std::thread::spawn(move || {
                    let locale = if i % 2 == 0 { "en-US" } else { "de-DE" };
                    let s = h.format_integer(locale, 1_000_000 + i);
                    assert!(!s.as_str().is_empty());
                })
            })
            .collect();
        for t in handles {
            t.join().expect("worker thread panicked");
        }
        assert_eq!(h.cached_locale_count(), 2);
    }

    // ─── IcuLocalizerHandle: poisoned-lock fallbacks (with_localizer{,_or}) ──

    #[test]
    fn handle_falls_back_to_defaults_when_the_cache_mutex_is_poisoned() {
        let h = IcuLocalizerHandle::new("en-US");

        // Poison the cache mutex by panicking while holding the guard.
        let h2 = h.clone();
        let _ = std::thread::spawn(move || {
            let _guard = h2.inner().cache.lock().unwrap();
            panic!("intentional panic to poison the cache mutex");
        })
        .join();
        assert!(h.inner().cache.lock().is_err(), "cache mutex should be poisoned");

        // `with_localizer` → `R::default()`
        assert_eq!(h.cached_locale_count(), 0);
        assert!(h.cached_locales().is_empty());
        assert_eq!(h.format_integer("en-US", 1234567).as_str(), "");
        assert_eq!(h.format_decimal("en-US", 123456, 2).as_str(), "");
        assert_eq!(h.get_language("en-US").as_str(), "");
        assert_eq!(h.get_plural_category("en-US", 1), PluralCategory::Other);
        assert_eq!(h.compare_strings("en-US", "a", "b"), 0);
        assert!(h.get_sort_key("en-US", "a").is_empty());

        // `with_localizer_or` → explicit fallbacks
        assert_eq!(
            h.pluralize("en-US", 1, "z", "o", "t", "f", "m", "OTHER").as_str(),
            "OTHER"
        );
        let items = [AzString::from("A"), AzString::from("B")];
        assert_eq!(h.format_list("en-US", &items, ListType::And).as_str(), "A, B");
        assert!(matches!(
            h.format_date("en-US", IcuDate::new(2025, 1, 15), FormatLength::Medium),
            IcuResult::Err(_)
        ));
        assert!(matches!(
            h.format_time("en-US", IcuTime::new(16, 30, 45), false),
            IcuResult::Err(_)
        ));
        assert!(matches!(
            h.format_datetime(
                "en-US",
                IcuDateTime::new(IcuDate::new(2025, 1, 15), IcuTime::new(16, 30, 45)),
                FormatLength::Short
            ),
            IcuResult::Err(_)
        ));
        assert!(h.strings_equal("en-US", "same", "same"));
        assert!(!h.strings_equal("en-US", "a", "b"));
        let sorted = h.sort_strings("en-US", &items);
        assert_eq!(sorted.len(), 2, "fallback returns the input unsorted");

        // Neither of these may panic on a poisoned cache.
        h.clear_cache();
        assert!(h.load_data_blob(&[0xff, 0x00, 0x01]));
    }

    // ─── IcuLocalizerHandle: data blob ───────────────────────────────────────

    #[test]
    fn handle_load_data_blob_accepts_garbage_without_breaking_formatting() {
        let h = IcuLocalizerHandle::new("en-US");
        // Populate the cache first so `load_data_blob` walks existing localizers.
        let before = h.format_integer("en-US", 1234567);

        assert!(h.load_data_blob(&[]), "empty blob");
        assert!(h.load_data_blob(&[0xde, 0xad, 0xbe, 0xef]), "garbage blob");
        assert!(h.load_data_blob(&vec![0u8; 100_000]), "large blob");

        // Formatting must still work (the blob is stored, formatters get rebuilt).
        let after = h.format_integer("en-US", 1234567);
        assert_eq!(before.as_str(), after.as_str());
        // A locale created *after* the blob was stored also gets it, without panicking.
        assert!(!h.format_integer("fr-FR", 1234567).as_str().is_empty());
    }

    // ─── shared_localizer_handle ─────────────────────────────────────────────

    #[test]
    fn shared_localizer_handle_is_a_process_wide_singleton() {
        let a = shared_localizer_handle();
        let b = shared_localizer_handle();
        assert!(core::ptr::eq(a, b), "OnceLock must hand out the same handle");
        assert!(core::ptr::eq(a.inner(), b.inner()));
        assert_eq!(a.get_default_locale().as_str(), "en-US");
        // It is a live, usable handle (and its cache persists between calls).
        assert!(!a.format_integer("en-US", 1).as_str().is_empty());
        assert!(b.cached_locale_count() >= 1);
    }

    // ─── ICU4X backend ───────────────────────────────────────────────────────

    #[cfg(all(
        feature = "icu",
        not(all(target_os = "macos", feature = "icu_macos")),
        not(all(target_os = "windows", feature = "icu_windows"))
    ))]
    mod icu4x {
        use super::super::*;

        #[test]
        fn new_falls_back_to_en_us_for_unparseable_locales() {
            for junk in ["!!!", "not a locale", "en US", "-", "@@@@"] {
                let l = IcuLocalizer::new(junk);
                assert_eq!(l.get_language().as_str(), "en", "junk locale {junk:?}");
                assert_eq!(
                    l.get_region().map(|r| r.as_str().to_string()),
                    Some(String::from("US")),
                    "junk locale {junk:?}"
                );
            }
        }

        #[test]
        fn new_with_extreme_inputs_does_not_panic() {
            // Empty, huge, and non-ASCII locale strings must all resolve to *some*
            // usable locale rather than panicking.
            for locale in [
                String::new(),
                "x".repeat(100_000),
                "\u{1f600}\u{202e}\0".to_string(),
                "en-US-u-ca-buddhist-nu-thai".to_string(),
            ] {
                let l = IcuLocalizer::new(&locale);
                assert!(
                    !l.get_language().as_str().is_empty(),
                    "empty language for {locale:?}"
                );
                assert!(!l.get_locale().as_str().is_empty());
            }
        }

        #[test]
        fn getters_reflect_the_parsed_locale() {
            let l = IcuLocalizer::new("en-US");
            assert_eq!(l.get_language().as_str(), "en");
            assert_eq!(l.get_region().map(|r| r.as_str().to_string()), Some(String::from("US")));

            // A language-only locale has no region
            let l = IcuLocalizer::new("de");
            assert_eq!(l.get_language().as_str(), "de");
            assert_eq!(l.get_region(), None);

            assert_eq!(IcuLocalizer::default().get_language().as_str(), "en");
        }

        #[test]
        fn set_locale_rejects_garbage_and_leaves_the_old_locale_intact() {
            let mut l = IcuLocalizer::new("de-DE");
            assert!(!l.set_locale("!!!"), "garbage locale must be rejected");
            // The failed set must not have mutated any state.
            assert_eq!(l.get_language().as_str(), "de");
            assert_eq!(l.get_region().map(|r| r.as_str().to_string()), Some(String::from("DE")));
            assert_eq!(l.get_locale().as_str(), "de-DE");

            assert!(l.set_locale("fr-FR"));
            assert_eq!(l.get_language().as_str(), "fr");
            assert_eq!(l.get_locale().as_str(), "fr-FR");
        }

        #[test]
        fn set_locale_clears_cached_formatters() {
            let mut l = IcuLocalizer::new("en-US");
            assert_eq!(l.format_integer(1234567).as_str(), "1,234,567");
            assert!(l.set_locale("de-DE"));
            // If the decimal formatter cache were not cleared, this would still be en-US.
            let de = l.format_integer(1234567);
            assert_ne!(de.as_str(), "1,234,567");
            assert!(de.as_str().contains('.') || de.as_str().contains('\u{a0}'));
        }

        #[test]
        fn clone_preserves_the_locale_and_the_formatting_behaviour() {
            let mut a = IcuLocalizer::new("de-DE");
            let warmed = a.format_integer(1234567);
            let mut b = a.clone();
            assert_eq!(b.get_locale().as_str(), a.get_locale().as_str());
            // The clone drops the cached formatters, but must rebuild identical ones.
            assert_eq!(b.format_integer(1234567).as_str(), warmed.as_str());
        }

        #[test]
        fn load_data_blob_with_garbage_does_not_break_formatters() {
            let mut l = IcuLocalizer::new("en-US");
            assert_eq!(l.format_integer(1000).as_str(), "1,000");
            l.load_data_blob(vec![0xde, 0xad, 0xbe, 0xef]);
            // Caches were cleared; rebuilding them must still work.
            assert_eq!(l.format_integer(1000).as_str(), "1,000");
            l.load_data_blob(Vec::new());
            assert_eq!(l.format_integer(1000).as_str(), "1,000");
        }

        // ── numeric formatting ──

        #[test]
        fn format_integer_zero_and_negatives() {
            let mut l = IcuLocalizer::new("en-US");
            assert_eq!(l.format_integer(0).as_str(), "0");
            assert_eq!(l.format_integer(1).as_str(), "1");
            assert_eq!(l.format_integer(-1).as_str(), "-1");
            assert_eq!(l.format_integer(999).as_str(), "999");
            assert_eq!(l.format_integer(1000).as_str(), "1,000");
            assert_eq!(l.format_integer(-1234567).as_str(), "-1,234,567");
        }

        #[test]
        fn format_integer_round_trips_at_the_i64_limits() {
            let mut l = IcuLocalizer::new("en-US");
            for value in [i64::MIN, i64::MIN + 1, -1, 0, 1, i64::MAX - 1, i64::MAX] {
                let s = l.format_integer(value);
                let round_tripped: i64 = s
                    .as_str()
                    .replace(',', "")
                    .parse()
                    .unwrap_or_else(|e| panic!("cannot re-parse {:?} ({value}): {e}", s.as_str()));
                assert_eq!(round_tripped, value, "lossy formatting of {value}");
            }
        }

        #[test]
        fn format_decimal_places_zero_and_negative() {
            let mut l = IcuLocalizer::new("en-US");
            assert_eq!(l.format_decimal(0, 0).as_str(), "0");
            assert_eq!(l.format_decimal(0, 2).as_str(), "0.00");
            assert_eq!(l.format_decimal(123456, 2).as_str(), "1,234.56");
            assert_eq!(l.format_decimal(5, 3).as_str(), "0.005");
            assert_eq!(l.format_decimal(-5, 3).as_str(), "-0.005");
            assert_eq!(l.format_decimal(-123456, 2).as_str(), "-1,234.56");
            // Negative decimal places multiply by 10^n
            assert_eq!(l.format_decimal(5, -3).as_str(), "5,000");
        }

        #[test]
        fn format_decimal_is_exact_for_large_i64_values() {
            let mut l = IcuLocalizer::new("en-US");
            // The whole point of the i64/i16 API (vs f64) is that this stays exact.
            let s = l.format_decimal(i64::MAX, 2);
            assert_eq!(s.as_str(), "92,233,720,368,547,758.07");
            let s = l.format_decimal(i64::MIN, 2);
            assert_eq!(s.as_str(), "-92,233,720,368,547,758.08");
            // Every significant digit of i64::MAX survives 0 decimal places
            assert_eq!(
                l.format_decimal(i64::MAX, 0).as_str().replace(',', ""),
                "9223372036854775807"
            );
        }

        #[test]
        fn format_decimal_extreme_decimal_places_do_not_panic() {
            let mut l = IcuLocalizer::new("en-US");
            // i16::MAX places → a very small number; must not panic or hang.
            let s = l.format_decimal(1, i16::MAX);
            assert!(s.as_str().starts_with('0'), "unexpected: {:?}", s.as_str());
            // i16::MIN + 1 places → a very large number; must not panic.
            //
            // NOTE: i16::MIN itself is deliberately *not* tested here: the impl does
            // `multiply_pow10(-decimal_places)`, and negating i16::MIN overflows.
            // See the report — that is a genuine bug, not something to assert around.
            let s = l.format_decimal(1, i16::MIN + 1);
            assert!(!s.as_str().is_empty());
        }

        // ── plural rules ──

        #[test]
        fn plural_category_english_matches_the_documented_examples() {
            let mut l = IcuLocalizer::new("en-US");
            assert_eq!(l.get_plural_category(0), PluralCategory::Other);
            assert_eq!(l.get_plural_category(1), PluralCategory::One);
            assert_eq!(l.get_plural_category(2), PluralCategory::Other);
            assert_eq!(l.get_plural_category(100), PluralCategory::Other);
        }

        #[test]
        fn plural_category_uses_the_absolute_value_for_negatives() {
            let mut l = IcuLocalizer::new("en-US");
            // The impl calls `value.unsigned_abs()`, so -1 is categorised like 1.
            assert_eq!(l.get_plural_category(-1), PluralCategory::One);
            assert_eq!(l.get_plural_category(-2), PluralCategory::Other);
        }

        #[test]
        fn plural_category_at_the_i64_limits_does_not_panic() {
            let mut l = IcuLocalizer::new("en-US");
            // `i64::MIN.unsigned_abs()` is the classic overflow trap here.
            assert_eq!(l.get_plural_category(i64::MIN), PluralCategory::Other);
            assert_eq!(l.get_plural_category(i64::MIN + 1), PluralCategory::Other);
            assert_eq!(l.get_plural_category(i64::MAX), PluralCategory::Other);
        }

        #[test]
        fn plural_category_polish_and_arabic_match_the_documented_examples() {
            let mut pl = IcuLocalizer::new("pl");
            assert_eq!(pl.get_plural_category(1), PluralCategory::One);
            assert_eq!(pl.get_plural_category(2), PluralCategory::Few);
            assert_eq!(pl.get_plural_category(5), PluralCategory::Many);

            let mut ar = IcuLocalizer::new("ar");
            assert_eq!(ar.get_plural_category(0), PluralCategory::Zero);
            assert_eq!(ar.get_plural_category(1), PluralCategory::One);
            assert_eq!(ar.get_plural_category(2), PluralCategory::Two);
            assert_eq!(ar.get_plural_category(3), PluralCategory::Few);
            assert_eq!(ar.get_plural_category(11), PluralCategory::Many);
        }

        #[test]
        fn plural_category_conversion_is_exhaustive() {
            assert_eq!(PluralCategory::from(IcuPluralCategory::Zero), PluralCategory::Zero);
            assert_eq!(PluralCategory::from(IcuPluralCategory::One), PluralCategory::One);
            assert_eq!(PluralCategory::from(IcuPluralCategory::Two), PluralCategory::Two);
            assert_eq!(PluralCategory::from(IcuPluralCategory::Few), PluralCategory::Few);
            assert_eq!(PluralCategory::from(IcuPluralCategory::Many), PluralCategory::Many);
            assert_eq!(PluralCategory::from(IcuPluralCategory::Other), PluralCategory::Other);
        }

        #[test]
        fn pluralize_selects_the_category_string_and_substitutes_every_placeholder() {
            let mut l = IcuLocalizer::new("en-US");
            assert_eq!(
                l.pluralize(1, "ZERO", "ONE", "TWO", "FEW", "MANY", "OTHER").as_str(),
                "ONE"
            );
            // English has no `zero` category → 0 falls into `other`, NOT `zero`.
            assert_eq!(
                l.pluralize(0, "ZERO", "ONE", "TWO", "FEW", "MANY", "OTHER").as_str(),
                "OTHER"
            );
            // Every `{}` occurrence is replaced, not just the first
            assert_eq!(l.pluralize(3, "", "", "", "", "", "{} of {}").as_str(), "3 of 3");
            // Templates without a placeholder are passed through untouched
            assert_eq!(l.pluralize(7, "", "", "", "", "", "items").as_str(), "items");
            // Empty templates stay empty
            assert_eq!(l.pluralize(7, "", "", "", "", "", "").as_str(), "");
        }

        #[test]
        fn pluralize_at_the_i64_limits_substitutes_the_signed_value() {
            let mut l = IcuLocalizer::new("en-US");
            assert_eq!(
                l.pluralize(i64::MIN, "z", "o", "t", "f", "m", "{}").as_str(),
                "-9223372036854775808"
            );
            assert_eq!(
                l.pluralize(i64::MAX, "z", "o", "t", "f", "m", "{}").as_str(),
                "9223372036854775807"
            );
            // Note: the *category* uses |value|, so -1 selects `one` while the
            // substituted number keeps its sign.
            assert_eq!(l.pluralize(-1, "z", "{} one", "t", "f", "m", "{} other").as_str(), "-1 one");
        }

        // ── list formatting ──

        #[test]
        fn format_list_handles_empty_and_single_item_lists() {
            let mut l = IcuLocalizer::new("en-US");
            for list_type in [ListType::And, ListType::Or, ListType::Unit] {
                assert_eq!(l.format_list(&[], list_type).as_str(), "");
                assert_eq!(l.format_list(&[AzString::from("A")], list_type).as_str(), "A");
            }
        }

        #[test]
        fn format_list_uses_the_right_conjunction() {
            let mut l = IcuLocalizer::new("en-US");
            let items = [AzString::from("A"), AzString::from("B"), AzString::from("C")];
            assert!(l.format_list(&items, ListType::And).as_str().contains("and"));
            assert!(l.format_list(&items, ListType::Or).as_str().contains("or"));
            // Unit lists are a plain comma join (documented TODO in the impl)
            assert_eq!(l.format_list(&items, ListType::Unit).as_str(), "A, B, C");
        }

        #[test]
        fn format_list_survives_hostile_items() {
            let mut l = IcuLocalizer::new("en-US");
            // Empty strings, embedded separators, NULs, emoji, RTL overrides
            let items = [
                AzString::from(""),
                AzString::from(", and "),
                AzString::from("a\0b"),
                AzString::from("\u{1f600}\u{202e}"),
            ];
            assert!(!l.format_list(&items, ListType::And).as_str().is_empty());

            // A large list must not blow up
            let many: Vec<AzString> = (0..1000).map(|i| AzString::from(i.to_string())).collect();
            let joined = l.format_list(&many, ListType::Unit);
            assert_eq!(joined.as_str().split(", ").count(), 1000);
            assert!(!l.format_list(&many, ListType::And).as_str().is_empty());
        }

        // ── date / time formatting ──

        #[test]
        fn format_date_rejects_out_of_range_months_and_days() {
            let mut l = IcuLocalizer::new("en-US");
            for (y, m, d) in [
                (2025, 0, 15),   // month 0
                (2025, 13, 15),  // month 13
                (2025, 255, 15), // month 255
                (2025, 1, 0),    // day 0
                (2025, 1, 32),   // day 32
                (2025, 1, 255),  // day 255
                (2025, 2, 30),   // February 30th
                (2025, 2, 29),   // 2025 is not a leap year
            ] {
                let r = l.format_date(IcuDate::new(y, m, d), FormatLength::Medium);
                assert!(
                    matches!(r, IcuResult::Err(_)),
                    "expected Err for {y}-{m}-{d}, got {r:?}"
                );
            }
            // ...but a real leap day is fine
            assert!(matches!(
                l.format_date(IcuDate::new(2024, 2, 29), FormatLength::Medium),
                IcuResult::Ok(_)
            ));
        }

        #[test]
        fn format_date_formats_all_lengths_for_a_valid_date() {
            let mut l = IcuLocalizer::new("en-US");
            let date = IcuDate::new(2025, 1, 15);
            for length in [FormatLength::Short, FormatLength::Medium, FormatLength::Long] {
                match l.format_date(date, length) {
                    IcuResult::Ok(s) => {
                        assert!(s.as_str().contains("15"), "no day in {:?}", s.as_str());
                        assert!(!s.as_str().is_empty());
                    }
                    IcuResult::Err(e) => panic!("valid date rejected: {e:?}"),
                }
            }
        }

        #[test]
        fn format_date_at_extreme_years_returns_a_result_instead_of_panicking() {
            let mut l = IcuLocalizer::new("en-US");
            // Year 0 and negative (BCE) years cross the era boundary; the JS Date
            // range endpoints are the widest years any caller can realistically hit.
            //
            // NOTE: i32::MIN / i32::MAX are deliberately excluded — the era-year
            // conversion (`1 - extended_year`) overflows there, inside icu4x rather
            // than in azul. See the report.
            for year in [-271_821, -1, 0, 1, 275_760] {
                // Ok or Err are both acceptable; a panic is not.
                let r = l.format_date(IcuDate::new(year, 1, 1), FormatLength::Short);
                assert!(matches!(r, IcuResult::Ok(_) | IcuResult::Err(_)));
            }
        }

        #[test]
        fn format_time_rejects_out_of_range_components() {
            let mut l = IcuLocalizer::new("en-US");
            for (h, m, s) in [
                (24, 0, 0),
                (255, 0, 0),
                (0, 60, 0),
                (0, 255, 0),
                (0, 0, 255),
            ] {
                for include_seconds in [true, false] {
                    let r = l.format_time(IcuTime::new(h, m, s), include_seconds);
                    assert!(
                        matches!(r, IcuResult::Err(_)),
                        "expected Err for {h}:{m}:{s}, got {r:?}"
                    );
                }
            }
        }

        #[test]
        fn format_time_boundary_values_are_accepted() {
            let mut l = IcuLocalizer::new("en-US");
            for (h, m, s) in [(0, 0, 0), (23, 59, 59), (12, 0, 0)] {
                for include_seconds in [true, false] {
                    assert!(
                        matches!(
                            l.format_time(IcuTime::new(h, m, s), include_seconds),
                            IcuResult::Ok(_)
                        ),
                        "valid time {h}:{m}:{s} rejected"
                    );
                }
            }
            // Seconds only show up when asked for
            let with = l.format_time(IcuTime::new(16, 30, 45), true).unwrap_or(AzString::from(""));
            let without = l.format_time(IcuTime::new(16, 30, 45), false).unwrap_or(AzString::from(""));
            assert!(with.as_str().contains("45"));
            assert!(!without.as_str().contains("45"));
        }

        #[test]
        fn format_datetime_propagates_invalid_date_and_time_as_err() {
            let mut l = IcuLocalizer::new("en-US");
            let good_date = IcuDate::new(2025, 1, 15);
            let good_time = IcuTime::new(16, 30, 45);

            assert!(matches!(
                l.format_datetime(IcuDateTime::new(good_date, good_time), FormatLength::Medium),
                IcuResult::Ok(_)
            ));
            assert!(matches!(
                l.format_datetime(
                    IcuDateTime::new(IcuDate::new(2025, 13, 1), good_time),
                    FormatLength::Medium
                ),
                IcuResult::Err(_)
            ));
            assert!(matches!(
                l.format_datetime(
                    IcuDateTime::new(good_date, IcuTime::new(24, 0, 0)),
                    FormatLength::Medium
                ),
                IcuResult::Err(_)
            ));
        }

        // ── collation ──

        #[test]
        fn compare_is_reflexive_and_antisymmetric() {
            let mut l = IcuLocalizer::new("en-US");
            let words = ["", "a", "A", "z", "\u{e9}", "\u{1f600}", "a\0b", "stra\u{df}e"];
            for a in words {
                assert_eq!(l.compare(a, a), core::cmp::Ordering::Equal, "not reflexive: {a:?}");
                for b in words {
                    assert_eq!(
                        l.compare(a, b),
                        l.compare(b, a).reverse(),
                        "not antisymmetric: {a:?} vs {b:?}"
                    );
                }
            }
        }

        #[test]
        fn compare_treats_the_empty_string_as_smallest() {
            let mut l = IcuLocalizer::new("en-US");
            assert_eq!(l.compare("", ""), core::cmp::Ordering::Equal);
            assert_eq!(l.compare("", "a"), core::cmp::Ordering::Less);
            assert_eq!(l.compare("a", ""), core::cmp::Ordering::Greater);
            assert!(l.strings_equal("", ""));
        }

        #[test]
        fn collation_is_canonically_equivalent_but_case_sensitive() {
            let mut l = IcuLocalizer::new("en-US");
            // Precomposed "é" and decomposed "e" + U+0301 must collate as equal.
            assert!(l.strings_equal("\u{e9}", "e\u{301}"));
            assert_eq!(l.compare("\u{e9}", "e\u{301}"), core::cmp::Ordering::Equal);
            // Default strength is tertiary → case differences still matter.
            assert!(!l.strings_equal("a", "A"));
        }

        #[test]
        fn sort_strings_is_stable_idempotent_and_preserves_the_multiset() {
            let mut l = IcuLocalizer::new("en-US");
            let input = vec![
                AzString::from("cherry"),
                AzString::from(""),
                AzString::from("apple"),
                AzString::from("apple"),
                AzString::from("banana"),
            ];
            let once = l.sorted_strings(&input);
            assert_eq!(once.len(), input.len(), "sorting must not drop duplicates");
            for item in &input {
                assert!(once.iter().any(|s| s.as_str() == item.as_str()));
            }
            // Output is non-decreasing under the same collator
            for pair in once.windows(2) {
                assert_ne!(
                    l.compare(pair[0].as_str(), pair[1].as_str()),
                    core::cmp::Ordering::Greater
                );
            }
            // Sorting an already-sorted list changes nothing
            let twice = l.sorted_strings(&once);
            assert_eq!(twice, once);
        }

        #[test]
        fn sort_strings_handles_degenerate_slices() {
            let mut l = IcuLocalizer::new("en-US");
            let mut empty: Vec<AzString> = Vec::new();
            l.sort_strings(&mut empty);
            assert!(empty.is_empty());

            let mut single = vec![AzString::from("only")];
            l.sort_strings(&mut single);
            assert_eq!(single[0].as_str(), "only");
        }

        #[test]
        fn sort_keys_order_the_same_way_compare_does() {
            let mut l = IcuLocalizer::new("en-US");
            let words = ["", "apple", "banana", "cherry", "\u{e9}clair", "zebra"];
            for a in words {
                for b in words {
                    let key_a = l.get_sort_key(a);
                    let key_b = l.get_sort_key(b);
                    assert_eq!(
                        key_a.cmp(&key_b),
                        l.compare(a, b),
                        "sort key order disagrees with compare() for {a:?} vs {b:?}"
                    );
                }
            }
        }

        #[test]
        fn collation_survives_very_long_and_hostile_inputs() {
            let mut l = IcuLocalizer::new("en-US");
            let long = "a".repeat(10_000);
            let long_plus = format!("{long}b");
            assert_eq!(l.compare(&long, &long), core::cmp::Ordering::Equal);
            assert_eq!(l.compare(&long, &long_plus), core::cmp::Ordering::Less);
            assert!(!l.get_sort_key(&long).is_empty());
            // NULs and unpaired combining marks must not panic
            let _ = l.get_sort_key("\0\u{301}\u{1f600}");
            assert_eq!(l.compare("\0", "\0"), core::cmp::Ordering::Equal);
        }
    }

    // ─── macOS / Windows native backend helpers ──────────────────────────────

    #[cfg(any(
        all(target_os = "macos", feature = "icu_macos"),
        all(target_os = "windows", feature = "icu_windows"),
    ))]
    mod native_helpers {
        use super::super::*;

        #[test]
        fn decimal_string_zero_and_positive_places() {
            assert_eq!(decimal_string(0, 0), "0");
            assert_eq!(decimal_string(0, 2), "0.00");
            assert_eq!(decimal_string(123456, 2), "1234.56");
            assert_eq!(decimal_string(5, 3), "0.005");
            assert_eq!(decimal_string(1, 1), "0.1");
            // Exactly as many digits as decimal places → leading "0."
            assert_eq!(decimal_string(99, 2), "0.99");
        }

        #[test]
        fn decimal_string_negative_values_keep_a_single_leading_minus() {
            assert_eq!(decimal_string(-123456, 2), "-1234.56");
            assert_eq!(decimal_string(-5, 3), "-0.005");
            assert_eq!(decimal_string(-1, 0), "-1");
            assert_eq!(decimal_string(-1, -2), "-100");
        }

        #[test]
        fn decimal_string_negative_places_append_zeros() {
            assert_eq!(decimal_string(5, -3), "5000");
            assert_eq!(decimal_string(0, -3), "0000");
            assert_eq!(decimal_string(i64::MIN, -1), "-92233720368547758080");
        }

        #[test]
        fn decimal_string_is_exact_at_the_i64_limits() {
            // The whole reason this helper exists: no f64 round-trip, no lost digits.
            assert_eq!(decimal_string(i64::MAX, 0), "9223372036854775807");
            assert_eq!(decimal_string(i64::MIN, 0), "-9223372036854775808");
            assert_eq!(decimal_string(i64::MAX, 2), "92233720368547758.07");
            // i64::MIN.abs() overflows i64 — the impl must go through i128/unsigned_abs.
            assert_eq!(decimal_string(i64::MIN, 2), "-92233720368547758.08");
            // Exactly 19 decimal places = the digit count of |i64::MIN|
            assert_eq!(decimal_string(i64::MIN, 19), "-0.9223372036854775808");
            assert_eq!(decimal_string(i64::MIN, 20), "-0.09223372036854775808");
        }

        #[test]
        fn decimal_string_huge_decimal_places_do_not_panic() {
            let s = decimal_string(1, i16::MAX);
            assert!(s.starts_with("0."));
            assert_eq!(s.len(), 2 + i16::MAX as usize);

            let s = decimal_string(1, i16::MIN + 1);
            assert!(s.starts_with('1'));
            assert_eq!(s.len(), 1 + (i16::MAX as usize));

            // i16::MIN: `-decimal_places` used to overflow i16; the zero count is
            // now taken via `unsigned_abs` in a wider type. |i16::MIN| = 32768.
            let s = decimal_string(1, i16::MIN);
            assert!(s.starts_with('1'));
            assert_eq!(s.len(), 1 + 32768);
        }

        #[test]
        fn plural_for_defaults_to_english_rules_for_unknown_languages() {
            for lang in ["en", "xx", "", "klingon", "de"] {
                assert_eq!(plural_for(1, lang), PluralCategory::One, "lang {lang:?}");
                assert_eq!(plural_for(0, lang), PluralCategory::Other, "lang {lang:?}");
                assert_eq!(plural_for(2, lang), PluralCategory::Other, "lang {lang:?}");
            }
        }

        #[test]
        fn plural_for_strips_region_and_script_subtags() {
            assert_eq!(plural_for(2, "pl-PL"), plural_for(2, "pl"));
            assert_eq!(plural_for(2, "pl_PL"), plural_for(2, "pl"));
            assert_eq!(plural_for(1, "en-US"), PluralCategory::One);
            assert_eq!(plural_for(0, "fr-CA"), PluralCategory::One);
        }

        #[test]
        fn plural_for_arabic_covers_all_six_categories() {
            assert_eq!(plural_for(0, "ar"), PluralCategory::Zero);
            assert_eq!(plural_for(1, "ar"), PluralCategory::One);
            assert_eq!(plural_for(2, "ar"), PluralCategory::Two);
            assert_eq!(plural_for(3, "ar"), PluralCategory::Few);
            assert_eq!(plural_for(10, "ar"), PluralCategory::Few);
            assert_eq!(plural_for(11, "ar"), PluralCategory::Many);
            assert_eq!(plural_for(99, "ar"), PluralCategory::Many);
            assert_eq!(plural_for(100, "ar"), PluralCategory::Other);
            assert_eq!(plural_for(103, "ar"), PluralCategory::Few);
        }

        #[test]
        fn plural_for_slavic_and_polish_rules() {
            assert_eq!(plural_for(1, "ru"), PluralCategory::One);
            assert_eq!(plural_for(11, "ru"), PluralCategory::Many);
            assert_eq!(plural_for(2, "ru"), PluralCategory::Few);
            assert_eq!(plural_for(12, "ru"), PluralCategory::Many);
            assert_eq!(plural_for(5, "ru"), PluralCategory::Many);

            assert_eq!(plural_for(1, "pl"), PluralCategory::One);
            assert_eq!(plural_for(2, "pl"), PluralCategory::Few);
            assert_eq!(plural_for(5, "pl"), PluralCategory::Many);
            assert_eq!(plural_for(22, "pl"), PluralCategory::Few);
            assert_eq!(plural_for(12, "pl"), PluralCategory::Many);
        }

        #[test]
        fn plural_for_negative_values_are_deterministic() {
            // Languages whose rules go through `.abs()` mirror the positive result...
            assert_eq!(plural_for(-2, "ru"), plural_for(2, "ru"));
            assert_eq!(plural_for(-11, "ar"), PluralCategory::Many);
            // ...while the English-style default branch compares against 1 directly,
            // so -1 is `Other`, not `One`.
            assert_eq!(plural_for(-1, "en"), PluralCategory::Other);
        }

        #[test]
        fn plural_for_at_the_i64_limits_does_not_panic() {
            // NOTE: `plural_for(i64::MIN, "ar")` (and any other `.abs()` branch) is
            // deliberately not tested: `i64::MIN.abs()` overflows. See the report.
            assert_eq!(plural_for(i64::MAX, "en"), PluralCategory::Other);
            assert_eq!(plural_for(i64::MIN, "en"), PluralCategory::Other);
            assert_eq!(plural_for(i64::MIN, "cs"), PluralCategory::Other);
            assert_eq!(plural_for(i64::MIN, "fr"), PluralCategory::Other);
            assert_eq!(plural_for(i64::MIN, "he"), PluralCategory::Other);
            // The `.abs()` branches are safe for everything except i64::MIN itself.
            // i64::MAX ends in ...807, so |n| % 100 == 7 → Arabic "few".
            assert_eq!(plural_for(i64::MIN + 1, "ru"), PluralCategory::Many);
            assert_eq!(plural_for(i64::MAX, "ar"), PluralCategory::Few);
        }
    }

    // ─── chrono-backed constructors ──────────────────────────────────────────

    #[cfg(feature = "icu_chrono")]
    mod chrono_backed {
        use super::super::*;

        #[test]
        fn now_constructors_return_in_range_fields() {
            for d in [IcuDate::now(), IcuDate::now_utc()] {
                assert!((1..=12).contains(&d.month), "month out of range: {d:?}");
                assert!((1..=31).contains(&d.day), "day out of range: {d:?}");
                assert!(d.year >= 2020, "clock looks broken: {d:?}");
            }
            for t in [IcuTime::now(), IcuTime::now_utc()] {
                assert!(t.hour < 24, "hour out of range: {t:?}");
                assert!(t.minute < 60, "minute out of range: {t:?}");
                assert!(t.second <= 60, "second out of range: {t:?}");
            }
            for dt in [IcuDateTime::now(), IcuDateTime::now_utc()] {
                assert!((1..=12).contains(&dt.date.month));
                assert!(dt.time.hour < 24);
            }
        }

        #[test]
        fn timestamp_now_seconds_and_millis_agree() {
            let millis = IcuDateTime::timestamp_now();
            let secs = IcuDateTime::timestamp_now_seconds();
            assert!(millis > 1_600_000_000_000, "millis look wrong: {millis}");
            assert!(secs > 1_600_000_000, "seconds look wrong: {secs}");
            assert!(
                (millis / 1000 - secs).abs() <= 2,
                "millis {millis} and seconds {secs} disagree"
            );
        }

        #[test]
        fn from_timestamp_at_the_epoch_and_just_before_it() {
            let dt = IcuDateTime::from_timestamp(0).expect("epoch must be representable");
            assert_eq!(dt.date, IcuDate::new(1970, 1, 1));
            assert_eq!(dt.time, IcuTime::new(0, 0, 0));

            let dt = IcuDateTime::from_timestamp(-1).expect("1969 must be representable");
            assert_eq!(dt.date, IcuDate::new(1969, 12, 31));
            assert_eq!(dt.time, IcuTime::new(23, 59, 59));
        }

        #[test]
        fn from_timestamp_returns_none_at_the_i64_limits() {
            assert_eq!(IcuDateTime::from_timestamp(i64::MAX), None);
            assert_eq!(IcuDateTime::from_timestamp(i64::MIN), None);
            assert_eq!(IcuDateTime::from_timestamp_millis(i64::MAX), None);
            assert_eq!(IcuDateTime::from_timestamp_millis(i64::MIN), None);
        }

        #[test]
        fn from_timestamp_millis_truncates_towards_zero() {
            // Sub-second millis are dropped by the `/ 1000` integer division...
            let dt = IcuDateTime::from_timestamp_millis(999).expect("representable");
            assert_eq!(dt.date, IcuDate::new(1970, 1, 1));
            assert_eq!(dt.time, IcuTime::new(0, 0, 0));

            // ...and for negative millis the truncation rounds *up* (towards zero),
            // so -1ms lands on the epoch rather than on 1969-12-31T23:59:59.
            let dt = IcuDateTime::from_timestamp_millis(-1).expect("representable");
            assert_eq!(dt.date, IcuDate::new(1970, 1, 1));
            assert_eq!(dt.time, IcuTime::new(0, 0, 0));

            let dt = IcuDateTime::from_timestamp_millis(-1000).expect("representable");
            assert_eq!(dt.time, IcuTime::new(23, 59, 59));
            assert_eq!(dt.date, IcuDate::new(1969, 12, 31));
        }

        #[test]
        fn from_timestamp_round_trips_a_known_instant() {
            // 2025-01-15T16:30:45Z
            let dt = IcuDateTime::from_timestamp(1_736_958_645).expect("representable");
            assert_eq!(dt.date, IcuDate::new(2025, 1, 15));
            assert_eq!(dt.time, IcuTime::new(16, 30, 45));
            // The millisecond constructor must agree with the seconds one
            assert_eq!(
                IcuDateTime::from_timestamp_millis(1_736_958_645_123),
                Some(dt)
            );
        }
    }
}
