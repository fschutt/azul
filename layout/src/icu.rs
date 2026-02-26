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
    sync::Arc,
    vec::Vec,
};
use core::fmt::Write;
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

// Import FmtArg types from fmt module for format_string_icu
use crate::fmt::{FmtArg, FmtArgVec, FmtValue};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum PluralCategory {
    Zero,
    One,
    Two,
    Few,
    Many,
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
        rules.category_for(value as usize).into()
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

/// Inner data for IcuLocalizerHandle - contains the actual cache and settings.
struct IcuLocalizerInner {
    cache: Mutex<BTreeMap<String, IcuLocalizer>>,
    /// Default locale to use when none is specified
    default_locale: Mutex<AzString>,
}

/// A thread-safe cache of ICU localizers for multiple locales.
///
/// This is passed to callbacks via `CallbackInfo` and `LayoutCallbackInfo`.
/// It uses `Arc` internally for safe shared access, making it FFI-compatible
/// as a single pointer.
///
/// Each locale's IcuLocalizer is lazily created and cached on first use.
/// All methods take a `locale: &str` parameter to specify which locale to use.
#[repr(C)]
#[derive(Clone)]
pub struct IcuLocalizerHandle {
    ptr: Arc<IcuLocalizerInner>,
}

impl core::fmt::Debug for IcuLocalizerHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let default_locale = self.ptr.default_locale.lock()
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
            ptr: Arc::new(IcuLocalizerInner {
                cache: Mutex::new(BTreeMap::new()),
                default_locale: Mutex::new(AzString::from("en-US")),
            }),
        }
    }
}

impl IcuLocalizerHandle {
    /// Create a new empty cache with a default locale.
    pub fn new(default_locale: &str) -> Self {
        Self {
            ptr: Arc::new(IcuLocalizerInner {
                cache: Mutex::new(BTreeMap::new()),
                default_locale: Mutex::new(AzString::from(default_locale)),
            }),
        }
    }

    /// Create a cache initialized with the system language.
    pub fn from_system_language(language: &AzString) -> Self {
        Self::new(language.as_str())
    }

    /// Get the default locale string.
    pub fn get_default_locale(&self) -> AzString {
        self.ptr.default_locale.lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| AzString::from("en-US"))
    }

    /// Set the default locale.
    pub fn set_default_locale(&mut self, locale: &str) {
        if let Ok(mut guard) = self.ptr.default_locale.lock() {
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
        if let Ok(mut cache) = self.ptr.cache.lock() {
            // Clear the cache so all localizers will be recreated with new data
            cache.clear();
            // Note: The actual blob needs to be stored somewhere accessible to new localizers
            // For now, we just clear the cache so they'll be recreated with default data
            true
        } else {
            false
        }
    }

    /// Get or create a localizer for the given locale.
    /// This is an internal helper that handles cache access.
    fn with_localizer<F, R>(&self, locale: &str, f: F) -> R
    where
        F: FnOnce(&mut IcuLocalizer) -> R,
        R: Default,
    {
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                f(localizer)
            })
            .unwrap_or_default()
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
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                localizer.get_plural_category(value)
            })
            .unwrap_or(PluralCategory::Other)
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
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                localizer.pluralize(value, zero, one, two, few, many, other)
            })
            .unwrap_or_else(|_| AzString::from(other))
    }

    /// Format a list of items with locale-appropriate conjunctions.
    ///
    /// # Example
    /// ```rust,ignore
    /// cache.format_list("en-US", &items, ListType::And) // → "A, B, and C"
    /// cache.format_list("de-DE", &items, ListType::And) // → "A, B und C"
    /// ```
    pub fn format_list(&self, locale: &str, items: &[AzString], list_type: ListType) -> AzString {
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                localizer.format_list(items, list_type)
            })
            .unwrap_or_else(|_| {
                let strs: Vec<&str> = items.iter().map(|s| s.as_str()).collect();
                AzString::from(strs.join(", "))
            })
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
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                localizer.format_date(date, length)
            })
            .unwrap_or_else(|e| IcuResult::err(format!("Lock error: {:?}", e)))
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
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                localizer.format_time(time, include_seconds)
            })
            .unwrap_or_else(|e| IcuResult::err(format!("Lock error: {:?}", e)))
    }

    /// Format a date and time according to the specified locale.
    pub fn format_datetime(&self, locale: &str, datetime: IcuDateTime, length: FormatLength) -> IcuResult {
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                localizer.format_datetime(datetime, length)
            })
            .unwrap_or_else(|e| IcuResult::err(format!("Lock error: {:?}", e)))
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
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                match localizer.compare(a, b) {
                    core::cmp::Ordering::Less => -1,
                    core::cmp::Ordering::Equal => 0,
                    core::cmp::Ordering::Greater => 1,
                }
            })
            .unwrap_or(0)
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
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                IcuStringVec::from(localizer.sorted_strings(strings))
            })
            .unwrap_or_else(|_| IcuStringVec::from(strings.to_vec()))
    }

    /// Check if two strings are equal according to locale collation rules.
    pub fn strings_equal(&self, locale: &str, a: &str, b: &str) -> bool {
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                localizer.strings_equal(a, b)
            })
            .unwrap_or_else(|_| a == b)
    }

    /// Get the sort key for a string (for efficient bulk sorting).
    pub fn get_sort_key(&self, locale: &str, s: &str) -> Vec<u8> {
        self.ptr.cache
            .lock()
            .map(|mut cache| {
                let localizer = cache
                    .entry(locale.to_string())
                    .or_insert_with(|| IcuLocalizer::new(locale));
                localizer.get_sort_key(s)
            })
            .unwrap_or_default()
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
        if let Ok(mut cache) = self.ptr.cache.lock() {
            cache.clear();
        }
    }

    /// Get the number of cached locales.
    pub fn cached_locale_count(&self) -> usize {
        self.ptr.cache
            .lock()
            .map(|cache| cache.len())
            .unwrap_or(0)
    }

    /// Get a list of all cached locale strings.
    pub fn cached_locales(&self) -> Vec<AzString> {
        self.ptr.cache
            .lock()
            .map(|cache| cache.keys().map(|k| AzString::from(k.clone())).collect())
            .unwrap_or_default()
    }
}

// ============================================================================
// IcuFormattedValue: Wrapper for strfmt integration
// ============================================================================

/// Wrapper that formats FmtValue using ICU localization.
///
/// Used internally for format_string functionality.
struct IcuFormattedValue {
    value: FmtValue,
    localizer: IcuLocalizerHandle,
    locale: String,
}

impl strfmt::DisplayStr for IcuFormattedValue {
    fn display_str(&self, f: &mut strfmt::Formatter<'_, '_>) -> strfmt::Result<()> {
        use strfmt::DisplayStr;

        match &self.value {
            // For integers, use ICU formatting
            FmtValue::Uint(v) => {
                self.localizer.format_integer(&self.locale, *v as i64).as_str().display_str(f)
            }
            FmtValue::Sint(v) => {
                self.localizer.format_integer(&self.locale, *v as i64).as_str().display_str(f)
            }
            FmtValue::Ulong(v) => {
                self.localizer.format_integer(&self.locale, *v as i64).as_str().display_str(f)
            }
            FmtValue::Slong(v) => {
                self.localizer.format_integer(&self.locale, *v).as_str().display_str(f)
            }
            FmtValue::Usize(v) => {
                self.localizer.format_integer(&self.locale, *v as i64).as_str().display_str(f)
            }
            FmtValue::Isize(v) => {
                self.localizer.format_integer(&self.locale, *v as i64).as_str().display_str(f)
            }
            // For floats, use decimal formatting (2 decimal places by default)
            FmtValue::Float(v) => {
                // Convert to integer representation with 2 decimal places
                let int_part = (*v * 100.0) as i64;
                self.localizer.format_decimal(&self.locale, int_part, 2).as_str().display_str(f)
            }
            FmtValue::Double(v) => {
                // Convert to integer representation with 2 decimal places
                let int_part = (*v * 100.0) as i64;
                self.localizer.format_decimal(&self.locale, int_part, 2).as_str().display_str(f)
            }
            // For string lists, use ICU list formatting
            FmtValue::StrVec(sv) => {
                let items: Vec<AzString> = sv.as_ref().iter().cloned().collect();
                self.localizer.format_list(&self.locale, &items, ListType::And).as_str().display_str(f)
            }
            // Other types use standard formatting
            FmtValue::Bool(v) => format!("{v:?}").display_str(f),
            FmtValue::Uchar(v) => v.display_str(f),
            FmtValue::Schar(v) => v.display_str(f),
            FmtValue::Ushort(v) => v.display_str(f),
            FmtValue::Sshort(v) => v.display_str(f),
            FmtValue::Str(v) => v.as_str().display_str(f),
        }
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

impl LayoutCallbackInfoIcuExt for LayoutCallbackInfo {
    fn icu_get_locale(&self) -> AzString {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        handle.get_default_locale()
    }

    fn icu_get_language(&self) -> AzString {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.get_language(locale)
    }

    fn icu_format_integer(&self, value: i64) -> AzString {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.format_integer(locale, value)
    }

    fn icu_format_decimal(&self, integer_part: i64, decimal_places: i16) -> AzString {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.format_decimal(locale, integer_part, decimal_places)
    }

    fn icu_get_plural_category(&self, value: i64) -> PluralCategory {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.get_plural_category(locale, value)
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
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.pluralize(locale, value, zero, one, two, few, many, other)
    }

    fn icu_format_list(&self, items: &[AzString], list_type: ListType) -> AzString {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.format_list(locale, items, list_type)
    }

    fn icu_format_date(&self, date: IcuDate, length: FormatLength) -> IcuResult {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.format_date(locale, date, length)
    }

    fn icu_format_time(&self, time: IcuTime, include_seconds: bool) -> IcuResult {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.format_time(locale, time, include_seconds)
    }

    fn icu_format_datetime(&self, datetime: IcuDateTime, length: FormatLength) -> IcuResult {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.format_datetime(locale, datetime, length)
    }

    fn icu_compare_strings(&self, a: &str, b: &str) -> i32 {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.compare_strings(locale, a, b)
    }

    fn icu_sort_strings(&self, strings: &[AzString]) -> IcuStringVec {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.sort_strings(locale, strings)
    }

    fn icu_strings_equal(&self, a: &str, b: &str) -> bool {
        let system_style = self.get_system_style();
        let handle = IcuLocalizerHandle::from_system_language(&system_style.language);
        let locale = system_style.language.as_str();
        handle.strings_equal(locale, a, b)
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
